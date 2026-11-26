//! Layer pixels for the `sampleImage` expression method.
//!
//! Expressions run before anything is rendered, so pixels reach them through
//! this cache. A miss records a request and reads as transparent black; the
//! renderer that shares the cache renders the sampled layer, reads the sample
//! area back without waiting for the GPU, and fills the value in; the next
//! evaluation finds it. Callers that show frames hold a frame back while
//! [`ImageSamples::is_waiting`] (the preview keeps the previous image, export
//! waits), so a frame with a missing sample is never shown.

use std::sync::{Mutex, MutexGuard};

use bae_model::{PrimPath, Time};
use rustc_hash::FxHashMap;

/// Sample areas are kept in 1/1024 layer pixel.
const AREA_UNIT: f64 = 1024.0;

/// Finished samples kept before the oldest are dropped.
const MAX_SAMPLES: usize = 1 << 16;

/// One `sampleImage` call: which layer, when, and where.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SampleKey {
    /// The comp the sampled layer is in.
    pub comp: PrimPath,
    /// The sampled layer.
    pub layer: PrimPath,
    /// Comp time of the sample.
    pub time: Time,
    /// `[left, top, right, bottom]` in layer pixels, in units of 1/1024 pixel.
    area: [i64; 4],
    /// With the layer's masks and effects (`postEffect`), or its source alone.
    pub post_effect: bool,
}

impl SampleKey {
    /// The area `center ± radius` in layer pixels (a layer pixel `(x, y)`
    /// covers `[x, x + 1) × [y, y + 1)`).
    pub fn new(
        comp: PrimPath,
        layer: PrimPath,
        time: Time,
        center: [f64; 2],
        radius: [f64; 2],
        post_effect: bool,
    ) -> Self {
        let unit = |v: f64| (v * AREA_UNIT).round() as i64;
        let [rx, ry] = radius.map(f64::abs);
        Self {
            comp,
            layer,
            time,
            area: [
                unit(center[0] - rx),
                unit(center[1] - ry),
                unit(center[0] + rx),
                unit(center[1] + ry),
            ],
            post_effect,
        }
    }

    /// `[left, top, right, bottom]` in layer pixels.
    pub fn area(&self) -> [f64; 4] {
        self.area.map(|v| v as f64 / AREA_UNIT)
    }
}

#[derive(Debug)]
enum State {
    /// Asked for by an expression; no renderer has taken it yet.
    Requested,
    /// A renderer is reading it back.
    Taken,
    /// Straight RGBA.
    Done([f32; 4]),
}

#[derive(Debug, Default)]
struct Inner {
    states: FxHashMap<SampleKey, State>,
    requested: Vec<SampleKey>,
    /// Bumped by [`ImageSamples::clear`]: results of older requests are dropped.
    generation: u64,
}

/// Requests taken by a renderer ([`ImageSamples::take_requests`]).
#[derive(Debug, Default)]
pub struct SampleRequests {
    /// Pass back to [`ImageSamples::fulfil`].
    pub generation: u64,
    /// The samples to read.
    pub keys: Vec<SampleKey>,
}

/// The sample cache shared by an evaluator and the renderer that serves it.
#[derive(Debug, Default)]
pub struct ImageSamples {
    inner: Mutex<Inner>,
}

impl ImageSamples {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The straight RGBA of `key`, or `None` after recording a request for it.
    pub fn get(&self, key: &SampleKey) -> Option<[f32; 4]> {
        let mut inner = self.lock();
        match inner.states.get(key) {
            Some(State::Done(v)) => Some(*v),
            Some(_) => None,
            None => {
                inner.states.insert(key.clone(), State::Requested);
                inner.requested.push(key.clone());
                None
            }
        }
    }

    /// Requests no renderer has taken yet; they count as waiting until
    /// [`ImageSamples::fulfil`]led.
    pub fn take_requests(&self) -> SampleRequests {
        let mut inner = self.lock();
        let keys = std::mem::take(&mut inner.requested);
        for key in &keys {
            inner.states.insert(key.clone(), State::Taken);
        }
        SampleRequests {
            generation: inner.generation,
            keys,
        }
    }

    /// Stores the straight RGBA of a taken request (ignored when the cache was
    /// cleared since it was taken).
    pub fn fulfil(&self, generation: u64, key: SampleKey, rgba: [f32; 4]) {
        let mut inner = self.lock();
        if inner.generation != generation {
            return;
        }
        if inner.states.len() >= MAX_SAMPLES {
            inner.states.retain(|_, s| !matches!(s, State::Done(_)));
        }
        inner.states.insert(key, State::Done(rgba));
    }

    /// Whether any request is still without a value (taken or not).
    pub fn is_waiting(&self) -> bool {
        self.lock()
            .states
            .values()
            .any(|s| !matches!(s, State::Done(_)))
    }

    /// Whether some request has not been taken by a renderer yet.
    pub fn has_requests(&self) -> bool {
        !self.lock().requested.is_empty()
    }

    /// Forgets every sample (the project changed, so pixels may have too).
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.states.clear();
        inner.requested.clear();
        inner.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: f64) -> SampleKey {
        SampleKey::new(
            PrimPath::new("/comps/c").unwrap(),
            PrimPath::new("/comps/c/layers/l").unwrap(),
            Time::ZERO,
            [x, 20.0],
            [0.5, 0.5],
            true,
        )
    }

    #[test]
    fn miss_request_fulfil_hit() {
        let s = ImageSamples::new();
        assert_eq!(key(10.0).area(), [9.5, 19.5, 10.5, 20.5]);
        assert_eq!(s.get(&key(10.0)), None);
        assert_eq!(s.get(&key(10.0)), None, "asked once");
        assert!(s.is_waiting() && s.has_requests());
        let taken = s.take_requests();
        assert_eq!(taken.keys, vec![key(10.0)]);
        assert!(s.is_waiting() && !s.has_requests());
        s.fulfil(taken.generation, key(10.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(s.get(&key(10.0)), Some([1.0, 0.0, 0.0, 1.0]));
        assert!(!s.is_waiting());
    }

    #[test]
    fn clear_drops_results_of_older_requests() {
        let s = ImageSamples::new();
        s.get(&key(1.0));
        let taken = s.take_requests();
        s.clear();
        s.fulfil(taken.generation, key(1.0), [1.0; 4]);
        assert_eq!(s.get(&key(1.0)), None);
        assert!(s.has_requests());
    }
}
