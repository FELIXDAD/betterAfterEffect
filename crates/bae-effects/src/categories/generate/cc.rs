use crate::{
    Builtin, EffectTarget, Registry, TargetEffect, catalog::DefBuilder, effects::sampled::Shader,
    gpu, rust_builtin,
};
use crate::{effects::image_grid, params::Params};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Time, Value};
use bae_render::RenderError;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect};
use std::sync::Arc;

fn layer(p: Params<'_>, roi: IRect) -> IRect {
    roi.union(&image_grid::layer_rect(p.p))
}

static SHADER: Shader = Shader {
    label: "bae.effects.generate.cc",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../effects/surface_lights.wgsl"),
        include_str!("cc.wgsl")
    ),
};

pub(super) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry.effects.extend([cc_glue_gun()]);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}

fn cc_glue_gun() -> Builtin {
    let mut b = DefBuilder::new("CC Glue Gun", "CC Glue Gun", "Generate");
    b.push(
        1,
        "Brush Position",
        "Brush Position",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        2,
        "Stroke Width",
        "Stroke Width",
        4.0,
        [0.0, 1024.0],
        [0.0, 1024.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Density",
        "Density",
        25.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Time Span (sec)",
        "Time Span (sec)",
        0.0,
        [0.0, 3000.0],
        [0.0, 3000.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Reflection",
        "Reflection",
        100.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Strength",
        "Strength",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b.push(7, "Style", "Style", ParamKind::GroupStart, None);
    b = b.popup(8, "Paint Style", "Paint Style", 1, &["Plain", "Wobbly"]);
    b = b.slider(
        9,
        "Wobble Width",
        "Wobble Width",
        2.5,
        [0.0, 1024.0],
        [0.0, 1024.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Wobble Height",
        "Wobble Height",
        2.5,
        [0.0, 1024.0],
        [0.0, 1024.0],
        2,
        "none",
    );
    b = b.slider(
        11,
        "Wobble Speed",
        "Wobble Speed",
        100.0,
        [0.0, 30000.0],
        [0.0, 30000.0],
        2,
        "none",
    );
    b.push(12, "", "", ParamKind::GroupEnd, None);
    b.push(13, "Light", "Light", ParamKind::GroupStart, None);
    b = b.popup(28, "Using", "Using", 1, &["Effect Light", "AE Lights"]);
    b = b.slider(
        14,
        "Light Intensity",
        "Light Intensity",
        100.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(
        15,
        "Light Color",
        "Light Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        16,
        "Light Type",
        "Light Type",
        1,
        &["Distant Light", "Point Light"],
    );
    b = b.slider(
        17,
        "Light Height",
        "Light Height",
        65.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b.push(
        18,
        "Light Position",
        "Light Position",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 90.0])),
    );
    b.push(
        19,
        "Light Direction",
        "Light Direction",
        ParamKind::Angle,
        Some(Value::Float(-45.0)),
    );
    b.push(20, "", "", ParamKind::GroupEnd, None);
    b.push(21, "Shading", "Shading", ParamKind::GroupStart, None);
    b = b.slider(
        22,
        "Ambient",
        "Ambient",
        100.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        23,
        "Diffuse",
        "Diffuse",
        25.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        24,
        "Specular",
        "Specular",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        25,
        "Roughness",
        "Roughness",
        0.024994,
        [0.000992, 0.5],
        [0.000992, 0.5],
        4,
        "none",
    );
    b = b.slider(
        26,
        "Metal",
        "Metal",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(27, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (Arc::new(GlueGun(def)), labels),
        "cc-glue-gun",
        "CC Glue Gun",
        &["CC Glue Gun", "生成"],
    )
}

const MAX_SAMPLES: usize = 64;
const SAMPLE_BASE: usize = 169;

struct GlueGun(EffectDef);

impl Effect for GlueGun {
    fn def(&self) -> &EffectDef {
        &self.0
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            return roi;
        }
        layer(Params::new(&self.0, p), roi)
    }

    /// AE Lights: comp lights shade the beads (and key the frame cache).
    fn uses_scene(&self, p: &EffectParams<'_>) -> bool {
        self.scene_lights(p)
    }

    fn scene_lights(&self, p: &EffectParams<'_>) -> bool {
        Params::new(&self.0, p).popup(28) == 2
    }

    fn parameter_sample_times(&self, p: &EffectParams<'_>) -> Vec<Time> {
        let v = Params::new(&self.0, p);
        let density = v.f(3);
        let elapsed = p.comp_time.saturating_sub(p.layer_in).as_seconds();
        if density <= 0.0 || elapsed <= 0.0 {
            return Vec::new();
        }
        let stretch = p.layer_time_scale.abs().max(1e-6);
        let step = stretch / density;
        let last = (elapsed / step).floor();
        let span = v.f(4) * stretch;
        let first = if span > 0.0 {
            ((elapsed - span) / step).ceil().max(0.0)
        } else {
            0.0
        };
        if first > last {
            return Vec::new();
        }
        let count = ((last - first + 1.0) as usize).min(MAX_SAMPLES);
        (0..count)
            .map(|i| {
                // Start from the latest bead. Decimate long strokes across their visible length.
                let t = i as f64 / count.saturating_sub(1).max(1) as f64;
                p.layer_in + Time::from_seconds((last - ((last - first) * t).round()) * step)
            })
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

impl TargetEffect for GlueGun {
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
        SHADER.dispatch_at(
            "main",
            target,
            Params::new(&self.0, p),
            roi,
            input,
            [],
            [0, 0],
            |uniform, header| {
                let mut previous = p.point(1);
                let mut count = 0;
                for (_, sample) in p.history.iter().take(MAX_SAMPLES) {
                    if let Some(xy) = EffectParams::new(sample).point(1)
                        && previous != Some(xy)
                    {
                        uniform.f32s(
                            header + (SAMPLE_BASE + count) * 16,
                            &xy.map(|v| (v * p.scale) as f32),
                        );
                        previous = Some(xy);
                        count += 1;
                    }
                }
                uniform.f32(header + (SAMPLE_BASE - 1) * 16, count as f32);
            },
        )
    }
}
