use bae_model::Time;
use bae_render::{EffectParams, GpuImage, IRect, RenderError, effect::LayerDependency};

use crate::{EffectTarget, effects::temporal::before_effect, gpu::UniformWriter};

const MAX_FRAMES_PER_SIDE: i64 = 48;

pub(super) struct Smoothing {
    sides: [Vec<(LayerDependency, f32)>; 2],
    detect: bool,
}

impl Smoothing {
    pub(super) fn new(p: &EffectParams<'_>, seconds: f64, detect: bool) -> Self {
        let half = Time::from_seconds(seconds * 0.5 * p.layer_time_scale.abs());
        let radius = (half.as_seconds() * p.frame_rate.as_f64()).round() as i64;
        // Wide windows sample uniformly, with the first comparison against
        // the adjacent frame so scene detection can stop at an immediate cut.
        let stride = ((radius + MAX_FRAMES_PER_SIDE - 1) / MAX_FRAMES_PER_SIDE).max(1);
        Self {
            sides: [-1, 1].map(|direction| {
                (1..=radius)
                    .step_by(stride as usize)
                    .filter_map(|distance| {
                        let time = p.comp_time + p.frame_rate.frame_to_time(direction * distance);
                        before_effect(p, time).map(|dep| {
                            let weight = 1. - distance as f32 / (radius + 1) as f32;
                            (dep, weight)
                        })
                    })
                    .collect()
            }),
            detect,
        }
    }

    pub(super) fn dependencies(self) -> Vec<LayerDependency> {
        self.sides
            .into_iter()
            .flatten()
            .map(|(dep, _)| dep)
            .collect()
    }

    pub(super) fn blend(
        &self,
        target: &mut dyn EffectTarget,
        input: &GpuImage,
        mut stats: GpuImage,
        mut measure: impl FnMut(&mut dyn EffectTarget, &GpuImage) -> Result<GpuImage, RenderError>,
    ) -> Result<GpuImage, RenderError> {
        let width = stats.texel_size()[0];
        let mut u = UniformWriter::with_size(16);
        u.u32(0, width);
        for side in &self.sides {
            let mut gate: Option<GpuImage> = None;
            let mut previous = input.clone();
            for (dep, weight) in side {
                let Some(image) = target.effect_layer(dep).cloned() else {
                    continue;
                };
                if self.detect {
                    let next_gate = target.allocate(IRect::from_size(1, 1), 1.)?;
                    u.u32(4, u32::from(gate.is_some()));
                    super::dispatch(
                        target,
                        "scene_compare",
                        &u,
                        &[&previous, &image, gate.as_ref().unwrap_or(input)],
                        &next_gate,
                        [8, 8],
                    )?;
                    gate = Some(next_gate);
                    previous = image.clone();
                }

                let sample = measure(target, &image)?;
                let combined = target.allocate(IRect::from_size(width + 1, 1), 1.)?;
                u.u32(4, u32::from(gate.is_some())).f32(8, *weight);
                super::dispatch(
                    target,
                    "mix_temporal_stats",
                    &u,
                    &[&stats, &sample, gate.as_ref().unwrap_or(input)],
                    &combined,
                    [width + 1, 1],
                )?;
                stats = combined;
            }
        }
        Ok(stats)
    }
}
