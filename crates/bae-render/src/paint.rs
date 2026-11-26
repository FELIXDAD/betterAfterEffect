//! Paint strokes as the renderer sees them (the Paint effect, `ADBE Paint`).
//! The comp conversion packs the strokes visible at the frame into the
//! effect's parameter [`STROKES_PARAM`]; the effect unpacks them. Packing keeps
//! them in the effect's parameters, so the frame cache sees every change.

use bae_anim::EvaluatedPrim;
use bae_model::{Time, Value};
use glam::DVec2;

/// The Paint effect's matchName.
pub const PAINT_EFFECT: &str = "ADBE Paint";
/// The effect parameter holding the packed strokes (AE parameters start at 1).
pub const STROKES_PARAM: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Clone,
    Eraser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erase {
    LayerAndPaint,
    PaintOnly,
    LastStroke,
}

/// One stroke at the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub tool: Tool,
    pub erase: Erase,
    /// The path in layer pixels (the stroke's transform applied), between its
    /// Start and End.
    pub points: Vec<DVec2>,
    /// Straight RGBA.
    pub color: [f32; 4],
    pub diameter: f64,
    /// Degrees.
    pub angle: f64,
    /// 0–1.
    pub hardness: f32,
    pub roundness: f32,
    /// Distance between dabs as a fraction of the diameter.
    pub spacing: f64,
    pub opacity: f32,
    pub flow: f32,
    /// 0 RGBA, 1 RGB, 2 Alpha.
    pub channels: u8,
    /// Clone Stamp: source point minus painted point.
    pub clone_offset: DVec2,
}

fn token(p: &EvaluatedPrim, attr: &str) -> String {
    match p.get(attr) {
        Some(Value::Enum(s) | Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The strokes of a Paint effect visible at layer time `t`, oldest first.
pub fn strokes(children: &[EvaluatedPrim], t: Time) -> Vec<Stroke> {
    children
        .iter()
        .filter_map(|p| {
            let time = |a: &str| match p.get(a) {
                Some(Value::Time(t)) => Some(*t),
                _ => None,
            };
            let (from, to) = (time("in_point")?, time("out_point")?);
            if t < from || t >= to {
                return None;
            }
            let path = match p.get("path") {
                Some(Value::Bezier(b)) => b,
                _ => return None,
            };
            let f = |a: &str, d: f64| p.f64(a).unwrap_or(d);
            let v2 = |a: &str, d: [f64; 2]| match p.get(a) {
                Some(Value::Vec2(v)) => DVec2::from_array(*v),
                _ => DVec2::from_array(d),
            };
            // The stroke's transform: anchor, scale, rotation, position.
            let (anchor, position) = (
                v2("transform:anchor_point", [0.0; 2]),
                v2("transform:position", [0.0; 2]),
            );
            let scale = v2("transform:scale", [100.0; 2]) / 100.0;
            let rot = DVec2::from_angle(f("transform:rotation", 0.0).to_radians());
            let place = |q: DVec2| position + rot.rotate((q - anchor) * scale) + anchor;
            let all: Vec<DVec2> = path
                .vertices
                .iter()
                .map(|v| place(DVec2::from_array(*v)))
                .collect();
            let points = section(&all, f("start", 0.0) / 100.0, f("end", 100.0) / 100.0);
            let color = match p.get("color") {
                Some(Value::Color(c)) => c.map(|v| v as f32),
                _ => [1.0; 4],
            };
            let clone = v2("clone:position", [0.0; 2]) - all.first().copied().unwrap_or_default();
            Some(Stroke {
                tool: match token(p, "tool").as_str() {
                    "clone" => Tool::Clone,
                    "eraser" => Tool::Eraser,
                    _ => Tool::Brush,
                },
                erase: match token(p, "erase").as_str() {
                    "paint" => Erase::PaintOnly,
                    "last_stroke" => Erase::LastStroke,
                    _ => Erase::LayerAndPaint,
                },
                points,
                color,
                diameter: f("diameter", 10.0).max(0.0),
                angle: f("angle", 0.0),
                hardness: (f("hardness", 100.0) / 100.0).clamp(0.0, 1.0) as f32,
                roundness: (f("roundness", 100.0) / 100.0).clamp(0.0, 1.0) as f32,
                spacing: (f("spacing", 25.0) / 100.0).max(0.01),
                opacity: (f("opacity", 100.0) / 100.0).clamp(0.0, 1.0) as f32,
                flow: (f("flow", 100.0) / 100.0).clamp(0.0, 1.0) as f32,
                channels: match token(p, "channels").as_str() {
                    "rgb" => 1,
                    "alpha" => 2,
                    _ => 0,
                },
                clone_offset: clone,
            })
        })
        .collect()
}

/// The part of a polyline between fractions `a` and `b` of its length.
fn section(points: &[DVec2], a: f64, b: f64) -> Vec<DVec2> {
    let (a, b) = (a.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
    if points.len() < 2 || (a <= 0.0 && b >= 1.0) {
        return if b > a { points.to_vec() } else { Vec::new() };
    }
    if b <= a {
        return Vec::new();
    }
    let total: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    let (from, to) = (a * total, b * total);
    let mut out = Vec::new();
    let mut at = 0.0;
    for w in points.windows(2) {
        let len = w[0].distance(w[1]);
        let (s, e) = (at, at + len);
        if e >= from && (s < to || out.is_empty()) && len > 0.0 {
            let p = |d: f64| w[0].lerp(w[1], ((d - s) / len).clamp(0.0, 1.0));
            if out.is_empty() {
                out.push(p(from.max(s)));
            }
            out.push(p(to.min(e)));
        }
        at = e;
    }
    out
}

const HEADER: usize = 18;

/// The strokes as one parameter value.
pub fn pack(strokes: &[Stroke]) -> Value {
    let mut v = Vec::new();
    for s in strokes {
        v.extend([
            s.tool as u8 as f64,
            s.erase as u8 as f64,
            s.points.len() as f64,
            f64::from(s.color[0]),
            f64::from(s.color[1]),
            f64::from(s.color[2]),
            f64::from(s.color[3]),
            s.diameter,
            s.angle,
            f64::from(s.hardness),
            f64::from(s.roundness),
            s.spacing,
            f64::from(s.opacity),
            f64::from(s.flow),
            f64::from(s.channels),
            s.clone_offset.x,
            s.clone_offset.y,
            0.0,
        ]);
        for p in &s.points {
            v.extend([p.x, p.y]);
        }
    }
    Value::FloatList(v)
}

/// The strokes [`pack`] packed.
pub fn unpack(value: &Value) -> Vec<Stroke> {
    let Value::FloatList(v) = value else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i + HEADER <= v.len() {
        let h = &v[i..i + HEADER];
        let n = h[2] as usize;
        let end = i + HEADER + 2 * n;
        if end > v.len() {
            break;
        }
        out.push(Stroke {
            tool: match h[0] as u8 {
                1 => Tool::Clone,
                2 => Tool::Eraser,
                _ => Tool::Brush,
            },
            erase: match h[1] as u8 {
                1 => Erase::PaintOnly,
                2 => Erase::LastStroke,
                _ => Erase::LayerAndPaint,
            },
            points: v[i + HEADER..end]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| DVec2::new(c[0], c[1]))
                .collect(),
            color: [h[3], h[4], h[5], h[6]].map(|c| c as f32),
            diameter: h[7],
            angle: h[8],
            hardness: h[9] as f32,
            roundness: h[10] as f32,
            spacing: h[11],
            opacity: h[12] as f32,
            flow: h[13] as f32,
            channels: h[14] as u8,
            clone_offset: DVec2::new(h[15], h[16]),
        });
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_pack_and_cut_at_start_and_end() {
        let pts = vec![DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::new(10.0, 10.0)];
        let half = section(&pts, 0.0, 0.5);
        assert_eq!(half, vec![DVec2::ZERO, DVec2::new(10.0, 0.0)]);
        let s = Stroke {
            tool: Tool::Eraser,
            erase: Erase::LastStroke,
            points: pts,
            color: [1.0, 0.5, 0.0, 1.0],
            diameter: 20.0,
            angle: 30.0,
            hardness: 0.5,
            roundness: 1.0,
            spacing: 0.25,
            opacity: 0.8,
            flow: 1.0,
            channels: 1,
            clone_offset: DVec2::new(3.0, 4.0),
        };
        assert_eq!(unpack(&pack(&[s.clone(), s.clone()])), vec![s.clone(), s]);
    }
}
