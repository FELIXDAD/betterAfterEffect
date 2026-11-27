//! AE 26 disk parameter IDs, order and scripting units.
use super::{Kind, Stylize};
use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use std::sync::Arc;

const CARTOON_RENDER: &[&str] = &["Fill", "Edges", "Fill & Edges"];
const EDGE_TYPES: &[&str] = &[
    "Roughen",
    "Roughen Color",
    "Cut",
    "Spiky",
    "Rusty",
    "Rusty Color",
    "Photocopy",
    "Photocopy Color",
];
const SHADOW_RENDER: &[&str] = &["Regular", "Glass Edge"];

pub(in crate::effects::spatial) fn options_zh(
    match_name: &str,
    id: u32,
) -> Option<&'static [&'static str]> {
    Some(match (match_name, id) {
        ("ADBE Cartoonify", 1) => &["填充", "边缘", "填充及边缘"],
        ("ADBE Roughen Edges", 1) => &[
            "粗糙化",
            "颜色粗糙化",
            "剪切",
            "刺状",
            "生锈",
            "生锈颜色",
            "影印",
            "影印颜色",
        ],
        ("ADBE Radial Shadow", 6) => &["常规", "玻璃边缘"],
        _ => return None,
    })
}

fn param(
    mut def: DefBuilder,
    id: u32,
    en: &str,
    zh: &str,
    kind: ParamKind,
    value: Value,
) -> DefBuilder {
    def.push(id, en, zh, kind, Some(value));
    def
}

fn group(mut def: DefBuilder, id: u32, en: &str, zh: &str, kind: ParamKind) -> DefBuilder {
    def.push(id, en, zh, kind, None);
    def
}

fn emboss(match_name: &str, name: &str) -> DefBuilder {
    param(
        DefBuilder::new(match_name, name, "Stylize"),
        1,
        "Direction",
        "方向",
        ParamKind::Angle,
        Value::Float(45.),
    )
    .slider(2, "Relief", "起伏", 1., [0., 1000.], [0., 10.], 2, "pixels")
    .slider(
        3,
        "Contrast",
        "对比度",
        100.,
        [0., 32767.],
        [0., 500.],
        1,
        "none",
    )
    .slider(
        4,
        "Blend With Original",
        "与原始图像混合",
        0.,
        [0., 100.],
        [0., 100.],
        1,
        "percent",
    )
}

fn cartoon() -> DefBuilder {
    let def = DefBuilder::new("ADBE Cartoonify", "Cartoon", "Stylize")
        .popup(1, "Render", "渲染", 3, CARTOON_RENDER)
        .slider(
            2,
            "Detail Radius",
            "细节半径",
            8.,
            [0.001, 40.],
            [0.001, 40.],
            3,
            "pixels",
        )
        .slider(
            3,
            "Detail Threshold",
            "细节阈值",
            10.,
            [0.001, 100.],
            [0.001, 100.],
            3,
            "none",
        );
    let def = group(def, 4, "Fill", "填充", ParamKind::GroupStart)
        .int(5, "Shading Steps", "阴影步骤", 8, [2, 30])
        .slider(
            6,
            "Shading Smoothness",
            "阴影平滑度",
            70.,
            [0.001, 100.],
            [0.001, 100.],
            3,
            "percent",
        );
    let def = group(def, 7, "Fill", "填充", ParamKind::GroupEnd);
    let def = group(def, 8, "Edge", "边缘", ParamKind::GroupStart)
        .slider(9, "Threshold", "阈值", 1.6, [1., 10.], [1., 10.], 2, "none")
        .slider(
            10,
            "Width",
            "宽度",
            1.5,
            [0.001, 20.],
            [0.001, 20.],
            3,
            "pixels",
        )
        .slider(
            11,
            "Softness",
            "柔和度",
            60.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        )
        .slider(
            12,
            "Opacity",
            "不透明度",
            100.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        );
    let def = group(def, 13, "Edge", "边缘", ParamKind::GroupEnd);
    let def = group(def, 14, "Advanced", "高级", ParamKind::GroupStart)
        .slider(
            15,
            "Edge Enhancement",
            "边缘增强",
            1.,
            [-100., 100.],
            [-100., 100.],
            2,
            "none",
        )
        .slider(
            16,
            "Edge Black Level",
            "边缘黑色阶",
            0.,
            [-10., 10.],
            [-10., 10.],
            2,
            "none",
        )
        .slider(
            17,
            "Edge Contrast",
            "边缘对比度",
            0.5,
            [0., 5.],
            [0., 5.],
            2,
            "none",
        );
    group(def, 18, "Advanced", "高级", ParamKind::GroupEnd)
}

fn roughen() -> DefBuilder {
    let def = DefBuilder::new("ADBE Roughen Edges", "Roughen Edges", "Stylize").popup(
        1,
        "Edge Type",
        "边缘类型",
        1,
        EDGE_TYPES,
    );
    let def = param(
        def,
        10,
        "Edge Color",
        "边缘颜色",
        ParamKind::Color,
        Value::Color([0.6, 0.2, 0., 1.]),
    )
    .slider(2, "Border", "边界", 8., [0., 500.], [0., 100.], 1, "pixels")
    .slider(
        3,
        "Edge Sharpness",
        "边缘锐度",
        1.,
        [0., 10.],
        [0., 10.],
        2,
        "none",
    )
    .slider(
        4,
        "Fractal Influence",
        "分形影响",
        1.,
        [0., 1.],
        [0., 1.],
        2,
        "none",
    )
    .slider(
        5,
        "Scale",
        "比例",
        100.,
        [10., 1000.],
        [10., 400.],
        1,
        "percent",
    )
    .slider(
        6,
        "Stretch Width or Height",
        "伸缩宽度或高度",
        0.,
        [-100., 100.],
        [-100., 100.],
        1,
        "none",
    );
    let def = param(
        def,
        7,
        "Offset (Turbulence)",
        "偏移（湍流）",
        ParamKind::Point,
        Value::Vec2([0.; 2]),
    )
    .slider(
        8,
        "Complexity",
        "复杂度",
        2.,
        [1., 10.],
        [1., 10.],
        1,
        "none",
    );
    let def = param(
        def,
        9,
        "Evolution",
        "演化",
        ParamKind::Angle,
        Value::Float(0.),
    );
    let def = group(
        def,
        11,
        "Evolution Options",
        "演化选项",
        ParamKind::GroupStart,
    )
    .checkbox(12, "Cycle Evolution", "循环演化", false)
    .int(13, "Cycle (in Revolutions)", "循环（旋转次数）", 1, [1, 88])
    .int(14, "Random Seed", "随机植入", 0, [0, 100000]);
    group(def, 15, "Random Seed", "随机植入", ParamKind::GroupEnd)
}

fn tile() -> DefBuilder {
    let def = param(
        DefBuilder::new("ADBE Tile", "Motion Tile", "Stylize"),
        1,
        "Tile Center",
        "拼贴中心",
        ParamKind::Point,
        Value::Vec2([320., 180.]),
    )
    .slider(
        2,
        "Tile Width",
        "拼贴宽度",
        100.,
        [0., 100.],
        [0., 100.],
        1,
        "percent",
    )
    .slider(
        3,
        "Tile Height",
        "拼贴高度",
        100.,
        [0., 100.],
        [0., 100.],
        1,
        "percent",
    )
    .slider(
        4,
        "Output Width",
        "输出宽度",
        100.,
        [0., 30000.],
        [0., 300.],
        1,
        "percent",
    )
    .slider(
        5,
        "Output Height",
        "输出高度",
        100.,
        [0., 30000.],
        [0., 300.],
        1,
        "percent",
    )
    .checkbox(6, "Mirror Edges", "镜像边缘", false);
    param(def, 7, "Phase", "相位", ParamKind::Angle, Value::Float(0.)).checkbox(
        8,
        "Horizontal Phase Shift",
        "水平位移",
        false,
    )
}

fn radial_shadow() -> DefBuilder {
    let def = param(
        DefBuilder::new("ADBE Radial Shadow", "Radial Shadow", "Perspective"),
        1,
        "Shadow Color",
        "阴影颜色",
        ParamKind::Color,
        Value::Color([0., 0., 0., 1.]),
    )
    .slider(2, "Opacity", "不透明度", 0.5, [0., 1.], [0., 1.], 2, "none");
    param(
        def,
        3,
        "Light Source",
        "光源",
        ParamKind::Point,
        Value::Vec2([160., 90.]),
    )
    .slider(
        4,
        "Projection Distance",
        "投影距离",
        10.,
        [0., 100.],
        [0., 100.],
        1,
        "none",
    )
    .slider(
        5,
        "Softness",
        "柔和度",
        0.,
        [0., 100.],
        [0., 100.],
        1,
        "pixels",
    )
    .popup(6, "Render", "渲染", 1, SHADOW_RENDER)
    .slider(
        7,
        "Color Influence",
        "颜色影响",
        1.,
        [0., 2.],
        [0., 2.],
        2,
        "none",
    )
    .checkbox(8, "Shadow Only", "仅阴影", false)
    .checkbox(9, "Resize Layer", "调整图层大小", false)
}

pub(crate) fn stylize_builtins() -> [Builtin; 5] {
    [
        (
            Kind::Emboss,
            emboss("ADBE Emboss", "Emboss"),
            "emboss",
            "浮雕",
        ),
        (
            Kind::ColorEmboss,
            emboss("ADBE Color Emboss", "Color Emboss"),
            "color-emboss",
            "彩色浮雕",
        ),
        (Kind::Cartoon, cartoon(), "cartoon", "卡通"),
        (Kind::Roughen, roughen(), "roughen-edges", "毛边"),
        (
            Kind::Tile,
            tile().reference_points(),
            "motion-tile",
            "动态拼贴",
        ),
    ]
    .map(|(kind, def, id, zh)| builtin(kind, def, id, zh))
}

pub(crate) fn radial_shadow_builtin() -> Builtin {
    builtin(
        Kind::RadialShadow,
        radial_shadow().reference_points(),
        "radial-shadow",
        "径向阴影",
    )
}

fn builtin(kind: Kind, def: DefBuilder, id: &str, zh: &str) -> Builtin {
    let (def, names) = def.build();
    rust_builtin((Arc::new(Stylize { def, kind }), names), id, zh, &[id, zh])
}
