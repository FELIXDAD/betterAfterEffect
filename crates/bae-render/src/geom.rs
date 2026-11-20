//! Pixel rectangles and the affine helpers the render graph uses for bounds and
//! ROI (region of interest) negotiation.
//!
//! Conventions:
//! * Pixel `(x, y)` covers the square `[x, x + 1) × [y, y + 1)`; its centre is at
//!   `(x + 0.5, y + 0.5)`. This matches AE: a 100×100 solid at anchor (50, 50) and
//!   position (50, 50) covers pixels 0..100 exactly.
//! * The y axis points down (AE comp space, texture space).

use glam::{DAffine2, DVec2};

/// A floating-point rectangle `[x0, x1) × [y0, y1)` (layer or comp space).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge (exclusive).
    pub x1: f64,
    /// Bottom edge (exclusive).
    pub y1: f64,
}

impl Rect {
    /// A rectangle from its edges.
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// `[0, w) × [0, h)`.
    pub fn from_size(width: f64, height: f64) -> Self {
        Self::new(0.0, 0.0, width, height)
    }

    /// Width (0 when empty).
    pub fn width(&self) -> f64 {
        (self.x1 - self.x0).max(0.0)
    }

    /// Height (0 when empty).
    pub fn height(&self) -> f64 {
        (self.y1 - self.y0).max(0.0)
    }

    /// True when the rectangle covers no area.
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    /// The four corners, clockwise from top-left.
    pub fn corners(&self) -> [DVec2; 4] {
        [
            DVec2::new(self.x0, self.y0),
            DVec2::new(self.x1, self.y0),
            DVec2::new(self.x1, self.y1),
            DVec2::new(self.x0, self.y1),
        ]
    }

    /// Axis-aligned bounds of this rectangle after `transform`.
    pub fn transformed_bounds(&self, transform: &DAffine2) -> Rect {
        let mut out = Rect::new(
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for corner in self.corners() {
            let p = transform.transform_point2(corner);
            out.x0 = out.x0.min(p.x);
            out.y0 = out.y0.min(p.y);
            out.x1 = out.x1.max(p.x);
            out.y1 = out.y1.max(p.y);
        }
        out
    }

    /// Smallest pixel rectangle that contains this one.
    pub fn round_out(&self) -> IRect {
        if self.is_empty() || !self.x0.is_finite() || !self.y0.is_finite() {
            return IRect::EMPTY;
        }
        let clamp = |v: f64| v.clamp(i32::MIN as f64 / 2.0, i32::MAX as f64 / 2.0);
        IRect::new(
            clamp(self.x0.floor()) as i32,
            clamp(self.y0.floor()) as i32,
            clamp(self.x1.ceil()) as i32,
            clamp(self.y1.ceil()) as i32,
        )
    }
}

/// A pixel rectangle `[x0, x1) × [y0, y1)` in integer pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct IRect {
    /// Left edge.
    pub x0: i32,
    /// Top edge.
    pub y0: i32,
    /// Right edge (exclusive).
    pub x1: i32,
    /// Bottom edge (exclusive).
    pub y1: i32,
}

impl IRect {
    /// The canonical empty rectangle.
    pub const EMPTY: IRect = IRect::new(0, 0, 0, 0);

    /// A rectangle from its edges.
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// `[0, w) × [0, h)`.
    pub const fn from_size(width: u32, height: u32) -> Self {
        Self::new(0, 0, width as i32, height as i32)
    }

    /// Width in pixels (0 when empty).
    pub fn width(&self) -> u32 {
        (self.x1 - self.x0).max(0) as u32
    }

    /// Height in pixels (0 when empty).
    pub fn height(&self) -> u32 {
        (self.y1 - self.y0).max(0) as u32
    }

    /// `[width, height]`.
    pub fn size(&self) -> [u32; 2] {
        [self.width(), self.height()]
    }

    /// Number of pixels.
    pub fn area(&self) -> u64 {
        u64::from(self.width()) * u64::from(self.height())
    }

    /// True when the rectangle covers no pixel.
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    /// Intersection (empty rectangles normalise to [`IRect::EMPTY`]).
    pub fn intersect(&self, other: &IRect) -> IRect {
        let r = IRect::new(
            self.x0.max(other.x0),
            self.y0.max(other.y0),
            self.x1.min(other.x1),
            self.y1.min(other.y1),
        );
        if r.is_empty() { IRect::EMPTY } else { r }
    }

    /// Smallest rectangle containing both (an empty side is ignored).
    pub fn union(&self, other: &IRect) -> IRect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        IRect::new(
            self.x0.min(other.x0),
            self.y0.min(other.y0),
            self.x1.max(other.x1),
            self.y1.max(other.y1),
        )
    }

    /// True when `other` lies completely inside `self` (an empty `other` always does).
    pub fn contains(&self, other: &IRect) -> bool {
        other.is_empty()
            || (other.x0 >= self.x0
                && other.y0 >= self.y0
                && other.x1 <= self.x1
                && other.y1 <= self.y1)
    }

    /// Grows every edge outwards by `n` pixels (shrinks for negative `n`).
    pub fn inflate(&self, n: i32) -> IRect {
        if self.is_empty() {
            return IRect::EMPTY;
        }
        let r = IRect::new(self.x0 - n, self.y0 - n, self.x1 + n, self.y1 + n);
        if r.is_empty() { IRect::EMPTY } else { r }
    }

    /// As a floating-point rectangle.
    pub fn to_rect(&self) -> Rect {
        Rect::new(
            f64::from(self.x0),
            f64::from(self.y0),
            f64::from(self.x1),
            f64::from(self.y1),
        )
    }
}

/// Uniform scale used for reduced preview resolution (1/2, 1/3, 1/4 …).
pub fn scale_affine(scale: f64) -> DAffine2 {
    DAffine2::from_scale(DVec2::splat(scale))
}

/// Axis scale factors of `transform`: how many output units one input unit along
/// x and along y covers. Used to size anti-aliasing margins.
pub fn axis_scales(transform: &DAffine2) -> DVec2 {
    DVec2::new(
        transform.matrix2.x_axis.length(),
        transform.matrix2.y_axis.length(),
    )
}

/// True when `transform` is invertible with a usable determinant.
pub fn is_invertible(transform: &DAffine2) -> bool {
    let det = transform.matrix2.determinant();
    det.is_finite() && det.abs() > 1e-12
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_ops() {
        let a = IRect::new(0, 0, 10, 10);
        let b = IRect::new(5, 5, 20, 20);
        assert_eq!(a.intersect(&b), IRect::new(5, 5, 10, 10));
        assert_eq!(a.union(&b), IRect::new(0, 0, 20, 20));
        assert!(a.intersect(&IRect::new(10, 0, 20, 10)).is_empty());
        assert_eq!(IRect::EMPTY.union(&a), a);
        assert!(a.contains(&IRect::new(1, 1, 9, 9)));
        assert!(!a.contains(&b));
        assert_eq!(a.inflate(2), IRect::new(-2, -2, 12, 12));
        assert_eq!(a.inflate(-5), IRect::EMPTY);
    }

    #[test]
    fn transformed_bounds_round_out() {
        let t =
            DAffine2::from_angle_translation(std::f64::consts::FRAC_PI_4, DVec2::new(50.0, 50.0));
        let r = Rect::from_size(10.0, 10.0)
            .transformed_bounds(&t)
            .round_out();
        // A 10×10 square rotated by 45° spans x = 50 ± 7.07, y = 50 … 64.14.
        assert_eq!(r, IRect::new(42, 50, 58, 65));
        assert_eq!(
            Rect::new(0.5, 0.5, 9.5, 9.5).round_out(),
            IRect::new(0, 0, 10, 10)
        );
    }
}
