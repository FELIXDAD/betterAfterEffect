use super::*;
use bae_model::Time;
use bae_render::effect::{LayerDependency, MaskSelect};

pub(super) struct RadioWaves {
    pub(super) def: EffectDef,
}
const MAX_WAVES: usize = 128;
pub(super) const PASS: Pass = Pass {
    label: "bae.effects.radio-waves",
    shader: include_str!("waves.wgsl"),
    bytes: 160 + 64 * MAX_WAVES as u32,
};

impl RadioWaves {
    fn births(&self, p: &EffectParams<'_>) -> Vec<Time> {
        let v = Params::new(&self.def, p);
        let time = p.time.as_seconds();
        let rate = v.f(34);
        if rate <= 0. || time < 0. || v.f(56) <= 0. {
            return Vec::new();
        }
        let last = (time * rate).floor() as i64;
        let first = ((time - v.f(56)) * rate).ceil().max(0.) as i64;
        (first.max(last - MAX_WAVES as i64 + 1)..=last)
            .map(|i| Time::from_seconds(i as f64 / rate))
            .collect()
    }
}
impl Effect for RadioWaves {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn parameter_ids(&self) -> &'static [u32] {
        &[4, 36, 38, 40, 42, 44, 46, 50, 52, 54, 56, 58, 60, 62]
    }
    fn parameter_times(&self, p: &EffectParams<'_>) -> Vec<Time> {
        if Params::new(&self.def, p).popup(6) == 1 {
            self.births(p)
                .into_iter()
                .filter(|&t| t != p.time)
                .collect()
        } else {
            Vec::new()
        }
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        if Params::new(&self.def, p).popup(2) == 2 {
            layer_input::dependency(p, 18, true).into_iter().collect()
        } else {
            Vec::new()
        }
    }
    fn mask_dependencies(&self, p: &EffectParams<'_>) -> Vec<MaskSelect> {
        match (Params::new(&self.def, p).popup(2), p.value(66)) {
            (3, Some(Value::PrimRef(path))) => vec![MaskSelect::One(path.clone())],
            _ => vec![],
        }
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
impl TargetEffect for RadioWaves {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let source = self
            .layer_dependencies(p)
            .first()
            .and_then(|d| target.effect_layer(d))
            .or_else(|| {
                self.mask_dependencies(p)
                    .first()
                    .and_then(|m| target.effect_mask(m))
            })
            .cloned();
        let births = self.births(p);
        let mut u = UniformWriter::with_size(PASS.bytes as usize);
        u.f32s(
            80,
            &[
                births.len() as f32,
                v.popup(2) as f32,
                v.f(8) as f32,
                v.popup(48) as f32,
            ],
        );
        u.f32s(
            96,
            &[
                v.f(10) as f32,
                v.f(12) as f32,
                if v.b(14) { v.f(16) as f32 } else { 0. },
                v.f(64) as f32,
            ],
        );
        u.f32s(112, &point(v, 20))
            .f32(120, v.popup(22) as f32)
            .f32(124, v.f(26) as f32 / 255.);
        u.f32s(
            128,
            &[
                v.f(28) as f32 * p.scale as f32,
                v.f(30) as f32,
                v.f(32) as f32,
                f32::from(v.b(24)),
            ],
        );
        let rect = if p.layer_bounds.is_empty() {
            input.rect
        } else {
            image_grid::layer_rect(p)
        };
        u.f32s(
            144,
            &[
                rect.width() as f32,
                rect.height() as f32,
                f32::from(source.is_some()),
                0.,
            ],
        );
        for (index, birth) in births.into_iter().enumerate() {
            let at = v.at_time(birth);
            let age = (p.time - birth).as_seconds().max(0.);
            let life = at.f(56).max(0.00001);
            let fade = (age / at.f(58).max(0.00001)).min(1.)
                * ((life - age) / at.f(60).max(0.00001)).clamp(0., 1.);
            let fade = if age == 0. && at.f(58) == 0. {
                1.
            } else {
                fade
            };
            let index = 160 + index * 64;
            let mut origin = point(at, 4);
            let direction = at.f(42).to_radians();
            for (i, delta) in [direction.sin(), -direction.cos()].into_iter().enumerate() {
                origin[i] += (delta * at.f(40) * age * p.scale) as f32;
                if at.b(62) {
                    let limit = [rect.width(), rect.height()][i].max(1) as f32;
                    origin[i] = limit - (origin[i].rem_euclid(2. * limit) - limit).abs();
                }
            }
            u.f32s(index, &origin)
                .f32(index + 8, (at.f(36) * age * p.scale) as f32)
                .f32(
                    index + 12,
                    ((at.f(38) + at.f(44) * age).to_radians()) as f32,
                )
                .f32s(index + 16, &color(at, 46))
                .f32s(
                    index + 32,
                    &[
                        (at.f(54) * fade) as f32,
                        ((at.f(50) + (at.f(52) - at.f(50)) * (age / life).min(1.)) * p.scale)
                            as f32,
                        0.,
                        0.,
                    ],
                );
        }
        PASS.encode(target, p, [input, source.as_ref().unwrap_or(input)], roi, u)
    }
}
