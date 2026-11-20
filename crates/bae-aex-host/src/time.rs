//! Time translation between BAE flicks and the AE `(value, scale)` pairs.
//!
//! BAE stores all time as integer **flicks** (705,600,000 per second,
//! `docs/ARCHITECTURE.md` §4.4). The AE effect protocol expresses time as a
//! signed 32-bit tick count plus an unsigned ticks-per-second scale:
//! `PF_InData::current_time`, `time_step`, `local_time_step`, `total_time`
//! all share `PF_InData::time_scale`; SmartFX layer checkouts carry their own
//! `(what_time, time_step, time_scale)` triple (SDK guide, "Effect Basics ›
//! PF_InData"; "SmartFX › PF_PreRenderExtra").
//!
//! Constraints that drive the design:
//!
//! * The tick value is 32-bit, so a scale of 705,600,000 (flicks themselves)
//!   would overflow after ~3 s. The host therefore picks a per-instance
//!   [`AeTimeBase`] derived from the comp frame rate.
//! * Every frame boundary of every common rate is an integer number of flicks,
//!   and with the time base below it is also an integer number of AE ticks, so
//!   frame times round-trip exactly.
//! * Sub-frame times (motion-blur samples, time remapping, stretched layers)
//!   use `subdivisions` extra ticks per frame; anything finer is rounded to
//!   the nearest tick and the rounding is reported.
//! * Plug-ins may ask for times at their own scale (checkout at other times);
//!   [`flicks_from_ae`] converts any `(value, scale)` back to flicks, rounding
//!   to the nearest flick (≈1.4 ns).

use crate::error::{HostError, Result};

/// Flicks per second (BAE time unit).
pub const FLICKS_PER_SECOND: i64 = 705_600_000;

/// A BAE time or duration in flicks. (Local mirror; the canonical type is
/// owned by `bae-model`. P3 replaces this with a `From` impl.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Flicks(pub i64);

/// A frame rate as an exact rational `num / den` frames per second
/// (e.g. 29.97 = 30000/1001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameRate {
    /// Numerator (> 0).
    pub num: u32,
    /// Denominator (> 0).
    pub den: u32,
}

/// An AE-style time value: `value / scale` seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AeTime {
    /// Signed tick count (`A_long` in the protocol).
    pub value: i32,
    /// Ticks per second (`A_u_long`, > 0).
    pub scale: u32,
}

/// Whether a conversion was exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exactness {
    /// The result represents the input exactly.
    Exact,
    /// The result was rounded to the nearest representable tick.
    Rounded,
}

/// Per-effect-instance time base: the `time_scale` the host reports to a
/// plug-in in `PF_InData`, chosen once per (instance, comp frame rate) and kept
/// stable, because some plug-ins cache time-derived values in sequence data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AeTimeBase {
    /// Ticks per second reported as `time_scale`.
    pub scale: u32,
    /// Ticks per frame at the comp frame rate (`local_time_step` for an
    /// unstretched layer).
    pub frame_step: u32,
}

const fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Divide rounding to nearest, ties toward +infinity. `d` must be > 0.
fn div_round(n: i128, d: i128) -> i128 {
    debug_assert!(d > 0);
    (2 * n + d).div_euclid(2 * d)
}

impl AeTimeBase {
    /// Choose a time base for a comp at `rate` with `subdivisions` ticks per
    /// frame (1 = frame-accurate only; use e.g. 16 or 64 to make motion-blur
    /// sub-samples exact).
    ///
    /// `scale = (num / g) * subdivisions`, `frame_step = (den / g) * subdivisions`
    /// with `g = gcd(num, den)`, which makes every frame boundary an integer
    /// tick count. Fails if the result does not fit the 32-bit protocol fields
    /// or if `max_duration` (the longest time the instance can be asked about,
    /// typically the comp duration plus margin) would overflow `i32` ticks.
    pub fn for_frame_rate(
        rate: FrameRate,
        subdivisions: u32,
        max_duration: Flicks,
    ) -> Result<AeTimeBase> {
        if rate.num == 0 || rate.den == 0 || subdivisions == 0 {
            return Err(HostError::OutOfRange {
                what: "frame rate / subdivisions",
            });
        }
        let g = gcd(rate.num as u64, rate.den as u64);
        let scale = (rate.num as u64 / g) * subdivisions as u64;
        let step = (rate.den as u64 / g) * subdivisions as u64;
        if scale > u32::MAX as u64 || step > u32::MAX as u64 {
            return Err(HostError::OutOfRange { what: "time_scale" });
        }
        let base = AeTimeBase {
            scale: scale as u32,
            frame_step: step as u32,
        };
        let max_ticks = div_round(
            max_duration.0.unsigned_abs() as i128 * scale as i128,
            FLICKS_PER_SECOND as i128,
        );
        if max_ticks > i32::MAX as i128 {
            return Err(HostError::OutOfRange {
                what: "duration in ticks",
            });
        }
        Ok(base)
    }

    /// Like [`AeTimeBase::for_frame_rate`] but picks the largest power-of-two
    /// subdivision `<= max_subdivisions` that still fits `max_duration`.
    pub fn best_for(
        rate: FrameRate,
        max_subdivisions: u32,
        max_duration: Flicks,
    ) -> Result<AeTimeBase> {
        let mut sub = max_subdivisions.max(1).next_power_of_two();
        if sub > max_subdivisions.max(1) {
            sub /= 2;
        }
        loop {
            match AeTimeBase::for_frame_rate(rate, sub, max_duration) {
                Ok(b) => return Ok(b),
                Err(e) if sub == 1 => return Err(e),
                Err(_) => sub /= 2,
            }
        }
    }

    /// Convert a BAE time (or a signed duration) to ticks of this base.
    pub fn to_ae(&self, t: Flicks) -> Result<(AeTime, Exactness)> {
        let num = t.0 as i128 * self.scale as i128;
        let den = FLICKS_PER_SECOND as i128;
        let exact = num % den == 0;
        let v = div_round(num, den);
        if v < i32::MIN as i128 || v > i32::MAX as i128 {
            return Err(HostError::OutOfRange { what: "AE time" });
        }
        Ok((
            AeTime {
                value: v as i32,
                scale: self.scale,
            },
            if exact {
                Exactness::Exact
            } else {
                Exactness::Rounded
            },
        ))
    }

    /// Strict variant of [`AeTimeBase::to_ae`] that refuses to round.
    pub fn to_ae_exact(&self, t: Flicks) -> Result<AeTime> {
        match self.to_ae(t)? {
            (v, Exactness::Exact) => Ok(v),
            (_, Exactness::Rounded) => Err(HostError::Inexact { what: "AE time" }),
        }
    }
}

/// Convert any AE `(value, scale)` back to flicks, rounding to the nearest
/// flick. Returns the exactness of the conversion.
pub fn flicks_from_ae(t: AeTime) -> Result<(Flicks, Exactness)> {
    if t.scale == 0 {
        return Err(HostError::OutOfRange { what: "time_scale" });
    }
    let num = t.value as i128 * FLICKS_PER_SECOND as i128;
    let den = t.scale as i128;
    let exact = num % den == 0;
    Ok((
        Flicks(div_round(num, den) as i64),
        if exact {
            Exactness::Exact
        } else {
            Exactness::Rounded
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [FrameRate; 9] = [
        FrameRate {
            num: 24000,
            den: 1001,
        },
        FrameRate { num: 24, den: 1 },
        FrameRate { num: 25, den: 1 },
        FrameRate {
            num: 30000,
            den: 1001,
        },
        FrameRate { num: 30, den: 1 },
        FrameRate { num: 50, den: 1 },
        FrameRate {
            num: 60000,
            den: 1001,
        },
        FrameRate { num: 60, den: 1 },
        FrameRate { num: 120, den: 1 },
    ];

    fn frame_flicks(rate: FrameRate, n: i64) -> Flicks {
        // Every common rate divides flicks exactly (that is the point of flicks).
        let num = n as i128 * FLICKS_PER_SECOND as i128 * rate.den as i128;
        assert_eq!(num % rate.num as i128, 0);
        Flicks((num / rate.num as i128) as i64)
    }

    #[test]
    fn frames_roundtrip_exactly_for_common_rates() {
        let three_hours = Flicks(3 * 3600 * FLICKS_PER_SECOND);
        for rate in RATES {
            let base = AeTimeBase::for_frame_rate(rate, 1, three_hours).unwrap();
            for n in [-5i64, 0, 1, 2, 1000, 323_999] {
                let f = frame_flicks(rate, n);
                let ae = base.to_ae_exact(f).unwrap();
                assert_eq!(
                    ae.value as i64,
                    n * base.frame_step as i64,
                    "{rate:?} frame {n}"
                );
                let (back, ex) = flicks_from_ae(ae).unwrap();
                assert_eq!(back, f);
                assert_eq!(ex, Exactness::Exact);
            }
        }
    }

    #[test]
    fn subframes_and_rounding() {
        let rate = FrameRate {
            num: 30000,
            den: 1001,
        };
        let base = AeTimeBase::for_frame_rate(rate, 16, Flicks(60 * FLICKS_PER_SECOND)).unwrap();
        assert_eq!(base.scale, 480_000);
        assert_eq!(base.frame_step, 16_016);
        // Half a frame is exact with 16 subdivisions.
        let half = Flicks(frame_flicks(rate, 1).0 / 2);
        assert_eq!(base.to_ae(half).unwrap().1, Exactness::Exact);
        // One flick is not representable: rounds to 0 ticks.
        let (v, ex) = base.to_ae(Flicks(1)).unwrap();
        assert_eq!((v.value, ex), (0, Exactness::Rounded));
        assert!(base.to_ae_exact(Flicks(1)).is_err());
    }

    #[test]
    fn best_for_shrinks_subdivisions_to_fit() {
        let rate = FrameRate {
            num: 30000,
            den: 1001,
        };
        let three_hours = Flicks(3 * 3600 * FLICKS_PER_SECOND);
        // 64 subdivisions at 30000 ticks/s overflows i32 within 3 h; 4 fits.
        let base = AeTimeBase::best_for(rate, 64, three_hours).unwrap();
        assert!(base.scale as u64 * 3 * 3600 <= i32::MAX as u64);
        assert!(AeTimeBase::for_frame_rate(rate, 64, three_hours).is_err());
        assert_eq!(base.scale, 30000 * 4);
    }

    #[test]
    fn negative_times_and_foreign_scales() {
        let (f, ex) = flicks_from_ae(AeTime {
            value: -3,
            scale: 3,
        })
        .unwrap();
        assert_eq!((f, ex), (Flicks(-FLICKS_PER_SECOND), Exactness::Exact));
        let (f, ex) = flicks_from_ae(AeTime { value: 1, scale: 7 }).unwrap();
        assert_eq!(ex, Exactness::Exact); // 705.6M is divisible by 7
        assert_eq!(f.0, 100_800_000);
        let (_, ex) = flicks_from_ae(AeTime {
            value: 1,
            scale: 11,
        })
        .unwrap();
        assert_eq!(ex, Exactness::Rounded);
        assert!(flicks_from_ae(AeTime { value: 1, scale: 0 }).is_err());
    }
}
