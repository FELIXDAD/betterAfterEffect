//! PNG output (8-bit RGBA). Video encoding and other image formats belong to
//! 08's export module; this is the minimum for `bae render` and screenshots.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use bae_gpu::transfer::ImageData;

use crate::error::RenderError;

/// Writes tightly packed 8-bit RGBA rows to `path`.
pub fn write_rgba8(path: &Path, image: &ImageData) -> Result<(), RenderError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let file = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // Fast compression: render output is written once and usually large.
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder
        .write_header()
        .map_err(|e| RenderError::Other(format!("{}: {e}", path.display())))?;
    writer
        .write_image_data(&image.data)
        .map_err(|e| RenderError::Other(format!("{}: {e}", path.display())))?;
    writer
        .finish()
        .map_err(|e| RenderError::Other(format!("{}: {e}", path.display())))?;
    Ok(())
}

/// Reads an 8-bit RGBA (or RGB) PNG into tightly packed RGBA rows (tests and
/// golden-image comparisons).
pub fn read_rgba8(path: &Path) -> Result<(u32, u32, Vec<u8>), RenderError> {
    let err = |e: &dyn std::fmt::Display| RenderError::Other(format!("{}: {e}", path.display()));
    let mut decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| err(&e))?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| err(&"image too large"))?
    ];
    let info = reader.next_frame(&mut buf).map_err(|e| err(&e))?;
    let (w, h) = (info.width, info.height);
    let data = match info.color_type {
        png::ColorType::Rgba => buf[..(w * h * 4) as usize].to_vec(),
        png::ColorType::Rgb => buf[..(w * h * 3) as usize]
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..(w * h * 2) as usize]
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..(w * h) as usize]
            .iter()
            .flat_map(|&g| [g, g, g, 255])
            .collect(),
        png::ColorType::Indexed => return Err(err(&"indexed PNG after EXPAND")),
    };
    Ok((w, h, data))
}
