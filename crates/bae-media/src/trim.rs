//! Clip trimming for project packaging (workstream 21, ARCHITECTURE §16).
//! Reserved: the parameters are fixed here so 21 can build against them; the
//! implementation (passthrough on GOP boundaries, re-encode fallback) lands in a
//! later PR together with the encoders.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::source::MediaError;

/// How a trimmed clip is produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrimMode {
    /// Copy compressed samples without re-encoding; the range is widened to the
    /// enclosing keyframes (GOP boundaries). Falls back to re-encoding when the
    /// container cannot be copied.
    #[default]
    PreferPassthrough,
    /// Always copy compressed samples (fail if impossible).
    Passthrough,
    /// Re-encode exactly the requested frames into an intermediate codec.
    Reencode,
}

/// One clip to cut out of a source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimRequest {
    /// Source file.
    pub source: PathBuf,
    /// Output file (same container type as the source for passthrough).
    pub output: PathBuf,
    /// First source frame needed (inclusive).
    pub first_frame: i64,
    /// Last source frame needed (inclusive).
    pub last_frame: i64,
    /// Extra frames kept before and after (handles).
    pub handles: u32,
    /// Passthrough or re-encode.
    pub mode: TrimMode,
}

/// Result of a trim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimResult {
    /// The written file.
    pub output: PathBuf,
    /// Source frame that is frame 0 of the output (the project reference gets
    /// this offset).
    pub source_offset: i64,
    /// Frames in the output.
    pub frame_count: u64,
    /// Whether samples were copied (`true`) or re-encoded.
    pub passthrough: bool,
}

/// Cuts clips out of media files.
pub trait Trimmer: Send + Sync {
    /// Produces the clip.
    fn trim(&self, request: &TrimRequest) -> Result<TrimResult, MediaError>;
}
