use crate::{
    Builtin, EffectTarget, Registry, TargetEffect,
    catalog::DefBuilder,
    effects::{image_grid, sampled::Shader},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_anim::TextPathOptions;
use bae_gpu::wgpu;
use bae_model::{BezierPath, EffectDef, ParamKind, TextDocument, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::MaskSelect,
};
use bae_text::render::{TextProvider, TextSource, pixel_bounds};
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};

static COMPOSE: Shader = Shader {
    label: "bae.effects.text.overlay",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        "
fn evaluate(p:vec2f)->vec4f {
    let base=fx_sample(input_tex,u.images[0],p,false);
    let txt=fx_bilinear(map_a,u.images[1],p-u.values[231].xy,false);
    return txt+base*(1.-txt.a);
}"
    ),
};

pub(super) fn register(r: &mut Registry) {
    r.effects.extend([builtin(false), builtin(true)]);
    r.prewarm.push(prewarm);
}
fn prewarm(rt: &gpu::Runtime, f: wgpu::TextureFormat) -> Result<(), RenderError> {
    COMPOSE.kernel(rt, f)?;
    Ok(())
}

fn builtin(path: bool) -> Builtin {
    let (id, en, zh, slug) = if path {
        ("ADBE Path Text", "Path Text", "路径文本", "path-text")
    } else {
        ("ADBE Basic Text2", "Basic Text", "基本文字", "basic-text")
    };
    let mut b = DefBuilder::new(id, en, "Obsolete");
    for (i, en, zh, v) in [(1, "Text", "文字", "Text"), (2, "Font", "字体", "ArialMT")] {
        b.push(i, en, zh, ParamKind::Text, Some(Value::String(v.into())));
    }
    b = b
        .slider(
            3,
            "Size",
            "大小",
            48.,
            [0.1, 4096.],
            [1., 500.],
            1,
            "pixels",
        )
        .slider(
            4,
            "Tracking",
            "字距",
            0.,
            [-1000., 10000.],
            [-200., 200.],
            1,
            "none",
        );
    for (i, en, zh, c) in [
        (5, "Fill Color", "填充颜色", [1.; 4]),
        (6, "Stroke Color", "描边颜色", [0., 0., 0., 1.]),
    ] {
        b.push(i, en, zh, ParamKind::Color, Some(Value::Color(c)));
    }
    b = b
        .slider(
            7,
            "Stroke Width",
            "描边宽度",
            0.,
            [0., 1000.],
            [0., 50.],
            1,
            "pixels",
        )
        .popup(
            8,
            "Alignment",
            "对齐",
            if path { 1 } else { 2 },
            if path {
                &["Left", "Center", "Right", "Force"]
            } else {
                &["Left", "Center", "Right"]
            },
        )
        .popup(
            9,
            "Fill and Stroke",
            "填充和描边",
            1,
            &[
                "Fill Only",
                "Stroke Only",
                "Fill Over Stroke",
                "Stroke Over Fill",
            ],
        );
    let point = |b: &mut DefBuilder, i, en, zh, xy| {
        b.push(i, en, zh, ParamKind::Point, Some(Value::Vec2(xy)))
    };
    if path {
        b = b.popup(
            11,
            "Shape Type",
            "形状类型",
            1,
            &["Bezier", "Circle", "Loop", "Line"],
        );
        point(
            &mut b,
            12,
            "Vertex 1 / Circle Center",
            "顶点 1 / 圆心",
            [64., 180.],
        );
        point(
            &mut b,
            13,
            "Tangent 1 / Circle Point",
            "切线 1 / 圆上点",
            [200., 60.],
        );
        point(&mut b, 14, "Tangent 2", "切线 2", [440., 300.]);
        point(&mut b, 15, "Vertex 2", "顶点 2", [576., 180.]);
        b.push(16, "Custom Path", "自定义路径", ParamKind::Path, None);
        b = b
            .checkbox(17, "Reverse Path", "反转路径", false)
            .checkbox(18, "Perpendicular to Path", "垂直于路径", true)
            .slider(
                19,
                "First Margin",
                "首字边距",
                0.,
                [-30000., 30000.],
                [-500., 500.],
                1,
                "pixels",
            )
            .slider(
                20,
                "Last Margin",
                "末字边距",
                0.,
                [-30000., 30000.],
                [-500., 500.],
                1,
                "pixels",
            );
    } else {
        point(&mut b, 10, "Position", "位置", [320., 180.]);
    }
    let (mut def, names) = b.reference_points().build();
    // AE opens a modal on application. The capture has no disk parameter IDs.
    for p in &mut def.params {
        p.match_name = None;
    }
    let mut fx = rust_builtin(
        (
            Arc::new(TextFx {
                def,
                path,
                cache: Mutex::default(),
            }),
            names,
        ),
        slug,
        zh,
        &["text", "文字", "排版"],
    );
    for (i, opts) in [
        (
            8,
            if path {
                &["左对齐", "居中", "右对齐", "强制对齐"][..]
            } else {
                &["左对齐", "居中", "右对齐"][..]
            },
        ),
        (9, &["仅填充", "仅描边", "填充在描边上", "描边在填充上"][..]),
        (11, &["贝塞尔", "圆", "环", "直线"][..]),
    ] {
        if fx.def().param(i).is_some() {
            fx.info
                .param_options_zh
                .push((i, opts.iter().map(|s| s.to_string()).collect()));
        }
    }
    fx
}

#[derive(PartialEq)]
struct PathStyle {
    shape: BezierPath,
    reverse: bool,
    perpendicular: bool,
    force: bool,
    first: f64,
    last: f64,
}
struct Entry {
    doc: TextDocument,
    path: Option<PathStyle>,
    source: Arc<TextSource>,
}
struct TextFx {
    def: EffectDef,
    path: bool,
    cache: Mutex<VecDeque<Entry>>,
}

fn point(v: Params<'_>, id: u32, layer: IRect) -> [f64; 2] {
    let p = v.def.param(id).unwrap();
    if let Some(Value::Vec2(pos)) = v.p.value(id) {
        return *pos;
    }
    let Some(Value::Vec2(pos)) = p.default.as_ref() else {
        return [0.; 2];
    };
    [
        layer.x0 as f64 + pos[0] * f64::from(layer.width()) / 640.,
        layer.y0 as f64 + pos[1] * f64::from(layer.height()) / 360.,
    ]
}

fn circle(center: [f64; 2], start: [f64; 2], closed: bool) -> BezierPath {
    let mut shape = BezierPath {
        closed,
        ..Default::default()
    };
    let mut d = [start[0] - center[0], start[1] - center[1]];
    for _ in 0..if closed { 4 } else { 5 } {
        shape.vertices.push([center[0] + d[0], center[1] + d[1]]);
        let t = [-d[1] * 0.5522847498, d[0] * 0.5522847498];
        shape.in_tangents.push(t.map(|x| -x));
        shape.out_tangents.push(t);
        d = [-d[1], d[0]];
    }
    shape
}
fn path_style(v: Params<'_>, layer: IRect) -> PathStyle {
    let shape = if let Some(mask) = v.p.mask_paths.first() {
        mask.shape.clone()
    } else {
        let a = point(v, 12, layer);
        let b = point(v, 13, layer);
        let c = point(v, 14, layer);
        let d = point(v, 15, layer);
        match v.popup(11) {
            2 | 3 => circle(a, b, v.popup(11) == 2),
            mode => BezierPath {
                vertices: vec![a, d],
                in_tangents: vec![
                    [0.; 2],
                    if mode == 1 {
                        [c[0] - d[0], c[1] - d[1]]
                    } else {
                        [0.; 2]
                    },
                ],
                out_tangents: vec![
                    if mode == 1 {
                        [b[0] - a[0], b[1] - a[1]]
                    } else {
                        [0.; 2]
                    },
                    [0.; 2],
                ],
                ..Default::default()
            },
        }
    };
    PathStyle {
        shape,
        reverse: v.b(17),
        perpendicular: v.b(18),
        force: v.popup(8) == 4,
        first: v.f(19),
        last: v.f(20),
    }
}

impl TextFx {
    fn source(&self, v: Params<'_>, layer: IRect) -> Result<Arc<TextSource>, RenderError> {
        let string = |id| v.value(id).and_then(Value::as_str).unwrap_or_default();
        let color = |id| match v.value(id) {
            Some(Value::Color(c)) => *c,
            _ => [1.; 4],
        };
        let mut doc = TextDocument::new(string(1), string(2), v.f(3));
        doc.tracking = v.f(4);
        doc.justification = match v.popup(8) {
            2 => "center",
            3 => "right",
            _ => "left",
        }
        .into();
        doc.fill_color = if v.popup(9) == 2 { [0.; 4] } else { color(5) };
        doc.stroke_color = (v.popup(9) != 1).then(|| color(6));
        doc.stroke_width = v.f(7);
        doc.stroke_over_fill = v.popup(9) == 4;
        let path = self.path.then(|| path_style(v, layer));
        if let Some(entry) = self
            .cache
            .lock()
            .iter()
            .find(|e| e.doc == doc && e.path == path)
        {
            return Ok(entry.source.clone());
        }
        let mut layout = TextProvider::shared().layout_for_render(&doc)?;
        if let Some(style) = &path {
            let options = TextPathOptions {
                mask: v.p.effect.path.clone(),
                reverse: style.reverse,
                perpendicular: style.perpendicular,
                force_alignment: style.force,
                first_margin: style.first,
                last_margin: style.last,
            };
            bae_text::path::place_on_path(&mut layout, &style.shape, &options, &doc.justification);
        }
        let source = Arc::new(TextSource::new(Arc::new(layout)));
        let mut cache = self.cache.lock();
        if cache.len() >= 16 {
            cache.pop_front();
        }
        cache.push_back(Entry {
            doc,
            path,
            source: source.clone(),
        });
        Ok(source)
    }
}

impl Effect for TextFx {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn path_dependencies(&self, p: &EffectParams<'_>) -> Option<MaskSelect> {
        match p.value(16) {
            Some(Value::PrimRef(path)) if self.path => Some(MaskSelect::One(path.clone())),
            _ => None,
        }
    }
    fn render(
        &self,
        t: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(t, p, input, roi)
    }
}
impl TargetEffect for TextFx {
    fn encode(
        &self,
        t: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let grid = image_grid::grid_scale(p, input);
        let layer = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / (p.scale * grid))
        } else {
            p.layer_bounds
        };
        let v = Params::new(&self.def, p);
        let source = self.source(v, layer)?;
        let bounds = pixel_bounds(source.layout());
        let offset = if self.path {
            [0.; 2]
        } else {
            let [x, y] = point(v, 10, layer);
            [x, y - (f64::from(bounds.y0) + f64::from(bounds.y1)) * 0.5]
        };
        let visible = IRect::new(
            (roi.x0 as f64 / p.scale - offset[0]).floor() as i32,
            (roi.y0 as f64 / p.scale - offset[1]).floor() as i32,
            (roi.x1 as f64 / p.scale - offset[0]).ceil() as i32,
            (roi.y1 as f64 / p.scale - offset[1]).ceil() as i32,
        )
        .inflate(1)
        .intersect(&bounds);
        if visible.is_empty() {
            return Ok(input.clone());
        }
        let text = source.render_on(t.gpu(), visible, p.scale)?;
        // Vello returns full-resolution layer coordinates at the requested density.
        let mut stamp = text.clone();
        stamp.rect = image_grid::pixel_rect(stamp.rect, grid * p.scale);
        stamp.pixel_scale *= grid * p.scale;
        COMPOSE.dispatch_at(
            "main",
            t,
            v,
            roi,
            input,
            [Some(&stamp)],
            [0, 0],
            |w, header| {
                w.f32s(header + 231 * 16, &offset.map(|x| (x * p.scale) as f32));
            },
        )
    }
}
