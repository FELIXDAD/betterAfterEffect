//! Encoding / export interface (reserved: implementations land in a later PR).
//!
//! The render queue hands rendered frames to an [`Encoder`]. GPU frames are read
//! back asynchronously with double buffering (`bae_gpu::transfer`, ARCHITECTURE
//! §5) before they reach the encoder, so `push_frame` takes CPU pixels. Hardware
//! encoders (VideoToolbox, Media Foundation) may later get a GPU entry point.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::frame::CpuFrame;
use crate::meta::{AlphaMode, ColorInfo};
use crate::source::MediaError;
use crate::time::{FrameRate, Time};

/// Output container / file type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Container {
    /// QuickTime `.mov`.
    QuickTime,
    /// MPEG-4 `.mp4`.
    Mp4,
    /// Numbered PNG files.
    PngSequence,
    /// Numbered OpenEXR files.
    ExrSequence,
}

/// Video codec of an export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VideoCodec {
    /// H.264 / AVC (hardware encoder).
    H264,
    /// H.265 / HEVC (hardware encoder; HEVC with alpha on macOS).
    Hevc,
    /// Apple ProRes 422 family (macOS native).
    ProRes422(ProResFlavor),
    /// Apple ProRes 4444 (with alpha).
    ProRes4444,
    /// Uncompressed / format-defined (image sequences).
    Image,
}

/// ProRes 422 variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProResFlavor {
    /// ProRes 422 Proxy.
    Proxy,
    /// ProRes 422 LT.
    Lt,
    /// ProRes 422.
    Standard,
    /// ProRes 422 HQ.
    Hq,
}

/// What to write.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EncodeSettings {
    /// Output path (for sequences: the first frame's path; numbering follows the
    /// [`ImageSequence`](crate::ImageSequence) rules).
    pub path: PathBuf,
    /// Container.
    pub container: Container,
    /// Codec.
    pub codec: VideoCodec,
    /// Frame size.
    pub width: u32,
    /// Frame size.
    pub height: u32,
    /// Frame rate.
    pub frame_rate: FrameRate,
    /// Alpha in the output ([`AlphaMode::Opaque`] drops it).
    pub alpha: AlphaMode,
    /// Color tags written to the file.
    pub color: ColorInfo,
    /// Target bit rate in bits per second (`None`: codec default / quality based).
    pub bit_rate: Option<u64>,
    /// Quality 0–1 for quality-based encoders.
    pub quality: Option<f32>,
}

/// Writes frames to a file.
pub trait Encoder: Send {
    /// Appends a frame presented at `time` (from the start of the output).
    fn push_frame(&mut self, frame: &CpuFrame, time: Time) -> Result<(), MediaError>;

    /// Flushes and closes the file.
    fn finish(self: Box<Self>) -> Result<(), MediaError>;
}

/// An encoder implementation (built-in or plugin) registered under an id.
pub trait EncoderFactory: Send + Sync {
    /// Stable id (`"videotoolbox"`, `"png-sequence"` …).
    fn id(&self) -> &str;

    /// Whether this factory can write `settings`.
    fn supports(&self, settings: &EncodeSettings) -> bool;

    /// Creates the encoder.
    fn create(&self, settings: &EncodeSettings) -> Result<Box<dyn Encoder>, MediaError>;
}

/// Progress of a render-queue item.
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
    /// Failed (the message is kept by the queue).
    Failed,
}
