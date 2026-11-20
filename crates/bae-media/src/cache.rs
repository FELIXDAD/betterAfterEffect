//! Decoded-frame cache: an LRU of [`GpuFrame`]s bounded by GPU memory.
//!
//! Keys are (source id, frame index, request variant). The source id is chosen
//! by the owner of the source (the compositor's footage item, the timeline's
//! clip …); [`source_id_for`] derives one from the file path and modification
//! time so that a re-rendered file is not served from stale entries.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::Path;

use parking_lot::Mutex;

use crate::frame::{FrameRequest, GpuFrame};
use crate::gpu::MediaGpu;
use crate::source::{MediaError, MediaSource};

/// Cache key of one decoded frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameKey {
    /// Identifies the source (see [`source_id_for`]).
    pub source: u64,
    /// Frame index.
    pub index: i64,
    /// Hash of the request parameters other than the index (alpha handling,
    /// downscale), so different deliveries of one frame do not collide.
    pub variant: u64,
}

impl FrameKey {
    /// The key of `request` on `source`.
    pub fn new(source: u64, request: &FrameRequest) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        FrameRequest {
            index: 0,
            ..*request
        }
        .hash(&mut hasher);
        FrameKey {
            source,
            index: request.index,
            variant: hasher.finish(),
        }
    }
}

/// A source id from the path, size and modification time of a file.
pub fn source_id_for(path: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut hasher);
        if let Ok(modified) = meta.modified() {
            modified.hash(&mut hasher);
        }
    }
    hasher.finish()
}

struct Entry {
    frame: GpuFrame,
    tick: u64,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<FrameKey, Entry>,
    /// Access tick → key, oldest first.
    order: BTreeMap<u64, FrameKey>,
    tick: u64,
    bytes: u64,
    budget: u64,
    hits: u64,
    misses: u64,
}

impl Inner {
    fn touch(&mut self, key: &FrameKey) -> Option<GpuFrame> {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entries.get_mut(key)?;
        self.order.remove(&entry.tick);
        entry.tick = tick;
        self.order.insert(tick, *key);
        Some(entry.frame.clone())
    }

    fn remove(&mut self, key: &FrameKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.order.remove(&entry.tick);
            self.bytes -= entry.frame.byte_size();
        }
    }

    fn evict_to(&mut self, budget: u64) {
        while self.bytes > budget {
            let Some((_, key)) = self.order.pop_first() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes -= entry.frame.byte_size();
            }
        }
    }
}

/// Cache statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Frames held.
    pub frames: usize,
    /// GPU bytes held.
    pub bytes: u64,
    /// Byte budget.
    pub budget: u64,
    /// Lookups that found a frame.
    pub hits: u64,
    /// Lookups that did not.
    pub misses: u64,
}

/// A thread-safe LRU of decoded GPU frames with a byte budget.
pub struct FrameCache {
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for FrameCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCache")
            .field("stats", &self.stats())
            .finish()
    }
}

impl FrameCache {
    /// A cache holding at most `budget_bytes` of textures (an `Rgba16Float` 4K
    /// frame is ~66 MB, 1080p ~16.6 MB).
    pub fn new(budget_bytes: u64) -> Self {
        FrameCache {
            inner: Mutex::new(Inner {
                budget: budget_bytes,
                ..Inner::default()
            }),
        }
    }

    /// The frame for `key`, marking it most recently used.
    pub fn get(&self, key: &FrameKey) -> Option<GpuFrame> {
        let mut inner = self.inner.lock();
        let frame = inner.touch(key);
        if frame.is_some() {
            inner.hits += 1;
        } else {
            inner.misses += 1;
        }
        frame
    }

    /// `true` if `key` is cached (does not count as an access).
    pub fn contains(&self, key: &FrameKey) -> bool {
        self.inner.lock().entries.contains_key(key)
    }

    /// Inserts (or replaces) a frame, evicting least recently used frames over
    /// budget. A frame larger than the whole budget is not kept.
    pub fn insert(&self, key: FrameKey, frame: GpuFrame) {
        let mut inner = self.inner.lock();
        inner.remove(&key);
        let size = frame.byte_size();
        if size > inner.budget {
            return;
        }
        let budget = inner.budget;
        inner.evict_to(budget - size);
        inner.tick += 1;
        let tick = inner.tick;
        inner.order.insert(tick, key);
        inner.entries.insert(key, Entry { frame, tick });
        inner.bytes += size;
    }

    /// Removes every frame of `source` (file changed, footage replaced).
    pub fn remove_source(&self, source: u64) {
        let mut inner = self.inner.lock();
        let keys: Vec<_> = inner
            .entries
            .keys()
            .filter(|key| key.source == source)
            .copied()
            .collect();
        for key in keys {
            inner.remove(&key);
        }
    }

    /// Drops everything.
    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.entries.clear();
        inner.order.clear();
        inner.bytes = 0;
    }

    /// Changes the budget, evicting as needed.
    pub fn set_budget(&self, budget_bytes: u64) {
        let mut inner = self.inner.lock();
        inner.budget = budget_bytes;
        inner.evict_to(budget_bytes);
    }

    /// Current statistics.
    pub fn stats(&self) -> CacheStats {
        let inner = self.inner.lock();
        CacheStats {
            frames: inner.entries.len(),
            bytes: inner.bytes,
            budget: inner.budget,
            hits: inner.hits,
            misses: inner.misses,
        }
    }

    /// Returns the cached frame for `request`, or decodes it with `source` on
    /// `gpu` and caches it.
    pub fn decode(
        &self,
        source_id: u64,
        source: &mut dyn MediaSource,
        request: &FrameRequest,
        gpu: &MediaGpu,
    ) -> Result<GpuFrame, MediaError> {
        let key = FrameKey::new(source_id, request);
        if let Some(frame) = self.get(&key) {
            return Ok(frame);
        }
        let frame = source.decode_gpu(request, gpu)?;
        self.insert(key, frame.clone());
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_separate_variants() {
        let a = FrameKey::new(1, &FrameRequest::new(5));
        let b = FrameKey::new(1, &FrameRequest::new(5).straight());
        let c = FrameKey::new(1, &FrameRequest::new(6));
        assert_ne!(a, b);
        assert_eq!(a.variant, c.variant);
        assert_ne!(a, c);
    }

    #[test]
    fn source_ids_track_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        std::fs::write(&path, b"one").unwrap();
        let first = source_id_for(&path);
        assert_eq!(first, source_id_for(&path));
        std::fs::write(&path, b"three").unwrap();
        assert_ne!(first, source_id_for(&path));
    }
}
