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
//! | Video (Windows) | H.264, HEVC | Media Foundation (placeholder; next PR) |
//!
//! Reserved interfaces for later PRs: [`export`] (encoders, render queue
//! status), [`audio`], [`trim`] (clip trimming for packaging), proxies
//! ([`FrameRequest::downscale`]).

#[cfg(target_vendor = "apple")]
pub mod apple;
pub mod audio;
mod cache;
pub mod export;
mod frame;
mod gpu;
#[cfg(windows)]
pub mod media_foundation;
mod meta;
mod module;
pub mod sequence;
mod source;
pub mod still;
pub mod time;
pub mod trim;

use std::sync::Arc;

pub use cache::{CacheStats, FrameCache, FrameKey, source_id_for};
pub use frame::{AlphaOutput, CpuFrame, FrameRequest, GpuFrame, PixelFormat, WORKING_FORMAT};
pub use gpu::{MediaGpu, SourceTexture};
pub use meta::{
    AlphaMode, AudioInfo, Chromaticities, ColorInfo, ColorPrimaries, ColorRange,
    MatrixCoefficients, MediaInfo, MediaKind, PixelAspect, SampleType, SequenceInfo,
    TransferFunction, VideoInfo,
};
pub use module::{DEFAULT_CACHE_BUDGET, MediaModule};
pub use sequence::{ImageSequence, ImageSequenceSource, detect_sequence};
pub use source::{MediaError, MediaFormat, MediaFormats, MediaSource, OpenOptions, SequenceMode};
pub use time::{FrameRate, Time, Timecode};

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
