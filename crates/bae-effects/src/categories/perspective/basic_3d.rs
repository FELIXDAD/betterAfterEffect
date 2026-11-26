use crate::{
    EffectTarget, Registry, TargetEffect,
    catalog::DefBuilder,
    effects::{image_grid, sampled::Shader},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, RenderPurpose,
};
use std::sync::Arc;

static SHADER: Shader = Shader {
    label: "bae.effects.basic_3d",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("basic_3d.wgsl")
    ),
};

pub(super) fn register(r: &mut Registry) {
    let mut b = DefBuilder::new("ADBE Basic 3D", "Basic 3D", "Obsolete");
    for (id, en, zh) in [(1, "Swivel", "旋转"), (2, "Tilt", "倾斜")] {
        b.push(id, en, zh, ParamKind::Angle, Some(Value::Float(0.)));
    }
    let (def, names) = b
        .slider(
            3,
            "Distance to Image",
            "与图像的距离",
            0.,
            [-30000., 30000.],
            [-100., 100.],
            1,
            "percent",
        )
        .checkbox(4, "Specular Highlight", "镜面高光", false)
        .checkbox(5, "Preview", "预览", false)
        .build();
    r.effects.push(rust_builtin(
        (Arc::new(Plane(def)), names),
        "basic-3d",
        "基本 3D",
        &["swivel", "tilt", "透视"],
    ));
    r.prewarm.push(prewarm);
}
fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}
struct Plane(EffectDef);
impl Effect for Plane {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let layer = image_grid::layer_rect(p);
        if layer.is_empty() { roi } else { layer }
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
impl TargetEffect for Plane {
    fn encode(
        &self,
        t: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.0, p);
        let wire = v.b(5) && p.purpose == RenderPurpose::Preview;
        if roi.is_empty() || (!wire && !v.b(4) && [1, 2, 3].iter().all(|id| v.f(*id) == 0.)) {
            return Ok(input.clone());
        }
        SHADER.dispatch(t, v, roi, input, [], [u32::from(wire), 0])
    }
}
