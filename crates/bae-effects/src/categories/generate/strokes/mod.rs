//! Path and time based generators. Geometry is in layer pixels.
mod contour;
mod paint;
mod paint_strokes;
mod scribble;
mod stroke;
mod vegas;
mod write_on;

use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::MaskSelect,
    input::RenderMask,
};
use glam::DVec2;

use crate::{Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, params::Params};

pub(crate) use self::{
    paint_strokes::paint_effect, scribble::scribble, stroke::stroke, vegas::vegas,
    write_on::write_on,
};

const STYLE: &[&str] = &[
    "On Original Image",
    "On Transparent",
    "Reveal Original Image",
];
const STYLE_ZH: &[&str] = &["在原始图像上", "在透明背景上", "显示原始图像"];

pub(crate) fn options_zh(effect: &str, id: u32) -> Option<&'static [&'static str]> {
    match (effect, id) {
        ("ADBE Stroke", 7) | ("ADBE Write-on", 10) | ("ADBE Scribble Fill", 26) => Some(STYLE_ZH),
        ("ADBE Write-on", _) => write_on::options_zh(id),
        ("ADBE Scribble Fill", _) => scribble::options_zh(id),
        ("APC Vegas", _) => vegas::options_zh(id),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Generator {
    Paint,
    Stroke,
    WriteOn,
    Scribble,
    Vegas,
}

struct PaintEffect {
    def: EffectDef,
    kind: Generator,
}

impl Generator {
    fn builtin(self, b: DefBuilder, id: &str, zh: &str) -> Builtin {
        let (def, names) = b.build();
        let mut effect = crate::rust_builtin(
            (std::sync::Arc::new(PaintEffect { def, kind: self }), names),
            id,
            zh,
            &[id, zh],
        );
        if matches!(self, Self::Scribble) {
            effect.info.keywords.push("涂抹".into());
        }
        effect
    }
}

impl Effect for PaintEffect {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn path_dependencies(&self, p: &EffectParams<'_>) -> Option<MaskSelect> {
        let v = Params::new(&self.def, p);
        let id = match self.kind {
            Generator::Stroke if v.b(10) => return Some(MaskSelect::All),
            Generator::Stroke => 1,
            Generator::Scribble => match v.popup(64) {
                3 => 2,
                4 | 5 => return Some(MaskSelect::All),
                _ => return None,
            },
            Generator::Vegas if v.popup(52) == 2 => 50,
            _ => return None,
        };
        match v.value(id) {
            Some(Value::PrimRef(path)) => Some(MaskSelect::One(path.clone())),
            _ => None,
        }
    }

    fn parameter_sample_times(&self, p: &EffectParams<'_>) -> Vec<bae_model::Time> {
        match self.kind {
            Generator::WriteOn => write_on::sample_times(Params::new(&self.def, p)),
            _ => Vec::new(),
        }
    }

    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<bae_render::effect::LayerDependency> {
        match self.kind {
            Generator::Vegas if Params::new(&self.def, p).popup(52) == 1 => {
                crate::effects::layer_input::dependency(p, 2, false)
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        if matches!(self.kind, Generator::Vegas) && Params::new(&self.def, p).popup(52) == 1 {
            return roi.union(&crate::effects::image_grid::layer_rect(p));
        }
        roi
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

impl TargetEffect for PaintEffect {
    fn encode(
        &self,
        ctx: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let v = Params::new(&self.def, p);
        let paint = match self.kind {
            Generator::Paint => {
                let clear = paint_strokes::on_transparent(v);
                return paint_strokes::render(ctx, p, clear, input, roi);
            }
            Generator::Stroke => stroke::paint(v),
            Generator::WriteOn => write_on::paint(v, input),
            Generator::Scribble => scribble::paint(v, input),
            Generator::Vegas if v.popup(52) == 1 => {
                return vegas::render_contours(ctx, v, input, roi);
            }
            Generator::Vegas => vegas::paint(v),
        };
        paint.render(ctx, p, input, roi)
    }
}

fn percent(b: DefBuilder, id: u32, name: &str, zh: &str, default: f64, max: f64) -> DefBuilder {
    b.slider(id, name, zh, default, [0., max], [0., max], 1, "percent")
}

fn slider(
    b: DefBuilder,
    id: u32,
    name: &str,
    zh: &str,
    default: f64,
    range: [f64; 2],
    unit: &str,
) -> DefBuilder {
    b.slider(id, name, zh, default, range, range, 3, unit)
}

fn style(b: DefBuilder, id: u32, name: &str, zh: &str, default: i64) -> DefBuilder {
    b.popup(id, name, zh, default, STYLE)
}

fn color(v: Params<'_>, id: u32) -> [f32; 3] {
    match v.value(id) {
        Some(Value::Color([r, g, b, _])) => [*r as f32, *g as f32, *b as f32],
        _ => [1.; 3],
    }
}

fn trim(v: Params<'_>, start: u32, end: u32) -> [f64; 2] {
    let a = v.f(start).clamp(0., 100.) * 0.01;
    let b = v.f(end).clamp(0., 100.) * 0.01;
    [a.min(b), a.max(b)]
}

struct Path {
    points: Vec<DVec2>,
    lengths: Vec<f64>,
}

impl Path {
    fn new(mut points: Vec<DVec2>) -> Self {
        points.dedup();
        let mut lengths = Vec::with_capacity(points.len());
        if !points.is_empty() {
            lengths.push(0.);
        }
        for pair in points.windows(2) {
            lengths.push(lengths.last().unwrap() + pair[0].distance(pair[1]));
        }
        Self { points, lengths }
    }

    fn from_mask(mask: &RenderMask) -> Self {
        let shape = bae_render::mask::to_bez_path(&mask.shape);
        Self::from_elements(shape.elements())
    }

    fn from_elements(elements: &[kurbo::PathEl]) -> Self {
        let mut points = Vec::new();
        kurbo::flatten(elements.iter().copied(), 0.05, |el| match el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => {
                points.push(DVec2::new(p.x, p.y))
            }
            kurbo::PathEl::ClosePath => points.push(points[0]),
            _ => unreachable!(),
        });
        Self::new(points)
    }

    fn length(&self) -> f64 {
        self.lengths.last().copied().unwrap_or_default()
    }

    fn at(&self, distance: f64) -> DVec2 {
        let i = self.lengths.partition_point(|&x| x < distance);
        if i == 0 {
            self.points[0]
        } else if i == self.points.len() {
            self.points[i - 1]
        } else {
            let from = self.lengths[i - 1];
            self.points[i - 1].lerp(self.points[i], (distance - from) / (self.lengths[i] - from))
        }
    }

    fn segments(
        &self,
        start: f64,
        end: f64,
    ) -> impl Iterator<Item = (DVec2, DVec2, f64, f64)> + '_ {
        let a = self
            .lengths
            .partition_point(|&x| x <= start)
            .saturating_sub(1);
        let b = self
            .lengths
            .partition_point(|&x| x < end)
            .min(self.lengths.len().saturating_sub(1));
        (a..b).map(move |i| {
            let l = self.lengths[i];
            let r = self.lengths[i + 1];
            let from = start.max(l);
            let to = end.min(r);
            let p = self.points[i];
            let q = self.points[i + 1];
            (
                p.lerp(q, (from - l) / (r - l)),
                p.lerp(q, (to - l) / (r - l)),
                from,
                to,
            )
        })
    }
}

// Gives each path's visible interval. Sequential trim measures the whole
// ordered stack, with no artificial connecting segment between masks.
fn ranges(
    paths: &[Path],
    [start, end]: [f64; 2],
    sequential: bool,
) -> impl Iterator<Item = (usize, &Path, f64, f64)> {
    let total: f64 = paths.iter().map(Path::length).sum();
    let mut offset = 0.;
    paths.iter().enumerate().filter_map(move |(i, path)| {
        let len = path.length();
        let (from, to) = if sequential {
            (start * total - offset, end * total - offset)
        } else {
            (start * len, end * len)
        };
        offset += len;
        let from = from.max(0.);
        let to = to.min(len);
        (from < to).then_some((i, path, from, to))
    })
}

pub(crate) fn prewarm(
    rt: &crate::gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<(), RenderError> {
    paint::kernel(rt, format)?;
    vegas::contour_kernel(rt)?;
    contour::prewarm(rt, format)?;
    Ok(())
}
