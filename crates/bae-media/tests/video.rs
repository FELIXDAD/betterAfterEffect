//! macOS hardware video decoding: metadata, frame-accurate random access on the
//! CPU and GPU paths, alpha. Movies are generated with AVAssetWriter.

#![cfg(target_vendor = "apple")]

mod common;

use bae_media::{
    AlphaMode, FrameRate, FrameRequest, MediaError, MediaFormats, MediaKind, OpenOptions,
    PixelFormat, Time,
};
use common::movie::{ALPHA, Codec, frame_of, write_movie};

fn open(path: &std::path::Path) -> Box<dyn bae_media::MediaSource> {
    MediaFormats::builtin()
        .open(path, &OpenOptions::default())
        .unwrap()
}

#[test]
fn h264_metadata_and_random_access() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("count.mov");
    write_movie(
        &path,
        Codec::H264 { gop: 10 },
        64,
        48,
        FrameRate::FPS_25,
        40,
    );

    let mut source = open(&path);
    let info = source.info().clone();
    assert_eq!(info.kind, MediaKind::Movie);
    assert_eq!(info.format, "avfoundation");
    let video = info.video.unwrap();
    assert_eq!(video.codec, "h264");
    assert_eq!((video.width, video.height), (64, 48));
    assert_eq!(video.frame_rate, Some(FrameRate::FPS_25));
    assert_eq!(video.frame_count, Some(40));
    assert_eq!(video.alpha, AlphaMode::Opaque);
    assert_eq!(source.frame_at(Time::from_seconds(1.0)), 25);

    // Forward, repeated, backward, across GOPs, last frame.
    for index in [0_i64, 1, 2, 2, 13, 37, 5, 39, 20, 21, 0] {
        let frame = source.decode_cpu(&FrameRequest::new(index)).unwrap();
        assert_eq!(frame.format, PixelFormat::Bgra8);
        let decoded = frame_of(64, |x| frame.pixel_f32(x, 10));
        assert_eq!(decoded, index as u32, "frame {index}");
    }
    let err = source.decode_cpu(&FrameRequest::new(40)).unwrap_err();
    assert!(matches!(err, MediaError::FrameOutOfRange { .. }), "{err}");
}

/// Every frame, in a scrambled order (scrubbing), at 29.97 fps.
#[test]
fn ntsc_rate_scrubbing_is_frame_accurate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ntsc.mov");
    let frames = 48_u32;
    write_movie(
        &path,
        Codec::H264 { gop: 12 },
        64,
        16,
        FrameRate::FPS_29_97,
        frames,
    );
    let mut source = open(&path);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.frame_rate, Some(FrameRate::FPS_29_97));
    assert_eq!(video.frame_count, Some(u64::from(frames)));
    // Time → frame mapping agrees with the model's FrameRate.
    let t = FrameRate::FPS_29_97.frame_to_time(31);
    assert_eq!(source.frame_at(t), 31);
    // A full-period LCG visits every index once in a jumpy order.
    let mut index = 7_u32;
    for _ in 0..frames {
        index = (index * 29 + 11) % frames;
        let frame = source
            .decode_cpu(&FrameRequest::new(i64::from(index)))
            .unwrap();
        assert_eq!(
            frame_of(64, |x| frame.pixel_f32(x, 8)),
            index,
            "frame {index}"
        );
    }
}

#[test]
fn h264_decodes_to_gpu_frames() {
    let Some(ctx) = common::gpu() else { return };
    let media = bae_media::MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gpu.mov");
    write_movie(&path, Codec::H264 { gop: 8 }, 96, 64, FrameRate::FPS_30, 24);
    let mut source = open(&path);
    for index in [3_i64, 4, 17, 0, 23] {
        let frame = source
            .decode_gpu(&FrameRequest::new(index), &media)
            .unwrap();
        assert_eq!((frame.width(), frame.height()), (96, 64));
        assert_eq!(frame.alpha, AlphaMode::Opaque);
        let pixels = media.read_back(&frame).unwrap();
        assert_eq!(
            frame_of(96, |x| pixels.pixel_f32(x, 30)),
            index as u32,
            "frame {index}"
        );
        assert!((pixels.pixel_f32(0, 0)[3] - 1.0).abs() < 1e-3);
    }
}

#[test]
fn prores4444_keeps_alpha_and_precision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alpha.mov");
    write_movie(&path, Codec::ProRes4444, 64, 32, FrameRate::FPS_24, 12);
    let mut source = open(&path);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.codec, "prores4444");
    assert_eq!(video.alpha, AlphaMode::Straight);
    assert!(video.bit_depth > 8);

    let frame = source.decode_cpu(&FrameRequest::new(7)).unwrap();
    assert_eq!(frame.format, PixelFormat::Rgba16Float);
    assert_eq!(frame_of(64, |x| frame.pixel_f32(x, 5)), 7);
    let a = frame.pixel_f32(5, 5)[3];
    assert!((a - f32::from(ALPHA) / 255.0).abs() < 0.02, "alpha {a}");

    let Some(ctx) = common::gpu() else { return };
    let media = bae_media::MediaGpu::new(ctx);
    let frame = source.decode_gpu(&FrameRequest::new(3), &media).unwrap();
    assert_eq!(frame.alpha, AlphaMode::Premultiplied);
    let pixels = media.read_back(&frame).unwrap();
    // Premultiplied: divide by alpha to read the code.
    let straight = |x| {
        let [r, g, b, a] = pixels.pixel_f32(x, 5);
        [r / a, g / a, b / a, a]
    };
    assert_eq!(frame_of(64, straight), 3);
    assert!((pixels.pixel_f32(5, 5)[3] - f32::from(ALPHA) / 255.0).abs() < 0.02);
}

/// 4K decode throughput, decoder → working texture (GPU work included):
/// `cargo test -p bae-media --release --test video -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark: writes 4K movies"]
fn decode_throughput_4k() {
    use std::time::Instant;

    let Some(ctx) = common::gpu() else { return };
    let media = bae_media::MediaGpu::new(ctx.clone());
    let dir = tempfile::tempdir().unwrap();
    let frames = 90_u32;
    for (name, codec) in [
        ("H.264", Codec::H264 { gop: 30 }),
        ("ProRes 422", Codec::ProRes422),
        ("ProRes 4444", Codec::ProRes4444),
    ] {
        let path = dir.path().join("bench.mov");
        let _ = std::fs::remove_file(&path);
        write_movie(&path, codec, 3840, 2160, FrameRate::FPS_30, frames);
        let mut source = open(&path);
        source.decode_gpu(&FrameRequest::new(0), &media).unwrap();
        ctx.wait_idle().unwrap();

        let start = Instant::now();
        for index in 1..i64::from(frames) {
            let frame = source
                .decode_gpu(&FrameRequest::new(index), &media)
                .unwrap();
            drop(frame);
            ctx.poll();
        }
        ctx.wait_idle().unwrap();
        let fps = f64::from(frames - 1) / start.elapsed().as_secs_f64();

        let seeks = [70_i64, 10, 55, 25, 85, 5, 40, 62];
        let start = Instant::now();
        for index in seeks {
            source
                .decode_gpu(&FrameRequest::new(index), &media)
                .unwrap();
            ctx.wait_idle().unwrap();
        }
        let seek_ms = start.elapsed().as_secs_f64() * 1000.0 / seeks.len() as f64;
        eprintln!("4K {name}: sequential {fps:.1} fps, random seek {seek_ms:.1} ms/frame");
    }
}
