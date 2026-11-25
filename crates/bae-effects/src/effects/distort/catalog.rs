//! AE 26 parameter disk IDs and scripting values. Points are recorded against
//! the 640 × 360 layer used for the reference dump.
use super::*;
use crate::catalog::DefBuilder;
use bae_model::ParamKind;

impl DefBuilder {
    fn number(self, id: u32, en: &str, zh: &str, value: f64, range: [f64; 2]) -> Self {
        self.slider(id, en, zh, value, range, range, 2, "none")
    }
    fn position(&mut self, id: u32, en: &str, zh: &str, xy: [f64; 2]) {
        self.push(id, en, zh, ParamKind::Point, Some(Value::Vec2(xy)));
    }
}

fn transform() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Geometry2", "Transform", "Distort");
    b.position(1, "Anchor Point", "锚点", [320.0, 180.0]);
    b.position(2, "Position", "位置", [320.0, 180.0]);
    b = b.checkbox(11, "Uniform Scale", "统一缩放", true);
    b = b.number(3, "Scale Height", "缩放高度", 100.0, [-30000.0, 30000.0]);
    b = b.number(4, "Scale Width", "缩放宽度", 100.0, [-30000.0, 30000.0]);
    b = b.number(5, "Skew", "倾斜", 0.0, [-70.0, 70.0]);
    b = b.angle(6, "Skew Axis", "倾斜轴", 0.0);
    b = b.angle(7, "Rotation", "旋转", 0.0);
    b = b.number(8, "Opacity", "不透明度", 100.0, [0.0, 100.0]);
    b = b.checkbox(
        9,
        "Use Composition’s Shutter Angle",
        "使用合成的快门角度",
        true,
    );
    b = b.number(10, "Shutter Angle", "快门角度", 0.0, [0.0, 360.0]);
    b = b.popup(12, "Sampling", "采样", 1, &["Bilinear", "Bicubic"]);
    b
}

fn turbulent_displace() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Turbulent Displace", "Turbulent Displace", "Distort");
    b = b.popup(
        1,
        "Displacement",
        "置换",
        1,
        &[
            "Turbulent",
            "Bulge",
            "Twist",
            "(-",
            "Turbulent Smoother",
            "Bulge Smoother",
            "Twist Smoother",
            "(-",
            "Vertical Displacement",
            "Horizontal Displacement",
            "Cross Displacement",
        ],
    );
    b = b.number(2, "Amount", "数量", 50.0, [-10000.0, 10000.0]);
    b = b.number(3, "Size", "大小", 100.0, [2.0, 1000.0]);
    b.position(4, "Offset (Turbulence)", "偏移（湍流）", [320.0, 180.0]);
    b = b.number(5, "Complexity", "复杂度", 1.0, [1.0, 10.0]);
    b = b.angle(6, "Evolution", "演化", 0.0);
    b.push(
        7,
        "Evolution Options",
        "演化选项",
        ParamKind::GroupStart,
        None,
    );
    b = b.checkbox(8, "Cycle Evolution", "循环演化", false);
    b = b.int(9, "Cycle (in Revolutions)", "循环（旋转次数）", 1, [1, 88]);
    b = b.int(10, "Random Seed", "随机植入", 0, [0, 100000]);
    b.push(11, "Random Seed", "随机植入", ParamKind::GroupEnd, None);
    b = b.popup(
        12,
        "Pinning",
        "固定",
        3,
        &[
            "None",
            "(-",
            "Pin All",
            "Pin Horizontal",
            "Pin Vertical",
            "Pin Left",
            "Pin Right",
            "Pin Top",
            "Pin Bottom",
            "(-",
            "Pin All Locked",
            "Pin Horizontal Locked",
            "Pin Vertical Locked",
            "Pin Left Locked",
            "Pin Right Locked",
            "Pin Top Locked",
            "Pin Bottom Locked",
        ],
    );
    b = b.checkbox(13, "Resize Layer", "调整图层大小", false);
    b = b.popup(
        14,
        "Antialiasing for Best Quality",
        "消除锯齿（最佳品质）",
        1,
        &["Low", "High"],
    );
    b
}

fn wave_warp() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Wave Warp", "Wave Warp", "Distort");
    b = b.popup(
        1,
        "Wave Type",
        "波浪类型",
        1,
        &[
            "Sine",
            "Square",
            "Triangle",
            "Sawtooth",
            "Circle",
            "Semicircle",
            "Uncircle",
            "Noise",
            "Smooth Noise",
        ],
    );
    b = b.int(2, "Wave Height", "波形高度", 10, [-32000, 32000]);
    b = b.int(3, "Wave Width", "波形宽度", 40, [1, 32000]);
    b = b.angle(4, "Direction", "方向", 90.0);
    b = b.number(5, "Wave Speed", "波形速度", 1.0, [-100.0, 100.0]);
    b = b.popup(
        6,
        "Pinning",
        "固定",
        1,
        &[
            "None",
            "All Edges",
            "Center",
            "(-",
            "Left Edge",
            "Top Edge",
            "Right Edge",
            "Bottom Edge",
            "(-",
            "Horizontal Edges",
            "Vertical Edges",
        ],
    );
    b = b.angle(7, "Phase", "相位", 0.0);
    b = b.popup(
        8,
        "Antialiasing (Best Quality)",
        "消除锯齿（最佳品质）",
        1,
        &["Low", "Medium", "High"],
    );
    b
}

fn bulge() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Bulge", "Bulge", "Distort");
    b = b.number(1, "Horizontal Radius", "水平半径", 50.0, [0.0, 8000.0]);
    b = b.number(2, "Vertical Radius", "垂直半径", 50.0, [0.0, 8000.0]);
    b.position(3, "Bulge Center", "凸出中心", [320.0, 180.0]);
    b = b.number(4, "Bulge Height", "凸出高度", 1.0, [-4.0, 4.0]);
    b = b.number(5, "Taper Radius", "锥形半径", 0.0, [0.0, 8000.0]);
    b = b.popup(
        6,
        "Antialiasing (Best Qual Only)",
        "为最佳品质消除锯齿",
        1,
        &["Low", "High"],
    );
    b = b.checkbox(7, "Pinning", "固定", false);
    b
}

fn twirl() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Twirl", "Twirl", "Distort");
    b = b.angle(1, "Angle", "角度", 0.0);
    b = b.number(2, "Twirl Radius", "旋转扭曲半径", 30.0, [0.0, 100.0]);
    b.position(3, "Twirl Center", "旋转扭曲中心", [320.0, 180.0]);
    b
}

fn polar_coordinates() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Polar Coordinates", "Polar Coordinates", "Distort");
    b = b.number(1, "Interpolation", "插值", 0.0, [0.0, 1.0]);
    b = b.popup(
        2,
        "Type of Conversion",
        "转换类型",
        2,
        &["Rect to Polar", "Polar to Rect"],
    );
    b
}

fn corner_pin() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Corner Pin", "Corner Pin", "Distort");
    b.position(1, "Upper Left", "左上", [0.0, 0.0]);
    b.position(2, "Upper Right", "右上", [640.0, 0.0]);
    b.position(3, "Lower Left", "左下", [0.0, 360.0]);
    b.position(4, "Lower Right", "右下", [640.0, 360.0]);
    b
}

fn offset() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Offset", "Offset", "Distort");
    b.position(1, "Shift Center To", "将中心转换为", [320.0, 180.0]);
    b = b.number(2, "Blend With Original", "与原始图像混合", 0.0, [0.0, 1.0]);
    b
}

fn mirror() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Mirror", "Mirror", "Distort");
    b.position(1, "Reflection Center", "反射中心", [640.0, 180.0]);
    b = b.angle(2, "Reflection Angle", "反射角度", 0.0);
    b
}

fn spherize() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Spherize", "Spherize", "Distort");
    b = b.number(1, "Radius", "半径", 0.0, [0.0, 2500.0]);
    b.position(2, "Center of Sphere", "球面中心", [320.0, 180.0]);
    b
}

fn ripple() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Ripple", "Ripple", "Distort");
    b = b.number(1, "Radius", "半径", 0.0, [0.0, 100.0]);
    b.position(2, "Center of Ripple", "波纹中心", [320.0, 180.0]);
    b = b.popup(
        3,
        "Type of Conversion",
        "转换类型",
        1,
        &["Asymmetric", "Symmetric"],
    );
    b = b.number(4, "Wave Speed", "波形速度", 1.0, [-15.0, 15.0]);
    b = b.number(5, "Wave Width", "波形宽度", 20.0, [2.0, 100.0]);
    b = b.number(6, "Wave Height", "波形高度", 20.0, [0.0, 400.0]);
    b = b.angle(7, "Ripple Phase", "波纹相", 0.0);
    b
}

fn optics_compensation() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Optics Compensation", "Optics Compensation", "Distort");
    b = b.number(
        1,
        "Field Of View (FOV)",
        "视场 (FOV)",
        0.0,
        [0.0, 179.999985],
    );
    b = b.checkbox(2, "Reverse Lens Distortion", "反转镜头扭曲", false);
    b = b.popup(
        3,
        "FOV Orientation",
        "FOV 方向",
        1,
        &["Horizontal", "Vertical", "Diagonal"],
    );
    b.position(4, "View Center", "视图中心", [320.0, 180.0]);
    b = b.checkbox(
        5,
        "Optimal Pixels (Invalidates Reversal)",
        "最佳像素（反转无效）",
        false,
    );
    b = b.popup(
        6,
        "Resize",
        "调整大小",
        1,
        &["Off", "Max 2X", "Max 4X", "Unlimited"],
    );
    b
}

pub(crate) fn entries() -> Vec<Builtin> {
    [
        (Kind::Transform, transform(), "transform", "变换"),
        (
            Kind::TurbulentDisplace,
            turbulent_displace(),
            "turbulent-displace",
            "湍流置换",
        ),
        (Kind::WaveWarp, wave_warp(), "wave-warp", "波形变形"),
        (Kind::Bulge, bulge(), "bulge", "凸出"),
        (Kind::Twirl, twirl(), "twirl", "旋转扭曲"),
        (
            Kind::PolarCoordinates,
            polar_coordinates(),
            "polar-coordinates",
            "极坐标",
        ),
        (Kind::CornerPin, corner_pin(), "corner-pin", "边角定位"),
        (Kind::Offset, offset(), "offset", "偏移"),
        (Kind::Mirror, mirror(), "mirror", "镜像"),
        (Kind::Spherize, spherize(), "spherize", "球面化"),
        (Kind::Ripple, ripple(), "ripple", "波纹"),
        (
            Kind::OpticsCompensation,
            optics_compensation(),
            "optics-compensation",
            "光学补偿",
        ),
    ]
    .into_iter()
    .map(|(kind, b, id, zh)| {
        let (def, names) = b.build();
        rust_builtin(
            (Arc::new(Distortion { kind, def }), names),
            id,
            zh,
            &[id, zh],
        )
    })
    .collect()
}
/// Chinese popup labels in AE menu order.
pub fn options_zh(name: &str, id: u32) -> Option<&'static [&'static str]> {
    Some(match (name, id) {
        ("ADBE Geometry2", 12) => &["双线性", "双立方"],
        ("ADBE Turbulent Displace", 1) => &[
            "湍流",
            "凸出",
            "扭转",
            "(-",
            "湍流较平滑",
            "凸出较平滑",
            "扭转较平滑",
            "(-",
            "垂直置换",
            "水平置换",
            "交叉置换",
        ],
        ("ADBE Turbulent Displace", 12) => &[
            "无",
            "(-",
            "全部固定",
            "水平固定",
            "垂直固定",
            "左侧固定",
            "右侧固定",
            "顶部固定",
            "底部固定",
            "(-",
            "锁定全部固定",
            "锁定水平固定",
            "锁定垂直固定",
            "锁定左侧固定",
            "锁定右侧固定",
            "锁定顶部固定",
            "锁定底部固定",
        ],
        ("ADBE Turbulent Displace", 14) => &["低", "高"],
        ("ADBE Wave Warp", 1) => &[
            "正弦",
            "正方形",
            "三角形",
            "锯齿",
            "圆形",
            "半圆形",
            "非圆形",
            "杂色",
            "平滑杂色",
        ],
        ("ADBE Wave Warp", 6) => &[
            "无",
            "全部边缘",
            "居中",
            "(-",
            "左侧边缘",
            "顶部边缘",
            "右侧边缘",
            "底部边缘",
            "(-",
            "水平边缘",
            "垂直边缘",
        ],
        ("ADBE Wave Warp", 8) => &["低", "中", "高"],
        ("ADBE Bulge", 6) => &["低", "高"],
        ("ADBE Polar Coordinates", 2) => &["矩形到极线", "极线到矩形"],
        ("ADBE Ripple", 3) => &["不对称", "对称"],
        ("ADBE Optics Compensation", 3) => &["水平", "垂直", "对角"],
        ("ADBE Optics Compensation", 6) => &["关闭", "最大 2X", "最大 4X", "无限"],
        _ => return None,
    })
}
