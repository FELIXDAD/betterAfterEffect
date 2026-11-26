//! Translate the old generator into Advanced Lightning, then composite its bolts.
use super::*;
use crate::catalog::DefBuilder;
use crate::{
    EffectTarget, Form, Implementation, TargetEffect,
    effects::{image_grid, sampled::Shader},
    gpu,
    params::Params,
};
use bae_gpu::wgpu;

static COMPOSE: Shader = Shader {
    label: "bae.effects.legacy_lightning_blend",
    source: concat!(
        include_str!("../effects/sampled.wgsl"),
        include_str!("../shaders/generated_blend.wgsl"),
        "
fn bae_unpremultiply(c: vec4f) -> vec4f { return fx_unpremult(c); }
fn evaluate(p: vec2f) -> vec4f {
    let background = fx_sample(input_tex, u.images[0], p, false);
    let bolt = aux_a(p);
    return generated_blend(background, bolt.rgb, bolt.a, u.stage);
}"
    ),
};
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    COMPOSE.kernel(rt, format)?;
    Ok(())
}
pub(super) fn register(target: &Builtin, out: &mut Vec<Builtin>) {
    if target.info.match_name != "ADBE Lightning 2" {
        return;
    }
    let Implementation::Declarative(shader) = &target.implementation else {
        unreachable!("Advanced Lightning is declarative")
    };
    let mut alias = Definition::inherit(
        target,
        "ADBE Lightning",
        "lightning-legacy",
        "Lightning",
        "闪光",
    );
    let mut b = DefBuilder::new("ADBE Lightning", "Lightning", "Obsolete");
    b.push(
        1,
        "Start Point",
        "起始点",
        ParamKind::Point,
        Some(Value::Vec2([160., 180.])),
    );
    b.push(
        2,
        "End Point",
        "结束点",
        ParamKind::Point,
        Some(Value::Vec2([480., 180.])),
    );
    b = b.slider(3, "Segments", "区段", 7., [3., 25.], [3., 25.], 2, "none");
    b = b.slider(4, "Amplitude", "振幅", 10., [0., 50.], [0., 50.], 2, "none");
    b = b.slider(
        5,
        "Detail Level",
        "细节级别",
        2.,
        [0., 8.],
        [0., 8.],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Detail Amplitude",
        "细节振幅",
        0.299988,
        [0., 1.],
        [0., 1.],
        2,
        "none",
    );
    b = b.slider(
        7,
        "Branching",
        "设置分支",
        0.199997,
        [0., 1.],
        [0., 1.],
        2,
        "none",
    );
    b = b.slider(
        8,
        "Rebranching",
        "再分支",
        0.099991,
        [0., 1.],
        [0., 1.],
        2,
        "none",
    );
    b = b.slider(
        9,
        "Branch Angle",
        "分支角度",
        20.,
        [0., 60.],
        [0., 60.],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Branch Segment Length",
        "分支线段长度",
        0.5,
        [0., 10.],
        [0., 10.],
        2,
        "none",
    );
    b = b.slider(
        11,
        "Branch Segments",
        "分支线段",
        4.,
        [0., 40.],
        [0., 40.],
        2,
        "none",
    );
    b = b.slider(
        12,
        "Branch Width",
        "分支宽度",
        0.699997,
        [0., 10.],
        [0., 10.],
        2,
        "none",
    );
    b = b.slider(13, "Speed", "速度", 10., [1., 20.], [1., 20.], 2, "none");
    b = b.slider(
        14,
        "Stability",
        "稳定性",
        0.299988,
        [0., 10.],
        [0., 10.],
        2,
        "none",
    );
    b = b.checkbox(15, "Fixed Endpoint", "固定端点", true);
    b = b.slider(
        16,
        "Width",
        "宽度",
        10.,
        [1., 100.],
        [1., 100.],
        2,
        "percent",
    );
    b = b.slider(
        17,
        "Width Variation",
        "宽度变化",
        0.,
        [0., 1.],
        [0., 1.],
        2,
        "none",
    );
    b = b.slider(
        18,
        "Core Width",
        "核心宽度",
        0.399994,
        [0., 1.],
        [0., 1.],
        2,
        "none",
    );
    b.push(
        19,
        "Outside Color",
        "外部颜色",
        ParamKind::Color,
        Some(Value::Color([0., 0., 1., 0.])),
    );
    b.push(
        20,
        "Inside Color",
        "内部颜色",
        ParamKind::Color,
        Some(Value::Color([1., 1., 1., 1.])),
    );
    b = b.slider(
        21,
        "Pull Force",
        "拉力",
        0.,
        [0., 1000.],
        [0., 1000.],
        2,
        "none",
    );
    b.push(
        22,
        "Pull Direction",
        "拉力方向",
        ParamKind::Angle,
        Some(Value::Float(0.)),
    );
    b = b.slider(
        23,
        "Random Seed",
        "随机植入",
        1.,
        [0., 100.],
        [0., 100.],
        2,
        "none",
    );
    b = b.popup(
        24,
        "Blending Mode",
        "混合模式",
        2,
        &["Normal", "Add", "Screen"],
    );
    b = b.checkbox(25, "Rerun at Each Frame", "模拟", false);
    b = b.reference_points();
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh = vec![(
        24,
        ["正常", "相加", "滤色"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    )];
    let mut adapter = target.clone();
    adapter.implementation = Implementation::Rust(Arc::new(Bolt {
        source: alias.def.clone(),
        shader: shader.clone(),
    }));
    alias.info.form = Form::Rust;
    out.push(alias.finish(&adapter, None));
}
struct Bolt {
    source: EffectDef,
    shader: Arc<bae_plugin::DeclarativeEffect>,
}
impl Effect for Bolt {
    fn def(&self) -> &EffectDef {
        self.shader.def()
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
impl TargetEffect for Bolt {
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
        let v = Params::new(&self.source, p);
        let layer = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(
                input.rect,
                1. / (image_grid::grid_scale(p, input) * p.scale),
            )
        } else {
            p.layer_bounds
        };
        let point = |id| {
            let param = self.source.param(id).unwrap();
            if let Some(Value::Vec2(pt)) = p.value(id) {
                return *pt;
            }
            let Some(Value::Vec2(pt)) = &param.default else {
                unreachable!("point default")
            };
            let [x, y] = param
                .flags
                .point_default_space()
                .layer_pixels(*pt, [f64::from(layer.width()), f64::from(layer.height())]);
            [f64::from(layer.x0) + x, f64::from(layer.y0) + y]
        };
        let start = point(1);
        let mut end = point(2);
        let length = (end[0] - start[0]).hypot(end[1] - start[1]).max(1.);
        let time = p.time.as_seconds();
        let phase = time * v.f(13);
        let state = v.f(23) * 19.
            + if v.b(25) {
                (time / p.time_step.as_seconds().max(1e-6)).floor() * 17. + phase
            } else {
                phase
            };
        // The old amplitude is a percentage of layer width; the modern noise is relative to bolt length.
        let amplitude = v.f(4) * 0.01 * f64::from(layer.width());
        let pull = v.f(22).to_radians();
        let normal_pull = ((start[1] - end[1]) * pull.cos() + (end[0] - start[0]) * pull.sin())
            / length
            * v.f(21);
        let wobble = amplitude * (1. + v.f(14).min(10.) * 0.1);
        if !v.b(15) {
            end[0] += pull.cos() * v.f(21) + state.sin() * wobble;
            end[1] += pull.sin() * v.f(21) + state.cos() * wobble;
        }
        let width = v.f(16) * (1. + v.f(17) * 0.5 * state.sin());
        let color = |id| {
            let Some(Value::Color(c)) = v.value(id) else {
                unreachable!("color default")
            };
            Value::Color(*c)
        };
        let changes = [
            (1, Value::Int(if v.b(15) { 2 } else { 1 })),
            (2, Value::Vec2(start)),
            (3, Value::Vec2(end)),
            (4, Value::Float(state + normal_pull / length)),
            (6, Value::Float(width * v.f(18) * 0.5)),
            (7, Value::Float(100.)),
            (8, color(20)),
            (11, Value::Float(width * 0.5)),
            (12, Value::Float(100.)),
            (13, color(19)),
            (
                16,
                Value::Float(
                    ((wobble * (1. + v.f(6)) + normal_pull.abs()) / length * 8.).clamp(0., 10.),
                ),
            ),
            (17, Value::Float((v.f(7) * (1. + v.f(8))).min(1.))),
            (20, Value::Bool(false)),
            (22, Value::Float((v.f(3).log2() + v.f(5)).clamp(1., 10.))),
            (23, Value::Float((length / v.f(3)).max(1.))),
            (
                24,
                Value::Float((v.f(10) * v.f(11) / (0.22 * v.f(3)) * 100.).clamp(0.25, 100.)),
            ),
            (28, Value::Float((v.f(12) * 100.).min(100.))),
            (29, Value::Float(v.f(9) / 60. * 100.)),
        ]
        .map(|(id, value)| (id, Some(value)));
        let values = EffectValues::new(&[], &changes);
        let converted = EffectParams {
            translated_values: Some(&values),
            ..*p
        };
        let bolt = t.declarative(&self.shader, &converted, input, roi)?;
        let mode = match v.popup(24) {
            2 => 4,
            3 => 6,
            _ => 2,
        };
        COMPOSE.dispatch(t, v, roi, input, [Some(&bolt)], [mode, 0])
    }
}
