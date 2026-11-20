//! Writes small test movies with AVAssetWriter (macOS). Frame `i` shows its
//! number as [`BITS`] vertical gray bars, black or white per bit, which
//! survives any Y'CbCr matrix / range / transfer mismatch of the round trip, so
//! decoding can check that the right frame came back ([`frame_of`]).

use std::path::Path;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor,
    AVFileTypeQuickTimeMovie, AVMediaTypeVideo, AVVideoCodecKey, AVVideoCodecTypeAppleProRes422,
    AVVideoCodecTypeAppleProRes4444, AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey,
    AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey, AVVideoWidthKey,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelFormatType_32BGRA, kCVReturnSuccess,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

/// Codec of a test movie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    /// H.264 with a keyframe every `gop` frames.
    H264 { gop: u32 },
    /// ProRes 4444 with alpha.
    ProRes4444,
    /// ProRes 422.
    ProRes422,
}

/// Bits (bars) of the frame number code.
pub const BITS: u32 = 6;
/// Alpha written into every frame (visible with ProRes 4444).
pub const ALPHA: u8 = 128;

/// RGBA of pixel column `x` of frame `i` in a `width`-wide movie.
pub fn code_pixel(i: u32, x: u32, width: u32) -> [u8; 4] {
    let bit = (x * BITS / width).min(BITS - 1);
    let v = if i >> bit & 1 == 1 { 235 } else { 20 };
    [v, v, v, ALPHA]
}

/// Decodes the frame number from a row of a decoded frame: `pixel(x)` returns
/// straight (not premultiplied) RGBA as floats.
pub fn frame_of(width: u32, pixel: impl Fn(u32) -> [f32; 4]) -> u32 {
    (0..BITS)
        .map(|bit| {
            let x = (2 * bit + 1) * width / (2 * BITS);
            let [r, g, b, _] = pixel(x);
            u32::from((r + g + b) / 3.0 > 0.5) << bit
        })
        .sum()
}

fn any(obj: &impl AsRef<AnyObject>) -> &AnyObject {
    obj.as_ref()
}

fn pixel_buffer(width: usize, height: usize, frame: u32) -> CFRetained<CVPixelBuffer> {
    let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
    // SAFETY: plain CoreVideo calls; `out` receives a +1 reference, rows are
    // `bytes_per_row` apart.
    unsafe {
        let status = CVPixelBufferCreate(
            None,
            width,
            height,
            kCVPixelFormatType_32BGRA,
            None,
            NonNull::from(&mut out),
        );
        assert_eq!(status, kCVReturnSuccess);
        let buffer = CFRetained::from_raw(NonNull::new(out).unwrap());
        CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
        let base = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
        let stride = CVPixelBufferGetBytesPerRow(&buffer);
        for y in 0..height {
            let row = std::slice::from_raw_parts_mut(base.add(y * stride), width * 4);
            for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let [r, g, b, a] = code_pixel(frame, x as u32, width as u32);
                *px = [b, g, r, a];
            }
        }
        CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
        buffer
    }
}

/// Writes `frames` frames of `width`×`height` at `rate` to `path`.
pub fn write_movie(
    path: &Path,
    codec: Codec,
    width: u32,
    height: u32,
    rate: bae_media::FrameRate,
    frames: u32,
) {
    let (num, den) = (rate.num as i32, i64::from(rate.den));
    // SAFETY: plain AVFoundation calls with valid arguments; the statics are
    // provided by AVFoundation.
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().unwrap()));
        let writer = AVAssetWriter::assetWriterWithURL_fileType_error(
            &url,
            AVFileTypeQuickTimeMovie.unwrap(),
        )
        .expect("AVAssetWriter");
        let width_n = NSNumber::new_u32(width);
        let height_n = NSNumber::new_u32(height);
        let gop_n;
        let compression: Retained<NSDictionary<NSString, AnyObject>>;
        let mut keys: Vec<&NSString> = vec![
            AVVideoCodecKey.unwrap(),
            AVVideoWidthKey.unwrap(),
            AVVideoHeightKey.unwrap(),
        ];
        let mut values: Vec<&AnyObject> = vec![
            any(match codec {
                Codec::H264 { .. } => AVVideoCodecTypeH264.unwrap(),
                Codec::ProRes4444 => AVVideoCodecTypeAppleProRes4444.unwrap(),
                Codec::ProRes422 => AVVideoCodecTypeAppleProRes422.unwrap(),
            }),
            any(&*width_n),
            any(&*height_n),
        ];
        if let Codec::H264 { gop } = codec {
            gop_n = NSNumber::new_u32(gop);
            compression = NSDictionary::from_slices(
                &[AVVideoMaxKeyFrameIntervalKey.unwrap()],
                &[any(&*gop_n)],
            );
            keys.push(AVVideoCompressionPropertiesKey.unwrap());
            values.push(any(&*compression));
        }
        let settings = NSDictionary::from_slices(&keys, &values);
        let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
            AVMediaTypeVideo.unwrap(),
            Some(&settings),
        );
        input.setExpectsMediaDataInRealTime(false);
        let adaptor =
            AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &input, None,
            );
        writer.addInput(&input);
        assert!(writer.startWriting(), "{:?}", writer.error());
        writer.startSessionAtSourceTime(CMTime::new(0, num));
        for i in 0..frames {
            while !input.isReadyForMoreMediaData() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let buffer = pixel_buffer(width as usize, height as usize, i);
            assert!(
                adaptor.appendPixelBuffer_withPresentationTime(
                    &buffer,
                    CMTime::new(i64::from(i) * den, num)
                ),
                "append frame {i}: {:?}",
                writer.error()
            );
        }
        input.markAsFinished();
        #[allow(deprecated)]
        let finished = writer.finishWriting();
        assert!(finished, "{:?}", writer.error());
    }
}
