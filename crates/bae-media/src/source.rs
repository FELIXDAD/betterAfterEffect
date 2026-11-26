//! The decoder-facing traits: [`MediaFormat`] (a file type that can be probed
//! and opened) and [`MediaSource`] (an opened file with random frame access),
//! plus [`MediaFormats`], the lookup used to open a path.

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_gpu::GpuError;

use crate::frame::{CpuFrame, FrameRequest, GpuFrame};
use crate::gpu::MediaGpu;
use crate::meta::{MediaInfo, MediaKind};
use crate::sequence::{self, ImageSequenceSource};
use crate::time::{DEFAULT_SEQUENCE_RATE, FrameRate, Time};

/// Errors from probing, opening and decoding media.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MediaError {
    /// Reading the file failed.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// Writing an output file (or creating its directory) failed.
    #[error("cannot write {path}: {source}")]
    Write {
        /// The file or directory.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// No registered format recognizes the file.
    #[error("unsupported media format: {0}")]
    UnsupportedFormat(String),
    /// The file is recognized but broken or uses an unsupported variant.
    #[error("cannot decode {path}: {message}")]
    Decode {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// The requested frame index is outside the source.
    #[error("frame {index} is outside the source (0..{count})")]
    FrameOutOfRange {
        /// Requested index.
        index: i64,
        /// Number of frames in the source.
        count: u64,
    },
    /// A frame of an image sequence has no file (a gap in the numbering).
    #[error("frame {number} of the image sequence is missing: {path}")]
    MissingFrame {
        /// The missing frame number (as in the file name).
        number: i64,
        /// The path that was expected.
        path: PathBuf,
    },
    /// GPU upload or conversion failed.
    #[error(transparent)]
    Gpu(#[from] GpuError),
    /// The operation is not supported by this source or platform (yet).
    #[error("not supported: {0}")]
    Unsupported(String),
    /// The system cannot decode the requested HEVC video.
    #[error(
        "HEVC decoding is unsupported. Check that Windows HEVC Video Extensions are installed."
    )]
    HevcDecoderUnavailable,
    /// The operation was cancelled (render queue).
    #[error("cancelled")]
    Canceled,
    /// The platform media framework (AVFoundation, VideoToolbox, Media
    /// Foundation) reported an error.
    #[error("{framework}: {message}")]
    Platform {
        /// Framework name.
        framework: &'static str,
        /// Error description (including the OSStatus / HRESULT).
        message: String,
    },
}

impl MediaError {
    /// A [`MediaError::Decode`] for `path`.
    pub fn decode(path: &Path, message: impl fmt::Display) -> Self {
        MediaError::Decode {
            path: path.to_owned(),
            message: message.to_string(),
        }
    }

    /// A [`MediaError::Io`] for `path`.
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        MediaError::Io {
            path: path.to_owned(),
            source,
        }
    }

    /// A [`MediaError::Write`] for `path`.
    pub fn write(path: &Path, source: std::io::Error) -> Self {
        MediaError::Write {
            path: path.to_owned(),
            source,
        }
    }
}

/// An opened media file with frame-accurate random access.
///
/// Sources are stateful (a video decoder keeps its read position), so decoding
/// takes `&mut self`; share one between threads behind a mutex, or open the file
/// once per thread.
pub trait MediaSource: Send {
    /// Metadata, available immediately after opening.
    fn info(&self) -> &MediaInfo;

    /// Decodes frame `request.index` into CPU memory.
    ///
    /// Hardware video decoders may return [`MediaError::Unsupported`]; use
    /// [`Self::decode_gpu`] there.
    fn decode_cpu(&mut self, request: &FrameRequest) -> Result<CpuFrame, MediaError>;

    /// Decodes frame `request.index` into an `Rgba16Float` texture on `gpu`'s
    /// device. The default decodes on the CPU and uploads; hardware decoders
    /// override it to stay on the GPU.
    fn decode_gpu(
        &mut self,
        request: &FrameRequest,
        gpu: &MediaGpu,
    ) -> Result<GpuFrame, MediaError> {
        let frame = self.decode_cpu(request)?;
        gpu.upload(&frame, request)
    }

    /// The frame index shown at `time` (measured from the start of the source).
    /// Stills always return 0.
    fn frame_at(&self, time: Time) -> i64 {
        match self.info().video.as_ref().and_then(|v| v.frame_rate) {
            Some(rate) if self.info().kind != MediaKind::Still => rate.time_to_frame(time),
            _ => 0,
        }
    }

    /// Frame indices that can be decoded without earlier frames (keyframes /
    /// sync samples), for trimming on GOP boundaries (workstream 21). `None` when
    /// unknown; every frame of an image source is a sync frame.
    fn sync_frames(&mut self) -> Option<Vec<i64>> {
        None
    }
}

impl fmt::Debug for dyn MediaSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaSource")
            .field("info", self.info())
            .finish_non_exhaustive()
    }
}

/// Checks `index` against the source's frame count.
pub(crate) fn check_index(info: &MediaInfo, index: i64) -> Result<(), MediaError> {
    let count = info.video.as_ref().and_then(|v| v.frame_count);
    match count {
        Some(count) if index < 0 || index as u64 >= count => {
            Err(MediaError::FrameOutOfRange { index, count })
        }
        _ => Ok(()),
    }
}

/// How [`MediaFormats::open`] treats numbered file names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SequenceMode {
    /// Open exactly the given file (AE: "sequence" checkbox off).
    #[default]
    Single,
    /// Open the image sequence the file belongs to (AE: "PNG Sequence" on).
    /// Falls back to a single still if the name has no frame number.
    Sequence,
    /// All stills with this extension in the same directory, in filename order.
    Alphabetical,
}

/// Options for opening a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpenOptions {
    /// Single file or image sequence.
    pub sequence: SequenceMode,
    /// Frame rate assigned to image sequences (AE preference "Sequence Footage
    /// frames per second", default 30).
    pub sequence_frame_rate: FrameRate,
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions {
            sequence: SequenceMode::Single,
            sequence_frame_rate: DEFAULT_SEQUENCE_RATE,
        }
    }
}

impl OpenOptions {
    /// Options opening the image sequence a file belongs to.
    pub fn sequence(rate: FrameRate) -> Self {
        OpenOptions {
            sequence: SequenceMode::Sequence,
            sequence_frame_rate: rate,
        }
    }
}

/// A media file type: probing and opening. Built-in formats and plugins (e.g.
/// the optional FFmpeg plugin) implement it and register into
/// `bae_app::Registry<dyn MediaFormat>` (see [`MediaModule`](crate::MediaModule)).
pub trait MediaFormat: Send + Sync {
    /// Stable id, e.g. `"png"`, `"avfoundation"`.
    fn id(&self) -> &str;

    /// Lower-case file extensions without the dot.
    fn extensions(&self) -> &[&str];

    /// `true` if the first bytes of a file (up to 64) look like this format.
    fn sniff(&self, header: &[u8]) -> bool;

    /// `true` for single-image formats that can form image sequences.
    fn is_image(&self) -> bool;

    /// Opens `path` as a single file (the caller handles sequences).
    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError>;

    /// Selection priority when several formats claim a file (higher wins).
    /// Built-ins use 0; a plugin overriding a built-in uses more, a long-tail
    /// fallback (FFmpeg) uses less.
    fn priority(&self) -> i32 {
        0
    }
}

impl fmt::Debug for dyn MediaFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaFormat")
            .field("id", &self.id())
            .finish_non_exhaustive()
    }
}

/// A set of formats to open files with.
#[derive(Clone, Default)]
pub struct MediaFormats {
    formats: Vec<Arc<dyn MediaFormat>>,
}

impl fmt::Debug for MediaFormats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.formats.iter().map(|format| format.id()))
            .finish()
    }
}

impl MediaFormats {
    /// The formats compiled into this crate for the current platform.
    pub fn builtin() -> Self {
        Self::from_formats(crate::builtin_formats())
    }

    /// A set from explicit formats.
    pub fn from_formats(formats: impl IntoIterator<Item = Arc<dyn MediaFormat>>) -> Self {
        let mut formats: Vec<_> = formats.into_iter().collect();
        // Stable: equal priorities keep registration order.
        formats.sort_by_key(|format| std::cmp::Reverse(format.priority()));
        MediaFormats { formats }
    }

    /// A snapshot of the app's format registry (built-ins plus plugins).
    pub fn from_registry(registry: &bae_app::Registry<dyn MediaFormat>) -> Self {
        Self::from_formats(registry.snapshot().iter().map(|entry| entry.item().clone()))
    }

    /// The formats, highest priority first.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn MediaFormat>> + '_ {
        self.formats.iter()
    }

    /// The format for `path`: by content (magic bytes) first, then by extension.
    pub fn find(&self, path: &Path) -> Result<Arc<dyn MediaFormat>, MediaError> {
        let mut header = [0_u8; 64];
        let len = read_header(path, &mut header)?;
        let header = &header[..len];
        if let Some(format) = self.formats.iter().find(|format| format.sniff(header)) {
            return Ok(format.clone());
        }
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        self.formats
            .iter()
            .find(|format| format.extensions().contains(&ext.as_str()))
            .cloned()
            .ok_or_else(|| MediaError::UnsupportedFormat(path.display().to_string()))
    }

    /// Opens `path` (or the image sequence it belongs to).
    pub fn open(
        &self,
        path: &Path,
        options: &OpenOptions,
    ) -> Result<Box<dyn MediaSource>, MediaError> {
        let format = self.find(path)?;
        if options.sequence == SequenceMode::Alphabetical && format.is_image() {
            let seq = sequence::alphabetical_sequence(path)?;
            return Ok(Box::new(ImageSequenceSource::open(
                format,
                seq,
                options.sequence_frame_rate,
            )?));
        }
        if options.sequence == SequenceMode::Sequence
            && format.is_image()
            && let Some(seq) = sequence::detect_sequence(path)?
        {
            let source = ImageSequenceSource::open(format, seq, options.sequence_frame_rate)?;
            return Ok(Box::new(source));
        }
        format.open(path)
    }
}

/// `true` for ISO base media / QuickTime files (a box type at offset 4).
pub(crate) fn sniff_iso_bmff(header: &[u8]) -> bool {
    header.len() >= 8
        && matches!(
            &header[4..8],
            b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip" | b"pnot"
        )
}

fn read_header(path: &Path, buf: &mut [u8]) -> Result<usize, MediaError> {
    let mut file = std::fs::File::open(path).map_err(|e| MediaError::io(path, e))?;
    let mut len = 0;
    while len < buf.len() {
        match file.read(&mut buf[len..]) {
            Ok(0) => break,
            Ok(n) => len += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(MediaError::io(path, e)),
        }
    }
    Ok(len)
}
