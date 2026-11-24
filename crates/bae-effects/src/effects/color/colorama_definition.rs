use crate::catalog::DefBuilder;
use bae_model::{EffectDef, ParamKind, Value};

pub(super) fn definition() -> (EffectDef, Vec<(u32, String)>) {
    let mut b = DefBuilder::new("APC Colorama", "Colorama", "Color Correction");
    b.push(3, "Input Phase", "输入相位", ParamKind::GroupStart, None);
    b = b.popup(
        2,
        "Get Phase From",
        "获取相位，自",
        1,
        &[
            "Intensity",
            "Red",
            "Green",
            "Blue",
            "Hue",
            "Lightness",
            "Saturation",
            "Value",
            "Alpha",
            "Zero",
        ],
    );
    b = b.layer(4, "Add Phase", "添加相位");
    b = b.popup(
        6,
        "Add Phase From",
        "添加相位，自",
        1,
        &[
            "Intensity",
            "Red",
            "Green",
            "Blue",
            "Hue",
            "Lightness",
            "Saturation",
            "Value",
            "Alpha",
            "Zero",
        ],
    );
    b = b.popup(
        40,
        "Add Mode",
        "添加模式",
        1,
        &["Wrap", "Clamp", "Average", "Screen"],
    );
    b.push(
        8,
        "Phase Shift",
        "相移",
        ParamKind::Angle,
        Some(Value::Float(0.)),
    );
    b.push(15, "", "", ParamKind::GroupEnd, None);
    b.push(17, "Output Cycle", "输出循环", ParamKind::GroupStart, None);
    b = b.popup(
        10,
        "Use Preset Palette",
        "使用预设调板",
        1,
        &[
            "[none]",
            "Alpha Ramp",
            "Hue Cycle",
            "Negative",
            "Ramp Red",
            "Ramp Green",
            "Ramp Blue",
            "Ramp Grey",
            "RGB",
            "Saturation Ramp",
            "Solarize Red",
            "Solarize Green",
            "Solarize Blue",
            "Solarize Grey",
            "(-",
            "Carribean",
            "Clay",
            "Copper",
            "Deep Ocean",
            "Earthenware",
            "Fire",
            "Fire And Smoke",
            "Golden 1",
            "Golden 2",
            "Granite",
            "Horizon",
            "Leather",
            "Moldy",
            "Mossy",
            "Old Glory",
            "Rusty",
            "Sepia 1",
            "Sepia 2",
            "Skink",
            "Sunset",
        ],
    );
    b.push(
        12,
        "Output Cycle",
        "输出循环",
        ParamKind::ArbitraryData,
        Some(Value::Bytes(vec![])),
    );
    b = b.slider(
        38,
        "Cycle Repetitions",
        "循环重复次数",
        1.0,
        [0.0, 64.0],
        [0.0, 64.0],
        2,
        "none",
    );
    b = b.checkbox(14, "Interpolate Palette", "插值调板", true);
    b.push(27, "", "", ParamKind::GroupEnd, None);
    b.push(29, "Modify", "修改", ParamKind::GroupStart, None);
    b = b.popup(
        16,
        "Modify",
        "修改",
        1,
        &[
            "All",
            "Red",
            "Green",
            "Blue",
            "RG",
            "GB",
            "RB",
            "Hue",
            "Lightness",
            "Saturation",
            "HL",
            "LS",
            "HS",
            "None",
        ],
    );
    b = b.checkbox(18, "Modify Alpha", "修改 Alpha", true);
    b = b.checkbox(34, "Change Empty Pixels", "更改空像素", false);
    b.push(37, "", "", ParamKind::GroupEnd, None);
    b.push(
        39,
        "Pixel Selection",
        "像素选区",
        ParamKind::GroupStart,
        None,
    );
    b.push(
        20,
        "Matching Color",
        "匹配颜色",
        ParamKind::Color,
        Some(Value::Color([1., 0., 0., 1.])),
    );
    b = b.slider(
        22,
        "Matching Tolerance",
        "匹配容差",
        0.5,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        24,
        "Matching Softness",
        "匹配柔和度",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.popup(
        26,
        "Matching Mode",
        "匹配模式",
        1,
        &["Off", "RGB", "Hue", "Chroma"],
    );
    b.push(49, "", "", ParamKind::GroupEnd, None);
    b.push(51, "Masking", "蒙版", ParamKind::GroupStart, None);
    b = b.layer(28, "Mask Layer", "蒙版图层");
    b = b.popup(
        30,
        "Masking Mode",
        "蒙版模式",
        1,
        &[
            "Off",
            "Intensity",
            "Alpha",
            "Inverted Intensity",
            "Inverted Alpha",
        ],
    );
    b.push(57, "", "", ParamKind::GroupEnd, None);
    b = b.checkbox(32, "Composite Over Layer", "在图层上合成", true);
    b = b.slider(
        36,
        "Blend With Original",
        "与原始图像混合",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    let (mut def, names) = b.build();
    let preset = def.params.iter_mut().find(|p| p.id == 10).unwrap();
    preset.flags.cannot_animate = true;
    preset.flags.cannot_time_vary = true;
    (def, names)
}

pub(crate) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    Some(match id {
        2 | 6 => &[
            "强度",
            "红色",
            "绿色",
            "蓝色",
            "色相",
            "亮度",
            "饱和度",
            "值",
            "Alpha",
            "零",
        ],
        40 => &["回绕", "固定", "平均", "滤色"],
        10 => &[
            "[无]",
            "Alpha 渐变",
            "色相循环",
            "反向",
            "渐变红色",
            "渐变绿色",
            "渐变蓝色",
            "渐变灰色",
            "RGB",
            "饱和度渐变",
            "曝光过度红色",
            "曝光过度绿色",
            "曝光过度蓝色",
            "曝光过度灰色",
            "(-",
            "Carribean",
            "粘土",
            "铜色",
            "深海洋",
            "陶器",
            "火焰",
            "火和烟",
            "金色 1",
            "金色 2",
            "花岗岩",
            "地平线",
            "皮革",
            "发霉",
            "生苔",
            "星条光辉",
            "锈",
            "深褐色 1",
            "深褐色 2",
            "石龙子",
            "夕阳余晖",
        ],
        16 => &[
            "全部",
            "红色",
            "绿色",
            "蓝色",
            "RG",
            "GB",
            "RB",
            "色相",
            "亮度",
            "饱和度",
            "HL",
            "LS",
            "HS",
            "无",
        ],
        26 => &["关", "RGB", "色相", "色度"],
        30 => &["关", "强度", "Alpha", "反转强度", "反转 Alpha"],
        _ => return None,
    })
}
