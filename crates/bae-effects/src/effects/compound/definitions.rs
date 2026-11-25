use std::sync::Arc;

use bae_model::{ParamKind, Value};

use super::{Compound, Mode};
use crate::{Builtin, catalog::DefBuilder, rust_builtin};

const CHANNELS: &[&str] = &[
    "Red",
    "Green",
    "Blue",
    "Alpha",
    "Luminance",
    "Hue",
    "Lightness",
    "Saturation",
    "Full",
    "Half",
    "Off",
];

fn builtin(mode: Mode, b: DefBuilder, id: &str, zh: &str) -> Builtin {
    let (def, names) = b.build();
    rust_builtin((Arc::new(Compound { def, mode }), names), id, zh, &[id, zh])
}

pub(crate) fn gradient() -> Builtin {
    builtin(
        Mode::Gradient,
        DefBuilder::new("ADBE Gradient Wipe", "Gradient Wipe", "Transition")
            .slider(
                1,
                "Transition Completion",
                "过渡完成",
                0.,
                [0., 1.],
                [0., 1.],
                2,
                "none",
            )
            .slider(
                2,
                "Transition Softness",
                "过渡柔和度",
                0.,
                [0., 1.],
                [0., 1.],
                2,
                "none",
            )
            .layer(3, "Gradient Layer", "渐变图层")
            .layer_stage(3)
            .popup(
                4,
                "Gradient Placement",
                "渐变位置",
                3,
                &[
                    "Tile Gradient",
                    "Center Gradient",
                    "Stretch Gradient to Fit",
                ],
            )
            .checkbox(5, "Invert Gradient", "反转渐变", false),
        "gradient-wipe",
        "渐变擦除",
    )
}

pub(crate) fn displacement() -> Builtin {
    builtin(
        Mode::Displacement,
        DefBuilder::new("ADBE Displacement Map", "Displacement Map", "Distort")
            .layer(1, "Displacement Map Layer", "置换图层")
            .layer_stage(1)
            .popup(
                2,
                "Use For Horizontal Displacement",
                "用于水平置换",
                1,
                CHANNELS,
            )
            .slider(
                3,
                "Max Horizontal Displacement",
                "最大水平置换",
                5.,
                [-32000., 32000.],
                [-100., 100.],
                1,
                "pixels",
            )
            .popup(
                4,
                "Use For Vertical Displacement",
                "用于垂直置换",
                2,
                CHANNELS,
            )
            .slider(
                5,
                "Max Vertical Displacement",
                "最大垂直置换",
                5.,
                [-32000., 32000.],
                [-100., 100.],
                1,
                "pixels",
            )
            .popup(
                6,
                "Displacement Map Behavior",
                "置换图特性",
                1,
                &["Center Map", "Stretch Map to Fit", "Tile Map"],
            )
            .checkbox(7, "Edge Behavior", "边缘特性", false)
            .checkbox(8, "Expand Output", "扩展输出", true),
        "displacement-map",
        "置换图",
    )
}

pub(crate) fn compound_blur() -> Builtin {
    builtin(
        Mode::Blur,
        DefBuilder::new("ADBE Compound Blur", "Compound Blur", "Blur & Sharpen")
            .layer(1, "Blur Layer", "模糊图层")
            .layer_stage(1)
            .slider(
                2,
                "Maximum Blur",
                "最大模糊",
                20.,
                [0., 4000.],
                [0., 100.],
                1,
                "pixels",
            )
            .checkbox(3, "If Layer Sizes Differ", "如果图层大小不同", true)
            .checkbox(4, "Invert Blur", "反转模糊", false),
        "compound-blur",
        "复合模糊",
    )
}

pub(crate) fn texturize() -> Builtin {
    let mut b = DefBuilder::new("ADBE Texturize", "Texturize", "Stylize")
        .layer(1, "Texture Layer", "纹理图层")
        .layer_stage(1);
    b.push(
        2,
        "Light Direction",
        "灯光方向",
        ParamKind::Angle,
        Some(Value::Float(135.)),
    );
    builtin(
        Mode::Texturize,
        b.slider(
            3,
            "Texture Contrast",
            "纹理对比度",
            1.,
            [0., 2.],
            [0., 2.],
            2,
            "none",
        )
        .popup(
            4,
            "Texture Placement",
            "纹理位置",
            1,
            &["Tile Texture", "Center Texture", "Stretch Texture to Fit"],
        ),
        "texturize",
        "纹理化",
    )
}

pub(crate) fn fill() -> Builtin {
    let mut b = DefBuilder::new("ADBE Fill", "Fill", "Generate");
    b.push(1, "Fill Mask", "填充蒙版", ParamKind::Path, None);
    b = b.checkbox(7, "All Masks", "所有蒙版", false);
    b.push(
        2,
        "Color",
        "颜色",
        ParamKind::Color,
        Some(Value::Color([1., 0., 0., 1.])),
    );
    builtin(
        Mode::Fill,
        b.checkbox(6, "Invert", "反转", false)
            .slider(
                3,
                "Horizontal Feather",
                "水平羽化",
                0.,
                [0., 999.],
                [0., 999.],
                1,
                "pixels",
            )
            .slider(
                4,
                "Vertical Feather",
                "垂直羽化",
                0.,
                [0., 999.],
                [0., 999.],
                1,
                "pixels",
            )
            .slider(5, "Opacity", "不透明度", 1., [0., 1.], [0., 1.], 2, "none"),
        "fill",
        "填充",
    )
}
