use super::*;
use bae_model::ParamKind;
pub(super) fn effect() -> Builtin {
    let mut b = DefBuilder::new("ADBE AudSpect", "Audio Spectrum", "Generate");
    b = b.layer(1, "Audio Layer", "音频层");
    b.push(
        2,
        "Start Point",
        "起始点",
        ParamKind::Point,
        Some(Value::Vec2([64.0, 180.0])),
    );
    b.push(
        3,
        "End Point",
        "结束点",
        ParamKind::Point,
        Some(Value::Vec2([576.0, 180.0])),
    );
    b.push(4, "Path", "路径", ParamKind::Path, None);
    b = b.checkbox(5, "Use Polar Path", "使用极坐标路径", false);
    b = b.slider(
        6,
        "Start Frequency",
        "起始频率",
        20.0,
        [1.0, 22050.0],
        [1.0, 22050.0],
        2,
        "percent",
    );
    b = b.slider(
        7,
        "End Frequency",
        "结束频率",
        2000.0,
        [1.0, 22050.0],
        [1.0, 22050.0],
        2,
        "percent",
    );
    b = b.int(8, "Frequency Bands", "频段", 64, [1, 4096]);
    b = b.slider(
        9,
        "Maximum Height",
        "最大高度",
        480.0,
        [1.0, 32000.0],
        [1.0, 32000.0],
        2,
        "percent",
    );
    b = b.slider(
        10,
        "Audio Duration (milliseconds)",
        "音频持续时间（毫秒）",
        90.0,
        [0.0, 30000.0],
        [0.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        11,
        "Audio Offset (milliseconds)",
        "音频偏移（毫秒）",
        0.0,
        [-30000.0, 30000.0],
        [-30000.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        12,
        "Thickness",
        "厚度",
        3.0,
        [0.0, 4000.0],
        [0.0, 4000.0],
        2,
        "none",
    );
    b = b.slider(
        13,
        "Softness",
        "柔和度",
        0.5,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b.push(
        14,
        "Inside Color",
        "内部颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 0.0, 1.0, 0.0])),
    );
    b.push(
        15,
        "Outside Color",
        "外部颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 0.196078, 0.588235, 0.196078])),
    );
    b = b.checkbox(16, "Blend Overlapping Colors", "混合叠加颜色", false);
    b.push(
        17,
        "Hue Interpolation",
        "色相插值",
        ParamKind::Angle,
        Some(Value::Float(0.000458)),
    );
    b = b.checkbox(18, "Dynamic Hue Phase", "动态色相", true);
    b = b.checkbox(19, "Color Symmetry", "颜色对称", true);
    b = b.popup(
        20,
        "Display Options",
        "显示选项",
        1,
        &["Digital", "Analog Lines", "Analog Dots"],
    );
    b = b.popup(
        21,
        "Side Options",
        "面选项",
        3,
        &["Side A", "Side B", "Side A & B"],
    );
    b = b.checkbox(22, "Duration Averaging", "持续时间平均化", false);
    b = b.checkbox(23, "Composite On Original", "在原始图像上合成", false);
    builtin("audio-spectrum", "音频频谱", b, &[1, 4])
}
