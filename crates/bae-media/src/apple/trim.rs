//! Movie trimming on macOS for packaging (workstream 21).
//!
//! * **Passthrough:** `AVAssetReader` outputs without decoding settings return
//!   compressed samples; `AVAssetWriter` inputs without encoding settings (with
//!   the source format as hint) write them unchanged. The range is whole GOPs
//!   ([`plan_trim`]); every video and audio track is copied for that range.
//! * **Re-encode:** exact frames, decoded with [`AvSource`] and written with
//!   [`AvWriterEncoder`] (ProRes 422 HQ, or 4444 when the source has alpha,
//!   unless the request names a codec).

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetWriter,
    AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeMPEG4, AVFileTypeQuickTimeMovie,
    AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_core_media::{CMFormatDescription, CMTime, CMTimeRange};
use objc2_foundation::{NSString, NSURL};

use super::{AvSource, FLICKS_TIMESCALE, platform_error};
use crate::export::{EncodeSettings, Encoder, ProResProfile, VideoCodec};
use crate::frame::FrameRequest;
use crate::meta::{MediaInfo, MediaKind};
use crate::source::{MediaError, MediaSource};
use crate::trim::{TrimPlan, TrimRequest, TrimResult, Trimmer, plan_trim};

/// Trims QuickTime / MPEG-4 movies through AVFoundation.
#[derive(Debug, Default, Clone, Copy)]
pub struct AvTrimmer;

impl Trimmer for AvTrimmer {
    fn id(&self) -> &str {
        "avfoundation"
    }

    fn supports(&self, info: &MediaInfo) -> bool {
        info.kind == MediaKind::Movie && info.format == "avfoundation"
    }

    fn trim(&self, request: &TrimRequest) -> Result<TrimResult, MediaError> {
        let mut source = AvSource::open(&request.source)?;
        let info = source.info().clone();
        let rate = info
            .video
            .as_ref()
            .and_then(|v| v.frame_rate)
            .ok_or_else(|| MediaError::Unsupported("movie without a frame rate".into()))?;
        let sync = source.sync_frames();
        let plan = plan_trim(&info, sync.as_deref(), request)?;
        prepare_output(&request.output)?;
        if plan.passthrough {
            source.copy_range(&request.output, &plan)?;
        } else {
            reencode(&mut source, request, &plan)?;
        }
        Ok(TrimResult {
            files: vec![request.output.clone()],
            source_range: plan.source_range(rate),
            first_frame: plan.first_frame,
            frame_count: plan.frame_count(),
            passthrough: plan.passthrough,
        })
    }
}

fn prepare_output(output: &Path) -> Result<(), MediaError> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| MediaError::io(dir, e))?;
    }
    if output.exists() {
        std::fs::remove_file(output).map_err(|e| MediaError::io(output, e))?;
    }
    Ok(())
}

fn reencode(
    source: &mut AvSource,
    request: &TrimRequest,
    plan: &TrimPlan,
) -> Result<(), MediaError> {
    let video = source
        .info()
        .video
        .clone()
        .ok_or_else(|| MediaError::Unsupported("movie without a picture".into()))?;
    let codec = request
        .reencode_codec
        .unwrap_or(if video.alpha.has_alpha() {
            VideoCodec::ProRes(ProResProfile::P4444)
        } else {
            VideoCodec::ProRes(ProResProfile::Hq)
        });
    let mut settings = EncodeSettings::new(
        request.output.clone(),
        codec,
        video.width,
        video.height,
        video.frame_rate.expect("checked by plan_trim"),
    );
    settings.alpha = if codec.supports_alpha() {
        video.alpha
    } else {
        crate::meta::AlphaMode::Opaque
    };
    settings.color = video.color.clone();
    settings.pixel_aspect = video.pixel_aspect;
    let mut encoder: Box<dyn Encoder> = Box::new(super::AvWriterEncoder::new(&settings)?);
    for index in plan.first_frame..=plan.last_frame {
        let frame = match source.decode_cpu(&FrameRequest::new(index)) {
            Ok(frame) => frame,
            Err(error) => {
                encoder.abort();
                return Err(error);
            }
        };
        if let Err(error) = encoder.push_frame(&frame) {
            encoder.abort();
            return Err(error);
        }
    }
    encoder.finish().map(|_| ())
}

struct TrackCopy {
    output: Retained<AVAssetReaderTrackOutput>,
    input: Retained<AVAssetWriterInput>,
    done: bool,
}

impl AvSource {
    /// Copies the compressed samples of frames `plan.first_frame ..=
    /// plan.last_frame` (and the audio of that time range) into `output`.
    pub(crate) fn copy_range(&self, output: &Path, plan: &TrimPlan) -> Result<(), MediaError> {
        let path_str = output
            .to_str()
            .ok_or_else(|| MediaError::Unsupported("output path is not UTF-8".into()))?;
        let start = self.start_flicks + self.rate.frame_to_time(plan.first_frame).flicks();
        let end = self.start_flicks + self.rate.frame_to_time(plan.last_frame + 1).flicks();
        let mp4 = output
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp4" | "m4v"));
        // SAFETY: plain AVFoundation calls with valid arguments; the statics
        // are provided by AVFoundation.
        unsafe {
            let reader = AVAssetReader::assetReaderWithAsset_error(&self.asset)
                .map_err(|e| platform_error(e.localizedDescription().to_string()))?;
            let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
            let file_type = if mp4 {
                AVFileTypeMPEG4
            } else {
                AVFileTypeQuickTimeMovie
            }
            .expect("AVFoundation file types");
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|e| platform_error(e.localizedDescription().to_string()))?;

            let mut tracks = Vec::new();
            for media_type in [AVMediaTypeVideo, AVMediaTypeAudio].into_iter().flatten() {
                #[allow(deprecated)]
                let asset_tracks = self.asset.tracksWithMediaType(media_type);
                for track in asset_tracks.iter() {
                    let hint: Option<Retained<AnyObject>> =
                        track.formatDescriptions().firstObject();
                    // SAFETY: formatDescriptions holds CMFormatDescriptionRefs.
                    let hint = hint
                        .as_ref()
                        .map(|d| &*Retained::as_ptr(d).cast::<CMFormatDescription>());
                    let output =
                        AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                            &track, None,
                        );
                    output.setAlwaysCopiesSampleData(false);
                    let input =
                        AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings_sourceFormatHint(
                            media_type, None, hint,
                        );
                    input.setExpectsMediaDataInRealTime(false);
                    if !reader.canAddOutput(&output) || !writer.canAddInput(&input) {
                        continue;
                    }
                    reader.addOutput(&output);
                    writer.addInput(&input);
                    tracks.push(TrackCopy {
                        output,
                        input,
                        done: false,
                    });
                }
            }
            if tracks.is_empty() {
                return Err(platform_error("no track can be copied"));
            }
            let start_time = CMTime::new(start, FLICKS_TIMESCALE);
            reader.setTimeRange(CMTimeRange {
                start: start_time,
                duration: CMTime::new(end - start, FLICKS_TIMESCALE),
            });
            if !reader.startReading() {
                return Err(super::reader_error(&reader));
            }
            if !writer.startWriting() {
                return Err(writer_failure(&writer));
            }
            writer.startSessionAtSourceTime(start_time);

            // Round-robin over the tracks so the writer can interleave them.
            loop {
                let mut progressed = false;
                for track in tracks.iter_mut().filter(|t| !t.done) {
                    while track.input.isReadyForMoreMediaData() {
                        match track.output.copyNextSampleBuffer() {
                            Some(sample) => {
                                if !track.input.appendSampleBuffer(&sample) {
                                    reader.cancelReading();
                                    writer.cancelWriting();
                                    return Err(writer_failure(&writer));
                                }
                                progressed = true;
                            }
                            None => {
                                track.input.markAsFinished();
                                track.done = true;
                                progressed = true;
                                break;
                            }
                        }
                    }
                }
                if tracks.iter().all(|t| t.done) {
                    break;
                }
                if !progressed {
                    if reader.status() == AVAssetReaderStatus::Failed {
                        writer.cancelWriting();
                        return Err(super::reader_error(&reader));
                    }
                    if writer.status() == AVAssetWriterStatus::Failed {
                        reader.cancelReading();
                        return Err(writer_failure(&writer));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            writer.endSessionAtSourceTime(CMTime::new(end, FLICKS_TIMESCALE));
            #[allow(deprecated)]
            let finished = writer.finishWriting();
            if !finished || writer.status() != AVAssetWriterStatus::Completed {
                return Err(writer_failure(&writer));
            }
        }
        Ok(())
    }
}

fn writer_failure(writer: &AVAssetWriter) -> MediaError {
    // SAFETY: error query.
    let message = unsafe { writer.error() }
        .map(|e| e.localizedDescription().to_string())
        .unwrap_or_else(|| "AVAssetWriter failed".into());
    MediaError::Platform {
        framework: "AVAssetWriter",
        message,
    }
}
