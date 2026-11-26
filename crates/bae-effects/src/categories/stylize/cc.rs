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
    label: "bae.effects.stylize.cc",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../effects/surface_lights.wgsl"),
        include_str!("cc.wgsl")
    ),
};

pub(super) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry.effects.extend([
        cc_hextile(),
        cc_plastic(),
        cc_block_load(),
        cc_mr_smoothie(),
    ]);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}

fn cc_hextile() -> Builtin {
    let mut b = DefBuilder::new("CS HexTile", "CC HexTile", "Stylize");
    b = b.popup(
        1,
        "Render",
        "Render",
        1,
        &["Tile", "Fold Aligned", "Fold Seamlessly"],
    );
    b = b.slider(
        2,
        "Radius",
        "Radius",
        50.0,
        [3.0, 10000.0],
        [3.0, 10000.0],
        2,
        "none",
    );
    b.push(
        3,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.checkbox(4, "Lock Center Tile", "Lock Center Tile", false);
    b.push(
        5,
        "Rotate",
        "Rotate",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b = b.slider(
        6,
        "Smearing",
        "Smearing",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
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
                inputs: &[],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-hextile",
        "CC HexTile",
        &["CC HexTile", "风格化"],
    )
}

fn cc_plastic() -> Builtin {
    let mut b = DefBuilder::new("CC Plastic", "CC Plastic", "Stylize");
    b.push(
        1,
        "Surface Bump",
        "Surface Bump",
        ParamKind::GroupStart,
        None,
    );
    b = b.layer(2, "Bump Layer", "Bump Layer").layer_stage(2);
    b = b.popup(
        3,
        "Property",
        "Property",
        5,
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
        "Height",
        "Height",
        25.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b = b.slider(
        6,
        "Cut Min",
        "Cut Min",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        7,
        "Cut Max",
        "Cut Max",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(8, "", "", ParamKind::GroupEnd, None);
    b.push(9, "Light", "Light", ParamKind::GroupStart, None);
    b = b.popup(26, "Using", "Using", 1, &["Effect Light", "AE Lights"]);
    b = b.slider(
        10,
        "Light Intensity",
        "Light Intensity",
        100.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(
        11,
        "Light Color",
        "Light Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        12,
        "Light Type",
        "Light Type",
        1,
        &["Distant Light", "Point Light"],
    );
    b = b.slider(
        13,
        "Light Height",
        "Light Height",
        65.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b.push(
        14,
        "Light Position",
        "Light Position",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 90.0])),
    );
    b.push(
        15,
        "Light Direction",
        "Light Direction",
        ParamKind::Angle,
        Some(Value::Float(-45.0)),
    );
    b.push(
        16,
        "Ambient Light Color",
        "Ambient Light Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b.push(17, "", "", ParamKind::GroupEnd, None);
    b.push(18, "Shading", "Shading", ParamKind::GroupStart, None);
    b = b.slider(
        19,
        "Ambient",
        "Ambient",
        75.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        20,
        "Diffuse",
        "Diffuse",
        25.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        21,
        "Dust",
        "Dust",
        25.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        22,
        "Specular",
        "Specular",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        23,
        "Roughness",
        "Roughness",
        0.024994,
        [0.000992, 0.5],
        [0.000992, 0.5],
        4,
        "none",
    );
    b = b.slider(
        24,
        "Metal",
        "Metal",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(25, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 1,
                inputs: &[(2, true)],
                read: unchanged,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-plastic",
        "CC Plastic",
        &["CC Plastic", "风格化"],
    )
}

fn cc_block_load() -> Builtin {
    let mut b = DefBuilder::new("CS BlockLoad", "CC Block Load", "Stylize");
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
    b = b.int(2, "Scans", "Scans", 4, [1, 16]);
    b = b.checkbox(3, "Start Cleared", "Start Cleared", true);
    b = b.checkbox(4, "Bilinear", "Bilinear", false);
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
        "cc-block-load",
        "CC Block Load",
        &["CC Block Load", "风格化"],
    )
}

fn cc_mr_smoothie() -> Builtin {
    let mut b = DefBuilder::new("CC Mr. Smoothie", "CC Mr. Smoothie", "Stylize");
    b = b.layer(1, "Flow Layer", "Flow Layer").layer_stage(1);
    b = b.popup(
        2,
        "Property",
        "Property",
        5,
        &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Luminance",
            "Lightness",
            "Hue",
            "Saturation",
        ],
    );
    b = b.slider(
        3,
        "Smoothness",
        "Smoothness",
        4.0,
        [0.0, 511.0],
        [0.0, 511.0],
        2,
        "none",
    );
    b.push(
        4,
        "Sample A",
        "Sample A",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 90.0])),
    );
    b.push(
        5,
        "Sample B",
        "Sample B",
        ParamKind::Point,
        Some(Value::Vec2([480.0, 270.0])),
    );
    b.push(
        6,
        "Phase",
        "Phase",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b = b.popup(
        7,
        "Color Loop",
        "Color Loop",
        1,
        &["AB", "BA", "ABA", "BAB"],
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &SHADER,
                mode: 3,
                inputs: &[(1, true)],
                read: layer,
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-mr-smoothie",
        "CC Mr. Smoothie",
        &["CC Mr. Smoothie", "风格化"],
    )
}
