use crate::{
    Builtin, Registry,
    catalog::DefBuilder,
    effects::sampled::{Sampled, Shader, unchanged},
    gpu, rust_builtin,
};
use crate::{EffectTarget, TargetEffect, common};
use crate::{
    effects::{image_grid, layer_input},
    params::Params,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderComp, RenderEffect, RenderError,
    RenderLayer, effect::LayerDependency,
};
use std::sync::Arc;

/// Blobbylize blurs the blob layer by Softness first, then finds the normals and
/// the Cut Away threshold on that smooth height field in one pass.
struct Blobbylize(Sampled);

impl Effect for Blobbylize {
    fn def(&self) -> &EffectDef {
        self.0.def()
    }
    fn prepare_scene(&self, instance: &mut RenderEffect, comp: &RenderComp, layer: &RenderLayer) {
        self.0.prepare_scene(instance, comp, layer);
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        self.0.input_roi(p, roi)
    }
    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        self.0.output_bounds(p, input)
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.0.layer_dependencies(p)
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

impl TargetEffect for Blobbylize {
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
        let s = &self.0;
        let blob = layer_input::dependency(p, 2, true).and_then(|dep| {
            target.effect_layer(&dep).cloned().or_else(|| {
                (p.layer_bounds.is_empty() && p.value(2).is_none()).then(|| input.clone())
            })
        });
        let v = Params::new(&s.def, p);
        let blob = blob
            .map(|b| common::blurred(target, &b, v.f(4) * 0.5))
            .transpose()?;
        s.shader
            .dispatch(target, v, roi, input, [blob.as_ref()], [s.mode, 0])
    }
}

fn layer(p: Params<'_>, roi: IRect) -> IRect {
    roi.union(&image_grid::layer_rect(p.p))
}

static SHADER: Shader = Shader {
    label: "bae.effects.distort.cc",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../effects/surface_lights.wgsl"),
        include_str!("cc.wgsl")
    ),
};

pub(super) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry.effects.extend([
        cc_bender(),
        cc_blobbylize(),
        cc_flo_motion(),
        cc_ripple_pulse(),
        cc_slant(),
        cc_smear(),
        cc_split(),
        cc_split_2(),
    ]);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}

fn cc_bender() -> Builtin {
    let mut b = DefBuilder::new("CC Bender", "CC Bender", "Distort");
    b = b.slider(
        1,
        "Amount",
        "Amount",
        0.0,
        [-250.0, 250.0],
        [-250.0, 250.0],
        2,
        "pixels",
    );
    b = b.popup(
        2,
        "Style",
        "Style",
        1,
        &["Bend", "Marilyn", "Sharp", "Boxer"],
    );
    b = b.checkbox(3, "Adjust To Distance", "Adjust To Distance", false);
    b.push(
        4,
        "Top",
        "Top",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 90.0])),
    );
    b.push(
        5,
        "Base",
        "Base",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 270.0])),
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 0,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-bender",
        "CC Bender",
        &["CC Bender", "扭曲"],
    )
}

fn cc_blobbylize() -> Builtin {
    let mut b = DefBuilder::new("CC Blobbylize", "CC Blobbylize", "Distort");
    b.push(1, "Blobbiness", "Blobbiness", ParamKind::GroupStart, None);
    b = b.layer(2, "Blob Layer", "Blob Layer").layer_stage(2);
    b = b.popup(
        3,
        "Property",
        "Property",
        6,
        &["Red", "Green", "Blue", "Alpha", "Luminance", "Lightness"],
    );
    b = b.slider(
        4,
        "Softness",
        "Softness",
        20.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Cut Away",
        "Cut Away",
        25.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(6, "", "", ParamKind::GroupEnd, None);
    b.push(7, "Light", "Light", ParamKind::GroupStart, None);
    b = b.popup(22, "Using", "Using", 1, &["Effect Light", "AE Lights"]);
    b = b.slider(
        8,
        "Light Intensity",
        "Light Intensity",
        100.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(
        9,
        "Light Color",
        "Light Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        10,
        "Light Type",
        "Light Type",
        1,
        &["Distant Light", "Point Light"],
    );
    b = b.slider(
        11,
        "Light Height",
        "Light Height",
        65.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b.push(
        12,
        "Light Position",
        "Light Position",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 90.0])),
    );
    b.push(
        13,
        "Light Direction",
        "Light Direction",
        ParamKind::Angle,
        Some(Value::Float(-45.0)),
    );
    b.push(14, "", "", ParamKind::GroupEnd, None);
    b.push(15, "Shading", "Shading", ParamKind::GroupStart, None);
    b = b.slider(
        16,
        "Ambient",
        "Ambient",
        75.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        17,
        "Diffuse",
        "Diffuse",
        25.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        18,
        "Specular",
        "Specular",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        19,
        "Roughness",
        "Roughness",
        0.024994,
        [0.000992, 0.5],
        [0.000992, 0.5],
        4,
        "none",
    );
    b = b.slider(
        20,
        "Metal",
        "Metal",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(21, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Blobbylize(Sampled {
                def,
                shader: &SHADER,
                mode: 1,
                inputs: &[(2, true)],
                read: layer,
                bounds: unchanged,
            })),
            labels,
        ),
        "cc-blobbylize",
        "CC Blobbylize",
        &["CC Blobbylize", "扭曲"],
    )
}

fn cc_flo_motion() -> Builtin {
    let mut b = DefBuilder::new("CC Flo Motion", "CC Flo Motion", "Distort");
    b = b.checkbox(1, "Finer Controls", "Finer Controls", false);
    b.push(
        2,
        "Knot 1",
        "Knot 1",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 90.0])),
    );
    b = b.slider(
        3,
        "Amount 1",
        "Amount 1",
        0.0,
        [-1600.0, 1600.0],
        [-1600.0, 1600.0],
        2,
        "none",
    );
    b.push(
        4,
        "Knot 2",
        "Knot 2",
        ParamKind::Point,
        Some(Value::Vec2([480.0, 270.0])),
    );
    b = b.slider(
        5,
        "Amount 2",
        "Amount 2",
        0.0,
        [-1600.0, 1600.0],
        [-1600.0, 1600.0],
        2,
        "none",
    );
    b = b.checkbox(6, "Tile Edges", "Tile Edges", true);
    b = b.popup(
        7,
        "Antialiasing",
        "Antialiasing",
        2,
        &["Low", "Medium", "High"],
    );
    b = b.slider(
        8,
        "Falloff",
        "Falloff",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 2,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-flo-motion",
        "CC Flo Motion",
        &["CC Flo Motion", "扭曲"],
    )
}

fn cc_ripple_pulse() -> Builtin {
    let mut b = DefBuilder::new("CC Ripple Pulse", "CC Ripple Pulse", "Distort");
    b.push(
        1,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        2,
        "Pulse Level (Animate)",
        "Pulse Level (Animate)",
        100.0,
        [-10000.0, 10000.0],
        [-10000.0, 10000.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Time Span (sec)",
        "Time Span (sec)",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Amplitude",
        "Amplitude",
        100.0,
        [-10000.0, 10000.0],
        [-10000.0, 10000.0],
        2,
        "none",
    );
    b = b.checkbox(5, "Render Bump Map (RGBA)", "Render Bump Map (RGBA)", false);
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 3,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-ripple-pulse",
        "CC Ripple Pulse",
        &["CC Ripple Pulse", "扭曲"],
    )
}

fn cc_slant() -> Builtin {
    let mut b = DefBuilder::new("CC Slant", "CC Slant", "Distort");
    b = b.slider(
        1,
        "Slant",
        "Slant",
        0.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b = b.checkbox(2, "Stretching", "Stretching", true);
    b = b.slider(
        3,
        "Height",
        "Height",
        100.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b.push(
        4,
        "Floor",
        "Floor",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 360.0])),
    );
    b = b.checkbox(5, "Set Color", "Set Color", false);
    b.push(
        6,
        "Color",
        "Color",
        ParamKind::Color,
        Some(Value::Color([0.0, 0.0, 0.0, 1.0])),
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 4,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-slant",
        "CC Slant",
        &["CC Slant", "扭曲"],
    )
}

fn cc_smear() -> Builtin {
    let mut b = DefBuilder::new("CC Smear", "CC Smear", "Distort");
    b.push(
        1,
        "From",
        "From",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        2,
        "To",
        "To",
        ParamKind::Point,
        Some(Value::Vec2([480.0, 90.0])),
    );
    b = b.slider(
        3,
        "Reach",
        "Reach",
        100.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Radius",
        "Radius",
        80.0,
        [0.0, 4000.0],
        [0.0, 4000.0],
        2,
        "none",
    );
    b = b.popup(
        5,
        "Tip Type",
        "Tip Type",
        1,
        &["Classic", "Round", "Bell", "Wizard"],
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 5,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-smear",
        "CC Smear",
        &["CC Smear", "扭曲"],
    )
}

fn cc_split() -> Builtin {
    let mut b = DefBuilder::new("CC Split", "CC Split", "Distort");
    b.push(
        1,
        "Point A",
        "Point A",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 180.0])),
    );
    b.push(
        2,
        "Point B",
        "Point B",
        ParamKind::Point,
        Some(Value::Vec2([480.0, 180.0])),
    );
    b = b.slider(
        3,
        "Split",
        "Split",
        0.0,
        [0.0, 250.0],
        [0.0, 250.0],
        2,
        "none",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 6,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-split",
        "CC Split",
        &["CC Split", "扭曲"],
    )
}

fn cc_split_2() -> Builtin {
    let mut b = DefBuilder::new("CC Split 2", "CC Split 2", "Distort");
    b.push(
        1,
        "Point A",
        "Point A",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 180.0])),
    );
    b.push(
        2,
        "Point B",
        "Point B",
        ParamKind::Point,
        Some(Value::Vec2([480.0, 180.0])),
    );
    b = b.slider(
        3,
        "Split 1",
        "Split 1",
        0.0,
        [0.0, 250.0],
        [0.0, 250.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Split 2",
        "Split 2",
        0.0,
        [0.0, 250.0],
        [0.0, 250.0],
        2,
        "none",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 7,
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-split-2",
        "CC Split 2",
        &["CC Split 2", "扭曲"],
    )
}
