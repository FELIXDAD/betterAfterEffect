//! Persistent RGBA preview frames. Filesystem work runs on the playback producer
//! or the low-priority cache worker. Only a single GPU readback can be pending.
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
use std::time::SystemTime;

#[derive(Clone, PartialEq, Eq)]
struct Config {
    dir: PathBuf,
    limit: u64,
    enabled: bool,
    preview: bool,
    compressed: bool,
}

impl Config {
    fn new(base: &Path, p: &Preferences) -> Self {
        Self {
            dir: p.preview_cache_dir(base),
            limit: (p.int("disk.max_disk_cache_size_gb") as u64) << 30,
            enabled: p.bool("disk.enable_disk_cache"),
            preview: p.bool("disk.enable_preview_from_disk_cache"),
            compressed: p.bool("disk.enable_compressed_frames_lossless"),
        }
    }
}

#[derive(Default)]
struct Index {
    files: HashMap<u64, (SystemTime, u64)>,
    order: BTreeSet<(SystemTime, u64)>,
    bytes: u64,
}
impl Index {
    fn add(&mut self, key: u64, date: SystemTime, bytes: u64) {
        if let Some((old, size)) = self.files.insert(key, (date, bytes)) {
            self.order.remove(&(old, key));
            self.bytes -= size;
        }
        self.bytes += bytes;
        self.order.insert((date, key));
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
            if entry.file_type().is_ok_and(|t| t.is_file())
                && let Ok(meta) = entry.metadata()
            {
                index.add(
                    key,
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    meta.len(),
                );
            }
        }
        Ok(index)
    }
    fn trim(&mut self, dir: &Path, limit: u64) {
        while self.bytes > limit {
            let Some((_, key)) = self.order.pop_first() else {
                break;
            };
            if let Some((_, size)) = self.files.remove(&key) {
                self.bytes -= size;
                let _ = fs::remove_file(file_path(dir, key));
            }
        }
    }
}
fn file_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.bpf"))
}

struct State {
    cfg: Config,
    epoch: u64,
    ready: bool,
    index: Option<Index>,
}

enum Task {
    Refresh,
    Clear(PathBuf),
    Save {
        epoch: u64,
        key: u64,
        texture: wgpu::Texture,
        rect: IRect,
        hold: Arc<dyn Send + Sync>,
    },
}

pub struct DiskFrames {
    #[cfg(test)]
    pub(crate) restored: std::sync::atomic::AtomicUsize,
    base: PathBuf,
    state: Arc<Mutex<State>>,
    binary_stamp: OnceLock<u64>,
    busy: Arc<AtomicBool>,
    tx: mpsc::Sender<Task>,
    rx: Mutex<Option<mpsc::Receiver<Task>>>,
}
impl DiskFrames {
    /// The configured cache is indexed by the app's background warm-up.
    pub fn new(base: PathBuf, prefs: &PrefsStore) -> Arc<Self> {
        let (tx, rx) = mpsc::channel();
        let cache = Arc::new(Self {
            #[cfg(test)]
            restored: Default::default(),
            base: base.clone(),
            state: Arc::new(Mutex::new(State {
                cfg: Config::new(&base, &prefs.current()),
                epoch: 0,
                ready: false,
                index: None,
            })),
            binary_stamp: OnceLock::new(),
            busy: Arc::default(),
            tx,
            rx: Mutex::new(Some(rx)),
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
    pub(crate) fn enabled(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.ready && state.cfg.limit > 0 && state.index.is_some()
    }
    pub(crate) fn read(&self, key: u64, gpu: &GpuContext) -> Option<(OutputTarget, IRect)> {
        let (path, epoch) = {
            let state = self.state.lock().unwrap();
            if !state.ready || !state.cfg.preview || !state.index.as_ref()?.files.contains_key(&key)
            {
                return None;
            }
            (file_path(&state.cfg.dir, key), state.epoch)
        };
        let (w, h, rect, data) =
            match read_file(&path, gpu.device().limits().max_texture_dimension_2d) {
                Ok(frame) => frame,
                Err(e) => {
                    // A partial or old frame can be rendered and saved again.
                    if matches!(
                        e.kind(),
                        io::ErrorKind::InvalidData
                            | io::ErrorKind::UnexpectedEof
                            | io::ErrorKind::NotFound
                    ) {
                        let _ = fs::remove_file(&path);
                        let mut s = self.state.lock().unwrap();
                        if s.epoch == epoch
                            && let Some(index) = s.index.as_mut()
                            && let Some((date, size)) = index.files.remove(&key)
                        {
                            index.order.remove(&(date, key));
                            index.bytes -= size;
                        }
                    }
                    return None;
                }
            };
        let target = OutputTarget::new(gpu.device(), [w, h]);
        let tex = target.view().texture();
        transfer::upload_texture(gpu, tex, transfer::TextureRegion::full(tex), &data).ok()?;
        #[cfg(test)]
        self.restored.fetch_add(1, Ordering::Release);
        Some((target, rect))
    }
    pub(crate) fn save(
        &self,
        key: u64,
        texture: wgpu::Texture,
        rect: IRect,
        hold: Arc<dyn Send + Sync>,
    ) {
        let state = self.state.lock().unwrap();
        let Some(index) = &state.index else { return };
        if !state.ready
            || state.cfg.limit == 0
            || index.files.contains_key(&key)
            || self.busy.swap(true, Ordering::AcqRel)
        {
            return;
        }
        if self
            .tx
            .send(Task::Save {
                epoch: state.epoch,
                key,
                texture,
                rect,
                hold,
            })
            .is_err()
        {
            self.busy.store(false, Ordering::Release);
        }
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
        let (state, busy, gpu) = (self.state.clone(), self.busy.clone(), gpu.clone());
        std::thread::Builder::new()
            .name("bae-disk-preview".into())
            .spawn(move || {
                bae_gpu::warm_up::lower_priority();
                let mut warm = DeflateEncoder::new(Vec::new(), Compression::fast());
                if warm.write_all(&[0; 4]).is_ok()
                    && let Ok(bytes) = warm.finish()
                {
                    let _ = DeflateDecoder::new(bytes.as_slice()).read_exact(&mut [0; 4]);
                }
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
                            hold,
                        } => {
                            save_frame(&state, &gpu, epoch, key, &texture, rect);
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
    index.trim(&cfg.dir, cfg.limit);
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
) {
    let cfg = {
        let s = state.lock().unwrap();
        if s.epoch != epoch {
            return;
        }
        s.cfg.clone()
    };
    let Ok(image) = transfer::read_texture(gpu, texture) else {
        return;
    };
    let Ok(mut tmp) = tempfile::NamedTempFile::new_in(&cfg.dir) else {
        return;
    };
    let Ok(()) = write_file(
        tmp.as_file_mut(),
        image.width,
        image.height,
        rect,
        &image.data,
        cfg.compressed,
    ) else {
        return;
    };
    let Ok(meta) = tmp.as_file().metadata() else {
        return;
    };
    let bytes = meta.len();
    if bytes > cfg.limit {
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
    index.add(key, SystemTime::now(), bytes);
    index.trim(&cfg.dir, cfg.limit);
    let mut s = state.lock().unwrap();
    if s.epoch == epoch {
        s.index = Some(index);
    }
}

fn write_file(
    file: &mut File,
    w: u32,
    h: u32,
    rect: IRect,
    data: &[u8],
    compressed: bool,
) -> io::Result<()> {
    let mut buf = BufWriter::new(file);
    buf.write_all(b"BPF1")?;
    for field in [
        w,
        h,
        rect.x0 as u32,
        rect.y0 as u32,
        rect.x1 as u32,
        rect.y1 as u32,
    ] {
        buf.write_all(&field.to_le_bytes())?;
    }
    buf.write_all(&[u8::from(compressed)])?;
    if compressed {
        let mut enc = DeflateEncoder::new(&mut buf, Compression::fast());
        enc.write_all(data)?;
        enc.finish()?;
    } else {
        buf.write_all(data)?;
    }
    buf.flush()
}

fn read_file(path: &Path, max_dimension: u32) -> io::Result<(u32, u32, IRect, Vec<u8>)> {
    let mut buf = BufReader::new(File::open(path)?);
    let mut header = [0_u8; 29];
    buf.read_exact(&mut header)?;
    let field = |i: usize| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
    let (w, h) = (field(4), field(8));
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid preview frame");
    if &header[..4] != b"BPF1" || w == 0 || h == 0 || w.max(h) > max_dimension || header[28] > 1 {
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
    let n = u64::from(w) * u64::from(h) * 4;
    let mut data = Vec::new();
    if header[28] == 1 {
        DeflateDecoder::new(buf)
            .take(n + 1)
            .read_to_end(&mut data)?;
    } else {
        buf.take(n + 1).read_to_end(&mut data)?;
    }
    if data.len() as u64 != n {
        return Err(invalid());
    }
    Ok((w, h, rect, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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

    #[test]
    fn gpu_frames_reopen_losslessly_and_purging_preserves_other_files() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let store = PrefsStore::default();
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
            cache.save(key, texture, rect, Arc::new(target));
            wait(|| cache.read(key, &gpu));
            drop(cache);
            cache = setup();
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
            let mut header = [0; 29];
            stored.read_exact(&mut header).unwrap();
            assert_eq!(header[28], u8::from(comp));
        }
        let keep = tmp.path().join("BAE Preview Frames").join("keep.txt");
        fs::write(&keep, "keep").unwrap();
        let mut prefs = (*store.current()).clone();
        prefs.set("disk.enable_preview_from_disk_cache", false);
        store.set(prefs.clone()).unwrap();
        wait(|| cache.enabled().then_some(()));
        assert!(cache.read(1, &gpu).is_none());
        cache.clear_folder(&prefs);
        wait(|| (!file_path(&tmp.path().join("BAE Preview Frames"), 1).exists()).then_some(()));
        assert_eq!(fs::read_to_string(keep).unwrap(), "keep");
    }

    #[test]
    fn storage_limit_removes_oldest_frames_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = Index::default();
        for n in 1..=3 {
            fs::write(file_path(dir.path(), n), [0; 17]).unwrap();
            index.add(n, SystemTime::UNIX_EPOCH + Duration::from_secs(n), 17);
        }
        index.trim(dir.path(), 34);
        assert!(!file_path(dir.path(), 1).exists());
        assert_eq!(Index::scan(dir.path()).unwrap().bytes, 34);
    }
}
