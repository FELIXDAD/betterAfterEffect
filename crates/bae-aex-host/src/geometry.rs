//! Rectangles, ratios and 16.16 fixed point as the AE protocol uses them.
//!
//! * Rectangles in the AE effect API are integer, half-open
//!   (`left <= x < right`, `top <= y < bottom`) and expressed in the
//!   coordinate space of the layer the effect is applied to (SDK guide,
//!   "SmartFX › PF_PreRenderExtra": request rects are in layer coordinates).
//! * Downsample factors and pixel aspect ratios are rationals
//!   (`PF_RationalScale`: signed numerator, unsigned denominator).
//! * Many legacy parameter values (fixed sliders, angles, points) are 16.16
//!   fixed point (`PF_Fixed`).
//!
//! BAE's render graph (04) works in `f32` pixel space with explicit ROI
//! rectangles; the conversions below are the translation contract.

use crate::error::{HostError, Result};

/// Integer half-open rectangle in layer pixel coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LRect {
    /// Inclusive left edge.
    pub left: i32,
    /// Inclusive top edge.
    pub top: i32,
    /// Exclusive right edge.
    pub right: i32,
    /// Exclusive bottom edge.
    pub bottom: i32,
}

impl LRect {
    /// The empty rectangle. SmartFX uses an empty request rect to ask an
    /// effect only for its `max_result_rect`.
    pub const EMPTY: LRect = LRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    /// Construct from origin and size.
    pub const fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> LRect {
        LRect {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
    }

    /// Width (0 when empty or inverted).
    pub fn width(&self) -> u32 {
        (self.right as i64 - self.left as i64).max(0) as u32
    }

    /// Height (0 when empty or inverted).
    pub fn height(&self) -> u32 {
        (self.bottom as i64 - self.top as i64).max(0) as u32
    }

    /// True when the rectangle covers no pixels.
    pub fn is_empty(&self) -> bool {
        self.width() == 0 || self.height() == 0
    }

    /// Intersection; returns [`LRect::EMPTY`] when disjoint.
    pub fn intersect(&self, other: &LRect) -> LRect {
        let r = LRect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        if r.is_empty() { LRect::EMPTY } else { r }
    }

    /// Smallest rectangle containing both (empty inputs are ignored).
    pub fn union(&self, other: &LRect) -> LRect {
        match (self.is_empty(), other.is_empty()) {
            (true, true) => LRect::EMPTY,
            (true, false) => *other,
            (false, true) => *self,
            (false, false) => LRect {
                left: self.left.min(other.left),
                top: self.top.min(other.top),
                right: self.right.max(other.right),
                bottom: self.bottom.max(other.bottom),
            },
        }
    }

    /// True when `inner` lies completely inside `self` (an empty `inner` is
    /// always contained).
    pub fn contains(&self, inner: &LRect) -> bool {
        inner.is_empty()
            || (inner.left >= self.left
                && inner.top >= self.top
                && inner.right <= self.right
                && inner.bottom <= self.bottom)
    }

    /// Translate by (dx, dy).
    pub fn offset(&self, dx: i32, dy: i32) -> LRect {
        LRect {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    /// Grow on every side by `n` pixels (negative shrinks).
    pub fn inflate(&self, n: i32) -> LRect {
        LRect {
            left: self.left - n,
            top: self.top - n,
            right: self.right + n,
            bottom: self.bottom + n,
        }
    }

    /// Number of pixels covered.
    pub fn area(&self) -> u64 {
        self.width() as u64 * self.height() as u64
    }
}

/// Rational scale (`num / den`), used for downsample factors and pixel aspect
/// ratio. `den` is never zero for a valid value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RationalScale {
    /// Numerator.
    pub num: i32,
    /// Denominator (> 0).
    pub den: u32,
}

impl RationalScale {
    /// 1/1.
    pub const ONE: RationalScale = RationalScale { num: 1, den: 1 };

    /// Construct, rejecting a zero denominator.
    pub fn new(num: i32, den: u32) -> Result<RationalScale> {
        if den == 0 {
            return Err(HostError::OutOfRange {
                what: "rational denominator",
            });
        }
        Ok(RationalScale { num, den })
    }

    /// Downsample factor for BAE preview resolution "1/n" (Full, Half, Third,
    /// Quarter...). AE expresses "render every nth pixel" as `1/n`.
    pub fn downsample(n: u32) -> Result<RationalScale> {
        RationalScale::new(1, n)
    }

    /// Value as `f64`.
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

/// 16.16 signed fixed point (`PF_Fixed`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fixed(pub i32);

impl Fixed {
    /// One unit (1.0).
    pub const ONE: Fixed = Fixed(1 << 16);

    /// Convert from `f64`, rounding to nearest representable value.
    /// Returns an error if the value does not fit in 16.16.
    pub fn from_f64(v: f64) -> Result<Fixed> {
        let scaled = (v * 65536.0).round();
        if !scaled.is_finite() || scaled < i32::MIN as f64 || scaled > i32::MAX as f64 {
            return Err(HostError::OutOfRange {
                what: "16.16 fixed",
            });
        }
        Ok(Fixed(scaled as i32))
    }

    /// Convert from `f64`, saturating at the representable range (NaN → 0).
    pub fn from_f64_saturating(v: f64) -> Fixed {
        if v.is_nan() {
            return Fixed(0);
        }
        let scaled = (v * 65536.0).round();
        Fixed(scaled.clamp(i32::MIN as f64, i32::MAX as f64) as i32)
    }

    /// Value as `f64` (exact: every 16.16 value is representable).
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 65536.0
    }

    /// Integer part, rounding toward negative infinity (what legacy plug-ins
    /// get by dropping the low 16 bits).
    pub fn floor_int(self) -> i32 {
        self.0 >> 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_ops() {
        let a = LRect::from_xywh(0, 0, 10, 10);
        let b = LRect::from_xywh(5, 5, 10, 10);
        assert_eq!(a.intersect(&b), LRect::from_xywh(5, 5, 5, 5));
        assert_eq!(a.union(&b), LRect::from_xywh(0, 0, 15, 15));
        assert!(a.intersect(&LRect::from_xywh(20, 20, 1, 1)).is_empty());
        assert_eq!(LRect::EMPTY.union(&a), a);
        assert!(a.contains(&LRect::EMPTY));
        assert!(!a.contains(&b));
        assert_eq!(a.inflate(2), LRect::from_xywh(-2, -2, 14, 14));
        assert_eq!(a.area(), 100);
        let inverted = LRect {
            left: 5,
            top: 0,
            right: 0,
            bottom: 5,
        };
        assert!(inverted.is_empty());
    }

    #[test]
    fn fixed_roundtrip() {
        for v in [0.0, 1.0, -1.0, 0.5, 123.456, -32768.0, 32767.99998] {
            let f = Fixed::from_f64(v).unwrap();
            assert!((f.to_f64() - v).abs() <= 1.0 / 65536.0, "{v}");
        }
        assert_eq!(Fixed::from_f64(1.0).unwrap(), Fixed::ONE);
        assert!(Fixed::from_f64(40000.0).is_err());
        assert_eq!(Fixed::from_f64_saturating(f64::NAN), Fixed(0));
        assert_eq!(Fixed::from_f64_saturating(1e12), Fixed(i32::MAX));
        assert_eq!(Fixed::from_f64(-0.5).unwrap().floor_int(), -1);
    }

    #[test]
    fn rational() {
        assert!(RationalScale::new(1, 0).is_err());
        assert_eq!(RationalScale::downsample(4).unwrap().as_f64(), 0.25);
    }
}
