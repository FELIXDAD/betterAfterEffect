//! Still-image formats: PNG (`png`), JPEG (`zune-jpeg`), WebP (`image`'s
//! `image-webp`), TIFF (`tiff`) and OpenEXR (`exr`), all pure Rust.
//!
//! Opening reads only the header (size, bit depth, color tags, alpha,
//! timecode); pixels are decoded on each [`MediaSource::decode_cpu`] call into
//! RGBA ([`PixelFormat::Rgba8`] / [`PixelFormat::Rgba16`] for PNG, JPEG, WebP;
//! [`PixelFormat::Rgba16Float`] for EXR). Callers cache decoded frames with
//! [`FrameCache`](crate::FrameCache).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use half::f16;

use crate::frame::{CpuFrame, FrameRequest, PixelFormat};
use crate::meta::{
    AlphaMode, Chromaticities, ColorInfo, ColorPrimaries, MatrixCoefficients, MediaInfo, MediaKind,
    PixelAspect, SampleType, TransferFunction, VideoInfo,
};
use crate::source::{MediaError, MediaFormat, MediaSource};
use crate::time::Timecode;

/// Which decoder a [`StillSource`] uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StillCodec {
    Png,
    Jpeg,
    WebP,
    Tiff,
    Exr,
}

/// A single image file.
struct StillSource {
    info: MediaInfo,
    codec: StillCodec,
}

impl StillSource {
    fn new(path: &Path, format: &str, codec: StillCodec, video: VideoInfo) -> Self {
        StillSource {
            info: MediaInfo {
                path: path.to_owned(),
                kind: MediaKind::Still,
                format: format.to_owned(),
                video: Some(video),
                audio: Vec::new(),
                timecode: None,
                sequence: None,
            },
            codec,
        }
    }

    fn video(&self) -> &VideoInfo {
        self.info
            .video
            .as_ref()
            .expect("stills always have a picture")
    }
}

impl MediaSource for StillSource {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn decode_cpu(&mut self, _request: &FrameRequest) -> Result<CpuFrame, MediaError> {
        // Every frame index shows the same picture.
        let path = &self.info.path;
        let video = self.video();
        let (width, height, format, data) = match self.codec {
            StillCodec::Png => decode_png(path)?,
            StillCodec::Jpeg => decode_jpeg(path)?,
            StillCodec::WebP => decode_webp(path)?,
            StillCodec::Tiff => decode_tiff(path)?,
            StillCodec::Exr => decode_exr(path)?,
        };
        Ok(CpuFrame {
            width,
            height,
            format,
            data,
            alpha: video.alpha,
            color: video.color.clone(),
            pixel_aspect: video.pixel_aspect,
        })
    }

    fn sync_frames(&mut self) -> Option<Vec<i64>> {
        Some(vec![0])
    }
}

type Decoded = (u32, u32, PixelFormat, Vec<u8>);

fn open_file(path: &Path) -> Result<BufReader<File>, MediaError> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|e| MediaError::io(path, e))
}

fn still_video(
    width: u32,
    height: u32,
    bit_depth: u8,
    sample_type: SampleType,
    codec: &str,
) -> VideoInfo {
    VideoInfo {
        width,
        height,
        pixel_aspect: PixelAspect::SQUARE,
        frame_rate: None,
        frame_count: None,
        bit_depth,
        sample_type,
        color: ColorInfo::default(),
        alpha: AlphaMode::Opaque,
        codec: codec.to_owned(),
    }
}

// ---- PNG -------------------------------------------------------------------

/// PNG (8 and 16 bit, palette, gray, alpha; color tags from `cICP`, `sRGB`,
/// `iCCP`, `gAMA`, `cHRM`). APNG files show their first frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct PngFormat;

impl MediaFormat for PngFormat {
    fn id(&self) -> &str {
        "png"
    }

    fn extensions(&self) -> &[&str] {
        &["png"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        header.starts_with(b"\x89PNG\r\n\x1a\n")
    }

    fn is_image(&self) -> bool {
        true
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        let reader = png::Decoder::new(open_file(path)?)
            .read_info()
            .map_err(|e| MediaError::decode(path, e))?;
        let info = reader.info();
        let has_alpha = matches!(
            info.color_type,
            png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
        ) || info.trns.is_some();
        let mut video = still_video(
            info.width,
            info.height,
            info.bit_depth as u8,
            SampleType::UInt,
            "png",
        );
        video.alpha = if has_alpha {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        };
        video.color = png_color(info);
        if let Some(dims) = info.pixel_dims {
            // Pixels per unit: fewer per unit horizontally = wider pixels.
            video.pixel_aspect = PixelAspect::new(dims.yppu, dims.xppu);
        }
        Ok(Box::new(StillSource::new(
            path,
            "png",
            StillCodec::Png,
            video,
        )))
    }
}

fn png_color(info: &png::Info<'_>) -> ColorInfo {
    if let Some(cicp) = info.coding_independent_code_points {
        let mut color = ColorInfo::from_cicp(
            cicp.color_primaries,
            cicp.transfer_function,
            cicp.matrix_coefficients,
            cicp.is_video_full_range_image,
        );
        color.matrix = MatrixCoefficients::Identity;
        return color;
    }
    let mut color = ColorInfo {
        matrix: MatrixCoefficients::Identity,
        ..ColorInfo::default()
    };
    if info.srgb.is_some() {
        color.primaries = ColorPrimaries::Bt709;
        color.transfer = TransferFunction::Srgb;
    } else {
        if let Some(gamma) = info.gama_chunk.or(info.source_gamma) {
            // gAMA stores the encoding exponent (1/2.2 = 0.45455).
            let encode = gamma.into_value();
            if encode > 0.0 {
                let decode = 1.0 / encode;
                color.transfer = if (decode - 1.0).abs() < 0.01 {
                    TransferFunction::Linear
                } else {
                    TransferFunction::Gamma((decode * 1000.0).round() / 1000.0)
                };
            }
        }
        if let Some(c) = info.chrm_chunk.or(info.source_chromaticities) {
            let xy = |p: (png::ScaledFloat, png::ScaledFloat)| [p.0.into_value(), p.1.into_value()];
            color.primaries = ColorPrimaries::from_chromaticities(Chromaticities {
                red: xy(c.red),
                green: xy(c.green),
                blue: xy(c.blue),
                white: xy(c.white),
            });
        }
    }
    if let Some(icc) = &info.icc_profile {
        color.icc_profile = Some(Arc::from(&icc[..]));
    }
    color
}

fn decode_png(path: &Path) -> Result<Decoded, MediaError> {
    let mut decoder = png::Decoder::new(open_file(path)?);
    // Palette → RGB, tRNS → alpha, 1/2/4-bit → 8-bit; 16-bit stays 16-bit.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| MediaError::decode(path, e))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| MediaError::decode(path, "image too large"))?;
    let mut buf = vec![0; size];
    let out = reader
        .next_frame(&mut buf)
        .map_err(|e| MediaError::decode(path, e))?;
    buf.truncate(out.buffer_size());
    let channels = out.color_type.samples();
    let pixels = out.width as usize * out.height as usize;
    let (format, data) = match out.bit_depth {
        png::BitDepth::Sixteen => {
            // Big-endian samples → native-endian RGBA16.
            let mut data = Vec::with_capacity(pixels * 8);
            for px in buf.chunks_exact(channels * 2) {
                let s = |i: usize| u16::from_be_bytes([px[2 * i], px[2 * i + 1]]);
                let rgba = expand_to_rgba(channels, s, u16::MAX);
                for v in rgba {
                    data.extend_from_slice(&v.to_ne_bytes());
                }
            }
            (PixelFormat::Rgba16, data)
        }
        _ => {
            let data = if channels == 4 {
                buf
            } else {
                let mut data = Vec::with_capacity(pixels * 4);
                for px in buf.chunks_exact(channels) {
                    data.extend_from_slice(&expand_to_rgba(channels, |i| px[i], u8::MAX));
                }
                data
            };
            (PixelFormat::Rgba8, data)
        }
    };
    Ok((out.width, out.height, format, data))
}

/// Expands gray / gray+alpha / RGB / RGBA samples to RGBA.
fn expand_to_rgba<T: Copy>(channels: usize, sample: impl Fn(usize) -> T, opaque: T) -> [T; 4] {
    match channels {
        1 => {
            let v = sample(0);
            [v, v, v, opaque]
        }
        2 => {
            let v = sample(0);
            [v, v, v, sample(1)]
        }
        3 => [sample(0), sample(1), sample(2), opaque],
        _ => [sample(0), sample(1), sample(2), sample(3)],
    }
}

// ---- JPEG ------------------------------------------------------------------

/// Baseline and progressive JPEG (gray, YCbCr, CMYK), ICC profile.
#[derive(Debug, Default, Clone, Copy)]
pub struct JpegFormat;

impl MediaFormat for JpegFormat {
    fn id(&self) -> &str {
        "jpeg"
    }

    fn extensions(&self) -> &[&str] {
        &["jpg", "jpeg", "jpe", "jfif"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        header.starts_with(&[0xFF, 0xD8, 0xFF])
    }

    fn is_image(&self) -> bool {
        true
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        let mut decoder = zune_jpeg::JpegDecoder::new(open_file(path)?);
        decoder
            .decode_headers()
            .map_err(|e| MediaError::decode(path, format!("{e:?}")))?;
        let info = decoder
            .info()
            .ok_or_else(|| MediaError::decode(path, "no JPEG header"))?;
        let mut video = still_video(
            u32::from(info.width),
            u32::from(info.height),
            8,
            SampleType::UInt,
            "jpeg",
        );
        video.color.matrix = MatrixCoefficients::Identity;
        video.color.icc_profile = decoder.icc_profile().map(Arc::from);
        // JFIF density unit 0 means the densities are only an aspect ratio.
        if info.pixel_density == 0 && info.x_density > 0 && info.y_density > 0 {
            video.pixel_aspect =
                PixelAspect::new(u32::from(info.y_density), u32::from(info.x_density));
        }
        Ok(Box::new(StillSource::new(
            path,
            "jpeg",
            StillCodec::Jpeg,
            video,
        )))
    }
}

fn decode_jpeg(path: &Path) -> Result<Decoded, MediaError> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;

    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(1 << 16)
        .set_max_height(1 << 16);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(open_file(path)?, options);
    let data = decoder
        .decode()
        .map_err(|e| MediaError::decode(path, format!("{e:?}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| MediaError::decode(path, "no JPEG header"))?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if data.len() != width as usize * height as usize * 4 {
        return Err(MediaError::decode(path, "unexpected JPEG output size"));
    }
    Ok((width, height, PixelFormat::Rgba8, data))
}

// ---- WebP ------------------------------------------------------------------

/// Lossy and lossless WebP with alpha (animated files show their first frame).
#[derive(Debug, Default, Clone, Copy)]
pub struct WebpFormat;

impl MediaFormat for WebpFormat {
    fn id(&self) -> &str {
        "webp"
    }

    fn extensions(&self) -> &[&str] {
        &["webp"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        header.len() >= 12 && &header[..4] == b"RIFF" && &header[8..12] == b"WEBP"
    }

    fn is_image(&self) -> bool {
        true
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        use image::ImageDecoder as _;

        let mut decoder = image::codecs::webp::WebPDecoder::new(open_file(path)?)
            .map_err(|e| MediaError::decode(path, e))?;
        let (width, height) = decoder.dimensions();
        let mut video = still_video(width, height, 8, SampleType::UInt, "webp");
        video.alpha = if decoder.color_type().has_alpha() {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        };
        video.color.matrix = MatrixCoefficients::Identity;
        video.color.icc_profile = decoder.icc_profile().ok().flatten().map(Arc::from);
        Ok(Box::new(StillSource::new(
            path,
            "webp",
            StillCodec::WebP,
            video,
        )))
    }
}

fn decode_webp(path: &Path) -> Result<Decoded, MediaError> {
    use image::ImageDecoder as _;

    let decoder = image::codecs::webp::WebPDecoder::new(open_file(path)?)
        .map_err(|e| MediaError::decode(path, e))?;
    let (width, height) = decoder.dimensions();
    let channels = usize::from(decoder.color_type().channel_count());
    let mut buf = vec![0; decoder.total_bytes() as usize];
    decoder
        .read_image(&mut buf)
        .map_err(|e| MediaError::decode(path, e))?;
    let data = if channels == 4 {
        buf
    } else {
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for px in buf.chunks_exact(channels) {
            data.extend_from_slice(&expand_to_rgba(channels, |i| px[i], u8::MAX));
        }
        data
    };
    Ok((width, height, PixelFormat::Rgba8, data))
}

// ---- TIFF ------------------------------------------------------------------

/// Baseline TIFF (8 / 16-bit integer and 32-bit float, gray / RGB, with or
/// without alpha; uncompressed, LZW, Deflate, PackBits, JPEG); the first image
/// of multi-page files.
///
/// Alpha: an extra sample after gray or RGB is alpha. `ExtraSamples` says how:
/// associated (1) = premultiplied, unassociated (2) = straight. AE's "TIFF
/// Sequence with Alpha" writes premultiplied pixels tagged *unspecified* (0)
/// (checked against AE 2026 output), so an explicit 0 reads as premultiplied;
/// a missing tag (writers that omit it) reads as straight.
#[derive(Debug, Default, Clone, Copy)]
pub struct TiffFormat;

/// What the header of a TIFF says about its pixels.
struct TiffLayout {
    width: u32,
    height: u32,
    /// Samples per pixel (1 gray, 2 gray + alpha, 3 RGB, 4 RGBA).
    samples: usize,
    bits: u8,
    float: bool,
    alpha: AlphaMode,
    /// The alpha sample is tagged "unspecified", which the `tiff` decoder
    /// drops; decoding works on a copy tagged "associated".
    unspecified_alpha: bool,
    icc: Option<Vec<u8>>,
}

fn tiff_layout<R: std::io::Read + std::io::Seek>(
    decoder: &mut tiff::decoder::Decoder<R>,
    path: &Path,
) -> Result<TiffLayout, MediaError> {
    use tiff::tags::Tag;

    let err = |e: tiff::TiffError| MediaError::decode(path, e);
    let (width, height) = decoder.dimensions().map_err(err)?;
    let photometric: u16 = decoder
        .find_tag_unsigned(Tag::PhotometricInterpretation)
        .map_err(err)?
        .unwrap_or(2);
    let samples: usize = decoder
        .find_tag_unsigned(Tag::SamplesPerPixel)
        .map_err(err)?
        .unwrap_or(1);
    let bits: u16 = decoder
        .find_tag_unsigned_vec::<u16>(Tag::BitsPerSample)
        .map_err(err)?
        .and_then(|v| v.first().copied())
        .unwrap_or(1);
    let float = decoder
        .find_tag_unsigned_vec::<u16>(Tag::SampleFormat)
        .map_err(err)?
        .is_some_and(|v| v.first() == Some(&3));
    let color_samples = match photometric {
        0 | 1 => 1, // WhiteIsZero, BlackIsZero
        2 => 3,     // RGB
        other => {
            return Err(MediaError::Unsupported(format!(
                "TIFF photometric interpretation {other} (CMYK, YCbCr, palette …)"
            )));
        }
    };
    if !(color_samples..=color_samples + 1).contains(&samples) {
        return Err(MediaError::Unsupported(format!(
            "TIFF with {samples} samples per pixel"
        )));
    }
    let extra = decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .map_err(err)?
        .and_then(|v| v.first().copied());
    let alpha = if samples > color_samples {
        match extra {
            // Explicitly unspecified (AE) or associated.
            Some(0 | 1) => AlphaMode::Premultiplied,
            // Unassociated, or no tag at all (generic writers that omit it
            // write straight RGBA).
            _ => AlphaMode::Straight,
        }
    } else {
        AlphaMode::Opaque
    };
    let icc = decoder.get_tag_u8_vec(Tag::IccProfile).ok();
    Ok(TiffLayout {
        width,
        height,
        samples,
        bits: bits.min(32) as u8,
        float,
        alpha,
        unspecified_alpha: samples > color_samples && extra == Some(0),
        icc,
    })
}

/// Rewrites a single, inline `ExtraSamples` value of 0 (unspecified) to 1
/// (associated) in the first IFD of a classic TIFF held in memory. Returns
/// whether it did.
fn tag_unspecified_alpha_as_associated(bytes: &mut [u8]) -> bool {
    let big_endian = match bytes.get(..2) {
        Some(b"II") => false,
        Some(b"MM") => true,
        _ => return false,
    };
    let u16_at = |b: &[u8], at: usize| -> Option<u16> {
        let v: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
        Some(if big_endian {
            u16::from_be_bytes(v)
        } else {
            u16::from_le_bytes(v)
        })
    };
    let u32_at = |b: &[u8], at: usize| -> Option<u32> {
        let v: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
        Some(if big_endian {
            u32::from_be_bytes(v)
        } else {
            u32::from_le_bytes(v)
        })
    };
    let Some(ifd) = (u16_at(bytes, 2) == Some(42))
        .then(|| u32_at(bytes, 4))
        .flatten()
        .map(|o| o as usize)
    else {
        return false;
    };
    let Some(count) = u16_at(bytes, ifd) else {
        return false;
    };
    for i in 0..usize::from(count) {
        let entry = ifd + 2 + i * 12;
        // Tag 338, type SHORT (3), one value, inline value 0.
        if u16_at(bytes, entry) == Some(338)
            && u16_at(bytes, entry + 2) == Some(3)
            && u32_at(bytes, entry + 4) == Some(1)
            && u16_at(bytes, entry + 8) == Some(0)
        {
            let one = if big_endian {
                1_u16.to_be_bytes()
            } else {
                1_u16.to_le_bytes()
            };
            bytes[entry + 8..entry + 10].copy_from_slice(&one);
            return true;
        }
    }
    false
}

fn tiff_decoder(path: &Path) -> Result<tiff::decoder::Decoder<BufReader<File>>, MediaError> {
    tiff::decoder::Decoder::new(open_file(path)?).map_err(|e| MediaError::decode(path, e))
}

impl MediaFormat for TiffFormat {
    fn id(&self) -> &str {
        "tiff"
    }

    fn extensions(&self) -> &[&str] {
        &["tif", "tiff"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        header.starts_with(b"II*\0") || header.starts_with(b"MM\0*")
    }

    fn is_image(&self) -> bool {
        true
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        let mut decoder = tiff_decoder(path)?;
        let layout = tiff_layout(&mut decoder, path)?;
        let sample_type = if layout.float {
            SampleType::Float
        } else {
            SampleType::UInt
        };
        let mut video = still_video(
            layout.width,
            layout.height,
            layout.bits,
            sample_type,
            "tiff",
        );
        video.alpha = layout.alpha;
        video.color.matrix = MatrixCoefficients::Identity;
        if layout.float {
            video.color.transfer = TransferFunction::Linear;
        }
        video.color.icc_profile = layout.icc.map(Arc::from);
        Ok(Box::new(StillSource::new(
            path,
            "tiff",
            StillCodec::Tiff,
            video,
        )))
    }
}

fn decode_tiff(path: &Path) -> Result<Decoded, MediaError> {
    use tiff::decoder::DecodingResult;

    let mut decoder = tiff_decoder(path)?;
    let layout = tiff_layout(&mut decoder, path)?;
    let image = if layout.unspecified_alpha {
        let mut bytes = std::fs::read(path).map_err(|e| MediaError::io(path, e))?;
        if !tag_unspecified_alpha_as_associated(&mut bytes) {
            return Err(MediaError::Unsupported(
                "TIFF alpha tagged unspecified in an unusual layout".into(),
            ));
        }
        tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).and_then(|mut d| d.read_image())
    } else {
        decoder.read_image()
    }
    .map_err(|e| MediaError::decode(path, e))?;
    let (width, height, channels) = (layout.width, layout.height, layout.samples);
    let pixels = width as usize * height as usize;
    let check = |len: usize| {
        if len < pixels * channels {
            Err(MediaError::decode(path, "TIFF image data is short"))
        } else {
            Ok(())
        }
    };
    let (format, data) = match image {
        DecodingResult::U8(buf) => {
            check(buf.len())?;
            let mut data = Vec::with_capacity(pixels * 4);
            for px in buf.chunks_exact(channels).take(pixels) {
                data.extend_from_slice(&expand_to_rgba(channels, |i| px[i], u8::MAX));
            }
            (PixelFormat::Rgba8, data)
        }
        DecodingResult::U16(buf) => {
            check(buf.len())?;
            let mut data = Vec::with_capacity(pixels * 8);
            for px in buf.chunks_exact(channels).take(pixels) {
                for v in expand_to_rgba(channels, |i| px[i], u16::MAX) {
                    data.extend_from_slice(&v.to_ne_bytes());
                }
            }
            (PixelFormat::Rgba16, data)
        }
        DecodingResult::F32(buf) => {
            check(buf.len())?;
            let mut data = Vec::with_capacity(pixels * 16);
            for px in buf.chunks_exact(channels).take(pixels) {
                for v in expand_to_rgba(channels, |i| px[i], 1.0) {
                    data.extend_from_slice(&v.to_ne_bytes());
                }
            }
            (PixelFormat::Rgba32Float, data)
        }
        DecodingResult::F16(buf) => {
            check(buf.len())?;
            let mut data = Vec::with_capacity(pixels * 8);
            for px in buf.chunks_exact(channels).take(pixels) {
                for v in expand_to_rgba(channels, |i| px[i], f16::ONE) {
                    data.extend_from_slice(&v.to_ne_bytes());
                }
            }
            (PixelFormat::Rgba16Float, data)
        }
        _ => {
            return Err(MediaError::Unsupported(format!(
                "{}-bit TIFF samples",
                layout.bits
            )));
        }
    };
    Ok((width, height, format, data))
}

// ---- OpenEXR ---------------------------------------------------------------

/// OpenEXR scanline / tiled, all compressions, half and float RGB(A) in the
/// first layer. Decoded to half floats (the GPU working format); the data window
/// is placed inside the display window. Alpha is premultiplied (EXR
/// convention), color is scene-linear.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExrFormat;

impl MediaFormat for ExrFormat {
    fn id(&self) -> &str {
        "exr"
    }

    fn extensions(&self) -> &[&str] {
        &["exr"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        header.starts_with(&[0x76, 0x2f, 0x31, 0x01])
    }

    fn is_image(&self) -> bool {
        true
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        let meta = exr::meta::MetaData::read_from_file(path, false)
            .map_err(|e| MediaError::decode(path, e))?;
        let header = meta
            .headers
            .iter()
            .find(|h| !h.deep && has_rgb(h))
            .ok_or_else(|| MediaError::decode(path, "no RGB layer"))?;
        let attrs = &header.shared_attributes;
        let display = attrs.display_window;
        let float = header
            .channels
            .list
            .iter()
            .any(|c| c.sample_type != exr::meta::attribute::SampleType::F16);
        let mut video = still_video(
            display.size.0 as u32,
            display.size.1 as u32,
            if float { 32 } else { 16 },
            if float {
                SampleType::Float
            } else {
                SampleType::Half
            },
            &format!("exr/{}", format!("{:?}", header.compression).to_lowercase()),
        );
        video.alpha = if header.channels.list.iter().any(|c| channel_is(c, "A")) {
            AlphaMode::Premultiplied
        } else {
            AlphaMode::Opaque
        };
        video.color = ColorInfo::linear_bt709();
        if let Some(c) = attrs.chromaticities {
            let xy = |v: exr::math::Vec2<f32>| [v.0, v.1];
            video.color.primaries = ColorPrimaries::from_chromaticities(Chromaticities {
                red: xy(c.red),
                green: xy(c.green),
                blue: xy(c.blue),
                white: xy(c.white),
            });
        }
        video.pixel_aspect = PixelAspect::from_f32(attrs.pixel_aspect);
        let mut source = StillSource::new(path, "exr", StillCodec::Exr, video);
        source.info.timecode = attrs.time_code.map(|tc| Timecode {
            hours: tc.hours,
            minutes: tc.minutes,
            seconds: tc.seconds,
            frames: tc.frame,
            drop_frame: tc.drop_frame,
        });
        Ok(Box::new(source))
    }
}

fn channel_is(channel: &exr::meta::attribute::ChannelDescription, name: &str) -> bool {
    channel.name.eq_case_insensitive(name)
}

fn has_rgb(header: &exr::meta::header::Header) -> bool {
    ["R", "G", "B"]
        .iter()
        .all(|name| header.channels.list.iter().any(|c| channel_is(c, name)))
}

struct ExrPixels {
    width: usize,
    data: Vec<f16>,
}

fn decode_exr(path: &Path) -> Result<Decoded, MediaError> {
    let image = exr::prelude::read_first_rgba_layer_from_file(
        path,
        |size, _channels| ExrPixels {
            width: size.0,
            data: vec![f16::ZERO; size.0 * size.1 * 4],
        },
        |pixels: &mut ExrPixels, pos, (r, g, b, a): (f16, f16, f16, f16)| {
            let i = (pos.1 * pixels.width + pos.0) * 4;
            pixels.data[i..i + 4].copy_from_slice(&[r, g, b, a]);
        },
    )
    .map_err(|e| MediaError::decode(path, e))?;
    let display = image.attributes.display_window;
    let layer = &image.layer_data;
    let data_pos = layer.attributes.layer_position;
    let (data_w, data_h) = (layer.size.0, layer.size.1);
    let pixels = &layer.channel_data.pixels.data;
    let (width, height) = (display.size.0, display.size.1);
    let out = if display.position == data_pos && (width, height) == (data_w, data_h) {
        bytemuck::cast_slice::<f16, u8>(pixels).to_vec()
    } else {
        // Place the data window inside the display window; outside is transparent.
        let mut out = vec![f16::ZERO; width * height * 4];
        let dx = i64::from(data_pos.0) - i64::from(display.position.0);
        let dy = i64::from(data_pos.1) - i64::from(display.position.1);
        for y in 0..data_h {
            let ty = y as i64 + dy;
            if ty < 0 || ty >= height as i64 {
                continue;
            }
            let x0 = dx.max(0);
            let x1 = (dx + data_w as i64).min(width as i64);
            if x0 >= x1 {
                continue;
            }
            let src_x = (x0 - dx) as usize;
            let len = (x1 - x0) as usize * 4;
            let src = (y * data_w + src_x) * 4;
            let dst = (ty as usize * width + x0 as usize) * 4;
            out[dst..dst + len].copy_from_slice(&pixels[src..src + len]);
        }
        bytemuck::cast_slice::<f16, u8>(&out).to_vec()
    };
    Ok((width as u32, height as u32, PixelFormat::Rgba16Float, out))
}

/// The still-image formats of this module.
pub(crate) fn formats() -> Vec<Arc<dyn MediaFormat>> {
    vec![
        Arc::new(PngFormat),
        Arc::new(JpegFormat),
        Arc::new(WebpFormat),
        Arc::new(TiffFormat),
        Arc::new(ExrFormat),
    ]
}

/// Opens `path` with `format` and decodes its picture (convenience for tests and
/// thumbnails).
pub fn decode_still(format: &dyn MediaFormat, path: &Path) -> Result<CpuFrame, MediaError> {
    format.open(path)?.decode_cpu(&FrameRequest::new(0))
}
