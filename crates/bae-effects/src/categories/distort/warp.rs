use crate::{
    Builtin, EffectTarget, TargetEffect,
    effects::{image_grid, layer_input, sampled::Shader, temporal::before_effect},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{
    EffectDef, Time, Value,
    effect::stabilizer::{CELLS, MOTION},
};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;
#[path = "warp_definition.rs"]
mod definition;
static SHADER: Shader = Shader {
    label: "bae.effects/warp-stabilizer",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("warp.wgsl")
    ),
};
pub(super) fn prewarm(rt: &gpu::Runtime, fmt: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel_at(rt, fmt, "border")?;
    SHADER.kernel(rt, fmt)?;
    Ok(())
}
pub(super) fn builtin() -> Builtin {
    let (mut def, labels) = definition::definition().build();
    for param in &mut def.params {
        if matches!(param.id, 1 | 11 | 305) {
            param.flags.hidden = true;
        }
    }
    let mut item = rust_builtin(
        (Arc::new(Warp { def }), labels),
        "warp-stabilizer",
        "变形稳定器",
        &["stabilize", "tracking", "稳定", "防抖"],
    );
    item.info.param_options_zh = definition::options();
    item
}
struct Warp {
    def: EffectDef,
}
fn motion(v: Option<&Value>) -> Option<&[f64]> {
    if let Some(Value::FloatList(data)) = v
        && data.len() == CELLS * 2
    {
        Some(data)
    } else {
        None
    }
}
impl Warp {
    fn edge_times(&self, p: &EffectParams<'_>) -> Vec<Time> {
        let v = Params::new(&self.def, p);
        if v.popup(22) != 4 || v.f(38) <= 0. || motion(p.value(MOTION)).is_none() {
            return vec![];
        }
        let span = Time::from_seconds(v.f(38) * p.layer_time_scale.abs());
        let step = Time(span.0.min(p.time_step.0).max(1));
        [
            p.comp_time - step,
            p.comp_time + step,
            p.comp_time - span,
            p.comp_time + span,
        ]
        .into_iter()
        .filter(|t| *t >= p.layer_in)
        .collect()
    }
}
impl Effect for Warp {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        roi.union(&image_grid::layer_rect(p))
    }
    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        let Some(raw) = motion(p.value(MOTION)) else {
            return input;
        };
        if v.popup(22) != 1 {
            return input;
        }
        let shift = raw.iter().copied().map(f64::abs).fold(0., f64::max) * p.scale;
        let extra =
            (v.f(16) / 100. - 1.).max(0.) * f64::from(input.width().max(input.height())) * 0.5;
        let pad = (shift * 2. + extra).ceil() as i32;
        IRect::new(
            input.x0.saturating_sub(pad),
            input.y0.saturating_sub(pad),
            input.x1.saturating_add(pad),
            input.y1.saturating_add(pad),
        )
    }
    fn parameter_ids(&self) -> &'static [u32] {
        &[MOTION]
    }
    fn parameter_times(&self, p: &EffectParams<'_>) -> Vec<Time> {
        let v = Params::new(&self.def, p);
        if motion(p.value(MOTION)).is_none() || v.popup(19) != 1 || v.f(20) == 0. {
            return vec![];
        }
        let radius = (p.frame_rate.as_f64() * 0.45 * (v.f(20) / 50.)).max(1.);
        let step = p.time_step.0 as f64 / p.layer_time_scale.abs().max(0.001);
        (-4..=4)
            .filter(|i| *i != 0)
            .map(|i| p.time + Time((i as f64 * radius / 4.).round() as i64 * step.round() as i64))
            .collect()
    }
    fn parameter_sample_times(&self, p: &EffectParams<'_>) -> Vec<Time> {
        self.edge_times(p)
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        let v = Params::new(&self.def, p);
        if v.popup(47) >= 5 {
            return layer_input::dependency(p, 46, false).into_iter().collect();
        }
        self.edge_times(p)
            .into_iter()
            .filter_map(|t| before_effect(p, t))
            .collect()
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
impl TargetEffect for Warp {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let Some(raw) = motion(p.value(MOTION)) else {
            return Ok(input.clone());
        };
        let v = Params::new(&self.def, p);
        let mut correction = [0.; CELLS * 2];
        let mut weights = 1.;
        if v.popup(19) == 1 {
            for sample in p.parameter_samples {
                if let Some(values) = motion(
                    sample
                        .values
                        .iter()
                        .find(|(id, _)| *id == MOTION)
                        .map(|x| &x.1),
                ) {
                    for (i, x) in correction.iter_mut().enumerate() {
                        *x += raw[i] - values[i];
                    }
                    weights += 1.;
                }
            }
        } else {
            correction.copy_from_slice(raw);
        }
        let strength = if v.popup(19) == 2 || !matches!(v.popup(22), 2 | 3) {
            1.
        } else {
            0.25 + 0.0075 * v.f(23)
        };
        let sign = if v.popup(47) >= 4 { -1. } else { 1. };
        for c in &mut correction {
            *c *= sign * strength * p.scale / weights;
        }
        let unmasked = correction;
        for i in 0..CELLS {
            use bae_model::effect::stabilizer::{DELETED, excluded};
            let deleted = p.value(DELETED);
            if !excluded(deleted, i, p.time) {
                continue;
            }
            let mut sum = [0.; 2];
            let mut total = 0.;
            for j in 0..CELLS {
                if excluded(deleted, j, p.time) {
                    continue;
                }
                let distance = (i % 3).abs_diff(j % 3).pow(2) + (i / 3).abs_diff(j / 3).pow(2);
                let weight = 1. / distance.max(1) as f64;
                for axis in 0..2 {
                    sum[axis] += weight * unmasked[j * 2 + axis];
                }
                total += weight;
            }
            for axis in 0..2 {
                correction[i * 2 + axis] = sum[axis] / total.max(1e-6);
            }
        }
        let mut maps = [None, None, None, None];
        let deps = self.layer_dependencies(p);
        for (slot, dep) in maps.iter_mut().zip(&deps) {
            *slot = target.effect_layer(dep).cloned();
        }
        let write_grid = |w: &mut gpu::UniformWriter, header: usize| {
            for (i, cell) in correction.as_chunks::<2>().0.iter().enumerate() {
                w.f32s(header + (100 + i) * 16, &cell.map(|v| v as f32));
            }
        };
        if matches!(v.popup(22), 2 | 3) {
            maps[3] = Some(SHADER.dispatch_at(
                "border",
                target,
                v,
                IRect::from_size(1, 1),
                input,
                [],
                [0, 0],
                write_grid,
            )?);
        }
        SHADER.dispatch_at(
            "main",
            target,
            v,
            roi,
            input,
            maps.each_ref().map(Option::as_ref),
            [0, 0],
            |w, header| {
                write_grid(w, header);
                for (i, dep) in deps.iter().enumerate().take(4) {
                    if let Some(other) = p
                        .history
                        .iter()
                        .find(|(t, _)| *t == dep.time)
                        .and_then(|(_, e)| motion(e.param(MOTION)))
                    {
                        for cell in 0..CELLS {
                            w.f32s(
                                header + (110 + i * CELLS + cell) * 16,
                                &[
                                    (p.scale * (other[cell * 2] - raw[cell * 2])) as f32,
                                    (p.scale * (other[cell * 2 + 1] - raw[cell * 2 + 1])) as f32,
                                ],
                            );
                        }
                    }
                }
            },
        )
    }
}
