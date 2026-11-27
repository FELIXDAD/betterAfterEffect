use super::*;
use crate::{EncodeSettings, MediaEncoders, MediaFormats, MediaGpu, OpenOptions, VideoCodec};
use std::time::Instant;

fn hardware() -> bool {
    match Platform::new() {
        Ok(_) => {}
        Err(MediaError::Unsupported(reason)) => {
            eprintln!("SKIP: {reason}");
            return false;
        }
        Err(e) => panic!("{e}"),
    }
    use windows::Win32::Graphics::Direct3D11::ID3D11VideoDevice;
    let video = platform::video_device().and_then(|device| device.cast::<ID3D11VideoDevice>());
    match video {
        // SAFETY: query the profiles on this live D3D11 device.
        Ok(device) if unsafe { device.GetVideoDecoderProfileCount() } > 0 => true,
        Ok(_) => {
            eprintln!("SKIP: D3D11 adapter has no DXVA decode profiles");
            false
        }
        Err(e) => {
            eprintln!("SKIP: no hardware D3D11 video device ({e})");
            false
        }
    }
}

#[test]
fn software_reader_preserves_colors_when_seeking_h264_and_hevc() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    match Platform::new() {
        Ok(_) => {}
        Err(MediaError::Unsupported(_)) => return,
        Err(e) => panic!("{e}"),
    }
    for codec in ["h264", "hevc", "hevc10"] {
        let path = dir.join(format!("{codec}-192x128.mp4"));
        let mut source = match Source::open_with_hardware(&path, false) {
            Err(MediaError::HevcDecoderUnavailable) if codec != "h264" => continue,
            result => result.unwrap(),
        };
        for index in [0, 21, 4] {
            let frame = source.decode_cpu(&FrameRequest::new(index)).unwrap();
            assert_picture(&frame, index);
            let [r, g, b, _] = frame.pixel_f32(110, 60);
            assert!(
                r > 0.85 && g < 0.15 && b < 0.15,
                "{codec} frame {index}: {r} {g} {b}"
            );
        }
    }
}

#[test]
fn ntsc_timestamps_round_trip() {
    for n in [0, 1, 2, 17, 30_000, 500_000] {
        assert_eq!(
            frame_index(timestamp(n, FrameRate::FPS_29_97), FrameRate::FPS_29_97),
            n
        );
    }
    assert_eq!(timestamp(30_000, FrameRate::FPS_29_97), 10_010_000_000);
}

fn assert_picture(frame: &CpuFrame, index: i64) {
    let top = (24 + index * 41 % 208) as f32;
    for (y, expected) in [(0, top), (frame.height - 1, 255.0 - top)] {
        let px = frame.pixel_f32(0, y);
        for channel in &px[..3] {
            assert!(
                (channel * 255.0 - expected).abs() < 6.0,
                "frame {index}, y={y}: {px:?} expected {expected}"
            );
        }
    }
}

#[test]
fn seek_across_gops_and_resume_playback() {
    if !hardware() {
        return;
    }
    for codec in ["h264", "hevc", "hevc10"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/{codec}-192x128.mp4"));
        let mut source = match MediaFormats::builtin().open(&path, &OpenOptions::default()) {
            Err(reason @ MediaError::HevcDecoderUnavailable) => {
                eprintln!("SKIP HEVC: {reason}");
                continue;
            }
            result => result.unwrap(),
        };
        let video = source.info().video.as_ref().unwrap();
        assert_eq!(video.frame_count, Some(48));
        if codec == "h264" {
            let audio = &source.info().audio[0];
            assert_eq!(
                (audio.codec.as_str(), audio.channels, audio.sample_rate),
                ("aac", 1, 48_000)
            );
            assert_eq!(audio.frame_count, Some(144_000));
        }
        assert_eq!(video.frame_rate, Some(FrameRate::FPS_29_97));
        for index in [0, 1, 2, 30, 9, 10, 47, 12, 12, 0] {
            let frame = source.decode_cpu(&FrameRequest::new(index)).unwrap();
            assert_picture(&frame, index);
            let [r, g, b, _] = frame.pixel_f32(110, 60);
            assert!(r > 0.85 && g < 0.15 && b < 0.15, "{codec}: {r} {g} {b}");
        }
        if let Some(ctx) = bae_gpu::testing::shared_context() {
            let gpu = MediaGpu::new(ctx);
            let texture = source.decode_gpu(&FrameRequest::new(21), &gpu).unwrap();
            assert_picture(&gpu.read_back(&texture).unwrap(), 21);
        }
    }
}

#[test]
fn encode_then_reopen_mp4() {
    if !hardware() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for codec in [VideoCodec::H264, VideoCodec::Hevc] {
        if !encoder_available(codec) {
            eprintln!("SKIP: no {codec:?} encoder");
            continue;
        }
        let path = dir.path().join(format!("{codec:?}.mp4"));
        let settings = EncodeSettings::new(&path, codec, 320, 240, FrameRate::FPS_29_97);
        let mut writer = MediaEncoders::builtin().create(&settings).unwrap();
        for index in 0..24 {
            let top = (24 + index * 41 % 208) as u8;
            let half = 320 * 120;
            let mut data = [top, top, top, 255].repeat(half);
            data.extend([255 - top, 255 - top, 255 - top, 255].repeat(half));
            writer
                .push_frame(&CpuFrame {
                    width: 320,
                    height: 240,
                    format: PixelFormat::Bgra8,
                    data,
                    alpha: AlphaMode::Opaque,
                    color: ColorInfo::srgb(),
                    pixel_aspect: PixelAspect::SQUARE,
                })
                .unwrap();
        }
        assert_eq!(writer.finish().unwrap().frames, 24);
        let mut reader = match Source::open(&path) {
            Err(reason @ MediaError::HevcDecoderUnavailable) => {
                eprintln!("SKIP HEVC readback: {reason}");
                continue;
            }
            result => result.unwrap(),
        };
        assert_eq!(reader.info.video.as_ref().unwrap().frame_count, Some(24));
        let video = reader.info.video.as_ref().unwrap();
        assert_eq!((video.width, video.height), (320, 240));
        assert_eq!(video.color.primaries, settings.color.primaries);
        assert_eq!(video.color.transfer, settings.color.transfer);
        assert_eq!(video.color.matrix, settings.color.matrix);
        for index in [0, 22, 2, 3] {
            assert_picture(
                &reader.decode_cpu(&FrameRequest::new(index)).unwrap(),
                index,
            );
        }
    }
}

#[test]
fn unavailable_hevc_has_a_distinct_import_error() {
    let missing = windows::core::Error::from_hresult(MF_E_TOPO_CODEC_NOT_FOUND);
    assert!(matches!(
        decode_error(true, missing.clone()),
        MediaError::HevcDecoderUnavailable
    ));
    assert!(matches!(
        decode_error(false, missing),
        MediaError::Platform { .. }
    ));
}

#[test]
#[ignore = "set BAE_MF_BENCH to a 48-frame MP4 made by tests/fixtures/generate_mf.py"]
fn benchmark_upload() {
    let path = std::env::var_os("BAE_MF_BENCH").unwrap();
    let mut source = Source::open(Path::new(&path)).unwrap();
    let ctx = bae_gpu::testing::shared_context().unwrap();
    let gpu = MediaGpu::new(ctx.clone());
    eprintln!(
        "{} / {:?}",
        ctx.adapter_info().name,
        ctx.adapter_info().backend
    );
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    let request = FrameRequest::new(0);
    gpu.upload(&frame, &request).unwrap();
    ctx.wait_idle().unwrap();
    let start = Instant::now();
    for _ in 1..48 {
        gpu.upload(&frame, &request).unwrap();
        ctx.wait_idle().unwrap();
    }
    eprintln!(
        "upload: {:.2} ms/frame",
        start.elapsed().as_secs_f64() * 1000.0 / 47.0
    );
    for mode in ["sequential", "seek"] {
        source.decode_gpu(&FrameRequest::new(0), &gpu).unwrap();
        ctx.wait_idle().unwrap();
        let start = Instant::now();
        for n in 1..48 {
            let index = if mode == "seek" { n * 17 % 48 } else { n };
            source.decode_gpu(&FrameRequest::new(index), &gpu).unwrap();
            ctx.wait_idle().unwrap();
        }
        eprintln!(
            "{mode}: {:.2} ms/frame",
            start.elapsed().as_secs_f64() * 1000.0 / 47.0
        );
    }
}
