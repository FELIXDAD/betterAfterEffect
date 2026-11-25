use super::*;

pub(super) fn effect() -> Builtin {
    let builder = DefBuilder::new("ADBE Aud HiLo", "High-Low Pass", "Audio")
        .popup(
            1,
            "Filter Options",
            "滤镜选项",
            1,
            &["High Pass", "Low Pass"],
        )
        .slider(
            2,
            "Cutoff Frequency",
            "屏蔽频率",
            2000.0,
            [1.0, 20000.0],
            [1.0, 20000.0],
            2,
            "none",
        )
        .slider(
            3,
            "Dry Out",
            "干输出",
            0.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        )
        .slider(
            4,
            "Wet Out",
            "湿输出",
            100.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        );
    audio_control("high-low-pass", "高通/低通", builder, &[1])
}
