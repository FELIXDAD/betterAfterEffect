use super::*;
use bae_model::ParamKind;

pub(super) fn compressor() -> Builtin {
    let mut b = DefBuilder::new("ADBE Aud Compressor", "Compressor", "Audio");
    b = b.slider(
        1,
        "Threshold (dB)",
        "阈值 (dB)",
        -16.,
        [-60., 0.],
        [-60., 0.],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Ratio (x:1)",
        "比率 (x:1)",
        3.,
        [0.4, 30.],
        [0.4, 30.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Knee (dB)",
        "转折 (db)",
        15.,
        [0., 30.],
        [0., 30.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Attack (ms)",
        "触发 (ms)",
        6.,
        [0., 400.],
        [0., 400.],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Release (ms)",
        "释放 (ms)",
        440.,
        [1., 4000.],
        [1., 4000.],
        2,
        "none",
    );
    b = b.checkbox(6, "Auto Release", "自动释放", true);
    b = b.slider(
        7,
        "Makeup Gain (dB)",
        "补偿增益 (dB)",
        0.,
        [-30., 30.],
        [-30., 30.],
        2,
        "none",
    );
    b = b.slider(
        8,
        "Output Ceiling (dB)",
        "输出限值 (dB)",
        -1.,
        [-30., 0.],
        [-30., 0.],
        2,
        "none",
    );
    audio_control("audio-compressor", "压缩程序", b, &[])
}

pub(super) fn distortion() -> Builtin {
    let mut b = DefBuilder::new("ADBE Aud Distortion", "Distortion", "Audio");
    b.push(9, "Amplify", "放大", ParamKind::GroupStart, None);
    b = b.popup(
        8,
        "Type",
        "类型",
        1,
        &[
            "Soft Clip",
            "Hard Clip",
            "Saturation 1",
            "Saturation 2",
            "Tube",
            "Fuzz",
        ],
    );
    b = b.slider(
        2,
        "Drive",
        "驱动",
        25.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    b = b.slider(3, "Gain", "增益", 25., [0., 300.], [0., 300.], 2, "none");
    b = b.slider(4, "Mix", "混合", 100., [0., 100.], [0., 100.], 2, "percent");
    b = b.slider(5, "Volume", "音量", 11., [0., 100.], [0., 100.], 2, "none");
    b.push(10, "", "", ParamKind::GroupEnd, None);
    b.push(11, "Bitcrusher", "比特压缩器", ParamKind::GroupStart, None);
    b = b.int(6, "Resolution (bits)", "分辨率（位）", 24, [1, 24]);
    b = b.int(7, "Downsample (x)", "降低采样率 (x)", 1, [1, 48]);
    b.push(12, "", "", ParamKind::GroupEnd, None);
    let mut fx = audio_control("audio-distortion", "扭曲", b, &[9, 10, 11, 12]);
    fx.info.param_options_zh.push((
        8,
        ["软剪辑", "硬剪辑", "饱和度 1", "饱和度 2", "电子管", "模糊"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    ));
    fx
}

pub(super) fn gate() -> Builtin {
    let mut b = DefBuilder::new("ADBE Aud Gate", "Gate", "Audio");
    b = b.slider(
        1,
        "Threshold (dB)",
        "阈值 (dB)",
        -60.,
        [-60., 0.],
        [-60., 0.],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Attack (ms)",
        "触发 (ms)",
        10.,
        [0., 1000.],
        [0., 1000.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Hold (ms)",
        "保持 (ms)",
        40.,
        [0., 2500.],
        [0., 2500.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Release (ms)",
        "释放 (ms)",
        220.,
        [5., 4000.],
        [5., 4000.],
        2,
        "none",
    );
    audio_control("audio-gate", "门", b, &[])
}

pub(super) fn modulator() -> Builtin {
    let mut b = DefBuilder::new("ADBE Aud Modulator", "Modulator", "Audio");
    b = b.popup(1, "Modulation Type", "调制类型", 1, &["Sine", "Triangle"]);
    b = b.slider(
        2,
        "Modulation Rate",
        "调制速率",
        10.,
        [0., 1000.],
        [0., 1000.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Modulation Depth",
        "调制深度",
        2.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    b = b.slider(
        4,
        "Amplitude Modulation",
        "振幅变调",
        50.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    let mut fx = audio_control("audio-modulator", "调制器", b, &[1]);
    fx.info.param_options_zh.push((
        1,
        ["正弦", "三角形"].into_iter().map(str::to_owned).collect(),
    ));
    fx
}

pub(super) fn tone() -> Builtin {
    let mut b = DefBuilder::new("ADBE Aud Tone", "Tone", "Audio");
    b = b.popup(
        1,
        "Waveform Options",
        "波形选项",
        1,
        &["Sine", "Triangle", "Sawtooth", "Square", "White Noise"],
    );
    b = b.slider(
        2,
        "Frequency 1",
        "频率 1",
        440.,
        [0., 30000.],
        [0., 30000.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Frequency 2",
        "频率 2",
        493.679993,
        [0., 30000.],
        [0., 30000.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Frequency 3",
        "频率 3",
        554.400024,
        [0., 30000.],
        [0., 30000.],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Frequency 4",
        "频率 4",
        587.400024,
        [0., 30000.],
        [0., 30000.],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Frequency 5",
        "频率 5",
        659.119995,
        [0., 30000.],
        [0., 30000.],
        2,
        "none",
    );
    b = b.slider(
        7,
        "Level",
        "级别",
        20.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    let mut fx = audio_control("audio-tone", "音调", b, &[1]);
    fx.info.param_options_zh.push((
        1,
        ["正弦波", "三角波", "锯齿波", "方波", "白噪声"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    ));
    fx
}

pub(super) fn flange_chorus() -> Builtin {
    let mut b = DefBuilder::new("ADBE Aud_Flange", "Flange & Chorus", "Audio");
    b = b.slider(
        1,
        "Voice Separation Time (ms)",
        "语音分离时间 (ms)",
        3.,
        [0., 500.],
        [0., 500.],
        2,
        "none",
    );
    b = b.int(2, "Voices", "语音", 1, [0, 32]);
    b = b.slider(
        3,
        "Modulation Rate",
        "调制速率",
        0.3,
        [0., 1000.],
        [0., 1000.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Modulation Depth",
        "调制深度",
        50.,
        [0., 100.],
        [0., 100.],
        2,
        "percent",
    );
    b = b.slider(
        5,
        "Voice Phase Change",
        "语音相变",
        0.,
        [0., 360.],
        [0., 360.],
        2,
        "none",
    );
    b = b.checkbox(6, "Invert Phase", "反转相位", false);
    b = b.checkbox(7, "Stereo", "立体声", false);
    b = b.slider(
        8,
        "Dry Out",
        "干输出",
        50.,
        [0., 400.],
        [0., 400.],
        2,
        "percent",
    );
    b = b.slider(
        9,
        "Wet Out",
        "湿输出",
        50.,
        [0., 400.],
        [0., 400.],
        2,
        "percent",
    );
    audio_control("audio-flange-chorus", "变调与合声", b, &[6, 7])
}
