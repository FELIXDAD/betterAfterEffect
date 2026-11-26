//! The AVAssetWriter audio input accepts interleaved PCM tagged with its sample clock.
use super::encode::{any, dict, writer_error};
use crate::export::AudioEncodeSettings;
use crate::{AudioBuffer, MediaError};
use objc2::rc::Retained;
use objc2_av_foundation::{AVAssetWriter, AVAssetWriterInput, AVMediaTypeAudio};
use objc2_avf_audio::{AVEncoderBitRateKey, AVFormatIDKey, AVNumberOfChannelsKey, AVSampleRateKey};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked,
    kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescriptionCreate, CMAudioSampleBufferCreateReadyWithPacketDescriptions,
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime, kCMBlockBufferAssureMemoryNowFlag,
};
use objc2_foundation::NSNumber;
use std::ptr::{self, NonNull};

fn check(status: i32) -> Result<(), MediaError> {
    if status == 0 {
        Ok(())
    } else {
        Err(MediaError::Platform {
            framework: "CoreMedia",
            message: format!("creating PCM sample buffer: OSStatus {status}"),
        })
    }
}

pub(super) struct AudioInput {
    pub input: Retained<AVAssetWriterInput>,
    format: CFRetained<CMFormatDescription>,
    frames: i64,
}
impl AudioInput {
    pub fn add(writer: &AVAssetWriter, settings: &AudioEncodeSettings) -> Result<Self, MediaError> {
        let channels = u32::from(settings.channels);
        let mut description = AudioStreamBasicDescription {
            mSampleRate: f64::from(settings.sample_rate),
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsPacked | kAudioFormatFlagIsFloat,
            mBytesPerPacket: channels * 4,
            mFramesPerPacket: 1,
            mBytesPerFrame: channels * 4,
            mChannelsPerFrame: channels,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        // SAFETY: all arrays, output pointers and format objects live through their calls.
        unsafe {
            let mut format = ptr::null();
            check(CMAudioFormatDescriptionCreate(
                None,
                NonNull::from(&mut description),
                0,
                ptr::null(),
                0,
                ptr::null(),
                None,
                NonNull::from(&mut format),
            ))?;
            let format =
                CFRetained::from_raw(NonNull::new(format.cast_mut()).expect("format on success"));
            let codec = NSNumber::new_u32(kAudioFormatMPEG4AAC);
            let rate = NSNumber::new_u32(settings.sample_rate);
            let count = NSNumber::new_u32(channels);
            let bits = NSNumber::new_u32(192_000);
            let output = dict(&[
                (AVFormatIDKey.expect("audio format"), any(&*codec)),
                (AVSampleRateKey.expect("sample rate"), any(&*rate)),
                (AVNumberOfChannelsKey.expect("channels"), any(&*count)),
                (AVEncoderBitRateKey.expect("bit rate"), any(&*bits)),
            ]);
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                AVMediaTypeAudio.expect("audio"),
                Some(&output),
            );
            input.setExpectsMediaDataInRealTime(false);
            if !writer.canAddInput(&input) {
                return Err(writer_error(writer, "cannot add AAC input"));
            }
            writer.addInput(&input);
            Ok(Self {
                input,
                format,
                frames: 0,
            })
        }
    }

    pub fn append(&mut self, writer: &AVAssetWriter, pcm: &AudioBuffer) -> Result<(), MediaError> {
        let count = pcm.frames();
        if count == 0 {
            return Ok(());
        }
        let size = std::mem::size_of_val(pcm.samples.as_slice());
        // SAFETY: CoreMedia allocates its own block; the copy completes before the source
        // slice goes out of scope. An appended sample retains its block and description.
        unsafe {
            let mut block = ptr::null_mut();
            check(CMBlockBuffer::create_with_memory_block(
                None,
                ptr::null_mut(),
                size,
                None,
                ptr::null(),
                0,
                size,
                kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut block),
            ))?;
            let block = CFRetained::from_raw(NonNull::new(block).expect("block on success"));
            check(CMBlockBuffer::replace_data_bytes(
                NonNull::from(&pcm.samples[0]).cast(),
                &block,
                0,
                size,
            ))?;
            let mut sample = ptr::null_mut();
            let pts = CMTime::new(self.frames, pcm.sample_rate as i32);
            check(CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &block,
                &self.format,
                count as isize,
                pts,
                ptr::null(),
                NonNull::from(&mut sample),
            ))?;
            let sample: CFRetained<CMSampleBuffer> =
                CFRetained::from_raw(NonNull::new(sample).expect("sample on success"));
            if !self.input.appendSampleBuffer(&sample) {
                return Err(writer_error(writer, "cannot append PCM samples"));
            }
        }
        self.frames += count as i64;
        Ok(())
    }
}
