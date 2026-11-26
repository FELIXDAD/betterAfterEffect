//! Legacy matte controls drive the modern keyers and shared alpha filters.
use super::*;
use crate::effects::{
    image_grid,
    morphology::{self, Extremum},
    sampled::Shader,
};
use crate::{EffectTarget, Form, Implementation, TargetEffect, catalog::DefBuilder};
use crate::{blur, gpu};
use bae_gpu::wgpu;

pub(super) fn register(target: &Builtin, out: &mut Vec<Builtin>) {
    let fx = match target.info.match_name.as_str() {
        "ADBE RefineMatte2" => refine(target),
        "ADBE Linear Color Key2" => color(&with_edges(target)),
        "ADBE Extract" => luma(&with_edges(target)),
        _ => return,
    };
    out.push(fx);
}

fn refine(target: &Builtin) -> Builtin {
    let mut alias = Definition::inherit(
        target,
        "ADBE RefineMatte",
        "refine-key-legacy",
        "Refine Matte",
        "调整遮罩",
    );
    let mut b = DefBuilder::new("ADBE RefineMatte", "Refine Matte", "Obsolete");
    b = b.slider(5, "Smooth", "平滑", 2., [0., 1000.], [0., 1000.], 2, "none");
    b = b.slider(
        6,
        "Feather",
        "羽化",
        20.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    b = b.slider(
        15,
        "Choke",
        "阻塞",
        0.,
        [-100., 100.],
        [-100., 100.],
        2,
        "percent",
    );
    b = b.slider(
        16,
        "Reduce Chatter",
        "减少震颤",
        50.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    b = b.checkbox(8, "Use Motion Blur", "使用运动模糊", true);
    b.push(17, "Motion Blur", "运动模糊", ParamKind::GroupStart, None);
    b = b.slider(
        9,
        "Samples Per Frame",
        "每帧样本",
        8.,
        [2., 128.],
        [2., 128.],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Shutter Angle",
        "快门角度",
        180.,
        [1., 720.],
        [1., 720.],
        2,
        "none",
    );
    b = b.checkbox(14, "Higher Quality", "较高品质", false);
    b.push(18, "Higher Quality", "较高品质", ParamKind::GroupEnd, None);
    b = b.checkbox(11, "Decontaminate Edge Colors", "净化边缘颜色", true);
    b.push(19, "Decontamination", "净化", ParamKind::GroupStart, None);
    b = b.slider(
        22,
        "Amount",
        "数量",
        100.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    b = b.checkbox(12, "Extend Where Smoothed", "扩展平滑的地方", false);
    b = b.slider(
        2,
        "Increase Radius",
        "增大半径",
        0.,
        [0., 80.],
        [0., 80.],
        2,
        "none",
    );
    b = b.checkbox(20, "View Map", "查看地图", false);
    b.push(21, "View Map", "查看地图", ParamKind::GroupEnd, None);
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh.clear();

    alias.finish(target, Some(map_refine))
}

fn color(target: &Builtin) -> Builtin {
    let mut alias = Definition::inherit(
        target,
        "ADBE Color Key",
        "color-key-legacy",
        "Color Key",
        "颜色键",
    );
    let mut b = DefBuilder::new("ADBE Color Key", "Color Key", "Obsolete");
    b.push(
        1,
        "Key Color",
        "主色",
        ParamKind::Color,
        Some(Value::Color([0., 0., 1., 1.])),
    );
    b = b.slider(
        2,
        "Color Tolerance",
        "颜色容差",
        0.,
        [0., 255.],
        [0., 255.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Edge Thin",
        "薄化边缘",
        0.,
        [-5., 5.],
        [-5., 5.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Edge Feather",
        "羽化边缘",
        0.,
        [0., 100.],
        [0., 100.],
        2,
        "none",
    );
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh.clear();

    alias.finish(target, Some(map_color))
}

fn luma(target: &Builtin) -> Builtin {
    let mut alias = Definition::inherit(
        target,
        "ADBE Luma Key",
        "luma-key-legacy",
        "Luma Key",
        "亮度键",
    );
    let mut b = DefBuilder::new("ADBE Luma Key", "Luma Key", "Obsolete");
    b = b.popup(
        1,
        "Key Type",
        "键控类型",
        2,
        &[
            "Key Out Brighter",
            "Key Out Darker",
            "Key Out Similar",
            "Key Out Dissimilar",
        ],
    );
    b = b.slider(
        2,
        "Threshold",
        "阈值",
        0.,
        [0., 255.],
        [0., 255.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Tolerance",
        "容差",
        0.,
        [0., 255.],
        [0., 255.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Edge Thin",
        "薄化边缘",
        0.,
        [-5., 5.],
        [-5., 5.],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Edge Feather",
        "羽化边缘",
        0.,
        [0., 100.],
        [0., 100.],
        2,
        "none",
    );
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh.clear();
    alias.info.param_options_zh.push((
        1,
        [
            "抠出较亮区域",
            "抠出较暗区域",
            "抠出亮度相似的区域",
            "抠出亮度不同的区域",
        ]
        .map(String::from)
        .to_vec(),
    ));
    alias.finish(target, Some(map_luma))
}

fn map_refine(p: Params<'_>) -> Translation<'_> {
    // Both generations share the decontamination and shutter controls.
    Translation::patch(
        p,
        [
            (24, Some(Value::Bool(false))),
            (27, Some(Value::Float(p.f(5)))),
            (36, Some(Value::Float(p.f(6)))),
            (33, Some(Value::Float(-p.f(15)))),
            (35, Some(Value::Int(if p.f(16) > 0. { 3 } else { 1 }))),
            (29, Some(Value::Float(p.f(16)))),
            (8, Some(Value::Bool(p.b(8)))),
        ],
    )
}

const EDGE_THIN: u32 = 1001;
const EDGE_FEATHER: u32 = 1002;

fn map_color(p: Params<'_>) -> Translation<'_> {
    Translation::replace([
        (3, p.value(1).unwrap().clone()),
        (4, Value::Int(1)),
        (5, Value::Float(100. * p.f(2) / 255.)),
        (6, Value::Float(0.)),
        (EDGE_THIN, Value::Float(p.f(3))),
        (EDGE_FEATHER, Value::Float(p.f(4))),
    ])
}
fn map_luma(p: Params<'_>) -> Translation<'_> {
    let t = p.f(2);
    let d = p.f(3);
    // Extract retains the closed interval. Use one open end when rejecting
    // brighter/darker values, and invert the rejected band for Similar.
    let (lo, hi) = match p.popup(1) {
        1 => (0., t),
        2 => (t, 255.),
        _ => (t - d, t + d),
    };
    let directional = p.popup(1) <= 2;
    Translation::replace([
        (2, Value::Int(1)),
        (3, Value::Float(lo)),
        (4, Value::Float(hi)),
        (
            5,
            Value::Float(if directional && p.popup(1) == 2 {
                d
            } else {
                0.
            }),
        ),
        (
            6,
            Value::Float(if directional && p.popup(1) == 1 {
                d
            } else {
                0.
            }),
        ),
        (7, Value::Bool(p.popup(1) == 3)),
        (EDGE_THIN, Value::Float(p.f(4))),
        (EDGE_FEATHER, Value::Float(p.f(5))),
    ])
}

static REPLACE_ALPHA: Shader = Shader {
    label: "bae.effects.legacy_key_edges",
    source: concat!(
        include_str!("../effects/sampled.wgsl"),
        "fn evaluate(p:vec2f)->vec4f {return output(src(p).rgb,aux_a(p).a);}"
    ),
};
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    REPLACE_ALPHA.kernel(rt, format)?;
    Ok(())
}

/// Run the modern declarative key first. A legacy instance may additionally
/// erode/dilate and feather that key's alpha before it is applied to the RGB.
struct KeyEdges(Arc<bae_plugin::DeclarativeEffect>);
fn with_edges(target: &Builtin) -> Builtin {
    let mut result = target.clone();
    let Implementation::Declarative(key) = &target.implementation else {
        unreachable!("declarative key")
    };
    result.implementation = Implementation::Rust(Arc::new(KeyEdges(key.clone())));
    result.info.form = Form::Rust;
    result
}
impl KeyEdges {
    fn settings(&self, p: &EffectParams<'_>) -> (i32, f64) {
        let v = Params::new(self.def(), p);
        (
            (v.f(EDGE_THIN) * p.scale).round() as i32,
            v.f(EDGE_FEATHER) * p.scale * 0.5,
        )
    }
}
impl Effect for KeyEdges {
    fn def(&self) -> &EffectDef {
        self.0.def()
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let (thin, sigma) = self.settings(p);
        blur::plan_gaussian(roi, [sigma; 2], blur::Edge::Repeat, roi)
            .read
            .inflate(thin.abs())
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
impl TargetEffect for KeyEdges {
    fn encode(
        &self,
        t: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let (thin, sigma) = self.settings(p);
        if roi.is_empty() || (thin == 0 && blur::radius(sigma) == 0) {
            return t.declarative(&self.0, p, input, roi);
        }
        let grid = image_grid::grid_scale(p, input);
        let domain = image_grid::pixel_rect(input.rect, 1. / grid);
        let plan = blur::plan_gaussian(domain, [sigma; 2], blur::Edge::Repeat, roi);
        let key = t.declarative(
            &self.0,
            p,
            input,
            plan.read.inflate(thin.abs()).intersect(&domain),
        )?;
        let op = if thin > 0 {
            Extremum::Min(thin)
        } else {
            Extremum::Max(-thin)
        };
        let mut alpha = if thin == 0 {
            key
        } else {
            morphology::filter_alpha(t, p, &key, plan.read, &[op])?
        };
        if blur::radius(sigma) > 0 {
            alpha.rect = image_grid::pixel_rect(alpha.rect, 1. / grid);
            alpha.pixel_scale = 1.;
            alpha = plan.encode(t, &alpha)?;
            alpha.rect = image_grid::pixel_rect(alpha.rect, grid);
            alpha.pixel_scale = grid;
        }
        REPLACE_ALPHA.dispatch(
            t,
            Params::new(self.def(), p),
            roi,
            input,
            [Some(&alpha)],
            [0, 0],
        )
    }
}
