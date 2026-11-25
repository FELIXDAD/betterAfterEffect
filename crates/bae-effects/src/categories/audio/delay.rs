use super::*;

pub(super) fn effect() -> Builtin {
    let builder = DefBuilder::new("ADBE Aud Delay", "Delay", "Audio")
        .slider(
            1,
            "Delay Time (ms)",
            "延迟时间（毫秒）",
            500.0,
            [0.0, 5000.0],
            [0.0, 5000.0],
            2,
            "none",
        )
        .slider(
            2,
            "Delay Amount",
            "延迟量",
            50.0,
            [0.0, 1000.0],
            [0.0, 1000.0],
            2,
            "percent",
        )
        .slider(
            3,
            "Feedback",
            "反馈",
            50.0,
            [0.0, 1000.0],
            [0.0, 1000.0],
            2,
            "percent",
        )
        .slider(
            4,
            "Dry Out",
            "干输出",
            75.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        )
        .slider(
            5,
            "Wet Out",
            "湿输出",
            75.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        );
    audio_control("audio-delay", "延迟", builder, &[1])
}
