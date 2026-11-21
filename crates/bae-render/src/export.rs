//! Pipelined export (ARCHITECTURE §5: GPU→CPU readback during export is
//! asynchronous and does not block rendering): [`export_frames`] renders a
//! comp to PNG / TIFF / EXR sequences or movies ([`ExportSettings`]) for
//! `bae render` and the render queue.
//!
//! [`ExportReadback`] converts each rendered frame into the encoder's pixel
//! format on the GPU (the output pass, into a reused target), copies it into
//! one of a few reused staging buffers and maps it without waiting. The caller
//! keeps rendering; when every staging buffer is busy it collects the oldest
//! frame, waiting only for *that* frame's submission. With bae-media's
//! parallel sequence writer behind it, `bae render` overlaps all stages:
//!
//! ```text
//! main thread  eval N ─ render N ─ convert+copy N ─ collect N−2 (copy out) ─► encoder queue
//! GPU                    … frame N−1 … frame N …
//! encoders     compress + write N−3, N−4, … on every core (bounded queue)
//! ```

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use bae_gpu::transfer::padded_bytes_per_row;
use bae_gpu::{GpuContext, GpuError, wgpu};
use bae_model::{FrameRate, PrimPath, Stage, Time};
use parking_lot::Mutex;

use crate::error::RenderError;
use crate::eval::CompEvaluator;
use crate::renderer::{OUTPUT_PARAMS_SIZE, OutputAlpha, RenderedFrame, Renderer, output_params};
#[cfg(feature = "media")]
use crate::request::BitDepth;
use crate::request::RenderRequest;

/// Staging buffers in flight by default: the GPU works on one frame while the
/// previous one is mapped and the one before is copied out.
pub const READBACK_DEPTH: usize = 3;

/// Pixel layout of read-back frames (rows tightly packed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadbackFormat {
    /// 8-bit RGBA (PNG, TIFF).
    Rgba8,
    /// 8-bit BGRA (movie encoders).
    Bgra8,
    /// Half-float RGBA (EXR half, 16-bit integer outputs).
    Rgba16Float,
    /// Float RGBA (EXR float, 32-bpc work buffers).
    Rgba32Float,
}

impl ReadbackFormat {
    /// The render target format of the output pass.
    pub fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            ReadbackFormat::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            ReadbackFormat::Bgra8 => wgpu::TextureFormat::Bgra8Unorm,
            ReadbackFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            ReadbackFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        }
    }

    /// Bytes per pixel.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            ReadbackFormat::Rgba8 | ReadbackFormat::Bgra8 => 4,
            ReadbackFormat::Rgba16Float => 8,
            ReadbackFormat::Rgba32Float => 16,
        }
    }

    /// Float formats keep colour outside `[0, alpha]` (32-bpc super-whites).
    pub fn is_float(self) -> bool {
        matches!(
            self,
            ReadbackFormat::Rgba16Float | ReadbackFormat::Rgba32Float
        )
    }
}

/// A frame read back by [`ExportReadback`].
#[derive(Debug)]
pub struct ReadFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Layout of [`Self::data`].
    pub format: ReadbackFormat,
    /// How alpha was written.
    pub alpha: OutputAlpha,
    /// Pixels, rows tightly packed.
    pub data: Vec<u8>,
    /// The tag given to [`ExportReadback::submit`] (e.g. the frame index).
    pub tag: u64,
    /// Time the caller waited for the GPU (render, conversion, copy) and the
    /// mapping; zero when the frame was ready.
    pub wait: Duration,
    /// Time copying the pixels out of the staging buffer.
    pub copy: Duration,
}

struct Staging {
    buffer: wgpu::Buffer,
    size: u64,
}

/// A read-back frame still in its mapped staging buffer. [`Self::into_frame`]
/// copies the pixels out, on any thread (the export loop hands this to the
/// encoder thread, keeping the copy off the render thread); the buffer then
/// goes back to its [`ExportReadback`] for reuse.
pub struct MappedFrame {
    staging: Option<Staging>,
    recycle: std::sync::mpsc::Sender<Staging>,
    padded_bytes_per_row: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Layout of the pixels.
    pub format: ReadbackFormat,
    /// How alpha was written.
    pub alpha: OutputAlpha,
    /// The tag given to [`ExportReadback::submit`].
    pub tag: u64,
    /// Time the caller waited for the GPU and the mapping.
    pub wait: Duration,
}

impl std::fmt::Debug for MappedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("tag", &self.tag)
            .finish_non_exhaustive()
    }
}

impl MappedFrame {
    /// Copies the pixels out (rows tightly packed) and returns the staging
    /// buffer for reuse.
    pub fn into_frame(mut self) -> Result<ReadFrame, RenderError> {
        let staging = self.staging.take().expect("present until consumed");
        let started = Instant::now();
        let unpadded = (self.width * self.format.bytes_per_pixel()) as usize;
        let padded = self.padded_bytes_per_row as usize;
        let rows = self.height as usize;
        let data = staging
            .buffer
            .get_mapped_range(..)
            .map_err(|e| GpuError::Map(e.to_string()))
            .map(|view| {
                let mut data = Vec::with_capacity(unpadded * rows);
                if padded == unpadded {
                    data.extend_from_slice(&view[..unpadded * rows]);
                } else {
                    for row in view.chunks(padded).take(rows) {
                        data.extend_from_slice(&row[..unpadded]);
                    }
                }
                data
            });
        staging.buffer.unmap();
        let _ = self.recycle.send(staging);
        Ok(ReadFrame {
            width: self.width,
            height: self.height,
            format: self.format,
            alpha: self.alpha,
            data: data?,
            tag: self.tag,
            wait: self.wait,
            copy: started.elapsed(),
        })
    }
}

impl Drop for MappedFrame {
    fn drop(&mut self) {
        if let Some(staging) = self.staging.take() {
            staging.buffer.unmap();
            let _ = self.recycle.send(staging);
        }
    }
}

struct InFlight {
    staging: Staging,
    submission: wgpu::SubmissionIndex,
    mapped: Arc<Mutex<Option<Result<(), String>>>>,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: ReadbackFormat,
    alpha: OutputAlpha,
    tag: u64,
}

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: ReadbackFormat,
    size: [u32; 2],
}

/// Asynchronous readback of rendered frames for export, with reused output
/// targets and staging buffers. Frames come out in submission order.
pub struct ExportReadback {
    gpu: Arc<GpuContext>,
    depth: usize,
    free: Vec<Staging>,
    /// Buffers coming back from [`MappedFrame`]s (possibly other threads).
    recycle: std::sync::mpsc::Sender<Staging>,
    recycled: std::sync::mpsc::Receiver<Staging>,
    in_flight: VecDeque<InFlight>,
    target: Option<Target>,
    uniform: wgpu::Buffer,
    params: Option<[u8; OUTPUT_PARAMS_SIZE as usize]>,
}

impl std::fmt::Debug for ExportReadback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportReadback")
            .field("depth", &self.depth)
            .field("in_flight", &self.in_flight.len())
            .finish_non_exhaustive()
    }
}

impl ExportReadback {
    /// A readback keeping up to `depth` (at least 1) frames in flight
    /// ([`READBACK_DEPTH`] is a good default).
    pub fn new(gpu: Arc<GpuContext>, depth: usize) -> Self {
        let uniform = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae-render.export.output"),
            size: OUTPUT_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (recycle, recycled) = std::sync::mpsc::channel();
        Self {
            gpu,
            depth: depth.max(1),
            free: Vec::new(),
            recycle,
            recycled,
            in_flight: VecDeque::new(),
            target: None,
            uniform,
            params: None,
        }
    }

    /// Frames submitted and not yet collected.
    pub fn len(&self) -> usize {
        self.in_flight.len()
    }

    /// `true` when nothing is in flight.
    pub fn is_empty(&self) -> bool {
        self.in_flight.is_empty()
    }

    /// `true` when `depth` frames are in flight on the GPU: collect one before
    /// the next [`Self::submit`]. (Frames collected as [`MappedFrame`]s hold
    /// their buffer until copied out; more buffers are made meanwhile.)
    pub fn is_full(&self) -> bool {
        self.in_flight.len() >= self.depth
    }

    /// Converts `frame` to `format` with `alpha` on the GPU and starts copying
    /// it to the CPU. Returns without waiting; `frame` may be dropped (the GPU
    /// orders the copy before any later reuse of its texture).
    pub fn submit(
        &mut self,
        renderer: &mut Renderer,
        frame: &RenderedFrame,
        alpha: OutputAlpha,
        format: ReadbackFormat,
        tag: u64,
    ) -> Result<(), RenderError> {
        let device = self.gpu.device();
        let [w, h] = frame.image.texel_size();
        let (w, h) = (w.max(1), h.max(1));
        if !self
            .target
            .as_ref()
            .is_some_and(|t| t.format == format && t.size == [w, h])
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.export.output"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: format.texture_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some(Target {
                texture,
                view,
                format,
                size: [w, h],
            });
        }
        let target = self.target.as_ref().expect("just ensured");
        // Ordered before this submission, after the earlier ones that read it.
        let params = output_params(alpha, format.is_float());
        if self.params != Some(params) {
            self.gpu.queue().write_buffer(&self.uniform, 0, &params);
            self.params = Some(params);
        }

        let padded = padded_bytes_per_row(w * format.bytes_per_pixel());
        let size = u64::from(padded) * u64::from(h);
        self.free.extend(self.recycled.try_iter());
        let staging = match self.free.iter().position(|s| s.size == size) {
            Some(i) => self.free.swap_remove(i),
            None => Staging {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bae-render.export.staging"),
                    size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                size,
            },
        };
        // Buffers of another size are not coming back.
        self.free.retain(|s| s.size == size);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae-render.export"),
        });
        renderer.record_output(
            &mut encoder,
            frame,
            &target.view,
            format.texture_format(),
            &self.uniform,
        );
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.gpu.queue().submit([encoder.finish()]);
        let mapped: Arc<Mutex<Option<Result<(), String>>>> = Arc::default();
        let callback = mapped.clone();
        staging
            .buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                *callback.lock() = Some(result.map_err(|e| e.to_string()));
            });
        self.in_flight.push_back(InFlight {
            staging,
            submission,
            mapped,
            width: w,
            height: h,
            padded_bytes_per_row: padded,
            format,
            alpha,
            tag,
        });
        Ok(())
    }

    /// The oldest frame if its copy has finished (polls the device without
    /// blocking), copied out.
    pub fn try_next(&mut self) -> Option<Result<ReadFrame, RenderError>> {
        Some(self.try_next_mapped()?.and_then(MappedFrame::into_frame))
    }

    /// The oldest frame, waiting for its submission only (later frames keep
    /// the GPU busy meanwhile), copied out.
    pub fn wait_next(&mut self) -> Option<Result<ReadFrame, RenderError>> {
        Some(self.wait_next_mapped()?.and_then(MappedFrame::into_frame))
    }

    /// Like [`Self::try_next`], but leaves the pixels in the staging buffer.
    pub fn try_next_mapped(&mut self) -> Option<Result<MappedFrame, RenderError>> {
        let front = self.in_flight.front()?;
        if front.mapped.lock().is_none() {
            let _ = self.gpu.device().poll(wgpu::PollType::Poll);
            if front.mapped.lock().is_none() {
                return None;
            }
        }
        let item = self.in_flight.pop_front()?;
        Some(self.mapped(item, Duration::ZERO))
    }

    /// Like [`Self::wait_next`], but leaves the pixels in the staging buffer.
    pub fn wait_next_mapped(&mut self) -> Option<Result<MappedFrame, RenderError>> {
        let item = self.in_flight.pop_front()?;
        let started = Instant::now();
        while item.mapped.lock().is_none() {
            if let Some(info) = self.gpu.lost_info() {
                return Some(Err(GpuError::DeviceLost(info).into()));
            }
            if let Err(error) = self.gpu.device().poll(wgpu::PollType::Wait {
                submission_index: Some(item.submission.clone()),
                timeout: None,
            }) {
                return Some(Err(GpuError::from(error).into()));
            }
        }
        Some(self.mapped(item, started.elapsed()))
    }

    fn mapped(&self, item: InFlight, wait: Duration) -> Result<MappedFrame, RenderError> {
        let status = item.mapped.lock().take().expect("collected after mapping");
        // A failed mapping leaves the buffer unmapped; it is dropped.
        status.map_err(GpuError::Map)?;
        Ok(MappedFrame {
            staging: Some(item.staging),
            recycle: self.recycle.clone(),
            padded_bytes_per_row: item.padded_bytes_per_row,
            width: item.width,
            height: item.height,
            format: item.format,
            alpha: item.alpha,
            tag: item.tag,
            wait,
        })
    }
}

/// An encoder (bae-media) on its own thread behind a short bounded queue, so
/// the render loop never runs encoder code: frames arrive still in their
/// staging buffers and are copied out here, a movie encoder works on frame N
/// while the GPU renders N+1, and image sequences dispatch to their writer
/// threads from here.
#[cfg(feature = "media")]
pub(crate) struct EncoderThread {
    sender: Option<std::sync::mpsc::SyncSender<MappedFrame>>,
    handle: Option<std::thread::JoinHandle<Result<EncoderOutput, RenderError>>>,
}

/// What the encoder thread did.
#[cfg(feature = "media")]
pub(crate) struct EncoderOutput {
    pub output: bae_media::EncodeOutput,
    /// Copying frames out of staging buffers.
    pub copy: Duration,
}

#[cfg(feature = "media")]
impl EncoderThread {
    /// Frames queued ahead of the encoder thread.
    const QUEUE: usize = 2;

    /// Frames that were written stay written when the export stops or fails
    /// (AE keeps the frames rendered before Stop): on an error the encoder is
    /// finished, not aborted, so complete files are kept.
    pub(crate) fn spawn(mut encoder: Box<dyn bae_media::Encoder>) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<MappedFrame>(Self::QUEUE);
        let handle = std::thread::Builder::new()
            .name("bae-render.encode".into())
            .spawn(move || {
                let mut copy = Duration::ZERO;
                for mapped in receiver.iter() {
                    let pushed = mapped.into_frame().and_then(|frame| {
                        copy += frame.copy;
                        let frame = cpu_frame(frame, encoder.settings());
                        encoder.push_frame_owned(frame).map_err(media_error)
                    });
                    if let Err(error) = pushed {
                        // Wait for the frames before this one; keep them.
                        let _ = encoder.finish();
                        return Err(error);
                    }
                }
                let output = encoder.finish().map_err(media_error)?;
                Ok(EncoderOutput { output, copy })
            })
            .expect("spawn encoder thread");
        Self {
            sender: Some(sender),
            handle: Some(handle),
        }
    }

    /// Queues a frame (blocks while the encoder is behind). After an encoder
    /// error, returns that error.
    pub(crate) fn push(&mut self, frame: MappedFrame) -> Result<(), RenderError> {
        let sender = self.sender.as_ref().expect("open until finish");
        if sender.send(frame).is_ok() {
            return Ok(());
        }
        // The thread stopped on an error: report it.
        self.sender = None;
        Err(self
            .join()
            .err()
            .unwrap_or_else(|| RenderError::Other("encoder stopped".into())))
    }

    /// Waits for every queued frame to be written.
    pub(crate) fn finish(mut self) -> Result<EncoderOutput, RenderError> {
        self.sender = None;
        self.join()
    }

    /// Stops after an error: waits for the queued frames, keeps what was
    /// written.
    pub(crate) fn close(mut self) {
        self.sender = None;
        let _ = self.join();
    }

    fn join(&mut self) -> Result<EncoderOutput, RenderError> {
        let handle = self
            .handle
            .take()
            .ok_or_else(|| RenderError::Other("encoder already finished".into()))?;
        handle
            .join()
            .unwrap_or_else(|_| Err(RenderError::Other("encoder thread panicked".into())))
    }
}

#[cfg(feature = "media")]
impl Drop for EncoderThread {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.sender = None;
            let _ = self.join();
        }
    }
}

// ---- output settings ----------------------------------------------------------

/// Output format of an export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    /// PNG sequence (8 or 16 bit).
    #[default]
    Png,
    /// TIFF sequence (8 or 16 bit; uncompressed by default, as AE).
    Tiff,
    /// OpenEXR sequence (half or float, premultiplied).
    Exr,
    /// H.264 movie (macOS).
    H264,
    /// HEVC movie (macOS).
    Hevc,
    /// ProRes 422 HQ movie (macOS).
    ProRes,
    /// ProRes 4444 movie with alpha (macOS).
    ProRes4444,
}

impl ExportFormat {
    /// Names accepted by [`Self::parse`] (`bae render --format`).
    pub const NAMES: [&'static str; 7] =
        ["png", "tiff", "exr", "h264", "hevc", "prores", "prores4444"];

    /// From a name in [`Self::NAMES`] or a file extension.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "png" => Self::Png,
            "tiff" | "tif" => Self::Tiff,
            "exr" => Self::Exr,
            "h264" | "mp4" => Self::H264,
            "hevc" => Self::Hevc,
            "prores" | "mov" => Self::ProRes,
            "prores4444" => Self::ProRes4444,
            _ => return None,
        })
    }

    /// From an output file's extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        Self::parse(&ext)
    }

    /// The name in [`Self::NAMES`].
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Tiff => "tiff",
            Self::Exr => "exr",
            Self::H264 => "h264",
            Self::Hevc => "hevc",
            Self::ProRes => "prores",
            Self::ProRes4444 => "prores4444",
        }
    }

    /// File extension.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Tiff => "tif",
            Self::Exr => "exr",
            Self::H264 | Self::Hevc => "mp4",
            Self::ProRes | Self::ProRes4444 => "mov",
        }
    }

    /// One file per frame.
    pub fn is_image(self) -> bool {
        matches!(self, Self::Png | Self::Tiff | Self::Exr)
    }

    /// The compression used when none is given.
    pub fn default_compression(self) -> &'static str {
        match self {
            Self::Png => "fast",
            Self::Tiff => "none",
            Self::Exr => "zip",
            _ => "default",
        }
    }
}

/// How an export writes alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExportAlpha {
    /// RGB + straight alpha (EXR: premultiplied, which EXR requires).
    #[default]
    Straight,
    /// RGB + premultiplied alpha.
    Premultiplied,
    /// RGB over the comp background, no alpha (always for codecs without
    /// alpha, like AE's "RGB" output).
    Background,
}

impl ExportAlpha {
    /// The output pass's alpha for a comp with `background` (straight RGBA).
    pub fn output(self, background: [f32; 4]) -> OutputAlpha {
        match self {
            ExportAlpha::Straight => OutputAlpha::Straight,
            ExportAlpha::Premultiplied => OutputAlpha::Premultiplied,
            ExportAlpha::Background => {
                OutputAlpha::Over([background[0], background[1], background[2]])
            }
        }
    }
}

/// What an export writes and how (AE's output module).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportSettings {
    /// Format.
    pub format: ExportFormat,
    /// Output file; for sequences a pattern whose first run of `#` in the file
    /// name is the frame number (`out_####.png`, AE's `out_[####].png`), else
    /// `_####` is added.
    pub path: PathBuf,
    /// Format-specific compression: png `none | fastest | fast | balanced |
    /// high | 0-9`, tiff `none | lzw | deflate`, exr `none | rle | zips | zip |
    /// piz`. `None`: [`ExportFormat::default_compression`].
    pub compression: Option<String>,
    /// Bits per channel: png / tiff 8 or 16, exr 16 or 32. `None`: 8, EXR 16
    /// (32 for 32-bpc projects).
    pub bits: Option<u8>,
    /// Encoder threads for image sequences; `None`: one per core.
    pub threads: Option<usize>,
    /// Alpha.
    pub alpha: ExportAlpha,
}

impl ExportSettings {
    /// Settings with defaults for `format` writing to `path`.
    pub fn new(format: ExportFormat, path: impl Into<PathBuf>) -> Self {
        Self {
            format,
            path: path.into(),
            compression: None,
            bits: None,
            threads: None,
            alpha: ExportAlpha::default(),
        }
    }

    /// Checks the options (before any rendering).
    pub fn validate(&self, rate: FrameRate, request: &RenderRequest) -> Result<(), RenderError> {
        #[cfg(feature = "media")]
        {
            encode_settings(
                self,
                request.depth == BitDepth::Float32,
                &explicit_pattern(&self.path),
                [2, 2],
                rate,
                0,
            )
            .map(|_| ())
        }
        #[cfg(not(feature = "media"))]
        {
            let _ = (rate, request);
            if self.format == ExportFormat::Png
                && self.compression.as_deref().is_none_or(|c| c == "fast")
                && self.bits.is_none_or(|b| b == 8)
            {
                Ok(())
            } else {
                Err(RenderError::Other(
                    "this build writes 8-bit PNG only (bae-render without the `media` feature)"
                        .into(),
                ))
            }
        }
    }
}

/// Replaces the first run of `#` in the file name with the zero-padded frame
/// number (AE's `[####]` loses its brackets), or appends `_NNNN` before the
/// extension when there is none. Same naming as bae-media's sequence writer.
pub fn frame_file(pattern: &Path, frame: i64) -> PathBuf {
    let name = pattern
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let number = |width: usize| {
        if frame < 0 {
            format!("-{:0width$}", frame.unsigned_abs())
        } else {
            format!("{frame:0width$}")
        }
    };
    if let Some(start) = name.find('#') {
        let end = start + name[start..].chars().take_while(|&c| c == '#').count();
        let (mut prefix, mut suffix) = (&name[..start], &name[end..]);
        if prefix.ends_with('[') && suffix.starts_with(']') {
            prefix = &prefix[..prefix.len() - 1];
            suffix = &suffix[1..];
        }
        return pattern.with_file_name(format!("{prefix}{}{suffix}", number(end - start)));
    }
    let stem = pattern
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = pattern
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "png".into());
    pattern.with_file_name(format!("{stem}_{}.{ext}", number(4)))
}

/// `pattern` with an explicit `####` run when its file name has none, so
/// bae-media numbers files exactly like [`frame_file`].
#[cfg_attr(not(feature = "media"), allow(dead_code))]
pub(crate) fn explicit_pattern(pattern: &Path) -> PathBuf {
    let has_run = pattern
        .file_name()
        .is_some_and(|n| n.to_string_lossy().contains('#'));
    if has_run {
        return pattern.to_owned();
    }
    let stem = pattern
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = pattern
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "png".into());
    pattern.with_file_name(format!("{stem}_####.{ext}"))
}

/// Encoders available to an export (bae-media's, or nothing without it).
#[cfg(feature = "media")]
pub type Encoders = bae_media::MediaEncoders;
/// Encoders available to an export (bae-media's, or nothing without it).
#[cfg(not(feature = "media"))]
pub type Encoders = ();

#[cfg(feature = "media")]
fn media_error(error: bae_media::MediaError) -> RenderError {
    RenderError::Other(error.to_string())
}

/// bae-media encoder settings for `settings` writing `size` frames to `path`.
#[cfg(feature = "media")]
fn encode_settings(
    settings: &ExportSettings,
    float_project: bool,
    path: &Path,
    size: [u32; 2],
    rate: FrameRate,
    start_number: i64,
) -> Result<bae_media::EncodeSettings, RenderError> {
    use bae_media::{
        AlphaMode, EncodeSettings, ExrCompression, PngCompression, ProResProfile, TiffCompression,
        VideoCodec,
    };
    let level = settings.compression.as_deref();
    let bad = |what: &str| {
        RenderError::Other(format!(
            "bad compression `{}` for {what}",
            level.unwrap_or("")
        ))
    };
    let format = settings.format;
    let codec = match format {
        ExportFormat::Png => VideoCodec::Png,
        ExportFormat::Tiff => VideoCodec::Tiff(match level {
            None | Some("none") => TiffCompression::None,
            Some("lzw") => TiffCompression::Lzw,
            Some("deflate" | "zip") => TiffCompression::Deflate,
            Some(_) => return Err(bad("tiff (none | lzw | deflate)")),
        }),
        ExportFormat::Exr => VideoCodec::Exr(match level {
            None => ExrCompression::default(),
            Some("none") => ExrCompression::None,
            Some("rle") => ExrCompression::Rle,
            Some("zips") => ExrCompression::Zips,
            Some("zip") => ExrCompression::Zip,
            Some("piz") => ExrCompression::Piz,
            Some(_) => return Err(bad("exr (none | rle | zips | zip | piz)")),
        }),
        ExportFormat::H264 => VideoCodec::H264,
        ExportFormat::Hevc => VideoCodec::Hevc,
        ExportFormat::ProRes => VideoCodec::ProRes(ProResProfile::Hq),
        ExportFormat::ProRes4444 => VideoCodec::ProRes(ProResProfile::P4444),
    };
    let mut encode = EncodeSettings::new(path, codec, size[0], size[1], rate);
    match (format, level) {
        (ExportFormat::Png, Some(level)) => {
            encode.png_compression =
                match level {
                    "none" => PngCompression::None,
                    "fastest" => PngCompression::Fastest,
                    "fast" => PngCompression::Fast,
                    "balanced" => PngCompression::Balanced,
                    "high" => PngCompression::High,
                    n => PngCompression::Level(n.parse().ok().filter(|n| *n <= 9).ok_or_else(
                        || bad("png (none | fastest | fast | balanced | high | 0-9)"),
                    )?),
                };
        }
        (format, Some(_)) if !format.is_image() => {
            return Err(RenderError::Other(
                "compression applies to png, tiff and exr".into(),
            ));
        }
        _ => {}
    }
    if let Some(bits) = settings.bits {
        encode.bit_depth = bits;
    } else if format == ExportFormat::Exr && float_project {
        encode.bit_depth = 32;
    }
    encode.alpha = match settings.alpha {
        ExportAlpha::Straight => AlphaMode::Straight,
        ExportAlpha::Premultiplied => AlphaMode::Premultiplied,
        ExportAlpha::Background => AlphaMode::Opaque,
    };
    if encode.has_alpha() && !codec.supports_alpha() {
        // RGB output shows the comp background, like AE.
        encode.alpha = AlphaMode::Opaque;
    }
    if format == ExportFormat::Exr && encode.alpha == AlphaMode::Straight {
        // OpenEXR colour is premultiplied by definition (AE offers nothing else).
        encode.alpha = AlphaMode::Premultiplied;
    }
    encode.threads = settings.threads;
    encode.start_number = start_number;
    encode.validate().map_err(media_error)?;
    Ok(encode)
}

// ---- where frames go ------------------------------------------------------------

/// Where read-back frames go.
enum Sink {
    /// A bae-media encoder on its own thread (image sequences, movies).
    #[cfg(feature = "media")]
    Encoder {
        thread: EncoderThread,
        settings: bae_media::EncodeSettings,
        format: ReadbackFormat,
        alpha: ExportAlpha,
        /// Image sequences: file naming.
        pattern: Option<bae_media::export::SequencePattern>,
    },
    /// One image file, written on this thread.
    #[cfg(feature = "media")]
    Image {
        settings: bae_media::EncodeSettings,
        path: PathBuf,
        format: ReadbackFormat,
        alpha: ExportAlpha,
        stats: bae_media::EncodeStats,
    },
    /// PNG without bae-media (8-bit RGBA, on this thread).
    #[cfg(not(feature = "media"))]
    Png {
        pattern: PathBuf,
        sequence: bool,
        alpha: ExportAlpha,
        encode: Duration,
        bytes: u64,
        /// Written so far (removed on abort).
        files: Vec<PathBuf>,
    },
}

/// What a sink wrote.
#[derive(Clone, Copy, Debug, Default)]
struct SinkReport {
    files: usize,
    encode: Duration,
    write: Duration,
    bytes: u64,
    threads: usize,
    /// Copying frames out of staging buffers on the encoder thread.
    copy: Duration,
}

impl Sink {
    /// The sink for frames of `size` (texels) whose first frame is `first`.
    fn create(
        job: &ExportJob<'_>,
        sequence: bool,
        size: [u32; 2],
        first: i64,
        float_work: bool,
    ) -> Result<Sink, RenderError> {
        let settings = job.settings;
        #[cfg(feature = "media")]
        {
            let float_project = job.request.depth == BitDepth::Float32;
            let readback = |format: bae_media::PixelFormat| {
                use bae_media::PixelFormat;
                match format {
                    PixelFormat::Rgba8 => ReadbackFormat::Rgba8,
                    PixelFormat::Bgra8 => ReadbackFormat::Bgra8,
                    PixelFormat::Rgba16Float => ReadbackFormat::Rgba16Float,
                    // 16-bit integers from half floats are exact for RGBA16F
                    // work buffers; RGBA32F ones need float.
                    PixelFormat::Rgba16 if !float_work => ReadbackFormat::Rgba16Float,
                    PixelFormat::Rgba16 | PixelFormat::Rgba32Float => ReadbackFormat::Rgba32Float,
                }
            };
            // The output pass writes the alpha the encoder wants (EXR is
            // premultiplied, codecs without alpha get the background).
            let alpha_for = |encode: &bae_media::EncodeSettings| match encode.alpha {
                bae_media::AlphaMode::Straight => ExportAlpha::Straight,
                bae_media::AlphaMode::Premultiplied => ExportAlpha::Premultiplied,
                _ => ExportAlpha::Background,
            };
            if settings.format.is_image() && !sequence {
                let encode = encode_settings(
                    settings,
                    float_project,
                    &settings.path,
                    size,
                    job.rate,
                    first,
                )?;
                return Ok(Sink::Image {
                    format: readback(bae_media::export::sequence_input_format(&encode).0),
                    alpha: alpha_for(&encode),
                    path: settings.path.clone(),
                    settings: encode,
                    stats: bae_media::EncodeStats::default(),
                });
            }
            let path = if settings.format.is_image() {
                explicit_pattern(&settings.path)
            } else {
                settings.path.clone()
            };
            let encode = encode_settings(settings, float_project, &path, size, job.rate, first)?;
            let builtin;
            let encoders = match job.encoders {
                Some(encoders) => encoders,
                None => {
                    builtin = bae_media::MediaEncoders::builtin();
                    &builtin
                }
            };
            let encoder = encoders.create(&encode).map_err(media_error)?;
            let format = encoder
                .input_format()
                .map_or(ReadbackFormat::Rgba8, |(format, _)| readback(format));
            Ok(Sink::Encoder {
                pattern: settings
                    .format
                    .is_image()
                    .then(|| bae_media::export::SequencePattern::parse(&path)),
                thread: EncoderThread::spawn(encoder),
                alpha: alpha_for(&encode),
                settings: encode,
                format,
            })
        }
        #[cfg(not(feature = "media"))]
        {
            let _ = (size, first, float_work);
            settings.validate(job.rate, job.request)?;
            Ok(Sink::Png {
                pattern: settings.path.clone(),
                sequence,
                alpha: settings.alpha,
                encode: Duration::ZERO,
                bytes: 0,
                files: Vec::new(),
            })
        }
    }

    /// Pixel layout to read back.
    fn readback_format(&self) -> ReadbackFormat {
        match self {
            #[cfg(feature = "media")]
            Sink::Encoder { format, .. } | Sink::Image { format, .. } => *format,
            #[cfg(not(feature = "media"))]
            Sink::Png { .. } => ReadbackFormat::Rgba8,
        }
    }

    /// Alpha the output pass writes.
    fn alpha(&self) -> ExportAlpha {
        match self {
            #[cfg(feature = "media")]
            Sink::Encoder { alpha, .. } | Sink::Image { alpha, .. } => *alpha,
            #[cfg(not(feature = "media"))]
            Sink::Png { alpha, .. } => *alpha,
        }
    }

    /// The file frame `frame` goes to.
    fn file(&self, frame: Option<i64>) -> PathBuf {
        match self {
            #[cfg(feature = "media")]
            Sink::Encoder {
                pattern, settings, ..
            } => match (pattern, frame) {
                (Some(pattern), Some(frame)) => pattern.path_for(frame),
                _ => settings.path.clone(),
            },
            #[cfg(feature = "media")]
            Sink::Image { path, .. } => path.clone(),
            #[cfg(not(feature = "media"))]
            Sink::Png {
                pattern, sequence, ..
            } => match frame {
                Some(frame) if *sequence => frame_file(pattern, frame),
                _ => pattern.clone(),
            },
        }
    }

    /// Hands a frame over to `file`. Returns the time spent waiting for the
    /// encoder and copying out on this thread.
    fn push(
        &mut self,
        frame: MappedFrame,
        file: &Path,
    ) -> Result<(Duration, Duration), RenderError> {
        match self {
            #[cfg(feature = "media")]
            Sink::Encoder { thread, .. } => {
                let _ = file;
                let started = Instant::now();
                thread.push(frame)?;
                Ok((started.elapsed(), Duration::ZERO))
            }
            #[cfg(feature = "media")]
            Sink::Image {
                settings, stats, ..
            } => {
                let frame = frame.into_frame()?;
                let copy = frame.copy;
                *stats = bae_media::export::write_image_file(
                    settings,
                    file,
                    &cpu_frame(frame, settings),
                )
                .map_err(media_error)?;
                Ok((Duration::ZERO, copy))
            }
            #[cfg(not(feature = "media"))]
            Sink::Png {
                encode,
                bytes,
                files,
                ..
            } => {
                let frame = frame.into_frame()?;
                let copy = frame.copy;
                let started = Instant::now();
                let image = bae_gpu::transfer::ImageData {
                    width: frame.width,
                    height: frame.height,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    bytes_per_row: frame.width * 4,
                    data: frame.data,
                };
                files.push(file.to_owned());
                crate::png_out::write_rgba8(file, &image)
                    .map_err(|e| RenderError::Other(format!("{}: {e}", file.display())))?;
                *encode += started.elapsed();
                *bytes += std::fs::metadata(file).map_or(0, |m| m.len());
                Ok((Duration::ZERO, copy))
            }
        }
    }

    /// Waits for everything to be written.
    fn finish(self, frames: usize) -> Result<SinkReport, RenderError> {
        #[cfg(not(feature = "media"))]
        let _ = frames;
        match self {
            #[cfg(feature = "media")]
            Sink::Encoder { thread, .. } => {
                let EncoderOutput { output, copy } = thread.finish()?;
                // Movie encoders do not count bytes: take the file sizes.
                let bytes = match output.stats.bytes {
                    0 => output
                        .files
                        .iter()
                        .filter_map(|f| std::fs::metadata(f).ok())
                        .map(|m| m.len())
                        .sum(),
                    bytes => bytes,
                };
                Ok(SinkReport {
                    files: output.files.len(),
                    encode: output.stats.encode,
                    write: output.stats.write,
                    bytes,
                    threads: output.stats.threads,
                    copy,
                })
            }
            #[cfg(feature = "media")]
            Sink::Image { stats, .. } => Ok(SinkReport {
                files: frames,
                encode: stats.encode,
                write: stats.write,
                bytes: stats.bytes,
                threads: 1,
                copy: Duration::ZERO,
            }),
            #[cfg(not(feature = "media"))]
            Sink::Png {
                encode,
                bytes,
                files,
                ..
            } => Ok(SinkReport {
                files: files.len(),
                encode,
                write: Duration::ZERO,
                bytes,
                threads: 1,
                copy: Duration::ZERO,
            }),
        }
    }

    /// Stops after an error, keeping every file written so far.
    fn close(self) {
        match self {
            #[cfg(feature = "media")]
            Sink::Encoder { thread, .. } => thread.close(),
            #[cfg(feature = "media")]
            Sink::Image { .. } => {}
            #[cfg(not(feature = "media"))]
            Sink::Png { .. } => {}
        }
    }
}

/// A read-back frame as bae-media's frame type.
#[cfg(feature = "media")]
fn cpu_frame(frame: ReadFrame, settings: &bae_media::EncodeSettings) -> bae_media::CpuFrame {
    use bae_media::{AlphaMode, PixelFormat};
    bae_media::CpuFrame {
        width: frame.width,
        height: frame.height,
        format: match frame.format {
            ReadbackFormat::Rgba8 => PixelFormat::Rgba8,
            ReadbackFormat::Bgra8 => PixelFormat::Bgra8,
            ReadbackFormat::Rgba16Float => PixelFormat::Rgba16Float,
            ReadbackFormat::Rgba32Float => PixelFormat::Rgba32Float,
        },
        data: frame.data,
        alpha: match frame.alpha {
            OutputAlpha::Straight => AlphaMode::Straight,
            OutputAlpha::Premultiplied => AlphaMode::Premultiplied,
            OutputAlpha::Over(_) => AlphaMode::Opaque,
        },
        color: settings.color.clone(),
        pixel_aspect: settings.pixel_aspect,
    }
}

// ---- the export loop --------------------------------------------------------------

/// What to export.
#[derive(Clone, Copy)]
pub struct ExportJob<'a> {
    /// The project.
    pub stage: &'a Stage,
    /// The comp.
    pub comp: &'a PrimPath,
    /// Frames in output order: comp frame number (`None` for a free time) and
    /// comp time. More than one frame makes a sequence.
    pub times: &'a [(Option<i64>, Time)],
    /// The comp's frame rate (movies, sequence timecode).
    pub rate: FrameRate,
    /// Resolution, ROI, depth.
    pub request: &'a RenderRequest,
    /// Output.
    pub settings: &'a ExportSettings,
    /// Encoders (the app's registry); `None`: bae-media's built-in ones.
    pub encoders: Option<&'a Encoders>,
}

/// A frame handed to the encoder.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportedFrame {
    /// Index in [`ExportJob::times`].
    pub index: usize,
    /// Comp frame number.
    pub frame: Option<i64>,
    /// Comp time.
    pub time: Time,
    /// The file it goes to (the movie for movies).
    pub file: PathBuf,
    /// Size in pixels.
    pub width: u32,
    /// Size in pixels.
    pub height: u32,
    /// Evaluation time.
    pub eval: Duration,
    /// Graph build and GPU command encoding (CPU).
    pub render: Duration,
    /// Layer draws.
    pub draws: usize,
    /// GPU draw calls.
    pub draw_calls: usize,
}

/// Where an export's time went. `eval` … `drain` are the render thread's wall
/// time; `encode` and `write` are summed over the encoder threads (they run in
/// parallel, so they can exceed `wall`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExportStats {
    /// Frames exported.
    pub frames: usize,
    /// Evaluating the comp.
    pub eval: Duration,
    /// Building render graphs, encoding and submitting GPU commands.
    pub render: Duration,
    /// Waiting for the GPU (render, output conversion, copy) and the mapping.
    pub readback_wait: Duration,
    /// Copying frames out of the staging buffers (on the encoder thread for
    /// sequences and movies, so not on the render thread's critical path).
    pub readback_copy: Duration,
    /// Waiting for the encoder (its queue was full).
    pub encoder_stall: Duration,
    /// After the last frame: waiting for the encoder to finish.
    pub drain: Duration,
    /// Encoder: pixel conversion and compression.
    pub encode: Duration,
    /// Encoder: file writes.
    pub write: Duration,
    /// Files written.
    pub files: usize,
    /// Bytes written.
    pub bytes: u64,
    /// Encoder threads.
    pub encoder_threads: usize,
    /// First frame to last byte written.
    pub wall: Duration,
}

/// What an export produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportReport {
    /// Every frame, in order.
    pub frames: Vec<ExportedFrame>,
    /// Timing.
    pub stats: ExportStats,
    /// Things not rendered faithfully (deduplicated).
    pub warnings: Vec<String>,
    /// Stopped through the cancel flag: `frames` are the ones rendered before
    /// (all written), the rest of the range was not rendered.
    pub stopped: bool,
}

/// A frame submitted for readback, not yet handed to the sink.
struct Pending {
    index: usize,
    frame: Option<i64>,
    time: Time,
    eval: Duration,
    render: Duration,
    draws: usize,
    draw_calls: usize,
}

struct Run<'f> {
    readback: ExportReadback,
    sink: Option<Sink>,
    pending: VecDeque<Pending>,
    stats: ExportStats,
    frames: Vec<ExportedFrame>,
    on_frame: &'f mut dyn FnMut(&ExportedFrame),
}

impl Run<'_> {
    /// Hands a read-back frame to the sink.
    fn collect(&mut self, done: MappedFrame) -> Result<(), RenderError> {
        let pending = self
            .pending
            .pop_front()
            .ok_or_else(|| RenderError::Other("readback returned an unexpected frame".into()))?;
        self.stats.readback_wait += done.wait;
        let sink = self.sink.as_mut().expect("created with the first frame");
        let file = sink.file(pending.frame);
        let (width, height) = (done.width, done.height);
        let (stall, copy) = sink.push(done, &file)?;
        self.stats.encoder_stall += stall;
        self.stats.readback_copy += copy;
        let frame = ExportedFrame {
            index: pending.index,
            frame: pending.frame,
            time: pending.time,
            file,
            width,
            height,
            eval: pending.eval,
            render: pending.render,
            draws: pending.draws,
            draw_calls: pending.draw_calls,
        };
        (self.on_frame)(&frame);
        self.frames.push(frame);
        Ok(())
    }

    /// Collects the oldest frame, waiting for it. `false` when none is left.
    fn wait_one(&mut self) -> Result<bool, RenderError> {
        match self.readback.wait_next_mapped() {
            Some(done) => {
                self.collect(done?)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Collects the frames that are ready, without waiting.
    fn collect_ready(&mut self) -> Result<(), RenderError> {
        while let Some(done) = self.readback.try_next_mapped() {
            self.collect(done?)?;
        }
        Ok(())
    }
}

/// Exports `job`: evaluates and renders each frame, reads it back
/// asynchronously ([`ExportReadback`], [`READBACK_DEPTH`] in flight) and hands
/// it to the encoder on its own thread (bae-media's sequence writer compresses
/// and writes on every core). `on_frame` is called, in order, as each frame
/// is handed over. Setting `cancel` stops rendering new frames: the frames
/// already rendered are still written and the report says `stopped` (AE keeps
/// the frames rendered before Stop). On an error the files written so far
/// are kept and the error names the frame (render errors) or the file (write
/// errors).
pub fn export_frames(
    renderer: &mut Renderer,
    evaluator: &mut dyn CompEvaluator,
    job: &ExportJob<'_>,
    cancel: Option<&AtomicBool>,
    on_frame: &mut dyn FnMut(&ExportedFrame),
) -> Result<ExportReport, RenderError> {
    let started = Instant::now();
    let sequence = job.times.len() > 1;
    let float_work = renderer.work_format(job.request) == wgpu::TextureFormat::Rgba32Float;
    let mut run = Run {
        readback: ExportReadback::new(renderer.gpu().clone(), READBACK_DEPTH),
        sink: None,
        pending: VecDeque::with_capacity(READBACK_DEPTH + 1),
        stats: ExportStats::default(),
        frames: Vec::with_capacity(job.times.len()),
        on_frame,
    };
    let mut warnings: Vec<String> = Vec::new();
    let mut stopped = false;
    let result = (|| -> Result<(), RenderError> {
        for (index, (frame, time)) in job.times.iter().enumerate() {
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
                stopped = true;
                break;
            }
            let at = |error: RenderError| match frame {
                Some(f) => RenderError::Other(format!("frame {f}: {error}")),
                None => error,
            };
            let t0 = Instant::now();
            let rc = evaluator.evaluate(job.stage, job.comp, *time).map_err(at)?;
            let eval = t0.elapsed();
            let t1 = Instant::now();
            let rendered = renderer.render_frame(&rc, job.request).map_err(at)?;
            let render = t1.elapsed();
            run.stats.eval += eval;
            run.stats.render += render;
            for w in evaluator
                .take_warnings()
                .into_iter()
                .chain(rendered.warnings.iter().cloned())
            {
                if !warnings.contains(&w) {
                    warnings.push(w);
                }
            }
            if run.sink.is_none() {
                run.sink = Some(Sink::create(
                    job,
                    sequence,
                    rendered.image.texel_size(),
                    frame.unwrap_or_default(),
                    float_work,
                )?);
            }
            // Every staging buffer busy: collect the oldest frame first.
            if run.readback.is_full() {
                run.wait_one()?;
            }
            let sink = run.sink.as_ref().expect("created");
            let (format, alpha) = (sink.readback_format(), sink.alpha().output(rc.background));
            run.readback
                .submit(renderer, &rendered, alpha, format, index as u64)?;
            run.pending.push_back(Pending {
                index,
                frame: *frame,
                time: *time,
                eval,
                render,
                draws: rendered.stats.draws,
                draw_calls: rendered.stats.draw_calls,
            });
            drop(rendered);
            run.collect_ready()?;
        }
        // Frames already rendered (in flight on the GPU) are written too.
        while run.wait_one()? {}
        Ok(())
    })();
    if let Err(error) = result {
        if let Some(sink) = run.sink.take() {
            sink.close();
        }
        return Err(error);
    }
    let drain = Instant::now();
    let written = match run.sink.take() {
        Some(sink) => sink.finish(job.times.len())?,
        None => SinkReport::default(),
    };
    let mut stats = run.stats;
    stats.drain = drain.elapsed();
    stats.frames = run.frames.len();
    stats.readback_copy += written.copy;
    stats.encode = written.encode;
    stats.write = written.write;
    stats.files = written.files;
    stats.bytes = written.bytes;
    stats.encoder_threads = written.threads;
    stats.wall = started.elapsed();
    Ok(ExportReport {
        frames: run.frames,
        stats,
        warnings,
        stopped,
    })
}
