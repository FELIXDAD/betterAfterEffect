//! Motion blur (AE-CMP-064, AE-ANI-102 … 105): the per-frame data the renderer
//! receives and the sample arithmetic.
//!
//! A layer is blurred when its Motion Blur switch and the comp's Enable Motion
//! Blur switch are both on. 03 (`bae_anim::MotionBlur::sample_times`) gives the
//! sample times inside the shutter interval (shutter angle, phase, samples per
//! frame from the comp settings); the evaluator samples each blurred layer's
//! transform (and the active camera) at those times into [`LayerSample`]s.
//!
//! The renderer averages the layer drawn at every sample in premultiplied space
//! with equal weights, on the GPU, in the same draw that composites the layer:
//! the composite shader loops over the samples per pixel, so a blurred layer
//! costs one draw, no extra target and no readback. Like AE, raster content
//! (source, masks, effects) is rendered once at the frame time and only the
//! transform is sampled; 2D layers get AE's adaptive sample count
//! ([`adaptive_samples`]), 3D layers the fixed samples per frame.
//!
//! Measured in AE 26.2.1 (`crates/bae-render/tests/ae/README.md`): samples sit at
//! `t_open + dur · k / N` (the first at shutter open), and 2D layers get one
//! sample per 1.3 px of travel below 32 px, one per 1.9 px from there on.

use bae_model::Time;
use glam::{DAffine2, DMat4};

use crate::camera::Camera;
use crate::geom::Rect;

/// A comp's motion blur at one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct CompMotionBlur {
    /// Sample times (comp time) in shutter order:
    /// `t_k = t_open + dur · k / N` (AE-ANI-103).
    pub times: Vec<Time>,
    /// The active camera at each sample time (`None` = the default camera).
    pub cameras: Vec<Option<Camera>>,
    /// Adaptive Sample Limit: the most samples a 2D layer gets (AE default 128).
    pub adaptive_limit: u32,
}

/// A blurred layer's placement at one sample time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerSample {
    /// Layer space → comp space (2D part, as [`crate::RenderLayer::transform`]).
    pub transform: DAffine2,
    /// 3D layers: layer space → comp space (as [`crate::RenderLayer::matrix_3d`]).
    pub matrix_3d: Option<DMat4>,
}

/// Whether all samples place the layer identically (then it needs no blur).
pub fn is_static(samples: &[LayerSample]) -> bool {
    let Some(first) = samples.first() else {
        return true;
    };
    samples.iter().all(|s| {
        s.transform.abs_diff_eq(first.transform, 1e-9)
            && match (s.matrix_3d, first.matrix_3d) {
                (Some(a), Some(b)) => a.abs_diff_eq(b, 1e-9),
                (None, None) => true,
                _ => false,
            }
    })
}

/// Travel (output pixels over the whole shutter) below which AE spaces 2D
/// samples [`STEP_SHORT`] apart, and [`STEP_LONG`] from there on.
pub const LONG_TRAVEL: f64 = 32.0;
/// Sample spacing for short travel (output pixels).
pub const STEP_SHORT: f64 = 1.3;
/// Sample spacing for long travel (output pixels).
pub const STEP_LONG: f64 = 1.9;

/// AE's adaptive sample count for a 2D layer (AE-ANI-104), as measured in AE
/// 26.2.1: with `d` the largest travel of a corner of `rect` over the whole
/// shutter, in output pixels (comp pixels / `scale`),
/// `N = clamp(floor(d / step) + 1, samples, limit)` – sample points at most
/// `step` apart along the travel – where `step` is 1.3 px when `d < 32` and
/// 1.9 px otherwise (independent of layer size and direction).
///
/// `transforms` place the layer at the comp's evenly spaced sample times
/// `t_k = t_open + dur · k / n` (Samples Per Frame of them); the extra samples
/// interpolate them piecewise linearly (extrapolating over the last step).
/// Returns the transforms unchanged when they are already enough.
pub fn adaptive_samples(
    transforms: &[DAffine2],
    rect: &Rect,
    limit: u32,
    scale: f64,
) -> Vec<DAffine2> {
    let n = transforms.len();
    if n < 2 || scale <= 0.0 {
        return transforms.to_vec();
    }
    let corners = rect.corners();
    let travel: f64 = transforms
        .windows(2)
        .map(|w| {
            corners
                .iter()
                .map(|c| (w[1].transform_point2(*c) - w[0].transform_point2(*c)).length())
                .fold(0.0, f64::max)
        })
        .sum();
    // The samples cover (n − 1)/n of the shutter.
    let d = travel * n as f64 / (n - 1) as f64 / scale;
    let step = if d < LONG_TRAVEL {
        STEP_SHORT
    } else {
        STEP_LONG
    };
    let wanted = (d / step).floor() + 1.0;
    if !wanted.is_finite() || wanted <= n as f64 {
        return transforms.to_vec();
    }
    let m = (wanted as usize).min(limit.max(n as u32) as usize);
    if m <= n {
        return transforms.to_vec();
    }
    (0..m)
        .map(|j| {
            let f = j as f64 * n as f64 / m as f64;
            let i = (f.floor() as usize).min(n - 2);
            lerp(&transforms[i], &transforms[i + 1], f - i as f64)
        })
        .collect()
}

fn lerp(a: &DAffine2, b: &DAffine2, t: f64) -> DAffine2 {
    DAffine2 {
        matrix2: a.matrix2 * (1.0 - t) + b.matrix2 * t,
        translation: a.translation.lerp(b.translation, t),
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;

    fn at(x: f64) -> DAffine2 {
        DAffine2::from_translation(DVec2::new(x, 0.0))
    }

    #[test]
    fn dense_samples_stay() {
        let t: Vec<DAffine2> = (0..16).map(|k| at(f64::from(k) * 0.5)).collect();
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 128, 1.0);
        assert_eq!(out, t);
    }

    #[test]
    fn sample_counts_match_after_effects() {
        // AE 26.2.1, a 30×30 solid moving d px over the shutter with Samples Per
        // Frame 2 and Adaptive Sample Limit 256: the sample count N fitted
        // exactly from its renders.
        for (d, n) in [
            (3.0, 3),
            (6.0, 5),
            (10.0, 8),
            (16.0, 13),
            (18.0, 14),
            (22.0, 17),
            (24.0, 19),
            (26.0, 21),
            (30.0, 24),
            (32.0, 17),
            (38.0, 21),
            (40.0, 22),
            (60.0, 32),
            (80.0, 43),
            (200.0, 106),
            (300.0, 158),
        ] {
            let t = [at(0.0), at(d / 2.0)];
            let out = adaptive_samples(&t, &Rect::from_size(30.0, 30.0), 256, 1.0);
            assert_eq!(out.len(), n, "{d} px");
        }
    }

    #[test]
    fn extra_samples_follow_the_shutter() {
        // 16 samples 4 px apart (64 px over the shutter) → 34 samples at
        // t = j/34 of the shutter.
        let t: Vec<DAffine2> = (0..16).map(|k| at(2.0 + f64::from(k) * 4.0)).collect();
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 128, 1.0);
        assert_eq!(out.len(), 34);
        for (j, a) in out.iter().enumerate() {
            let x = 2.0 + 64.0 * j as f64 / 34.0;
            assert!(
                (a.translation.x - x).abs() < 1e-9,
                "{j}: {}",
                a.translation.x
            );
        }
        // Capped by the adaptive limit.
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 20, 1.0);
        assert_eq!(out.len(), 20);
        // Half resolution: 32 output px of travel.
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 128, 2.0);
        assert_eq!(out.len(), 17);
    }

    #[test]
    fn rotation_counts_corner_travel() {
        let rect = Rect::from_size(100.0, 100.0);
        let t: Vec<DAffine2> = (0..4)
            .map(|k| DAffine2::from_angle(f64::from(k) * 0.1))
            .collect();
        let out = adaptive_samples(&t, &rect, 256, 1.0);
        // The far corner (141 px out) moves ~14 px per step, ~57 px in all.
        assert_eq!(out.len(), 30);
    }

    #[test]
    fn static_detection() {
        let s = LayerSample {
            transform: at(3.0),
            matrix_3d: None,
        };
        assert!(is_static(&[s, s, s]));
        let mut moved = s;
        moved.transform = at(3.5);
        assert!(!is_static(&[s, moved]));
    }
}
