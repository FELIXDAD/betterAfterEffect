use crate::catalog::DefBuilder;
use bae_model::{ParamKind, Value};

pub(super) fn lens() -> DefBuilder {
    let mut b = DefBuilder::new(
        "ADBE Camera Lens Blur",
        "Camera Lens Blur",
        "Blur & Sharpen",
    );
    b = b.slider(
        1,
        "Blur Radius",
        "模糊半径",
        5.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    b.push(
        2,
        "Iris Properties",
        "光圈属性",
        ParamKind::GroupStart,
        None,
    );
    b = b.popup(
        3,
        "Shape",
        "形状",
        4,
        &[
            "Triangle", "Square", "Pentagon", "Hexagon", "Heptagon", "Octagon", "Nonagon",
            "Decagon",
        ],
    );
    b = b.slider(
        4,
        "Roundness",
        "圆度",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Aspect Ratio",
        "长宽比",
        1.0,
        [0.01, 100.0],
        [0.01, 100.0],
        2,
        "none",
    );
    b.push(
        6,
        "Rotation",
        "旋转",
        ParamKind::Angle,
        Some(Value::Float(0.)),
    );
    b = b.slider(
        7,
        "Diffraction Fringe",
        "衍射条纹",
        100.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    b.push(8, "Iris Properties", "光圈属性", ParamKind::GroupEnd, None);
    b.push(9, "Blur Map", "模糊图", ParamKind::GroupStart, None);
    b = b.layer(10, "Layer", "图层").layer_stage(10);
    b = b.popup(
        11,
        "Channel",
        "声道",
        1,
        &["Luminance", "Red", "Green", "Blue", "Alpha"],
    );
    b = b.popup(
        12,
        "Placement",
        "位置",
        1,
        &["Center Map", "Stretch Map to Fit"],
    );
    b = b.slider(
        13,
        "Blur Focal Distance",
        "模糊焦距",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(14, "Invert Blur Map", "反转模糊图", false);
    b.push(15, "Blur Map", "模糊图", ParamKind::GroupEnd, None);
    b.push(16, "Highlights", "高光", ParamKind::GroupStart, None);
    b = b.slider(
        17,
        "Gain",
        "增益",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        18,
        "Threshold",
        "阈值",
        0.8,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        19,
        "Saturation",
        "饱和度",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(20, "Highlights", "高光", ParamKind::GroupEnd, None);
    b = b.checkbox(21, "Repeat Edge Pixels", "边缘特性", true);
    b = b.checkbox(22, "Use Linear Working Space", "使用“线性”工作空间", true);
    b
}

pub(super) fn bilateral() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Bilateral", "Bilateral Blur", "Blur & Sharpen");
    b = b.slider(
        1,
        "Radius",
        "半径",
        25.0,
        [0.0, 30000.0],
        [0.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Threshold",
        "阈值",
        3.0,
        [0.0, 200.0],
        [0.0, 200.0],
        2,
        "none",
    );
    b = b.checkbox(3, "Colorize", "彩色化", true);
    b
}

pub(super) fn smart() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Smart Blur", "Smart Blur", "Blur & Sharpen");
    b = b.slider(
        1,
        "Radius",
        "半径",
        3.0,
        [0.1, 100.0],
        [0.1, 100.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Threshold",
        "阈值",
        25.0,
        [0.1, 100.0],
        [0.1, 100.0],
        2,
        "none",
    );
    b = b.popup(
        3,
        "Mode",
        "模式",
        1,
        &["Normal", "Edge Only", "Overlay Edge"],
    );
    b
}

pub(super) fn channel() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Channel Blur", "Channel Blur", "Blur & Sharpen");
    b = b.slider(
        1,
        "Red Blurriness",
        "红色模糊度",
        0.0,
        [0.0, 32767.0],
        [0.0, 32767.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Green Blurriness",
        "绿色模糊度",
        0.0,
        [0.0, 32767.0],
        [0.0, 32767.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Blue Blurriness",
        "蓝色模糊度",
        0.0,
        [0.0, 32767.0],
        [0.0, 32767.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Alpha Blurriness",
        "Alpha 模糊度",
        0.0,
        [0.0, 32767.0],
        [0.0, 32767.0],
        2,
        "none",
    );
    b = b.checkbox(5, "Repeat Edge Pixels", "边缘特性", true);
    b = b.popup(
        6,
        "Blur Dimensions",
        "模糊方向",
        1,
        &["Horizontal and Vertical", "Horizontal", "Vertical"],
    );
    b
}

pub(super) fn radial() -> DefBuilder {
    let mut b = DefBuilder::new("CC Radial Blur", "CC Radial Blur", "Blur & Sharpen");
    b = b.popup(
        1,
        "Type",
        "Type",
        6,
        &[
            "Straight Zoom",
            "Fading Zoom",
            "Centered Zoom",
            "(-",
            "Rotate",
            "Scratch",
            "Rotate Fading",
        ],
    );
    b = b.slider(
        2,
        "Amount",
        "Amount",
        0.0,
        [-250.0, 250.0],
        [-250.0, 250.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Quality",
        "Quality",
        50.0,
        [10.0, 100.0],
        [10.0, 100.0],
        2,
        "none",
    );
    b.push(
        4,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b
}

pub(super) fn radial_fast() -> DefBuilder {
    let mut b = DefBuilder::new(
        "CC Radial Fast Blur",
        "CC Radial Fast Blur",
        "Blur & Sharpen",
    );
    b.push(
        1,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        2,
        "Amount",
        "Amount",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.popup(3, "Zoom", "Zoom", 1, &["Standard", "Brightest", "Darkest"]);
    b
}

pub(super) fn vector() -> DefBuilder {
    let mut b = DefBuilder::new("CC Vector Blur", "CC Vector Blur", "Blur & Sharpen");
    b = b.popup(
        1,
        "Type",
        "Type",
        1,
        &[
            "Natural",
            "Constant Length",
            "Perpendicular",
            "Direction Center",
            "Direction Fading",
        ],
    );
    b = b.slider(
        2,
        "Amount",
        "Amount",
        0.0,
        [-500.0, 500.0],
        [-500.0, 500.0],
        2,
        "none",
    );
    b.push(
        3,
        "Angle Offset",
        "Angle Offset",
        ParamKind::Angle,
        Some(Value::Float(0.)),
    );
    b = b.slider(
        4,
        "Ridge Smoothness",
        "Ridge Smoothness",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.layer(5, "Vector Map", "Vector Map").layer_stage(5);
    b = b.popup(
        6,
        "Property",
        "Property",
        6,
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
        7,
        "Map Softness",
        "Map Softness",
        15.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    b
}

pub(super) fn cross() -> DefBuilder {
    let mut b = DefBuilder::new("CS CrossBlur", "CC Cross Blur", "Blur & Sharpen");
    b = b.slider(
        1,
        "Radius X",
        "Radius X",
        0.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Radius Y",
        "Radius Y",
        0.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b = b.popup(
        3,
        "Transfer Mode",
        "Transfer Mode",
        1,
        &["Blend", "Add", "Screen", "Multiply", "Lighten", "Darken"],
    );
    b = b.checkbox(4, "Repeat Edge Pixels", "Repeat Edge Pixels", true);
    b
}

pub(crate) fn options(m: &str, id: u32) -> Option<&'static [&'static str]> {
    Some(match (m, id) {
        ("ADBE Camera Lens Blur", 3) => &[
            "三角形",
            "正方形",
            "五边形",
            "六边形",
            "七边形",
            "八边形",
            "九边形",
            "十边形",
        ],
        ("ADBE Camera Lens Blur", 11) => &["明亮度", "红色", "绿色", "蓝色", "Alpha"],
        ("ADBE Camera Lens Blur", 12) => &["图居中", "拉伸图以适合"],
        ("ADBE Smart Blur", 3) => &["正常", "仅限边缘", "叠加边缘"],
        ("ADBE Channel Blur", 6) => &["水平和垂直", "水平", "垂直"],
        ("CC Radial Blur", 1) => &[
            "Straight Zoom",
            "Fading Zoom",
            "Centered Zoom",
            "(-",
            "Rotate",
            "Scratch",
            "Rotate Fading",
        ],
        ("CC Radial Fast Blur", 3) => &["Standard", "Brightest", "Darkest"],
        ("CC Vector Blur", 1) => &[
            "Natural",
            "Constant Length",
            "Perpendicular",
            "Direction Center",
            "Direction Fading",
        ],
        ("CC Vector Blur", 6) => &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Luminance",
            "Lightness",
            "Hue",
            "Saturation",
        ],
        ("CS CrossBlur", 3) => &["Blend", "Add", "Screen", "Multiply", "Lighten", "Darken"],
        _ => return None,
    })
}
