//! macOS movie encoding: `AVAssetWriter` drives the VideoToolbox encoders
//! (hardware H.264 / HEVC on Apple silicon, ProRes in the media engine or in
//! software).
//!
//! Frames arrive as [`CpuFrame`]s (read back from the renderer) and are copied
//! into IOSurface-backed pixel buffers from the writer's pool: `32BGRA` for
//! 8-bit H.264 / HEVC, `64RGBAHalf` for ProRes and 10-bit HEVC so precision and
//! alpha survive. Color tags come from the settings' H.273 code points; alpha is
//! written straight or premultiplied as requested (HEVC tags the mode, ProRes
//! has no flag for it, so readers must be told or guess).

use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVFileTypeQuickTimeMovie, AVMediaTypeVideo, AVVideoAverageBitRateKey,
    AVVideoCodecKey, AVVideoCodecTypeAppleProRes422, AVVideoCodecTypeAppleProRes422HQ,
    AVVideoCodecTypeAppleProRes422LT, AVVideoCodecTypeAppleProRes422Proxy,
    AVVideoCodecTypeAppleProRes4444, AVVideoCodecTypeAppleProRes4444XQ, AVVideoCodecTypeH264,
    AVVideoCodecTypeHEVC, AVVideoCodecTypeHEVCWithAlpha, AVVideoColorPrimariesKey,
    AVVideoColorPropertiesKey, AVVideoCompressionPropertiesKey, AVVideoEncoderSpecificationKey,
    AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey, AVVideoPixelAspectRatioHorizontalSpacingKey,
    AVVideoPixelAspectRatioKey, AVVideoPixelAspectRatioVerticalSpacingKey, AVVideoProfileLevelKey,
    AVVideoQualityKey, AVVideoTransferFunctionKey, AVVideoWidthKey, AVVideoYCbCrMatrixKey,
};
use objc2_core_foundation::{CFRetained, CFString};
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVColorPrimariesGetStringForIntegerCodePoint, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferPool, CVPixelBufferUnlockBaseAddress,
    CVTransferFunctionGetStringForIntegerCodePoint, CVYCbCrMatrixGetStringForIntegerCodePoint,
    kCVPixelBufferHeightKey, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_64RGBAHalf, kCVReturnSuccess,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_video_toolbox::{
    kVTAlphaChannelMode_PremultipliedAlpha, kVTAlphaChannelMode_StraightAlpha,
    kVTCompressionPropertyKey_AlphaChannelMode, kVTProfileLevel_HEVC_Main10_AutoLevel,
    kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder,
};

use crate::export::{
    EncodeOutput, EncodeSettings, Encoder, EncoderFormat, ProResProfile, VideoCodec,
    convert_frame_cow,
};
use crate::frame::{CpuFrame, PixelFormat};
use crate::meta::{AlphaMode, ColorPrimaries, MatrixCoefficients};
use crate::source::MediaError;

pub(super) fn writer_error(writer: &AVAssetWriter, what: &str) -> MediaError {
    // SAFETY: error query.
    let detail = unsafe { writer.error() }
        .map(|e| e.localizedDescription().to_string())
        .unwrap_or_else(|| "unknown error".into());
    MediaError::Platform {
        framework: "AVAssetWriter",
        message: format!("{what}: {detail}"),
    }
}

/// Bridges a CoreFoundation string to `NSString` (toll-free).
fn ns(s: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged.
    unsafe { &*std::ptr::from_ref(s).cast::<NSString>() }
}

pub(super) fn any<T: AsRef<AnyObject> + ?Sized>(obj: &T) -> &AnyObject {
    obj.as_ref()
}

pub(super) fn dict(
    entries: &[(&NSString, &AnyObject)],
) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = entries.iter().map(|(k, _)| *k).collect();
    let values: Vec<&AnyObject> = entries.iter().map(|(_, v)| *v).collect();
    NSDictionary::from_slices(&keys, &values)
}

/// `true` for settings the AVFoundation encoder handles.
fn movie_codec(settings: &EncodeSettings) -> bool {
    let mp4 = is_mp4(&settings.path);
    match settings.codec {
        VideoCodec::H264 | VideoCodec::Hevc => true,
        // ProRes lives in QuickTime files only.
        VideoCodec::ProRes(_) => !mp4,
        VideoCodec::Png | VideoCodec::Exr(_) | VideoCodec::Tiff(_) => false,
    }
}

fn is_mp4(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp4" | "m4v"))
}

/// Encoder format for QuickTime / MPEG-4 movies through AVFoundation.
#[derive(Debug, Default, Clone, Copy)]
pub struct AvWriterFormat;

impl EncoderFormat for AvWriterFormat {
    fn id(&self) -> &str {
        "avfoundation"
    }

    fn supports(&self, settings: &EncodeSettings) -> bool {
        movie_codec(settings)
    }

    fn create(&self, settings: &EncodeSettings) -> Result<Box<dyn Encoder>, MediaError> {
        Ok(Box::new(AvWriterEncoder::new(settings)?))
    }
}

/// A movie being written.
pub struct AvWriterEncoder {
    settings: EncodeSettings,
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    audio: Option<super::audio_input::AudioInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    pixel_format: PixelFormat,
    frame_alpha: AlphaMode,
    frames: u64,
}

// SAFETY: the AVFoundation objects are only used through `&mut self` / `self`,
// i.e. by one thread at a time; AVAssetWriter does not require a specific thread.
unsafe impl Send for AvWriterEncoder {}

impl AvWriterEncoder {
    /// Creates the file and starts a writing session.
    pub fn new(settings: &EncodeSettings) -> Result<Self, MediaError> {
        settings.validate()?;
        if !movie_codec(settings) {
            return Err(MediaError::Unsupported(format!(
                "{:?} in {}",
                settings.codec,
                settings.path.display()
            )));
        }
        let path = &settings.path;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| MediaError::io(dir, e))?;
        }
        // AVAssetWriter refuses to overwrite.
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| MediaError::io(path, e))?;
        }
        let path_str = path
            .to_str()
            .ok_or_else(|| MediaError::Unsupported("output path is not UTF-8".into()))?;

        let ten_bit = matches!(settings.codec, VideoCodec::Hevc) && settings.bit_depth > 8;
        let (pixel_format, cv_format) =
            if matches!(settings.codec, VideoCodec::ProRes(_)) || ten_bit {
                (PixelFormat::Rgba16Float, kCVPixelFormatType_64RGBAHalf)
            } else {
                (PixelFormat::Bgra8, kCVPixelFormatType_32BGRA)
            };
        // ProRes carries no alpha-mode flag: write what was asked (AE's
        // default for "High Quality with Alpha" is premultiplied); HEVC tags it.
        let frame_alpha = match settings.alpha {
            AlphaMode::Opaque | AlphaMode::Ignore => AlphaMode::Opaque,
            alpha => alpha,
        };

        // SAFETY: plain AVFoundation calls with valid arguments; the statics
        // are provided by AVFoundation / CoreVideo / VideoToolbox.
        unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
            let file_type = if is_mp4(path) {
                AVFileTypeMPEG4
            } else {
                AVFileTypeQuickTimeMovie
            }
            .expect("AVFoundation file types");
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|e| MediaError::Platform {
                    framework: "AVAssetWriter",
                    message: e.localizedDescription().to_string(),
                })?;
            writer.setMovieTimeScale(settings.frame_rate.num as i32);

            let output_settings = video_settings(settings);
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                AVMediaTypeVideo.expect("AVMediaTypeVideo"),
                Some(&output_settings),
            );
            input.setExpectsMediaDataInRealTime(false);
            input.setMediaTimeScale(settings.frame_rate.num as i32);
            if !writer.canAddInput(&input) {
                return Err(writer_error(&writer, "cannot add the video input"));
            }
            writer.addInput(&input);
            let audio = settings
                .audio
                .as_ref()
                .map(|a| super::audio_input::AudioInput::add(&writer, a))
                .transpose()?;

            let format = NSNumber::new_u32(cv_format);
            let width = NSNumber::new_u32(settings.width);
            let height = NSNumber::new_u32(settings.height);
            let iosurface = NSDictionary::<NSString, AnyObject>::new();
            let attributes = dict(&[
                (ns(kCVPixelBufferPixelFormatTypeKey), any(&*format)),
                (ns(kCVPixelBufferWidthKey), any(&*width)),
                (ns(kCVPixelBufferHeightKey), any(&*height)),
                (ns(kCVPixelBufferIOSurfacePropertiesKey), any(&*iosurface)),
            ]);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &input,
                Some(&attributes),
            );
            if !writer.startWriting() {
                return Err(writer_error(&writer, "cannot start writing"));
            }
            writer.startSessionAtSourceTime(CMTime::new(0, settings.frame_rate.num as i32));
            Ok(AvWriterEncoder {
                settings: settings.clone(),
                writer,
                input,
                adaptor,
                pixel_format,
                frame_alpha,
                audio,
                frames: 0,
            })
        }
    }

    fn pixel_buffer(&self, frame: &CpuFrame) -> Result<CFRetained<CVPixelBuffer>, MediaError> {
        // SAFETY: the pool exists once writing started; the buffer is locked
        // while written, rows are `stride` apart and at least `row` bytes long.
        unsafe {
            let pool = self
                .adaptor
                .pixelBufferPool()
                .ok_or_else(|| writer_error(&self.writer, "no pixel buffer pool"))?;
            let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
            let status =
                CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut out));
            let buffer = NonNull::new(out)
                .filter(|_| status == kCVReturnSuccess)
                .map(|ptr| CFRetained::from_raw(ptr))
                .ok_or_else(|| MediaError::Platform {
                    framework: "CoreVideo",
                    message: format!("CVPixelBufferPoolCreatePixelBuffer failed ({status})"),
                })?;
            CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
            let base = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
            let stride = CVPixelBufferGetBytesPerRow(&buffer);
            let row = frame.bytes_per_row() as usize;
            for y in 0..frame.height as usize {
                std::ptr::copy_nonoverlapping(
                    frame.data.as_ptr().add(y * row),
                    base.add(y * stride),
                    row,
                );
            }
            CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
            Ok(buffer)
        }
    }

    fn wait_ready(&self, input: &AVAssetWriterInput) -> Result<(), MediaError> {
        // SAFETY: status queries on a writing session.
        unsafe {
            while !input.isReadyForMoreMediaData() {
                if self.writer.status() == AVAssetWriterStatus::Failed {
                    return Err(writer_error(&self.writer, "encoding failed"));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        Ok(())
    }
}

fn prores_codec(profile: ProResProfile) -> Option<&'static NSString> {
    // SAFETY: extern statics provided by AVFoundation.
    unsafe {
        match profile {
            ProResProfile::Proxy => AVVideoCodecTypeAppleProRes422Proxy,
            ProResProfile::Lt => AVVideoCodecTypeAppleProRes422LT,
            ProResProfile::Standard => AVVideoCodecTypeAppleProRes422,
            ProResProfile::Hq => AVVideoCodecTypeAppleProRes422HQ,
            ProResProfile::P4444 => AVVideoCodecTypeAppleProRes4444,
            ProResProfile::P4444Xq => AVVideoCodecTypeAppleProRes4444XQ,
        }
    }
}

/// The CoreVideo / AVFoundation string for an H.273 code point, falling back
/// to BT.709 when there is none.
fn color_string(
    code: u8,
    lookup: extern "C-unwind" fn(i32) -> Option<CFRetained<CFString>>,
) -> CFRetained<CFString> {
    lookup(i32::from(code))
        .or_else(|| lookup(1))
        .expect("CoreVideo knows BT.709")
}

/// The `outputSettings` dictionary of the video input.
fn video_settings(settings: &EncodeSettings) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: extern statics provided by AVFoundation / VideoToolbox.
    unsafe {
        let codec = match settings.codec {
            VideoCodec::H264 => AVVideoCodecTypeH264,
            VideoCodec::Hevc if settings.has_alpha() => AVVideoCodecTypeHEVCWithAlpha,
            VideoCodec::Hevc => AVVideoCodecTypeHEVC,
            VideoCodec::ProRes(profile) => prores_codec(profile),
            VideoCodec::Png | VideoCodec::Exr(_) | VideoCodec::Tiff(_) => None,
        }
        .expect("AVFoundation codec type");
        let width = NSNumber::new_u32(settings.width);
        let height = NSNumber::new_u32(settings.height);

        // Color: the code points of the settings; the matrix follows the
        // primaries when the settings leave it open (frames are RGB).
        let color = &settings.color;
        let matrix = match color.matrix {
            MatrixCoefficients::Identity | MatrixCoefficients::Unspecified => match color.primaries
            {
                ColorPrimaries::Bt2020 => MatrixCoefficients::Bt2020Ncl,
                ColorPrimaries::Bt601Pal | ColorPrimaries::Bt601Ntsc => MatrixCoefficients::Bt601,
                _ => MatrixCoefficients::Bt709,
            },
            other => other,
        };
        let primaries = color_string(
            color.primaries.to_cicp(),
            CVColorPrimariesGetStringForIntegerCodePoint,
        );
        let transfer = color_string(
            color.transfer.to_cicp(),
            CVTransferFunctionGetStringForIntegerCodePoint,
        );
        let matrix = color_string(matrix.to_cicp(), CVYCbCrMatrixGetStringForIntegerCodePoint);
        let color_properties = dict(&[
            (AVVideoColorPrimariesKey.unwrap(), any(ns(&primaries))),
            (AVVideoTransferFunctionKey.unwrap(), any(ns(&transfer))),
            (AVVideoYCbCrMatrixKey.unwrap(), any(ns(&matrix))),
        ]);

        let mut entries: Vec<(&NSString, &AnyObject)> = vec![
            (AVVideoCodecKey.unwrap(), any(codec)),
            (AVVideoWidthKey.unwrap(), any(&*width)),
            (AVVideoHeightKey.unwrap(), any(&*height)),
            (AVVideoColorPropertiesKey.unwrap(), any(&*color_properties)),
        ];

        let par_h = NSNumber::new_u32(settings.pixel_aspect.num);
        let par_v = NSNumber::new_u32(settings.pixel_aspect.den);
        let par = dict(&[
            (
                AVVideoPixelAspectRatioHorizontalSpacingKey.unwrap(),
                any(&*par_h),
            ),
            (
                AVVideoPixelAspectRatioVerticalSpacingKey.unwrap(),
                any(&*par_v),
            ),
        ]);
        if !settings.pixel_aspect.is_square() {
            entries.push((AVVideoPixelAspectRatioKey.unwrap(), any(&*par)));
        }

        // Compression properties and the hardware encoder only apply to the
        // long-GOP codecs; ProRes rejects unknown keys.
        let bit_rate = settings.bit_rate.map(NSNumber::new_u64);
        let quality = settings
            .quality
            .map(|q| NSNumber::new_f32(q.clamp(0.0, 1.0)));
        let keyframes = settings.keyframe_interval.map(NSNumber::new_u32);
        let alpha_mode = if settings.alpha == AlphaMode::Premultiplied {
            kVTAlphaChannelMode_PremultipliedAlpha
        } else {
            kVTAlphaChannelMode_StraightAlpha
        };
        let compression;
        let hardware_on = NSNumber::new_bool(true);
        let encoder_spec;
        if matches!(settings.codec, VideoCodec::H264 | VideoCodec::Hevc) {
            let mut props: Vec<(&NSString, &AnyObject)> = Vec::new();
            if let Some(bit_rate) = &bit_rate {
                props.push((AVVideoAverageBitRateKey.unwrap(), any(&**bit_rate)));
            }
            if let Some(quality) = &quality {
                props.push((AVVideoQualityKey.unwrap(), any(&**quality)));
            }
            if let Some(keyframes) = &keyframes {
                props.push((AVVideoMaxKeyFrameIntervalKey.unwrap(), any(&**keyframes)));
            }
            if settings.codec == VideoCodec::Hevc && settings.bit_depth > 8 {
                props.push((
                    AVVideoProfileLevelKey.unwrap(),
                    any(ns(kVTProfileLevel_HEVC_Main10_AutoLevel)),
                ));
            }
            if settings.codec == VideoCodec::Hevc && settings.has_alpha() {
                props.push((
                    ns(kVTCompressionPropertyKey_AlphaChannelMode),
                    any(ns(alpha_mode)),
                ));
            }
            compression = dict(&props);
            entries.push((AVVideoCompressionPropertiesKey.unwrap(), any(&*compression)));
            encoder_spec = dict(&[(
                ns(kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder),
                any(&*hardware_on),
            )]);
            entries.push((AVVideoEncoderSpecificationKey.unwrap(), any(&*encoder_spec)));
        }
        dict(&entries)
    }
}

impl Encoder for AvWriterEncoder {
    fn settings(&self) -> &EncodeSettings {
        &self.settings
    }

    fn push_frame(&mut self, frame: &CpuFrame) -> Result<(), MediaError> {
        if (frame.width, frame.height) != (self.settings.width, self.settings.height) {
            return Err(MediaError::Unsupported(format!(
                "{}×{} frame for a {}×{} output",
                frame.width, frame.height, self.settings.width, self.settings.height
            )));
        }
        let converted = convert_frame_cow(frame, self.pixel_format, self.frame_alpha);
        let buffer = self.pixel_buffer(&converted)?;
        self.wait_ready(&self.input)?;
        let rate = self.settings.frame_rate;
        // SAFETY: building a time and appending a pool buffer to a writing session.
        let appended = unsafe {
            let time = CMTime::new(self.frames as i64 * i64::from(rate.den), rate.num as i32);
            self.adaptor
                .appendPixelBuffer_withPresentationTime(&buffer, time)
        };
        if !appended {
            return Err(writer_error(&self.writer, "cannot append a frame"));
        }
        self.frames += 1;
        Ok(())
    }

    fn frames_written(&self) -> u64 {
        self.frames
    }

    fn input_format(&self) -> Option<(PixelFormat, AlphaMode)> {
        Some((self.pixel_format, self.frame_alpha))
    }

    fn push_audio(&mut self, pcm: &crate::AudioBuffer) -> Result<(), MediaError> {
        let Some(audio) = &self.audio else {
            return Err(MediaError::Unsupported("no audio stream".into()));
        };
        self.settings
            .audio
            .as_ref()
            .expect("configured audio")
            .validate_input(pcm)?;
        self.wait_ready(&audio.input)?;
        self.audio
            .as_mut()
            .expect("configured audio")
            .append(&self.writer, pcm)
    }

    fn finish(self: Box<Self>) -> Result<EncodeOutput, MediaError> {
        // SAFETY: closing a writing session.
        unsafe {
            self.input.markAsFinished();
            if let Some(audio) = &self.audio {
                audio.input.markAsFinished();
            }
            #[allow(deprecated)]
            let finished = self.writer.finishWriting();
            if !finished || self.writer.status() != AVAssetWriterStatus::Completed {
                return Err(writer_error(&self.writer, "cannot finish the movie"));
            }
        }
        Ok(EncodeOutput {
            files: vec![self.settings.path.clone()],
            frames: self.frames,
            stats: crate::export::EncodeStats {
                threads: 1,
                ..Default::default()
            },
        })
    }

    fn abort(self: Box<Self>) {
        // SAFETY: cancelling a writing session.
        unsafe { self.writer.cancelWriting() };
        let _ = std::fs::remove_file(&self.settings.path);
    }
}

/// Output path helper for callers that want a default movie name.
pub fn default_movie_path(dir: &Path, name: &str, codec: VideoCodec) -> PathBuf {
    let ext = match codec {
        VideoCodec::H264 | VideoCodec::Hevc => "mp4",
        _ => "mov",
    };
    dir.join(format!("{name}.{ext}"))
}
