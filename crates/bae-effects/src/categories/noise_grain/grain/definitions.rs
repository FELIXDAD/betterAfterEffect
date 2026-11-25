use crate::catalog::DefBuilder;
use bae_model::{ParamKind, Value};

pub(super) fn match_grain() -> DefBuilder {
    let mut b = DefBuilder::new("VISINF Grain Duplication", "Match Grain", "Noise & Grain");
    b = b.popup(
        2,
        "Viewing Mode",
        "查看模式",
        1,
        &[
            "Preview",
            "Noise Samples",
            "Compensation Samples",
            "Blending Matte",
            "Final Output",
        ],
    );
    b = b
        .layer(13, "Noise Source Layer", "杂色源图层")
        .layer_stage(13);
    b.push(
        200,
        "Preview Region",
        "预览区域",
        ParamKind::GroupStart,
        None,
    );
    b.push(
        14,
        "Center",
        "中心",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        15,
        "Width",
        "宽度",
        200.0,
        [50.0, 200.0],
        [50.0, 200.0],
        2,
        "none",
    );
    b = b.slider(
        16,
        "Height",
        "高度",
        200.0,
        [50.0, 200.0],
        [50.0, 200.0],
        2,
        "none",
    );
    b = b.checkbox(117, "Show Box", "显示方框", true);
    b.push(
        115,
        "Box Color",
        "方框颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b.push(201, "", "", ParamKind::GroupEnd, None);
    b = b.slider(
        118,
        "Compensate For Existing Noise",
        "补偿现有杂色",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(204, "Tweaking", "微调", ParamKind::GroupStart, None);
    b = b.slider(
        21,
        "Intensity",
        "强度",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        119,
        "Size",
        "大小",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        232,
        "Softness",
        "柔和度",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    b = b.slider(
        120,
        "Aspect Ratio",
        "长宽比",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    b.push(
        206,
        "Channel Intensities",
        "通道强度",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        22,
        "Red Intensity",
        "红色强度",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        23,
        "Green Intensity",
        "绿色强度",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        24,
        "Blue Intensity",
        "蓝色强度",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(207, "", "", ParamKind::GroupEnd, None);
    b.push(220, "Channel Size", "通道大小", ParamKind::GroupStart, None);
    b = b.slider(
        122,
        "Red Size",
        "红色大小",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        123,
        "Green Size",
        "绿色大小",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        124,
        "Blue Size",
        "蓝色大小",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b.push(221, "", "", ParamKind::GroupEnd, None);
    b.push(205, "", "", ParamKind::GroupEnd, None);
    b.push(226, "Color", "颜色", ParamKind::GroupStart, None);
    b = b.checkbox(31, "Monochrome", "单色", false);
    b = b.slider(
        20,
        "Saturation",
        "饱和度",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    b = b.slider(
        114,
        "Tint Amount",
        "色调量",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(
        25,
        "Tint Color",
        "色调颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b.push(227, "", "", ParamKind::GroupEnd, None);
    b.push(216, "Application", "应用", ParamKind::GroupStart, None);
    b = b.popup(
        29,
        "Blending Mode",
        "混合模式",
        3,
        &["Film", "Multiply", "Add", "Screen", "Overlay"],
    );
    b = b.slider(
        128,
        "Shadows",
        "阴影",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        129,
        "Midtones",
        "中间调",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        130,
        "Highlights",
        "高光",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        131,
        "Midpoint",
        "中点",
        0.5,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b.push(
        210,
        "Channel Balance",
        "通道平衡",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        132,
        "Red Shadows",
        "红色阴影",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        133,
        "Red Midtones",
        "红色中间调",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        134,
        "Red Highlights",
        "红色高光",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        135,
        "Green Shadows",
        "绿色阴影",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        136,
        "Green Midtones",
        "绿色中间调",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        137,
        "Green Highlights",
        "绿色高光",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        138,
        "Blue Shadows",
        "蓝色阴影",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        139,
        "Blue Midtones",
        "蓝色中间调",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        140,
        "Blue Highlights",
        "蓝色高光",
        1.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b.push(211, "", "", ParamKind::GroupEnd, None);
    b.push(217, "", "", ParamKind::GroupEnd, None);
    b.push(202, "Sampling", "采样", ParamKind::GroupStart, None);
    b = b.slider(
        141,
        "Source Frame",
        "源帧",
        0.0,
        [0.0, 100000.0],
        [0.0, 100000.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Number of Samples",
        "样本数量",
        8.0,
        [1.0, 10.0],
        [1.0, 10.0],
        2,
        "none",
    );
    b = b.slider(
        11,
        "Sample Size",
        "样本大小",
        24.0,
        [16.0, 256.0],
        [16.0, 256.0],
        2,
        "none",
    );
    b.push(
        142,
        "Sample Box Color",
        "采样框颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        18,
        "Sample Selection",
        "样本选择",
        1,
        &["Automatic", "Manual"],
    );
    b.push(
        212,
        "Noise Sample Points",
        "杂色样本点",
        ParamKind::GroupStart,
        None,
    );
    b.push(
        50,
        "1",
        "1",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        51,
        "2",
        "2",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        52,
        "3",
        "3",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        53,
        "4",
        "4",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        54,
        "5",
        "5",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        55,
        "6",
        "6",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        56,
        "7",
        "7",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        57,
        "8",
        "8",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        58,
        "9",
        "9",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        59,
        "10",
        "10",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(213, "", "", ParamKind::GroupEnd, None);
    b.push(214, "Intensity", "强度", ParamKind::GroupStart, None);
    b.push(
        70,
        "1",
        "1",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        71,
        "2",
        "2",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        72,
        "3",
        "3",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        73,
        "4",
        "4",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        74,
        "5",
        "5",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        75,
        "6",
        "6",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        76,
        "7",
        "7",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        77,
        "8",
        "8",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        78,
        "9",
        "9",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        79,
        "10",
        "10",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(215, "", "", ParamKind::GroupEnd, None);
    b.push(203, "", "", ParamKind::GroupEnd, None);
    b.push(224, "Animation", "动画", ParamKind::GroupStart, None);
    b = b.slider(
        155,
        "Animation Speed",
        "动画速度",
        1.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(154, "Animate Smoothly", "动画流畅", false);
    b = b.slider(
        17,
        "Random Seed",
        "随机植入",
        0.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b.push(225, "", "", ParamKind::GroupEnd, None);
    b.push(
        218,
        "Blend With Original",
        "与原始图像混合",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        143,
        "Amount",
        "数量",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.popup(
        153,
        "Combine Match and Mask Using",
        "结合匹配和蒙版使用",
        1,
        &["Multiply", "Screen"],
    );
    b = b.slider(
        149,
        "Blur Matte",
        "模糊遮罩",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(
        228,
        "Color Matching",
        "颜色匹配",
        ParamKind::GroupStart,
        None,
    );
    b = b.popup(
        147,
        "Match Color Using",
        "匹配颜色使用",
        3,
        &[
            "Off",
            "(-",
            "RGB",
            "Hue",
            "Chroma",
            "Luminance",
            "Red Channel",
            "Green Channel",
            "Blue Channel",
        ],
    );
    b.push(
        144,
        "Match Colors",
        "匹配颜色",
        ParamKind::Color,
        Some(Value::Color([0.498039, 0.498039, 0.498039, 0.498039])),
    );
    b = b.slider(
        145,
        "Matching Tolerance",
        "匹配容差",
        0.099991,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        146,
        "Matching Softness",
        "匹配柔和度",
        0.099991,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(148, "Invert Match", "反转匹配", false);
    b.push(229, "", "", ParamKind::GroupEnd, None);
    b.push(230, "Mask Layer", "蒙版图层", ParamKind::GroupStart, None);
    b = b.layer(150, "Mask Layer", "蒙版图层").layer_stage(150);
    b = b.popup(
        152,
        "Masking Mode",
        "蒙版模式",
        1,
        &["Intensity", "Alpha", "Inverted Intensity", "Inverted Alpha"],
    );
    b = b.popup(
        151,
        "If Layer Sizes Differ",
        "如果蒙版大小不同",
        1,
        &["Center", "Stretch to Fit"],
    );
    b.push(231, "", "", ParamKind::GroupEnd, None);
    b.push(219, "", "", ParamKind::GroupEnd, None);
    b
}

pub(super) fn remove_grain() -> DefBuilder {
    let mut b = DefBuilder::new("VISINF Grain Removal", "Remove Grain", "Noise & Grain");
    b = b.popup(
        15,
        "Viewing Mode",
        "查看模式",
        1,
        &["Preview", "Noise Samples", "Blending Matte", "Final Output"],
    );
    b.push(
        100,
        "Preview Region",
        "预览区域",
        ParamKind::GroupStart,
        None,
    );
    b.push(
        16,
        "Center",
        "中心",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        18,
        "Width",
        "宽度",
        200.0,
        [50.0, 1024.0],
        [50.0, 1024.0],
        2,
        "none",
    );
    b = b.slider(
        19,
        "Height",
        "高度",
        200.0,
        [50.0, 1024.0],
        [50.0, 1024.0],
        2,
        "none",
    );
    b = b.checkbox(56, "Show Box", "显示方框", true);
    b.push(
        57,
        "Box Color",
        "方框颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b.push(101, "", "", ParamKind::GroupEnd, None);
    b.push(
        108,
        "Noise Reduction Settings",
        "杂色深度减低设置",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        1,
        "Noise Reduction",
        "杂色深度减低",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(20, "Passes", "成功", 3.0, [1.0, 6.0], [1.0, 6.0], 2, "none");
    b = b.popup(59, "Mode", "模式", 1, &["Multichannel", "Single Channel"]);
    b.push(
        104,
        "Channel Noise Reduction",
        "通道杂色深度减低",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        65,
        "Red Noise Reduction",
        "红色杂色深度减低",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        66,
        "Green Noise Reduction",
        "绿色杂色深度减低",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        67,
        "Blue Noise Reduction",
        "蓝色杂色深度减低",
        1.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(105, "", "", ParamKind::GroupEnd, None);
    b.push(109, "", "", ParamKind::GroupEnd, None);
    b.push(118, "Tweaking", "微调", ParamKind::GroupStart, None);
    b = b.slider(
        64,
        "Chroma Suppression",
        "色度抑制",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    b = b.slider(
        62,
        "Texture",
        "纹理",
        0.099991,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        60,
        "Noise Size Bias",
        "杂点大小偏差",
        0.0,
        [-1.0, 1.0],
        [-1.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(63, "Clean Solid Areas", "清理固态区域", false);
    b.push(119, "", "", ParamKind::GroupEnd, None);
    b.push(
        114,
        "Temporal Filtering",
        "临时过滤",
        ParamKind::GroupStart,
        None,
    );
    b = b.checkbox(22, "Enable", "启用", false);
    b = b.slider(
        14,
        "Amount",
        "数量",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        61,
        "Motion Sensitivity",
        "运动敏感度",
        0.899994,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b.push(115, "", "", ParamKind::GroupEnd, None);
    b.push(110, "Unsharp Mask", "钝化蒙版", ParamKind::GroupStart, None);
    b = b.slider(2, "Amount", "数量", 0.0, [0.0, 5.0], [0.0, 5.0], 2, "none");
    b = b.slider(
        50,
        "Radius",
        "半径",
        1.0,
        [0.0, 24.0],
        [0.0, 24.0],
        2,
        "none",
    );
    b = b.slider(
        51,
        "Threshold",
        "阈值",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b.push(111, "", "", ParamKind::GroupEnd, None);
    b.push(102, "Sampling", "采样", ParamKind::GroupStart, None);
    b = b.slider(
        58,
        "Source Frame",
        "源帧",
        0.0,
        [0.0, 100000.0],
        [0.0, 100000.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Number of Samples",
        "样本数量",
        8.0,
        [1.0, 10.0],
        [1.0, 10.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Sample Size",
        "样本大小",
        24.0,
        [16.0, 256.0],
        [16.0, 256.0],
        2,
        "none",
    );
    b.push(
        54,
        "Sample Box Color",
        "采样框颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        21,
        "Sample Selection",
        "样本选择",
        1,
        &["Automatic", "Manual"],
    );
    b.push(
        116,
        "Noise Sample Points",
        "杂色样本点",
        ParamKind::GroupStart,
        None,
    );
    b.push(
        30,
        "1",
        "1",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        31,
        "2",
        "2",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        32,
        "3",
        "3",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        33,
        "4",
        "4",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        34,
        "5",
        "5",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        35,
        "6",
        "6",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        36,
        "7",
        "7",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        37,
        "8",
        "8",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        38,
        "9",
        "9",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(
        39,
        "10",
        "10",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b.push(117, "", "", ParamKind::GroupEnd, None);
    b.push(103, "", "", ParamKind::GroupEnd, None);
    b.push(
        112,
        "Blend With Original",
        "与原始图像混合",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        8,
        "Amount",
        "数量",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.popup(
        72,
        "Combine Match and Mask Using",
        "结合匹配和蒙版使用",
        1,
        &["Multiply", "Screen"],
    );
    b = b.slider(
        68,
        "Blur Matte",
        "模糊遮罩",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(
        120,
        "Color Matching",
        "颜色匹配",
        ParamKind::GroupStart,
        None,
    );
    b = b.popup(
        23,
        "Match Color Using",
        "匹配颜色使用",
        3,
        &[
            "Off",
            "(-",
            "RGB",
            "Hue",
            "Chroma",
            "Luminance",
            "Red Channel",
            "Green Channel",
            "Blue Channel",
        ],
    );
    b.push(
        9,
        "Match Colors",
        "匹配颜色",
        ParamKind::Color,
        Some(Value::Color([0.498039, 0.498039, 0.498039, 0.498039])),
    );
    b = b.slider(
        10,
        "Matching Tolerance",
        "匹配容差",
        0.099991,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        11,
        "Matching Softness",
        "匹配柔和度",
        0.099991,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(12, "Invert Match", "反转匹配", false);
    b.push(121, "", "", ParamKind::GroupEnd, None);
    b.push(122, "Mask Layer", "蒙版图层", ParamKind::GroupStart, None);
    b = b.layer(69, "Mask Layer", "蒙版图层").layer_stage(69);
    b = b.popup(
        71,
        "Masking Mode",
        "蒙版模式",
        1,
        &["Intensity", "Alpha", "Inverted Intensity", "Inverted Alpha"],
    );
    b = b.popup(
        70,
        "If Layer Sizes Differ",
        "如果蒙版大小不同",
        1,
        &["Center", "Stretch to Fit"],
    );
    b.push(123, "", "", ParamKind::GroupEnd, None);
    b.push(113, "", "", ParamKind::GroupEnd, None);
    b
}

pub(crate) fn options(m: &str, id: u32) -> Option<&'static [&'static str]> {
    Some(match (m, id) {
        ("VISINF Grain Duplication", 2) => {
            &["预览", "杂色样本", "补偿范例", "混合遮罩", "最终输出"]
        }
        ("VISINF Grain Duplication", 29) => &["胶片", "相乘", "相加", "滤色", "叠加"],
        ("VISINF Grain Duplication", 18) => &["自动", "手动"],
        ("VISINF Grain Duplication", 153) => &["相乘", "滤色"],
        ("VISINF Grain Duplication", 147) => &[
            "关闭",
            "(-",
            "RGB",
            "色相",
            "色度",
            "明亮度",
            "红色通道",
            "绿色通道",
            "蓝色通道",
        ],
        ("VISINF Grain Duplication", 152) => &["强度", "Alpha", "反转强度", "反转 Alpha"],
        ("VISINF Grain Duplication", 151) => &["居中", "伸缩以适合"],
        ("VISINF Grain Removal", 15) => &["预览", "杂色样本", "混合遮罩", "最终输出"],
        ("VISINF Grain Removal", 59) => &["多通道", "单通道"],
        ("VISINF Grain Removal", 21) => &["自动", "手动"],
        ("VISINF Grain Removal", 72) => &["相乘", "滤色"],
        ("VISINF Grain Removal", 23) => &[
            "关闭",
            "(-",
            "RGB",
            "色相",
            "色度",
            "明亮度",
            "红色通道",
            "绿色通道",
            "蓝色通道",
        ],
        ("VISINF Grain Removal", 71) => &["强度", "Alpha", "反转强度", "反转 Alpha"],
        ("VISINF Grain Removal", 70) => &["居中", "伸缩以适合"],
        _ => return None,
    })
}
