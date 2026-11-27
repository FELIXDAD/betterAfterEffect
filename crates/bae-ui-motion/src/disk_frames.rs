//! Persistent RGBA preview frames. Like AE's disk cache, only frames that take
//! clearly longer to render than to read back are kept, and only those are read
//! back. Reads run on their own low-priority thread, writes on the cache worker;
//! only a single GPU readback can be pending.
use bae_gpu::{GpuContext, WarmUp, transfer, wgpu};
use bae_prefs::{Preferences, PrefsStore};
use bae_render::{IRect, OutputTarget};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, SystemTime};

/// A frame is written only when rendering it took this many times longer than
/// reading it back is expected to take (and longer than the frame lasts).
const SAVE_MARGIN: f64 = 2.0;
/// A stored frame is read instead of rendered when its render time exceeds the
/// expected read time by this factor.
const READ_MARGIN: f64 = 1.25;
const MAGIC: &[u8; 4] = b"BPF2";
const HEADER_LEN: usize = 33;

#[derive(Clone, PartialEq, Eq)]
struct Config {
    dir: PathBuf,
    /// The set Maximum Disk Cache Size, else automatic (a share of the free space).
    limit: Option<u64>,
    enabled: bool,
    preview: bool,
    compressed: bool,
}

impl Config {
    fn new(base: &Path, p: &Preferences) -> Self {
        Self {
            dir: p.preview_cache_dir(base),
            limit: p.explicit_disk_limit_gb().map(|gib| gib << 30),
            enabled: p.bool("disk.enable_disk_cache"),
            preview: p.bool("disk.enable_preview_from_disk_cache"),
            compressed: p.bool("disk.enable_compressed_frames_lossless"),
        }
    }
}

/// What a stored frame's header says.
#[derive(Clone, Copy)]
struct Header {
    w: u32,
    h: u32,
    rect: IRect,
    compressed: bool,
    /// How long the frame took to render (µs).
    cost_us: u32,
}

impl Header {
    fn pixel_bytes(&self) -> u64 {
        u64::from(self.w) * u64::from(self.h) * 4
    }
}

#[derive(Clone, Copy)]
struct Entry {
    date: SystemTime,
    bytes: u64,
    pixel_bytes: u64,
    compressed: bool,
    cost_us: u32,
}

#[derive(Default)]
struct Index {
    limit: u64,
    files: HashMap<u64, Entry>,
    order: BTreeSet<(SystemTime, u64)>,
    bytes: u64,
}
impl Index {
    fn add(&mut self, key: u64, entry: Entry) {
        self.remove(key);
        self.bytes += entry.bytes;
        self.order.insert((entry.date, key));
        self.files.insert(key, entry);
    }
    fn remove(&mut self, key: u64) -> Option<Entry> {
        let entry = self.files.remove(&key)?;
        self.order.remove(&(entry.date, key));
        self.bytes -= entry.bytes;
        Some(entry)
    }
    /// A read: the frame is trimmed last (least recently used first).
    fn touch(&mut self, key: u64) {
        if let Some(mut entry) = self.remove(key) {
            entry.date = SystemTime::now();
            self.add(key, entry);
        }
    }
    fn scan(dir: &Path) -> io::Result<Self> {
        let mut index = Self::default();
        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("bpf") {
                continue;
            }
            let Some(key) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| s.len() == 16)
                .and_then(|s| u64::from_str_radix(s, 16).ok())
            else {
                continue;
            };
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            match File::open(&path).and_then(|mut f| read_header(&mut f)) {
                Ok(h) => index.add(
                    key,
                    Entry {
                        date: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        bytes: meta.len(),
                        pixel_bytes: h.pixel_bytes(),
                        compressed: h.compressed,
                        cost_us: h.cost_us,
                    },
                ),
                // An older format or a damaged frame: rendered again when needed.
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    let _ = fs::remove_file(&path);
                }
                Err(_) => {}
            }
        }
        Ok(index)
    }
    fn trim(&mut self, dir: &Path, limit: u64) {
        while self.bytes > limit {
            let Some(&(_, key)) = self.order.first() else {
                break;
            };
            self.remove(key);
            let _ = fs::remove_file(file_path(dir, key));
        }
    }
}
fn file_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.bpf"))
}

/// Measured seconds per pixel byte to read, inflate and upload a stored frame
/// (uncompressed, compressed); the first reads replace the conservative prior.
struct ReadSpeed {
    per_byte: [f64; 2],
    samples: u32,
}
impl Default for ReadSpeed {
    fn default() -> Self {
        Self {
            per_byte: [1.0e-9, 2.5e-9],
            samples: 0,
        }
    }
}
impl ReadSpeed {
    fn estimate(&self, pixel_bytes: u64, compressed: bool) -> f64 {
        self.per_byte[usize::from(compressed)] * pixel_bytes as f64
    }
    fn measured(&mut self, pixel_bytes: u64, compressed: bool, seconds: f64) {
        let rate = &mut self.per_byte[usize::from(compressed)];
        let weight = if self.samples == 0 { 1.0 } else { 0.25 };
        *rate += weight * (seconds / pixel_bytes.max(1) as f64 - *rate);
        self.samples = self.samples.saturating_add(1);
    }
}

struct State {
    cfg: Config,
    epoch: u64,
    ready: bool,
    index: Option<Index>,
    speed: ReadSpeed,
}

enum Task {
    Refresh,
    Clear(PathBuf),
    Save {
        epoch: u64,
        key: u64,
        texture: wgpu::Texture,
        rect: IRect,
        cost_us: u32,
        hold: Arc<dyn Send + Sync>,
    },
}

/// Makes the texture a stored frame of this size is uploaded into.
pub(crate) type TargetFn = Box<dyn FnOnce([u32; 2]) -> OutputTarget + Send>;
/// Gets the uploaded frame, or `None` when it has to be rendered after all.
pub(crate) type ReadDone = Box<dyn FnOnce(Option<(OutputTarget, IRect)>) + Send>;

struct ReadTask {
    key: u64,
    target: TargetFn,
    done: ReadDone,
}

pub struct DiskFrames {
    base: PathBuf,
    state: Arc<Mutex<State>>,
    binary_stamp: OnceLock<u64>,
    busy: Arc<AtomicBool>,
    tx: mpsc::Sender<Task>,
    rx: Mutex<Option<mpsc::Receiver<Task>>>,
    reads: mpsc::Sender<ReadTask>,
    reads_rx: Mutex<Option<mpsc::Receiver<ReadTask>>>,
}
impl DiskFrames {
    /// The configured cache is indexed by the app's background warm-up.
    pub fn new(base: PathBuf, prefs: &PrefsStore) -> Arc<Self> {
        let (tx, rx) = mpsc::channel();
        let (reads, reads_rx) = mpsc::channel();
        let cache = Arc::new(Self {
            base: base.clone(),
            state: Arc::new(Mutex::new(State {
                cfg: Config::new(&base, &prefs.current()),
                epoch: 0,
                ready: false,
                index: None,
                speed: ReadSpeed::default(),
            })),
            binary_stamp: OnceLock::new(),
            busy: Arc::default(),
            tx,
            rx: Mutex::new(Some(rx)),
            reads,
            reads_rx: Mutex::new(Some(reads_rx)),
        });
        let weak = Arc::downgrade(&cache);
        prefs.subscribe(move |p| {
            if let Some(c) = weak.upgrade() {
                c.configure(Config::new(&base, p));
            }
        });
        cache
    }
    fn configure(&self, cfg: Config) {
        let mut state = self.state.lock().unwrap();
        let reindex = state.cfg.dir != cfg.dir
            || state.cfg.limit != cfg.limit
            || state.cfg.enabled != cfg.enabled;
        state.cfg = cfg;
        if !reindex {
            return;
        }
        state.epoch += 1;
        state.ready = false;
        drop(state);
        let _ = self.tx.send(Task::Refresh);
    }
    /// Clear only this application's frames in the selected cache directory.
    pub fn clear_folder(&self, prefs: &Preferences) {
        let dir = Config::new(&self.base, prefs).dir;
        let mut state = self.state.lock().unwrap();
        state.epoch += 1;
        state.ready = false;
        let _ = self.tx.send(Task::Clear(dir));
    }
    pub(crate) fn binary_stamp(&self) -> u64 {
        self.binary_stamp.get().copied().unwrap_or_default()
    }
    /// Frames are stored and read back (Preview from Disk Cache is on).
    pub(crate) fn enabled(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.ready && state.cfg.preview && state.index.as_ref().is_some_and(|i| i.limit > 0)
    }
    /// Whether `key` is stored and reading it back should beat rendering it.
    pub(crate) fn worth_reading(&self, key: u64) -> bool {
        let s = self.state.lock().unwrap();
        s.ready
            && s.cfg.preview
            && s.index
                .as_ref()
                .and_then(|i| i.files.get(&key))
                .is_some_and(|e| {
                    f64::from(e.cost_us) * 1e-6
                        > READ_MARGIN * s.speed.estimate(e.pixel_bytes, e.compressed)
                })
    }
    /// Reads `key` on the reader thread into `target(size)` and hands it to `done`.
    pub(crate) fn read_async(&self, key: u64, target: TargetFn, done: ReadDone) {
        if let Err(mpsc::SendError(task)) = self.reads.send(ReadTask { key, target, done }) {
            (task.done)(None);
        }
    }
    /// Stores a frame that took `cost` to render when that is clearly longer than
    /// reading it back and than the frame lasts (`frame`). Returns whether it is queued.
    pub(crate) fn save(
        &self,
        key: u64,
        texture: wgpu::Texture,
        rect: IRect,
        hold: Arc<dyn Send + Sync>,
        cost: Duration,
        frame: Duration,
    ) -> bool {
        let state = self.state.lock().unwrap();
        let Some(index) = &state.index else {
            return false;
        };
        let pixel_bytes = u64::from(texture.width()) * u64::from(texture.height()) * 4;
        let read = state.speed.estimate(pixel_bytes, state.cfg.compressed);
        if !state.ready
            || !state.cfg.preview
            || index.limit == 0
            || cost <= frame
            || cost.as_secs_f64() <= SAVE_MARGIN * read
            || index.files.contains_key(&key)
            || self.busy.swap(true, Ordering::AcqRel)
        {
            return false;
        }
        let task = Task::Save {
            epoch: state.epoch,
            key,
            texture,
            rect,
            cost_us: u32::try_from(cost.as_micros()).unwrap_or(u32::MAX),
            hold,
        };
        if self.tx.send(task).is_err() {
            self.busy.store(false, Ordering::Release);
            return false;
        }
        true
    }
}

impl WarmUp for DiskFrames {
    fn name(&self) -> &str {
        "disk preview cache"
    }
    fn priority(&self) -> u8 {
        85
    }
    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String> {
        let Some(rx) = self.rx.lock().unwrap().take() else {
            return Ok(());
        };
        let stamp = std::env::current_exe()
            .ok()
            .and_then(|path| fs::metadata(path).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |t| t.as_nanos() as u64);
        let _ = self.binary_stamp.set(stamp);
        if let Some(reads) = self.reads_rx.lock().unwrap().take() {
            let (state, gpu) = (self.state.clone(), gpu.clone());
            std::thread::Builder::new()
                .name("bae-disk-read".into())
                .spawn(move || {
                    bae_gpu::warm_up::lower_priority();
                    let mut buf = Vec::new();
                    for task in reads {
                        (task.done)(load(&state, &gpu, task.key, task.target, &mut buf));
                    }
                })
                .map_err(|e| e.to_string())?;
        }
        let (state, busy, gpu) = (self.state.clone(), self.busy.clone(), gpu.clone());
        std::thread::Builder::new()
            .name("bae-disk-preview".into())
            .spawn(move || {
                bae_gpu::warm_up::lower_priority();
                refresh(&state);
                for task in rx {
                    match task {
                        Task::Refresh => refresh(&state),
                        Task::Clear(dir) => {
                            if let Ok(mut index) = Index::scan(&dir) {
                                index.trim(&dir, 0);
                            }
                            refresh(&state);
                        }
                        Task::Save {
                            epoch,
                            key,
                            texture,
                            rect,
                            cost_us,
                            hold,
                        } => {
                            save_frame(&state, &gpu, epoch, key, &texture, rect, cost_us);
                            drop(hold);
                            busy.store(false, Ordering::Release);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Reads, inflates and uploads a stored frame, timing it for later decisions.
/// A frame that cannot be read leaves the index, so it is rendered instead.
fn load(
    state: &Mutex<State>,
    gpu: &GpuContext,
    key: u64,
    target: impl FnOnce([u32; 2]) -> OutputTarget,
    buf: &mut Vec<u8>,
) -> Option<(OutputTarget, IRect)> {
    let (path, epoch) = {
        let s = state.lock().unwrap();
        if !s.ready || !s.index.as_ref()?.files.contains_key(&key) {
            return None;
        }
        (file_path(&s.cfg.dir, key), s.epoch)
    };
    let start = Instant::now();
    let max = gpu.device().limits().max_texture_dimension_2d;
    let result = read_file(&path, max, buf).and_then(|header| {
        let frame = target([header.w, header.h]);
        let tex = frame.view().texture();
        transfer::upload_texture(gpu, tex, transfer::TextureRegion::full(tex), buf)
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok((frame, header))
    });
    let mut s = state.lock().unwrap();
    let current = s.epoch == epoch;
    match result {
        Ok((frame, header)) => {
            let seconds = start.elapsed().as_secs_f64();
            s.speed
                .measured(header.pixel_bytes(), header.compressed, seconds);
            if current && let Some(index) = s.index.as_mut() {
                index.touch(key);
            }
            Some((frame, header.rect))
        }
        Err(e) => {
            if current && let Some(index) = s.index.as_mut() {
                index.remove(key);
            }
            drop(s);
            if matches!(
                e.kind(),
                io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
            ) {
                let _ = fs::remove_file(&path);
            }
            None
        }
    }
}

fn refresh(state: &Mutex<State>) {
    let (epoch, cfg) = {
        let s = state.lock().unwrap();
        (s.epoch, s.cfg.clone())
    };
    if !cfg.enabled {
        return;
    }
    let mut index = match fs::create_dir_all(&cfg.dir).and_then(|()| Index::scan(&cfg.dir)) {
        Ok(index) => index,
        Err(error) => {
            tracing::warn!(path=%cfg.dir.display(), %error, "preview cache unavailable");
            return;
        }
    };
    // The automatic limit counts the space the cache already takes as free.
    let Some(limit) = cfg.limit.or_else(|| {
        bae_platform::disk::available_bytes(&cfg.dir)
            .map(|free| bae_prefs::automatic_disk_limit_gb(free + index.bytes) << 30)
    }) else {
        return;
    };
    index.limit = limit;
    index.trim(&cfg.dir, limit);
    let mut s = state.lock().unwrap();
    if s.epoch == epoch {
        s.ready = true;
        let previous = s.index.replace(index);
        drop(s);
        drop(previous);
    }
}

fn save_frame(
    state: &Mutex<State>,
    gpu: &GpuContext,
    epoch: u64,
    key: u64,
    texture: &wgpu::Texture,
    rect: IRect,
    cost_us: u32,
) {
    let (cfg, limit) = {
        let s = state.lock().unwrap();
        if s.epoch != epoch {
            return;
        }
        let Some(index) = &s.index else { return };
        (s.cfg.clone(), index.limit)
    };
    let Ok(image) = transfer::read_texture(gpu, texture) else {
        return;
    };
    let Ok(mut tmp) = tempfile::NamedTempFile::new_in(&cfg.dir) else {
        return;
    };
    let header = Header {
        w: image.width,
        h: image.height,
        rect,
        compressed: cfg.compressed,
        cost_us,
    };
    let Ok(()) = write_file(tmp.as_file_mut(), &header, &image.data) else {
        return;
    };
    let Ok(meta) = tmp.as_file().metadata() else {
        return;
    };
    let bytes = meta.len();
    if bytes > limit {
        return;
    }
    let path = file_path(&cfg.dir, key);
    let bytes = match tmp.persist_noclobber(&path) {
        Ok(_) => bytes,
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
            let Ok(meta) = fs::metadata(&path) else {
                return;
            };
            meta.len()
        }
        Err(_) => return,
    };
    let mut index = {
        let mut s = state.lock().unwrap();
        if s.epoch != epoch {
            return;
        }
        let Some(i) = s.index.take() else { return };
        i
    };
    index.add(
        key,
        Entry {
            date: SystemTime::now(),
            bytes,
            pixel_bytes: header.pixel_bytes(),
            compressed: header.compressed,
            cost_us,
        },
    );
    index.trim(&cfg.dir, limit);
    let mut s = state.lock().unwrap();
    if s.epoch == epoch {
        s.index = Some(index);
    }
}

fn write_file(file: &mut File, header: &Header, data: &[u8]) -> io::Result<()> {
    let mut buf = BufWriter::new(file);
    buf.write_all(MAGIC)?;
    let r = header.rect;
    for field in [
        header.w,
        header.h,
        r.x0 as u32,
        r.y0 as u32,
        r.x1 as u32,
        r.y1 as u32,
    ] {
        buf.write_all(&field.to_le_bytes())?;
    }
    buf.write_all(&[u8::from(header.compressed)])?;
    buf.write_all(&header.cost_us.to_le_bytes())?;
    if header.compressed {
        let mut enc = DeflateEncoder::new(&mut buf, Compression::fast());
        enc.write_all(data)?;
        enc.finish()?;
    } else {
        buf.write_all(data)?;
    }
    buf.flush()
}

fn read_header(r: &mut impl Read) -> io::Result<Header> {
    let mut header = [0_u8; HEADER_LEN];
    r.read_exact(&mut header)?;
    let field = |i: usize| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid preview frame");
    if &header[..4] != MAGIC || header[28] > 1 {
        return Err(invalid());
    }
    let rect = IRect::new(
        field(12) as i32,
        field(16) as i32,
        field(20) as i32,
        field(24) as i32,
    );
    let valid_span =
        |lo: i32, hi: i32| (0..=i64::from(i32::MAX)).contains(&(i64::from(hi) - i64::from(lo)));
    if !valid_span(rect.x0, rect.x1) || !valid_span(rect.y0, rect.y1) {
        return Err(invalid());
    }
    Ok(Header {
        w: field(4),
        h: field(8),
        rect,
        compressed: header[28] == 1,
        cost_us: field(29),
    })
}

/// The frame's header; its pixels end up in `data`.
fn read_file(path: &Path, max_dimension: u32, data: &mut Vec<u8>) -> io::Result<Header> {
    let mut buf = BufReader::new(File::open(path)?);
    let header = read_header(&mut buf)?;
    if header.w == 0 || header.h == 0 || header.w.max(header.h) > max_dimension {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid preview frame",
        ));
    }
    let n = header.pixel_bytes();
    data.clear();
    data.reserve(n as usize);
    if header.compressed {
        DeflateDecoder::new(buf).take(n + 1).read_to_end(data)?;
    } else {
        buf.take(n + 1).read_to_end(data)?;
    }
    if data.len() as u64 != n {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid preview frame",
        ));
    }
    Ok(header)
}

#[cfg(test)]
impl DiskFrames {
    /// Reads `key` on this thread.
    pub(crate) fn read(&self, key: u64, gpu: &GpuContext) -> Option<(OutputTarget, IRect)> {
        let target = |size| OutputTarget::new(gpu.device(), size);
        load(&self.state, gpu, key, target, &mut Vec::new())
    }
    /// Reads cost nothing, so every frame that renders slower than it lasts is stored.
    pub(crate) fn assume_free_reads(&self) {
        self.state.lock().unwrap().speed.per_byte = [0.0; 2];
    }
    /// How many stored frames were read back.
    pub(crate) fn reads(&self) -> u32 {
        self.state.lock().unwrap().speed.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait<T>(mut f: impl FnMut() -> Option<T>) -> T {
        let start = Instant::now();
        loop {
            if let Some(v) = f() {
                return v;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "disk cache timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    const SLOW: Duration = Duration::from_secs(10);
    const FRAME: Duration = Duration::from_millis(40);

    #[test]
    fn gpu_frames_reopen_losslessly_and_purging_preserves_other_files() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let mut prefs = Preferences::default();
        prefs.set(bae_prefs::keys::DISK_LIMIT_GB, 1);
        let store = PrefsStore::new(prefs);
        let setup = || {
            let cache = DiskFrames::new(tmp.path().to_owned(), &store);
            cache.run(&gpu).unwrap();
            wait(|| cache.enabled().then_some(()));
            cache
        };
        let rect = IRect::new(-3, 4, -1, 5);
        let rgba = [17, 42, 63, 73, 255, 29, 1, 255];
        let mut cache = setup();
        for (key, comp) in [(1, true), (2, false)] {
            let mut prefs = (*store.current()).clone();
            prefs.set("disk.enable_compressed_frames_lossless", comp);
            store.set(prefs).unwrap();
            wait(|| cache.enabled().then_some(()));
            let target = OutputTarget::new(gpu.device(), [2, 1]);
            let texture = target.view().texture().clone();
            transfer::upload_texture(
                &gpu,
                &texture,
                transfer::TextureRegion::full(&texture),
                &rgba,
            )
            .unwrap();
            gpu.queue().submit([]);
            assert!(cache.save(key, texture, rect, Arc::new(target), SLOW, FRAME));
            wait(|| cache.read(key, &gpu));
            drop(cache);
            cache = setup();
            assert!(cache.worth_reading(key));
            let (frame, r) = cache.read(key, &gpu).unwrap();
            assert_eq!(r, rect);
            assert_eq!(
                transfer::read_texture(&gpu, frame.view().texture())
                    .unwrap()
                    .data,
                rgba
            );
            let mut stored =
                File::open(file_path(&tmp.path().join("BAE Preview Frames"), key)).unwrap();
            let header = read_header(&mut stored).unwrap();
            assert_eq!((header.compressed, header.cost_us), (comp, 10_000_000));
        }
        let keep = tmp.path().join("BAE Preview Frames").join("keep.txt");
        fs::write(&keep, "keep").unwrap();
        let mut prefs = (*store.current()).clone();
        prefs.set("disk.enable_preview_from_disk_cache", false);
        store.set(prefs.clone()).unwrap();
        assert!(!cache.worth_reading(1));
        cache.clear_folder(&prefs);
        wait(|| (!file_path(&tmp.path().join("BAE Preview Frames"), 1).exists()).then_some(()));
        assert_eq!(fs::read_to_string(keep).unwrap(), "keep");
    }

    /// Frames that render in real time, or faster than they read back, are not
    /// written; a stored frame is read only while that beats rendering it.
    #[test]
    fn only_frames_slower_to_render_than_to_read_are_kept() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let mut prefs = Preferences::default();
        prefs.set(bae_prefs::keys::DISK_LIMIT_GB, 1);
        let cache = DiskFrames::new(tmp.path().to_owned(), &PrefsStore::new(prefs));
        cache.run(&gpu).unwrap();
        wait(|| cache.enabled().then_some(()));
        let frame = |size| {
            let target = OutputTarget::new(gpu.device(), size);
            (target.view().texture().clone(), Arc::new(target))
        };
        let rect = IRect::new(0, 0, 1, 1);
        // A 4K frame that renders within its frame time, then one that renders
        // slower than its frame but faster than the prior read estimate.
        let (t, hold) = frame([3840, 2160]);
        assert!(!cache.save(1, t, rect, hold, Duration::from_millis(30), FRAME));
        let (t, hold) = frame([3840, 2160]);
        assert!(!cache.save(2, t, rect, hold, Duration::from_millis(50), FRAME));
        let (t, hold) = frame([2, 2]);
        assert!(cache.save(3, t, rect, hold, SLOW, FRAME));
        wait(|| cache.read(3, &gpu));
        assert!(cache.worth_reading(3));
        cache.state.lock().unwrap().speed.per_byte = [1.0; 2];
        assert!(!cache.worth_reading(3), "reading got slower than rendering");
    }

    #[test]
    fn storage_limit_removes_least_recently_used_frames_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = Index::default();
        for n in 1..=3 {
            fs::write(file_path(dir.path(), n), [0; 17]).unwrap();
            index.add(
                n,
                Entry {
                    date: SystemTime::UNIX_EPOCH + Duration::from_secs(n),
                    bytes: 17,
                    pixel_bytes: 4,
                    compressed: false,
                    cost_us: 0,
                },
            );
        }
        index.touch(1);
        index.trim(dir.path(), 34);
        assert!(file_path(dir.path(), 1).exists());
        assert!(!file_path(dir.path(), 2).exists());
    }
}
