use crate::{
    Builtin, EffectTarget, TargetEffect,
    catalog::DefBuilder,
    effects::{image_grid, sampled::Shader, temporal::before_effect},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Time, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

static SHADER: Shader = Shader {
    label: "bae.effects/color-stabilizer",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("deflicker.wgsl")
    ),
};
pub(super) fn prewarm(rt: &gpu::Runtime, fmt: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel_at(rt, fmt, "measure")?;
    SHADER.kernel(rt, fmt)?;
    Ok(())
}
pub(super) fn builtin() -> Builtin {
    let mut b = DefBuilder::new("ADBE Deflicker", "Color Stabilizer", "Color Correction").popup(
        1,
        "Stabilize",
        "稳定",
        2,
        &["Brightness", "Levels", "Curves"],
    );
    for (id, en, zh, x) in [
        (2, "Black Point", "黑场", 128.),
        (3, "Mid Point", "中点", 320.),
        (4, "White Point", "白场", 512.),
    ] {
        b.push(id, en, zh, ParamKind::Point, Some(Value::Vec2([x, 180.])));
    }
    let (def, labels) = b
        .slider(
            5,
            "Sample Size",
            "样本大小",
            3.,
            [1., 50.],
            [1., 50.],
            1,
            "percent",
        )
        .reference_points()
        .build();
    let mut item = rust_builtin(
        (Arc::new(Deflicker { def }), labels),
        "color-stabilizer",
        "颜色稳定器",
        &["deflicker", "flicker", "去闪烁"],
    );
    item.info
        .param_options_zh
        .push((1, ["亮度", "色阶", "曲线"].map(str::to_owned).to_vec()));
    item
}
struct Deflicker {
    def: EffectDef,
}
fn pivot(p: &EffectParams<'_>) -> Time {
    p.value(9001)
        .and_then(Value::as_time)
        .map_or(p.layer_in, |local| {
            p.comp_time + Time::from_seconds((local - p.time).as_seconds() * p.layer_time_scale)
        })
}
impl Effect for Deflicker {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        roi.union(&image_grid::layer_rect(p))
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        let t = pivot(p);
        if t == p.comp_time {
            return vec![];
        }
        before_effect(p, t).into_iter().collect()
    }
    fn parameter_sample_times(&self, p: &EffectParams<'_>) -> Vec<Time> {
        let t = pivot(p);
        if t == p.comp_time { vec![] } else { vec![t] }
    }
    fn render(
        &self,
        c: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        i: &GpuImage,
        r: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(c, p, i, r)
    }
}
impl TargetEffect for Deflicker {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let Some(reference) = self
            .layer_dependencies(p)
            .first()
            .and_then(|d| target.effect_layer(d))
            .cloned()
        else {
            return Ok(input.clone());
        };
        let v = Params::new(&self.def, p);
        let pivot_values = p
            .history
            .iter()
            .find(|(t, _)| *t == pivot(p))
            .map(|(_, e)| e);
        let stats = SHADER.dispatch_at(
            "measure",
            target,
            v,
            IRect::from_size(3, 2),
            input,
            [Some(&reference)],
            [0, 0],
            |w, h| {
                for id in 2..=4 {
                    let authored = pivot_values
                        .and_then(|e| e.param(id))
                        .or_else(|| v.value(id));
                    if let Some(Value::Vec2(pt)) = authored {
                        let pt = if p.point(id).is_none()
                            && pivot_values.and_then(|e| e.param(id)).is_none()
                        {
                            [
                                p.layer_bounds.x0 as f64
                                    + pt[0] * p.layer_bounds.width() as f64 / 640.,
                                p.layer_bounds.y0 as f64
                                    + pt[1] * p.layer_bounds.height() as f64 / 360.,
                            ]
                        } else {
                            *pt
                        };
                        w.f32s(
                            h + (id as usize + 10) * 16,
                            &pt.map(|x| (x * p.scale) as f32),
                        );
                    }
                }
                w.f32(
                    h + 15 * 16,
                    pivot_values
                        .and_then(|e| e.param(5))
                        .and_then(Value::as_f64)
                        .unwrap_or(v.f(5)) as f32,
                );
            },
        )?;
        SHADER.dispatch(target, v, roi, input, [Some(&stats)], [0, 0])
    }
}
