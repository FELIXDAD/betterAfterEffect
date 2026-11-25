use super::*;
use crate::catalog::DefBuilder;
use bae_model::{ParamKind, Value};

fn cc_kaleida() -> Builtin {
    let mut b = DefBuilder::new("CC Kaleida", "CC Kaleida", "Stylize");
    b.push(
        1,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        2,
        "Size",
        "Size",
        20.0,
        [1.0, 1000.0],
        [1.0, 1000.0],
        2,
        "percent",
    );
    b = b.popup(
        3,
        "Mirroring",
        "Mirroring",
        6,
        &[
            "Unfold",
            "Wheel",
            "Fish Head",
            "Can Meas",
            "Flip Flop",
            "Flower",
            "Dia Cross",
            "Flipper",
            "Starlish",
        ],
    );
    b = b.angle(4, "Rotation", "Rotation", 0.0);
    b = b.checkbox(5, "Floating Center", "Floating Center", false);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                mode: 0,
                def,
                label: "bae.effects/cc_kaleida",
                shader: CC_KALEIDA,
                inputs: &[],
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-kaleida",
        "CC Kaleida",
        &["cc-kaleida", "万花筒"],
    );
    effect.info.param_options_zh.push((
        3,
        [
            "Unfold",
            "Wheel",
            "Fish Head",
            "Can Meas",
            "Flip Flop",
            "Flower",
            "Dia Cross",
            "Flipper",
            "Starlish",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn cc_repetile() -> Builtin {
    let mut b = DefBuilder::new("CC RepeTile", "CC RepeTile", "Stylize");
    b = b.slider(
        1,
        "Expand Right",
        "Expand Right",
        0.0,
        [0.0, 20000.0],
        [0.0, 20000.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Expand Left",
        "Expand Left",
        0.0,
        [0.0, 20000.0],
        [0.0, 20000.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Expand Down",
        "Expand Down",
        0.0,
        [0.0, 20000.0],
        [0.0, 20000.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Expand Up",
        "Expand Up",
        0.0,
        [0.0, 20000.0],
        [0.0, 20000.0],
        2,
        "none",
    );
    b = b.popup(
        5,
        "Tiling",
        "Tiling",
        1,
        &[
            "Repeat",
            "Checker Flip H",
            "Checker Flip V",
            "Unfold",
            "Checker 180 deg",
            "Checker Flip 45 deg",
            "Checker 90 deg CCW",
            "Checker 90 deg CW",
            "Rosette",
            "Random",
            "None",
            "(-",
            "Turn CW",
            "Turn CCW",
            "Twist",
            "Slide",
            "Brick",
        ],
    );
    b = b.slider(
        6,
        "Blend Borders",
        "Blend Borders",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                mode: 0,
                def,
                label: "bae.effects/cc_repetile",
                shader: CC_REPETILE,
                inputs: &[],
                bounds: repeat_bounds,
            }),
            labels,
        ),
        "cc-repetile",
        "CC RepeTile",
        &["cc-repetile", "平铺"],
    );
    effect.info.param_options_zh.push((
        5,
        [
            "Repeat",
            "Checker Flip H",
            "Checker Flip V",
            "Unfold",
            "Checker 180 deg",
            "Checker Flip 45 deg",
            "Checker 90 deg CCW",
            "Checker 90 deg CW",
            "Rosette",
            "Random",
            "None",
            "(-",
            "Turn CW",
            "Turn CCW",
            "Twist",
            "Slide",
            "Brick",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn cc_glass() -> Builtin {
    let mut b = DefBuilder::new("CC Glass", "CC Glass", "Stylize");
    b.push(1, "Surface", "Surface", ParamKind::GroupStart, None);
    b = b.layer(2, "Bump Map", "Bump Map").layer_stage(2);
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
        "Displacement",
        "Displacement",
        100.0,
        [-500.0, 500.0],
        [-500.0, 500.0],
        2,
        "pixels",
    );
    b.push(7, "", "", ParamKind::GroupEnd, None);
    b.push(8, "Light", "Light", ParamKind::GroupStart, None);
    b = b.popup(23, "Using", "Using", 1, &["Effect Light", "AE Lights"]);
    b = b.slider(
        9,
        "Light Intensity",
        "Light Intensity",
        100.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(
        10,
        "Light Color",
        "Light Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        11,
        "Light Type",
        "Light Type",
        1,
        &["Distant Light", "Point Light"],
    );
    b = b.slider(
        12,
        "Light Height",
        "Light Height",
        65.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b.push(
        13,
        "Light Position",
        "Light Position",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.angle(14, "Light Direction", "Light Direction", -45.0);
    b.push(15, "", "", ParamKind::GroupEnd, None);
    b.push(16, "Shading", "Shading", ParamKind::GroupStart, None);
    b = b.slider(
        17,
        "Ambient",
        "Ambient",
        50.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        18,
        "Diffuse",
        "Diffuse",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        19,
        "Specular",
        "Specular",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        20,
        "Roughness",
        "Roughness",
        0.024994,
        [0.000992, 0.5],
        [0.000992, 0.5],
        2,
        "none",
    );
    b = b.slider(
        21,
        "Metal",
        "Metal",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(22, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                mode: 0,
                def,
                label: "bae.effects/cc_glass",
                shader: CC_GLASS,
                inputs: &[(2, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-glass",
        "CC Glass",
        &["cc-glass", "玻璃"],
    );
    effect.info.param_options_zh.push((
        3,
        ["Red", "Green", "Blue", "Alpha", "Luminance", "Lightness"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect.info.param_options_zh.push((
        23,
        ["Effect Light", "AE Lights"].map(str::to_owned).to_vec(),
    ));
    effect.info.param_options_zh.push((
        11,
        ["Distant Light", "Point Light"].map(str::to_owned).to_vec(),
    ));
    effect
}

pub(crate) fn entries() -> Vec<Builtin> {
    vec![cc_kaleida(), cc_repetile(), cc_glass()]
}
