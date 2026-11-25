//! BAE media I/O (workstream 08): probing, metadata and decoding of images,
//! image sequences and video into GPU textures.
//!
//! # Shape of the API
//!
//! * [`MediaFormats`] finds the [`MediaFormat`] for a path (magic bytes, then
//!   extension) and opens it — as a single file or, with
//!   [`SequenceMode::Sequence`], as the image sequence it belongs to.
//! * A [`MediaSource`] reports [`MediaInfo`] (size, frame rate, frame count,
//!   pixel aspect, color tags, alpha mode, timecode) and decodes any frame by
//!   index: [`MediaSource::decode_gpu`] → [`GpuFrame`] (an `Rgba16Float` texture
//!   on the shared device, premultiplied by default), or
//!   [`MediaSource::decode_cpu`] → [`CpuFrame`].
//! * [`MediaGpu`] (one per `GpuContext`) uploads CPU frames and converts
//!   decoder textures to the working format with a compute pass.
//! * [`FrameCache`] is an LRU of decoded GPU frames bounded by memory.
//! * [`MediaModule`] registers the built-in formats into the app's
//!   `Registry<dyn MediaFormat>`, where plugins add theirs.
//!
//! Time is `bae-model`'s [`Time`] (integer flicks) and [`FrameRate`]; frame
//! indices are 0-based from the source's first frame.
//!
//! ```no_run
//! use std::path::Path;
//! use bae_media::{FrameRequest, MediaFormats, OpenOptions};
//!
//! let formats = MediaFormats::builtin();
//! let mut source = formats.open(Path::new("shot_0001.exr"), &OpenOptions::default())?;
//! println!("{:?}", source.info());
//! let frame = source.decode_cpu(&FrameRequest::new(0))?;
//! # Ok::<(), bae_media::MediaError>(())
//! ```
//!
//! # Formats
//!
//! | Kind | Formats | Implementation |
//! |---|---|---|
//! | Still / sequence | PNG, JPEG, WebP, OpenEXR | pure Rust (`png`, `zune-jpeg`, `image-webp`, `exr`) |
//! | Video (macOS) | H.264, HEVC, ProRes (QuickTime / MP4) | AVFoundation / VideoToolbox, IOSurface → Metal texture, zero copy ([`apple`]) |
//! | Video (Windows) | H.264, HEVC | Media Foundation / D3D11, one upload into wgpu |
//!
//! Export: [`export`] (encoders, async GPU readback, render loop, render
//! queue); PNG / TIFF / OpenEXR sequence encoders built in. Audio: [`audio`]
//! (sample access, waveforms). Packaging: [`trim`] (time-range clips on GOP
//! boundaries, keyframe lookup). Proxies are reserved
//! ([`FrameRequest::downscale`]).

#[cfg(target_vendor = "apple")]
pub mod apple;
pub mod audio;
mod cache;
pub mod export;
mod frame;
mod gpu;
mod library;
#[cfg(windows)]
pub mod media_foundation;
mod meta;
mod module;
mod mp4;
pub mod sequence;
mod source;
pub mod still;
pub mod time;
pub mod trim;

use std::sync::Arc;

pub use cache::{CacheStats, FrameCache, FrameKey, source_id_for};
pub use frame::{AlphaOutput, CpuFrame, FrameRequest, GpuFrame, PixelFormat, WORKING_FORMAT};
pub use gpu::{ExportTexture, MediaGpu, SourceTexture};
pub use library::{DEFAULT_OPEN_LIMIT, MediaLibrary, MediaLocation};
pub use meta::{
    AlphaMode, AudioInfo, Chromaticities, ColorInfo, ColorPrimaries, ColorRange,
    MatrixCoefficients, MediaInfo, MediaKind, PixelAspect, SampleType, SequenceInfo,
    TransferFunction, VideoInfo,
};
pub use module::{DEFAULT_CACHE_BUDGET, MediaModule};
pub use sequence::{ImageSequence, ImageSequenceSource, detect_sequence};
pub use source::{MediaError, MediaFormat, MediaFormats, MediaSource, OpenOptions, SequenceMode};
pub use time::{FrameRate, Time, Timecode};

pub use audio::{AudioBuffer, AudioSource, FileAudio, MemoryAudio, Waveform};
pub use export::{
    CancelToken, EncodeOutput, EncodeSettings, EncodeStats, Encoder, EncoderFormat, ExrCompression,
    FrameProducer, FrameReadback, MediaEncoders, PngCompression, ProResProfile, ProducedFrame,
    RenderJob, RenderJobId, RenderProgress, RenderQueue, RenderStatus, TiffCompression, VideoCodec,
};
pub use trim::{TimeRange, TrimMode, TrimPlan, TrimRequest, TrimResult, Trimmer, plan_trim};

/// The formats compiled into this crate for the current platform, in
/// registration order.
pub fn builtin_formats() -> Vec<Arc<dyn MediaFormat>> {
    #[allow(unused_mut)]
    let mut formats = still::formats();
    #[cfg(target_vendor = "apple")]
    formats.push(Arc::new(apple::AvFoundationFormat));
    #[cfg(windows)]
    formats.push(Arc::new(media_foundation::MediaFoundationFormat));
    formats
}

/// The encoders compiled into this crate for the current platform.
pub fn builtin_encoders() -> Vec<Arc<dyn EncoderFormat>> {
    #[allow(unused_mut)]
    let mut encoders: Vec<Arc<dyn EncoderFormat>> = vec![Arc::new(export::ImageSequenceFormat)];
    #[cfg(target_vendor = "apple")]
    encoders.push(Arc::new(apple::AvWriterFormat));
    #[cfg(windows)]
    encoders.push(Arc::new(media_foundation::MfWriterFormat));
    encoders
}

/// The trimmers compiled into this crate for the current platform.
pub fn builtin_trimmers() -> Vec<Arc<dyn Trimmer>> {
    #[allow(unused_mut)]
    let mut trimmers: Vec<Arc<dyn Trimmer>> = vec![Arc::new(trim::SequenceTrimmer::new(
        time::DEFAULT_SEQUENCE_RATE,
    ))];
    #[cfg(target_vendor = "apple")]
    trimmers.push(Arc::new(apple::AvTrimmer));
    trimmers
}
