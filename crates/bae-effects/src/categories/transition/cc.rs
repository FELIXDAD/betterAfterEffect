use crate::{
    Builtin, Registry,
    catalog::DefBuilder,
    effects::sampled::{Sampled, Shader, unchanged},
    gpu, rust_builtin,
};
use crate::{effects::image_grid, params::Params};
use bae_gpu::wgpu;
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use bae_render::RenderError;
use std::sync::Arc;

fn layer(p: Params<'_>, roi: IRect) -> IRect {
    roi.union(&image_grid::layer_rect(p.p))
}

static SHADER: Shader = Shader {
    label: "bae.effects.transition.cc",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("sampling.wgsl"),
        include_str!("glass_wipe.wgsl"),
        include_str!("twister.wgsl"),
        include_str!("warpomatic.wgsl"),
        include_str!("cc.wgsl")
    ),
};

pub(super) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry
        .effects
        .extend([cc_glass_wipe(), cc_twister(), cc_warpomatic()]);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}

fn cc_glass_wipe() -> Builtin {
    let mut b = DefBuilder::new("CC Glass Wipe", "CC Glass Wipe", "Transition");
    b = b.slider(
        1,
        "Completion",
        "Completion",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b = b
        .layer(2, "Layer to Reveal", "Layer to Reveal")
        .layer_stage(2);
    b = b
        .layer(3, "Gradient Layer", "Gradient Layer")
        .layer_stage(3);
    b = b.slider(
        4,
        "Softness",
        "Softness",
        10.0,
        [5.0, 100.0],
        [5.0, 100.0],
        2,
        "percent",
    );
    b = b.slider(
        5,
        "Displacement Amount",
        "Displacement Amount",
        25.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 0,
                inputs: &[(2, true), (3, true)],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-glass-wipe",
        "CC Glass Wipe",
        &["CC Glass Wipe", "过渡"],
    )
}

fn cc_twister() -> Builtin {
    let mut b = DefBuilder::new("CC Twister", "CC Twister", "Transition");
    b = b.slider(
        2,
        "Completion",
        "Completion",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b = b.layer(1, "Backside", "Backside").layer_stage(1);
    b = b.checkbox(3, "Shading", "Shading", true);
    b.push(
        4,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        5,
        "Axis",
        "Axis",
        ParamKind::Angle,
        Some(Value::Float(-120.0)),
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 1,
                inputs: &[(1, true)],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-twister",
        "CC Twister",
        &["CC Twister", "过渡"],
    )
}

fn cc_warpomatic() -> Builtin {
    let mut b = DefBuilder::new("CC WarpoMatic", "CC WarpoMatic", "Transition");
    b = b.slider(
        1,
        "Completion",
        "Completion",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b
        .layer(2, "Layer to Reveal", "Layer to Reveal")
        .layer_stage(2);
    b = b.popup(
        3,
        "Reactor",
        "Reactor",
        1,
        &[
            "Brightness",
            "Contrast Differences",
            "Brightness Differences",
            "Local Differences",
        ],
    );
    b = b.slider(
        4,
        "Smoothness",
        "Smoothness",
        50.0,
        [5.0, 250.0],
        [5.0, 250.0],
        2,
        "percent",
    );
    b = b.slider(
        5,
        "Warp Amount",
        "Warp Amount",
        25.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b = b.popup(
        6,
        "Warp Direction",
        "Warp Direction",
        1,
        &["Joint", "Opposing", "Twisting"],
    );
    b = b.slider(
        7,
        "Blend Span",
        "Blend Span",
        75.0,
        [1.0, 100.0],
        [1.0, 100.0],
        2,
        "percent",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 2,
                inputs: &[(2, true)],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-warpomatic",
        "CC WarpoMatic",
        &["CC WarpoMatic", "过渡"],
    )
}
