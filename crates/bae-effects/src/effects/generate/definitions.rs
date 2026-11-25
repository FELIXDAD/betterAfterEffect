use super::*;
use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use std::sync::Arc;

pub(crate) fn radio_waves() -> Builtin {
    let mut b = DefBuilder::new("APC Radio Waves", "Radio Waves", "Generate");
    b.push(
        4,
        "Producer Point",
        "产生点",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.popup(
        6,
        "Parameters Are Set At",
        "参数设置为",
        1,
        &["Birth", "Each Frame"],
    );
    b = b.slider(
        64,
        "Render Quality",
        "渲染品质",
        4.0,
        [1.0, 16.0],
        [1.0, 16.0],
        2,
        "none",
    );
    b = b.popup(
        2,
        "Wave Type",
        "波浪类型",
        1,
        &["Polygon", "Image Contour", "Mask"],
    );
    b.push(68, "Polygon", "多边形", ParamKind::GroupStart, None);
    b = b.slider(
        8,
        "Sides",
        "边",
        64.0,
        [3.0, 128.0],
        [3.0, 128.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Curve Size",
        "曲线大小",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        12,
        "Curvyness",
        "曲线弯曲度",
        0.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.checkbox(14, "Star", "星形", false);
    b = b.slider(
        16,
        "Star Depth",
        "星深度",
        -0.5,
        [-1.0, 10.0],
        [-1.0, 10.0],
        2,
        "none",
    );
    b.push(23, "", "", ParamKind::GroupEnd, None);
    b.push(
        70,
        "Image Contour",
        "图像等高线",
        ParamKind::GroupStart,
        None,
    );
    b = b.layer(18, "Source Layer", "源图层").layer_stage(18);
    b.push(
        20,
        "Source Center",
        "源中心",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.popup(
        22,
        "Value Channel",
        "值通道",
        5,
        &[
            "Intensity",
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Hue",
            "Lightness",
            "Saturation",
            "Value",
        ],
    );
    b = b.checkbox(24, "Invert Input", "反转输入", false);
    b = b.slider(
        26,
        "Value Threshold",
        "值阈值",
        127.0,
        [0.0, 255.0],
        [0.0, 255.0],
        2,
        "none",
    );
    b = b.slider(
        28,
        "Pre-Blur",
        "预模糊",
        0.0,
        [0.0, 50.0],
        [0.0, 50.0],
        2,
        "none",
    );
    b = b.slider(
        30,
        "Tolerance",
        "容差",
        0.5,
        [0.0, 5.0],
        [0.0, 5.0],
        2,
        "none",
    );
    b = b.slider(
        32,
        "Contour",
        "等高线",
        1.0,
        [1.0, 1000.0],
        [1.0, 1000.0],
        2,
        "none",
    );
    b.push(43, "", "", ParamKind::GroupEnd, None);
    b.push(72, "Mask", "蒙版", ParamKind::GroupStart, None);
    b.push(66, "Mask", "蒙版", ParamKind::Path, None);
    b.push(49, "", "", ParamKind::GroupEnd, None);
    b.push(51, "Wave Motion", "波动", ParamKind::GroupStart, None);
    b = b.slider(
        34,
        "Frequency",
        "频率",
        1.0,
        [0.0, 500.0],
        [0.0, 500.0],
        2,
        "none",
    );
    b = b.slider(
        36,
        "Expansion",
        "扩展",
        5.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b.push(
        38,
        "Orientation",
        "方向",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b.push(
        42,
        "Direction",
        "方向",
        ParamKind::Angle,
        Some(Value::Float(90.0)),
    );
    b = b.slider(
        40,
        "Velocity",
        "速率",
        0.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b = b.slider(
        44,
        "Spin",
        "旋转",
        0.0,
        [-10000.0, 10000.0],
        [-10000.0, 10000.0],
        2,
        "none",
    );
    b = b.slider(
        56,
        "Lifespan (sec)",
        "寿命（秒）",
        10.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.checkbox(62, "Reflection", "反射", false);
    b.push(69, "", "", ParamKind::GroupEnd, None);
    b.push(71, "Stroke", "描边", ParamKind::GroupStart, None);
    b = b.popup(
        48,
        "Profile",
        "配置文件",
        1,
        &[
            "Square",
            "Triangle",
            "Sawtooth Out",
            "Sawtooth In",
            "Gaussian",
            "Bell",
            "Curve",
        ],
    );
    b.push(
        46,
        "Color",
        "颜色",
        ParamKind::Color,
        Some(Value::Color([0.0, 0.0, 1.0, 1.0])),
    );
    b = b.slider(
        54,
        "Opacity",
        "不透明度",
        1.0,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "none",
    );
    b = b.slider(
        58,
        "Fade-in Time",
        "淡入时间",
        0.0,
        [0.0, 300.0],
        [0.0, 300.0],
        2,
        "none",
    );
    b = b.slider(
        60,
        "Fade-out Time",
        "淡出时间",
        5.0,
        [0.0, 300.0],
        [0.0, 300.0],
        2,
        "none",
    );
    b = b.slider(
        50,
        "Start Width",
        "开始宽度",
        5.0,
        [1.0, 100.0],
        [1.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        52,
        "End Width",
        "末端宽度",
        5.0,
        [1.0, 100.0],
        [1.0, 100.0],
        2,
        "none",
    );
    b.push(87, "", "", ParamKind::GroupEnd, None);
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (Arc::new(RadioWaves { def }), names),
        "radio-waves",
        "无线电波",
        &["Radio Waves", "波纹"],
    );
    fx.info.param_options_zh.extend([
        (6, vec!["生成".into(), "每帧".into()]),
        (2, vec!["多边形".into(), "图像等高线".into(), "蒙版".into()]),
        (
            22,
            vec![
                "强度".into(),
                "红色".into(),
                "绿色".into(),
                "蓝色".into(),
                "Alpha".into(),
                "色相".into(),
                "亮度".into(),
                "饱和度".into(),
                "值".into(),
            ],
        ),
        (
            48,
            vec![
                "正方形".into(),
                "三角形".into(),
                "出点锯齿".into(),
                "入点锯齿".into(),
                "高斯分布".into(),
                "铃铛".into(),
                "曲线".into(),
            ],
        ),
    ]);
    fx
}

pub(crate) fn paint_bucket() -> Builtin {
    let mut b = DefBuilder::new("ADBE Paint Bucket", "Paint Bucket", "Generate");
    b.push(
        2,
        "Fill Point",
        "填充点",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.popup(
        4,
        "Fill Selector",
        "填充选择器",
        1,
        &[
            "Color & Alpha",
            "Straight Color",
            "(-",
            "Transparency",
            "Opacity",
            "Alpha Channel",
        ],
    );
    b = b.slider(
        3,
        "Tolerance",
        "容差",
        50.0,
        [0.0, 255.0],
        [0.0, 255.0],
        2,
        "none",
    );
    b = b.checkbox(7, "View Threshold", "查看阈值", false);
    b = b.popup(
        8,
        "Stroke",
        "描边",
        1,
        &["Antialias", "Feather", "Spread", "Choke", "Stroke"],
    );
    b = b.slider(
        5,
        "Context Slider",
        "上下文滑块",
        3.0,
        [0.0, 40.0],
        [0.0, 40.0],
        2,
        "none",
    );
    b = b.checkbox(9, "Invert Fill", "反转填充", false);
    b.push(
        1,
        "Color",
        "颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 0.0, 0.0, 1.0])),
    );
    b = b.slider(
        10,
        "Opacity",
        "不透明度",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.popup(
        11,
        "Blending Mode",
        "混合模式",
        2,
        &[
            "Fill Only",
            "Normal",
            "Stencil Alpha",
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
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (Arc::new(PaintBucket { def }), names),
        "paint-bucket",
        "油漆桶",
        &["Paint Bucket", "油漆"],
    );
    fx.info.param_options_zh.extend([
        (
            4,
            vec![
                "颜色和 Alpha".into(),
                "直接颜色".into(),
                "(-".into(),
                "透明度".into(),
                "不透明度".into(),
                "Alpha 通道".into(),
            ],
        ),
        (
            8,
            vec![
                "消除锯齿".into(),
                "羽化".into(),
                "扩展".into(),
                "阻塞".into(),
                "描边".into(),
            ],
        ),
        (
            11,
            vec![
                "仅填充".into(),
                "正常".into(),
                "模板 Alpha".into(),
                "(-".into(),
                "相加".into(),
                "相乘".into(),
                "滤色".into(),
                "叠加".into(),
                "柔光".into(),
                "强光".into(),
                "(-".into(),
                "颜色减淡".into(),
                "颜色加深".into(),
                "(-".into(),
                "变暗".into(),
                "变亮".into(),
                "差值".into(),
                "排除".into(),
                "(-".into(),
                "色相".into(),
                "饱和度".into(),
                "颜色".into(),
                "发光度".into(),
            ],
        ),
    ]);
    fx
}

pub(crate) fn cc_light_rays() -> Builtin {
    let mut b = DefBuilder::new("CC Light Rays", "CC Light Rays", "Generate");
    b = b.slider(
        1,
        "Intensity",
        "Intensity",
        100.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b.push(
        2,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([160.0, 90.0])),
    );
    b = b.slider(
        3,
        "Radius",
        "Radius",
        40.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Warp Softness",
        "Warp Softness",
        50.0,
        [0.0, 400.0],
        [0.0, 400.0],
        2,
        "none",
    );
    b = b.popup(5, "Shape", "Shape", 1, &["Round", "Square"]);
    b.push(
        6,
        "Direction",
        "Direction",
        ParamKind::Angle,
        Some(Value::Float(0.0)),
    );
    b = b.checkbox(7, "Color from Source", "Color from Source", true);
    b = b.checkbox(10, "Allow Brightening", "Allow Brightening", true);
    b.push(
        8,
        "Color",
        "Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 1.0, 1.0, 1.0])),
    );
    b = b.popup(
        9,
        "Transfer Mode",
        "Transfer Mode",
        2,
        &["None", "Add", "Lighten", "Screen"],
    );
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (Arc::new(LightRays { def, burst: false }), names),
        "cc-light-rays",
        "CC Light Rays",
        &["CC Light Rays", "光线"],
    );
    fx.info.param_options_zh.extend([
        (5, vec!["Round".into(), "Square".into()]),
        (
            9,
            vec![
                "None".into(),
                "Add".into(),
                "Lighten".into(),
                "Screen".into(),
            ],
        ),
    ]);
    fx
}

pub(crate) fn cc_light_burst_2_5() -> Builtin {
    let mut b = DefBuilder::new("CC Light Burst 2.5", "CC Light Burst 2.5", "Generate");
    b.push(
        1,
        "Center",
        "Center",
        ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        2,
        "Intensity",
        "Intensity",
        100.0,
        [0.0, 30000.0],
        [0.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Ray Length",
        "Ray Length",
        50.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b = b.popup(4, "Burst", "Burst", 2, &["Straight", "Fade", "Center"]);
    b = b.checkbox(5, "Halo Alpha", "Halo Alpha", false);
    b = b.checkbox(6, "Set Color", "Set Color", false);
    b.push(
        7,
        "Color",
        "Color",
        ParamKind::Color,
        Some(Value::Color([1.0, 0.901961, 0.25098, 1.0])),
    );
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (Arc::new(LightRays { def, burst: true }), names),
        "cc-light-burst-2-5",
        "CC Light Burst 2.5",
        &["CC Light Burst 2.5", "放射"],
    );
    fx.info
        .param_options_zh
        .extend([(4, vec!["Straight".into(), "Fade".into(), "Center".into()])]);
    fx
}

pub(crate) fn eyedropper_fill() -> Builtin {
    let mut b = DefBuilder::new("ADBE Eyedropper Fill", "Eyedropper Fill", "Generate");
    b.push(
        1,
        "Sample Point",
        "采样点",
        ParamKind::Point,
        Some(Value::Vec2([0.5, 0.5])),
    );
    let (mut def, names) = b
        .slider(
            2,
            "Sample Radius",
            "采样半径",
            10.,
            [0.5, 200.],
            [0.5, 200.],
            2,
            "pixels",
        )
        .popup(
            3,
            "Average Pixel Color",
            "平均像素颜色",
            1,
            &["Skip Empty", "All", "All Premultiplied", "Including Alpha"],
        )
        .checkbox(4, "Maintain Original Alpha", "保持原始 Alpha", false)
        .slider(
            5,
            "Blend With Original",
            "与原始图像混合",
            0.,
            [0., 1.],
            [0., 1.],
            2,
            "none",
        )
        .build();
    def.params[0].flags.relative_default = true;
    let mut fx = rust_builtin(
        (Arc::new(eyedropper::EyedropperFill { def }), names),
        "eyedropper-fill",
        "吸管填充",
        &["Eyedropper Fill", "取色"],
    );
    fx.info.param_options_zh.push((
        3,
        ["跳过空白", "全部", "全部预乘", "包括 Alpha"]
            .map(str::to_owned)
            .into(),
    ));
    fx
}
