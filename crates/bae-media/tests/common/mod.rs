//! Test fixtures generated in code (no binary files in the repository).

#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use bae_gpu::GpuContext;

/// Writes an 8-bit PNG.
pub fn write_png(path: &Path, width: u32, height: u32, color: png::ColorType, data: &[u8]) {
    write_png_with(
        path,
        width,
        height,
        color,
        png::BitDepth::Eight,
        data,
        |_| {},
    );
}

/// Writes a PNG, letting `setup` add chunks.
pub fn write_png_with(
    path: &Path,
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
    setup: impl FnOnce(&mut png::Encoder<'_, std::io::BufWriter<std::fs::File>>),
) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    setup(&mut encoder);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
}

/// A solid RGBA8 PNG.
pub fn write_solid_png(path: &Path, width: u32, height: u32, rgba: [u8; 4]) {
    let data: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
    write_png(path, width, height, png::ColorType::Rgba, &data);
}

/// The shared device, or `None` (test skipped) without an adapter.
pub fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

/// Asserts two float pixels are within `tolerance`.
#[track_caller]
pub fn assert_close(actual: [f32; 4], expected: [f32; 4], tolerance: f32) {
    for i in 0..4 {
        assert!(
            (actual[i] - expected[i]).abs() <= tolerance,
            "pixel {actual:?} != {expected:?} (±{tolerance})"
        );
    }
}

#[cfg(target_vendor = "apple")]
pub mod movie;
