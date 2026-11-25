use super::*;
use crate::catalog::DefBuilder;
use bae_model::{ParamKind, Value};

fn mesh_warp() -> Builtin {
    let mut b = DefBuilder::new("ADBE MESH WARP", "Mesh Warp", "Distort");
    b = b.int(1, "Rows", "行数", 7, [1, 31]);
    b = b.int(2, "Columns", "列数", 7, [1, 31]);
    b = b.int(5, "Quality", "品质", 8, [1, 10]);
    b.push(
        4,
        "Distortion Mesh",
        "扭曲网格",
        ParamKind::ArbitraryData,
        None,
    );
    let (def, labels) = b.build();
    rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::MeshWarp,
            }),
            labels,
        ),
        "mesh-warp",
        "网格变形",
        &["mesh-warp", "扭曲"],
    )
}

fn bezier_warp() -> Builtin {
    let mut b = DefBuilder::new("ADBE BEZMESH", "Bezier Warp", "Distort");
    b.push(
        1,
        "Top Left Vertex",
        "上左顶点",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 0.0])),
    );
    b.push(
        2,
        "Top Left Tangent",
        "上左切点",
        ParamKind::Point,
        Some(Value::Vec2([213.312012, 0.0])),
    );
    b.push(
        3,
        "Top Right Tangent",
        "上右切点",
        ParamKind::Point,
        Some(Value::Vec2([426.624023, 0.0])),
    );
    b.push(
        4,
        "Right Top Vertex",
        "右上顶点",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 0.0])),
    );
    b.push(
        5,
        "Right Top Tangent",
        "右上切点",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 119.988007])),
    );
    b.push(
        6,
        "Right Bottom Tangent",
        "右下切点",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 239.976013])),
    );
    b.push(
        7,
        "Bottom Right Vertex",
        "下右顶点",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 360.0])),
    );
    b.push(
        8,
        "Bottom Right Tangent",
        "下右切点",
        ParamKind::Point,
        Some(Value::Vec2([426.624023, 360.0])),
    );
    b.push(
        9,
        "Bottom Left Tangent",
        "下左切点",
        ParamKind::Point,
        Some(Value::Vec2([213.312012, 360.0])),
    );
    b.push(
        10,
        "Left Bottom Vertex",
        "左下顶点",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 360.0])),
    );
    b.push(
        11,
        "Left Bottom Tangent",
        "左下切点",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 239.976013])),
    );
    b.push(
        12,
        "Left Top Tangent",
        "左上切点",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 119.988007])),
    );
    b = b.int(14, "Quality", "品质", 8, [1, 10]);
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::BezierWarp,
            }),
            labels,
        ),
        "bezier-warp",
        "贝塞尔曲线变形",
        &["bezier-warp", "扭曲"],
    )
}

fn magnify() -> Builtin {
    let mut b = DefBuilder::new("ADBE Magnify", "Magnify", "Distort");
    b = b.popup(10, "Shape", "形状", 1, &["Circle", "Square"]);
    b.push(
        1,
        "Center",
        "中心",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        3,
        "Magnification",
        "放大率",
        150.0,
        [100.0, 20000.0],
        [100.0, 20000.0],
        2,
        "pixels",
    );
    b = b.popup(
        11,
        "Link",
        "链接",
        1,
        &[
            "None",
            "Size to Magnification",
            "Size and Feather to Magnification",
        ],
    );
    b = b.slider(
        2,
        "Size",
        "大小",
        100.0,
        [1.0, 4000.0],
        [1.0, 4000.0],
        2,
        "percent",
    );
    b = b.slider(
        4,
        "Feather",
        "羽化",
        0.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Opacity",
        "不透明度",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b = b.popup(8, "Scale", "缩放", 1, &["Standard", "Soft", "Scatter"]);
    b = b.popup(
        12,
        "Blending Mode",
        "混合模式",
        2,
        &[
            "None",
            "Normal",
            "(-",
            "Add",
            "Multiply",
            "Screen",
            "Overlay",
            "Soft Light",
            "Hard Light",
            "(-",
            "Color Dodge",
            "Color Burn",
            "(-",
            "Darken",
            "Lighten",
            "Difference",
            "Exclusion",
            "(-",
            "Hue",
            "Saturation",
            "Color",
            "Luminosity",
        ],
    );
    b = b.checkbox(9, "Resize Layer", "调整图层大小", false);
    let (def, labels) = b.reference_points().build();
    let mut effect = rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::Magnify,
            }),
            labels,
        ),
        "magnify",
        "放大",
        &["magnify", "扭曲"],
    );
    effect
        .info
        .param_options_zh
        .push((10, ["圆形", "正方形"].map(str::to_owned).to_vec()));
    effect.info.param_options_zh.push((
        11,
        ["无", "大小至放大率", "大小和羽化至放大率"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect
        .info
        .param_options_zh
        .push((8, ["标准", "柔和", "散布"].map(str::to_owned).to_vec()));
    effect.info.param_options_zh.push((
        12,
        [
            "无",
            "正常",
            "(-",
            "相加",
            "相乘",
            "滤色",
            "叠加",
            "柔光",
            "强光",
            "(-",
            "颜色减淡",
            "颜色加深",
            "(-",
            "变暗",
            "变亮",
            "差值",
            "排除",
            "(-",
            "色相",
            "饱和度",
            "颜色",
            "发光度",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn warp() -> Builtin {
    let mut b = DefBuilder::new("ADBE WRPMESH", "Warp", "Distort");
    b = b.popup(
        1,
        "Warp Style",
        "变形样式",
        1,
        &[
            "Arc",
            "Arc Lower",
            "Arc Upper",
            "Arch",
            "Bulge",
            "Shell Lower",
            "Shell Upper",
            "Flag",
            "Wave",
            "Fish",
            "Rise",
            "Fisheye",
            "Inflate",
            "Twist",
            "Squeeze",
        ],
    );
    b = b.popup(2, "Warp Axis", "变形轴", 1, &["Horizontal", "Vertical"]);
    b = b.slider(
        3,
        "Bend",
        "弯曲",
        50.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Horizontal Distortion",
        "水平扭曲",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Vertical Distortion",
        "垂直扭曲",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::Warp,
            }),
            labels,
        ),
        "warp",
        "变形",
        &["warp", "扭曲"],
    );
    effect.info.param_options_zh.push((
        1,
        [
            "弧",
            "下弧形",
            "上弧形",
            "弓形",
            "凸出",
            "下抽壳",
            "上抽壳",
            "标记",
            "波形",
            "鱼",
            "上升",
            "白点",
            "膨胀",
            "扭转",
            "挤压",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
        .info
        .param_options_zh
        .push((2, ["水平", "垂直"].map(str::to_owned).to_vec()));
    effect
}

fn cc_power_pin() -> Builtin {
    let mut b = DefBuilder::new("CC Power Pin", "CC Power Pin", "Distort");
    b.push(
        2,
        "Top Left",
        "Top Left",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 0.0])),
    );
    b.push(
        3,
        "Top Right",
        "Top Right",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 0.0])),
    );
    b.push(
        4,
        "Bottom Left",
        "Bottom Left",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 360.0])),
    );
    b.push(
        5,
        "Bottom Right",
        "Bottom Right",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 360.0])),
    );
    b = b.checkbox(6, "Perspective", "Perspective", true);
    b = b.checkbox(7, "Unstretch", "Unstretch", false);
    b.push(
        8,
        "Expansion (%)",
        "Expansion (%)",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        9,
        "Top",
        "Top",
        0.0,
        [-200.0, 200.0],
        [-200.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Left",
        "Left",
        0.0,
        [-200.0, 200.0],
        [-200.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        11,
        "Right",
        "Right",
        0.0,
        [-200.0, 200.0],
        [-200.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        12,
        "Bottom",
        "Bottom",
        0.0,
        [-200.0, 200.0],
        [-200.0, 200.0],
        2,
        "none",
    );
    b.push(13, "", "", ParamKind::GroupEnd, None);
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(super::super::Distortion {
                def,
                kind: super::super::Kind::PowerPin,
            }),
            labels,
        ),
        "cc-power-pin",
        "CC Power Pin",
        &["cc-power-pin", "透视定位"],
    )
}

fn cc_page_turn() -> Builtin {
    let mut b = DefBuilder::new("CC Page Turn", "CC Page Turn", "Distort");
    b = b.popup(
        8,
        "Controls",
        "Controls",
        6,
        &[
            "Classic UI",
            "(-",
            "Top Left Corner",
            "Top Right Corner",
            "Bottom Left Corner",
            "Bottom Right Corner",
        ],
    );
    b.push(
        1,
        "Fold Position",
        "Fold Position",
        ParamKind::Point,
        Some(Value::Vec2([480.0, 180.0])),
    );
    b = b.angle(2, "Fold Direction", "Fold Direction", -60.0);
    b = b.slider(
        3,
        "Fold Radius",
        "Fold Radius",
        50.0,
        [4.0, 1000.0],
        [4.0, 1000.0],
        2,
        "pixels",
    );
    b = b.angle(4, "Light Direction", "Light Direction", -60.0);
    b = b.popup(
        7,
        "Render",
        "Render",
        1,
        &["Front & Back Page", "Back Page", "Front Page"],
    );
    b = b.layer(5, "Back Page", "Back Page").layer_stage(5);
    b = b.slider(
        9,
        "Back Opacity",
        "Back Opacity",
        85.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(
        6,
        "Paper Color",
        "Paper Color",
        ParamKind::Color,
        Some(Value::Color([0.768627, 0.768627, 0.705882, 1.0])),
    );
    let (def, labels) = b.reference_points().build();
    let mut effect = rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::CcPageTurn,
            }),
            labels,
        ),
        "cc-page-turn",
        "CC Page Turn",
        &["cc-page-turn", "翻页"],
    );
    effect.info.param_options_zh.push((
        8,
        [
            "Classic UI",
            "(-",
            "Top Left Corner",
            "Top Right Corner",
            "Bottom Left Corner",
            "Bottom Right Corner",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect.info.param_options_zh.push((
        7,
        ["Front & Back Page", "Back Page", "Front Page"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect
}

fn cc_lens() -> Builtin {
    let mut b = DefBuilder::new("CC Lens", "CC Lens", "Distort");
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
        50.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Convergence",
        "Convergence",
        100.0,
        [-200.0, 100.0],
        [-200.0, 100.0],
        2,
        "none",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::CcLens,
            }),
            labels,
        ),
        "cc-lens",
        "CC Lens",
        &["cc-lens", "镜头"],
    )
}

fn cc_bend_it() -> Builtin {
    let mut b = DefBuilder::new("CC Bend It", "CC Bend It", "Distort");
    b = b.slider(
        1,
        "Bend",
        "Bend",
        0.0,
        [-250.0, 250.0],
        [-250.0, 250.0],
        2,
        "pixels",
    );
    b.push(
        2,
        "Start",
        "Start",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 270.0])),
    );
    b.push(
        3,
        "End",
        "End",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 90.0])),
    );
    b = b.popup(
        4,
        "Render Prestart",
        "Render Prestart",
        3,
        &["None", "Static", "Bend", "Mirror"],
    );
    b = b.popup(5, "Distort", "Distort", 1, &["Legal", "Extended"]);
    let (def, labels) = b.reference_points().build();
    let mut effect = rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::CcBendIt,
            }),
            labels,
        ),
        "cc-bend-it",
        "CC Bend It",
        &["cc-bend-it", "弯曲"],
    );
    effect.info.param_options_zh.push((
        4,
        ["None", "Static", "Bend", "Mirror"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect
        .info
        .param_options_zh
        .push((5, ["Legal", "Extended"].map(str::to_owned).to_vec()));
    effect
}

fn cc_tiler() -> Builtin {
    let mut b = DefBuilder::new("CC Tiler", "CC Tiler", "Distort");
    b = b.slider(
        1,
        "Scale",
        "Scale",
        1.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b.push(
        2,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        3,
        "Blend w. Original",
        "Blend w. Original",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    let (def, labels) = b.reference_points().build();
    rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::CcTiler,
            }),
            labels,
        ),
        "cc-tiler",
        "CC Tiler",
        &["cc-tiler", "拼贴"],
    )
}

fn cc_griddler() -> Builtin {
    let mut b = DefBuilder::new("CC Griddler", "CC Griddler", "Distort");
    b = b.slider(
        1,
        "Horizontal Scale",
        "Horizontal Scale",
        100.0,
        [-2000.0, 2000.0],
        [-2000.0, 2000.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Vertical Scale",
        "Vertical Scale",
        100.0,
        [-2000.0, 2000.0],
        [-2000.0, 2000.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Tile Size",
        "Tile Size",
        10.0,
        [1.0, 200.0],
        [1.0, 200.0],
        2,
        "percent",
    );
    b = b.angle(4, "Rotation", "Rotation", 0.0);
    b = b.checkbox(5, "Cut Tiles", "Cut Tiles", true);
    let (def, labels) = b.build();
    rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::CcGriddler,
            }),
            labels,
        ),
        "cc-griddler",
        "CC Griddler",
        &["cc-griddler", "网格"],
    )
}

fn detail_preserving_upscale() -> Builtin {
    let mut b = DefBuilder::new("ADBE Upscale", "Detail-preserving Upscale", "Distort");
    b.push(3, "", "", ParamKind::Button, None);
    b.push(4, "", "", ParamKind::Button, None);
    b = b.slider(
        1,
        "Scale",
        "缩放",
        100.0,
        [100.0, 30000.0],
        [100.0, 30000.0],
        2,
        "percent",
    );
    b = b.slider(
        2,
        "Reduce Noise",
        "减少杂色",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b = b.slider(
        7,
        "Detail",
        "详细信息",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b = b.popup(5, "Alpha", "Alpha", 2, &["Detail-preserving", "Bicubic"]);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Warp {
                def,
                kind: Kind::DetailPreservingUpscale,
            }),
            labels,
        ),
        "detail-preserving-upscale",
        "保留细节放大",
        &["detail-preserving-upscale", "扭曲"],
    );
    effect
        .info
        .param_options_zh
        .push((5, ["保留细节", "双立方"].map(str::to_owned).to_vec()));
    effect
}

pub(crate) fn entries() -> Vec<Builtin> {
    vec![
        mesh_warp(),
        bezier_warp(),
        magnify(),
        warp(),
        cc_power_pin(),
        cc_page_turn(),
        cc_lens(),
        cc_bend_it(),
        cc_tiler(),
        cc_griddler(),
        detail_preserving_upscale(),
    ]
}
