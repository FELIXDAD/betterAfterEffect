use crate::catalog::DefBuilder;
use bae_model::{EffectDef, ParamKind, Value};

pub(super) fn definition() -> (EffectDef, Vec<(u32, String)>) {
    let mut b = DefBuilder::new("ADBE Lumetri", "Lumetri Color", "Color Correction");
    let group = |mut b: DefBuilder, id, name, zh, enabled| {
        b.push(id, name, zh, ParamKind::GroupStart, None);
        b.checkbox(enabled, "Active", "现用", true)
    };
    let slider = |b: DefBuilder, id, name, zh, default, min, max| {
        b.slider(id, name, zh, default, [min, max], [min, max], 1, "none")
    };
    let asset = |b: &mut DefBuilder, id, name, zh| {
        b.push(
            id,
            name,
            zh,
            ParamKind::ArbitraryData,
            Some(Value::Bytes(Vec::new())),
        );
    };
    b = group(b, 2, "Basic Correction", "基本校正", 3);
    asset(&mut b, 125, "Input LUT", "输入 LUT");
    b = slider(b, 7, "Temperature", "色温", 0., -300., 300.);
    b = slider(b, 8, "Tint", "色调", 0., -300., 300.);
    b = slider(b, 20, "Saturation", "饱和度", 100., 0., 300.);
    b = slider(b, 11, "Exposure", "曝光度", 0., -7., 7.);
    for (id, name, zh) in [
        (12, "Contrast", "对比度"),
        (13, "Highlights", "高光"),
        (14, "Shadows", "阴影"),
        (15, "Whites", "白色"),
        (16, "Blacks", "黑色"),
    ] {
        b = slider(b, id, name, zh, 0., -150., 150.);
    }
    b.push(21, "", "", ParamKind::GroupEnd, None);
    b = group(b, 22, "Creative", "创意", 23);
    asset(&mut b, 126, "Look", "Look");
    b = slider(b, 26, "Intensity", "强度", 100., 0., 200.);
    b = slider(b, 28, "Faded Film", "淡化胶片", 0., 0., 150.);
    b = slider(b, 29, "Sharpen", "锐化", 0., -100., 100.);
    b = slider(b, 30, "Vibrance", "自然饱和度", 0., -100., 100.);
    b = slider(b, 31, "Saturation", "饱和度", 100., 0., 300.);
    // Custom AE wheel payloads have no scalar disk IDs. BAE exposes their
    // editable RGB offsets as local parameters with neutral grey at the center.
    for (id, name, zh) in [
        (1001, "Shadow Tint", "阴影色调"),
        (1002, "Highlight Tint", "高光色调"),
    ] {
        b.push(
            id,
            name,
            zh,
            ParamKind::Color,
            Some(Value::Color([0.5, 0.5, 0.5, 1.])),
        );
    }
    b = slider(b, 33, "Tint Balance", "色调平衡", 0., -150., 150.);
    b.push(35, "", "", ParamKind::GroupEnd, None);
    b = group(b, 36, "Curves", "曲线", 37);
    for (id, name, zh) in [
        (39, "RGB Curves", "RGB 曲线"),
        (42, "Hue vs Sat", "色相与饱和度"),
    ] {
        b.push(
            id,
            name,
            zh,
            ParamKind::CurvePoints,
            Some(Value::FloatList(Vec::new())),
        );
    }
    b.push(44, "", "", ParamKind::GroupEnd, None);
    b = group(b, 45, "Color Wheels", "色轮", 46);
    for (id, name, zh) in [
        (1003, "Shadows", "阴影"),
        (1004, "Midtones", "中间调"),
        (1005, "Highlights", "高光"),
    ] {
        b.push(
            id,
            name,
            zh,
            ParamKind::Color,
            Some(Value::Color([0.5, 0.5, 0.5, 1.])),
        );
    }
    for (id, name, zh) in [
        (1013, "Shadow Luminance", "阴影亮度"),
        (1014, "Midtone Luminance", "中间调亮度"),
        (1015, "Highlight Luminance", "高光亮度"),
    ] {
        b = slider(b, id, name, zh, 0., -100., 100.);
    }
    b.push(48, "", "", ParamKind::GroupEnd, None);
    b = group(b, 67, "HSL Secondary", "HSL 次要", 68);
    b.push(70, "Set Color", "设置颜色", ParamKind::Color, None);
    for (id, name, zh, default, max) in [
        (1006, "Hue Range", "色相范围", 18., 180.),
        (1007, "Saturation Range", "饱和度范围", 5., 100.),
        (1008, "Lightness Range", "亮度范围", 5., 100.),
        (1009, "Falloff", "衰减", 5., 100.),
    ] {
        b = slider(b, id, name, zh, default, 0., max);
    }
    b = b
        .checkbox(74, "Show Mask", "显示蒙版", false)
        .popup(
            75,
            "Mask View",
            "蒙版视图",
            1,
            &["Color/Gray", "Color/Black", "White/Black"],
        )
        .checkbox(76, "Invert Mask", "反转蒙版", false);
    b = slider(b, 81, "Denoise", "降噪", 0., 0., 100.);
    b = slider(b, 82, "Blur", "模糊", 0., 0., 1000.);
    b.push(
        1016,
        "Correction Color",
        "校正颜色",
        ParamKind::Color,
        Some(Value::Color([0.5, 0.5, 0.5, 1.])),
    );
    b = slider(b, 86, "Temperature", "色温", 0., -300., 300.);
    b = slider(b, 87, "Tint", "色调", 0., -300., 300.);
    b = slider(b, 88, "Contrast", "对比度", 0., -150., 150.);
    b = slider(b, 89, "Sharpen", "锐化", 0., -100., 100.);
    b = slider(b, 90, "Saturation", "饱和度", 100., 0., 300.);
    b.push(92, "", "", ParamKind::GroupEnd, None);
    b = group(b, 49, "Vignette", "晕影", 50);
    for (id, name, zh, default, min, max) in [
        (51, "Amount", "数量", 0., -5., 5.),
        (52, "Midpoint", "中点", 50., 0., 100.),
        (53, "Roundness", "圆度", 0., -100., 100.),
        (54, "Feather", "羽化", 50., 0., 100.),
    ] {
        b = slider(b, id, name, zh, default, min, max);
    }
    b.push(55, "", "", ParamKind::GroupEnd, None);
    let (mut def, names) = b.build();
    for p in &mut def.params {
        if p.id >= 1000 {
            p.match_name = None;
        }
    }
    (def, names)
}
