//! Layer-space stroke samples shared by the document command and the GPU renderer.
use bae_model::{Time, Value};

pub const ROTO: &str = "ADBE Samurai";
/// Internal effect parameters outside AE's public controls.
pub const STROKES: u32 = 0;
pub const EDGE_STROKES: u32 = 50_005;
pub const SPAN: u32 = 50_001;
pub const FROZEN: u32 = 50_002;
/// Bake stores confidence, refine coverage and temporal confidence in RGB.
pub const BAKE: u32 = 50_003;
/// Comp start, layer start (flicks) and stretch (%) at bake time.
pub const FROZEN_TIMING: u32 = 50_004;
/// [`FROZEN_RGB`] when the frozen matte stores the [`BAKE`] channels; absent
/// for mattes frozen before, which store the confidence in alpha.
pub const FROZEN_FORMAT: u32 = 50_006;
pub const FROZEN_RGB: i64 = 2;
pub const SEGMENT_SHADER: &str = include_str!("segment.wgsl");

/// Consecutive segments preserve pen order and the source time of each seed.
/// Layout: layer time in flicks, signed brush radius, start.xy, end.xy.
pub const SEGMENT_LEN: usize = 6;
pub fn append_stroke(data: &mut Vec<f64>, time: Time, radius: f64, points: &[[f64; 2]]) {
    let Some(first) = points.first() else { return };
    for (start, end) in std::iter::once(first).chain(points).zip(points) {
        data.extend([time.0 as f64, radius, start[0], start[1], end[0], end[1]]);
    }
}

pub fn segments(value: Option<&Value>) -> &[[f64; SEGMENT_LEN]] {
    match value {
        Some(Value::FloatList(data)) => data.as_chunks().0,
        _ => &[],
    }
}

pub fn span(value: Option<&Value>) -> Option<[Time; 2]> {
    match value {
        Some(Value::Vec2([a, b])) => Some([Time(*a as i64), Time(*b as i64)]),
        _ => None,
    }
}

/// Sampling policies for reconstructing the transparent part of a layer.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum FillMethod {
    #[default]
    Object,
    Surface,
    EdgeBlend,
}
#[derive(Clone, Copy, Default)]
pub struct FillSettings {
    pub method: FillMethod,
    pub expansion: u32,
    /// Fraction of the local exposure difference to compensate on temporal samples.
    pub lighting: f32,
}
pub const FILL_SHADER: &str = include_str!("fill.wgsl");
