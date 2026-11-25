use crate::{
    Builtin, EffectTarget, Registry, TargetEffect,
    catalog::DefBuilder,
    effects::{layer_input, sampled::Shader},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::Value;
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderComp, RenderEffect, RenderError,
    RenderLayer, effect::LayerDependency,
};
use std::sync::Arc;

static SHADER: Shader = Shader {
    label: "bae.effects.perspective.cc",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("cc.wgsl")
    ),
};

pub(super) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry.effects.extend([cc_environment()]);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}

fn cc_environment() -> Builtin {
    let mut b = DefBuilder::new("CC Environment", "CC Environment", "Perspective");
    b = b.layer(1, "Environment", "Environment").layer_stage(1);
    b = b.popup(
        4,
        "Mapping",
        "Mapping",
        1,
        &["Spherical", "Probe", "Vertical Cross"],
    );
    b = b.slider(
        2,
        "Horizontal Pan",
        "Horizontal Pan",
        0.0,
        [-10000.0, 10000.0],
        [-10000.0, 10000.0],
        2,
        "none",
    );
    b = b.checkbox(3, "Filter Environment", "Filter Environment", false);
    b = b.slider(
        5,
        "Lens Distortion",
        "Lens Distortion",
        0.0,
        [-200.0, 500.0],
        [-200.0, 500.0],
        2,
        "none",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (Arc::new(Environment(def)), labels),
        "cc-environment",
        "CC Environment",
        &["CC Environment", "透视"],
    )
}

const BASIS: u32 = u32::MAX - 2;
struct Environment(bae_model::EffectDef);

impl Effect for Environment {
    fn def(&self) -> &bae_model::EffectDef {
        &self.0
    }
    fn prepare_scene(&self, instance: &mut RenderEffect, comp: &RenderComp, _: &RenderLayer) {
        let camera = comp.view_camera();
        let mut basis = Vec::with_capacity(12);
        for v in [
            camera.world.x_axis,
            camera.world.y_axis,
            camera.world.z_axis,
        ] {
            basis.extend(v.truncate().normalize_or_zero().to_array());
            basis.push(camera.zoom);
        }
        instance.params.push((BASIS, Value::FloatList(basis)));
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        layer_input::dependency(p, 1, true).into_iter().collect()
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
impl TargetEffect for Environment {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let map = layer_input::dependency(p, 1, true)
            .and_then(|d| target.effect_layer(&d).cloned())
            .or_else(|| (p.layer_bounds.is_empty() && p.value(1).is_none()).then(|| input.clone()));
        let width = if p.layer_bounds.is_empty() {
            input.rect.width()
        } else {
            p.layer_bounds.width()
        };
        let zoom = f64::from(width) * 50. / 36.;
        let mut rows = [
            [1., 0., 0., zoom as f32],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
        ];
        if let Some(Value::FloatList(data)) = p.value(BASIS) {
            for (row, values) in rows.iter_mut().zip(data.as_chunks::<4>().0) {
                for (out, &v) in row.iter_mut().zip(values) {
                    *out = v as f32;
                }
            }
        }
        SHADER.dispatch_with_uniforms(
            target,
            Params::new(&self.0, p),
            roi,
            input,
            [map.as_ref()],
            [0, 0],
            |u, header| {
                for (i, row) in rows.iter().enumerate() {
                    u.f32s(header + (61 + i) * 16, row);
                }
            },
        )
    }
}
