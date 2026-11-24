//! AE 26 match names, disk IDs, parameter order and scripting units.
use super::{Kind, Spatial};
use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use std::sync::Arc;

fn param(
    mut b: DefBuilder,
    id: u32,
    en: &str,
    zh: &str,
    kind: ParamKind,
    value: Value,
) -> DefBuilder {
    b.push(id, en, zh, kind, Some(value));
    b
}

fn radial() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Radial Blur", "Radial Blur", "Blur & Sharpen");
    // Disk ID 5 is an unnamed, non-animatable NO_VALUE custom UI control.
    b.push(5, "", "", ParamKind::ArbitraryData, None);
    b = b.slider(1, "Amount", "数量", 10., [0., 1000.], [0., 100.], 1, "none");
    param(
        b,
        2,
        "Center",
        "中心",
        ParamKind::Point,
        Value::Vec2([320., 180.]),
    )
    .popup(3, "Type", "类型", 1, &["Spin", "Zoom"])
    .popup(
        4,
        "Antialiasing (Best Quality)",
        "消除锯齿（最佳品质）",
        1,
        &["Low", "High"],
    )
    .int(6, "Random Seed", "随机植入", 0, [0, 10000])
}

fn unsharp() -> DefBuilder {
    DefBuilder::new("ADBE Unsharp Mask2", "Unsharp Mask", "Blur & Sharpen")
        .checkbox(4, "Color Mode", "颜色模式", false)
        .slider(
            1,
            "Amount",
            "数量",
            0.,
            [-500., 500.],
            [-500., 500.],
            1,
            "percent",
        )
        .slider(
            2,
            "Radius",
            "半径",
            1.,
            [0.100006, 500.],
            [0.100006, 10.],
            1,
            "pixels",
        )
        .slider(3, "Threshold", "阈值", 0., [0., 1.], [0., 1.], 3, "none")
}

fn shadow() -> DefBuilder {
    param(
        DefBuilder::new("ADBE Drop Shadow", "Drop Shadow", "Perspective"),
        1,
        "Shadow Color",
        "阴影颜色",
        ParamKind::Color,
        Value::Color([0., 0., 0., 1.]),
    )
    .slider(
        2,
        "Opacity",
        "不透明度",
        127.5,
        [0., 255.],
        [0., 255.],
        1,
        "none",
    )
    .angle(3, "Direction", "方向", 135.)
    .slider(
        4,
        "Distance",
        "距离",
        5.,
        [0., 4000.],
        [0., 120.],
        1,
        "pixels",
    )
    .slider(
        5,
        "Softness",
        "柔和度",
        0.,
        [0., 30000.],
        [0., 250.],
        1,
        "pixels",
    )
    .checkbox(6, "Shadow Only", "仅阴影", false)
}

impl DefBuilder {
    pub(crate) fn angle(self, id: u32, en: &str, zh: &str, degrees: f64) -> Self {
        param(self, id, en, zh, ParamKind::Angle, Value::Float(degrees))
    }
}

fn glow() -> DefBuilder {
    let b = DefBuilder::new("ADBE Glo2", "Glow", "Stylize")
        .popup(
            1,
            "Glow Based On",
            "发光基于",
            2,
            &["Alpha Channel", "Color Channels"],
        )
        .slider(
            2,
            "Glow Threshold",
            "发光阈值",
            153.,
            [0., 255.],
            [0., 255.],
            1,
            "none",
        )
        .slider(
            3,
            "Glow Radius",
            "发光半径",
            10.,
            [0., 1000.],
            [0., 100.],
            1,
            "pixels",
        )
        .slider(
            4,
            "Glow Intensity",
            "发光强度",
            1.,
            [0., 255.],
            [0., 4.],
            2,
            "none",
        )
        .popup(
            5,
            "Composite Original",
            "合成原始项目",
            2,
            &["On Top", "Behind", "None"],
        )
        .popup(
            6,
            "Glow Operation",
            "发光操作",
            3,
            &[
                "None",
                "Normal",
                "Add",
                "Multiply",
                "Dissolve",
                "Screen",
                "Overlay",
                "Soft Light",
                "Hard Light",
                "Darken",
                "Lighten",
                "Difference",
                "Hue",
                "Saturation",
                "Color",
                "Luminosity",
                "Color Dodge",
                "Color Burn",
                "Exclusion",
                "Stencil Alpha",
                "Stencil Luma",
                "Silhouette Alpha",
                "Silhouette Luma",
                "Luminescent Premul",
                "Alpha Add",
            ],
        )
        .popup(
            7,
            "Glow Colors",
            "发光颜色",
            1,
            &["Original Colors", "A & B Colors", "Arbitrary Map"],
        )
        .popup(
            8,
            "Color Looping",
            "颜色循环",
            3,
            &[
                "Sawtooth A>B",
                "Sawtooth B>A",
                "Triangle A>B>A",
                "Triangle B>A>B",
            ],
        )
        .slider(
            9,
            "Color Loops",
            "颜色循环",
            1.,
            [1., 127.],
            [1., 10.],
            2,
            "none",
        )
        .angle(10, "Color Phase", "色彩相位", 0.)
        .slider(
            11,
            "A & B Midpoint",
            "A 和 B 中点",
            0.5,
            [0.009995, 0.990005],
            [0.009995, 0.990005],
            2,
            "none",
        );
    let b = param(
        b,
        12,
        "Color A",
        "颜色 A",
        ParamKind::Color,
        Value::Color([1.; 4]),
    );
    param(
        b,
        13,
        "Color B",
        "颜色 B",
        ParamKind::Color,
        Value::Color([0., 0., 0., 1.]),
    )
    .popup(
        14,
        "Glow Dimensions",
        "发光维度",
        1,
        &["Horizontal and Vertical", "Horizontal", "Vertical"],
    )
}

pub(super) fn entries() -> [Builtin; 6] {
    let mosaic = DefBuilder::new("ADBE Mosaic", "Mosaic", "Stylize")
        .int(1, "Horizontal Blocks", "水平块", 10, [1, 4000])
        .int(2, "Vertical Blocks", "垂直块", 10, [1, 4000])
        .checkbox(3, "Sharp Colors", "锐化颜色", false);
    [
        (
            Kind::Directional,
            DefBuilder::new("ADBE Motion Blur", "Directional Blur", "Blur & Sharpen")
                .angle(1, "Direction", "方向", 90.)
                .slider(
                    2,
                    "Blur Length",
                    "模糊长度",
                    10.,
                    [0., 1000.],
                    [0., 100.],
                    1,
                    "pixels",
                ),
            "directional-blur",
            "定向模糊",
        ),
        (Kind::Radial, radial(), "radial-blur", "径向模糊"),
        (Kind::Mosaic, mosaic, "mosaic", "马赛克"),
        (Kind::Glow, glow(), "glow", "发光"),
        (Kind::Shadow, shadow(), "drop-shadow", "投影"),
        (Kind::Unsharp, unsharp(), "unsharp-mask", "钝化蒙版"),
    ]
    .map(|(kind, b, id, zh)| {
        let (mut def, names) = b.build();
        if kind == Kind::Radial {
            let control = &mut def.params[0];
            control.flags.hidden = true;
            control.flags.cannot_animate = true;
            control.flags.custom_ui = true;
        }
        rust_builtin((Arc::new(Spatial { kind, def }), names), id, zh, &[id, zh])
    })
}
