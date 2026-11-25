use super::*;
use crate::catalog::DefBuilder;

pub(crate) fn timecode() -> Builtin {
    let mut b = DefBuilder::new("ADBE Timecode", "Timecode", "Text")
        .popup(
            2,
            "Display Format",
            "显示格式",
            1,
            &[
                "SMPTE HH:MM:SS:FFF",
                "Frame Numbers",
                "Feet + Frames (35mm)",
                "Feet + Frames (16mm)",
            ],
        )
        .popup(
            9,
            "Time Source",
            "时间源",
            1,
            &["Layer Source", "Composition", "Custom"],
        );
    b.push(13, "Custom", "自定义", ParamKind::GroupStart, None);
    b = b
        .int(3, "Time Units", "时间单位", 30, [1, 1000])
        .checkbox(8, "Drop Frame", "丢帧", false)
        .int(4, "Starting Frame", "开始帧", 0, [-100000, 10000000]);
    b.push(14, "", "", ParamKind::GroupEnd, None);
    b.push(
        5,
        "Text Position",
        "文本位置",
        ParamKind::Point,
        Some(Value::Vec2([64., 36.])),
    );
    b = b.slider(
        6,
        "Text Size",
        "文字大小",
        36.,
        [8., 128.],
        [8., 128.],
        1,
        "pixels",
    );
    b.push(
        7,
        "Text Color",
        "文本颜色",
        ParamKind::Color,
        Some(Value::Color([1.; 4])),
    );
    b = b.checkbox(10, "Show Box", "显示方框", true);
    b.push(
        11,
        "Box Color",
        "方框颜色",
        ParamKind::Color,
        Some(Value::Color([0., 0., 0., 1.])),
    );
    let (def, names) = b
        .slider(
            12,
            "Opacity",
            "不透明度",
            100.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        )
        .checkbox(15, "Composite On Original", "在原始图像上合成", true)
        .build();
    let mut fx = rust_builtin(
        (
            Arc::new(Counter {
                def,
                timecode: true,
            }),
            names,
        ),
        "timecode",
        "时间码",
        &["timecode", "时间码"],
    );
    fx.info.param_options_zh = vec![
        (
            2,
            [
                "SMPTE HH:MM:SS:FFF",
                "帧编号",
                "英尺 + 帧（35 毫米）",
                "英尺 + 帧（16 毫米）",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (9, ["图层源", "合成", "自定义"].map(str::to_owned).to_vec()),
    ];
    fx
}

pub(crate) fn numbers() -> Builtin {
    // AE's modal font dialog prevents its parameter table from being exported.
    let mut b = DefBuilder::new("ADBE Numbers2", "Numbers", "Text")
        .popup(
            1,
            "Type",
            "类型",
            1,
            &[
                "Number",
                "Number (Leading Zeros)",
                "Timecode (30)",
                "Timecode (25)",
                "Timecode (24)",
                "Time",
                "Numerical Date",
                "Short Date",
                "Long Date",
                "Hexadecimal",
            ],
        )
        .checkbox(2, "Random Values", "随机值", false)
        .slider(
            3,
            "Value / Offset / Random Max",
            "数值 / 偏移 / 随机最大值",
            0.,
            [-30000., 30000.],
            [-1000., 1000.],
            3,
            "none",
        )
        .int(4, "Decimal Places", "小数位数", 0, [0, 9])
        .checkbox(5, "Current Time / Date", "当前时间 / 日期", false);
    b.push(
        6,
        "Position",
        "位置",
        ParamKind::Point,
        Some(Value::Vec2([320., 180.])),
    );
    b = b.popup(
        7,
        "Display Options",
        "显示选项",
        1,
        &[
            "Fill Only",
            "Stroke Only",
            "Fill Over Stroke",
            "Stroke Over Fill",
        ],
    );
    b.push(
        8,
        "Fill Color",
        "填充颜色",
        ParamKind::Color,
        Some(Value::Color([1.; 4])),
    );
    b.push(
        9,
        "Stroke Color",
        "描边颜色",
        ParamKind::Color,
        Some(Value::Color([0., 0., 0., 1.])),
    );
    let (def, names) = b
        .slider(
            10,
            "Stroke Width",
            "描边宽度",
            1.,
            [0., 100.],
            [0., 20.],
            1,
            "pixels",
        )
        .slider(
            11,
            "Size",
            "大小",
            36.,
            [1., 1000.],
            [1., 300.],
            1,
            "pixels",
        )
        .slider(
            12,
            "Tracking",
            "字距",
            0.,
            [-100., 1000.],
            [-50., 100.],
            1,
            "pixels",
        )
        .checkbox(13, "Proportional Spacing", "比例间距", false)
        .checkbox(14, "Composite On Original", "在原始图像上合成", true)
        .build();
    let mut fx = rust_builtin(
        (
            Arc::new(Counter {
                def,
                timecode: false,
            }),
            names,
        ),
        "numbers",
        "编号",
        &["numbers", "编号", "计数器"],
    );
    fx.info.param_options_zh = vec![
        (
            1,
            [
                "数字",
                "数字（前导零）",
                "时间码（30）",
                "时间码（25）",
                "时间码（24）",
                "时间",
                "数字日期",
                "短日期",
                "长日期",
                "十六进制",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (
            7,
            ["仅填充", "仅描边", "填充覆盖描边", "描边覆盖填充"]
                .map(str::to_owned)
                .to_vec(),
        ),
    ];
    fx
}
