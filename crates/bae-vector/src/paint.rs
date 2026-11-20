//! Paints: fill and stroke settings, colours and gradients (AE-SHP-016..028).
//!
//! Colours are straight (unpremultiplied) RGBA in the project working space,
//! clamped to `[0, 1]` like AE does for shape paints (AE-SHP-016).

use std::hash::{Hash, Hasher};

use kurbo::Point;

/// Straight RGBA, each channel in `[0, 1]`.
pub type Rgba = [f32; 4];

/// Clamps a project colour (`[f64; 4]`, may exceed 0–1 in 32-bit projects) to a
/// shape paint colour.
pub fn paint_color(c: [f64; 4]) -> Rgba {
    c.map(|v| {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(0.0, 1.0) as f32
        }
    })
}

/// Non-zero or even-odd (AE-SHP-017).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl FillRule {
    /// From the project token (`non_zero` / `even_odd`).
    pub fn from_token(token: &str) -> Self {
        match token {
            "even_odd" => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        }
    }
}

/// Line cap (AE-SHP-019).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    /// Square cap extending half the width beyond the end.
    Projecting,
}

impl LineCap {
    /// From the project token (`butt` / `round` / `projecting`).
    pub fn from_token(token: &str) -> Self {
        match token {
            "round" => LineCap::Round,
            "projecting" => LineCap::Projecting,
            _ => LineCap::Butt,
        }
    }
}

/// Line join (AE-SHP-019).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl LineJoin {
    /// From the project token (`miter` / `round` / `bevel`).
    pub fn from_token(token: &str) -> Self {
        match token {
            "round" => LineJoin::Round,
            "bevel" => LineJoin::Bevel,
            _ => LineJoin::Miter,
        }
    }
}

/// Stroke geometry settings (AE-SHP-018..020). Width and dashes are in the
/// stroke's group space; enclosing transforms scale them.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Miter length / width ratio above which a miter becomes a bevel.
    pub miter_limit: f64,
    /// Dash, gap, dash, gap … in pixels; empty = solid.
    pub dashes: Vec<f64>,
    /// Dash offset in pixels (SVG `stroke-dashoffset` semantics).
    pub dash_offset: f64,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 2.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dashes: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl StrokeStyle {
    /// The dash pattern that is actually applied: empty when solid (no entries,
    /// a negative or non-finite entry, or all zero); odd patterns are repeated to
    /// make them even (SVG semantics, AE-SHP-020).
    pub fn effective_dashes(&self) -> Vec<f64> {
        let d = &self.dashes;
        if d.is_empty()
            || d.iter().any(|v| !v.is_finite() || *v < 0.0)
            || d.iter().all(|v| *v == 0.0)
        {
            return Vec::new();
        }
        if d.len() % 2 == 1 {
            d.iter().chain(d.iter()).copied().collect()
        } else {
            d.clone()
        }
    }

    /// Half the width of the stroke outline beyond the path in the worst case
    /// (miter joins and projecting caps), for bounds.
    pub fn outset(&self) -> f64 {
        let half = self.width.max(0.0) / 2.0;
        let join = match self.join {
            LineJoin::Miter => self.miter_limit.max(1.0),
            _ => 1.0,
        };
        let cap = match self.cap {
            LineCap::Projecting => std::f64::consts::SQRT_2,
            _ => 1.0,
        };
        half * join.max(cap)
    }

    /// kurbo stroke settings (used for outline generation, hit testing and by the
    /// Vello encoder).
    pub fn to_kurbo(&self) -> kurbo::Stroke {
        let cap = match self.cap {
            LineCap::Butt => kurbo::Cap::Butt,
            LineCap::Round => kurbo::Cap::Round,
            LineCap::Projecting => kurbo::Cap::Square,
        };
        let join = match self.join {
            LineJoin::Miter => kurbo::Join::Miter,
            LineJoin::Round => kurbo::Join::Round,
            LineJoin::Bevel => kurbo::Join::Bevel,
        };
        kurbo::Stroke::new(self.width.max(0.0))
            .with_caps(cap)
            .with_join(join)
            .with_miter_limit(self.miter_limit.max(1.0))
            .with_dashes(self.dash_offset, self.effective_dashes())
    }
}

/// One stop of a combined colour + opacity ramp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position in `[0, 1]`.
    pub offset: f32,
    pub color: Rgba,
}

/// Linear or radial (AE-SHP-027/028).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    Linear,
    Radial {
        /// Percent, −100..100: moves the focal point towards the end point.
        highlight_length: f64,
        /// Degrees, relative to the start → end direction, clockwise.
        highlight_angle: f64,
    },
}

/// A gradient in the paint's group space.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub start: Point,
    pub end: Point,
    /// Sorted by offset; colours beyond the ends extend (pad).
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// Combines AE's flat gradient data — `[pos, r, g, b]` × `color_stops`, then
    /// `[pos, alpha]` per opacity stop (AE-SHP-025, the `colors` attribute) — into
    /// one ramp with a stop at every colour and opacity stop position. Without
    /// data the ramp is white → black.
    pub fn stops_from_ae(colors: &[f64], color_stops: usize) -> Vec<GradientStop> {
        let n_color = color_stops.min(colors.len() / 4);
        let color: Vec<(f64, [f64; 3])> = (0..n_color)
            .map(|i| {
                let c = &colors[i * 4..i * 4 + 4];
                (c[0], [c[1], c[2], c[3]])
            })
            .collect();
        let rest = &colors[n_color * 4..];
        let alpha: Vec<(f64, f64)> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| (c[0], c[1]))
            .collect();
        if color.is_empty() {
            return vec![
                GradientStop {
                    offset: 0.0,
                    color: [1.0; 4],
                },
                GradientStop {
                    offset: 1.0,
                    color: [0.0, 0.0, 0.0, 1.0],
                },
            ];
        }
        let mut positions: Vec<f64> = color
            .iter()
            .map(|c| c.0)
            .chain(alpha.iter().map(|a| a.0))
            .map(|p| p.clamp(0.0, 1.0))
            .collect();
        positions.sort_by(f64::total_cmp);
        positions.dedup();
        positions
            .into_iter()
            .map(|t| {
                let rgb = ramp(&color, t, |a, b, u| {
                    [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * u)
                });
                let a = if alpha.is_empty() {
                    1.0
                } else {
                    ramp(&alpha, t, |a, b, u| a + (b - a) * u)
                };
                GradientStop {
                    offset: t as f32,
                    color: paint_color([rgb[0], rgb[1], rgb[2], a]),
                }
            })
            .collect()
    }
}

/// Piecewise-linear ramp lookup with flat ends (stops sorted by position).
fn ramp<T: Copy>(stops: &[(f64, T)], t: f64, lerp: impl Fn(T, T, f64) -> T) -> T {
    let mut sorted: Vec<(f64, T)> = stops.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    if t <= sorted[0].0 {
        return sorted[0].1;
    }
    for w in sorted.windows(2) {
        let (p0, v0) = w[0];
        let (p1, v1) = w[1];
        if t <= p1 {
            let u = if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 };
            return lerp(v0, v1, u);
        }
    }
    sorted[sorted.len() - 1].1
}

/// What a paint draws with.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    Solid(Rgba),
    Gradient(Gradient),
}

/// Blend modes available to shape groups and paints (AE-SHP-005: the Lottie
/// subset that Vello implements). Other AE layer modes fall back to normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// From a project blend-mode token (`bae_model::schema::BLEND_MODES`).
    /// `None` for modes shapes cannot draw yet.
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "normal" => BlendMode::Normal,
            "multiply" => BlendMode::Multiply,
            "screen" => BlendMode::Screen,
            "overlay" => BlendMode::Overlay,
            "darken" => BlendMode::Darken,
            "lighten" => BlendMode::Lighten,
            "color_dodge" => BlendMode::ColorDodge,
            "color_burn" => BlendMode::ColorBurn,
            "hard_light" => BlendMode::HardLight,
            "soft_light" => BlendMode::SoftLight,
            "difference" => BlendMode::Difference,
            "exclusion" => BlendMode::Exclusion,
            "hue" => BlendMode::Hue,
            "saturation" => BlendMode::Saturation,
            "color" => BlendMode::Color,
            "luminosity" => BlendMode::Luminosity,
            _ => return None,
        })
    }
}

/// Where a paint goes relative to the paint above it (AE-SHP-004).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Composite {
    /// Below previous in same group (default).
    #[default]
    Below,
    /// Above previous in same group.
    Above,
}

impl Composite {
    /// From the project token (`below` / `above`).
    pub fn from_token(token: &str) -> Self {
        match token {
            "above" => Composite::Above,
            _ => Composite::Below,
        }
    }
}

/// Fill or stroke.
#[derive(Clone, Debug, PartialEq)]
pub enum PaintKind {
    Fill(FillRule),
    Stroke(StrokeStyle),
}

/// A paint item of a shape group: Fill, Stroke, Gradient Fill or Gradient Stroke.
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub kind: PaintKind,
    pub brush: Brush,
    /// `[0, 1]`.
    pub opacity: f64,
    pub blend: BlendMode,
    pub composite: Composite,
}

pub(crate) fn hash_f64(v: f64, h: &mut impl Hasher) {
    v.to_bits().hash(h);
}

impl Brush {
    pub(crate) fn hash_into(&self, h: &mut impl Hasher) {
        match self {
            Brush::Solid(c) => {
                0u8.hash(h);
                c.map(f32::to_bits).hash(h);
            }
            Brush::Gradient(g) => {
                1u8.hash(h);
                match g.kind {
                    GradientKind::Linear => 0u8.hash(h),
                    GradientKind::Radial {
                        highlight_length,
                        highlight_angle,
                    } => {
                        1u8.hash(h);
                        hash_f64(highlight_length, h);
                        hash_f64(highlight_angle, h);
                    }
                }
                for p in [g.start, g.end] {
                    hash_f64(p.x, h);
                    hash_f64(p.y, h);
                }
                for s in &g.stops {
                    s.offset.to_bits().hash(h);
                    s.color.map(f32::to_bits).hash(h);
                }
            }
        }
    }
}

impl StrokeStyle {
    pub(crate) fn hash_into(&self, h: &mut impl Hasher) {
        hash_f64(self.width, h);
        self.cap.hash(h);
        self.join.hash(h);
        hash_f64(self.miter_limit, h);
        self.dashes.len().hash(h);
        for d in &self.dashes {
            hash_f64(*d, h);
        }
        hash_f64(self.dash_offset, h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_clamp_to_unit_range() {
        assert_eq!(
            paint_color([1.5, -0.2, 0.5, f64::NAN]),
            [1.0, 0.0, 0.5, 0.0]
        );
    }

    #[test]
    fn dash_patterns() {
        let mut s = StrokeStyle {
            dashes: vec![10.0],
            ..StrokeStyle::default()
        };
        assert_eq!(s.effective_dashes(), vec![10.0, 10.0]);
        s.dashes = vec![0.0, 0.0];
        assert!(s.effective_dashes().is_empty());
        s.dashes = vec![5.0, -1.0];
        assert!(s.effective_dashes().is_empty());
        s.dashes = vec![0.0, 4.0];
        assert_eq!(s.effective_dashes(), vec![0.0, 4.0]);
    }

    #[test]
    fn stroke_outset_covers_miters_and_caps() {
        let s = StrokeStyle {
            width: 10.0,
            ..StrokeStyle::default()
        };
        assert_eq!(s.outset(), 20.0);
        let round = StrokeStyle {
            join: LineJoin::Round,
            ..s.clone()
        };
        assert_eq!(round.outset(), 5.0);
    }

    #[test]
    fn gradient_ramp_merges_colour_and_opacity_stops() {
        // Two colour stops (red → blue) and opacity 1 → 0 at 0 and 0.5.
        let data = [
            0.0, 1.0, 0.0, 0.0, //
            1.0, 0.0, 0.0, 1.0, //
            0.0, 1.0, //
            0.5, 0.0,
        ];
        let stops = Gradient::stops_from_ae(&data, 2);
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(stops[1].offset, 0.5);
        assert_eq!(stops[1].color, [0.5, 0.0, 0.5, 0.0]);
        assert_eq!(stops[2].color, [0.0, 0.0, 1.0, 0.0]);
        // No data: white → black.
        assert_eq!(Gradient::stops_from_ae(&[], 2).len(), 2);
    }

    #[test]
    fn tokens() {
        assert_eq!(BlendMode::from_token("screen"), Some(BlendMode::Screen));
        assert_eq!(BlendMode::from_token("dissolve"), None);
        assert_eq!(LineCap::from_token("projecting"), LineCap::Projecting);
        assert_eq!(LineJoin::from_token("bevel"), LineJoin::Bevel);
        assert_eq!(FillRule::from_token("even_odd"), FillRule::EvenOdd);
        assert_eq!(Composite::from_token("above"), Composite::Above);
    }
}
