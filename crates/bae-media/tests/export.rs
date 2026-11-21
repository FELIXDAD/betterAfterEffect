//! Export: image-sequence encoders round-tripped through the decoders, the
//! render loop (CPU and GPU frames, async readback), cancellation and the queue.

mod common;

use bae_media::export::{RenderJob, render};
use bae_media::{
    AlphaMode, CancelToken, ColorInfo, ColorPrimaries, CpuFrame, EncodeSettings, ExrCompression,
    FrameRate, FrameRequest, MediaError, MediaFormats, MediaKind, OpenOptions, PixelAspect,
    PixelFormat, ProducedFrame, RenderQueue, RenderStatus, Timecode, TransferFunction, VideoCodec,
};
use common::assert_close;

/// A premultiplied half-float test frame: horizontal ramp, alpha 0.5, and the
/// frame number in the blue channel (n / 10).
fn test_frame(n: i64, width: u32, height: u32) -> CpuFrame {
    let mut data = Vec::new();
    for _y in 0..height {
        for x in 0..width {
            let a = 0.5_f32;
            let straight = [x as f32 / (width - 1) as f32, 0.25, n as f32 / 10.0];
            for v in [straight[0] * a, straight[1] * a, straight[2] * a, a] {
                data.extend_from_slice(&half::f16::from_f32(v).to_ne_bytes());
            }
        }
    }
    CpuFrame {
        width,
        height,
        format: PixelFormat::Rgba16Float,
        data,
        alpha: AlphaMode::Premultiplied,
        color: ColorInfo::srgb(),
        pixel_aspect: PixelAspect::SQUARE,
    }
}

fn cpu_producer(width: u32, height: u32) -> impl FnMut(i64) -> Result<ProducedFrame, MediaError> {
    move |n| Ok(ProducedFrame::Cpu(test_frame(n, width, height)))
}

fn job(name: &str, frames: std::ops::Range<i64>, settings: EncodeSettings) -> RenderJob {
    RenderJob {
        name: name.into(),
        frames,
        settings,
    }
}

#[test]
fn png_sequence_round_trip_straight_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("out/comp_[####].png"),
        VideoCodec::Png,
        8,
        4,
        FrameRate::FPS_24,
    )
    .with_alpha(AlphaMode::Straight);
    settings.start_number = 10;
    let output = render(
        &job("comp", 0..5, settings),
        &mut cpu_producer(8, 4),
        None,
        &bae_media::MediaEncoders::builtin(),
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(output.frames, 5);
    assert_eq!(output.files[0], dir.path().join("out/comp_0010.png"));
    assert_eq!(output.files[4], dir.path().join("out/comp_0014.png"));

    let mut source = MediaFormats::builtin()
        .open(&output.files[0], &OpenOptions::sequence(FrameRate::FPS_24))
        .unwrap();
    let info = source.info().clone();
    assert_eq!(info.kind, MediaKind::ImageSequence);
    let video = info.video.unwrap();
    assert_eq!(
        (video.width, video.height, video.frame_count),
        (8, 4, Some(5))
    );
    assert_eq!(video.alpha, AlphaMode::Straight);
    assert_eq!(video.color.transfer, TransferFunction::Srgb);
    let frame = source.decode_cpu(&FrameRequest::new(3)).unwrap();
    // Straight output: color divided back out of the premultiplied input.
    assert_close(frame.pixel_f32(7, 2), [1.0, 0.25, 0.3, 0.5], 2.5 / 255.0);
}

#[test]
fn png_sixteen_bit_rgb_is_premultiplied_over_black() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("deep.png"),
        VideoCodec::Png,
        4,
        2,
        FrameRate::FPS_25,
    );
    settings.bit_depth = 16;
    let output = render(
        &job("deep", 0..1, settings),
        &mut cpu_producer(4, 2),
        None,
        &bae_media::MediaEncoders::builtin(),
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(output.files, [dir.path().join("deep_00000.png")]);
    let mut source = MediaFormats::builtin()
        .open(&output.files[0], &OpenOptions::default())
        .unwrap();
    let video = source.info().video.clone().unwrap();
    assert_eq!((video.bit_depth, video.alpha), (16, AlphaMode::Opaque));
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_eq!(frame.format, PixelFormat::Rgba16);
    assert_close(frame.pixel_f32(3, 1), [0.5, 0.125, 0.0, 1.0], 1e-3);
}

#[test]
fn exr_sequence_keeps_hdr_tags_and_timecode() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("beauty.####.exr"),
        VideoCodec::Exr(ExrCompression::Piz),
        6,
        3,
        FrameRate::FPS_24,
    )
    .with_alpha(AlphaMode::Premultiplied);
    settings.start_number = 1001;
    settings.color.primaries = ColorPrimaries::AcesAp1;
    settings.pixel_aspect = PixelAspect::new(2, 1);
    settings.start_timecode = Some(Timecode {
        hours: 1,
        ..Timecode::default()
    });
    // HDR input (> 1) must survive the half-float path.
    let mut producer = |n: i64| {
        let mut frame = test_frame(n, 6, 3);
        frame = bae_media::export::convert_frame(
            &frame,
            PixelFormat::Rgba32Float,
            AlphaMode::Premultiplied,
        );
        let floats: &mut [f32] = bytemuck::cast_slice_mut(&mut frame.data);
        floats[0] = 4.0;
        Ok(ProducedFrame::Cpu(frame))
    };
    let output = render(
        &job("beauty", 0..3, settings),
        &mut producer,
        None,
        &bae_media::MediaEncoders::builtin(),
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(output.files[2], dir.path().join("beauty.1003.exr"));

    let mut last = MediaFormats::builtin()
        .open(&output.files[2], &OpenOptions::default())
        .unwrap();
    let info = last.info().clone();
    let video = info.video.unwrap();
    assert_eq!(video.codec, "exr/piz");
    assert_eq!(video.color.primaries, ColorPrimaries::AcesAp1);
    assert_eq!(video.pixel_aspect, PixelAspect::new(2, 1));
    assert_eq!(video.alpha, AlphaMode::Premultiplied);
    assert_eq!(info.timecode.unwrap().to_string(), "01:00:00:02");
    let frame = last.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_close(frame.pixel_f32(0, 0), [4.0, 0.125, 0.1, 0.5], 1e-2);
    assert_close(frame.pixel_f32(5, 2), [0.5, 0.125, 0.1, 0.5], 1e-3);
}

#[test]
fn cancel_removes_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let settings = EncodeSettings::new(
        dir.path().join("c_#.png"),
        VideoCodec::Png,
        4,
        4,
        FrameRate::FPS_24,
    );
    let cancel = CancelToken::new();
    let token = cancel.clone();
    let mut seen = Vec::new();
    let result = render(
        &job("c", 0..10, settings),
        &mut cpu_producer(4, 4),
        None,
        &bae_media::MediaEncoders::builtin(),
        &cancel,
        &mut |p| {
            seen.push(p.done);
            if p.done == 3 {
                token.cancel();
            }
        },
    );
    assert!(matches!(result, Err(MediaError::Canceled)), "{result:?}");
    assert_eq!(seen, [1, 2, 3]);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "partial frames removed"
    );
}

#[test]
fn gpu_frames_are_read_back_in_order() {
    let Some(ctx) = common::gpu() else { return };
    let media = bae_media::MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let settings = EncodeSettings::new(
        dir.path().join("g_[###].exr"),
        VideoCodec::Exr(ExrCompression::Zip),
        16,
        8,
        FrameRate::FPS_30,
    )
    .with_alpha(AlphaMode::Premultiplied);
    let mut producer = |n: i64| {
        let frame = media.upload(&test_frame(n, 16, 8), &FrameRequest::new(0))?;
        Ok(ProducedFrame::Gpu(frame))
    };
    let mut progress = Vec::new();
    let output = render(
        &job("g", 0..7, settings),
        &mut producer,
        Some(&media),
        &bae_media::MediaEncoders::builtin(),
        &CancelToken::new(),
        &mut |p| progress.push(p.done),
    )
    .unwrap();
    assert_eq!(output.frames, 7);
    assert_eq!(progress, [1, 2, 3, 4, 5, 6, 7]);
    for (n, path) in output.files.iter().enumerate() {
        let mut source = MediaFormats::builtin()
            .open(path, &OpenOptions::default())
            .unwrap();
        let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
        // Blue carries the frame number: order survived the async readback.
        assert_close(
            frame.pixel_f32(0, 0),
            [0.0, 0.125, n as f32 / 20.0, 0.5],
            2e-3,
        );
    }
}

#[test]
fn queue_renders_jobs_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut queue = RenderQueue::new();
    let png = |name: &str| {
        EncodeSettings::new(
            dir.path().join(format!("{name}_#.png")),
            VideoCodec::Png,
            4,
            4,
            FrameRate::FPS_24,
        )
    };
    let a = queue.add(job("a", 0..3, png("a")));
    let bad = queue.add(job(
        "bad",
        0..3,
        EncodeSettings::new(
            dir.path().join("x.mov"),
            VideoCodec::H264,
            3,
            3,
            FrameRate::FPS_24,
        ),
    ));
    let skipped = queue.add(job("skipped", 0..3, png("s")));
    queue.cancel(skipped);
    let b = queue.add(job("b", 5..7, png("b")));
    queue.render_all(
        |_job| Ok(Box::new(cpu_producer(4, 4))),
        None,
        &bae_media::MediaEncoders::builtin(),
    );
    assert_eq!(queue.get(a).unwrap().status, RenderStatus::Done);
    assert_eq!(queue.get(a).unwrap().output.as_ref().unwrap().frames, 3);
    let failed = queue.get(bad).unwrap();
    assert_eq!(failed.status, RenderStatus::Failed);
    assert!(
        failed.error.as_ref().unwrap().contains("even sizes"),
        "{:?}",
        failed.error
    );
    assert_eq!(queue.get(skipped).unwrap().status, RenderStatus::Canceled);
    assert_eq!(queue.get(b).unwrap().status, RenderStatus::Done);
    assert!(dir.path().join("b_1.png").exists());
}

// ---- TIFF, compression levels, parallel writer --------------------------------

/// Straight 8-bit RGBA test pixels: a ramp with varying alpha.
fn rgba8_frame(n: u8, width: u32, height: u32) -> CpuFrame {
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&[
                (x * 255 / (width - 1)) as u8,
                (y * 255 / (height - 1)) as u8,
                n.wrapping_mul(37),
                (64 + (x + y) * 191 / (width + height - 2)) as u8,
            ]);
        }
    }
    CpuFrame {
        width,
        height,
        format: PixelFormat::Rgba8,
        data,
        alpha: AlphaMode::Straight,
        color: ColorInfo::srgb(),
        pixel_aspect: PixelAspect::SQUARE,
    }
}

fn write_sequence(settings: &EncodeSettings, frames: &[CpuFrame]) -> bae_media::EncodeOutput {
    let mut encoder = bae_media::MediaEncoders::builtin()
        .create(settings)
        .unwrap();
    for frame in frames {
        encoder.push_frame_owned(frame.clone()).unwrap();
    }
    encoder.finish().unwrap()
}

#[test]
fn tiff_sequences_round_trip() {
    use bae_media::TiffCompression;
    let dir = tempfile::tempdir().unwrap();
    let frames: Vec<CpuFrame> = (0..3).map(|n| rgba8_frame(n, 9, 5)).collect();
    for (name, compression, bits, alpha) in [
        ("straight", TiffCompression::None, 8, AlphaMode::Straight),
        ("premul", TiffCompression::Lzw, 8, AlphaMode::Premultiplied),
        ("deep", TiffCompression::Deflate, 16, AlphaMode::Straight),
        ("rgb", TiffCompression::None, 8, AlphaMode::Opaque),
    ] {
        let mut settings = EncodeSettings::new(
            dir.path().join(format!("{name}/f_####.tif")),
            VideoCodec::Tiff(compression),
            9,
            5,
            FrameRate::FPS_24,
        )
        .with_alpha(alpha);
        settings.bit_depth = bits;
        let output = write_sequence(&settings, &frames);
        assert_eq!(output.frames, 3);
        assert_eq!(
            output.files[2],
            dir.path().join(format!("{name}/f_0002.tif"))
        );
        if compression == TiffCompression::None && bits == 8 && alpha.has_alpha() {
            // Uncompressed: the pixels plus a small header.
            let len = std::fs::metadata(&output.files[0]).unwrap().len();
            assert!((9 * 5 * 4..9 * 5 * 4 + 1024).contains(&len), "{len}");
        }
        let mut source = MediaFormats::builtin()
            .open(&output.files[1], &OpenOptions::default())
            .unwrap();
        let video = source.info().video.clone().unwrap();
        assert_eq!((video.width, video.height), (9, 5), "{name}");
        assert_eq!(video.bit_depth, bits, "{name}");
        // ExtraSamples tells straight from premultiplied.
        assert_eq!(video.alpha, alpha, "{name}");
        // CPU decoding delivers the file's alpha (tagged); compare straight.
        let wanted = if alpha.has_alpha() {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        };
        let decoded = source.decode_cpu(&FrameRequest::new(0)).unwrap();
        let decoded = bae_media::export::convert_frame(&decoded, PixelFormat::Rgba16Float, wanted);
        let expected =
            bae_media::export::convert_frame(&frames[1], PixelFormat::Rgba16Float, alpha);
        let expected =
            bae_media::export::convert_frame(&expected, PixelFormat::Rgba16Float, wanted);
        // Premultiplied 8-bit loses precision at low alpha.
        let tolerance = if alpha == AlphaMode::Premultiplied {
            4.0 / 255.0
        } else {
            1.0 / 255.0
        };
        for (x, y) in [(0, 0), (4, 2), (8, 4)] {
            assert_close(decoded.pixel_f32(x, y), expected.pixel_f32(x, y), tolerance);
        }
    }
}

#[test]
fn png_compression_levels_are_lossless() {
    use bae_media::PngCompression;
    let dir = tempfile::tempdir().unwrap();
    let frame = rgba8_frame(5, 33, 17);
    let mut sizes = Vec::new();
    for (i, level) in [
        PngCompression::None,
        PngCompression::Fastest,
        PngCompression::Fast,
        PngCompression::Balanced,
        PngCompression::High,
        PngCompression::Level(1),
        PngCompression::Level(9),
    ]
    .into_iter()
    .enumerate()
    {
        let path = dir.path().join(format!("level{i}.png"));
        let mut settings = EncodeSettings::new(&path, VideoCodec::Png, 33, 17, FrameRate::FPS_24)
            .with_alpha(AlphaMode::Straight);
        settings.png_compression = level;
        let stats = bae_media::export::write_image_file(&settings, &path, &frame).unwrap();
        assert_eq!(stats.bytes, std::fs::metadata(&path).unwrap().len());
        sizes.push(stats.bytes);
        let mut source = MediaFormats::builtin()
            .open(&path, &OpenOptions::default())
            .unwrap();
        let decoded = source
            .decode_cpu(&FrameRequest {
                alpha_output: bae_media::AlphaOutput::Straight,
                ..FrameRequest::new(0)
            })
            .unwrap();
        let decoded =
            bae_media::export::convert_frame(&decoded, PixelFormat::Rgba8, AlphaMode::Straight);
        assert_eq!(decoded.data, frame.data, "{level:?}");
    }
    // Stored is the largest.
    assert!(sizes[1..].iter().all(|s| *s < sizes[0]), "{sizes:?}");
}

#[test]
fn sequence_writer_threads_stats_and_owned_frames() {
    let dir = tempfile::tempdir().unwrap();
    let frames: Vec<CpuFrame> = (0..12).map(|n| rgba8_frame(n, 16, 8)).collect();
    let mut settings = EncodeSettings::new(
        dir.path().join("owned/f_##.png"),
        VideoCodec::Png,
        16,
        8,
        FrameRate::FPS_24,
    )
    .with_alpha(AlphaMode::Straight);
    settings.threads = Some(3);
    let encoder = bae_media::export::ImageSequenceEncoder::new(&settings).unwrap();
    assert_eq!(encoder.threads(), 3);
    let owned = write_sequence(&settings, &frames);
    assert_eq!(owned.stats.threads, 3);
    let on_disk: u64 = owned
        .files
        .iter()
        .map(|f| std::fs::metadata(f).unwrap().len())
        .sum();
    assert_eq!(owned.stats.bytes, on_disk);
    assert!(owned.stats.encode > std::time::Duration::ZERO);

    // Borrowed frames on one thread write the same files.
    settings.path = dir.path().join("borrowed/f_##.png");
    settings.threads = Some(1);
    let mut encoder = bae_media::MediaEncoders::builtin()
        .create(&settings)
        .unwrap();
    for frame in &frames {
        encoder.push_frame(frame).unwrap();
    }
    let borrowed = encoder.finish().unwrap();
    assert_eq!(borrowed.stats.threads, 1);
    for (a, b) in owned.files.iter().zip(&borrowed.files) {
        assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
    }
}

#[test]
fn sequence_writer_threads_capped_by_memory() {
    // 8K float frames: two in flight already exceed the queue budget, so one
    // writer thread however many are asked for (nothing is allocated here).
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("big_####.exr"),
        VideoCodec::Exr(ExrCompression::None),
        8192,
        4320,
        FrameRate::FPS_24,
    );
    settings.bit_depth = 32;
    settings.threads = Some(8);
    let encoder = bae_media::export::ImageSequenceEncoder::new(&settings).unwrap();
    assert_eq!(encoder.threads(), 1);
    // 1080p 8-bit: the core count (or the request) is the limit.
    let settings = EncodeSettings {
        threads: Some(6),
        ..EncodeSettings::new(
            dir.path().join("hd_####.png"),
            VideoCodec::Png,
            1920,
            1080,
            FrameRate::FPS_24,
        )
    };
    assert_eq!(
        bae_media::export::ImageSequenceEncoder::new(&settings)
            .unwrap()
            .threads(),
        6
    );
}

#[test]
fn sequence_write_error_surfaces_and_abort_removes_files() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where frame 2's file goes.
    std::fs::create_dir_all(dir.path().join("f_02.png")).unwrap();
    let settings = EncodeSettings::new(
        dir.path().join("f_##.png"),
        VideoCodec::Png,
        16,
        8,
        FrameRate::FPS_24,
    );
    let mut encoder = bae_media::MediaEncoders::builtin()
        .create(&settings)
        .unwrap();
    let mut failed = false;
    for n in 0..40 {
        if encoder.push_frame_owned(rgba8_frame(n, 16, 8)).is_err() {
            failed = true;
            break;
        }
    }
    if failed {
        encoder.abort();
    } else {
        assert!(encoder.finish().is_err(), "the failed frame is reported");
    }
    let files: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .collect();
    assert!(
        !failed || files.is_empty(),
        "abort removes written files: {files:?}"
    );
}

/// A failed frame does not take the frames before it with it: they are all
/// written (writer threads skip only later frames), and `finish` reports the
/// failing file.
#[test]
fn sequence_write_error_keeps_earlier_frames() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("f_02.png")).unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("f_##.png"),
        VideoCodec::Png,
        16,
        8,
        FrameRate::FPS_24,
    );
    settings.threads = Some(4);
    let mut encoder = bae_media::MediaEncoders::builtin()
        .create(&settings)
        .unwrap();
    let mut pushed = Ok(());
    for n in 0..12 {
        pushed = encoder.push_frame_owned(rgba8_frame(n, 16, 8));
        if pushed.is_err() {
            break;
        }
    }
    // As the export does after an error: finish (waits for the writers, keeps
    // the files) instead of abort (removes them).
    let finished = encoder.finish();
    let error = pushed.err().or(finished.err()).expect("frame 2 fails");
    assert!(error.to_string().contains("f_02.png"), "{error}");
    for kept in ["f_00.png", "f_01.png"] {
        assert!(dir.path().join(kept).is_file(), "{kept} kept");
    }
}
