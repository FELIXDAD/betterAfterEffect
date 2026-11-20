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
