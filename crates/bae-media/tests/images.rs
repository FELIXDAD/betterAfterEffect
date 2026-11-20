//! Still-image and image-sequence decoding on the CPU, with fixtures generated
//! in code.

mod common;

use std::path::Path;

use bae_media::{
    AlphaMode, ColorPrimaries, FrameRate, FrameRequest, MediaError, MediaFormats, MediaKind,
    OpenOptions, PixelAspect, PixelFormat, SampleType, Time, TransferFunction,
};
use common::{assert_close, write_png, write_png_with, write_solid_png};

fn open(path: &Path) -> Box<dyn bae_media::MediaSource> {
    MediaFormats::builtin()
        .open(path, &OpenOptions::default())
        .unwrap()
}

#[test]
fn png_rgba8_straight_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rgba.png");
    let data = [
        255, 0, 0, 255, 0, 255, 0, 128, //
        0, 0, 255, 0, 10, 20, 30, 40,
    ];
    write_png(&path, 2, 2, png::ColorType::Rgba, &data);
    let mut source = open(&path);
    let info = source.info().clone();
    assert_eq!(info.kind, MediaKind::Still);
    assert_eq!(info.format, "png");
    let video = info.video.unwrap();
    assert_eq!((video.width, video.height, video.bit_depth), (2, 2, 8));
    assert_eq!(video.alpha, AlphaMode::Straight);
    assert_eq!(video.frame_count, None);
    // Any frame index shows the still.
    let frame = source.decode_cpu(&FrameRequest::new(42)).unwrap();
    assert_eq!(frame.format, PixelFormat::Rgba8);
    assert_eq!(frame.data, data);
    assert_eq!(source.frame_at(Time::from_seconds(10.0)), 0);
}

#[test]
fn png_gray16_palette_and_tags() {
    let dir = tempfile::tempdir().unwrap();

    // 16-bit gray → RGBA16, opaque.
    let gray = dir.path().join("gray16.png");
    write_png_with(
        &gray,
        2,
        1,
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        &[0x12, 0x34, 0xFF, 0xFF],
        |e| e.set_source_srgb(png::SrgbRenderingIntent::Perceptual),
    );
    let mut source = open(&gray);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.bit_depth, 16);
    assert_eq!(video.alpha, AlphaMode::Opaque);
    assert_eq!(video.color.transfer, TransferFunction::Srgb);
    assert_eq!(video.color.primaries, ColorPrimaries::Bt709);
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_eq!(frame.format, PixelFormat::Rgba16);
    let v = f32::from(0x1234_u16) / 65535.0;
    assert_close(frame.pixel_f32(0, 0), [v, v, v, 1.0], 1e-6);
    assert_close(frame.pixel_f32(1, 0), [1.0; 4], 1e-6);

    // Palette with tRNS → RGBA8 with alpha; gamma tag; non-square pixels.
    let pal = dir.path().join("palette.png");
    write_png_with(
        &pal,
        2,
        1,
        png::ColorType::Indexed,
        png::BitDepth::Eight,
        &[0, 1],
        |e| {
            e.set_palette(vec![255, 0, 0, 0, 0, 255]);
            e.set_trns(vec![255, 0]);
            e.set_source_gamma(png::ScaledFloat::new(1.0 / 2.2));
            e.set_pixel_dims(Some(png::PixelDimensions {
                xppu: 10,
                yppu: 11,
                unit: png::Unit::Unspecified,
            }));
        },
    );
    let mut source = open(&pal);
    let video = source.info().video.clone().unwrap();
    assert_eq!(video.alpha, AlphaMode::Straight);
    assert_eq!(video.color.transfer, TransferFunction::Gamma(2.2));
    assert_eq!(video.pixel_aspect, PixelAspect::new(11, 10));
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_eq!(frame.data, [255, 0, 0, 255, 0, 0, 255, 0]);
}

#[test]
fn jpeg_decodes_to_rgba() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.jpg");
    let rgb: Vec<u8> = (0..16 * 8).flat_map(|_| [200_u8, 100, 50]).collect();
    image::save_buffer_with_format(
        &path,
        &rgb,
        16,
        8,
        image::ExtendedColorType::Rgb8,
        image::ImageFormat::Jpeg,
    )
    .unwrap();
    let mut source = open(&path);
    assert_eq!(source.info().format, "jpeg");
    let video = source.info().video.clone().unwrap();
    assert_eq!((video.width, video.height), (16, 8));
    assert_eq!(video.alpha, AlphaMode::Opaque);
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_eq!(frame.format, PixelFormat::Rgba8);
    assert_close(
        frame.pixel_f32(5, 5),
        [200.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0, 1.0],
        3.0 / 255.0,
    );
}

#[test]
fn webp_lossless_with_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sprite.webp");
    let rgba: Vec<u8> = (0..3 * 2)
        .flat_map(|i| [i as u8 * 40, 255 - i as u8 * 40, 7, 60 + i as u8])
        .collect();
    image::save_buffer_with_format(
        &path,
        &rgba,
        3,
        2,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::WebP,
    )
    .unwrap();
    let mut source = open(&path);
    assert_eq!(source.info().format, "webp");
    assert_eq!(
        source.info().video.as_ref().unwrap().alpha,
        AlphaMode::Straight
    );
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_eq!(frame.data, rgba);
}

#[test]
fn exr_half_premultiplied_linear() {
    use exr::prelude::{Image, SpecificChannels, Vec2, WritableImage as _, f16};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("render.exr");
    let channels = SpecificChannels::rgba(|pos: Vec2<usize>| {
        (
            f16::from_f32(pos.x() as f32 * 0.5),
            f16::from_f32(pos.y() as f32 * 2.0),
            f16::from_f32(4.0),
            f16::from_f32(0.5),
        )
    });
    let mut image = Image::from_channels((3, 2), channels);
    image.attributes.pixel_aspect = 2.0;
    image.write().to_file(&path).unwrap();

    let mut source = open(&path);
    let info = source.info().clone();
    assert_eq!(info.format, "exr");
    let video = info.video.unwrap();
    assert_eq!((video.width, video.height), (3, 2));
    assert_eq!(video.sample_type, SampleType::Half);
    assert_eq!(video.alpha, AlphaMode::Premultiplied);
    assert_eq!(video.color.transfer, TransferFunction::Linear);
    assert_eq!(video.pixel_aspect, PixelAspect::new(2, 1));
    let frame = source.decode_cpu(&FrameRequest::new(0)).unwrap();
    assert_eq!(frame.format, PixelFormat::Rgba16Float);
    // HDR values (> 1) survive.
    assert_close(frame.pixel_f32(2, 1), [1.0, 2.0, 4.0, 0.5], 1e-3);
}

#[test]
fn format_is_sniffed_not_guessed_from_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actually_png.jpg");
    write_solid_png(&path, 1, 1, [1, 2, 3, 255]);
    assert_eq!(open(&path).info().format, "png");

    let junk = dir.path().join("notes.txt");
    std::fs::write(&junk, b"hello").unwrap();
    let err = MediaFormats::builtin()
        .open(&junk, &OpenOptions::default())
        .unwrap_err();
    assert!(matches!(err, MediaError::UnsupportedFormat(_)), "{err}");
}

#[test]
fn image_sequence_with_gap() {
    let dir = tempfile::tempdir().unwrap();
    for (number, value) in [(1, 10_u8), (2, 20), (3, 30), (5, 50)] {
        write_solid_png(
            &dir.path().join(format!("shot_{number:04}.png")),
            4,
            4,
            [value, 0, 0, 255],
        );
    }
    let options = OpenOptions::sequence(FrameRate::FPS_24);
    let formats = MediaFormats::builtin();
    let mut source = formats
        .open(&dir.path().join("shot_0003.png"), &options)
        .unwrap();
    let info = source.info().clone();
    assert_eq!(info.kind, MediaKind::ImageSequence);
    let seq = info.sequence.unwrap();
    assert_eq!((seq.first, seq.last), (1, 5));
    assert_eq!(seq.missing, vec![4]);
    assert_eq!(seq.pattern, "shot_####.png");
    let video = info.video.unwrap();
    assert_eq!(video.frame_rate, Some(FrameRate::FPS_24));
    assert_eq!(video.frame_count, Some(5));

    // Index 2 is file 0003; frame accuracy against the time axis.
    let t = FrameRate::FPS_24.frame_to_time(2);
    assert_eq!(source.frame_at(t), 2);
    let frame = source.decode_cpu(&FrameRequest::new(2)).unwrap();
    assert_eq!(frame.pixel(0, 0), [30, 0, 0, 255]);
    let frame = source.decode_cpu(&FrameRequest::new(4)).unwrap();
    assert_eq!(frame.pixel(0, 0), [50, 0, 0, 255]);

    let missing = source.decode_cpu(&FrameRequest::new(3)).unwrap_err();
    assert!(
        matches!(missing, MediaError::MissingFrame { number: 4, .. }),
        "{missing}"
    );
    let out = source.decode_cpu(&FrameRequest::new(5)).unwrap_err();
    assert!(matches!(out, MediaError::FrameOutOfRange { .. }), "{out}");

    // Single mode opens just the file.
    let single = formats
        .open(&dir.path().join("shot_0003.png"), &OpenOptions::default())
        .unwrap();
    assert_eq!(single.info().kind, MediaKind::Still);
}
