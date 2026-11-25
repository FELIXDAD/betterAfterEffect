use super::*;
use crate::catalog::DefBuilder;
use bae_model::ParamKind;

fn arithmetic() -> Builtin {
    let mut b = DefBuilder::new("ADBE Arithmetic", "Arithmetic", "Channel");
    b = b.popup(
        1,
        "Operator",
        "运算符",
        3,
        &[
            "And",
            "Or",
            "Xor",
            "Add",
            "Subtract",
            "Difference",
            "Minimum",
            "Maximum",
            "Block Above",
            "Block Below",
            "Slice",
            "Multiply",
            "Screen",
        ],
    );
    b = b.slider(
        2,
        "Red Value",
        "红色值",
        0.0,
        [0.0, 255.0],
        [0.0, 255.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Green Value",
        "绿色值",
        0.0,
        [0.0, 255.0],
        [0.0, 255.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Blue Value",
        "蓝色值",
        0.0,
        [0.0, 255.0],
        [0.0, 255.0],
        2,
        "none",
    );
    b = b.checkbox(5, "Clip", "剪切", true);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 1,
                label: "bae.effects/channel_blend",
                shader: BLEND,
                inputs: &[],
                bounds: unchanged,
            }),
            labels,
        ),
        "arithmetic",
        "算术",
        &["arithmetic", "算术"],
    );
    effect.info.param_options_zh.push((
        1,
        [
            "与",
            "或",
            "异或",
            "相加",
            "相减",
            "差值",
            "最小值",
            "最大值",
            "上界",
            "下界",
            "限制",
            "相乘",
            "滤色",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn blend() -> Builtin {
    let mut b = DefBuilder::new("ADBE Blend", "Blend", "Channel");
    b = b.layer(1, "Blend With Layer", "与图层混合").layer_stage(1);
    b = b.popup(
        2,
        "Mode",
        "模式",
        1,
        &[
            "Crossfade",
            "Color Only",
            "Tint Only",
            "Darken Only",
            "Lighten Only",
        ],
    );
    b = b.slider(
        3,
        "Blend With Original",
        "与原始图像混合",
        1.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b = b.popup(
        4,
        "If Layer Sizes Differ",
        "如果图层大小不同",
        1,
        &["Center", "Stretch to Fit"],
    );
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 2,
                label: "bae.effects/channel_blend",
                shader: BLEND,
                inputs: &[(1, false)],
                bounds: unchanged,
            }),
            labels,
        ),
        "blend",
        "混合",
        &["blend", "混合"],
    );
    effect.info.param_options_zh.push((
        2,
        ["交叉淡化", "仅颜色", "仅色调", "仅变暗", "仅变亮"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect
        .info
        .param_options_zh
        .push((4, ["居中", "伸缩以适合"].map(str::to_owned).to_vec()));
    effect
}

fn calculations() -> Builtin {
    let mut b = DefBuilder::new("ADBE Calculations", "Calculations", "Channel");
    b.push(1, "Input", "输入", ParamKind::GroupStart, None);
    b = b.popup(
        2,
        "Input Channel",
        "输入通道",
        1,
        &["RGBA", "Gray", "Red", "Green", "Blue", "Alpha"],
    );
    b = b.checkbox(3, "Invert Input", "反转输入", false);
    b.push(4, "", "", ParamKind::GroupEnd, None);
    b.push(5, "Second Source", "第二个源", ParamKind::GroupStart, None);
    b = b.layer(6, "Second Layer", "第二个图层").layer_stage(6);
    b = b.popup(
        7,
        "Second Layer Channel",
        "第二个图层通道",
        1,
        &["RGBA", "Gray", "Red", "Green", "Blue", "Alpha"],
    );
    b = b.slider(
        8,
        "Second Layer Opacity",
        "第二个图层不透明度",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b = b.checkbox(9, "Invert Second Layer", "反转第二个图层", false);
    b = b.checkbox(
        10,
        "Stretch Second Layer to Fit",
        "伸缩第二个图层以适合",
        true,
    );
    b.push(11, "", "", ParamKind::GroupEnd, None);
    b = b.popup(
        12,
        "Blending Mode",
        "混合模式",
        1,
        &[
            "Normal",
            "Copy",
            "(-",
            "Darken",
            "Multiply",
            "Color Burn",
            "Classic Color Burn",
            "(-",
            "Add",
            "Lighten",
            "Screen",
            "Color Dodge",
            "Classic Color Dodge",
            "(-",
            "Overlay",
            "Soft Light",
            "Hard Light",
            "Linear Light",
            "Vivid Light",
            "Pin Light",
            "(-",
            "Difference",
            "Classic Difference",
            "Exclusion",
            "(-",
            "Hue",
            "Saturation",
            "Color",
            "Luminosity",
            "(-",
            "Stencil Alpha",
            "Stencil Luma",
            "Silhouette Alpha",
            "Silhouette Luma",
            "(-",
            "Alpha Add",
            "Luminescent Premul",
        ],
    );
    b = b.checkbox(13, "Preserve Transparency", "保持透明度", true);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 3,
                label: "bae.effects/channel_blend",
                shader: BLEND,
                inputs: &[(6, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "calculations",
        "计算",
        &["calculations", "计算"],
    );
    effect.info.param_options_zh.push((
        2,
        ["RGBA", "灰色", "红色", "绿色", "蓝色", "Alpha"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect.info.param_options_zh.push((
        7,
        ["RGBA", "灰色", "红色", "绿色", "蓝色", "Alpha"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect.info.param_options_zh.push((
        12,
        [
            "正常",
            "复制",
            "(-",
            "变暗",
            "相乘",
            "颜色加深",
            "传统颜色加深",
            "(-",
            "相加",
            "变亮",
            "滤色",
            "颜色减淡",
            "传统颜色减淡",
            "(-",
            "叠加",
            "柔光",
            "强光",
            "线性光",
            "亮光",
            "点光",
            "(-",
            "差值",
            "传统差值",
            "排除",
            "(-",
            "色相",
            "饱和度",
            "颜色",
            "发光度",
            "(-",
            "模板 Alpha",
            "模板亮度",
            "轮廓 Alpha",
            "轮廓亮度",
            "(-",
            "Alpha 添加",
            "冷光添加",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn channel_combiner() -> Builtin {
    let mut b = DefBuilder::new("ADBE Channel Combiner", "Channel Combiner", "Channel");
    b.push(1, "Source Options", "源选项", ParamKind::GroupStart, None);
    b = b.checkbox(2, "Use Second Layer", "使用第二个图层", false);
    b = b.layer(3, "Source Layer", "源图层").layer_stage(3);
    b.push(4, "Source Layer", "源图层", ParamKind::GroupEnd, None);
    b = b.popup(
        5,
        "From",
        "自",
        1,
        &[
            "RGB to HLS",
            "HLS to RGB",
            "RGB to YUV",
            "YUV to RGB",
            "Straight to Premultiplied",
            "(-",
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "(-",
            "Hue",
            "Lightness",
            "Luminance",
            "Saturation",
            "Saturation (Composite)",
            "(-",
            "Minimum RGB",
            "Maximum RGB",
        ],
    );
    b = b.popup(
        6,
        "To",
        "至",
        7,
        &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "(-",
            "Hue",
            "Lightness",
            "Saturation",
            "(-",
            "Red Only",
            "Green Only",
            "Blue Only",
            "Alpha Only",
            "(-",
            "Hue Only",
            "Lightness Only",
            "Saturation Only",
        ],
    );
    b = b.checkbox(7, "Invert", "反转", false);
    b = b.checkbox(8, "Solid Alpha", "纯色 Alpha", false);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 0,
                label: "bae.effects/channel_combiner",
                shader: CHANNEL_COMBINER,
                inputs: &[(3, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "channel-combiner",
        "通道合成器",
        &["channel-combiner", "通道合成器"],
    );
    effect.info.param_options_zh.push((
        5,
        [
            "RGB 到 HLS",
            "HLS 到 RGB",
            "RGB 到 YUV",
            "YUV 到 RGB",
            "直接到预乘",
            "(-",
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "(-",
            "色相",
            "亮度",
            "明亮度",
            "饱和度",
            "饱和度（合成）",
            "(-",
            "RGB 最小值",
            "RGB 最大值",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect.info.param_options_zh.push((
        6,
        [
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "(-",
            "色相",
            "亮度",
            "饱和度",
            "(-",
            "仅红色",
            "仅绿色",
            "仅蓝色",
            "仅 Alpha",
            "(-",
            "仅色相",
            "仅亮度",
            "仅饱和度",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn compound_arithmetic() -> Builtin {
    let mut b = DefBuilder::new("ADBE Compound Arithmetic", "Compound Arithmetic", "Channel");
    b = b
        .layer(1, "Second Source Layer", "第二个源图层")
        .layer_stage(1);
    b = b.popup(
        2,
        "Operator",
        "运算符",
        1,
        &[
            "Copy",
            "Add",
            "Subtract",
            "Multiply",
            "Difference",
            "And",
            "Or",
            "Xor",
            "Lighten",
            "Darken",
            "Minimum",
            "Maximum",
            "Screen",
            "Overlay",
            "Hard Light",
        ],
    );
    b = b.popup(
        3,
        "Operate on Channels",
        "在通道上运算",
        1,
        &["RGB", "ARGB", "Alpha"],
    );
    b = b.popup(
        4,
        "Overflow Behavior",
        "溢出特性",
        1,
        &["Clip", "Wrap", "Scale"],
    );
    b = b.checkbox(
        5,
        "Stretch Second Source to Fit",
        "伸缩第二个源以适合",
        true,
    );
    b = b.slider(
        6,
        "Blend With Original",
        "与原始图像混合",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 4,
                label: "bae.effects/channel_blend",
                shader: BLEND,
                inputs: &[(1, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "compound-arithmetic",
        "复合运算",
        &["compound-arithmetic", "复合运算"],
    );
    effect.info.param_options_zh.push((
        2,
        [
            "复制",
            "相加",
            "相减",
            "相乘",
            "差值",
            "与",
            "或",
            "异或",
            "变亮",
            "变暗",
            "最小值",
            "最大值",
            "滤色",
            "叠加",
            "强光",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
        .info
        .param_options_zh
        .push((3, ["RGB", "ARGB", "Alpha"].map(str::to_owned).to_vec()));
    effect
        .info
        .param_options_zh
        .push((4, ["剪切", "回绕", "缩放"].map(str::to_owned).to_vec()));
    effect
}

fn set_channels() -> Builtin {
    let mut b = DefBuilder::new("ADBE Set Channels", "Set Channels", "Channel");
    b = b.layer(1, "Source Layer 1", "源图层 1").layer_stage(1);
    b = b.popup(
        2,
        "Set Red to Source 1",
        "将源 1 设置为红色",
        1,
        &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Luminance",
            "Hue",
            "Lightness",
            "Saturation",
            "Full On",
            "Full Off",
        ],
    );
    b = b.layer(3, "Source Layer 2", "源图层 2").layer_stage(3);
    b = b.popup(
        4,
        "Set Green to Source 2",
        "将 2 设置为绿色",
        2,
        &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Luminance",
            "Hue",
            "Lightness",
            "Saturation",
            "Full On",
            "Full Off",
        ],
    );
    b = b.layer(5, "Source Layer 3", "源图层 3").layer_stage(5);
    b = b.popup(
        6,
        "Set Blue to Source 3",
        "将源 3 设置为蓝色",
        3,
        &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Luminance",
            "Hue",
            "Lightness",
            "Saturation",
            "Full On",
            "Full Off",
        ],
    );
    b = b.layer(7, "Source Layer 4", "源图层 4").layer_stage(7);
    b = b.popup(
        8,
        "Set Alpha to Source 4",
        "将源 4 设置为 Alpha",
        4,
        &[
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Luminance",
            "Hue",
            "Lightness",
            "Saturation",
            "Full On",
            "Full Off",
        ],
    );
    b = b.checkbox(9, "If Layer Sizes Differ", "如果图层大小不同", true);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 0,
                label: "bae.effects/set_channels",
                shader: SET_CHANNELS,
                inputs: &[(1, false), (3, false), (5, false), (7, false)],
                bounds: unchanged,
            }),
            labels,
        ),
        "set-channels",
        "设置通道",
        &["set-channels", "设置通道"],
    );
    effect.info.param_options_zh.push((
        2,
        [
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "明亮度",
            "色相",
            "亮度",
            "饱和度",
            "完全",
            "关闭",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect.info.param_options_zh.push((
        4,
        [
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "明亮度",
            "色相",
            "亮度",
            "饱和度",
            "完全",
            "关闭",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect.info.param_options_zh.push((
        6,
        [
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "明亮度",
            "色相",
            "亮度",
            "饱和度",
            "完全",
            "关闭",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect.info.param_options_zh.push((
        8,
        [
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "明亮度",
            "色相",
            "亮度",
            "饱和度",
            "完全",
            "关闭",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

fn cc_composite() -> Builtin {
    let mut b = DefBuilder::new("CS Composite", "CC Composite", "Channel");
    b = b.layer(4, "Top Layer", "Top Layer").layer_stage(4);
    b = b.slider(
        1,
        "Opacity",
        "Opacity",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b = b.popup(
        2,
        "Transfer Mode",
        "Transfer Mode",
        3,
        &[
            "Copy",
            "Behind",
            "In front",
            "(-",
            "Darken",
            "Multiply",
            "Linear Burn",
            "Color Burn",
            "Classic Color Burn",
            "(-",
            "Add",
            "Lighten",
            "Screen",
            "Linear Dodge",
            "Color Dodge",
            "Classic Color Dodge",
            "(-",
            "Overlay",
            "Soft Light",
            "Hard Light",
            "Linear Light",
            "Vivid Light",
            "Pin Light",
            "Hard Mix",
            "(-",
            "Difference",
            "Classic Difference",
            "Exclusion",
            "(-",
            "Hue",
            "Saturation",
            "Color",
            "Luminosity",
            "(-",
            "Stencil Alpha",
            "Stencil Luma",
            "Silhouette Alpha",
            "Silhouette Luma",
            "(-",
            "Add Alpha",
            "Luminescent Premul",
        ],
    );
    b = b.checkbox(3, "RGB Only", "RGB Only", true);
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (
            Arc::new(Sampled {
                def,
                mode: 5,
                label: "bae.effects/channel_blend",
                shader: BLEND,
                inputs: &[(4, true)],
                bounds: unchanged,
            }),
            labels,
        ),
        "cc-composite",
        "CC Composite",
        &["cc-composite", "合成"],
    );
    effect.info.param_options_zh.push((
        2,
        [
            "Copy",
            "Behind",
            "In front",
            "(-",
            "Darken",
            "Multiply",
            "Linear Burn",
            "Color Burn",
            "Classic Color Burn",
            "(-",
            "Add",
            "Lighten",
            "Screen",
            "Linear Dodge",
            "Color Dodge",
            "Classic Color Dodge",
            "(-",
            "Overlay",
            "Soft Light",
            "Hard Light",
            "Linear Light",
            "Vivid Light",
            "Pin Light",
            "Hard Mix",
            "(-",
            "Difference",
            "Classic Difference",
            "Exclusion",
            "(-",
            "Hue",
            "Saturation",
            "Color",
            "Luminosity",
            "(-",
            "Stencil Alpha",
            "Stencil Luma",
            "Silhouette Alpha",
            "Silhouette Luma",
            "(-",
            "Add Alpha",
            "Luminescent Premul",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    effect
}

pub(crate) fn entries() -> Vec<Builtin> {
    vec![
        arithmetic(),
        blend(),
        calculations(),
        channel_combiner(),
        compound_arithmetic(),
        set_channels(),
        cc_composite(),
    ]
}
