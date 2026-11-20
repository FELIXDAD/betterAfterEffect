//! Compares After Effects renders with what bae-media reads and writes.
//!
//! Needs reference files rendered by AE (left half: opaque solid
//! (0.75, 0.25, 0.5); right half: 50 % solid (0.2, 0.6, 0.9); templates
//! "H.264 - Match Render Settings - 15 Mbps", "High Quality", "High Quality with
//! Alpha"; script: `tests/ae/render_refs.jsx`). The test also writes the same
//! picture with bae-media's encoder and prints both:
//! `BAE_AE_REF_DIR=/private/tmp/bae-08-ae cargo test -p bae-media --test ae_reference -- --ignored --nocapture`.

#![cfg(target_vendor = "apple")]

use bae_media::export::{RenderJob, render};
use bae_media::{
    AlphaMode, CancelToken, ColorInfo, CpuFrame, EncodeSettings, FrameRate, FrameRequest,
    MediaEncoders, MediaFormats, OpenOptions, PixelAspect, PixelFormat, ProResProfile,
    ProducedFrame, VideoCodec,
};

/// The AE test comp as a premultiplied frame.
fn reference_frame(width: u32, height: u32) -> CpuFrame {
    let left = [0.75_f32, 0.25, 0.5, 1.0];
    let right = [0.1_f32, 0.3, 0.45, 0.5];
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..height {
        for x in 0..width {
            let px = if x < width / 2 { left } else { right };
            data.extend(px.map(|v| (v * 255.0).round() as u8));
        }
    }
    CpuFrame {
        width,
        height,
        format: PixelFormat::Rgba8,
        data,
        alpha: AlphaMode::Premultiplied,
        color: ColorInfo::srgb(),
        pixel_aspect: PixelAspect::SQUARE,
    }
}

fn write_ours(dir: &std::path::Path) {
    let jobs = [
        ("bae_hd_h264.mp4", VideoCodec::H264, AlphaMode::Opaque),
        (
            "bae_hd_hq.mov",
            VideoCodec::ProRes(ProResProfile::Standard),
            AlphaMode::Opaque,
        ),
        (
            "bae_hd_hq_alpha.mov",
            VideoCodec::ProRes(ProResProfile::P4444),
            AlphaMode::Premultiplied,
        ),
    ];
    for (name, codec, alpha) in jobs {
        let mut settings =
            EncodeSettings::new(dir.join(name), codec, 1920, 1080, FrameRate::FPS_24);
        settings.alpha = alpha;
        render(
            &RenderJob {
                name: name.into(),
                frames: 0..2,
                settings,
            },
            &mut |_| Ok(ProducedFrame::Cpu(reference_frame(1920, 1080))),
            None,
            &MediaEncoders::builtin(),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
    }
}

#[test]
#[ignore = "needs After Effects reference renders (BAE_AE_REF_DIR)"]
fn probe_ae_renders() {
    let Some(dir) = std::env::var_os("BAE_AE_REF_DIR") else {
        eprintln!("BAE_AE_REF_DIR not set");
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    write_ours(&dir);
    for name in [
        "ae_h264.mp4",
        "ae_hq.mov",
        "ae_hq_alpha.mov",
        "ae_hd_h264.mp4",
        "ae_hd_hq.mov",
        "ae_hd_hq_alpha.mov",
        "bae_hd_h264.mp4",
        "bae_hd_hq.mov",
        "bae_hd_hq_alpha.mov",
    ] {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let mut source = MediaFormats::builtin()
            .open(&path, &OpenOptions::default())
            .unwrap();
        let video = source.info().video.clone().unwrap();
        let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
        let px = |x: u32| {
            frame
                .pixel_f32(x, frame.height / 2)
                .map(|v| (v * 2550.0).round() / 10.0)
        };
        eprintln!(
            "{name}: {} {}x{} depth {} alpha {:?} (guess {:?})\n  primaries {:?} transfer {:?} matrix {:?} range {:?}\n  left {:?} right {:?}",
            video.codec,
            video.width,
            video.height,
            video.bit_depth,
            video.alpha,
            frame.guess_alpha(),
            video.color.primaries,
            video.color.transfer,
            video.color.matrix,
            video.color.range,
            px(frame.width / 4),
            px(frame.width * 3 / 4)
        );
    }
}
