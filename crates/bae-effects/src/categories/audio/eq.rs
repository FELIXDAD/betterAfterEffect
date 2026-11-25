use super::*;

pub(super) fn effect() -> Builtin {
    let builder = {
        let mut builder = DefBuilder::new("ADBE Param EQ", "Parametric EQ", "Audio");
        builder.push(
            1,
            "Frequency Response",
            "网频响应",
            bae_model::ParamKind::Display,
            None,
        );
        builder
    }
    .checkbox(2, "Band 1 Enabled", "带 1 已启用", true)
    .slider(
        3,
        "Frequency",
        "频率",
        1000.0,
        [1.0, 21609.0],
        [1.0, 21609.0],
        2,
        "none",
    )
    .slider(
        4,
        "Bandwidth",
        "带宽",
        20.0,
        [0.1, 99.0],
        [0.1, 99.0],
        2,
        "none",
    )
    .slider(
        5,
        "Boost/Cut",
        "推进/剪切",
        50.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "percent",
    )
    .checkbox(6, "Band 2 Enabled", "带 2 已启用", false)
    .slider(
        7,
        "Frequency",
        "频率",
        5000.0,
        [1.0, 21609.0],
        [1.0, 21609.0],
        2,
        "none",
    )
    .slider(
        8,
        "Bandwidth",
        "带宽",
        20.0,
        [0.1, 99.0],
        [0.1, 99.0],
        2,
        "none",
    )
    .slider(
        9,
        "Boost/Cut",
        "推进/剪切",
        50.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "percent",
    )
    .checkbox(10, "Band 3 Enabled", "带 3 已启用", false)
    .slider(
        11,
        "Frequency",
        "频率",
        10000.0,
        [1.0, 21609.0],
        [1.0, 21609.0],
        2,
        "none",
    )
    .slider(
        12,
        "Bandwidth",
        "带宽",
        20.0,
        [0.1, 99.0],
        [0.1, 99.0],
        2,
        "none",
    )
    .slider(
        13,
        "Boost/Cut",
        "推进/剪切",
        50.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "percent",
    );
    audio_control("parametric-eq", "参数均衡", builder, &[1, 2, 6, 10])
}

pub(super) fn alias() -> Builtin {
    let builder = DefBuilder::new("ADBE Aud BT", "Bass & Treble", "Audio")
        .slider(
            1,
            "Bass",
            "低音",
            0.0,
            [-100.0, 100.0],
            [-100.0, 100.0],
            2,
            "none",
        )
        .slider(
            2,
            "Treble",
            "高音",
            0.0,
            [-100.0, 100.0],
            [-100.0, 100.0],
            2,
            "none",
        );
    audio_control("bass-treble", "低音和高音", builder, &[])
}
