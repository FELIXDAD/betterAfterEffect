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

/// #793: ProRes with uncompressed audio longer than the writer's interleave
/// window. The movie writer takes a frame only once audio past it is in; the
/// export hands audio over ahead of the video and says when it ends.
#[cfg(target_os = "macos")]
#[test]
fn prores_with_pcm_audio_finishes_past_the_interleave_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.mov");
    let rate = FrameRate::FPS_24;
    let codec = VideoCodec::ProRes(bae_media::export::ProResProfile::Standard);
    let mut settings = EncodeSettings::new(&path, codec, 160, 96, rate);
    settings.audio = Some(AudioEncodeSettings {
        codec: "pcm_s24le".into(),
        ..AudioEncodeSettings::aac_stereo()
    });
    let frames = 72;
    let lead = 24;
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
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
        let audio = |f: i64| {
            let start = time_to_sample(rate.frame_to_time(f), 48000);
            let end = time_to_sample(rate.frame_to_time(f + 1), 48000);
            AudioBuffer {
                sample_rate: 48000,
                channels: 2,
                samples: vec![0.25; ((end - start) * 2) as usize],
            }
        };
        let mut next = 0;
        for f in 0..frames {
            while next < (f + lead).min(frames) {
                writer.push_audio(&audio(next)).unwrap();
                next += 1;
                if next == frames {
                    writer.end_audio().unwrap();
                }
            }
            writer.push_frame(&frame).unwrap();
        }
        let _ = done.send(writer.finish().unwrap().frames);
    });
    let written = result
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the movie writer stalled");
    assert_eq!(written, frames as u64);
    let pcm = FileAudio::open(&path).unwrap().read(0, 200_000).unwrap();
    assert!(pcm.frames().abs_diff(144_000) <= 2048, "{}", pcm.frames());
}
