//! macOS video decoding: AVFoundation (`AVAssetReader`) demuxes and drives
//! VideoToolbox, which decodes H.264, HEVC and ProRes in hardware into
//! IOSurface-backed `CVPixelBuffer`s.
//!
//! **Zero copy:** each pixel buffer is wrapped as a Metal texture
//! (`CVMetalTextureCache`), then as a `wgpu::Texture` on the shared device
//! (wgpu-hal), and the conversion pass of [`MediaGpu`] writes the
//! `Rgba16Float` working frame from it. Pixels never touch the CPU; the pixel
//! buffer is released when wgpu has finished with the texture.
//!
//! **Pixel formats:** 8-bit sources are decoded to `32BGRA`; deeper sources
//! (10-bit HEVC, ProRes) and sources with alpha (ProRes 4444) to `64RGBAHalf`,
//! which keeps precision and alpha. VideoToolbox applies the Y'CbCr matrix and
//! range of the file, so frames are full-range RGB in the file's transfer
//! function and primaries ([`ColorInfo`]).
//!
//! **Frame-accurate random access:** frame `i` is the sample presented at
//! `start + i / rate`. A request at or shortly after the current read position
//! reads forward; anything else restarts the reader with a time range beginning
//! at the frame (AVFoundation then decodes from the preceding keyframe and only
//! outputs from the range start).

mod encode;

pub use encode::{AvWriterEncoder, AvWriterFormat, default_movie_path};

use std::path::Path;
use std::ptr::NonNull;

use bae_gpu::wgpu;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack, AVMediaTypeVideo,
    AVURLAsset, AVURLAssetPreferPreciseDurationAndTimingKey,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMFormatDescription, CMSampleBuffer, CMTime, CMTimeFlags, CMTimeRange,
    CMVideoFormatDescriptionGetDimensions, kCMFormatDescriptionAlphaChannelMode_PremultipliedAlpha,
    kCMFormatDescriptionExtension_AlphaChannelMode, kCMFormatDescriptionExtension_BitsPerComponent,
    kCMFormatDescriptionExtension_ColorPrimaries,
    kCMFormatDescriptionExtension_ContainsAlphaChannel, kCMFormatDescriptionExtension_Depth,
    kCMFormatDescriptionExtension_FullRangeVideo, kCMFormatDescriptionExtension_PixelAspectRatio,
    kCMFormatDescriptionExtension_TransferFunction, kCMFormatDescriptionExtension_YCbCrMatrix,
    kCMFormatDescriptionKey_PixelAspectRatioHorizontalSpacing,
    kCMFormatDescriptionKey_PixelAspectRatioVerticalSpacing, kCMTimePositiveInfinity,
};
use objc2_core_video::{
    CVColorPrimariesGetIntegerCodePointForString, CVMetalTexture, CVMetalTextureCache,
    CVMetalTextureGetTexture, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, CVTransferFunctionGetIntegerCodePointForString,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, kCVPixelFormatType_64RGBAHalf,
    kCVReturnSuccess,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_metal::{MTLDevice, MTLPixelFormat, MTLTexture, MTLTextureType};

use crate::frame::{CpuFrame, FrameRequest, GpuFrame, PixelFormat};
use crate::gpu::{MediaGpu, SourceTexture};
use crate::meta::{
    AlphaMode, ColorInfo, ColorPrimaries, ColorRange, MatrixCoefficients, MediaInfo, MediaKind,
    PixelAspect, SampleType, TransferFunction, VideoInfo,
};
use crate::source::{MediaError, MediaFormat, MediaSource, check_index, sniff_iso_bmff};
use crate::time::{FrameRate, Time, frame_rate_from_f64};

const FRAMEWORK: &str = "AVFoundation";
/// Timescale used for seek times: one flick.
const FLICKS_TIMESCALE: i32 = Time::FLICKS_PER_SECOND as i32;

fn platform_error(message: impl Into<String>) -> MediaError {
    MediaError::Platform {
        framework: FRAMEWORK,
        message: message.into(),
    }
}

/// QuickTime / MPEG-4 movies through AVFoundation (H.264, HEVC, ProRes …).
#[derive(Debug, Default, Clone, Copy)]
pub struct AvFoundationFormat;

impl MediaFormat for AvFoundationFormat {
    fn id(&self) -> &str {
        "avfoundation"
    }

    fn extensions(&self) -> &[&str] {
        &["mov", "mp4", "m4v", "qt"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        sniff_iso_bmff(header)
    }

    fn is_image(&self) -> bool {
        false
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        Ok(Box::new(AvSource::open(path)?))
    }
}

/// Keeps a decoded pixel buffer (and its Metal texture) alive until wgpu drops
/// the texture made from it.
struct KeepAlive {
    _texture: CFRetained<CVMetalTexture>,
    _buffer: CFRetained<CVPixelBuffer>,
}

// SAFETY: CoreFoundation objects may be retained and released from any thread;
// nothing else is done with them.
unsafe impl Send for KeepAlive {}
// SAFETY: as above.
unsafe impl Sync for KeepAlive {}

struct Reader {
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    /// The last frame read (index, buffer); requests at or after it read forward.
    current: Option<(i64, CFRetained<CVPixelBuffer>)>,
    /// A frame read past a gap, returned by the next forward read.
    ahead: Option<(i64, CFRetained<CVPixelBuffer>)>,
}

/// An opened movie.
pub struct AvSource {
    info: MediaInfo,
    asset: Retained<AVURLAsset>,
    track: Retained<AVAssetTrack>,
    rate: FrameRate,
    /// Presentation time of the first frame, in flicks.
    start_flicks: i64,
    pixel_format: u32,
    reader: Option<Reader>,
    /// Texture cache for one Metal device (keyed by the device pointer).
    texture_cache: Option<(usize, CFRetained<CVMetalTextureCache>)>,
}

// SAFETY: the AVFoundation objects are only used through `&mut self`, i.e. by one
// thread at a time; AVAsset / AVAssetReader may be used from any thread as long
// as calls are not concurrent.
unsafe impl Send for AvSource {}

fn cmtime_to_flicks(time: CMTime) -> Option<i64> {
    if !time.flags.contains(CMTimeFlags::Valid) || time.timescale <= 0 {
        return None;
    }
    let flicks =
        i128::from(time.value) * i128::from(Time::FLICKS_PER_SECOND) / i128::from(time.timescale);
    i64::try_from(flicks).ok()
}

fn fourcc(code: u32) -> String {
    code.to_be_bytes()
        .iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
        .collect()
}

fn codec_name(code: u32) -> String {
    match &code.to_be_bytes() {
        b"avc1" | b"avc3" => "h264".into(),
        b"hvc1" | b"hev1" => "hevc".into(),
        b"apco" => "prores422proxy".into(),
        b"apcs" => "prores422lt".into(),
        b"apcn" => "prores422".into(),
        b"apch" => "prores422hq".into(),
        b"ap4h" => "prores4444".into(),
        b"ap4x" => "prores4444xq".into(),
        _ => fourcc(code),
    }
}

fn number(value: &CFType) -> Option<i64> {
    value.downcast_ref::<CFNumber>()?.as_i64()
}

/// Reads the color, alpha, depth and aspect extensions of a format description.
fn describe(desc: &CMFormatDescription, video: &mut VideoInfo) {
    // SAFETY: the keys are valid CFStrings; extension values are property lists.
    let ext = |key: &CFString| unsafe { desc.extension(key) };
    // SAFETY: extern statics provided by CoreMedia.
    let (depth_key, bits_key, full_key, prim_key, tf_key, mat_key, par_key) = unsafe {
        (
            kCMFormatDescriptionExtension_Depth,
            kCMFormatDescriptionExtension_BitsPerComponent,
            kCMFormatDescriptionExtension_FullRangeVideo,
            kCMFormatDescriptionExtension_ColorPrimaries,
            kCMFormatDescriptionExtension_TransferFunction,
            kCMFormatDescriptionExtension_YCbCrMatrix,
            kCMFormatDescriptionExtension_PixelAspectRatio,
        )
    };
    let prores = video.codec.starts_with("prores");
    // ProRes 4444 signals alpha with a depth of 32; HEVC with alpha with
    // ContainsAlphaChannel and says whether it is premultiplied.
    // SAFETY: extern statics provided by CoreMedia.
    let (contains_key, mode_key, premultiplied) = unsafe {
        (
            kCMFormatDescriptionExtension_ContainsAlphaChannel,
            kCMFormatDescriptionExtension_AlphaChannelMode,
            kCMFormatDescriptionAlphaChannelMode_PremultipliedAlpha,
        )
    };
    let contains_alpha = ext(contains_key)
        .and_then(|v| v.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool))
        .unwrap_or(false);
    let alpha = contains_alpha || ext(depth_key).and_then(|v| number(&v)) == Some(32);
    let is_premultiplied = ext(mode_key)
        .and_then(|v| v.downcast_ref::<CFString>().map(|s| s == premultiplied))
        .unwrap_or(false);
    video.alpha = match (alpha, is_premultiplied) {
        (false, _) => AlphaMode::Opaque,
        (true, false) => AlphaMode::Straight,
        (true, true) => AlphaMode::Premultiplied,
    };
    let bits = ext(bits_key).and_then(|v| number(&v));
    video.bit_depth = match bits {
        Some(bits) => bits.clamp(8, 16) as u8,
        None if video.codec.starts_with("prores4444") => 12,
        None if prores => 10,
        None => 8,
    };
    let full_range = ext(full_key)
        .and_then(|v| v.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool))
        .unwrap_or(false);
    let code = |key: &CFString, to_code: extern "C-unwind" fn(Option<&CFString>) -> i32| {
        ext(key)
            .and_then(|v| v.downcast_ref::<CFString>().map(|s| to_code(Some(s))))
            .unwrap_or(2)
            .clamp(0, 255) as u8
    };
    let primaries = code(prim_key, CVColorPrimariesGetIntegerCodePointForString);
    let transfer = code(tf_key, CVTransferFunctionGetIntegerCodePointForString);
    let matrix = code(
        mat_key,
        objc2_core_video::CVYCbCrMatrixGetIntegerCodePointForString,
    );
    let mut color = ColorInfo::from_cicp(primaries, transfer, matrix, full_range);
    // Untagged HD video is BT.709 by convention; leave the decision to
    // bae-color but keep what the file says.
    if color.primaries == ColorPrimaries::Unspecified
        && color.transfer == TransferFunction::Unspecified
    {
        color.matrix = MatrixCoefficients::Unspecified;
    }
    video.color = color;
    if let Some(par) = ext(par_key).and_then(|v| v.downcast::<CFDictionary>().ok()) {
        // SAFETY: the keys are extern CFStrings; values are CFNumbers.
        let (h, v) = unsafe {
            (
                kCMFormatDescriptionKey_PixelAspectRatioHorizontalSpacing,
                kCMFormatDescriptionKey_PixelAspectRatioVerticalSpacing,
            )
        };
        // SAFETY: the dictionary maps CFString keys to CFNumber values.
        let par: &CFDictionary<CFString, CFType> = unsafe { par.cast_unchecked() };
        if let (Some(h), Some(v)) = (
            par.get(h).and_then(|n| number(&n)),
            par.get(v).and_then(|n| number(&n)),
        ) {
            video.pixel_aspect = PixelAspect::new(h.max(0) as u32, v.max(0) as u32);
        }
    }
}

impl AvSource {
    /// Opens the first video track of `path`.
    pub fn open(path: &Path) -> Result<Self, MediaError> {
        let path_str = path
            .to_str()
            .ok_or_else(|| MediaError::decode(path, "path is not valid UTF-8"))?;
        if !path.is_file() {
            return Err(MediaError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            ));
        }
        // SAFETY: plain AVFoundation calls with valid arguments.
        let (asset, track) = unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
            // Exact durations and time stamps from the sample tables (frame
            // accuracy matters more than the slightly slower open).
            let precise = NSNumber::new_bool(true);
            let options = NSDictionary::<NSString, AnyObject>::from_slices(
                &[AVURLAssetPreferPreciseDurationAndTimingKey],
                &[AsRef::<AnyObject>::as_ref(&*precise)],
            );
            let asset = AVURLAsset::URLAssetWithURL_options(&url, Some(&options));
            let video_type =
                AVMediaTypeVideo.ok_or_else(|| platform_error("no AVMediaTypeVideo"))?;
            #[allow(deprecated)]
            let tracks = asset.tracksWithMediaType(video_type);
            let track = tracks
                .firstObject()
                .ok_or_else(|| MediaError::decode(path, "no video track"))?;
            (asset, track)
        };
        // SAFETY: AVAssetTrack properties.
        let (descriptions, time_range, nominal, min_duration) = unsafe {
            (
                track.formatDescriptions(),
                track.timeRange(),
                track.nominalFrameRate(),
                track.minFrameDuration(),
            )
        };
        let description: Retained<AnyObject> = descriptions
            .firstObject()
            .ok_or_else(|| MediaError::decode(path, "video track without a format description"))?;
        // SAFETY: AVAssetTrack.formatDescriptions holds CMFormatDescriptionRefs
        // (toll-free CF objects), kept alive by `descriptions`.
        let desc: &CMFormatDescription =
            unsafe { &*Retained::as_ptr(&description).cast::<CMFormatDescription>() };
        // SAFETY: valid video format description.
        let (code, dims) = unsafe {
            (
                desc.media_sub_type(),
                CMVideoFormatDescriptionGetDimensions(desc),
            )
        };

        let rate = rate_from(min_duration, nominal)
            .ok_or_else(|| MediaError::decode(path, "video track without a frame rate"))?;
        let start_flicks = cmtime_to_flicks(time_range.start).unwrap_or(0);
        let duration_flicks = cmtime_to_flicks(time_range.duration).unwrap_or(0);
        // Round to whole frames: the last sample's duration may be a hair short.
        let frame_count = rate
            .time_to_frame(Time::from_flicks(
                duration_flicks + rate.frame_duration().flicks() / 2,
            ))
            .max(0) as u64;

        let mut video = VideoInfo {
            width: dims.width.max(0) as u32,
            height: dims.height.max(0) as u32,
            pixel_aspect: PixelAspect::SQUARE,
            frame_rate: Some(rate),
            frame_count: Some(frame_count),
            bit_depth: 8,
            sample_type: SampleType::UInt,
            color: ColorInfo::default(),
            alpha: AlphaMode::Opaque,
            codec: codec_name(code),
        };
        describe(desc, &mut video);
        let pixel_format = if video.bit_depth > 8 || video.alpha.has_alpha() {
            kCVPixelFormatType_64RGBAHalf
        } else {
            kCVPixelFormatType_32BGRA
        };
        // Frames are delivered as RGB.
        video.color.matrix = MatrixCoefficients::Identity;
        video.color.range = ColorRange::Full;

        let info = MediaInfo {
            path: path.to_owned(),
            kind: MediaKind::Movie,
            format: "avfoundation".into(),
            video: Some(video),
            audio: Vec::new(),
            timecode: None,
            sequence: None,
        };
        Ok(AvSource {
            info,
            asset,
            track,
            rate,
            start_flicks,
            pixel_format,
            reader: None,
            texture_cache: None,
        })
    }

    fn frame_duration_flicks(&self) -> i64 {
        self.rate.frame_duration().flicks()
    }

    /// Frame index of a presentation time stamp (nearest frame).
    fn index_of(&self, pts: CMTime) -> Option<i64> {
        let flicks = cmtime_to_flicks(pts)? - self.start_flicks;
        Some(
            self.rate
                .time_to_frame(Time::from_flicks(flicks + self.frame_duration_flicks() / 2)),
        )
    }

    fn output_settings(&self) -> Retained<NSDictionary<NSString, AnyObject>> {
        // SAFETY: CoreVideo's keys are CFStrings, toll-free bridged to NSString.
        let key = |k: &'static CFString| unsafe { &*std::ptr::from_ref(k).cast::<NSString>() };
        // SAFETY: extern statics provided by CoreVideo.
        let (format_key, metal_key, iosurface_key) = unsafe {
            (
                key(kCVPixelBufferPixelFormatTypeKey),
                key(kCVPixelBufferMetalCompatibilityKey),
                key(kCVPixelBufferIOSurfacePropertiesKey),
            )
        };
        let format = NSNumber::new_u32(self.pixel_format);
        let metal = NSNumber::new_bool(true);
        let iosurface = NSDictionary::<NSString, AnyObject>::new();
        NSDictionary::from_slices(
            &[format_key, metal_key, iosurface_key],
            &[
                AsRef::<AnyObject>::as_ref(&*format),
                AsRef::<AnyObject>::as_ref(&*metal),
                AsRef::<AnyObject>::as_ref(&*iosurface),
            ],
        )
    }

    /// (Re)starts reading at frame `index`.
    fn start_reader(&mut self, index: i64) -> Result<(), MediaError> {
        if let Some(old) = self.reader.take() {
            // SAFETY: cancelling an AVAssetReader is always allowed.
            unsafe { old.reader.cancelReading() };
        }
        let settings = self.output_settings();
        // AVAssetReader first outputs the sample on screen at the range start,
        // with its time stamp clamped to that start. Starting an eighth of a frame
        // into the target makes that sample the target itself (never its
        // predecessor) and keeps the clamped stamp rounding to the target.
        let start = self.start_flicks
            + self.rate.frame_to_time(index).flicks()
            + self.frame_duration_flicks() / 8;
        // SAFETY: plain AVFoundation calls with valid arguments.
        unsafe {
            let reader = AVAssetReader::assetReaderWithAsset_error(&self.asset)
                .map_err(|e| platform_error(e.localizedDescription().to_string()))?;
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                &self.track,
                Some(&settings),
            );
            output.setAlwaysCopiesSampleData(false);
            if !reader.canAddOutput(&output) {
                return Err(platform_error("cannot add a video output to the reader"));
            }
            reader.addOutput(&output);
            reader.setTimeRange(CMTimeRange {
                start: CMTime::new(start, FLICKS_TIMESCALE),
                duration: kCMTimePositiveInfinity,
            });
            if !reader.startReading() {
                return Err(reader_error(&reader));
            }
            self.reader = Some(Reader {
                reader,
                output,
                current: None,
                ahead: None,
            });
        }
        Ok(())
    }

    /// Reads the next decoded frame, or `None` at the end.
    fn next_frame(&mut self) -> Result<Option<(i64, CFRetained<CVPixelBuffer>)>, MediaError> {
        loop {
            let Some(reader) = self.reader.as_mut() else {
                return Ok(None);
            };
            if let Some(frame) = reader.ahead.take() {
                return Ok(Some(frame));
            }
            // SAFETY: the reader was started; copyNextSampleBuffer is synchronous.
            let sample: Option<Retained<CMSampleBuffer>> =
                unsafe { reader.output.copyNextSampleBuffer() };
            let Some(sample) = sample else {
                // SAFETY: status query.
                let status = unsafe { reader.reader.status() };
                if status == AVAssetReaderStatus::Failed {
                    return Err(reader_error(&reader.reader));
                }
                return Ok(None);
            };
            // SAFETY: valid sample buffer.
            let (pts, image) = unsafe { (sample.presentation_time_stamp(), sample.image_buffer()) };
            // Marker samples carry no image.
            let (Some(image), Some(index)) = (image, self.index_of(pts)) else {
                continue;
            };
            return Ok(Some((index, image)));
        }
    }

    /// The decoded pixel buffer showing frame `index`.
    fn pixel_buffer(&mut self, index: i64) -> Result<CFRetained<CVPixelBuffer>, MediaError> {
        check_index(&self.info, index)?;
        let forward_limit = (self.rate.as_f64().ceil() as i64).max(8);
        let restart = match self
            .reader
            .as_ref()
            .map(|r| r.current.as_ref().map(|c| c.0))
        {
            None => true,
            // Started for a later frame than the one asked for.
            Some(None) => true,
            Some(Some(current)) => index < current || index > current + forward_limit,
        };
        if !restart
            && let Some((current, buffer)) = self.reader.as_ref().and_then(|r| r.current.as_ref())
            && *current == index
        {
            return Ok(buffer.clone());
        }
        if restart {
            self.start_reader(index)?;
        }
        loop {
            match self.next_frame()? {
                Some((frame, buffer)) => {
                    let reader = self.reader.as_mut().expect("reader is running");
                    if frame < index {
                        reader.current = Some((frame, buffer));
                        continue;
                    }
                    if frame > index
                        && let Some((_, previous)) = reader.current.clone()
                    {
                        // Gap (variable frame rate): the previous sample is still
                        // on screen at `index`; keep the new one for later.
                        reader.ahead = Some((frame, buffer));
                        reader.current = Some((index, previous.clone()));
                        return Ok(previous);
                    }
                    reader.current = Some((frame, buffer.clone()));
                    return Ok(buffer);
                }
                None => {
                    // End of stream: the last frame stays on screen.
                    let last = self
                        .reader
                        .as_ref()
                        .and_then(|r| r.current.as_ref())
                        .map(|c| c.1.clone());
                    self.reader = None;
                    return last.ok_or(MediaError::FrameOutOfRange {
                        index,
                        count: self
                            .info
                            .video
                            .as_ref()
                            .and_then(|v| v.frame_count)
                            .unwrap_or(0),
                    });
                }
            }
        }
    }

    fn texture_cache(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<CFRetained<CVMetalTextureCache>, MediaError> {
        let key = std::ptr::from_ref(device) as usize;
        if let Some((cached_key, cache)) = &self.texture_cache
            && *cached_key == key
        {
            return Ok(cache.clone());
        }
        let mut out: *mut CVMetalTextureCache = std::ptr::null_mut();
        // SAFETY: valid device; `out` receives a +1 reference.
        let status = unsafe {
            CVMetalTextureCache::create(None, None, device, None, NonNull::from(&mut out))
        };
        let cache = NonNull::new(out)
            .filter(|_| status == kCVReturnSuccess)
            // SAFETY: created with a +1 retain count.
            .map(|ptr| unsafe { CFRetained::from_raw(ptr) })
            .ok_or_else(|| platform_error(format!("CVMetalTextureCacheCreate failed ({status})")))?;
        self.texture_cache = Some((key, cache.clone()));
        Ok(cache)
    }

    /// Wraps a decoded pixel buffer as a `wgpu::Texture` (no copy).
    fn wrap(
        &mut self,
        gpu: &MediaGpu,
        buffer: CFRetained<CVPixelBuffer>,
    ) -> Result<(wgpu::Texture, wgpu::TextureFormat), MediaError> {
        let ctx = gpu.context().clone();
        // SAFETY: only the device handle is cloned out of the closure.
        let device =
            unsafe { ctx.with_hal_device::<wgpu::hal::api::Metal, _>(|d| d.raw_device().clone()) }
                .ok_or_else(|| {
                    MediaError::Unsupported(
                        "hardware video decoding needs the Metal backend".into(),
                    )
                })?;
        let cache = self.texture_cache(&device)?;
        let width = CVPixelBufferGetWidth(&buffer);
        let height = CVPixelBufferGetHeight(&buffer);
        let (mtl_format, format) = match CVPixelBufferGetPixelFormatType(&buffer) {
            f if f == kCVPixelFormatType_32BGRA => {
                (MTLPixelFormat::BGRA8Unorm, wgpu::TextureFormat::Bgra8Unorm)
            }
            f if f == kCVPixelFormatType_64RGBAHalf => (
                MTLPixelFormat::RGBA16Float,
                wgpu::TextureFormat::Rgba16Float,
            ),
            other => {
                return Err(MediaError::Unsupported(format!(
                    "decoder pixel format '{}'",
                    fourcc(other)
                )));
            }
        };
        let mut out: *mut CVMetalTexture = std::ptr::null_mut();
        // SAFETY: valid cache and IOSurface-backed pixel buffer; `out` receives +1.
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &cache,
                &buffer,
                None,
                mtl_format,
                width,
                height,
                0,
                NonNull::from(&mut out),
            )
        };
        let cv_texture = NonNull::new(out)
            .filter(|_| status == kCVReturnSuccess)
            // SAFETY: created with a +1 retain count.
            .map(|ptr| unsafe { CFRetained::from_raw(ptr) })
            .ok_or_else(|| {
                platform_error(format!("CVMetalTextureCacheCreateTextureFromImage failed ({status})"))
            })?;
        let mtl: Retained<ProtocolObject<dyn MTLTexture>> =
            CVMetalTextureGetTexture(&cv_texture)
                .ok_or_else(|| platform_error("CVMetalTextureGetTexture returned nil"))?;
        let keep_alive = KeepAlive {
            _texture: cv_texture,
            _buffer: buffer,
        };
        let (width, height) = (width as u32, height as u32);
        // SAFETY: `mtl` is a 2D, single-mip texture of `format` and `width`×`height`
        // on the shared device; the drop callback keeps its IOSurface alive until
        // wgpu releases the texture (after the GPU finished using it).
        let texture = unsafe {
            let hal = wgpu::hal::metal::Device::texture_from_raw(
                mtl,
                format,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
                Some(Box::new(move || drop(keep_alive))),
            );
            ctx.device()
                .create_texture_from_hal::<wgpu::hal::api::Metal>(
                    hal,
                    &wgpu::TextureDescriptor {
                        label: Some("bae.media.avfoundation"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::TextureUses::RESOURCE,
                )
        };
        cache.flush(0);
        Ok((texture, format))
    }
}

fn rate_from(min_duration: CMTime, nominal: f32) -> Option<FrameRate> {
    let nominal = frame_rate_from_f64(f64::from(nominal));
    if min_duration.flags.contains(CMTimeFlags::Valid)
        && min_duration.value > 0
        && min_duration.timescale > 0
    {
        let (mut num, mut den) = (min_duration.timescale as u64, min_duration.value as u64);
        let g = gcd(num, den);
        (num, den) = (num / g, den / g);
        if let (Ok(num), Ok(den)) = (u32::try_from(num), u32::try_from(den)) {
            let exact = FrameRate::new(num, den);
            // The shortest sample duration is exact only if the track's timescale
            // can express the rate (29.97 in a 600 timescale reads as 20/600 =
            // 30 fps); trust it only when it matches the snapped average.
            return match nominal {
                Some(n) if (n.as_f64() - exact.as_f64()).abs() > 0.01 => Some(n),
                _ => Some(exact),
            };
        }
    }
    nominal
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

fn reader_error(reader: &AVAssetReader) -> MediaError {
    // SAFETY: error query.
    let message = unsafe { reader.error() }
        .map(|e| e.localizedDescription().to_string())
        .unwrap_or_else(|| "AVAssetReader failed".into());
    platform_error(message)
}

impl MediaSource for AvSource {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn decode_cpu(&mut self, request: &FrameRequest) -> Result<CpuFrame, MediaError> {
        let buffer = self.pixel_buffer(request.index)?;
        let video = self.info.video.as_ref().expect("movies have a picture");
        let format = match CVPixelBufferGetPixelFormatType(&buffer) {
            f if f == kCVPixelFormatType_32BGRA => PixelFormat::Bgra8,
            f if f == kCVPixelFormatType_64RGBAHalf => PixelFormat::Rgba16Float,
            other => {
                return Err(MediaError::Unsupported(format!(
                    "decoder pixel format '{}'",
                    fourcc(other)
                )));
            }
        };
        let width = CVPixelBufferGetWidth(&buffer) as u32;
        let height = CVPixelBufferGetHeight(&buffer) as u32;
        let row = (width * format.bytes_per_pixel()) as usize;
        let mut data = Vec::with_capacity(row * height as usize);
        // SAFETY: lock / read / unlock of a valid pixel buffer; rows are
        // `bytes_per_row` apart and at least `row` bytes long.
        unsafe {
            let status = CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly);
            if status != kCVReturnSuccess {
                return Err(platform_error(format!(
                    "CVPixelBufferLockBaseAddress failed ({status})"
                )));
            }
            let base = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
            let stride = CVPixelBufferGetBytesPerRow(&buffer);
            for y in 0..height as usize {
                data.extend_from_slice(std::slice::from_raw_parts(base.add(y * stride), row));
            }
            CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly);
        }
        Ok(CpuFrame {
            width,
            height,
            format,
            data,
            alpha: video.alpha,
            color: video.color.clone(),
            pixel_aspect: video.pixel_aspect,
        })
    }

    fn decode_gpu(
        &mut self,
        request: &FrameRequest,
        gpu: &MediaGpu,
    ) -> Result<GpuFrame, MediaError> {
        let buffer = self.pixel_buffer(request.index)?;
        let (texture, format) = self.wrap(gpu, buffer)?;
        let video = self.info.video.as_ref().expect("movies have a picture");
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.convert_texture(
            &SourceTexture {
                view: &view,
                format,
                width: video.width.min(texture.width()),
                height: video.height.min(texture.height()),
                alpha: video.alpha,
                color: video.color.clone(),
                pixel_aspect: video.pixel_aspect,
            },
            request,
        )
    }
}

impl Drop for AvSource {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            // SAFETY: cancelling is always allowed.
            unsafe { reader.reader.cancelReading() };
        }
    }
}
