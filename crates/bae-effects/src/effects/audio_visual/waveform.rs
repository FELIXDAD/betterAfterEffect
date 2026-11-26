use super::*;
use bae_model::ParamKind;
pub(super) fn effect() -> Builtin {
    let mut b = DefBuilder::new("ADBE AudWave", "Audio Waveform", "Generate");
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
    b = b.int(5, "Displayed Samples", "显示的范例", 120, [1, 44100]);
    b = b.slider(
        6,
        "Maximum Height",
        "最大高度",
        120.0,
        [1.0, 4000.0],
        [1.0, 4000.0],
        2,
        "percent",
    );
    b = b.slider(
        7,
        "Audio Duration (milliseconds)",
        "音频持续时间（毫秒）",
        125.0,
        [0.0, 30000.0],
        [0.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        8,
        "Audio Offset (milliseconds)",
        "音频偏移（毫秒）",
        0.0,
        [-30000.0, 30000.0],
        [-30000.0, 30000.0],
        2,
        "none",
    );
    b = b.slider(
        9,
        "Thickness",
        "厚度",
        3.0,
        [0.0, 4000.0],
        [0.0, 4000.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Softness",
        "柔和度",
        0.5,
        [0.0, 1.0],
        [0.0, 1.0],
        2,
        "percent",
    );
    b = b.int(
        11,
        "Random Seed (Analog)",
        "随机植入（模拟）",
        0,
        [0, 32000],
    );
    b.push(
        12,
        "Inside Color",
        "内部颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 0.0, 1.0, 0.0])),
    );
    b.push(
        13,
        "Outside Color",
        "外部颜色",
        ParamKind::Color,
        Some(Value::Color([1.0, 0.196078, 0.588235, 0.196078])),
    );
    b = b.popup(
        14,
        "Waveform Options",
        "波形选项",
        1,
        &["Mono", "Left", "Right"],
    );
    b = b.popup(
        15,
        "Display Options",
        "显示选项",
        2,
        &["Digital", "Analog Lines", "Analog Dots"],
    );
    b = b.checkbox(16, "Composite On Original", "在原始图像上合成", false);
    builtin("audio-waveform", "音频波形", b, &[1, 4])
}
