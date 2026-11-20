//! macOS movie encoding (AVAssetWriter / VideoToolbox), checked by decoding
//! the result with the AVFoundation decoder: frame order, codec, alpha, bit
//! depth, color tags, pixel aspect.

#![cfg(target_vendor = "apple")]

mod common;

use bae_media::export::{RenderJob, render};
use bae_media::{
    AlphaMode, CancelToken, ColorInfo, ColorPrimaries, CpuFrame, EncodeSettings, FrameRate,
    FrameRequest, MediaEncoders, MediaError, MediaFormats, MediaSource, OpenOptions, PixelAspect,
    PixelFormat, ProResProfile, ProducedFrame, TransferFunction, VideoCodec,
};
use common::movie::{ALPHA, code_pixel, frame_of};

const W: u32 = 128;
const H: u32 = 64;

/// Frame `n` as bar code; `alpha` decides whether the 50 % alpha counts.
fn coded_frame(n: u32, alpha: AlphaMode) -> CpuFrame {
    let mut data = Vec::with_capacity((W * H * 4) as usize);
    for _y in 0..H {
        for x in 0..W {
            data.extend_from_slice(&code_pixel(n, x, W));
        }
    }
    CpuFrame {
        width: W,
        height: H,
        format: PixelFormat::Rgba8,
        data,
        alpha,
        color: ColorInfo::srgb(),
        pixel_aspect: PixelAspect::SQUARE,
    }
}

fn encode(settings: EncodeSettings, frames: u32, input_alpha: AlphaMode) -> Box<dyn MediaSource> {
    let path = settings.path.clone();
    let output = render(
        &RenderJob {
            name: "test".into(),
            frames: 0..i64::from(frames),
            settings,
        },
        &mut |n: i64| Ok(ProducedFrame::Cpu(coded_frame(n as u32, input_alpha))),
        None,
        &MediaEncoders::builtin(),
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(output.files, std::slice::from_ref(&path));
    assert_eq!(output.frames, u64::from(frames));
    MediaFormats::builtin()
        .open(&path, &OpenOptions::default())
        .unwrap()
}

/// Checks a scrambled set of frames; `straight` reads the bars from straight
/// RGB (divides by alpha when the frame is premultiplied).
fn check_frames(source: &mut dyn MediaSource, frames: u32) {
    let mut index = 3_u32;
    for _ in 0..frames {
        index = (index * 7 + 5) % frames;
        let frame = source
            .decode_cpu(&FrameRequest::new(i64::from(index)))
            .unwrap();
        let decoded = frame_of(W, |x| {
            let [r, g, b, a] = frame.pixel_f32(x, H / 2);
            if frame.alpha == AlphaMode::Premultiplied && a > 0.0 {
                [r / a, g / a, b / a, a]
            } else {
                [r, g, b, a]
            }
        });
        assert_eq!(decoded, index, "frame {index}");
    }
}

#[test]
fn h264_mp4_with_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("out.mp4"),
        VideoCodec::H264,
        W,
        H,
        FrameRate::FPS_25,
    );
    settings.keyframe_interval = Some(10);
    settings.bit_rate = Some(2_000_000);
    // Explicit HD tags (the SD-size default would be BT.601).
    settings.color.primaries = ColorPrimaries::Bt709;
    settings.color.matrix = bae_media::MatrixCoefficients::Bt709;
    let mut source = encode(settings, 30, AlphaMode::Opaque);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.codec, "h264");
    assert_eq!((video.width, video.height), (W, H));
    assert_eq!(video.frame_rate, Some(FrameRate::FPS_25));
    assert_eq!(video.frame_count, Some(30));
    assert_eq!(video.alpha, AlphaMode::Opaque);
    assert_eq!(video.color.primaries, ColorPrimaries::Bt709);
    assert_eq!(video.color.transfer, TransferFunction::Bt709);
    check_frames(source.as_mut(), 30);
}

#[test]
fn sd_defaults_match_ae_tags() {
    // AE tags SD-sized H.264 as SMPTE-C primaries, BT.709 transfer, BT.601 matrix.
    let dir = tempfile::tempdir().unwrap();
    let settings = EncodeSettings::new(
        dir.path().join("sd.mp4"),
        VideoCodec::H264,
        W,
        H,
        FrameRate::FPS_24,
    );
    let source = encode(settings, 2, AlphaMode::Opaque);
    let color = source.info().video.clone().unwrap().color;
    assert_eq!(color.primaries, ColorPrimaries::Bt601Ntsc);
    assert_eq!(color.transfer, TransferFunction::Bt709);
    assert_eq!(color.matrix, bae_media::MatrixCoefficients::Bt601);
}

#[test]
fn hevc_ten_bit_display_p3_pq() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("hdr.mov"),
        VideoCodec::Hevc,
        W,
        H,
        FrameRate::FPS_29_97,
    );
    settings.bit_depth = 10;
    settings.color.primaries = ColorPrimaries::Bt2020;
    settings.color.transfer = TransferFunction::Pq;
    let mut source = encode(settings, 12, AlphaMode::Opaque);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.codec, "hevc");
    assert_eq!(video.bit_depth, 10);
    assert_eq!(video.frame_rate, Some(FrameRate::FPS_29_97));
    assert_eq!(video.color.primaries, ColorPrimaries::Bt2020);
    assert_eq!(video.color.transfer, TransferFunction::Pq);
    check_frames(source.as_mut(), 12);
}

#[test]
fn prores_422_hq_with_pixel_aspect() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = EncodeSettings::new(
        dir.path().join("master.mov"),
        VideoCodec::ProRes(ProResProfile::Hq),
        W,
        H,
        FrameRate::FPS_24,
    );
    settings.pixel_aspect = PixelAspect::new(2, 1);
    let mut source = encode(settings, 10, AlphaMode::Opaque);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.codec, "prores422hq");
    assert_eq!(video.pixel_aspect, PixelAspect::new(2, 1));
    check_frames(source.as_mut(), 10);
}

#[test]
fn prores_4444_keeps_straight_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let settings = EncodeSettings::new(
        dir.path().join("alpha.mov"),
        VideoCodec::ProRes(ProResProfile::P4444),
        W,
        H,
        FrameRate::FPS_24,
    )
    .with_alpha(AlphaMode::Straight);
    let mut source = encode(settings, 8, AlphaMode::Straight);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.codec, "prores4444");
    assert_eq!(video.alpha, AlphaMode::Straight);
    let frame = source.decode_cpu(&FrameRequest::new(5)).unwrap();
    let a = frame.pixel_f32(3, 3)[3];
    assert!((a - f32::from(ALPHA) / 255.0).abs() < 0.02, "alpha {a}");
    check_frames(source.as_mut(), 8);
}

#[test]
fn hevc_with_premultiplied_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let settings = EncodeSettings::new(
        dir.path().join("alpha_hevc.mov"),
        VideoCodec::Hevc,
        W,
        H,
        FrameRate::FPS_30,
    )
    .with_alpha(AlphaMode::Premultiplied);
    let mut source = encode(settings, 6, AlphaMode::Straight);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.codec, "hevc");
    assert_eq!(video.alpha, AlphaMode::Premultiplied);
    let frame = source.decode_cpu(&FrameRequest::new(2)).unwrap();
    assert!((frame.pixel_f32(3, 3)[3] - f32::from(ALPHA) / 255.0).abs() < 0.03);
    check_frames(source.as_mut(), 6);
}

#[test]
fn cancel_deletes_the_movie_and_bad_settings_fail_early() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("partial.mov");
    let settings = EncodeSettings::new(&path, VideoCodec::H264, W, H, FrameRate::FPS_24);
    let cancel = CancelToken::new();
    let token = cancel.clone();
    let result = render(
        &RenderJob {
            name: "c".into(),
            frames: 0..20,
            settings,
        },
        &mut |n: i64| Ok(ProducedFrame::Cpu(coded_frame(n as u32, AlphaMode::Opaque))),
        None,
        &MediaEncoders::builtin(),
        &cancel,
        &mut |p| {
            if p.done == 4 {
                token.cancel();
            }
        },
    );
    assert!(matches!(result, Err(MediaError::Canceled)));
    assert!(!path.exists());

    // ProRes cannot go into MP4; H.264 cannot carry alpha.
    let encoders = MediaEncoders::builtin();
    let prores_mp4 = EncodeSettings::new(
        dir.path().join("x.mp4"),
        VideoCodec::ProRes(ProResProfile::Standard),
        W,
        H,
        FrameRate::FPS_24,
    );
    assert!(encoders.create(&prores_mp4).is_err());
    let h264_alpha = EncodeSettings::new(
        dir.path().join("x.mov"),
        VideoCodec::H264,
        W,
        H,
        FrameRate::FPS_24,
    )
    .with_alpha(AlphaMode::Straight);
    assert!(encoders.create(&h264_alpha).is_err());
}
