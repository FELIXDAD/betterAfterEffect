//! Time mapping: comp time ↔ layer time (AE-ANI-087), nested comp time (AE-ANI-093/094)
//! and motion-blur sample times (AE-ANI-103).
//!
//! **Time base of keyframes.** Keyframes of a layer's attributes (and of every prim
//! below the layer: masks, effects, shape contents) are stored in *layer time*, as AE
//! stores them internally: moving a layer's start time or changing its stretch moves
//! its keyframes without rewriting them. AE scripting reports comp times
//! (`Property.keyTime`), so importers convert with [`LayerTiming::layer_time`].
//! Attributes of other prims (comps, the project) are keyed in their comp's time.

use bae_model::{FrameRate, Time, Value};
use serde::Serialize;

/// A layer's placement in its comp's time.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LayerTiming {
    /// Comp time at which layer time 0 occurs.
    pub start_time: Time,
    /// Percent; 100 = normal speed, negative = reversed. `|stretch| < 1` is treated as ±1.
    pub stretch: f64,
    /// Comp time span where the layer is active. For reversed layers AE may report
    /// `in_point > out_point`; [`LayerTiming::is_active`] accepts either order.
    pub in_point: Time,
    pub out_point: Time,
}

impl Default for LayerTiming {
    fn default() -> Self {
        LayerTiming {
            start_time: Time::ZERO,
            stretch: 100.0,
            in_point: Time::MIN,
            out_point: Time::MAX,
        }
    }
}

impl LayerTiming {
    /// Effective AE stretch in percent, including the minimum nonzero magnitude.
    pub fn stretch_percent(&self) -> f64 {
        let s = if self.stretch.is_finite() && self.stretch != 0.0 {
            self.stretch
        } else {
            100.0
        };
        if s.abs() < 1.0 { s.signum() } else { s }
    }

    /// `layerTime = (compTime - startTime) * 100 / stretch`.
    pub fn layer_time(&self, comp_time: Time) -> Time {
        let s = self.stretch_percent();
        let d = comp_time.0 as i128 - self.start_time.0 as i128;
        if s == 100.0 {
            return Time(d as i64);
        }
        if s == -100.0 {
            return Time((-d) as i64);
        }
        Time((d as f64 * (100.0 / s)).round() as i64)
    }

    /// `compTime = startTime + layerTime * stretch / 100`.
    pub fn comp_time(&self, layer_time: Time) -> Time {
        let s = self.stretch_percent();
        if s == 100.0 {
            return self.start_time + layer_time;
        }
        Time(self.start_time.0 + (layer_time.0 as f64 * (s / 100.0)).round() as i64)
    }

    /// Whether the layer is active (between its in and out points) at `comp_time`.
    pub fn is_active(&self, comp_time: Time) -> bool {
        let (a, b) = if self.in_point <= self.out_point {
            (self.in_point, self.out_point)
        } else {
            (self.out_point, self.in_point)
        };
        a <= comp_time && comp_time < b
    }
}

/// Interpret a stored frame rate: `float` fps (AE value) or `vec2` `[num, den]`.
/// Common NTSC rates are recognised (29.97002997 → 30000/1001).
pub fn frame_rate_from_value(v: &Value) -> Option<FrameRate> {
    match v {
        Value::Vec2([n, d]) if *n > 0.0 && *d > 0.0 => Some(FrameRate::new(*n as u32, *d as u32)),
        Value::Int(n) if *n > 0 => Some(FrameRate::new(*n as u32, 1)),
        Value::Float(f) if *f > 0.0 => Some(frame_rate_from_fps(*f)),
        _ => None,
    }
}

/// Rational frame rate for a floating-point fps.
pub fn frame_rate_from_fps(fps: f64) -> FrameRate {
    let rounded = fps.round();
    if (fps - rounded).abs() < 1e-6 {
        return FrameRate::new(rounded as u32, 1);
    }
    // NTSC: n * 1000 / 1001.
    let ntsc = (fps * 1001.0 / 1000.0).round();
    if (ntsc * 1000.0 / 1001.0 - fps).abs() < 1e-6 {
        return FrameRate::new((ntsc * 1000.0) as u32, 1001);
    }
    // Anything else: millihertz precision.
    FrameRate::new((fps * 1000.0).round() as u32, 1000)
}

/// Inner comp time for a pre-comp layer whose source time is `source_time`
/// (AE-ANI-093/094): unchanged, or snapped down to the inner comp's frame when the
/// inner comp preserves its frame rate.
pub fn nested_comp_time(
    source_time: Time,
    inner_rate: FrameRate,
    preserve_frame_rate: bool,
) -> Time {
    if preserve_frame_rate && inner_rate.num > 0 {
        inner_rate.snap(source_time)
    } else {
        source_time
    }
}

/// Motion-blur shutter settings of a comp (AE-ANI-103/104).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct MotionBlur {
    /// The comp's motion-blur switch (layers also need their own switch on).
    pub enabled: bool,
    /// Degrees, 0–720; 360 = one frame.
    pub shutter_angle: f64,
    /// Degrees, −360–360; offset of shutter open from the frame time.
    pub shutter_phase: f64,
    /// Samples per frame (minimum / fixed count).
    pub samples: u32,
    /// Adaptive sample limit (maximum for 2D layers).
    pub adaptive_limit: u32,
}

impl Default for MotionBlur {
    fn default() -> Self {
        MotionBlur {
            enabled: false,
            shutter_angle: 180.0,
            shutter_phase: -90.0,
            samples: 16,
            adaptive_limit: 128,
        }
    }
}

impl MotionBlur {
    /// Sample times for the frame at `time`, `samples` of them:
    /// `t_k = t_open + dur * k / N` with `t_open = t + phase/360 · T` and
    /// `dur = angle/360 · T` (AE-ANI-103; the first sample at shutter open, the
    /// last one `dur / N` before it closes, measured in AE 26.2.1).
    /// Returns just `[time]` when the shutter angle is 0 or `samples` is 0.
    pub fn sample_times(&self, time: Time, frame_rate: FrameRate, samples: u32) -> Vec<Time> {
        if self.shutter_angle <= 0.0 || samples == 0 || frame_rate.num == 0 {
            return vec![time];
        }
        let frame = frame_rate.frame_duration().0 as f64;
        let open = time.0 as f64 + self.shutter_phase / 360.0 * frame;
        let dur = self.shutter_angle / 360.0 * frame;
        (0..samples)
            .map(|k| Time((open + dur * k as f64 / samples as f64).round() as i64))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretch_and_offset() {
        let t = LayerTiming {
            start_time: Time::ONE_SECOND,
            stretch: 200.0,
            ..Default::default()
        };
        assert_eq!(t.layer_time(Time::from_seconds(3.0)), Time::ONE_SECOND);
        assert_eq!(t.comp_time(Time::ONE_SECOND), Time::from_seconds(3.0));
        let r = LayerTiming {
            stretch: -100.0,
            ..Default::default()
        };
        assert_eq!(
            r.layer_time(Time::from_seconds(-2.0)),
            Time::from_seconds(2.0)
        );
    }

    #[test]
    fn rates() {
        assert_eq!(
            frame_rate_from_fps(29.970_029_970_029_97),
            FrameRate::FPS_29_97
        );
        assert_eq!(
            frame_rate_from_fps(23.976_023_976_023_978),
            FrameRate::FPS_23_976
        );
        assert_eq!(frame_rate_from_fps(25.0), FrameRate::FPS_25);
        assert_eq!(
            frame_rate_from_value(&Value::Vec2([60000.0, 1001.0])),
            Some(FrameRate::FPS_59_94)
        );
    }

    #[test]
    fn default_shutter_is_centred() {
        let mb = MotionBlur::default();
        let r = FrameRate::FPS_24;
        let t = r.frame_to_time(10);
        let s = mb.sample_times(t, r, 2);
        let quarter = r.frame_duration().0 / 4;
        // Interval [t - T/4, t + T/4], two samples at its start and middle.
        assert_eq!(s, vec![Time(t.0 - quarter), t]);
    }
}
