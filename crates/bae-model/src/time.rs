//! Time in integer flicks (ARCHITECTURE §4.4).
//!
//! One second is [`Time::FLICKS_PER_SECOND`] = 705,600,000 flicks. Every frame of
//! every common frame rate (23.976, 24, 25, 29.97, 30, 47.952, 48, 50, 59.94, 60,
//! 90, 100, 119.88, 120 …) is an exact integer number of flicks, so frame
//! arithmetic never accumulates floating-point error.

use std::borrow::Cow;
use std::fmt;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

/// A point in time (or a duration) in flicks.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Time(pub i64);

impl Time {
    /// Flicks in one second.
    pub const FLICKS_PER_SECOND: i64 = 705_600_000;
    pub const ZERO: Time = Time(0);
    pub const ONE_SECOND: Time = Time(Self::FLICKS_PER_SECOND);
    /// The earliest representable time (used for open-ended ranges).
    pub const MIN: Time = Time(i64::MIN);
    /// The latest representable time (used for open-ended ranges).
    pub const MAX: Time = Time(i64::MAX);

    #[inline]
    pub const fn from_flicks(flicks: i64) -> Self {
        Time(flicks)
    }

    #[inline]
    pub const fn flicks(self) -> i64 {
        self.0
    }

    /// Nearest time to `seconds` (rounded to the closest flick).
    pub fn from_seconds(seconds: f64) -> Self {
        Time((seconds * Self::FLICKS_PER_SECOND as f64).round() as i64)
    }

    pub fn as_seconds(self) -> f64 {
        self.0 as f64 / Self::FLICKS_PER_SECOND as f64
    }

    pub const fn saturating_add(self, rhs: Time) -> Time {
        Time(self.0.saturating_add(rhs.0))
    }

    pub const fn saturating_sub(self, rhs: Time) -> Time {
        Time(self.0.saturating_sub(rhs.0))
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.as_seconds())
    }
}

impl Add for Time {
    type Output = Time;
    fn add(self, rhs: Time) -> Time {
        Time(self.0 + rhs.0)
    }
}

impl Sub for Time {
    type Output = Time;
    fn sub(self, rhs: Time) -> Time {
        Time(self.0 - rhs.0)
    }
}

impl AddAssign for Time {
    fn add_assign(&mut self, rhs: Time) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Time {
    fn sub_assign(&mut self, rhs: Time) {
        self.0 -= rhs.0;
    }
}

impl Neg for Time {
    type Output = Time;
    fn neg(self) -> Time {
        Time(-self.0)
    }
}

impl JsonSchema for Time {
    fn schema_name() -> Cow<'static, str> {
        "Time".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "integer",
            "description": "Time in flicks (1 second = 705600000 flicks)."
        })
    }
}

/// A rational frame rate, e.g. `30000/1001` for 29.97 fps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_23_976: FrameRate = FrameRate::new(24_000, 1001);
    pub const FPS_24: FrameRate = FrameRate::new(24, 1);
    pub const FPS_25: FrameRate = FrameRate::new(25, 1);
    pub const FPS_29_97: FrameRate = FrameRate::new(30_000, 1001);
    pub const FPS_30: FrameRate = FrameRate::new(30, 1);
    pub const FPS_50: FrameRate = FrameRate::new(50, 1);
    pub const FPS_59_94: FrameRate = FrameRate::new(60_000, 1001);
    pub const FPS_60: FrameRate = FrameRate::new(60, 1);
    pub const FPS_120: FrameRate = FrameRate::new(120, 1);

    pub const fn new(num: u32, den: u32) -> Self {
        FrameRate { num, den }
    }

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Duration of one frame. Exact when the rate divides 705,600,000 (all common rates).
    pub fn frame_duration(self) -> Time {
        Time((Time::FLICKS_PER_SECOND as i128 * self.den as i128 / self.num as i128) as i64)
    }

    /// Whether every frame boundary of this rate is an exact number of flicks.
    pub fn is_exact(self) -> bool {
        self.num != 0
            && (Time::FLICKS_PER_SECOND as i128 * self.den as i128) % self.num as i128 == 0
    }

    /// Start time of `frame`.
    pub fn frame_to_time(self, frame: i64) -> Time {
        let flicks =
            frame as i128 * Time::FLICKS_PER_SECOND as i128 * self.den as i128 / self.num as i128;
        Time(flicks as i64)
    }

    /// The frame that contains `time` (floor).
    pub fn time_to_frame(self, time: Time) -> i64 {
        let n = time.0 as i128 * self.num as i128;
        let d = Time::FLICKS_PER_SECOND as i128 * self.den as i128;
        n.div_euclid(d) as i64
    }

    /// `time` snapped down to the start of its frame.
    pub fn snap(self, time: Time) -> Time {
        self.frame_to_time(self.time_to_frame(time))
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{} fps", self.num)
        } else {
            write!(f, "{:.3} fps", self.as_f64())
        }
    }
}

/// A half-open time range `[start, end)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TimeRange {
    pub start: Time,
    pub end: Time,
}

impl TimeRange {
    /// All time. Used when a change affects every frame (static values, expressions).
    pub const ALL: TimeRange = TimeRange {
        start: Time::MIN,
        end: Time::MAX,
    };

    pub const fn new(start: Time, end: Time) -> Self {
        TimeRange { start, end }
    }

    pub fn is_all(&self) -> bool {
        *self == Self::ALL
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn duration(&self) -> Time {
        self.end.saturating_sub(self.start)
    }

    pub fn contains(&self, t: Time) -> bool {
        self.start <= t && t < self.end
    }

    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Smallest range covering both.
    pub fn union(&self, other: &TimeRange) -> TimeRange {
        TimeRange::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_rates_are_exact() {
        for rate in [
            FrameRate::FPS_23_976,
            FrameRate::FPS_24,
            FrameRate::FPS_25,
            FrameRate::FPS_29_97,
            FrameRate::FPS_30,
            FrameRate::FPS_50,
            FrameRate::FPS_59_94,
            FrameRate::FPS_60,
            FrameRate::FPS_120,
        ] {
            assert!(rate.is_exact(), "{rate}");
            for frame in [-7_i64, 0, 1, 29, 1_000_003] {
                let t = rate.frame_to_time(frame);
                assert_eq!(rate.time_to_frame(t), frame, "{rate} frame {frame}");
                assert_eq!(t, Time(rate.frame_duration().0 * frame));
            }
        }
    }

    #[test]
    fn floor_and_snap() {
        let r = FrameRate::FPS_30;
        let half = Time(r.frame_duration().0 / 2);
        assert_eq!(r.time_to_frame(half), 0);
        assert_eq!(r.time_to_frame(-half), -1);
        assert_eq!(r.snap(r.frame_to_time(5) + half), r.frame_to_time(5));
        assert_eq!(Time::from_seconds(1.5).flicks(), 1_058_400_000);
    }

    #[test]
    fn ranges() {
        let a = TimeRange::new(Time(0), Time(10));
        let b = TimeRange::new(Time(10), Time(20));
        assert!(!a.overlaps(&b));
        assert!(a.contains(Time(9)) && !a.contains(Time(10)));
        assert_eq!(a.union(&b), TimeRange::new(Time(0), Time(20)));
        assert!(TimeRange::ALL.overlaps(&a));
    }
}
