use super::*;

pub(super) struct EyedropperFill {
    pub(super) def: EffectDef,
}

pub(super) const SAMPLE: Pass = Pass {
    label: "bae.effects.eyedropper.sample",
    shader: include_str!("eyedropper_sample.wgsl"),
    bytes: 112,
};
pub(super) const FILL: Pass = Pass {
    label: "bae.effects.eyedropper.fill",
    shader: include_str!("eyedropper_fill.wgsl"),
    bytes: 96,
};

impl Effect for EyedropperFill {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        let center = sample_point(p, p.layer_bounds);
        let radius = v.f(2);
        let [x, y] = center;
        roi.union(&IRect::new(
            ((x - radius) * p.scale).floor() as i32,
            ((y - radius) * p.scale).floor() as i32,
            ((x + radius) * p.scale).ceil() as i32 + 1,
            ((y + radius) * p.scale).ceil() as i32 + 1,
        ))
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

fn sample_point(p: &EffectParams<'_>, bounds: IRect) -> [f64; 2] {
    if let Some(Value::Vec2(point)) = p.value(1) {
        *point
    } else {
        [
            (f64::from(bounds.x0) + f64::from(bounds.x1)) * 0.5,
            (f64::from(bounds.y0) + f64::from(bounds.y1)) * 0.5,
        ]
    }
}

impl TargetEffect for EyedropperFill {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let bounds = if p.layer_bounds.is_empty() {
            input.rect
        } else {
            p.layer_bounds
        };
        let grid = image_grid::grid_scale(p, input);
        let center = sample_point(p, bounds).map(|v| (v / grid) as f32);
        let mut u = UniformWriter::with_size(SAMPLE.bytes as usize);
        u.i32s(
            80,
            &image_grid::rect4(image_grid::pixel_rect(bounds, 1. / grid)),
        )
        .f32s(96, &center)
        .f32(104, (v.f(2) / grid) as f32)
        .u32(108, v.popup(3) as u32);
        let mean = SAMPLE.encode(target, p, [input, input], IRect::from_size(1, 1), u)?;
        let mut u = UniformWriter::with_size(FILL.bytes as usize);
        u.f32(80, v.f(5) as f32)
            .u32(84, u32::from(v.b(4)))
            .u32(88, v.popup(3) as u32)
            .u32(92, u32::from(p.float));
        FILL.encode(target, p, [input, &mean], roi, u)
    }
}
