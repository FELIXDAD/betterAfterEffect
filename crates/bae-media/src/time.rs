//! Time for media: re-exports `bae-model`'s [`Time`] (integer flicks,
//! ARCHITECTURE §4.4) and [`FrameRate`], plus what only media needs: snapping a
//! container's float frame rate to a rational one, and SMPTE [`Timecode`].
//!
//! Media frame indices are always **0-based from the first frame of the source**
//! (for an image sequence `name_0101.png … name_0200.png`, index 0 is `0101`).

use std::fmt;

pub use bae_model::{FrameRate, Time};
use serde::{Deserialize, Serialize};

/// Frame rate given to image sequences unless the caller chooses another (AE
/// preference "Sequence Footage: frames per second", default 30).
pub const DEFAULT_SEQUENCE_RATE: FrameRate = FrameRate::FPS_30;

/// `true` for a usable rate (non-zero numerator and denominator).
pub fn is_valid_rate(rate: FrameRate) -> bool {
    rate.num != 0 && rate.den != 0
}

/// Snaps a measured rate (a container's float `nominalFrameRate`, an average
/// over sample durations) to the closest common rate when within 0.01 fps,
/// otherwise to `round(fps * 1000) / 1000`.
pub fn frame_rate_from_f64(fps: f64) -> Option<FrameRate> {
    if !fps.is_finite() || fps <= 0.0 {
        return None;
    }
    const COMMON: &[FrameRate] = &[
        FrameRate::new(24_000, 1001),
        FrameRate::new(24, 1),
        FrameRate::new(25, 1),
        FrameRate::new(30_000, 1001),
        FrameRate::new(30, 1),
        FrameRate::new(48, 1),
        FrameRate::new(50, 1),
        FrameRate::new(60_000, 1001),
        FrameRate::new(60, 1),
        FrameRate::new(100, 1),
        FrameRate::new(120_000, 1001),
        FrameRate::new(120, 1),
        FrameRate::new(12, 1),
        FrameRate::new(15, 1),
    ];
    if let Some(rate) = COMMON
        .iter()
        .copied()
        .find(|rate| (rate.as_f64() - fps).abs() < 0.01)
    {
        return Some(rate);
    }
    let num = (fps * 1000.0).round();
    (num >= 1.0 && num <= f64::from(u32::MAX)).then(|| FrameRate::new(num as u32, 1000))
}

/// Frames counted per timecode second (24, 25, 30, 60 …).
pub fn timecode_base(rate: FrameRate) -> u32 {
    rate.as_f64().round().max(1.0) as u32
}

/// Whether drop-frame timecode can apply (29.97 / 59.94).
pub fn is_ntsc(rate: FrameRate) -> bool {
    rate.den == 1001 && matches!(rate.num, 30_000 | 60_000)
}

/// SMPTE timecode `HH:MM:SS:FF` (`;` before the frames for drop-frame).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timecode {
    /// Hours (0–23).
    pub hours: u8,
    /// Minutes (0–59).
    pub minutes: u8,
    /// Seconds (0–59).
    pub seconds: u8,
    /// Frames within the second.
    pub frames: u8,
    /// Drop-frame counting (29.97 / 59.94 only).
    pub drop_frame: bool,
}

impl Timecode {
    /// Timecode of absolute frame number `frame` at `rate` (counted from
    /// `00:00:00:00`). Drop-frame counting is used when `drop_frame` is set and the
    /// rate is NTSC.
    pub fn from_frame_number(frame: u64, rate: FrameRate, drop_frame: bool) -> Self {
        let base = u64::from(timecode_base(rate));
        let drop_frame = drop_frame && is_ntsc(rate);
        let mut frame = frame;
        if drop_frame {
            // Drop 2 (30 fps) or 4 (60 fps) frame numbers every minute except
            // every tenth minute.
            let drop = base / 15;
            let per_10_min = base * 600 - drop * 9;
            let per_min = base * 60 - drop;
            let tens = frame / per_10_min;
            let rem = frame % per_10_min;
            frame += drop * 9 * tens;
            if rem > drop {
                frame += drop * ((rem - drop) / per_min);
            }
        }
        let frames = frame % base;
        let total_seconds = frame / base;
        Timecode {
            hours: ((total_seconds / 3600) % 24) as u8,
            minutes: ((total_seconds / 60) % 60) as u8,
            seconds: (total_seconds % 60) as u8,
            frames: frames as u8,
            drop_frame,
        }
    }

    /// Absolute frame number of this timecode at `rate` (inverse of
    /// [`Self::from_frame_number`]).
    pub fn to_frame_number(self, rate: FrameRate) -> u64 {
        let base = u64::from(timecode_base(rate));
        let total_minutes = u64::from(self.hours) * 60 + u64::from(self.minutes);
        let mut frame =
            (total_minutes * 60 + u64::from(self.seconds)) * base + u64::from(self.frames);
        if self.drop_frame && is_ntsc(rate) {
            let drop = base / 15;
            frame -= drop * (total_minutes - total_minutes / 10);
        }
        frame
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sep = if self.drop_frame { ';' } else { ':' };
        write!(
            f,
            "{:02}:{:02}:{:02}{sep}{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_frame_index_round_trip() {
        for rate in [
            FrameRate::FPS_23_976,
            FrameRate::FPS_25,
            FrameRate::FPS_29_97,
            FrameRate::FPS_60,
        ] {
            for frame in [0_i64, 1, 1000, 123_457] {
                assert_eq!(rate.time_to_frame(rate.frame_to_time(frame)), frame);
            }
        }
    }

    #[test]
    fn snap_measured_rates() {
        assert_eq!(frame_rate_from_f64(23.976_025), Some(FrameRate::FPS_23_976));
        assert_eq!(frame_rate_from_f64(29.97), Some(FrameRate::FPS_29_97));
        assert_eq!(frame_rate_from_f64(25.0), Some(FrameRate::FPS_25));
        assert_eq!(frame_rate_from_f64(7.5), Some(FrameRate::new(7500, 1000)));
        assert_eq!(frame_rate_from_f64(0.0), None);
    }

    #[test]
    fn timecode_non_drop() {
        let tc = Timecode::from_frame_number(24 * 3661 + 5, FrameRate::FPS_24, false);
        assert_eq!(tc.to_string(), "01:01:01:05");
        assert_eq!(tc.to_frame_number(FrameRate::FPS_24), 24 * 3661 + 5);
    }

    #[test]
    fn timecode_drop_frame() {
        let rate = FrameRate::FPS_29_97;
        // Frame 1800 is the first frame of minute 1: 00:01:00;02.
        let tc = Timecode::from_frame_number(1800, rate, true);
        assert_eq!(tc.to_string(), "00:01:00;02");
        // Minute 10 does not drop: 17982 frames per 10 minutes.
        assert_eq!(
            Timecode::from_frame_number(17_982, rate, true).to_string(),
            "00:10:00;00"
        );
        for frame in [0, 1799, 1800, 1801, 17_981, 17_982, 107_892, 123_456] {
            let tc = Timecode::from_frame_number(frame, rate, true);
            assert_eq!(tc.to_frame_number(rate), frame, "{tc}");
        }
    }
}
