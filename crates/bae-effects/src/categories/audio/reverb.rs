use super::*;

pub(super) fn effect() -> Builtin {
    let builder = DefBuilder::new("ADBE Aud Reverb", "Reverb", "Audio")
        .slider(
            1,
            "Reverb Time (ms)",
            "混响时间（毫秒）",
            100.0,
            [0.0, 5000.0],
            [0.0, 5000.0],
            2,
            "none",
        )
        .slider(
            2,
            "Diffusion",
            "扩散",
            75.0,
            [0.0, 100.0],
            [0.0, 100.0],
            2,
            "percent",
        )
        .slider(
            3,
            "Decay",
            "衰减",
            25.0,
            [0.0, 100.0],
            [0.0, 100.0],
            2,
            "percent",
        )
        .slider(
            4,
            "Brightness",
            "亮度",
            10.0,
            [0.0, 100.0],
            [0.0, 100.0],
            2,
            "percent",
        )
        .slider(
            5,
            "Dry Out",
            "干输出",
            90.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        )
        .slider(
            6,
            "Wet Out",
            "湿输出",
            10.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        );
    audio_control("reverb", "混响", builder, &[1])
}
