use super::edge::{Kind, Paint, Shading};
use crate::{
    Builtin, EffectTarget, TargetEffect,
    blur::{self, Edge},
    catalog::DefBuilder,
    effects::{image_grid, sampled::Shader},
    params::Params,
    rust_builtin,
};
use bae_model::{
    EffectDef, Value,
    layer_styles::{Control, GROUPS, Group, RENDER_ID, SOURCE_BLEND_RANGES},
};
use bae_render::graph::layer_styles as graph;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use std::sync::Arc;

pub(super) static SHADER: Shader = Shader {
    label: "bae.effects/layer-style-coating",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../shaders/layer_sampling.wgsl"),
        include_str!("../../shaders/generated_blend.wgsl"),
        include_str!("blend.wgsl"),
        include_str!("native.wgsl")
    ),
};

pub(super) fn builtin() -> Builtin {
    let (def, names) = DefBuilder::new(RENDER_ID, "Layer Styles", "Layer Styles").build();
    rust_builtin(
        (Arc::new(Stack(def)), names),
        "layer-styles",
        "图层样式",
        &["Layer Styles", "图层样式"],
    )
}

struct Stack(EffectDef);

#[derive(Clone, Copy)]
struct Style<'a> {
    group: &'static Group,
    p: Params<'a>,
}
impl<'a> Style<'a> {
    fn new(p: Params<'a>, index: usize) -> Self {
        Self {
            group: &GROUPS[index],
            p,
        }
    }
    fn on(self) -> bool {
        self.p.b(self.group.enabled)
    }
    fn field(self, key: &str) -> Option<&'static bae_model::layer_styles::Field> {
        self.group
            .fields
            .iter()
            .find(|f| f.match_name.rsplit('/').next() == Some(key))
    }
    fn val(self, key: &str) -> Option<&'a Value> {
        self.p.value(self.field(key)?.id)
    }
    fn n(self, key: &str) -> f64 {
        let Some(f) = self.field(key) else {
            return 0.;
        };
        if self.p.value(f.id).is_some() {
            return self.p.f(f.id);
        }
        match f.kind {
            Control::Number(v, ..) | Control::Angle(v) => v,
            Control::Menu(i, _) => i as f64,
            Control::Switch(b) => f64::from(b),
            _ => 0.,
        }
    }
    fn color(self, key: &str) -> [f32; 4] {
        match self.val(key) {
            Some(Value::Color(c)) => c.map(|v| v as f32),
            _ => match self.field(key).map(|f| f.kind) {
                Some(Control::Color(c)) => c.map(|v| v as f32),
                _ => [0.; 4],
            },
        }
    }
    fn length(self, key: &str) -> f64 {
        self.n(key) * self.p.p.scale
    }
    fn angle(self) -> f64 {
        if self.n("useGlobalAngle") != 0. {
            Style::new(self.p, 0).n("ADBE Global Angle2")
        } else {
            self.n("localLightingAngle")
        }
        .to_radians()
    }
    fn offset(self) -> [f64; 2] {
        let a = self.angle();
        let d = self.length("distance");
        [-a.cos() * d, a.sin() * d]
    }
    fn shading(self, kind: Kind) -> Shading {
        let bevel = kind == Kind::Bevel;
        let a = self.angle();
        let altitude = if self.n("useGlobalAngle") != 0. {
            Style::new(self.p, 0).n("ADBE Global Altitude2")
        } else {
            self.n("localLightingAltitude")
        };
        let dir = if self.n("bevelDirection") == 2. {
            -1.
        } else {
            1.
        };
        let mut color = self.color(if bevel { "highlightColor" } else { "color" });
        color[3] *= (self.n(if bevel { "highlightOpacity" } else { "opacity" }) * 0.01) as f32;
        let mut shadow = self.color("shadowColor");
        shadow[3] *= (self.n("shadowOpacity") * 0.01) as f32;
        let spread = self.n("chokeMatte") * 0.01;
        let range = self.n("inputRange");
        let blur = self.length("blur");
        Shading {
            kind,
            color,
            shadow_color: shadow,
            offset: self.offset(),
            sigma: (blur * 0.45 + self.length("softness") * 0.5).max(0.),
            gain: if bevel {
                (self.n("strengthRatio") * 0.01 * (1. + blur) * altitude.to_radians().cos()) as f32
            } else {
                ((1. + spread * 4.) * if range > 0. { 50. / range } else { 1. }) as f32
            },
            center: kind == Kind::InnerGlow && self.n("innerGlowSource") == 2.,
            light: [(-a.cos() * dir) as f32, (a.sin() * dir) as f32],
            pillow: self.n("bevelStyle") == 4.,
            mode: blend(self.n(if bevel { "highlightMode" } else { "mode2" })),
            shadow_mode: blend(self.n("shadowMode")),
            bevel_style: self.n("bevelStyle") as u32,
            technique: self.n(if bevel {
                "bevelTechnique"
            } else {
                "glowTechnique"
            }) as u32,
            width: blur as f32,
            noise: (self.n("noise") * 0.01) as f32,
            jitter: (self.n("shadingNoise") * 0.01) as f32,
            knockout: self.n("layerConceals") != 0.,
        }
    }
}

// The shared color blend kernel uses the 4-Color Gradient menu's IDs.
fn blend(mode: f64) -> u32 {
    const IDS: [u32; 33] = [
        2, 31, 32, 14, 5, 12, 42, 23, 33, 4, 6, 11, 43, 24, 34, 7, 8, 9, 25, 26, 27, 28, 16, 44,
        17, 35, 36, 19, 20, 21, 22, 37, 39,
    ];
    IDS[(mode as usize).clamp(1, IDS.len()) - 1]
}

fn layer_mode(mode: usize) -> u32 {
    const IDS: [u32; 38] = [
        2, 31, 32, 14, 5, 12, 42, 23, 33, 4, 15, 6, 11, 43, 24, 34, 7, 8, 9, 25, 26, 27, 28, 16,
        44, 17, 35, 36, 19, 20, 21, 22, 37, 38, 39, 40, 41, 2,
    ];
    IDS.get(mode).copied().unwrap_or(2)
}

const EDGES: [(usize, Kind); 5] = [
    (1, Kind::DropShadow),
    (3, Kind::OuterGlow),
    (4, Kind::InnerGlow),
    (2, Kind::InnerShadow),
    (5, Kind::Bevel),
];
impl Stack {
    fn region(&self, p: &EffectParams<'_>, r: IRect, read: bool) -> IRect {
        if r.is_empty() {
            return r;
        }
        let params = Params::new(&self.0, p);
        let mut result = r;
        for (index, kind) in EDGES {
            let s = Style::new(params, index);
            if s.on() {
                let edge = s.shading(kind);
                result = result.union(&if read { edge.read(r) } else { edge.bounds(r) });
            }
        }
        let satin = Style::new(params, 6);
        if satin.on() && read {
            result = result.union(&r.inflate(
                (satin.length("distance") + 3. * satin.length("blur") * 0.45 + 2.).ceil() as i32,
            ));
        }
        let stroke = Style::new(params, 9);
        if stroke.on() {
            let width = stroke.length("size");
            let extra = if read {
                width + 2.
            } else {
                match stroke.n("style") as i32 {
                    2 => 0.,
                    3 => width * 0.5,
                    _ => width,
                }
            };
            result = result.union(&r.inflate(extra.ceil() as i32));
        }
        result
    }
}

impl Effect for Stack {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn input_roi(&self, p: &EffectParams<'_>, r: IRect) -> IRect {
        self.region(p, r, true)
    }
    fn output_bounds(&self, p: &EffectParams<'_>, r: IRect) -> IRect {
        self.region(p, r, false)
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

/// Upload authored stops in overlapping pages. GPU threads interpolate the
/// ramp once, shared by every shaded pixel regardless of the stop count.
fn gradient_lut(
    target: &mut dyn EffectTarget,
    style: Style<'_>,
    source: &GpuImage,
) -> Result<GpuImage, RenderError> {
    let mut stops = match style.val("gradient") {
        Some(Value::FloatList(v)) => v.as_chunks::<5>().0.to_vec(),
        _ => Vec::new(),
    };
    if stops.is_empty() {
        stops.extend([[0., 0., 0., 0., 1.], [1., 1., 1., 1., 1.]]);
    }
    stops.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut previous = None;
    let mut start = 0;
    loop {
        let end = (start + 96).min(stops.len());
        let page = &stops[start..end];
        let image = SHADER.dispatch_at(
            "raw_values",
            target,
            style.p,
            IRect::from_size(1024, 1),
            source,
            [previous.as_ref()],
            [10, page.len() as u32],
            |w, h| {
                w.f32(h + 200 * 16, (style.n("gradientSmoothness") * 0.01) as f32);
                for (i, stop) in page.iter().enumerate() {
                    w.f32s(
                        h + (2 * i) * 16,
                        &[
                            stop[1] as f32,
                            stop[2] as f32,
                            stop[3] as f32,
                            stop[4] as f32,
                        ],
                    );
                    w.f32(h + (2 * i + 1) * 16, stop[0] as f32);
                }
            },
        )?;
        if end == stops.len() {
            return Ok(image);
        }
        previous = Some(image);
        start = end - 1;
    }
}

impl TargetEffect for Stack {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        source: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(source.clone());
        }
        let params = Params::new(&self.0, p);
        let settings = Style::new(params, 0);
        let fill = (settings.n("ADBE Layer Fill Opacity2") / 100.) as f32;
        let backdrop = target
            .effect_layer(&graph::backdrop_input(p.effect, p.comp_time))
            .cloned();
        let matte = target
            .effect_layer(&graph::matte_input(p.effect, p.comp_time))
            .cloned();
        let grid = image_grid::grid_scale(p, source);
        let work = self
            .region(p, image_grid::pixel_rect(source.rect, 1. / grid), false)
            .intersect(&roi);
        if work.is_empty() {
            return Ok(backdrop.unwrap_or_else(|| source.clone()));
        }
        let stroke = Style::new(params, 9);
        let stroke_width = if stroke.on() {
            stroke.length("size")
        } else {
            0.
        };
        let mut radius = stroke_width;
        for (index, kind) in EDGES {
            let s = Style::new(params, index);
            if s.on() {
                let shading = s.shading(kind);
                if shading.technique > 1 && (kind != Kind::Bevel || shading.bevel_style != 5) {
                    radius = radius.max(shading.width.into());
                }
            }
        }
        let distance = if radius > 0. {
            Some(distance_field(target, params, source, work, radius)?)
        } else {
            None
        };
        let mut exterior =
            SHADER.dispatch(target, params, work, source, [backdrop.as_ref()], [8, 0])?;
        for (index, kind) in EDGES[..2].iter().copied() {
            let s = Style::new(params, index);
            if s.on() {
                exterior = edge(
                    target,
                    s,
                    kind,
                    source,
                    &exterior,
                    work,
                    Paint {
                        backdrop: true,
                        distance: distance.as_ref(),
                        ..Default::default()
                    },
                )?;
            }
        }
        let mode = layer_mode(params.f(graph::MODE) as usize);
        let group = settings.n("ADBE Blend Interior") != 0.;
        let apply_fill = |target: &mut dyn EffectTarget, top: &GpuImage, amount: f32| {
            SHADER.dispatch_at(
                "main",
                target,
                params,
                work,
                source,
                [Some(&exterior), Some(top)],
                [9, mode],
                |w, h| {
                    w.f32(h + 200 * 16, amount);
                },
            )
        };
        let mut result = if group {
            if fill == 1. {
                source.clone()
            } else {
                SHADER.dispatch_at("main", target, params, work, source, [], [0, 0], |w, h| {
                    w.f32(h + 200 * 16, fill);
                })?
            }
        } else {
            apply_fill(target, source, fill)?
        };
        for (index, stage) in [(8, 2), (7, 1), (6, 3)] {
            let s = Style::new(params, index);
            if s.on() {
                result = coating(target, s, stage, source, &result, work, None)?;
            }
        }
        for (index, kind) in EDGES[2..].iter().copied() {
            let s = Style::new(params, index);
            if s.on() {
                result = edge(
                    target,
                    s,
                    kind,
                    source,
                    &result,
                    work,
                    Paint {
                        distance: distance.as_ref(),
                        ..Default::default()
                    },
                )?;
            }
        }
        if group {
            result = apply_fill(target, &result, 1.)?;
        }
        if stroke.on() {
            result = coating(target, stroke, 4, source, &result, work, distance.as_ref())?;
        }
        let channels = [
            "ADBE R Channel Blend",
            "ADBE G Channel Blend",
            "ADBE B Channel Blend",
        ]
        .map(|k| settings.n(k) as f32);
        let opacity = params
            .value(graph::OPACITY)
            .and_then(Value::as_f64)
            .unwrap_or(1.) as f32;
        let preserve = params.b(graph::PRESERVE_ALPHA);
        let ranges = match params.value(SOURCE_BLEND_RANGES) {
            Some(Value::FloatList(r)) if settings.n("ADBE Blend Ranges") != 0. => &r[..],
            _ => &[],
        };
        if work != roi
            || matte.is_some()
            || opacity != 1.
            || preserve
            || channels != [1.; 3]
            || !ranges.is_empty()
        {
            result = SHADER.dispatch_at(
                "main",
                target,
                params,
                roi,
                source,
                [Some(&result), backdrop.as_ref(), matte.as_ref()],
                [7, params.f(graph::MATTE) as u32],
                |w, h| {
                    let count = (ranges.len() / 8).min(4);
                    w.f32s(h + 200 * 16, &[opacity, f32::from(preserve), count as f32])
                        .f32s(h + 201 * 16, &channels)
                        .f32s(
                            h + 202 * 16,
                            &[
                                work.x0 as f32,
                                work.y0 as f32,
                                work.x1 as f32,
                                work.y1 as f32,
                            ],
                        );
                    for (i, ends) in ranges.as_chunks::<4>().0.iter().take(count * 2).enumerate() {
                        w.f32s(h + (16 + i) * 16, &ends.map(|v| v as f32));
                    }
                },
            )?;
        }
        Ok(result)
    }
}

fn edge(
    target: &mut dyn EffectTarget,
    s: Style<'_>,
    kind: Kind,
    source: &GpuImage,
    base: &GpuImage,
    roi: IRect,
    paint: Paint<'_>,
) -> Result<GpuImage, RenderError> {
    let mut shading = s.shading(kind);
    if kind == Kind::Bevel && shading.bevel_style == 5 {
        let stroke = Style::new(s.p, 9);
        if !stroke.on() {
            return Ok(base.clone());
        }
        shading.width = stroke.length("size") as f32;
        shading.technique = shading.technique.max(2);
    }
    let colors = (s.n("AEColorChoice") == 2.)
        .then(|| gradient_lut(target, s, source))
        .transpose()?;
    shading.encode(
        target,
        s.p,
        source,
        base,
        roi,
        Paint {
            backdrop: paint.backdrop,
            ramp: colors.as_ref(),
            distance: if shading.technique > 1 {
                paint.distance
            } else {
                None
            },
        },
    )
}

fn distance_field(
    target: &mut dyn EffectTarget,
    params: Params<'_>,
    source: &GpuImage,
    roi: IRect,
    width: f64,
) -> Result<GpuImage, RenderError> {
    let radius = width.ceil() as u32 + 1;
    let region = roi.inflate(radius as i32);
    let mut field = SHADER.dispatch_at(
        "raw_values",
        target,
        params,
        region,
        source,
        [],
        [5, 0],
        |_, _| {},
    )?;
    let mut step = radius.next_power_of_two();
    while step > 0 {
        field = SHADER.dispatch_at(
            "raw_values",
            target,
            params,
            region,
            source,
            [None, Some(&field)],
            [6, step],
            |_, _| {},
        )?;
        step /= 2;
    }
    Ok(field)
}

fn coating(
    target: &mut dyn EffectTarget,
    s: Style<'_>,
    stage: u32,
    source: &GpuImage,
    base: &GpuImage,
    roi: IRect,
    distance: Option<&GpuImage>,
) -> Result<GpuImage, RenderError> {
    let mut extra = None;
    let gradient = (stage == 2)
        .then(|| gradient_lut(target, s, source))
        .transpose()?;
    let index = match stage {
        2 => s.n("type") as u32,
        3 => {
            let sigma = s.length("blur") * 0.45;
            let sample = roi.inflate(s.length("distance").ceil() as i32 + 1);
            let grid = image_grid::grid_scale(s.p.p, source);
            let mut alpha = source.clone();
            alpha.rect = image_grid::pixel_rect(source.rect, 1. / grid);
            alpha.pixel_scale = 1.;
            if blur::radius(sigma) > 0 {
                alpha = blur::plan_gaussian(alpha.rect, [sigma; 2], Edge::Transparent, sample)
                    .encode(target, &alpha)?;
            }
            alpha.rect = image_grid::pixel_rect(alpha.rect, grid);
            alpha.pixel_scale *= grid;
            extra = Some(alpha);
            s.n("invert") as u32
        }
        4 => {
            extra = distance.cloned();
            s.n("style") as u32
        }
        _ => 0,
    };
    SHADER.dispatch_at(
        "main",
        target,
        s.p,
        roi,
        source,
        [Some(base), extra.as_ref(), gradient.as_ref()],
        [stage, index],
        |w, h| {
            w.f32s(
                h + 200 * 16,
                &[
                    (s.n("opacity") * 0.01) as f32,
                    blend(s.n("mode2")) as f32,
                    s.length("size") as f32,
                    0.,
                ],
            )
            .f32s(h + 201 * 16, &s.color("color"));
            if stage == 3 {
                w.f32s(h + 202 * 16, &s.offset().map(|v| v as f32));
            }
            if stage == 2 {
                let rect = match (s.n("align") != 0., s.p.value(graph::COMP_SIZE)) {
                    (false, Some(Value::Vec2(size))) => IRect::from_size(
                        (size[0] * s.p.p.scale).ceil() as u32,
                        (size[1] * s.p.p.scale).ceil() as u32,
                    ),
                    _ if s.p.p.layer_bounds.is_empty() => image_grid::pixel_rect(
                        source.rect,
                        1. / image_grid::grid_scale(s.p.p, source),
                    ),
                    _ => image_grid::layer_rect(s.p.p),
                };
                let [width, height] = rect.size().map(|v| v as f32);
                let offset = match s.val("offset") {
                    Some(Value::Vec2(v)) => v.map(|x| (x * s.p.p.scale) as f32),
                    _ => [0.; 2],
                };
                let zoom = (s.n("scale") * 0.01) as f32;
                let angle = s.n("angle").to_radians();
                w.f32s(
                    h + 202 * 16,
                    &[
                        rect.x0 as f32 + width * 0.5 + offset[0],
                        rect.y0 as f32 + height * 0.5 + offset[1],
                        width * zoom,
                        height * zoom,
                    ],
                )
                .f32s(
                    h + 203 * 16,
                    &[
                        angle.cos() as f32,
                        -angle.sin() as f32,
                        s.n("reverse") as f32,
                        0.,
                    ],
                );
            }
        },
    )
}
