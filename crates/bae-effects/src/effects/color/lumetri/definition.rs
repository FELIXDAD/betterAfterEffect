use super::menus;
use crate::catalog::DefBuilder;
use bae_model::{EffectDef, ParamKind, Value};

fn scalar(
    b: DefBuilder,
    id: u32,
    en: &str,
    zh: &str,
    default: f64,
    min: f64,
    max: f64,
) -> DefBuilder {
    b.slider(id, en, zh, default, [min, max], [min, max], 1, "none")
}

fn color(b: &mut DefBuilder, id: u32, en: &str, zh: &str, value: [f64; 4]) {
    b.push(id, en, zh, ParamKind::Color, Some(Value::Color(value)));
}

fn opaque(b: &mut DefBuilder, id: u32, en: &str, zh: &str) {
    b.push(id, en, zh, ParamKind::ArbitraryData, None);
}

fn curve(b: &mut DefBuilder, id: u32, en: &str, zh: &str) {
    b.push(
        id,
        en,
        zh,
        ParamKind::CurvePoints,
        Some(Value::FloatList(Vec::new())),
    );
}

fn basic(mut b: DefBuilder) -> DefBuilder {
    opaque(&mut b, 1, "Blob", "Blob");
    b = b.checkbox(62, "High Dynamic Range", "高动态范围", false);
    b.push(
        2,
        "Basic Correction",
        "基本校正",
        ParamKind::GroupStart,
        None,
    );
    b = b.checkbox(3, "Active", "现用", true);
    opaque(&mut b, 4, " ", " ");
    opaque(&mut b, 95, " ", " ");
    b = b.popup(5, "Input LUT", "输入 LUT", 1, menus::INPUT);
    b = b.popup(127, "Input LUT", "输入 LUT", 1, menus::INPUT);
    b = scalar(b, 63, "HDR White", "HDR 白色", 100., 100., 1000.);
    b.push(120, " ", " ", ParamKind::Button, None);
    b.push(119, " ", " ", ParamKind::Button, None);
    b = scalar(b, 123, "Intensity", "强度", 50., 0., 100.);
    b.push(6, "Color", "颜色", ParamKind::GroupStart, None);
    color(
        &mut b,
        94,
        "White Balance",
        "白平衡",
        [0.752941, 0.752941, 0.752941, 1.],
    );
    b = scalar(b, 7, "Temperature", "色温", 0., -300., 300.);
    b = scalar(b, 8, "Tint", "色调", 0., -300., 300.);
    b = scalar(b, 20, "Saturation", "饱和度", 100., 0., 300.);
    b.push(9, "", "", ParamKind::GroupEnd, None);
    b.push(10, "Light", "轻", ParamKind::GroupStart, None);
    b = scalar(b, 11, "Exposure", "曝光度", 0., -7., 7.);
    b = scalar(b, 12, "Contrast", "对比度", 0., -150., 150.);
    b = scalar(b, 13, "Highlights", "高光", 0., -150., 150.);
    b = scalar(b, 14, "Shadows", "阴影", 0., -150., 150.);
    b = scalar(b, 15, "Whites", "白色", 0., -150., 150.);
    b = scalar(b, 16, "Blacks", "黑色", 0., -150., 150.);
    b = scalar(b, 64, "HDR Highlights", "HDR 高光", 0., -150., 150.);
    b.push(17, " ", " ", ParamKind::Button, None);
    b.push(18, " ", " ", ParamKind::Button, None);
    b.push(19, "", "", ParamKind::GroupEnd, None);
    b.push(21, "", "", ParamKind::GroupEnd, None);
    b
}

fn creative(mut b: DefBuilder) -> DefBuilder {
    b.push(22, "Creative", "创意", ParamKind::GroupStart, None);
    b = b.checkbox(23, "Active", "现用", true);
    opaque(&mut b, 24, " ", " ");
    opaque(&mut b, 96, " ", " ");
    b = b.popup(25, "Look", "Look", 1, menus::LOOK);
    b = b.popup(128, "Look", "Look", 1, menus::LOOK);
    opaque(&mut b, 129, " ", " ");
    b = b.popup(130, "Color Space", "色彩空间", 27, menus::SPACES);
    b = scalar(b, 26, "Intensity", "强度", 100., 0., 200.);
    b.push(27, "Adjustments", "调整", ParamKind::GroupStart, None);
    b = scalar(b, 28, "Faded Film", "淡化胶片", 0., 0., 150.);
    b = scalar(b, 29, "Sharpen", "锐化", 0., -100., 100.);
    b = scalar(b, 30, "Vibrance", "自然饱和度", 0., -100., 100.);
    b = scalar(b, 31, "Saturation", "饱和度", 100., 0., 300.);
    opaque(&mut b, 32, "Split Toning", "分离色调");
    color(&mut b, 1001, "Shadow Tint", "阴影色调", [0.5, 0.5, 0.5, 1.]);
    color(
        &mut b,
        1002,
        "Highlight Tint",
        "高光色调",
        [0.5, 0.5, 0.5, 1.],
    );
    b = scalar(b, 33, "Tint Balance", "色调平衡", 0., -150., 150.);
    b.push(34, "", "", ParamKind::GroupEnd, None);
    b.push(35, "", "", ParamKind::GroupEnd, None);
    b
}

fn curves(mut b: DefBuilder) -> DefBuilder {
    b.push(36, "Curves", "曲线", ParamKind::GroupStart, None);
    b = b.checkbox(37, "Active", "现用", true);
    b.push(38, "RGB Curves", "RGB 曲线", ParamKind::GroupStart, None);
    b = b.checkbox(118, "Active", "现用", true);
    b = scalar(b, 65, "HDR Range", "HDR 范围", 100., 100., 10000.);
    curve(&mut b, 39, "RGB Curves", "RGB 曲线");
    b.push(40, "", "", ParamKind::GroupEnd, None);
    b.push(
        41,
        "Hue Saturation Curves",
        "色相饱和度曲线",
        ParamKind::GroupStart,
        None,
    );
    for (enable, picker, id, name, zh, picker_zh) in [
        (
            104,
            113,
            42,
            "Hue vs Sat",
            "色相与饱和度",
            "色相（与饱和度）",
        ),
        (
            105,
            114,
            106,
            "Hue vs Hue",
            "色相与色相",
            "色相（与色相）选择器",
        ),
        (
            107,
            115,
            108,
            "Hue vs Luma",
            "色相与亮度",
            "色相（与亮度）选择器",
        ),
        (
            109,
            116,
            110,
            "Luma vs Sat",
            "亮度与饱和度",
            "亮度（与饱和度）",
        ),
        (
            111,
            117,
            112,
            "Sat vs Sat",
            "饱和度与饱和度",
            "饱和度（与饱和度）",
        ),
    ] {
        b = b.checkbox(enable, "Active", "现用", true);
        color(
            &mut b,
            picker,
            &format!("{name} Picker"),
            picker_zh,
            [0., 0., 0.003922, 1.],
        );
        curve(&mut b, id, name, zh);
    }
    b.push(43, "", "", ParamKind::GroupEnd, None);
    b.push(44, "", "", ParamKind::GroupEnd, None);
    b
}

fn wheels(mut b: DefBuilder) -> DefBuilder {
    b.push(45, "Color Wheels", "色轮", ParamKind::GroupStart, None);
    b = b.checkbox(46, "Active", "现用", true);
    b.push(101, " ", " ", ParamKind::Button, None);
    b = b.checkbox(102, "Face Detection", "人脸检测", true);
    b.push(103, " ", " ", ParamKind::Button, None);
    b = scalar(b, 66, "HDR White", "HDR 白色", 100., 100., 1000.);
    opaque(&mut b, 47, "Color Wheels", "色轮");
    color(&mut b, 1003, "Shadows", "阴影", [0.5, 0.5, 0.5, 1.]);
    color(&mut b, 1004, "Midtones", "中间调", [0.5, 0.5, 0.5, 1.]);
    color(&mut b, 1005, "Highlights", "高光", [0.5, 0.5, 0.5, 1.]);
    b = scalar(b, 1013, "Shadow Luminance", "阴影亮度", 0., -100., 100.);
    b = scalar(b, 1014, "Midtone Luminance", "中间调亮度", 0., -100., 100.);
    b = scalar(b, 1015, "Highlight Luminance", "高光亮度", 0., -100., 100.);
    b.push(48, "", "", ParamKind::GroupEnd, None);
    b
}

fn secondary(mut b: DefBuilder) -> DefBuilder {
    b.push(67, "HSL Secondary", "HSL 次要", ParamKind::GroupStart, None);
    b = b.checkbox(68, "Active", "现用", true);
    b.push(69, "Key", "键", ParamKind::GroupStart, None);
    color(&mut b, 70, "Set Color", "设置颜色", [0., 0., 0., 1.]);
    color(&mut b, 71, "Add Color", "添加颜色", [0., 0., 0., 1.]);
    color(&mut b, 72, "Remove Color", "移除颜色", [0., 0., 0., 1.]);
    opaque(&mut b, 73, "HSL Sliders", "HSL 滑块");
    let [hue, sat, light] = super::key::DEFAULT_WIDTHS;
    b = scalar(b, 1006, "Hue Range", "色相范围", hue, 0., 180.);
    b = scalar(b, 1007, "Saturation Range", "饱和度范围", sat, 0., 100.);
    b = scalar(b, 1008, "Lightness Range", "亮度范围", light, 0., 100.);
    b = b.checkbox(1017, "Has Key Selection", "已有键选区", true);
    b = scalar(b, 1009, "Falloff", "衰减", 5., 0., 100.);
    b = b.checkbox(74, "Show Mask", "显示蒙版", false);
    b = b.popup(75, " ", " ", 1, menus::MASK);
    b = b.checkbox(76, "Invert Mask", "反转蒙版", false);
    b.push(77, " ", " ", ParamKind::Button, None);
    b.push(78, " ", " ", ParamKind::Button, None);
    b.push(79, " ", " ", ParamKind::GroupEnd, None);
    b.push(80, "Refine", "优化", ParamKind::GroupStart, None);
    b = scalar(b, 81, "Denoise", "降噪", 0., 0., 100.);
    b = scalar(b, 82, "Blur", "模糊", 0., 0., 1000.);
    b.push(83, "模糊", "模糊", ParamKind::GroupEnd, None);
    b.push(84, "Correction", "更正", ParamKind::GroupStart, None);
    opaque(&mut b, 85, "Color Wheel", "色轮");
    color(
        &mut b,
        1016,
        "Correction Color",
        "校正颜色",
        [0.5, 0.5, 0.5, 1.],
    );
    opaque(&mut b, 93, " ", " ");
    b = scalar(b, 86, "Temperature", "色温", 0., -300., 300.);
    b = scalar(b, 87, "Tint", "色调", 0., -300., 300.);
    b = scalar(b, 88, "Contrast", "对比度", 0., -150., 150.);
    b = scalar(b, 89, "Sharpen", "锐化", 0., -100., 100.);
    b = scalar(b, 90, "Saturation", "饱和度", 100., 0., 300.);
    b.push(91, "饱和度", "饱和度", ParamKind::GroupEnd, None);
    b.push(92, "", "", ParamKind::GroupEnd, None);
    b
}

fn vignette(mut b: DefBuilder) -> DefBuilder {
    b.push(49, "Vignette", "晕影", ParamKind::GroupStart, None);
    b = b.checkbox(50, "Active", "现用", true);
    b = scalar(b, 51, "Amount", "数量", 0., -5., 5.);
    b = scalar(b, 52, "Midpoint", "中点", 50., 0., 100.);
    b = scalar(b, 53, "Roundness", "圆度", 0., -100., 100.);
    b = scalar(b, 54, "Feather", "羽化", 50., 0., 100.);
    b.push(55, "", "", ParamKind::GroupEnd, None);
    b
}

fn project_data(mut b: DefBuilder) -> DefBuilder {
    b.push(
        56,
        "SpeedGrade Custom",
        "SpeedGrade 自定义",
        ParamKind::GroupStart,
        None,
    );
    b = b.checkbox(57, "Custom Layers", "自定义图层", true);
    b.push(58, "unused", "unused", ParamKind::Button, None);
    b.push(59, "unused", "unused", ParamKind::Button, None);
    opaque(&mut b, 60, " ", " ");
    b.push(61, "", "", ParamKind::GroupEnd, None);
    b = b.checkbox(97, "Legacy Hue and Saturation", "旧版色调和饱和度", false);
    opaque(&mut b, 98, "Embedded LUTs", "Embedded LUTs");
    opaque(
        &mut b,
        99,
        "Media Embedded LUT type",
        "Media Embedded LUT type",
    );
    opaque(
        &mut b,
        100,
        "Media Embedded Lut blob",
        "Media Embedded Lut blob",
    );
    b = scalar(
        b,
        121,
        "SemanticAutoTone Progress",
        "SemanticAutoTone Progress",
        -1.,
        -1.,
        1.,
    );
    opaque(
        &mut b,
        122,
        "Auto Tone Analytics Data",
        "Auto Tone Analytics Data",
    );
    b = b.checkbox(124, "Auto Color Active...", "自动颜色激活...", false);
    opaque(&mut b, 125, "LUTAsset", "LUTAsset");
    opaque(&mut b, 126, "LookAsset", "LookAsset");
    b
}

pub(super) fn definition() -> (EffectDef, Vec<(u32, String)>) {
    let b = DefBuilder::new("ADBE Lumetri", "Lumetri Color", "Color Correction");
    let b = basic(b);
    let b = creative(b);
    let b = curves(b);
    let b = wheels(b);
    let b = secondary(b);
    let b = vignette(b);
    let b = project_data(b);
    let (mut def, names) = b.build();
    for p in &mut def.params {
        if p.id >= 1000 {
            p.match_name = None;
            p.flags.hidden = p.id == 1017;
            continue;
        }
        p.flags.cannot_animate = !matches!(p.id,7..=8 | 11..=16 | 20 | 26 | 28..=33 |
            47 | 51..=54 | 63..=66 | 73 | 81..=82 | 85..=90 | 97 | 121 | 123);
        p.flags.cannot_time_vary = p.flags.cannot_animate;
        // The duplicate LUT indices and backing payloads remain addressable on import.
        p.flags.hidden = p.name.trim().is_empty() && !matches!(p.kind, ParamKind::Popup { .. })
            || matches!(p.id, 1 | 32 | 47 | 56..=61 | 73 | 85 | 97..=100 | 121 | 122 | 124..=129);
        p.flags.disabled = p.id == 102;
    }
    (def, names)
}
