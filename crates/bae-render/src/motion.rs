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

use bae_model::Time;
use glam::{DAffine2, DMat4};

use crate::camera::Camera;
use crate::geom::Rect;

/// A comp's motion blur at one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct CompMotionBlur {
    /// Sample times (comp time) in shutter order:
    /// `t_k = t_open + dur · (k + ½) / N` (AE-ANI-103).
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

/// AE's adaptive sample count for a 2D layer (AE-ANI-104): `transforms` place
/// the layer's content `rect` at the comp's evenly spaced sample times (at least
/// Samples Per Frame of them). When a corner travels more than `step` output
/// pixels between samples, more samples are taken — up to one per `step` of
/// travel, at most `limit` — by interpolating the given transforms piecewise
/// linearly (extrapolating over the half steps at both ends of the shutter).
/// Returns the transforms unchanged when they are dense enough.
pub fn adaptive_samples(
    transforms: &[DAffine2],
    rect: &Rect,
    limit: u32,
    step: f64,
) -> Vec<DAffine2> {
    let n = transforms.len();
    if n < 2 || step <= 0.0 {
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
    // The samples sit half a step inside the shutter interval at both ends.
    let total = travel * n as f64 / (n - 1) as f64;
    let wanted = (total / step).ceil();
    if !wanted.is_finite() || wanted <= n as f64 {
        return transforms.to_vec();
    }
    let m = (wanted as usize).min(limit.max(n as u32) as usize);
    if m <= n {
        return transforms.to_vec();
    }
    (0..m)
        .map(|j| {
            let s = (j as f64 + 0.5) / m as f64;
            let f = s * n as f64 - 0.5;
            let i = (f.floor() as isize).clamp(0, n as isize - 2) as usize;
            let t = f - i as f64;
            lerp(&transforms[i], &transforms[i + 1], t)
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
    fn fast_motion_gets_one_sample_per_pixel() {
        // 16 samples 4 px apart: 64 px of travel → 64 samples, evenly spaced and
        // extending half a (new) step past the extremes of the old ones.
        let t: Vec<DAffine2> = (0..16).map(|k| at(2.0 + f64::from(k) * 4.0)).collect();
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 128, 1.0);
        assert_eq!(out.len(), 64);
        let xs: Vec<f64> = out.iter().map(|a| a.translation.x).collect();
        for w in xs.windows(2) {
            assert!((w[1] - w[0] - 1.0).abs() < 1e-9, "{w:?}");
        }
        assert!(
            (xs[0] - 0.5).abs() < 1e-9 && (xs[63] - 63.5).abs() < 1e-9,
            "{xs:?}"
        );
        // Capped by the adaptive limit.
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 40, 1.0);
        assert_eq!(out.len(), 40);
        // Coarser output resolution (half res): half the samples.
        let out = adaptive_samples(&t, &Rect::from_size(10.0, 10.0), 128, 2.0);
        assert_eq!(out.len(), 32);
    }

    #[test]
    fn rotation_counts_corner_travel() {
        let rect = Rect::from_size(100.0, 100.0);
        let t: Vec<DAffine2> = (0..4)
            .map(|k| DAffine2::from_angle(f64::from(k) * 0.1))
            .collect();
        let out = adaptive_samples(&t, &rect, 256, 1.0);
        // The far corner (141 px out) moves ~14 px per step: ~57 samples.
        assert!(out.len() > 50 && out.len() < 64, "{}", out.len());
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
