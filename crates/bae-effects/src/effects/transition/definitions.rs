use super::*;
use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use std::sync::Arc;

pub(crate) fn card_wipe() -> Builtin {
    let mut b = DefBuilder::new("APC CardWipeCam", "Card Wipe", "Transition");
    b = b.slider(
        2,
        "Transition Completion",
        "过渡完成",
        25.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Transition Width",
        "过渡宽度",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.layer(6, "Back Layer", "背面图层").layer_stage(6);
    b = b.popup(
        8,
        "Rows & Columns",
        "行数和列数",
        1,
        &["Independent", "Columns Follow Rows"],
    );
    b = b.slider(
        10,
        "Rows",
        "行数",
        9.0,
        [1.0, 1000.0],
        [1.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        12,
        "Columns",
        "列数",
        12.0,
        [1.0, 1000.0],
        [1.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        14,
        "Card Scale",
        "卡片缩放",
        1.0,
        [0.0, 10.0],
        [0.0, 10.0],
        2,
        "none",
    );
    b = b.popup(16, "Flip Axis", "翻转轴", 1, &["X", "Y", "Random"]);
    b = b.popup(
        18,
        "Flip Direction",
        "翻转方向",
        1,
        &["Positive", "Negative", "Random"],
    );
    b = b.popup(
        20,
        "Flip Order",
        "翻转顺序",
        1,
        &[
            "Left to Right",
            "Right to Left",
            "Top to Bottom",
            "Bottom to Top",
            "Top Left to Bottom Right",
            "Top Right to Bottom Left",
            "Bottom Left to Top Right",
            "Bottom Right to Top Left",
            "Gradient",
        ],
    );
    b = b.layer(22, "Gradient Layer", "渐变图层").layer_stage(22);
    b = b.slider(
        24,
        "Timing Randomness",
        "随机时间",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        26,
        "Random Seed",
        "随机植入",
        1.0,
        [1.0, 1000.0],
        [1.0, 1000.0],
        2,
        "none",
    );
    b = b.popup(
        28,
        "Camera System",
        "摄像机系统",
        1,
        &["Camera Position", "Corner Pins", "Comp Camera"],
    );
    b.push(
        98,
        "Camera Position",
        "摄像机位置",
        ParamKind::GroupStart,
        None,
    );
    b.push(
        30,
        "X Rotation",
        "X 轴旋转",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b.push(
        32,
        "Y Rotation",
        "Y 轴旋转",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b.push(
        34,
        "Z Rotation",
        "Z 轴旋转",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b.push(
        36,
        "X,Y Position",
        "X、Y 位置",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        38,
        "Z Position",
        "Z 位置",
        2.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        40,
        "Focal Length",
        "焦距",
        70.0,
        [10.0, 1000.0],
        [10.0, 1000.0],
        2,
        "none",
    );
    b = b.popup(
        42,
        "Transform Order",
        "变换顺序",
        1,
        &[
            "Rotation XYZ, Position",
            "Rotation XZY, Position",
            "Rotation YXZ, Position",
            "Rotation YZX, Position",
            "Rotation ZXY, Position",
            "Rotation ZYX, Position",
            "Position, Rotation XYZ",
            "Position, Rotation XZY",
            "Position, Rotation YXZ",
            "Position, Rotation YZX",
            "Position, Rotation ZXY",
            "Position, Rotation ZYX",
        ],
    );
    b.push(47, "", "", ParamKind::GroupEnd, None);
    b.push(100, "Corner Pins", "边角定位", ParamKind::GroupStart, None);
    b.push(
        44,
        "Upper Left",
        "左上角",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 0.0])),
    );
    b.push(
        46,
        "Upper Right",
        "右上角",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 0.0])),
    );
    b.push(
        48,
        "Lower Left",
        "左下角",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 360.0])),
    );
    b.push(
        50,
        "Lower Right",
        "右下角",
        ParamKind::Point,
        Some(Value::Vec2([640.0, 360.0])),
    );
    b = b.checkbox(52, "Auto Focal Length", "自动焦距", false);
    b = b.slider(
        54,
        "Focal Length",
        "焦距",
        70.0,
        [10.0, 1000.0],
        [10.0, 1000.0],
        2,
        "none",
    );
    b.push(63, "", "", ParamKind::GroupEnd, None);
    b.push(102, "Lighting", "灯光", ParamKind::GroupStart, None);
    b = b.popup(
        64,
        "Light Type",
        "灯光类型",
        2,
        &["Point Source", "Distant Source", "Comp Lights First"],
    );
    b = b.slider(
        56,
        "Light Intensity",
        "灯光强度",
        1.0,
        [0.0, 50.0],
        [0.0, 50.0],
        2,
        "none",
    );
    b.push(
        58,
        "Light Color",
        "灯光颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b.push(
        60,
        "Light Position",
        "灯光位置",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        62,
        "Light Depth",
        "灯光深度",
        1.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        66,
        "Ambient Light",
        "周围光",
        0.25,
        [0.0, 2.0],
        [0.0, 2.0],
        2,
        "none",
    );
    b.push(79, "", "", ParamKind::GroupEnd, None);
    b.push(104, "Material", "材质", ParamKind::GroupStart, None);
    b = b.slider(
        68,
        "Diffuse Reflection",
        "漫反射",
        0.75,
        [0.0, 2.0],
        [0.0, 2.0],
        2,
        "none",
    );
    b = b.slider(
        70,
        "Specular Reflection",
        "镜面反射",
        0.0,
        [0.0, 2.0],
        [0.0, 2.0],
        2,
        "none",
    );
    b = b.slider(
        72,
        "Highlight Sharpness",
        "高光锐度",
        10.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(89, "", "", ParamKind::GroupEnd, None);
    b.push(
        91,
        "Position Jitter",
        "位置抖动",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        74,
        "X Jitter Amount",
        "X 抖动量",
        0.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        76,
        "X Jitter Speed",
        "X 抖动速度",
        1.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        78,
        "Y Jitter Amount",
        "Y 抖动量",
        0.0,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        80,
        "Y Jitter Speed",
        "Y 抖动速度",
        1.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        82,
        "Z Jitter Amount",
        "Z 抖动量",
        0.0,
        [0.0, 25.0],
        [0.0, 25.0],
        2,
        "none",
    );
    b = b.slider(
        84,
        "Z Jitter Speed",
        "Z 抖动速度",
        1.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(105, "", "", ParamKind::GroupEnd, None);
    b.push(
        107,
        "Rotation Jitter",
        "旋转抖动",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        86,
        "X Rotation Jitter Amount",
        "X 旋转抖动量",
        0.0,
        [0.0, 360.0],
        [0.0, 360.0],
        2,
        "none",
    );
    b = b.slider(
        88,
        "X Rotation Jitter Speed",
        "X 旋转抖动速度",
        1.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        90,
        "Y Rotation Jitter Amount",
        "Y 旋转抖动量",
        0.0,
        [0.0, 360.0],
        [0.0, 360.0],
        2,
        "none",
    );
    b = b.slider(
        92,
        "Y Rotation Jitter Speed",
        "Y 旋转抖动速度",
        1.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        94,
        "Z Rotation Jitter Amount",
        "Z 旋转抖动量",
        0.0,
        [0.0, 360.0],
        [0.0, 360.0],
        2,
        "none",
    );
    b = b.slider(
        96,
        "Z Rotation Jitter Speed",
        "Z 旋转抖动速度",
        1.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(121, "", "", ParamKind::GroupEnd, None);
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (
            Arc::new(LayerWipe {
                def,
                mode: Mode::CardWipe,
            }),
            names,
        ),
        "card-wipe",
        "卡片擦除",
        &["Card Wipe", "卡片"],
    );
    fx.info.param_options_zh.extend([
        (8, vec!["独立".into(), "列数受行数控制".into()]),
        (16, vec!["X".into(), "Y".into(), "随机".into()]),
        (18, vec!["正向".into(), "反向".into(), "随机".into()]),
        (
            20,
            vec![
                "从左到右".into(),
                "从右到左".into(),
                "自上而下".into(),
                "自下而上".into(),
                "左上到右下".into(),
                "右上到左下".into(),
                "左下到右上".into(),
                "右下到左上".into(),
                "渐变".into(),
            ],
        ),
        (
            28,
            vec!["摄像机位置".into(), "边角定位".into(), "合成摄像机".into()],
        ),
        (
            42,
            vec![
                "旋转 XYZ，位置".into(),
                "旋转 XZY，位置".into(),
                "旋转 YXZ，位置".into(),
                "旋转 YZX，位置".into(),
                "旋转 ZXY，位置".into(),
                "旋转 ZYX，位置".into(),
                "位置，旋转 XYZ".into(),
                "位置，旋转 XZY".into(),
                "位置，旋转 YXZ".into(),
                "位置，旋转 YZX".into(),
                "位置，旋转 ZXY".into(),
                "位置，旋转 ZYX".into(),
            ],
        ),
        (
            64,
            vec!["点光源".into(), "远光源".into(), "首选合成灯光".into()],
        ),
    ]);
    fx
}

pub(crate) fn cc_image_wipe() -> Builtin {
    let mut b = DefBuilder::new("CC Image Wipe", "CC Image Wipe", "Transition");
    b = b.slider(
        1,
        "Completion",
        "Completion",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Border Softness",
        "Border Softness",
        0.099991,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(3, "Auto Softness", "Auto Softness", false);
    b.push(8, "Gradient", "Gradient", ParamKind::GroupStart, None);
    b = b.layer(5, "Layer", "Layer").layer_stage(5);
    b = b.popup(
        6,
        "Property",
        "Property",
        5,
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
        4,
        "Blur",
        "Blur",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.checkbox(7, "Inverse Gradient", "Inverse Gradient", false);
    b.push(9, "", "", ParamKind::GroupEnd, None);
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (
            Arc::new(LayerWipe {
                def,
                mode: Mode::CcImageWipe,
            }),
            names,
        ),
        "cc-image-wipe",
        "CC Image Wipe",
        &["CC Image Wipe", "图像"],
    );
    fx.info.param_options_zh.extend([(
        6,
        vec![
            "Red".into(),
            "Green".into(),
            "Blue".into(),
            "Alpha".into(),
            "Luminance".into(),
            "Lightness".into(),
            "Hue".into(),
            "Saturation".into(),
        ],
    )]);
    fx
}
