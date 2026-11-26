//! Frame cache (AE's RAM preview cache) with a memory budget and precise
//! invalidation from [`StageEvent`]s.
//!
//! Entries are keyed by `(comp, time, request fingerprint)` and remember the
//! [`Dependency`] list of the frame (every prim it was built from and the time
//! it was sampled at, in that prim's own time base). A change invalidates an
//! entry only if it touches one of those prims **and** its time range contains
//! the sampled time:
//!
//! * `attr_changed { path, range }` is matched against the *most specific*
//!   dependency covering `path` (a layer attribute uses the layer's time, a comp
//!   attribute the comp time), so moving a keyframe at 2 s only drops frames
//!   between its neighbouring keyframes.
//! * Structural changes (create / remove / move / type / references / resync)
//!   drop every frame that depends on anything at, above or below the path.
//!
//! The cache is generic over the stored value so it can be tested without a GPU;
//! the renderer stores frame textures in it.

use bae_model::{Change, PrimPath, StageEvent, Time};
use rustc_hash::FxHashMap;

use crate::GpuImage;
use crate::input::Dependency;
use std::hash::BuildHasher;

/// Frame-local writes and submitted intermediates share the renderer's LRU budget.
pub(crate) struct IntermediateImages<'a> {
    pub submitted: &'a mut FrameCache<GpuImage>,
    pub pending: &'a mut FrameCache<GpuImage>,
    pub request: u64,
}

impl IntermediateImages<'_> {
    fn key(&self, mut key: FrameKey) -> FrameKey {
        key.request =
            rustc_hash::FxBuildHasher.hash_one(("effect intermediate", self.request, key.request));
        key
    }

    pub(crate) fn get(&mut self, key: FrameKey) -> Option<GpuImage> {
        let key = self.key(key);
        self.pending
            .get(&key)
            .or_else(|| self.submitted.get(&key))
            .cloned()
    }

    pub(crate) fn store(&mut self, key: FrameKey, image: GpuImage) {
        let bytes = image.byte_size();
        self.pending
            .insert(self.key(key), image, bytes, Vec::new(), 0);
    }
}

impl FrameCache<GpuImage> {
    pub(crate) fn publish(self, submitted: &mut Self, comp: &crate::RenderComp) {
        for (key, entry) in self.entries {
            submitted.insert(key, entry.value, entry.bytes, comp.dependencies(), 0);
        }
    }
}

/// Key of a cached frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrameKey {
    /// The comp.
    pub comp: PrimPath,
    /// Comp time.
    pub time: Time,
    /// [`crate::RenderRequest::fingerprint`].
    pub request: u64,
}

#[derive(Debug)]
struct Entry<T> {
    value: T,
    history: bool,
    bytes: u64,
    deps: Vec<Dependency>,
    content_hash: u64,
    last_used: u64,
}

/// Statistics of a [`FrameCache`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Entries stored.
    pub entries: usize,
    /// Bytes stored.
    pub bytes: u64,
    /// Budget in bytes.
    pub budget: u64,
    /// Lookups that found a valid frame.
    pub hits: u64,
    /// Lookups that did not.
    pub misses: u64,
    /// Entries dropped by invalidation.
    pub invalidated: u64,
    /// Entries dropped to stay within the budget.
    pub evicted: u64,
}

/// An LRU frame cache bounded by bytes.
#[derive(Debug)]
pub struct FrameCache<T> {
    entries: FxHashMap<FrameKey, Entry<T>>,
    clock: u64,
    stats: CacheStats,
}

impl<T> FrameCache<T> {
    /// An empty cache holding at most `budget` bytes.
    pub fn new(budget: u64) -> Self {
        Self {
            entries: FxHashMap::default(),
            clock: 0,
            stats: CacheStats {
                budget,
                ..CacheStats::default()
            },
        }
    }

    /// Current statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            ..self.stats
        }
    }

    /// Changes the budget, evicting if needed.
    pub fn set_budget(&mut self, budget: u64) {
        self.stats.budget = budget;
        self.evict_to(budget);
    }

    /// The cached frame for `key`, marking it recently used.
    pub fn get(&mut self, key: &FrameKey) -> Option<&T> {
        self.clock += 1;
        match self.entries.get_mut(key) {
            Some(entry) => {
                entry.last_used = self.clock;
                self.stats.hits += 1;
                Some(&entry.value)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// True when a valid frame is cached (does not count as a lookup).
    pub fn contains(&self, key: &FrameKey) -> bool {
        self.entries.contains_key(key)
    }

    /// The content hash the frame was stored with.
    pub fn content_hash(&self, key: &FrameKey) -> Option<u64> {
        self.entries.get(key).map(|e| e.content_hash)
    }

    /// Stores a frame, evicting least recently used frames beyond the budget.
    /// A frame larger than the whole budget is not stored.
    pub fn insert(
        &mut self,
        key: FrameKey,
        value: T,
        bytes: u64,
        deps: Vec<Dependency>,
        content_hash: u64,
    ) {
        self.insert_surface(key, value, bytes, deps, content_hash, false);
    }

    /// Feedback surfaces carry dependencies across earlier frames.
    pub(crate) fn insert_surface(
        &mut self,
        key: FrameKey,
        value: T,
        bytes: u64,
        deps: Vec<Dependency>,
        content_hash: u64,
        history: bool,
    ) {
        if bytes > self.stats.budget {
            return;
        }
        self.clock += 1;
        if let Some(old) = self.entries.remove(&key) {
            self.stats.bytes -= old.bytes;
        }
        self.evict_to(self.stats.budget - bytes);
        self.stats.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                history,
                value,
                bytes,
                deps,
                content_hash,
                last_used: self.clock,
            },
        );
    }

    fn evict_to(&mut self, limit: u64) {
        while self.stats.bytes > limit {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(e) = self.entries.remove(&oldest) {
                self.stats.bytes -= e.bytes;
                self.stats.evicted += 1;
            }
        }
    }

    /// Drops every frame `event` affects; returns how many were dropped.
    pub fn invalidate(&mut self, event: &StageEvent) -> usize {
        if event.changes.is_empty() {
            return 0;
        }
        let before = self.entries.len();
        let mut freed = 0;
        self.entries.retain(|_, entry| {
            let hit = event.changes.iter().any(|c| {
                if entry.history
                    && let Change::AttrChanged { path, .. } = c
                {
                    return entry.deps.iter().any(|d| related(&d.path, path));
                }
                affects(c, &entry.deps)
            });
            if hit {
                freed += entry.bytes;
            }
            !hit
        });
        self.stats.bytes -= freed;
        let dropped = before - self.entries.len();
        self.stats.invalidated += dropped as u64;
        dropped
    }

    /// Drops everything.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats.bytes = 0;
    }

    /// Times with a cached frame for `comp` (any request), sorted: the data for
    /// the timeline's cached-frames bar (AE's green line).
    pub fn cached_times(&self, comp: &PrimPath) -> Vec<Time> {
        let mut times: Vec<Time> = self
            .entries
            .keys()
            .filter(|k| &k.comp == comp)
            .map(|k| k.time)
            .collect();
        times.sort_unstable();
        times.dedup();
        times
    }
}

/// Whether `change` can alter a frame built from `deps`.
pub fn affects(change: &Change, deps: &[Dependency]) -> bool {
    match change {
        Change::AttrChanged { path, attr, range } => {
            // The most specific dependencies covering `path` decide, in their own
            // time base; a layer that does not show in the frame only through
            // the switches that could make it show.
            let affected = |dep: &Dependency| {
                if dep.time == Dependency::INACTIVE {
                    dep.path == *path && Dependency::SWITCHES.contains(&attr.as_str())
                } else {
                    range.contains(dep.time)
                }
            };
            let mut best_depth = None;
            let mut hit = false;
            for dep in deps.iter().filter(|d| d.path.is_prefix_of(path)) {
                let depth = dep.path.depth();
                match best_depth {
                    Some(best) if depth < best => {}
                    Some(best) if depth == best => hit |= affected(dep),
                    _ => {
                        best_depth = Some(depth);
                        hit = affected(dep);
                    }
                }
            }
            hit
        }
        Change::PrimMoved { from, to } => deps
            .iter()
            .any(|d| related(&d.path, from) || related(&d.path, to)),
        other => {
            let path = other.path();
            deps.iter().any(|d| related(&d.path, path))
        }
    }
}

fn related(a: &PrimPath, b: &PrimPath) -> bool {
    a.is_prefix_of(b) || b.is_prefix_of(a)
}

#[cfg(test)]
mod tests {
    use bae_model::{EventOrigin, TimeRange};

    use super::*;

    fn p(s: &str) -> PrimPath {
        PrimPath::new(s).unwrap()
    }

    fn key(t: i64) -> FrameKey {
        FrameKey {
            comp: p("/comps/main"),
            time: Time::from_flicks(t),
            request: 0,
        }
    }

    /// A frame at comp time `t` whose layer `a` starts at 100 (layer time = t - 100).
    fn deps(t: i64) -> Vec<Dependency> {
        vec![
            Dependency {
                path: p("/comps/main"),
                time: Time::from_flicks(t),
            },
            Dependency {
                path: p("/comps/main/layers/a"),
                time: Time::from_flicks(t - 100),
            },
            Dependency {
                path: p("/assets/img"),
                time: Time::ZERO,
            },
        ]
    }

    fn event(changes: Vec<Change>) -> StageEvent {
        StageEvent {
            origin: EventOrigin::Local,
            author: None,
            sublayers: Vec::new(),
            changes,
        }
    }

    fn filled() -> FrameCache<u32> {
        let mut cache = FrameCache::new(1_000);
        for t in [0, 100, 200, 300, 400] {
            cache.insert(key(t), t as u32, 10, deps(t), 0);
        }
        cache
    }

    #[test]
    fn inert_layers_depend_only_on_their_switches() {
        // A layer shown only in the middle of the comp: frames outside its
        // in/out points do not depend on its content.
        let mut comp = crate::RenderComp::new(p("/comps/main"), 10, 10);
        let mut shown = crate::RenderLayer::new(p("/comps/main/layers/a"), Default::default());
        shown.time = Time::from_flicks(5);
        let mut hidden = crate::RenderLayer::new(p("/comps/main/layers/b"), Default::default());
        hidden.inert = true;
        comp.layers = vec![shown, hidden];
        let deps = comp.dependencies();
        let set = |path: &str, attr: &str| Change::AttrChanged {
            path: p(path),
            attr: attr.into(),
            range: TimeRange::ALL,
        };
        assert!(!affects(
            &set("/comps/main/layers/b/contents/fill", "color"),
            &deps
        ));
        assert!(!affects(
            &set("/comps/main/layers/b", "transform:position"),
            &deps
        ));
        assert!(affects(&set("/comps/main/layers/b", "in_point"), &deps));
        assert!(affects(&set("/comps/main/layers/b", "enabled"), &deps));
        assert!(affects(
            &set("/comps/main/layers/a/contents/fill", "color"),
            &deps
        ));
    }

    #[test]
    fn layer_attr_change_uses_layer_time() {
        let mut cache = filled();
        // Keyframes of layer `a` in layer time [0, 150) = comp time [100, 250).
        let n = cache.invalidate(&event(vec![Change::AttrChanged {
            path: p("/comps/main/layers/a"),
            attr: "transform:position".into(),
            range: TimeRange::new(Time::from_flicks(0), Time::from_flicks(150)),
        }]));
        assert_eq!(n, 2);
        assert_eq!(
            cache.cached_times(&p("/comps/main")),
            [0, 300, 400].map(Time::from_flicks)
        );
        assert_eq!(cache.stats().bytes, 30);
    }

    #[test]
    fn comp_attr_change_uses_comp_time_and_unrelated_changes_keep_frames() {
        let mut cache = filled();
        assert_eq!(
            cache.invalidate(&event(vec![Change::AttrChanged {
                path: p("/comps/other/layers/x"),
                attr: "transform:opacity".into(),
                range: TimeRange::ALL,
            }])),
            0
        );
        let n = cache.invalidate(&event(vec![Change::AttrChanged {
            path: p("/comps/main"),
            attr: "bg_color".into(),
            range: TimeRange::new(Time::from_flicks(350), Time::MAX),
        }]));
        assert_eq!(n, 1);
        // A new layer anywhere in the comp drops every frame of it.
        let n = cache.invalidate(&event(vec![Change::PrimCreated {
            path: p("/comps/main/layers/new"),
        }]));
        assert_eq!(n, 4);
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn editing_a_previous_keyframe_drops_accumulated_surfaces() {
        let mut cache = filled();
        cache.insert_surface(key(400), 400, 10, deps(400), 0, true);
        let change = Change::AttrChanged {
            path: p("/comps/main/layers/a"),
            attr: "transform:opacity".into(),
            range: TimeRange::new(Time::ZERO, Time::from_flicks(50)),
        };
        assert_eq!(cache.invalidate(&event(vec![change])), 2);
        assert!(!cache.contains(&key(400)));
        assert!(cache.contains(&key(300)));
    }

    #[test]
    fn asset_relink_and_resync() {
        let mut cache = filled();
        let n = cache.invalidate(&event(vec![Change::AttrChanged {
            path: p("/assets/img"),
            attr: "file:relative".into(),
            range: TimeRange::ALL,
        }]));
        assert_eq!(n, 5);
        let mut cache = filled();
        assert_eq!(
            cache.invalidate(&event(vec![Change::Resynced {
                path: PrimPath::root()
            }])),
            5
        );
    }

    #[test]
    fn lru_budget() {
        let mut cache = FrameCache::new(30);
        for t in 0..3 {
            cache.insert(key(t), t, 10, deps(t), 0);
        }
        assert!(cache.get(&key(0)).is_some()); // 0 is now most recent
        cache.insert(key(3), 3, 10, deps(3), 0);
        assert!(cache.contains(&key(0)));
        assert!(!cache.contains(&key(1)), "least recently used is evicted");
        assert_eq!(cache.stats().evicted, 1);
        cache.insert(key(9), 9, 31, deps(9), 0);
        assert!(!cache.contains(&key(9)), "larger than the budget");
        cache.set_budget(10);
        assert_eq!(cache.stats().entries, 1);
    }
}
