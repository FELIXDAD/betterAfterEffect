use super::*;

pub(super) fn effect() -> Builtin {
    let builder = DefBuilder::new("ADBE Aud Stereo Mixer", "Stereo Mixer", "Audio")
        .slider(
            1,
            "Left Level",
            "左声道级别",
            100.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        )
        .slider(
            2,
            "Right Level",
            "右声道级别",
            100.0,
            [0.0, 400.0],
            [0.0, 400.0],
            2,
            "percent",
        )
        .slider(
            3,
            "Left Pan",
            "向左平移",
            -100.0,
            [-100.0, 100.0],
            [-100.0, 100.0],
            2,
            "percent",
        )
        .slider(
            4,
            "Right Pan",
            "向右平移",
            100.0,
            [-100.0, 100.0],
            [-100.0, 100.0],
            2,
            "percent",
        )
        .checkbox(5, "Invert Phase", "反转相位", false);
    audio_control("stereo-mixer", "立体声混合器", builder, &[5])
}
