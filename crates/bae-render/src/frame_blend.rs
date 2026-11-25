//! AE Frame Mix weights (AE-CMP-065). A comp frame mixes the source frames
//! under a tent centred on its source position: the tent is about one and a
//! half frames wide while the source plays at or below its own rate, and widens
//! with the speed above it. Measured in AE 2026 (weights fit within 0.02);
//! frame-aligned positions still mix about 89 / 5.5 / 5.5 %.

use bae_model::{FrameRate, Time};

/// Most frames one comp frame mixes.
const MAX_FRAMES: f64 = 64.0;
/// Weights below this are dropped (and the rest renormalised).
const MIN_WEIGHT: f64 = 1e-3;

/// The CDF of a tent of half-width `h` centred on 0.
fn tent_cdf(x: f64, h: f64) -> f64 {
    if x <= -h {
        0.0
    } else if x >= h {
        1.0
    } else if x < 0.0 {
        (x + h).powi(2) / (2.0 * h * h)
    } else {
        1.0 - (h - x).powi(2) / (2.0 * h * h)
    }
}

/// The source frames (by start time) and weights to mix at source time `time`
/// when one comp frame spans `span` of a source at `rate`. Frames before the
/// first count as the first (the decoder clamps past the last). The weights sum
/// to 1.
pub(crate) fn weights(time: Time, span: Time, rate: FrameRate) -> Vec<(Time, f32)> {
    let fps = rate.as_f64();
    let s = time.as_seconds() * fps;
    let v = span.as_seconds() * fps;
    let h = if v <= 1.0 { 0.74 } else { 0.93 * v }.min(MAX_FRAMES / 2.0);
    let mut out: Vec<(i64, f64)> = Vec::new();
    for k in (s - h - 0.5).floor() as i64..=(s + h + 0.5).ceil() as i64 {
        let w = tent_cdf(k as f64 + 0.5 - s, h) - tent_cdf(k as f64 - 0.5 - s, h);
        if w < MIN_WEIGHT {
            continue;
        }
        let k = k.max(0);
        match out.last_mut() {
            Some((last, acc)) if *last == k => *acc += w,
            _ => out.push((k, w)),
        }
    }
    let total: f64 = out.iter().map(|(_, w)| w).sum();
    out.into_iter()
        .map(|(k, w)| (rate.frame_to_time(k), (w / total) as f32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: FrameRate = FrameRate::FPS_30;

    fn frames(time: f64, span_frames: f64) -> Vec<(i64, f32)> {
        weights(
            Time::from_seconds(time / 30.0),
            Time::from_seconds(span_frames / 30.0),
            FPS,
        )
        .into_iter()
        .map(|(t, w)| (FPS.time_to_frame(t), w))
        .collect()
    }

    #[test]
    fn aligned_frames_still_mix_their_neighbours() {
        let w = frames(10.0, 1.0);
        assert_eq!(w.iter().map(|f| f.0).collect::<Vec<_>>(), [9, 10, 11]);
        assert!((w[1].1 - 0.895).abs() < 0.01, "{w:?}");
        assert!((w[0].1 - w[2].1).abs() < 1e-6);
        assert!((w.iter().map(|f| f.1).sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn half_way_mixes_two_frames_evenly() {
        let w = frames(10.5, 0.5);
        assert_eq!(w.iter().map(|f| f.0).collect::<Vec<_>>(), [10, 11]);
        assert!((w[0].1 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn fast_playback_widens_the_mix_and_clamps_at_the_start() {
        let w = frames(30.0, 3.0);
        assert!(w.len() >= 6, "{w:?}");
        assert_eq!(frames(0.0, 3.0)[0].0, 0);
        assert!(frames(0.0, 3.0)[0].1 > 0.4);
    }
}
