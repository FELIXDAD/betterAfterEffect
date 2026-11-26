//! Exercise the native muxer and reopen its AAC track with the normal source decoder.
#![cfg(any(windows, target_os = "macos"))]
use bae_media::audio::time_to_sample;
use bae_media::export::AudioEncodeSettings;
use bae_media::{
    AlphaMode, AudioBuffer, AudioSource, ColorInfo, CpuFrame, EncodeSettings, FileAudio, FrameRate,
    MediaEncoders, PixelFormat, VideoCodec,
};

#[test]
fn movie_has_decodable_audio_aligned_with_ntsc_frames() {
    #[cfg(windows)]
    if !bae_media::media_foundation::encoder_available(VideoCodec::H264) {
        eprintln!("SKIP: Media Foundation H.264 encoder unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let tracks = [("mp4", "aac")];
    #[cfg(target_os = "macos")]
    let tracks = [("mp4", "aac"), ("mov", "aac"), ("mov", "pcm_s24le")];
    for (ext, codec) in tracks {
        let path = dir.path().join(format!("audio.{ext}"));
        let rate = FrameRate::FPS_29_97;
        let mut settings = EncodeSettings::new(&path, VideoCodec::H264, 160, 96, rate);
        settings.audio = Some(AudioEncodeSettings {
            codec: codec.into(),
            ..AudioEncodeSettings::aac_stereo()
        });
        let mut writer = MediaEncoders::builtin().create(&settings).unwrap();
        let frame = CpuFrame {
            width: 160,
            height: 96,
            format: PixelFormat::Bgra8,
            data: [32, 32, 32, 255].repeat(160 * 96),
            pixel_aspect: settings.pixel_aspect,
            alpha: AlphaMode::Opaque,
            color: ColorInfo::default(),
        };
        for f in 0..30 {
            writer.push_frame(&frame).unwrap();
            let start = time_to_sample(rate.frame_to_time(f), 48000);
            let end = time_to_sample(rate.frame_to_time(f + 1), 48000);
            let samples = (start..end)
                .flat_map(|s| {
                    let tone = (std::f32::consts::TAU * 440.0 * s as f32 / 48000.0).sin();
                    [0.5 * tone, 0.1 * tone]
                })
                .collect();
            writer
                .push_audio(&AudioBuffer {
                    sample_rate: 48000,
                    channels: 2,
                    samples,
                })
                .unwrap();
        }
        assert_eq!(writer.finish().unwrap().frames, 30);
        let mut source = FileAudio::open(&path).unwrap();
        assert_eq!(source.info().codec, codec);
        let pcm = source.read(0, 60_000).unwrap();
        assert_eq!((pcm.sample_rate, pcm.channels), (48000, 2));
        assert!(
            pcm.frames().abs_diff(48048) <= 2048,
            "{} audio frames",
            pcm.frames()
        );
        let mut energy = [0f64; 2];
        for pair in pcm.samples[8000..32000].as_chunks::<2>().0 {
            for (sum, &sample) in energy.iter_mut().zip(pair) {
                *sum += f64::from(sample).powi(2);
            }
        }
        assert!(
            energy[0] > 100.0 && (16.0..36.0).contains(&(energy[0] / energy[1])),
            "{energy:?}"
        );
    }
}
