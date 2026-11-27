//! [`MediaLibrary`]: shared, thread-safe access to footage for the renderer
//! (04), the edit timeline (14), thumbnails and the Agent.
//!
//! It opens each file once (per [`OpenOptions`]), maps footage time to frames,
//! decodes into the shared [`FrameCache`], and gives stable content hashes for
//! render caches. All methods take `&self`; different files decode in
//! parallel, requests for the same file are serialized (a video decoder keeps
//! its read position).
//!
//! The compositor's footage `LayerSource` (in `bae-render`, which depends on
//! this crate) is a thin wrapper:
//!
//! ```ignore
//! struct Footage { library: Arc<MediaLibrary>, file: PathBuf, open: OpenOptions,
//!                  request: FrameRequest, size: [u32; 2] }
//! impl LayerSource for Footage {
//!     fn size(&self) -> [u32; 2] { self.size }
//!     fn content_hash(&self, time: Time) -> u64 {
//!         self.library.content_hash(&self.file, &self.open, &self.request, time).unwrap_or(0)
//!     }
//!     fn render(&self, ctx: &mut EncodeContext<'_>, req: &SourceRequest) -> Result<GpuImage, RenderError> {
//!         let frame = self.library.frame_at(&self.file, &self.open, &self.request, req.time, &media_gpu)?;
//!         Ok(GpuImage::from_texture(frame.texture, ImageAlpha::Premultiplied)) // zero copy
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::cache::{FrameCache, FrameKey, source_id_for};
use crate::frame::{CpuFrame, FrameRequest, GpuFrame};
use crate::gpu::MediaGpu;
use crate::meta::MediaInfo;
use crate::source::{MediaError, MediaFormats, MediaSource, OpenOptions};
use crate::time::Time;

/// Default number of files kept open.
pub const DEFAULT_OPEN_LIMIT: usize = 64;

/// Where a footage file's bytes are: on disk, or embedded in a container
/// such as a `.baez` package (ARCHITECTURE §16 stores media entries
/// uncompressed, so an entry is a byte range of the container).
///
/// Packaging code maps its asset locations to this (for `bae-package`:
/// `AssetLocation::File { path }` → [`MediaLocation::File`];
/// `AssetLocation::Packaged { entry }` with the entry's `offset` / `size` →
/// [`MediaLocation::Embedded`]) and calls [`MediaLibrary::resolve`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MediaLocation {
    /// A file on disk.
    File(PathBuf),
    /// A stored (uncompressed) entry of a container file.
    Embedded {
        /// The container (`.baez`).
        container: PathBuf,
        /// Byte offset of the entry's data.
        offset: u64,
        /// Length in bytes.
        len: u64,
        /// Entry name, `/`-separated (`assets/footage/a.mov`); its directories
        /// are kept so the frames of an image sequence end up side by side.
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SourceKey {
    path: PathBuf,
    options: OpenOptions,
}

struct OpenSource {
    /// Cache identity: path, size and modification time at open, and the
    /// open options.
    id: u64,
    info: MediaInfo,
    source: Box<dyn MediaSource>,
}

struct Slot {
    source: Arc<Mutex<OpenSource>>,
    last_used: u64,
}

#[derive(Default)]
struct Sources {
    slots: HashMap<SourceKey, Slot>,
    tick: u64,
}

/// Opened footage shared by everyone who draws frames.
pub struct MediaLibrary {
    formats: MediaFormats,
    cache: Arc<FrameCache>,
    open_limit: usize,
    sources: Mutex<Sources>,
    /// Bumped by every invalidation and mixed into new source ids, so frames
    /// and content hashes of a re-opened file never match the old ones (a
    /// sequence frame can change without its first file changing).
    generation: std::sync::atomic::AtomicU64,
    /// Where embedded entries are extracted ([`Self::resolve`]).
    extract_dir: PathBuf,
    extract_lock: Mutex<()>,
}

impl std::fmt::Debug for MediaLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaLibrary")
            .field("formats", &self.formats)
            .field("open", &self.sources.lock().slots.len())
            .field("cache", &self.cache)
            .finish()
    }
}

impl MediaLibrary {
    /// A library opening files with `formats` and caching frames in `cache`
    /// (usually the app's `Arc<FrameCache>` resource from
    /// [`MediaModule`](crate::MediaModule)).
    pub fn new(formats: MediaFormats, cache: Arc<FrameCache>) -> Self {
        MediaLibrary {
            formats,
            cache,
            open_limit: DEFAULT_OPEN_LIMIT,
            sources: Mutex::new(Sources::default()),
            generation: std::sync::atomic::AtomicU64::new(0),
            extract_dir: std::env::temp_dir().join("bae-media-extract"),
            extract_lock: Mutex::new(()),
        }
    }

    /// Extracts embedded media under `dir` (e.g. the project's `cache/`
    /// directory) instead of the system temporary directory.
    pub fn with_extract_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.extract_dir = dir.into();
        self
    }

    /// A file path for `location`: the path itself for files; for embedded
    /// entries a copy extracted on first use (system decoders such as
    /// AVFoundation need a file) and reused while the container is unchanged.
    /// For an image sequence, resolve every frame's entry, then open the
    /// first frame's path as a sequence.
    pub fn resolve(&self, location: &MediaLocation) -> Result<PathBuf, MediaError> {
        let (container, offset, len, name) = match location {
            MediaLocation::File(path) => return Ok(path.clone()),
            MediaLocation::Embedded {
                container,
                offset,
                len,
                name,
            } => (container, *offset, *len, name),
        };
        // One directory per container version, entry directories mirrored.
        let dest = safe_join(
            &self
                .extract_dir
                .join(format!("{:016x}", source_id_for(container))),
            name,
        )
        .ok_or_else(|| MediaError::Unsupported(format!("embedded entry name {name:?}")))?;
        let _guard = self.extract_lock.lock();
        if std::fs::metadata(&dest).is_ok_and(|m| m.is_file() && m.len() == len) {
            return Ok(dest);
        }
        extract_range(container, offset, len, &dest)?;
        Ok(dest)
    }

    /// Changes how many files stay open (least recently used are closed).
    pub fn with_open_limit(mut self, limit: usize) -> Self {
        self.open_limit = limit.max(1);
        self
    }

    /// The frame cache.
    pub fn cache(&self) -> &Arc<FrameCache> {
        &self.cache
    }

    fn source(
        &self,
        path: &Path,
        options: &OpenOptions,
    ) -> Result<Arc<Mutex<OpenSource>>, MediaError> {
        let key = SourceKey {
            path: path.to_owned(),
            options: *options,
        };
        {
            let mut sources = self.sources.lock();
            sources.tick += 1;
            let tick = sources.tick;
            if let Some(slot) = sources.slots.get_mut(&key) {
                slot.last_used = tick;
                return Ok(slot.source.clone());
            }
        }
        // Open outside the lock: probing a movie can take milliseconds.
        let source = self.formats.open(path, options)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source_id_for(path).hash(&mut hasher);
        options.hash(&mut hasher);
        self.generation
            .load(std::sync::atomic::Ordering::Relaxed)
            .hash(&mut hasher);
        let opened = Arc::new(Mutex::new(OpenSource {
            id: hasher.finish(),
            info: source.info().clone(),
            source,
        }));
        let mut sources = self.sources.lock();
        sources.tick += 1;
        let tick = sources.tick;
        // Another thread may have opened it meanwhile; keep the first.
        let slot = sources.slots.entry(key).or_insert(Slot {
            source: opened,
            last_used: tick,
        });
        slot.last_used = tick;
        let source = slot.source.clone();
        while sources.slots.len() > self.open_limit {
            let Some(oldest) = sources
                .slots
                .iter()
                .min_by_key(|(_, slot)| slot.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            sources.slots.remove(&oldest);
        }
        Ok(source)
    }

    /// Metadata of a file (opening it if needed).
    pub fn info(&self, path: &Path, options: &OpenOptions) -> Result<MediaInfo, MediaError> {
        Ok(self.source(path, options)?.lock().info.clone())
    }

    /// The frame index shown at footage time `time`, held at the first / last
    /// frame outside the footage (AE holds the end frames when time remapping
    /// runs past them; the layer's in / out points limit normal playback).
    pub fn frame_index(
        &self,
        path: &Path,
        options: &OpenOptions,
        time: Time,
    ) -> Result<i64, MediaError> {
        let source = self.source(path, options)?;
        let source = source.lock();
        Ok(clamp_index(&source.info, source.source.frame_at(time)))
    }

    /// A content hash for render caches: equal hashes mean identical pixels
    /// (same file version, options, delivery and frame).
    pub fn content_hash(
        &self,
        path: &Path,
        options: &OpenOptions,
        request: &FrameRequest,
        time: Time,
    ) -> Result<u64, MediaError> {
        let source = self.source(path, options)?;
        let source = source.lock();
        let index = clamp_index(&source.info, source.source.frame_at(time));
        let key = FrameKey::new(source.id, &FrameRequest { index, ..*request });
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        Ok(hasher.finish())
    }

    /// The GPU frame shown at footage time `time` (see [`Self::frame_index`]),
    /// from the cache or decoded. `request.index` is ignored.
    pub fn frame_at(
        &self,
        path: &Path,
        options: &OpenOptions,
        request: &FrameRequest,
        time: Time,
        gpu: &MediaGpu,
    ) -> Result<GpuFrame, MediaError> {
        let source = self.source(path, options)?;
        let mut source = source.lock();
        let index = clamp_index(&source.info, source.source.frame_at(time));
        let request = FrameRequest { index, ..*request };
        let key = FrameKey::new(source.id, &request);
        if let Some(frame) = self.cache.get(&key) {
            return Ok(frame);
        }
        let frame = source.source.decode_gpu(&request, gpu)?;
        self.cache.insert(key, frame.clone());
        Ok(frame)
    }

    /// Look up the already decoded channel metadata for exactly this source frame.
    pub fn cached_channels(
        &self,
        path: &Path,
        options: &OpenOptions,
        request: &FrameRequest,
        time: Time,
    ) -> Option<Arc<bae_gpu::channels::ChannelInfo>> {
        let key = SourceKey {
            path: path.to_owned(),
            options: *options,
        };
        let source = self.sources.lock().slots.get(&key)?.source.clone();
        let source = source.lock();
        let index = clamp_index(&source.info, source.source.frame_at(time));
        let frame = self.cache.get(&FrameKey::new(
            source.id,
            &FrameRequest { index, ..*request },
        ))?;
        Some(frame.channels?.info.clone())
    }

    /// Frame `request.index` decoded to the CPU (thumbnails, packaging, Agent
    /// previews without a GPU). Not cached.
    pub fn frame_cpu(
        &self,
        path: &Path,
        options: &OpenOptions,
        request: &FrameRequest,
    ) -> Result<CpuFrame, MediaError> {
        let source = self.source(path, options)?;
        let mut source = source.lock();
        source.source.decode_cpu(request)
    }

    /// Forgets a file (it changed on disk or was relinked): closes every
    /// source using it — including image sequences it is a frame of — and drops
    /// their cached frames. The next request reopens them with new content
    /// hashes.
    pub fn invalidate(&self, path: &Path) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let removed: Vec<Slot> = {
            let mut sources = self.sources.lock();
            let keys: Vec<SourceKey> = sources
                .slots
                .keys()
                .filter(|key| key.path == path || same_sequence(key, path))
                .cloned()
                .collect();
            keys.iter()
                .filter_map(|key| sources.slots.remove(key))
                .collect()
        };
        for slot in removed {
            self.cache.remove_source(slot.source.lock().id);
        }
    }

    /// Number of open files.
    pub fn open_count(&self) -> usize {
        self.sources.lock().slots.len()
    }
}

/// `base` joined with the `/`-separated `name`, refusing anything that could
/// leave `base` (`..`, absolute or drive-prefixed segments): package contents
/// are untrusted.
fn safe_join(base: &Path, name: &str) -> Option<PathBuf> {
    let mut out = base.to_owned();
    let mut any = false;
    for segment in name.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => return None,
            s if s.contains(':') => return None,
            s => {
                out.push(s);
                any = true;
            }
        }
    }
    any.then_some(out)
}

/// Copies `len` bytes at `offset` of `container` to `dest` (atomically, via a
/// temporary file in the same directory).
fn extract_range(container: &Path, offset: u64, len: u64, dest: &Path) -> Result<(), MediaError> {
    use std::io::{Read, Seek, SeekFrom};

    let dir = dest.parent().expect("joined paths have a parent");
    std::fs::create_dir_all(dir).map_err(|e| MediaError::io(dir, e))?;
    let mut input = std::fs::File::open(container).map_err(|e| MediaError::io(container, e))?;
    let total = input
        .metadata()
        .map_err(|e| MediaError::io(container, e))?
        .len();
    if offset.checked_add(len).is_none_or(|end| end > total) {
        return Err(MediaError::decode(
            container,
            "embedded entry outside the container",
        ));
    }
    input
        .seek(SeekFrom::Start(offset))
        .map_err(|e| MediaError::io(container, e))?;
    let tmp = dir.join(format!(
        ".{}.part-{}",
        dest.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        std::process::id()
    ));
    let mut output = std::fs::File::create(&tmp).map_err(|e| MediaError::io(&tmp, e))?;
    let copied =
        std::io::copy(&mut input.take(len), &mut output).map_err(|e| MediaError::io(&tmp, e))?;
    drop(output);
    if copied != len {
        let _ = std::fs::remove_file(&tmp);
        return Err(MediaError::decode(container, "embedded entry is truncated"));
    }
    std::fs::rename(&tmp, dest).map_err(|e| MediaError::io(dest, e))
}

/// Whether `path` is a frame of the sequence opened under `key`.
fn same_sequence(key: &SourceKey, path: &Path) -> bool {
    use crate::sequence::parse_numbered_name;
    if key.options.sequence == crate::source::SequenceMode::Alphabetical {
        return key.path.parent() == path.parent()
            && key
                .path
                .extension()
                .zip(path.extension())
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
    }
    if key.options.sequence != crate::source::SequenceMode::Sequence
        || key.path.parent() != path.parent()
    {
        return false;
    }
    let name = |p: &Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(parse_numbered_name)
    };
    match (name(&key.path), name(path)) {
        (Some(a), Some(b)) => a.prefix == b.prefix && a.suffix.eq_ignore_ascii_case(&b.suffix),
        _ => false,
    }
}

fn clamp_index(info: &MediaInfo, index: i64) -> i64 {
    match info.video.as_ref().and_then(|v| v.frame_count) {
        Some(count) if count > 0 => index.clamp(0, count as i64 - 1),
        _ => index.max(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_refuses_escapes() {
        let base = Path::new("/cache/pkg");
        assert_eq!(
            safe_join(base, "assets/footage/a.mov"),
            Some(PathBuf::from("/cache/pkg/assets/footage/a.mov"))
        );
        assert_eq!(
            safe_join(base, "./assets//b.png"),
            Some(PathBuf::from("/cache/pkg/assets/b.png"))
        );
        assert_eq!(safe_join(base, "assets/../../etc/passwd"), None);
        assert_eq!(safe_join(base, "C:\\x.png"), None);
        assert_eq!(safe_join(base, ""), None);
    }

    #[test]
    fn source_keys_include_options() {
        let a = SourceKey {
            path: "a.png".into(),
            options: OpenOptions::default(),
        };
        let b = SourceKey {
            path: "a.png".into(),
            options: OpenOptions::sequence(crate::FrameRate::FPS_24),
        };
        assert_ne!(a, b);
    }
}
