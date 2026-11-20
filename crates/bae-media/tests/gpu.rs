//! Decoding into `Rgba16Float` GPU textures: upload, alpha conversion, readback
//! and the frame cache. Skipped (with a note) on machines without a GPU adapter;
//! `BAE_REQUIRE_GPU=1` turns that into a failure.

mod common;

use bae_media::{
    AlphaMode, FrameCache, FrameRequest, MediaFormats, MediaGpu, OpenOptions, WORKING_FORMAT,
    source_id_for,
};
use common::{assert_close, gpu, write_png, write_png_with};

fn exr_fixture(path: &std::path::Path) {
    use exr::prelude::{Image, SpecificChannels, Vec2, WritableImage as _, f16};
    let channels = SpecificChannels::rgba(|_: Vec2<usize>| {
        (
            f16::from_f32(1.0),
            f16::from_f32(2.0),
            f16::from_f32(4.0),
            f16::from_f32(0.5),
        )
    });
    Image::from_channels((5, 3), channels)
        .write()
        .to_file(path)
        .unwrap();
}

#[test]
fn straight_png_becomes_premultiplied_half_float() {
    let Some(ctx) = gpu() else { return };
    let media = MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.png");
    write_png(
        &path,
        2,
        1,
        png::ColorType::Rgba,
        &[255, 0, 0, 128, 0, 255, 255, 255],
    );
    let mut source = MediaFormats::builtin()
        .open(&path, &OpenOptions::default())
        .unwrap();

    let frame = source.decode_gpu(&FrameRequest::new(0), &media).unwrap();
    assert_eq!(frame.texture.format(), WORKING_FORMAT);
    assert_eq!((frame.width(), frame.height()), (2, 1));
    assert_eq!(frame.alpha, AlphaMode::Premultiplied);
    let pixels = media.read_back(&frame).unwrap();
    let a = 128.0 / 255.0;
    assert_close(pixels.pixel_f32(0, 0), [a, 0.0, 0.0, a], 2e-3);
    assert_close(pixels.pixel_f32(1, 0), [0.0, 1.0, 1.0, 1.0], 1e-3);

    // Straight output keeps the color.
    let frame = source
        .decode_gpu(&FrameRequest::new(0).straight(), &media)
        .unwrap();
    assert_eq!(frame.alpha, AlphaMode::Straight);
    let pixels = media.read_back(&frame).unwrap();
    assert_close(pixels.pixel_f32(0, 0), [1.0, 0.0, 0.0, a], 2e-3);

    // "Ignore alpha" makes it opaque.
    let frame = source
        .decode_gpu(
            &FrameRequest::new(0).with_alpha_override(AlphaMode::Ignore),
            &media,
        )
        .unwrap();
    assert_eq!(frame.alpha, AlphaMode::Opaque);
    let pixels = media.read_back(&frame).unwrap();
    assert_close(pixels.pixel_f32(0, 0), [1.0, 0.0, 0.0, 1.0], 1e-3);
}

#[test]
fn sixteen_bit_png_keeps_precision() {
    let Some(ctx) = gpu() else { return };
    let media = MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep.png");
    // 0x8000 / 65535 needs more than 8 bits to round-trip.
    write_png_with(
        &path,
        1,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Sixteen,
        &[0x80, 0x00, 0x00, 0x01, 0xFF, 0xFF],
        |_| {},
    );
    let mut source = MediaFormats::builtin()
        .open(&path, &OpenOptions::default())
        .unwrap();
    let frame = source.decode_gpu(&FrameRequest::new(0), &media).unwrap();
    assert_eq!(frame.alpha, AlphaMode::Opaque);
    let pixels = media.read_back(&frame).unwrap();
    assert_close(
        pixels.pixel_f32(0, 0),
        [32768.0 / 65535.0, 1.0 / 65535.0, 1.0, 1.0],
        1e-4,
    );
}

#[test]
fn exr_uploads_without_clamping() {
    let Some(ctx) = gpu() else { return };
    let media = MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hdr.exr");
    exr_fixture(&path);
    let mut source = MediaFormats::builtin()
        .open(&path, &OpenOptions::default())
        .unwrap();

    // Premultiplied → premultiplied: direct upload.
    let frame = source.decode_gpu(&FrameRequest::new(0), &media).unwrap();
    assert_eq!(frame.alpha, AlphaMode::Premultiplied);
    let pixels = media.read_back(&frame).unwrap();
    assert_close(pixels.pixel_f32(4, 2), [1.0, 2.0, 4.0, 0.5], 1e-3);

    // Straight output divides by alpha.
    let frame = source
        .decode_gpu(&FrameRequest::new(0).straight(), &media)
        .unwrap();
    let pixels = media.read_back(&frame).unwrap();
    assert_close(pixels.pixel_f32(0, 0), [2.0, 4.0, 8.0, 0.5], 1e-2);
}

#[test]
fn frame_cache_serves_repeats() {
    let Some(ctx) = gpu() else { return };
    let media = MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    for i in 1..=3_u8 {
        common::write_solid_png(
            &dir.path().join(format!("c_{i:03}.png")),
            8,
            8,
            [i * 50, 0, 0, 255],
        );
    }
    let first = dir.path().join("c_001.png");
    let mut source = MediaFormats::builtin()
        .open(&first, &OpenOptions::sequence(bae_media::FrameRate::FPS_25))
        .unwrap();
    let id = source_id_for(&first);
    // Budget for exactly two 8×8 half-float frames.
    let cache = FrameCache::new(2 * 8 * 8 * 8);

    let a = cache
        .decode(id, source.as_mut(), &FrameRequest::new(0), &media)
        .unwrap();
    let again = cache
        .decode(id, source.as_mut(), &FrameRequest::new(0), &media)
        .unwrap();
    assert_eq!(
        a.texture, again.texture,
        "second request is served from the cache"
    );
    cache
        .decode(id, source.as_mut(), &FrameRequest::new(1), &media)
        .unwrap();
    // Touch 0 so that 1 is the least recently used, then add 2.
    cache
        .decode(id, source.as_mut(), &FrameRequest::new(0), &media)
        .unwrap();
    let c = cache
        .decode(id, source.as_mut(), &FrameRequest::new(2), &media)
        .unwrap();
    let stats = cache.stats();
    assert_eq!((stats.frames, stats.hits, stats.misses), (2, 2, 3));
    assert!(cache.contains(&bae_media::FrameKey::new(id, &FrameRequest::new(0))));
    assert!(!cache.contains(&bae_media::FrameKey::new(id, &FrameRequest::new(1))));

    let pixels = media.read_back(&c).unwrap();
    assert_close(pixels.pixel_f32(3, 3), [150.0 / 255.0, 0.0, 0.0, 1.0], 1e-3);

    cache.remove_source(id);
    assert_eq!(cache.stats().frames, 0);
}
