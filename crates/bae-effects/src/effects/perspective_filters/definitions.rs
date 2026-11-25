use super::*;
use crate::catalog::DefBuilder;
use bae_model::{ParamKind, Value};

fn cc_sphere() -> Builtin {
    let mut b = DefBuilder::new("CC Sphere", "CC Sphere", "Perspective");
    b.push(1, "Rotation", "Rotation", ParamKind::GroupStart, None);
    b = b.angle(2, "Rotation X", "Rotation X", 0.0);
    b = b.angle(3, "Rotation Y", "Rotation Y", 0.0);
    b = b.angle(4, "Rotation Z", "Rotation Z", 0.0);
    b = b.popup(
        26,
        "Rotation Order",
        "Rotation Order",
        1,
        &["XYZ", "XZY", "YXZ", "YZX", "ZXY", "ZYX"],
    );
    b.push(5, "", "", ParamKind::GroupEnd, None);
    b = b.slider(
        6,
        "Radius",
        "Radius",
        200.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b.push(
        7,
        "Offset",
        "Offset",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.popup(8, "Render", "Render", 1, &["Full", "Outside", "Inside"]);
    b.push(9, "Light", "Light", ParamKind::GroupStart, None);
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
    b = b.slider(
        12,
        "Light Height",
        "Light Height",
        40.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b = b.angle(13, "Light Direction", "Light Direction", -85.0);
    b.push(14, "", "", ParamKind::GroupEnd, None);
    b.push(15, "Shading", "Shading", ParamKind::GroupStart, None);
    b = b.slider(
        16,
        "Ambient",
        "Ambient",
        10.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        17,
        "Diffuse",
        "Diffuse",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        18,
        "Specular",
        "Specular",
        10.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        19,
        "Roughness",
        "Roughness",
        0.049988,
        [0.000992, 0.5],
        [0.000992, 0.5],
        2,
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
    b = b.slider(
        21,
        "Reflective",
        "Reflective",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b
        .layer(22, "Reflection Map", "Reflection Map")
        .layer_stage(22);
    b = b.checkbox(23, "Internal Shadows", "Internal Shadows", false);
    b = b.checkbox(24, "Transparency Falloff", "Transparency Falloff", false);
    b.push(25, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                mode: 0,
                def,
                label: "bae.effects/cc_sphere",
                shader: CC_SPHERE,
                inputs: &[(22, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-sphere",
        "CC Sphere",
        &["cc-sphere", "球体"],
    );
    effect.info.param_options_zh.push((
        26,
        ["XYZ", "XZY", "YXZ", "YZX", "ZXY", "ZYX"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect
        .info
        .param_options_zh
        .push((8, ["Full", "Outside", "Inside"].map(str::to_owned).to_vec()));
    effect
}

fn cc_cylinder() -> Builtin {
    let mut b = DefBuilder::new("CC Cylinder", "CC Cylinder", "Perspective");
    b = b.slider(
        2,
        "Radius (%)",
        "Radius (%)",
        100.0,
        [0.0, 30000.0],
        [0.0, 30000.0],
        2,
        "none",
    );
    b.push(3, "Position", "Position", ParamKind::GroupStart, None);
    b = b.slider(
        4,
        "Position X",
        "Position X",
        0.0,
        [-30000.0, 30000.0],
        [-30000.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Position Y",
        "Position Y",
        0.0,
        [-30000.0, 30000.0],
        [-30000.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Position Z",
        "Position Z",
        0.0,
        [-30000.0, 30000.0],
        [-30000.0, 30000.0],
        2,
        "none",
    );
    b.push(7, "", "", ParamKind::GroupEnd, None);
    b.push(8, "Rotation", "Rotation", ParamKind::GroupStart, None);
    b = b.angle(9, "Rotation X", "Rotation X", 0.0);
    b = b.angle(10, "Rotation Y", "Rotation Y", 0.0);
    b = b.angle(11, "Rotation Z", "Rotation Z", 0.0);
    b = b.popup(
        27,
        "Rotation Order",
        "Rotation Order",
        3,
        &["XYZ", "XZY", "YXZ", "YZX", "ZXY", "ZYX"],
    );
    b.push(12, "", "", ParamKind::GroupEnd, None);
    b = b.popup(13, "Render", "Render", 1, &["Full", "Outside", "Inside"]);
    b.push(14, "Light", "Light", ParamKind::GroupStart, None);
    b = b.slider(
        15,
        "Light Intensity",
        "Light Intensity",
        100.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(
        16,
        "Light Color",
        "Light Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.slider(
        17,
        "Light Height",
        "Light Height",
        25.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "pixels",
    );
    b = b.angle(18, "Light Direction", "Light Direction", -45.0);
    b.push(19, "", "", ParamKind::GroupEnd, None);
    b.push(20, "Shading", "Shading", ParamKind::GroupStart, None);
    b = b.slider(
        21,
        "Ambient",
        "Ambient",
        50.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        22,
        "Diffuse",
        "Diffuse",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        23,
        "Specular",
        "Specular",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        24,
        "Roughness",
        "Roughness",
        0.039993,
        [0.000992, 0.5],
        [0.000992, 0.5],
        2,
        "none",
    );
    b = b.slider(
        25,
        "Metal",
        "Metal",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(26, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                mode: 0,
                def,
                label: "bae.effects/cc_cylinder",
                shader: CC_CYLINDER,
                inputs: &[],
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-cylinder",
        "CC Cylinder",
        &["cc-cylinder", "圆柱体"],
    );
    effect.info.param_options_zh.push((
        27,
        ["XYZ", "XZY", "YXZ", "YZX", "ZXY", "ZYX"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect.info.param_options_zh.push((
        13,
        ["Full", "Outside", "Inside"].map(str::to_owned).to_vec(),
    ));
    effect
}

fn cc_spotlight() -> Builtin {
    let mut b = DefBuilder::new("CC Spotlight", "CC Spotlight", "Perspective");
    b.push(
        1,
        "From",
        "From",
        ParamKind::Point,
        Some(Value::Vec2([576.0, 36.0])),
    );
    b.push(
        2,
        "To",
        "To",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        3,
        "Height",
        "Height",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Cone Angle",
        "Cone Angle",
        10.0,
        [0.0, 360.0],
        [0.0, 360.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Edge Softness",
        "Edge Softness",
        0.25,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b.push(
        6,
        "Color",
        "Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.slider(
        7,
        "Intensity",
        "Intensity",
        75.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.popup(
        8,
        "Render",
        "Render",
        4,
        &[
            "Light Only",
            "Light Add",
            "Light Add+",
            "Light Shadow",
            "(-",
            "Gel Only",
            "Gel Add",
            "Gel Add+",
            "Gel Shadow",
        ],
    );
    b = b.layer(9, "Gel Layer", "Gel Layer").layer_stage(9);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                mode: 0,
                def,
                label: "bae.effects/cc_spotlight",
                shader: CC_SPOTLIGHT,
                inputs: &[(9, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-spotlight",
        "CC Spotlight",
        &["cc-spotlight", "聚光灯"],
    );
    effect.info.param_options_zh.push((
        8,
        [
            "Light Only",
            "Light Add",
            "Light Add+",
            "Light Shadow",
            "(-",
            "Gel Only",
            "Gel Add",
            "Gel Add+",
            "Gel Shadow",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

pub(crate) fn entries() -> Vec<Builtin> {
    vec![cc_sphere(), cc_cylinder(), cc_spotlight()]
}
