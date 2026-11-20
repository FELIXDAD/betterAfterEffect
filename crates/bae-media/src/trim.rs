//! Clip trimming for project packaging (workstream 21, ARCHITECTURE §16):
//! copy only the part of a source that the timeline uses.
//!
//! * [`TrimRequest`] names a **source time range** (what the timeline samples,
//!   after time remapping, nested comps and merging overlaps — 21 computes it),
//!   handles, and a [`TrimMode`].
//! * [`plan_trim`] turns it into whole frames and, for passthrough, widens the
//!   range to whole GOPs using the source's keyframes
//!   ([`MediaSource::sync_frames`](crate::MediaSource::sync_frames)):
//!   lossless copies must start on a keyframe and should end right before the
//!   next one.
//! * A [`Trimmer`] does the work. [`SequenceTrimmer`] copies the used files of
//!   an image sequence (every frame is a keyframe); movie trimmers (AVFoundation
//!   passthrough, re-encode) live in the platform modules.
//!
//! Trimming never modifies the source; the result tells 21 where output frame
//! 0 sits in the source ([`TrimResult::source_range`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::meta::{MediaInfo, MediaKind};
use crate::sequence::detect_sequence;
use crate::source::MediaError;
use crate::time::{FrameRate, Time};

pub use bae_model::TimeRange;

/// How a trimmed clip is produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrimMode {
    /// Copy compressed samples on GOP boundaries when keyframes are known,
    /// otherwise re-encode.
    #[default]
    PreferPassthrough,
    /// Always copy compressed samples (error when impossible).
    Passthrough,
    /// Re-encode exactly the requested frames into an intermediate codec.
    Reencode,
}

/// One clip to cut out of a source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimRequest {
    /// Source file (for sequences: any frame of it).
    pub source: PathBuf,
    /// Output file; for image sequences the output directory.
    pub output: PathBuf,
    /// Source time needed, half-open `[start, end)`, measured from the source's
    /// first frame.
    pub range: TimeRange,
    /// Extra source time kept before and after the range.
    pub handles: Time,
    /// Passthrough or re-encode.
    pub mode: TrimMode,
}

/// Frames a trim will write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrimPlan {
    /// First source frame written.
    pub first_frame: i64,
    /// Last source frame written (inclusive).
    pub last_frame: i64,
    /// Copy compressed samples (`true`) or re-encode.
    pub passthrough: bool,
}

impl TrimPlan {
    /// Frames written.
    pub fn frame_count(&self) -> u64 {
        (self.last_frame - self.first_frame + 1).max(0) as u64
    }

    /// The source time the output covers.
    pub fn source_range(&self, rate: FrameRate) -> TimeRange {
        TimeRange::new(
            rate.frame_to_time(self.first_frame),
            rate.frame_to_time(self.last_frame + 1),
        )
    }
}

/// Result of a trim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimResult {
    /// Written files (one movie, or the sequence's frames).
    pub files: Vec<PathBuf>,
    /// Source time covered by the output: output time 0 is
    /// `source_range.start` in the source (the offset for the project
    /// reference).
    pub source_range: TimeRange,
    /// First source frame in the output.
    pub first_frame: i64,
    /// Frames in the output.
    pub frame_count: u64,
    /// Whether samples were copied (`true`) or re-encoded.
    pub passthrough: bool,
}

/// Cuts clips out of media files.
pub trait Trimmer: Send + Sync {
    /// Stable id.
    fn id(&self) -> &str;

    /// Whether this trimmer handles the source.
    fn supports(&self, info: &MediaInfo) -> bool;

    /// Produces the clip.
    fn trim(&self, request: &TrimRequest) -> Result<TrimResult, MediaError>;
}

/// The keyframe at or before `frame` (start of its GOP). `sync_frames` must be
/// ascending; frames before the first keyframe map to it.
pub fn gop_start(sync_frames: &[i64], frame: i64) -> Option<i64> {
    match sync_frames.binary_search(&frame) {
        Ok(i) => Some(sync_frames[i]),
        Err(0) => sync_frames.first().copied(),
        Err(i) => Some(sync_frames[i - 1]),
    }
}

/// The first keyframe after `frame` (start of the next GOP), if any.
pub fn next_gop_start(sync_frames: &[i64], frame: i64) -> Option<i64> {
    let i = sync_frames.partition_point(|&k| k <= frame);
    sync_frames.get(i).copied()
}

/// Plans a trim of a source with `info` and (if known) its keyframes.
pub fn plan_trim(
    info: &MediaInfo,
    sync_frames: Option<&[i64]>,
    request: &TrimRequest,
) -> Result<TrimPlan, MediaError> {
    let video = info
        .video
        .as_ref()
        .ok_or_else(|| MediaError::Unsupported("trimming a source without a picture".into()))?;
    if info.kind == MediaKind::Still {
        return Ok(TrimPlan {
            first_frame: 0,
            last_frame: 0,
            passthrough: true,
        });
    }
    let rate = video
        .frame_rate
        .ok_or_else(|| MediaError::Unsupported("trimming a source without a frame rate".into()))?;
    let count = video.frame_count.unwrap_or(u64::MAX).min(i64::MAX as u64) as i64;
    if request.range.is_empty() {
        return Err(MediaError::Unsupported("empty trim range".into()));
    }
    let start = request.range.start.saturating_sub(request.handles);
    let end = request.range.end.saturating_add(request.handles);
    let mut first = rate.time_to_frame(start).max(0);
    // `end` is exclusive: the last frame is the one containing `end - 1 flick`.
    let mut last = rate
        .time_to_frame(end.saturating_sub(Time::from_flicks(1)))
        .min(count - 1);
    if first > last {
        return Err(MediaError::Unsupported(format!(
            "trim range outside the source ({count} frames)"
        )));
    }
    let passthrough = match (request.mode, sync_frames) {
        (TrimMode::Reencode, _) => false,
        (_, Some(sync)) if !sync.is_empty() => {
            first = gop_start(sync, first).unwrap_or(0);
            last = next_gop_start(sync, last).map_or(count - 1, |next| (next - 1).min(count - 1));
            true
        }
        (TrimMode::Passthrough, _) => {
            return Err(MediaError::Unsupported(
                "passthrough trim needs the source's keyframes".into(),
            ));
        }
        (TrimMode::PreferPassthrough, _) => false,
    };
    Ok(TrimPlan {
        first_frame: first,
        last_frame: last,
        passthrough,
    })
}

/// Trims image sequences by copying the used frame files (names kept, so the
/// sequence's numbering — and the project reference — stays valid). Missing
/// frames in the range are skipped.
#[derive(Debug, Default, Clone, Copy)]
pub struct SequenceTrimmer {
    /// Frame rate used to turn the request's times into frames.
    pub frame_rate: Option<FrameRate>,
}

impl SequenceTrimmer {
    /// Trims with the AE default sequence rate unless the request's source info
    /// says otherwise.
    pub fn new(frame_rate: FrameRate) -> Self {
        SequenceTrimmer {
            frame_rate: Some(frame_rate),
        }
    }
}

impl Trimmer for SequenceTrimmer {
    fn id(&self) -> &str {
        "image-sequence"
    }

    fn supports(&self, info: &MediaInfo) -> bool {
        info.kind == MediaKind::ImageSequence
    }

    fn trim(&self, request: &TrimRequest) -> Result<TrimResult, MediaError> {
        let sequence = detect_sequence(&request.source)?.ok_or_else(|| {
            MediaError::Unsupported(format!(
                "{} is not part of an image sequence",
                request.source.display()
            ))
        })?;
        let rate = self
            .frame_rate
            .unwrap_or(crate::time::DEFAULT_SEQUENCE_RATE);
        let info = sequence_info(&request.source, &sequence, rate);
        // Every frame of a sequence is a keyframe: passthrough is exact.
        let plan = plan_trim(
            &info,
            None,
            &TrimRequest {
                mode: TrimMode::Reencode,
                ..request.clone()
            },
        )?;
        std::fs::create_dir_all(&request.output).map_err(|e| MediaError::io(&request.output, e))?;
        let mut files = Vec::new();
        for index in plan.first_frame..=plan.last_frame {
            let number = sequence.first() + index;
            if !sequence.contains(number) {
                continue;
            }
            let from = sequence.path_for(number);
            let to = request
                .output
                .join(from.file_name().expect("sequence paths have names"));
            std::fs::copy(&from, &to).map_err(|e| MediaError::io(&from, e))?;
            files.push(to);
        }
        Ok(TrimResult {
            files,
            source_range: plan.source_range(rate),
            first_frame: plan.first_frame,
            frame_count: plan.frame_count(),
            passthrough: true,
        })
    }
}

fn sequence_info(path: &Path, sequence: &crate::ImageSequence, rate: FrameRate) -> MediaInfo {
    MediaInfo {
        path: path.to_owned(),
        kind: MediaKind::ImageSequence,
        format: String::new(),
        video: Some(crate::VideoInfo {
            width: 0,
            height: 0,
            pixel_aspect: crate::PixelAspect::SQUARE,
            frame_rate: Some(rate),
            frame_count: Some(sequence.len()),
            bit_depth: 8,
            sample_type: crate::SampleType::UInt,
            color: crate::ColorInfo::default(),
            alpha: crate::AlphaMode::Opaque,
            codec: String::new(),
        }),
        audio: Vec::new(),
        timecode: None,
        sequence: Some(sequence.info()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(frames: u64) -> MediaInfo {
        let mut info = sequence_info(
            Path::new("a.mov"),
            &crate::ImageSequence {
                dir: PathBuf::new(),
                prefix: String::new(),
                suffix: String::new(),
                padding: 0,
                frames: vec![0, frames as i64 - 1],
            },
            FrameRate::FPS_25,
        );
        info.kind = MediaKind::Movie;
        info
    }

    fn request(start: i64, end: i64, mode: TrimMode) -> TrimRequest {
        let rate = FrameRate::FPS_25;
        TrimRequest {
            source: "a.mov".into(),
            output: "b.mov".into(),
            range: TimeRange::new(rate.frame_to_time(start), rate.frame_to_time(end)),
            handles: Time::from_flicks(0),
            mode,
        }
    }

    #[test]
    fn gop_lookup() {
        let sync = [0, 12, 24, 36];
        assert_eq!(gop_start(&sync, 0), Some(0));
        assert_eq!(gop_start(&sync, 13), Some(12));
        assert_eq!(gop_start(&sync, 24), Some(24));
        assert_eq!(gop_start(&sync, 99), Some(36));
        assert_eq!(next_gop_start(&sync, 12), Some(24));
        assert_eq!(next_gop_start(&sync, 23), Some(24));
        assert_eq!(next_gop_start(&sync, 36), None);
    }

    #[test]
    fn passthrough_widens_to_gops() {
        let info = movie(48);
        let sync = [0_i64, 12, 24, 36];
        // Frames 14..=29 → GOPs 12..=35.
        let plan = plan_trim(
            &info,
            Some(&sync),
            &request(14, 30, TrimMode::PreferPassthrough),
        )
        .unwrap();
        assert_eq!(
            (plan.first_frame, plan.last_frame, plan.passthrough),
            (12, 35, true)
        );
        assert_eq!(plan.frame_count(), 24);
        assert_eq!(
            plan.source_range(FrameRate::FPS_25),
            TimeRange::new(
                FrameRate::FPS_25.frame_to_time(12),
                FrameRate::FPS_25.frame_to_time(36)
            )
        );
        // Last GOP runs to the end of the source.
        let plan = plan_trim(&info, Some(&sync), &request(40, 41, TrimMode::Passthrough)).unwrap();
        assert_eq!((plan.first_frame, plan.last_frame), (36, 47));
    }

    #[test]
    fn reencode_is_exact_and_handles_clamp() {
        let info = movie(48);
        let plan = plan_trim(&info, Some(&[0, 12]), &request(14, 30, TrimMode::Reencode)).unwrap();
        assert_eq!(
            (plan.first_frame, plan.last_frame, plan.passthrough),
            (14, 29, false)
        );
        let mut req = request(2, 46, TrimMode::Reencode);
        req.handles = FrameRate::FPS_25.frame_to_time(5);
        let plan = plan_trim(&info, None, &req).unwrap();
        assert_eq!((plan.first_frame, plan.last_frame), (0, 47));
        // Unknown keyframes.
        assert!(plan_trim(&info, None, &request(1, 2, TrimMode::Passthrough)).is_err());
        assert!(
            !plan_trim(&info, None, &request(1, 2, TrimMode::PreferPassthrough))
                .unwrap()
                .passthrough
        );
        assert!(plan_trim(&info, None, &request(60, 70, TrimMode::Reencode)).is_err());
    }

    #[test]
    fn sequence_trim_copies_used_frames() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        for n in [100, 101, 102, 104, 105, 106] {
            std::fs::write(src.path().join(format!("f_{n:04}.png")), [n as u8]).unwrap();
        }
        let rate = FrameRate::FPS_25;
        let trimmer = SequenceTrimmer::new(rate);
        let result = trimmer
            .trim(&TrimRequest {
                source: src.path().join("f_0100.png"),
                output: out.path().join("clip"),
                // Indices 1..5 → files 101..=104 (103 missing).
                range: TimeRange::new(rate.frame_to_time(1), rate.frame_to_time(5)),
                handles: Time::from_flicks(0),
                mode: TrimMode::PreferPassthrough,
            })
            .unwrap();
        let names: Vec<_> = result
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["f_0101.png", "f_0102.png", "f_0104.png"]);
        assert_eq!((result.first_frame, result.frame_count), (1, 4));
        assert_eq!(
            std::fs::read(out.path().join("clip/f_0104.png")).unwrap(),
            [104]
        );
    }
}
