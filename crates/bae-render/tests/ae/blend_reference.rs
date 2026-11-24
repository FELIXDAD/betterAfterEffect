//! Scalar CPU oracle for the layer operator, returning premultiplied RGBA.
use bae_render::BlendMode::{self, *};

type Rgb = [f32; 3];

fn lum([r, g, b]: Rgb) -> f32 {
    0.30 * r + 0.59 * g + 0.11 * b
}
fn extrema(c: Rgb) -> (f32, f32) {
    (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]))
}
fn sat(c: Rgb) -> f32 {
    let (lo, hi) = extrema(c);
    hi - lo
}
fn set_sat(mut c: Rgb, s: f32) -> Rgb {
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| c[i].total_cmp(&c[j]));
    let [lo, mid, hi] = order;
    if c[hi] == c[lo] {
        return [0.; 3];
    }
    c[mid] = s * (c[mid] - c[lo]) / (c[hi] - c[lo]);
    c[lo] = 0.;
    c[hi] = s;
    c
}
fn set_lum(c: Rgb, l: f32) -> Rgb {
    let c = c.map(|x| x + l - lum(c));
    let (min, max) = extrema(c);
    c.map(|mut x| {
        if min < 0. && min < l {
            x = l + (x - l) * l / (l - min);
        }
        if max > 1. && max > l {
            x = l + (x - l) * (1. - l) / (max - l);
        }
        x.clamp(0., 1.)
    })
}
fn burn(b: f32, s: f32) -> f32 {
    if b >= 1. {
        1.
    } else if s <= 0. {
        0.
    } else {
        1. - ((1. - b) / s).min(1.)
    }
}
fn dodge(b: f32, s: f32) -> f32 {
    if b <= 0. {
        0.
    } else if s >= 1. {
        1.
    } else {
        (b / (1. - s)).min(1.)
    }
}
fn hard(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        2. * s * b
    } else {
        1. - 2. * (1. - s) * (1. - b)
    }
}

pub fn pixel(
    mode: BlendMode,
    src: [f32; 4],
    opacity: f32,
    dst: [f32; 4],
    depth: bae_render::BitDepth,
    random: f32,
) -> [f32; 4] {
    let bounded = depth.clamps();
    let [r, g, b, ab] = dst;
    let back = [r, g, b];
    let [r, g, b, alpha] = src;
    let front = [r, g, b];
    let a = alpha * opacity;
    let mut ao = a + ab * (1. - a);
    if matches!(mode, Hue | Saturation | Color | Luminosity) {
        let steps = match depth {
            bae_render::BitDepth::Bpc8 => 255.,
            bae_render::BitDepth::Bpc16 => 32768.,
            _ => 0.,
        };
        let q = |v: f32| {
            if steps > 0. {
                (v * steps).round() / steps
            } else {
                v
            }
        };
        let a = q(a);
        let ab = q(ab);
        let union = q(ab + (1. - ab) * a);
        if union == 0. {
            return [0.; 4];
        }
        let d = std::array::from_fn(|i| q((q(back[i] * ab) + front[i] * a * (1. - ab)) / union));
        let transfer = match mode {
            Hue => set_lum(set_sat(front, sat(d)), lum(d)),
            Saturation => set_lum(set_sat(d, sat(front)), lum(d)),
            Color => set_lum(front, lum(d)),
            _ => set_lum(d, lum(front)),
        };
        let mut result = [0.; 4];
        for i in 0..3 {
            result[i] = union * ((1. - a) * d[i] + a * transfer[i]);
        }
        result[3] = union;
        return result;
    }
    let whole = match mode {
        DarkerColor if front.iter().sum::<f32>() >= back.iter().sum() => back,
        LighterColor if front.iter().sum::<f32>() <= back.iter().sum() => back,
        _ => front,
    };
    let mut out = [0.; 4];
    for i in 0..3 {
        let (s, b) = (front[i], back[i]);
        let (sp, bp) = (s * a, b * ab);
        let overlap = match mode {
            Darken => b.min(s),
            Multiply => b * s,
            ColorBurn => burn(b, s),
            LinearBurn => b + s - 1.,
            Lighten => b.max(s),
            Screen => 1. - (1. - b) * (1. - s),
            ColorDodge => dodge(b, s),
            LinearDodge => (b + s).min(1.),
            Overlay => hard(s, b),
            HardLight => hard(b, s),
            SoftLight if s <= 0.5 => b - (1. - 2. * s) * b * (1. - b),
            SoftLight => {
                let d = if b <= 0.25 {
                    ((16. * b - 12.) * b + 4.) * b
                } else {
                    b.sqrt()
                };
                b + (2. * s - 1.) * (d - b)
            }
            LinearLight => b + 2. * s - 1.,
            VividLight if s <= 0.5 => burn(b, 2. * s),
            VividLight => dodge(b, 2. * s - 1.),
            PinLight if s <= 0.5 => b.min(2. * s),
            PinLight => b.max(2. * s - 1.),
            HardMix => f32::from(b + s >= 1.),
            Difference => (s - b).abs(),
            Exclusion => b + s - 2. * b * s,
            Subtract => b - s,
            Divide if !bounded => (b / s.max(1e-6)).clamp(-65504., 65504.),
            Divide if s <= 0. => f32::from(b > 0.),
            Divide => b / s,
            _ => whole[i],
        };
        let v = match mode {
            Dissolve | DancingDissolve if random < a => {
                ao = 1.;
                s
            }
            Dissolve | DancingDissolve => {
                ao = ab;
                bp
            }
            StencilAlpha | SilhouetteAlpha | StencilLuma | SilhouetteLuma => {
                let mut m = a;
                if matches!(mode, StencilLuma | SilhouetteLuma) {
                    m *= 0.299 * r + 0.587 * g + 0.114 * front[2];
                }
                m = m.clamp(0., 1.);
                if matches!(mode, SilhouetteAlpha | SilhouetteLuma) {
                    m = 1. - m;
                }
                ao = ab * m;
                bp * m
            }
            AlphaAdd => {
                ao = (a + ab).min(1.);
                sp + b * ab.min(1. - a)
            }
            LuminescentPremul => s * opacity + bp * (1. - a),
            Add => sp + bp,
            ClassicDifference => (bp - sp).abs(),
            ClassicColorBurn | ClassicColorDodge if ab == 0. => sp,
            ClassicColorBurn if a == 1. && sp <= 0. => 0.,
            ClassicColorDodge if sp >= 1. => ao,
            ClassicColorBurn => (ao - (ab - bp) / (1. - a + sp).max(1e-6)).max(0.),
            ClassicColorDodge => (bp / (1. - sp).max(1e-6)).min(ao),
            _ => {
                let mix = if bounded {
                    overlap.clamp(0., 1.)
                } else {
                    overlap
                };
                (1. - ab) * sp + (1. - a) * bp + a * ab * mix
            }
        };
        out[i] = if bounded { v.clamp(0., ao) } else { v };
    }
    out[3] = ao;
    out
}

pub fn random(x: u32, y: u32, path: &str, frame: u32) -> f32 {
    let mut h = 2166136261u32;
    for b in path.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16777619);
    }
    h ^= frame ^ x.wrapping_mul(0x9e3779b9) ^ y.wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    ((h ^ (h >> 16)) >> 8) as f32 / 16777216.
}
