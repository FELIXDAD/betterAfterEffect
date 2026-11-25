use super::*;

pub(super) struct LightRays {
    pub(super) def: EffectDef,
    pub(super) burst: bool,
}
pub(super) const PASS: Pass = Pass {
    label: "bae.effects.light-rays",
    shader: include_str!("rays.wgsl"),
    bytes: 144,
};

impl Effect for LightRays {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        image_grid::layer_rect(p).union(&roi)
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
impl TargetEffect for LightRays {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let mut u = UniformWriter::with_size(PASS.bytes as usize);
        // Both AE parameter layouts feed one radial-integration pipeline.
        let (center, intensity, radius, span, angle, shape, tint, mode, halo, brighten, rgb) =
            if self.burst {
                (
                    1,
                    2,
                    0.,
                    v.f(3) * 0.02,
                    0.,
                    v.popup(4) + 2,
                    v.b(6),
                    2,
                    v.b(5),
                    true,
                    7,
                )
            } else {
                (
                    2,
                    1,
                    v.f(3) * p.scale,
                    v.f(4) * 0.02,
                    v.f(6).to_radians(),
                    v.popup(5),
                    !v.b(7),
                    v.popup(9),
                    true,
                    v.b(10),
                    8,
                )
            };
        u.f32s(80, &point(v, center))
            .f32(88, v.f(intensity) as f32 * 0.01)
            .f32(92, radius as f32)
            .f32s(96, &[span as f32, angle as f32, shape as f32, mode as f32])
            .f32s(112, &color(v, rgb))
            .f32s(
                128,
                &[
                    f32::from(tint),
                    f32::from(halo),
                    f32::from(brighten),
                    f32::from(p.float),
                ],
            );
        PASS.encode(target, p, [input, input], roi, u)
    }
}
