//! Encoding and export: encoders, the render loop and the render queue's
//! runtime state.
//!
//! * [`EncodeSettings`] says what to write (AE's Output Module): codec,
//!   size, rate, alpha ("RGB" / "RGB + Alpha", straight or premultiplied), bit
//!   depth, color tags.
//! * An [`Encoder`] takes frames in order. [`EncoderFormat`]s (built-ins and
//!   plugins) are registered in `Registry<dyn EncoderFormat>`; [`MediaEncoders`]
//!   picks one for a settings value.
//! * [`render`] drives one [`RenderJob`]: it asks a [`FrameProducer`] (the
//!   compositor, workstream 04) for each frame, reads GPU frames back with
//!   [`FrameReadback`] (asynchronous, several in flight, ARCHITECTURE §5) and
//!   feeds the encoder, reporting [`RenderProgress`] and honoring a
//!   [`CancelToken`].
//! * [`RenderQueue`] is the runtime list of jobs with their [`RenderStatus`].
//!   Which jobs a *project* contains is project data and belongs to the document
//!   model (changed through commands); this queue only executes them.
//!
//! Built-in encoders here: PNG and OpenEXR sequences ([`ImageSequenceEncoder`]).
//! Movie encoders (VideoToolbox / AVFoundation, Media Foundation) live in the
//! platform modules.

use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bae_gpu::transfer::{PendingReadback, TextureReadback};
use half::f16;
use serde::{Deserialize, Serialize};

use crate::audio::AudioBuffer;
use crate::frame::{CpuFrame, GpuFrame, PixelFormat};
use crate::gpu::{ExportTexture, MediaGpu};
use crate::meta::{
    AlphaMode, ColorInfo, ColorPrimaries, MatrixCoefficients, PixelAspect, TransferFunction,
};
use crate::sequence::parse_numbered_name;
use crate::source::MediaError;
use crate::time::{FrameRate, Timecode};

// ---- settings --------------------------------------------------------------

/// Apple ProRes profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProResProfile {
    /// ProRes 422 Proxy.
    Proxy,
    /// ProRes 422 LT.
    Lt,
    /// ProRes 422.
    Standard,
    /// ProRes 422 HQ.
    Hq,
    /// ProRes 4444 (alpha capable).
    P4444,
    /// ProRes 4444 XQ (alpha capable).
    P4444Xq,
}

impl ProResProfile {
    /// Whether the profile can carry an alpha channel.
    pub fn supports_alpha(self) -> bool {
        matches!(self, Self::P4444 | Self::P4444Xq)
    }
}

/// OpenEXR compression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExrCompression {
    /// No compression.
    None,
    /// Run length.
    Rle,
    /// Zlib, one scan line per block.
    Zips,
    /// Zlib, 16 scan lines per block (AE's default).
    #[default]
    Zip,
    /// Wavelet (good for grainy images).
    Piz,
}

/// What an export is encoded with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VideoCodec {
    /// H.264 / AVC (hardware encoder). No alpha.
    H264,
    /// H.265 / HEVC (hardware encoder). Alpha on macOS ("HEVC with alpha").
    Hevc,
    /// Apple ProRes.
    ProRes(ProResProfile),
    /// PNG image sequence (8 or 16 bit).
    Png,
    /// OpenEXR image sequence (half or float).
    Exr(ExrCompression),
}

impl VideoCodec {
    /// `true` for image-sequence codecs (one file per frame).
    pub fn is_image_sequence(self) -> bool {
        matches!(self, Self::Png | Self::Exr(_))
    }

    /// Whether the codec can carry alpha.
    pub fn supports_alpha(self) -> bool {
        match self {
            Self::H264 => false,
            Self::Hevc | Self::Png | Self::Exr(_) => true,
            Self::ProRes(profile) => profile.supports_alpha(),
        }
    }
}

/// Audio in an export (reserved: muxing audio lands with the audio PR).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioEncodeSettings {
    /// Samples per second (48 000 by default).
    pub sample_rate: u32,
    /// Channels.
    pub channels: u16,
    /// Codec name (`"aac"`, `"pcm_s24le"` …).
    pub codec: String,
}

/// Everything an encoder needs to know (AE's Output Module settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EncodeSettings {
    /// Output file. For image sequences a pattern: `out_[####].png` (AE style),
    /// `out_####.png`, or a numbered name `out_0000.png` (its digits give the
    /// padding); without any, `_#####` is inserted before the extension.
    pub path: PathBuf,
    /// Codec (implies the container: `.mov` / `.mp4` for movies).
    pub codec: VideoCodec,
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Pixel aspect ratio written to the file.
    pub pixel_aspect: PixelAspect,
    /// Frame rate.
    pub frame_rate: FrameRate,
    /// Channels and alpha: [`AlphaMode::Opaque`] = RGB (premultiplied over
    /// black, like AE), [`AlphaMode::Straight`] / [`AlphaMode::Premultiplied`]
    /// = RGB + Alpha.
    pub alpha: AlphaMode,
    /// Bits per channel: PNG 8 / 16, EXR 16 (half) / 32 (float), HEVC 8 / 10.
    /// Ignored where the codec fixes it (ProRes, H.264).
    pub bit_depth: u8,
    /// Color tags written to the file (pixel values are not converted; the
    /// renderer delivers them in this encoding).
    pub color: ColorInfo,
    /// Quality 0–1 for quality-driven encoders; `None` = encoder default.
    pub quality: Option<f32>,
    /// Average bit rate in bits per second; `None` = encoder default.
    pub bit_rate: Option<u64>,
    /// Maximum distance between keyframes; `None` = encoder default.
    pub keyframe_interval: Option<u32>,
    /// Number of the first file of an image sequence (AE default 0).
    pub start_number: i64,
    /// Start timecode written to movies / EXR headers.
    pub start_timecode: Option<Timecode>,
    /// Audio (reserved).
    pub audio: Option<AudioEncodeSettings>,
}

impl EncodeSettings {
    /// Settings with defaults: opaque, 8 bit (16 for EXR); tags sRGB (PNG),
    /// linear BT.709 (EXR), or AE's video tags (BT.709, or BT.601 below 720
    /// lines).
    pub fn new(
        path: impl Into<PathBuf>,
        codec: VideoCodec,
        width: u32,
        height: u32,
        frame_rate: FrameRate,
    ) -> Self {
        let color = match codec {
            VideoCodec::Exr(_) => ColorInfo::linear_bt709(),
            VideoCodec::Png => ColorInfo::srgb(),
            // Same tags as AE's H.264 / QuickTime output (checked against AE
            // 2026 renders): BT.709 for HD and larger, SMPTE-C primaries with
            // the BT.601 matrix below 720 lines; BT.709 transfer either way.
            _ if height < 720 => ColorInfo {
                primaries: ColorPrimaries::Bt601Ntsc,
                transfer: TransferFunction::Bt709,
                matrix: MatrixCoefficients::Bt601,
                ..ColorInfo::srgb()
            },
            _ => ColorInfo {
                transfer: TransferFunction::Bt709,
                matrix: MatrixCoefficients::Bt709,
                ..ColorInfo::srgb()
            },
        };
        EncodeSettings {
            path: path.into(),
            codec,
            width,
            height,
            pixel_aspect: PixelAspect::SQUARE,
            frame_rate,
            alpha: AlphaMode::Opaque,
            bit_depth: if matches!(codec, VideoCodec::Exr(_)) {
                16
            } else {
                8
            },
            color,
            quality: None,
            bit_rate: None,
            keyframe_interval: None,
            start_number: 0,
            start_timecode: None,
            audio: None,
        }
    }

    /// Same settings with alpha.
    pub fn with_alpha(mut self, alpha: AlphaMode) -> Self {
        self.alpha = alpha;
        self
    }

    /// `true` if the output carries an alpha channel.
    pub fn has_alpha(&self) -> bool {
        self.alpha.has_alpha()
    }

    /// Checks the combination before any file is created.
    pub fn validate(&self) -> Result<(), MediaError> {
        let bad = |message: String| Err(MediaError::Unsupported(message));
        if self.width == 0 || self.height == 0 {
            return bad(format!("{}×{} output", self.width, self.height));
        }
        if self.frame_rate.num == 0 || self.frame_rate.den == 0 {
            return bad(format!(
                "frame rate {}/{}",
                self.frame_rate.num, self.frame_rate.den
            ));
        }
        if self.has_alpha() && !self.codec.supports_alpha() {
            return bad(format!("{:?} cannot carry alpha", self.codec));
        }
        let depth_ok = match self.codec {
            VideoCodec::Png => matches!(self.bit_depth, 8 | 16),
            VideoCodec::Exr(_) => matches!(self.bit_depth, 16 | 32),
            VideoCodec::Hevc => matches!(self.bit_depth, 8 | 10),
            VideoCodec::H264 | VideoCodec::ProRes(_) => true,
        };
        if !depth_ok {
            return bad(format!("{} bit {:?}", self.bit_depth, self.codec));
        }
        // 4:2:0 / 4:2:2 chroma subsampling needs even sizes.
        if matches!(
            self.codec,
            VideoCodec::H264 | VideoCodec::Hevc | VideoCodec::ProRes(_)
        ) && (!self.width.is_multiple_of(2) || !self.height.is_multiple_of(2))
        {
            return bad(format!(
                "{}×{} (video codecs need even sizes)",
                self.width, self.height
            ));
        }
        Ok(())
    }
}

/// What an encoder wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodeOutput {
    /// The files (one for a movie, one per frame for a sequence).
    pub files: Vec<PathBuf>,
    /// Frames written.
    pub frames: u64,
}

// ---- encoder traits -------------------------------------------------------

/// Writes frames, in order, to a file or an image sequence.
pub trait Encoder: Send {
    /// The settings the encoder was created with.
    fn settings(&self) -> &EncodeSettings;

    /// Appends the next frame. The frame must have the output size; any
    /// [`PixelFormat`] and alpha mode is accepted and converted.
    fn push_frame(&mut self, frame: &CpuFrame) -> Result<(), MediaError>;

    /// Appends audio (reserved).
    fn push_audio(&mut self, audio: &AudioBuffer) -> Result<(), MediaError> {
        let _ = audio;
        Err(MediaError::Unsupported("audio in this encoder".into()))
    }

    /// Frames written so far.
    fn frames_written(&self) -> u64;

    /// The pixel layout and alpha the encoder works in. The render loop
    /// converts GPU frames into it before readback
    /// ([`MediaGpu::convert_for_export`]), so `push_frame` receives frames it
    /// can use without per-pixel work. `None`: no preference.
    fn input_format(&self) -> Option<(PixelFormat, AlphaMode)> {
        None
    }

    /// Flushes and closes the output.
    fn finish(self: Box<Self>) -> Result<EncodeOutput, MediaError>;

    /// Stops and removes what was written (cancelled render).
    fn abort(self: Box<Self>);
}

/// An encoder implementation, registered under an id.
pub trait EncoderFormat: Send + Sync {
    /// Stable id (`"png-sequence"`, `"avfoundation"` …).
    fn id(&self) -> &str;

    /// Whether this implementation can write `settings`.
    fn supports(&self, settings: &EncodeSettings) -> bool;

    /// Creates the encoder (creates or truncates the output).
    fn create(&self, settings: &EncodeSettings) -> Result<Box<dyn Encoder>, MediaError>;

    /// Selection priority when several formats support the settings (higher
    /// wins).
    fn priority(&self) -> i32 {
        0
    }
}

impl fmt::Debug for dyn EncoderFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncoderFormat")
            .field("id", &self.id())
            .finish_non_exhaustive()
    }
}

/// A set of encoder formats.
#[derive(Clone, Default)]
pub struct MediaEncoders {
    formats: Vec<Arc<dyn EncoderFormat>>,
}

impl fmt::Debug for MediaEncoders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.formats.iter().map(|format| format.id()))
            .finish()
    }
}

impl MediaEncoders {
    /// The encoders compiled into this crate for the current platform.
    pub fn builtin() -> Self {
        Self::from_formats(crate::builtin_encoders())
    }

    /// A set from explicit formats.
    pub fn from_formats(formats: impl IntoIterator<Item = Arc<dyn EncoderFormat>>) -> Self {
        let mut formats: Vec<_> = formats.into_iter().collect();
        formats.sort_by_key(|format| std::cmp::Reverse(format.priority()));
        MediaEncoders { formats }
    }

    /// A snapshot of the app's encoder registry (built-ins plus plugins).
    pub fn from_registry(registry: &bae_app::Registry<dyn EncoderFormat>) -> Self {
        Self::from_formats(registry.snapshot().iter().map(|entry| entry.item().clone()))
    }

    /// The formats, highest priority first.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn EncoderFormat>> + '_ {
        self.formats.iter()
    }

    /// Validates `settings` and creates an encoder for them.
    pub fn create(&self, settings: &EncodeSettings) -> Result<Box<dyn Encoder>, MediaError> {
        settings.validate()?;
        let format = self
            .formats
            .iter()
            .find(|format| format.supports(settings))
            .ok_or_else(|| {
                MediaError::Unsupported(format!("no encoder for {:?}", settings.codec))
            })?;
        format.create(settings)
    }
}

// ---- pixel conversion ------------------------------------------------------

/// Converts `frame` to `format` with alpha `alpha` (encoders and plugin CPU
/// nodes). Converting to [`AlphaMode::Opaque`] keeps the color premultiplied
/// over black and sets alpha to 1 (AE's "RGB" output of a comp with
/// transparency). Integer formats are rounded and clamped to `0..=1`.
pub fn convert_frame(frame: &CpuFrame, format: PixelFormat, alpha: AlphaMode) -> CpuFrame {
    convert_frame_cow(frame, format, alpha).into_owned()
}

/// Like [`convert_frame`], but borrows `frame` when it already has the layout
/// and alpha (the normal case for frames converted on the GPU).
pub fn convert_frame_cow(
    frame: &CpuFrame,
    format: PixelFormat,
    alpha: AlphaMode,
) -> std::borrow::Cow<'_, CpuFrame> {
    let from = frame.alpha;
    let same_alpha = from == alpha
        || matches!(
            (from, alpha),
            (AlphaMode::Opaque | AlphaMode::Ignore, AlphaMode::Opaque)
        );
    if frame.format == format && same_alpha {
        return std::borrow::Cow::Borrowed(frame);
    }
    std::borrow::Cow::Owned(convert_pixels(frame, format, alpha))
}

fn convert_pixels(frame: &CpuFrame, format: PixelFormat, alpha: AlphaMode) -> CpuFrame {
    let from = frame.alpha;
    let pixels = frame.width as usize * frame.height as usize;
    let mut data = Vec::with_capacity(pixels * format.bytes_per_pixel() as usize);
    for y in 0..frame.height {
        for x in 0..frame.width {
            let [r, g, b, a] = frame.pixel_f32(x, y);
            let a = if matches!(from, AlphaMode::Opaque | AlphaMode::Ignore) {
                1.0
            } else {
                a
            };
            let (rgb, a) = match (from, alpha) {
                (AlphaMode::Straight, AlphaMode::Premultiplied | AlphaMode::Opaque) => {
                    ([r * a, g * a, b * a], a)
                }
                (AlphaMode::Premultiplied, AlphaMode::Straight) if a > 0.0 => {
                    ([r / a, g / a, b / a], a)
                }
                _ => ([r, g, b], a),
            };
            let a = if matches!(alpha, AlphaMode::Opaque | AlphaMode::Ignore) {
                1.0
            } else {
                a
            };
            write_pixel(&mut data, format, [rgb[0], rgb[1], rgb[2], a]);
        }
    }
    CpuFrame {
        width: frame.width,
        height: frame.height,
        format,
        data,
        alpha: if alpha == AlphaMode::Ignore {
            AlphaMode::Opaque
        } else {
            alpha
        },
        color: frame.color.clone(),
        pixel_aspect: frame.pixel_aspect,
    }
}

fn write_pixel(out: &mut Vec<u8>, format: PixelFormat, rgba: [f32; 4]) {
    let unorm8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let unorm16 = |v: f32| (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
    match format {
        PixelFormat::Rgba8 => out.extend(rgba.map(unorm8)),
        PixelFormat::Bgra8 => out.extend([rgba[2], rgba[1], rgba[0], rgba[3]].map(unorm8)),
        PixelFormat::Rgba16 => {
            for v in rgba {
                out.extend_from_slice(&unorm16(v).to_ne_bytes());
            }
        }
        PixelFormat::Rgba16Float => {
            for v in rgba {
                out.extend_from_slice(&f16::from_f32(v).to_ne_bytes());
            }
        }
        PixelFormat::Rgba32Float => {
            for v in rgba {
                out.extend_from_slice(&v.to_ne_bytes());
            }
        }
    }
}

fn check_frame_size(settings: &EncodeSettings, frame: &CpuFrame) -> Result<(), MediaError> {
    if (frame.width, frame.height) != (settings.width, settings.height)
        || frame.data.len() != frame.expected_len()
    {
        return Err(MediaError::Unsupported(format!(
            "{}×{} frame ({} bytes) for a {}×{} output",
            frame.width,
            frame.height,
            frame.data.len(),
            settings.width,
            settings.height
        )));
    }
    Ok(())
}

// ---- image sequences -------------------------------------------------------

/// File naming of an output image sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequencePattern {
    /// Directory.
    pub dir: PathBuf,
    /// Name part before the number.
    pub prefix: String,
    /// Name part after the number, including the extension.
    pub suffix: String,
    /// Zero-padded digits.
    pub padding: usize,
}

impl SequencePattern {
    /// Parses an output path (see [`EncodeSettings::path`]).
    pub fn parse(path: &Path) -> Self {
        let dir = path.parent().unwrap_or(Path::new("")).to_owned();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // `[####]` (AE) or a run of `#`.
        if let Some(start) = name.find('#') {
            let end = start + name[start..].chars().take_while(|&c| c == '#').count();
            let (mut prefix, mut suffix) = (name[..start].to_owned(), name[end..].to_owned());
            if prefix.ends_with('[') && suffix.starts_with(']') {
                prefix.pop();
                suffix.remove(0);
            }
            return SequencePattern {
                dir,
                prefix,
                suffix,
                padding: end - start,
            };
        }
        if let Some(numbered) = parse_numbered_name(&name) {
            return SequencePattern {
                dir,
                prefix: numbered.prefix,
                suffix: numbered.suffix,
                padding: numbered.digits,
            };
        }
        let (stem, ext) = match name.rfind('.') {
            Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
            _ => (name.as_str(), ""),
        };
        SequencePattern {
            dir,
            prefix: format!("{stem}_"),
            suffix: ext.to_owned(),
            padding: 5,
        }
    }

    /// Path of frame `number`.
    pub fn path_for(&self, number: i64) -> PathBuf {
        let digits = if number < 0 {
            format!("-{:0width$}", -number, width = self.padding)
        } else {
            format!("{number:0width$}", width = self.padding)
        };
        self.dir
            .join(format!("{}{digits}{}", self.prefix, self.suffix))
    }
}

/// PNG / OpenEXR sequence encoder (one file per frame).
///
/// Frames are compressed and written by a few worker threads (files are
/// independent), so exporting is not limited by one core; the queue is
/// bounded to keep memory in check at 4K. Errors surface on a later
/// `push_frame` or on `finish`.
pub struct ImageSequenceEncoder {
    settings: EncodeSettings,
    pattern: SequencePattern,
    files: Vec<PathBuf>,
    writer: Arc<SequenceWriter>,
    pool: Option<WriterPool>,
}

/// Writes one frame file (shared by the worker threads).
struct SequenceWriter {
    settings: EncodeSettings,
}

struct WriteJob {
    path: PathBuf,
    frame: CpuFrame,
    index: u64,
}

struct WriterPool {
    sender: Option<std::sync::mpsc::SyncSender<WriteJob>>,
    workers: Vec<std::thread::JoinHandle<()>>,
    error: Arc<parking_lot::Mutex<Option<MediaError>>>,
}

impl WriterPool {
    fn new(writer: &Arc<SequenceWriter>) -> Self {
        let threads = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4));
        let (sender, receiver) = std::sync::mpsc::sync_channel::<WriteJob>(threads);
        let receiver = Arc::new(parking_lot::Mutex::new(receiver));
        let error: Arc<parking_lot::Mutex<Option<MediaError>>> = Arc::default();
        let workers = (0..threads)
            .map(|_| {
                let (writer, receiver, error) = (writer.clone(), receiver.clone(), error.clone());
                std::thread::spawn(move || {
                    loop {
                        // Hold the lock only while taking a job.
                        let Ok(job) = receiver.lock().recv() else {
                            break;
                        };
                        if error.lock().is_some() {
                            continue;
                        }
                        if let Err(e) = writer.write(&job.path, &job.frame, job.index) {
                            error.lock().get_or_insert(e);
                        }
                    }
                })
            })
            .collect();
        WriterPool {
            sender: Some(sender),
            workers,
            error,
        }
    }

    /// Waits for every queued frame; returns the first error.
    fn join(mut self) -> Result<(), MediaError> {
        self.sender = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        self.error.lock().take().map_or(Ok(()), Err)
    }
}

impl ImageSequenceEncoder {
    /// Creates the encoder (and the output directory).
    pub fn new(settings: &EncodeSettings) -> Result<Self, MediaError> {
        settings.validate()?;
        if !settings.codec.is_image_sequence() {
            return Err(MediaError::Unsupported(format!(
                "{:?} is not an image-sequence codec",
                settings.codec
            )));
        }
        let pattern = SequencePattern::parse(&settings.path);
        if !pattern.dir.as_os_str().is_empty() {
            std::fs::create_dir_all(&pattern.dir).map_err(|e| MediaError::io(&pattern.dir, e))?;
        }
        let writer = Arc::new(SequenceWriter {
            settings: settings.clone(),
        });
        Ok(ImageSequenceEncoder {
            settings: settings.clone(),
            pattern,
            files: Vec::new(),
            pool: Some(WriterPool::new(&writer)),
            writer,
        })
    }
}

impl SequenceWriter {
    fn write(&self, path: &Path, frame: &CpuFrame, index: u64) -> Result<(), MediaError> {
        match self.settings.codec {
            VideoCodec::Png => self.write_png(path, frame),
            VideoCodec::Exr(compression) => self.write_exr(path, frame, compression, index),
            other => unreachable!("checked in ImageSequenceEncoder::new: {other:?}"),
        }
    }

    fn write_png(&self, path: &Path, frame: &CpuFrame) -> Result<(), MediaError> {
        let s = &self.settings;
        let sixteen = s.bit_depth == 16;
        let frame = convert_frame_cow(
            frame,
            if sixteen {
                PixelFormat::Rgba16
            } else {
                PixelFormat::Rgba8
            },
            s.alpha,
        );
        let channels = if s.has_alpha() { 4 } else { 3 };
        let bytes_per_sample = if sixteen { 2 } else { 1 };
        let mut data = Vec::with_capacity(
            frame.width as usize * frame.height as usize * channels * bytes_per_sample,
        );
        for px in frame.data.chunks_exact(4 * bytes_per_sample) {
            for c in 0..channels {
                let sample = &px[c * bytes_per_sample..(c + 1) * bytes_per_sample];
                if sixteen {
                    // PNG stores big-endian samples.
                    let v = u16::from_ne_bytes([sample[0], sample[1]]);
                    data.extend_from_slice(&v.to_be_bytes());
                } else {
                    data.push(sample[0]);
                }
            }
        }
        let file = std::fs::File::create(path).map_err(|e| MediaError::io(path, e))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), s.width, s.height);
        encoder.set_color(if s.has_alpha() {
            png::ColorType::Rgba
        } else {
            png::ColorType::Rgb
        });
        encoder.set_depth(if sixteen {
            png::BitDepth::Sixteen
        } else {
            png::BitDepth::Eight
        });
        encoder.set_compression(png::Compression::Fast);
        match s.color.transfer {
            TransferFunction::Srgb if matches!(s.color.primaries, ColorPrimaries::Bt709) => {
                encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            }
            TransferFunction::Gamma(g) if g > 0.0 => {
                encoder.set_source_gamma(png::ScaledFloat::new(1.0 / g));
            }
            TransferFunction::Linear => encoder.set_source_gamma(png::ScaledFloat::new(1.0)),
            _ => {}
        }
        if !s.pixel_aspect.is_square() {
            // Pixels per unit: a wider pixel means fewer per unit horizontally.
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: s.pixel_aspect.den,
                yppu: s.pixel_aspect.num,
                unit: png::Unit::Unspecified,
            }));
        }
        let mut writer = encoder
            .write_header()
            .map_err(|e| MediaError::decode(path, e))?;
        writer
            .write_image_data(&data)
            .map_err(|e| MediaError::decode(path, e))?;
        writer.finish().map_err(|e| MediaError::decode(path, e))
    }

    fn write_exr(
        &self,
        path: &Path,
        frame: &CpuFrame,
        compression: ExrCompression,
        index: u64,
    ) -> Result<(), MediaError> {
        use exr::prelude::{
            Blocks, Encoding, Image, LineOrder, SpecificChannels, Vec2, WritableImage as _,
        };
        let s = &self.settings;
        let half = s.bit_depth == 16;
        let frame = convert_frame_cow(
            frame,
            if half {
                PixelFormat::Rgba16Float
            } else {
                PixelFormat::Rgba32Float
            },
            s.alpha,
        );
        let width = s.width as usize;
        // `CpuFrame::data` is a byte vector: reinterpret when aligned, copy
        // otherwise.
        let halves: std::borrow::Cow<'_, [f16]> = if half {
            match bytemuck::try_cast_slice(&frame.data) {
                Ok(slice) => std::borrow::Cow::Borrowed(slice),
                Err(_) => std::borrow::Cow::Owned(bytemuck::pod_collect_to_vec(&frame.data)),
            }
        } else {
            std::borrow::Cow::Borrowed(&[])
        };
        let floats: std::borrow::Cow<'_, [f32]> = if half {
            std::borrow::Cow::Borrowed(&[])
        } else {
            match bytemuck::try_cast_slice(&frame.data) {
                Ok(slice) => std::borrow::Cow::Borrowed(slice),
                Err(_) => std::borrow::Cow::Owned(bytemuck::pod_collect_to_vec(&frame.data)),
            }
        };
        let (halves, floats) = (&*halves, &*floats);
        let px16 = move |pos: Vec2<usize>| {
            let i = (pos.1 * width + pos.0) * 4;
            [halves[i], halves[i + 1], halves[i + 2], halves[i + 3]]
        };
        let px32 = move |pos: Vec2<usize>| {
            let i = (pos.1 * width + pos.0) * 4;
            [floats[i], floats[i + 1], floats[i + 2], floats[i + 3]]
        };
        let encoding = Encoding {
            compression: match compression {
                ExrCompression::None => exr::compression::Compression::Uncompressed,
                ExrCompression::Rle => exr::compression::Compression::RLE,
                ExrCompression::Zips => exr::compression::Compression::ZIP1,
                ExrCompression::Zip => exr::compression::Compression::ZIP16,
                ExrCompression::Piz => exr::compression::Compression::PIZ,
            },
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        };
        let size = (width, s.height as usize);
        let mut attributes = exr::meta::header::ImageAttributes::with_size(size);
        attributes.pixel_aspect = s.pixel_aspect.as_f64() as f32;
        attributes.chromaticities = exr_chromaticities(s.color.primaries);
        if let Some(tc) = s.start_timecode {
            let frame_number = tc.to_frame_number(s.frame_rate) + index;
            let tc = Timecode::from_frame_number(frame_number, s.frame_rate, tc.drop_frame);
            attributes.time_code = Some(exr::meta::attribute::TimeCode {
                hours: tc.hours,
                minutes: tc.minutes,
                seconds: tc.seconds,
                frame: tc.frames,
                drop_frame: tc.drop_frame,
                color_frame: false,
                field_phase: false,
                binary_group_flags: [false; 3],
                binary_groups: [0; 8],
            });
        }
        let result = match (s.has_alpha(), half) {
            (true, true) => {
                let channels = SpecificChannels::rgba(move |pos| px16(pos).into());
                let mut image = Image::from_encoded_channels(size, encoding, channels);
                image.attributes = attributes;
                image.write().non_parallel().to_file(path)
            }
            (true, false) => {
                let channels = SpecificChannels::rgba(move |pos| px32(pos).into());
                let mut image = Image::from_encoded_channels(size, encoding, channels);
                image.attributes = attributes;
                image.write().non_parallel().to_file(path)
            }
            (false, true) => {
                let channels = SpecificChannels::rgb(move |pos| {
                    let [r, g, b, _] = px16(pos);
                    (r, g, b)
                });
                let mut image = Image::from_encoded_channels(size, encoding, channels);
                image.attributes = attributes;
                image.write().non_parallel().to_file(path)
            }
            (false, false) => {
                let channels = SpecificChannels::rgb(move |pos| {
                    let [r, g, b, _] = px32(pos);
                    (r, g, b)
                });
                let mut image = Image::from_encoded_channels(size, encoding, channels);
                image.attributes = attributes;
                image.write().non_parallel().to_file(path)
            }
        };
        result.map_err(|e| MediaError::decode(path, e))
    }
}

fn exr_chromaticities(primaries: ColorPrimaries) -> Option<exr::meta::attribute::Chromaticities> {
    use crate::meta::Chromaticities as C;
    let c = match primaries {
        ColorPrimaries::Bt709 => C::BT709,
        ColorPrimaries::Bt2020 => C::BT2020,
        ColorPrimaries::DisplayP3 => C::DISPLAY_P3,
        ColorPrimaries::AcesAp0 => C::ACES_AP0,
        ColorPrimaries::AcesAp1 => C::ACES_AP1,
        ColorPrimaries::Custom(c) => c,
        _ => return None,
    };
    let v = |p: [f32; 2]| exr::math::Vec2(p[0], p[1]);
    Some(exr::meta::attribute::Chromaticities {
        red: v(c.red),
        green: v(c.green),
        blue: v(c.blue),
        white: v(c.white),
    })
}

impl Encoder for ImageSequenceEncoder {
    fn settings(&self) -> &EncodeSettings {
        &self.settings
    }

    fn push_frame(&mut self, frame: &CpuFrame) -> Result<(), MediaError> {
        check_frame_size(&self.settings, frame)?;
        let index = self.files.len() as u64;
        let path = self
            .pattern
            .path_for(self.settings.start_number + index as i64);
        match &self.pool {
            Some(pool) => {
                if let Some(error) = pool.error.lock().take() {
                    return Err(error);
                }
                // Convert here (cheap when the GPU already did) so workers get
                // exactly what they write.
                let (format, alpha) = self.input_format().expect("sequences have a format");
                let frame = convert_frame_cow(frame, format, alpha).into_owned();
                let sender = pool.sender.as_ref().expect("open until finish");
                sender
                    .send(WriteJob {
                        path: path.clone(),
                        frame,
                        index,
                    })
                    .map_err(|_| MediaError::Unsupported("image writer stopped".into()))?;
            }
            None => self.writer.write(&path, frame, index)?,
        }
        self.files.push(path);
        Ok(())
    }

    fn frames_written(&self) -> u64 {
        self.files.len() as u64
    }

    fn input_format(&self) -> Option<(PixelFormat, AlphaMode)> {
        let s = &self.settings;
        let format = match (s.codec, s.bit_depth) {
            (VideoCodec::Png, 16) => PixelFormat::Rgba16,
            (VideoCodec::Png, _) => PixelFormat::Rgba8,
            (VideoCodec::Exr(_), 16) => PixelFormat::Rgba16Float,
            _ => PixelFormat::Rgba32Float,
        };
        Some((format, s.alpha))
    }

    fn finish(mut self: Box<Self>) -> Result<EncodeOutput, MediaError> {
        if let Some(pool) = self.pool.take() {
            pool.join()?;
        }
        Ok(EncodeOutput {
            frames: self.files.len() as u64,
            files: self.files,
        })
    }

    fn abort(mut self: Box<Self>) {
        if let Some(pool) = self.pool.take() {
            let _ = pool.join();
        }
        for file in &self.files {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// [`EncoderFormat`] for PNG and OpenEXR sequences.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImageSequenceFormat;

impl EncoderFormat for ImageSequenceFormat {
    fn id(&self) -> &str {
        "image-sequence"
    }

    fn supports(&self, settings: &EncodeSettings) -> bool {
        settings.codec.is_image_sequence()
    }

    fn create(&self, settings: &EncodeSettings) -> Result<Box<dyn Encoder>, MediaError> {
        Ok(Box::new(ImageSequenceEncoder::new(settings)?))
    }
}

// ---- GPU readback ----------------------------------------------------------

/// Reads rendered GPU frames back to the CPU asynchronously, keeping up to
/// `depth` copies in flight so the GPU can render the next frame while earlier
/// ones are mapped (double buffering with `depth = 2`). Frames come out in
/// submission order.
pub struct FrameReadback {
    depth: usize,
    in_flight: VecDeque<(PendingReadback, ReadbackMeta)>,
}

/// What a readback's bytes are.
#[derive(Clone, Debug)]
struct ReadbackMeta {
    format: PixelFormat,
    alpha: AlphaMode,
    color: ColorInfo,
    pixel_aspect: PixelAspect,
}

impl fmt::Debug for FrameReadback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameReadback")
            .field("depth", &self.depth)
            .field("in_flight", &self.in_flight.len())
            .finish()
    }
}

impl FrameReadback {
    /// A queue keeping up to `depth` (at least 1) readbacks in flight.
    pub fn new(depth: usize) -> Self {
        FrameReadback {
            depth: depth.max(1),
            in_flight: VecDeque::new(),
        }
    }

    /// Readbacks not yet collected.
    pub fn len(&self) -> usize {
        self.in_flight.len()
    }

    /// `true` when nothing is in flight.
    pub fn is_empty(&self) -> bool {
        self.in_flight.is_empty()
    }

    /// `true` when [`Self::submit`] would exceed the depth; collect first.
    pub fn is_full(&self) -> bool {
        self.in_flight.len() >= self.depth
    }

    /// Starts copying `frame` (its texture needs `COPY_SRC`, which every
    /// [`GpuFrame`] from [`MediaGpu`] has).
    pub fn submit(&mut self, gpu: &MediaGpu, frame: &GpuFrame) -> Result<(), MediaError> {
        let pending = TextureReadback::start(gpu.context(), &frame.texture)?;
        self.in_flight.push_back((
            pending,
            ReadbackMeta {
                format: PixelFormat::Rgba16Float,
                alpha: frame.alpha,
                color: frame.color.clone(),
                pixel_aspect: frame.pixel_aspect,
            },
        ));
        Ok(())
    }

    /// Starts copying a frame converted for an encoder
    /// ([`MediaGpu::convert_for_export`]).
    pub fn submit_export(
        &mut self,
        gpu: &MediaGpu,
        frame: &ExportTexture,
    ) -> Result<(), MediaError> {
        let pending = TextureReadback::start(gpu.context(), &frame.texture)?;
        self.in_flight.push_back((
            pending,
            ReadbackMeta {
                format: frame.format,
                alpha: frame.alpha,
                color: frame.color.clone(),
                pixel_aspect: frame.pixel_aspect,
            },
        ));
        Ok(())
    }

    /// The oldest frame if its copy has finished (non-blocking; the device must
    /// be polled, e.g. by the event loop or [`bae_gpu::GpuContext::poll`]).
    pub fn try_next(&mut self) -> Option<Result<CpuFrame, MediaError>> {
        let result = self.in_flight.front()?.0.try_finish()?;
        let (_, frame) = self.in_flight.pop_front()?;
        Some(
            result
                .map_err(MediaError::from)
                .map(|image| to_cpu(image, &frame)),
        )
    }

    /// The oldest frame, blocking until its copy has finished.
    pub fn wait_next(&mut self, gpu: &MediaGpu) -> Option<Result<CpuFrame, MediaError>> {
        let (pending, frame) = self.in_flight.pop_front()?;
        Some(
            pending
                .wait(gpu.context())
                .map_err(MediaError::from)
                .map(|image| to_cpu(image, &frame)),
        )
    }
}

fn to_cpu(image: bae_gpu::transfer::ImageData, frame: &ReadbackMeta) -> CpuFrame {
    CpuFrame {
        width: image.width,
        height: image.height,
        format: frame.format,
        data: image.data,
        alpha: frame.alpha,
        color: frame.color.clone(),
        pixel_aspect: frame.pixel_aspect,
    }
}

// ---- render loop -----------------------------------------------------------

/// A frame produced for export: on the GPU (normal) or in memory.
#[derive(Clone, Debug)]
pub enum ProducedFrame {
    /// A rendered texture (read back asynchronously).
    Gpu(GpuFrame),
    /// Pixels in memory (CPU renderers, tests).
    Cpu(CpuFrame),
}

/// Renders the frames of a job (the compositor implements this for a comp).
pub trait FrameProducer {
    /// Renders frame `frame` (in the job's frame numbering).
    fn produce(&mut self, frame: i64) -> Result<ProducedFrame, MediaError>;
}

impl<F: FnMut(i64) -> Result<ProducedFrame, MediaError>> FrameProducer for F {
    fn produce(&mut self, frame: i64) -> Result<ProducedFrame, MediaError> {
        self(frame)
    }
}

/// One export: what to render and how to write it (AE: a render queue item
/// with its output module).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderJob {
    /// Display name (usually the comp name).
    pub name: String,
    /// Frames to render, half-open, in the producer's numbering.
    pub frames: std::ops::Range<i64>,
    /// Output settings.
    pub settings: EncodeSettings,
}

impl RenderJob {
    /// Number of frames.
    pub fn frame_count(&self) -> u64 {
        (self.frames.end - self.frames.start).max(0) as u64
    }
}

/// Shared cancellation flag of a running job.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// `true` once cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Progress of a running job.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderProgress {
    /// Frames written.
    pub done: u64,
    /// Frames to write.
    pub total: u64,
    /// Time since the job started.
    pub elapsed: Duration,
}

impl RenderProgress {
    /// Fraction done, 0–1.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.done as f64 / self.total as f64
        }
    }

    /// Estimated time left, once at least one frame is done.
    pub fn remaining(&self) -> Option<Duration> {
        (self.done > 0).then(|| {
            self.elapsed
                .mul_f64((self.total - self.done.min(self.total)) as f64 / self.done as f64)
        })
    }
}

/// Readbacks in flight during [`render`].
pub const READBACK_DEPTH: usize = 2;

/// Renders `job`: produces each frame, reads GPU frames back asynchronously
/// ([`READBACK_DEPTH`] in flight) and encodes them. Calls `progress` after
/// every written frame. On cancellation or error the partial output is
/// removed; cancellation returns [`MediaError::Canceled`].
pub fn render(
    job: &RenderJob,
    producer: &mut dyn FrameProducer,
    gpu: Option<&MediaGpu>,
    encoders: &MediaEncoders,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(&RenderProgress),
) -> Result<EncodeOutput, MediaError> {
    let start = Instant::now();
    let total = job.frame_count();
    let mut encoder = encoders.create(&job.settings)?;
    let mut readback = FrameReadback::new(READBACK_DEPTH);
    let mut report = |encoder: &dyn Encoder| {
        progress(&RenderProgress {
            done: encoder.frames_written(),
            total,
            elapsed: start.elapsed(),
        });
    };
    let mut run = || -> Result<(), MediaError> {
        for frame in job.frames.clone() {
            if cancel.is_cancelled() {
                return Err(MediaError::Canceled);
            }
            match producer.produce(frame)? {
                ProducedFrame::Cpu(cpu) => {
                    // Keep order: drain GPU frames submitted before this one.
                    while let Some(done) = gpu.and_then(|gpu| readback.wait_next(gpu)) {
                        encoder.push_frame(&done?)?;
                        report(encoder.as_ref());
                    }
                    encoder.push_frame(&cpu)?;
                    report(encoder.as_ref());
                }
                ProducedFrame::Gpu(gpu_frame) => {
                    let gpu = gpu.ok_or_else(|| {
                        MediaError::Unsupported("GPU frame without a MediaGpu".into())
                    })?;
                    if readback.is_full()
                        && let Some(done) = readback.wait_next(gpu)
                    {
                        encoder.push_frame(&done?)?;
                        report(encoder.as_ref());
                    }
                    match encoder.input_format() {
                        Some((format, alpha))
                            if (format, alpha) != (PixelFormat::Rgba16Float, gpu_frame.alpha) =>
                        {
                            let converted = gpu.convert_for_export(&gpu_frame, format, alpha)?;
                            readback.submit_export(gpu, &converted)?;
                        }
                        _ => readback.submit(gpu, &gpu_frame)?,
                    }
                }
            }
        }
        if let Some(gpu) = gpu {
            while let Some(done) = readback.wait_next(gpu) {
                if cancel.is_cancelled() {
                    return Err(MediaError::Canceled);
                }
                encoder.push_frame(&done?)?;
                report(encoder.as_ref());
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => encoder.finish(),
        Err(error) => {
            encoder.abort();
            Err(error)
        }
    }
}

// ---- render queue ----------------------------------------------------------

/// Progress state of a render-queue item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderStatus {
    /// Waiting in the queue.
    Queued,
    /// Rendering; frames done so far out of the total.
    Rendering {
        /// Frames written.
        done: u64,
        /// Frames to write.
        total: u64,
    },
    /// Finished successfully.
    Done,
    /// Stopped by the user.
    Canceled,
    /// Failed (see [`RenderItem::error`]).
    Failed,
}

/// Id of a render-queue item, unique within its queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RenderJobId(pub u64);

/// A job in the [`RenderQueue`] with its runtime state.
#[derive(Clone, Debug)]
pub struct RenderItem {
    /// Id.
    pub id: RenderJobId,
    /// The job.
    pub job: RenderJob,
    /// Current state.
    pub status: RenderStatus,
    /// Error message of a failed job.
    pub error: Option<String>,
    /// What was written, once done.
    pub output: Option<EncodeOutput>,
    /// Cancels the job while it renders (or before it starts).
    pub cancel: CancelToken,
}

/// The runtime render queue: jobs in order, run one at a time.
#[derive(Clone, Debug, Default)]
pub struct RenderQueue {
    items: Vec<RenderItem>,
    next_id: u64,
}

impl RenderQueue {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a job; returns its id.
    pub fn add(&mut self, job: RenderJob) -> RenderJobId {
        let id = RenderJobId(self.next_id);
        self.next_id += 1;
        self.items.push(RenderItem {
            id,
            job,
            status: RenderStatus::Queued,
            error: None,
            output: None,
            cancel: CancelToken::new(),
        });
        id
    }

    /// The items in order.
    pub fn items(&self) -> &[RenderItem] {
        &self.items
    }

    /// One item.
    pub fn get(&self, id: RenderJobId) -> Option<&RenderItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Removes an item that is not rendering. Returns it.
    pub fn remove(&mut self, id: RenderJobId) -> Option<RenderItem> {
        let index = self.items.iter().position(|item| {
            item.id == id && !matches!(item.status, RenderStatus::Rendering { .. })
        })?;
        Some(self.items.remove(index))
    }

    /// Moves an item to position `index` (clamped).
    pub fn move_to(&mut self, id: RenderJobId, index: usize) -> bool {
        let Some(from) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        let item = self.items.remove(from);
        self.items.insert(index.min(self.items.len()), item);
        true
    }

    /// Cancels a queued or rendering item.
    pub fn cancel(&mut self, id: RenderJobId) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.cancel.cancel();
        if item.status == RenderStatus::Queued {
            item.status = RenderStatus::Canceled;
        }
        true
    }

    /// Puts a finished, cancelled or failed item back into the queue.
    pub fn requeue(&mut self, id: RenderJobId) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        if matches!(item.status, RenderStatus::Rendering { .. }) {
            return false;
        }
        item.status = RenderStatus::Queued;
        item.error = None;
        item.output = None;
        item.cancel = CancelToken::new();
        true
    }

    /// The first queued item.
    pub fn next_queued(&self) -> Option<RenderJobId> {
        self.items
            .iter()
            .find(|item| item.status == RenderStatus::Queued)
            .map(|item| item.id)
    }

    /// Records progress of a rendering item (called from the render loop's
    /// progress callback, possibly through a channel).
    pub fn set_progress(&mut self, id: RenderJobId, progress: &RenderProgress) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.status = RenderStatus::Rendering {
                done: progress.done,
                total: progress.total,
            };
        }
    }

    /// Records the result of a render.
    pub fn set_result(&mut self, id: RenderJobId, result: Result<EncodeOutput, MediaError>) {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return;
        };
        match result {
            Ok(output) => {
                item.status = RenderStatus::Done;
                item.output = Some(output);
            }
            Err(MediaError::Canceled) => item.status = RenderStatus::Canceled,
            Err(error) => {
                item.status = RenderStatus::Failed;
                item.error = Some(error.to_string());
            }
        }
    }

    /// Renders every queued item in order on the calling thread (headless
    /// `bae render`, tests). `producer_for` creates the frame producer of a job.
    pub fn render_all(
        &mut self,
        mut producer_for: impl FnMut(&RenderJob) -> Result<Box<dyn FrameProducer>, MediaError>,
        gpu: Option<&MediaGpu>,
        encoders: &MediaEncoders,
    ) {
        while let Some(id) = self.next_queued() {
            let (job, cancel) = {
                let item = self.get(id).expect("queued item exists");
                (item.job.clone(), item.cancel.clone())
            };
            self.set_progress(
                id,
                &RenderProgress {
                    total: job.frame_count(),
                    ..RenderProgress::default()
                },
            );
            let result = producer_for(&job).and_then(|mut producer| {
                let mut last = RenderProgress::default();
                let result = render(&job, producer.as_mut(), gpu, encoders, &cancel, &mut |p| {
                    last = *p;
                });
                self.set_progress(id, &last);
                result
            });
            self.set_result(id, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_patterns() {
        let p = SequencePattern::parse(Path::new("/out/comp_[####].png"));
        assert_eq!(
            (p.prefix.as_str(), p.suffix.as_str(), p.padding),
            ("comp_", ".png", 4)
        );
        assert_eq!(p.path_for(7), PathBuf::from("/out/comp_0007.png"));
        let p = SequencePattern::parse(Path::new("/out/shot.###.exr"));
        assert_eq!(p.path_for(1001), PathBuf::from("/out/shot.1001.exr"));
        let p = SequencePattern::parse(Path::new("/out/beauty_0100.exr"));
        assert_eq!((p.prefix.as_str(), p.padding), ("beauty_", 4));
        let p = SequencePattern::parse(Path::new("/out/render.png"));
        assert_eq!(p.path_for(3), PathBuf::from("/out/render_00003.png"));
    }

    #[test]
    fn settings_validation() {
        let ok = EncodeSettings::new("a.mov", VideoCodec::H264, 1920, 1080, FrameRate::FPS_24);
        assert!(ok.validate().is_ok());
        assert!(
            ok.clone()
                .with_alpha(AlphaMode::Straight)
                .validate()
                .is_err()
        );
        let odd = EncodeSettings {
            width: 1921,
            ..ok.clone()
        };
        assert!(odd.validate().is_err());
        let prores = EncodeSettings::new(
            "a.mov",
            VideoCodec::ProRes(ProResProfile::P4444),
            1920,
            1080,
            FrameRate::FPS_24,
        )
        .with_alpha(AlphaMode::Straight);
        assert!(prores.validate().is_ok());
        let hd = EncodeSettings::new("a.mp4", VideoCodec::H264, 1920, 1080, FrameRate::FPS_24);
        assert_eq!(
            (hd.color.primaries, hd.color.matrix),
            (ColorPrimaries::Bt709, MatrixCoefficients::Bt709)
        );
        let sd = EncodeSettings::new("a.mp4", VideoCodec::H264, 720, 480, FrameRate::FPS_29_97);
        assert_eq!(
            (sd.color.primaries, sd.color.matrix),
            (ColorPrimaries::Bt601Ntsc, MatrixCoefficients::Bt601)
        );
        let exr = EncodeSettings::new(
            "a.exr",
            VideoCodec::Exr(ExrCompression::Zip),
            3,
            3,
            FrameRate::FPS_24,
        );
        assert_eq!(exr.bit_depth, 16);
        assert!(exr.validate().is_ok());
        let json = serde_json::to_string(&prores).unwrap();
        assert_eq!(
            serde_json::from_str::<EncodeSettings>(&json).unwrap(),
            prores
        );
    }

    fn rgba8(pixels: &[[u8; 4]], alpha: AlphaMode) -> CpuFrame {
        CpuFrame {
            width: pixels.len() as u32,
            height: 1,
            format: PixelFormat::Rgba8,
            data: pixels.concat(),
            alpha,
            color: ColorInfo::srgb(),
            pixel_aspect: PixelAspect::SQUARE,
        }
    }

    #[test]
    fn conversions() {
        let straight = rgba8(&[[255, 128, 0, 128]], AlphaMode::Straight);
        let pre = convert_frame(&straight, PixelFormat::Rgba8, AlphaMode::Premultiplied);
        assert_eq!(pre.data, [128, 64, 0, 128]);
        assert_eq!(pre.alpha, AlphaMode::Premultiplied);
        let back = convert_frame(&pre, PixelFormat::Rgba16Float, AlphaMode::Straight);
        let p = back.pixel_f32(0, 0);
        assert!(
            (p[0] - 1.0).abs() < 0.01 && (p[1] - 0.5).abs() < 0.01,
            "{p:?}"
        );
        // RGB output is premultiplied over black.
        let opaque = convert_frame(&straight, PixelFormat::Bgra8, AlphaMode::Opaque);
        assert_eq!(opaque.data, [0, 64, 128, 255]);
        // Same format and alpha: unchanged.
        assert_eq!(
            convert_frame(&straight, PixelFormat::Rgba8, AlphaMode::Straight),
            straight
        );
    }

    #[test]
    fn progress_math() {
        let p = RenderProgress {
            done: 25,
            total: 100,
            elapsed: Duration::from_secs(10),
        };
        assert!((p.fraction() - 0.25).abs() < 1e-9);
        assert_eq!(p.remaining(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn queue_bookkeeping() {
        let job = |name: &str| RenderJob {
            name: name.into(),
            frames: 0..10,
            settings: EncodeSettings::new("a.png", VideoCodec::Png, 4, 4, FrameRate::FPS_24),
        };
        let mut queue = RenderQueue::new();
        let a = queue.add(job("a"));
        let b = queue.add(job("b"));
        assert_eq!(queue.next_queued(), Some(a));
        assert!(queue.cancel(a));
        assert_eq!(queue.get(a).unwrap().status, RenderStatus::Canceled);
        assert_eq!(queue.next_queued(), Some(b));
        assert!(queue.move_to(b, 0));
        assert_eq!(queue.items()[0].id, b);
        assert!(queue.requeue(a));
        assert!(!queue.get(a).unwrap().cancel.is_cancelled());
        queue.set_result(b, Err(MediaError::Unsupported("x".into())));
        assert_eq!(queue.get(b).unwrap().status, RenderStatus::Failed);
        assert!(queue.remove(b).is_some());
        assert_eq!(queue.items().len(), 1);
    }
}
