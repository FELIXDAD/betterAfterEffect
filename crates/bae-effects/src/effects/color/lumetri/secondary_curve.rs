use bae_model::Value;

/// One of the five Lumetri hue, saturation and luma curves.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LumetriCurve(u32);

impl LumetriCurve {
    /// Find a curve by its curve parameter or its color picker.
    pub fn from_param(id: u32) -> Option<Self> {
        Some(Self(match id {
            42 | 113 => 42,
            106 | 114 => 106,
            108 | 115 => 108,
            110 | 116 => 110,
            112 | 117 => 112,
            _ => return None,
        }))
    }

    /// ID of the editable curve parameter.
    pub fn id(self) -> u32 {
        self.0
    }

    /// Whether the horizontal axis wraps at red.
    pub fn is_hue_axis(self) -> bool {
        matches!(self.0, 42 | 106 | 108)
    }

    /// Ordered control points from the single channel read by the shader.
    pub fn points(self, value: Option<&Value>) -> Vec<[f64; 2]> {
        let Some(Value::FloatList(values)) = value else {
            return Vec::new();
        };
        let mut points: Vec<_> = values
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|p| p[0] == 0. && p[1].is_finite() && p[2].is_finite())
            .map(|p| {
                [
                    if self.is_hue_axis() {
                        p[1].rem_euclid(1.)
                    } else {
                        p[1]
                    },
                    p[2],
                ]
            })
            .collect();
        points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        points.dedup_by(|a, b| a[0] == b[0]);
        points
    }

    /// The same interpolation used for the GPU grading table.
    pub fn samples(self, points: &[[f64; 2]]) -> [f64; 256] {
        super::curve_samples(&mut points.to_vec(), self.is_hue_axis())
    }

    /// Insert a center and two range points into the existing curve.
    /// The sample contains straight, encoded RGB, as shown in the color control.
    pub fn pick(self, rgb: [f64; 3], value: Option<&Value>) -> Value {
        let [r, g, b] = rgb.map(|v| v.clamp(0., 1.).powf(2.4));
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        let delta = hi - lo;
        let x = match self.0 {
            110 => 0.2126 * r + 0.7152 * g + 0.0722 * b,
            _ if delta == 0. => 0.,
            112 => {
                let denom = 1. - (hi + lo - 1.).abs();
                if denom > 1e-6 { delta / denom } else { 0. }
            }
            _ => {
                let sector = if hi == r {
                    (g - b) / delta
                } else if hi == g {
                    2. + (b - r) / delta
                } else {
                    4. + (r - g) / delta
                };
                (sector / 6.).rem_euclid(1.)
            }
        }
        .clamp(0., 1.);
        let width = 1. / 12.;
        let xs = if self.is_hue_axis() {
            [(x - width).rem_euclid(1.), x, (x + width).rem_euclid(1.)]
        } else if x <= f64::EPSILON {
            [0., width, 2. * width]
        } else if x >= 1. - f64::EPSILON {
            [1. - 2. * width, 1. - width, 1.]
        } else {
            [(x - width).max(0.), x, (x + width).min(1.)]
        };
        let mut points = self.points(value);
        let samples = self.samples(&points);
        for x in xs {
            if points.iter().any(|p| (x - p[0]).abs() < 1e-8) {
                continue;
            }
            let t = x * 255.;
            let i = t.floor() as usize;
            let y = samples[i] + (samples[(i + 1).min(255)] - samples[i]) * t.fract();
            points.push([x, y]);
        }
        points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        Self::value(&points)
    }

    /// Project representation of the edited points.
    pub fn value(points: &[[f64; 2]]) -> Value {
        Value::FloatList(points.iter().flat_map(|&[x, y]| [0., x, y]).collect())
    }
}
