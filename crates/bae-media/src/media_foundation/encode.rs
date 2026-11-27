use std::ptr;
use std::sync::OnceLock;

use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::GUID;

use super::platform::{Platform, timestamp};
use crate::export::convert_frame_cow;
use crate::{
    AlphaMode, CpuFrame, EncodeOutput, EncodeSettings, Encoder, EncoderFormat, MediaError,
    PixelFormat, VideoCodec,
};

fn subtype(codec: VideoCodec) -> Option<GUID> {
    match codec {
        VideoCodec::H264 => Some(MFVideoFormat_H264),
        VideoCodec::Hevc => Some(MFVideoFormat_HEVC),
        _ => None,
    }
}

fn has_encoder(codec: VideoCodec) -> Result<bool, MediaError> {
    let platform = Platform::new()?;
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: subtype(codec).unwrap(),
    };
    let mut flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
    if codec == VideoCodec::H264 {
        flags |= MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT;
    }
    let mut list = ptr::null_mut();
    let mut count = 0;
    // SAFETY: MFTEnumEx transfers an array of count owned COM references.
    unsafe {
        (platform.enumerate)(
            MFT_CATEGORY_VIDEO_ENCODER,
            flags,
            ptr::null(),
            &output,
            &mut list,
            &mut count,
        )
        .ok()?;
        if !list.is_null() {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(list, count as usize));
            CoTaskMemFree(Some(list.cast()));
        }
    }
    Ok(count != 0)
}

/// System H.264 encoder or hardware HEVC encoder, cached for the format menu.
pub fn encoder_available(codec: VideoCodec) -> bool {
    static H264: OnceLock<bool> = OnceLock::new();
    static HEVC: OnceLock<bool> = OnceLock::new();
    let cache = match codec {
        VideoCodec::H264 => &H264,
        VideoCodec::Hevc => &HEVC,
        _ => return false,
    };
    *cache.get_or_init(|| has_encoder(codec).unwrap_or(false))
}

/// MP4 encoder using the Media Foundation Sink Writer.
#[derive(Debug, Default, Clone, Copy)]
pub struct MfWriterFormat;

impl EncoderFormat for MfWriterFormat {
    fn id(&self) -> &str {
        "mediafoundation"
    }
    fn supports(&self, settings: &EncodeSettings) -> bool {
        encoder_available(settings.codec)
            && settings.path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("m4v")
            })
    }
    fn create(&self, settings: &EncodeSettings) -> Result<Box<dyn Encoder>, MediaError> {
        Ok(Box::new(Writer::new(settings)?))
    }
}

struct Writer {
    writer: IMFSinkWriter,
    stream: u32,
    audio_stream: Option<u32>,
    audio_frames: i64,
    settings: EncodeSettings,
    frames: u64,
    platform: Platform,
}

// SAFETY: the free-threaded Sink Writer is used serially; Platform maintains the MTA.
unsafe impl Send for Writer {}

fn pack(a: u32, b: u32) -> u64 {
    (u64::from(a) << 32) | u64::from(b)
}

impl Writer {
    fn new(settings: &EncodeSettings) -> Result<Self, MediaError> {
        settings.validate()?;
        let subtype = subtype(settings.codec)
            .ok_or_else(|| MediaError::Unsupported(format!("{:?}", settings.codec)))?;
        if settings.bit_depth != 8 || settings.has_alpha() {
            return Err(MediaError::Unsupported(
                "Media Foundation MP4 output requires opaque 8-bit video".into(),
            ));
        }
        let platform = Platform::new()?;
        let attrs = platform.attributes()?;
        let output = platform.video_type(&subtype)?;
        let input = platform.video_type(&MFVideoFormat_RGB32)?;
        // SAFETY: MF media types and attributes remain live through negotiation.
        unsafe {
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
            for ty in [&input, &output] {
                ty.SetUINT64(&MF_MT_FRAME_SIZE, pack(settings.width, settings.height))?;
                ty.SetUINT64(
                    &MF_MT_FRAME_RATE,
                    pack(settings.frame_rate.num, settings.frame_rate.den),
                )?;
                ty.SetUINT64(
                    &MF_MT_PIXEL_ASPECT_RATIO,
                    pack(settings.pixel_aspect.num, settings.pixel_aspect.den),
                )?;
                ty.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            }
            input.SetUINT32(&MF_MT_DEFAULT_STRIDE, settings.width * 4)?;
            super::color::write(&output, &settings.color)?;
            super::color::write(&input, &settings.color)?;
            input.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_0_255.0 as u32)?;
            output.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
            output.SetUINT32(
                &MF_MT_AVG_BITRATE,
                settings
                    .bit_rate
                    .unwrap_or(16_000_000)
                    .try_into()
                    .map_err(|_| MediaError::Unsupported("MP4 bitrate exceeds UINT32".into()))?,
            )?;
            let params = platform.attributes()?;
            if let Some(interval) = settings.keyframe_interval {
                params.SetUINT32(&CODECAPI_AVEncMPVGOPSize, interval)?;
            }
            if let Some(q) = settings.quality {
                params.SetUINT32(
                    &CODECAPI_AVEncCommonQuality,
                    (100.0 * q.clamp(0.0, 1.0)).round() as u32,
                )?;
            }
            if let Some(dir) = settings.path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| MediaError::io(dir, e))?;
            }
            let writer: IMFSinkWriter =
                platform.open_url(&CLSID_MFSinkWriter, &settings.path, &attrs)?;
            let start = || -> windows::core::Result<(u32, Option<u32>)> {
                let stream = writer.AddStream(&output)?;
                writer.SetInputMediaType(stream, &input, &params)?;
                let audio = settings
                    .audio
                    .as_ref()
                    .map(|settings| add_audio(&platform, &writer, settings))
                    .transpose()?;
                writer.BeginWriting()?;
                Ok((stream, audio))
            };
            let (stream, audio_stream) = match start() {
                Ok(stream) => stream,
                Err(error) => {
                    drop(writer);
                    let _ = std::fs::remove_file(&settings.path);
                    return Err(error.into());
                }
            };
            Ok(Self {
                writer,
                stream,
                audio_stream,
                audio_frames: 0,
                settings: settings.clone(),
                frames: 0,
                platform,
            })
        }
    }
}

fn add_audio(
    platform: &Platform,
    writer: &IMFSinkWriter,
    settings: &crate::export::AudioEncodeSettings,
) -> windows::core::Result<u32> {
    let pcm = platform.stream_type(&MFMediaType_Audio, &MFAudioFormat_PCM)?;
    let aac = platform.stream_type(&MFMediaType_Audio, &MFAudioFormat_AAC)?;
    let channels = u32::from(settings.channels);
    // SAFETY: both live media types describe the stream supplied to this writer.
    unsafe {
        for ty in [&pcm, &aac] {
            ty.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
            ty.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, settings.sample_rate)?;
            ty.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        }
        pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2)?;
        pcm.SetUINT32(
            &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
            settings.sample_rate * channels * 2,
        )?;
        aac.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?;
        let stream = writer.AddStream(&aac)?;
        writer.SetInputMediaType(stream, &pcm, None)?;
        Ok(stream)
    }
}

impl Encoder for Writer {
    fn settings(&self) -> &EncodeSettings {
        &self.settings
    }
    fn frames_written(&self) -> u64 {
        self.frames
    }
    fn input_format(&self) -> Option<(PixelFormat, AlphaMode)> {
        Some((PixelFormat::Bgra8, AlphaMode::Opaque))
    }
    fn push_frame(&mut self, frame: &CpuFrame) -> Result<(), MediaError> {
        if (frame.width, frame.height) != (self.settings.width, self.settings.height)
            || frame.data.len() != frame.expected_len()
        {
            return Err(MediaError::Unsupported(
                "invalid MP4 input frame dimensions or length".into(),
            ));
        }
        let frame = convert_frame_cow(frame, PixelFormat::Bgra8, AlphaMode::Opaque);
        let rate = self.settings.frame_rate;
        let pts = timestamp(self.frames as i64, rate);
        self.write_sample(
            self.stream,
            &frame.data,
            pts,
            timestamp(self.frames as i64 + 1, rate) - pts,
        )?;
        self.frames += 1;
        Ok(())
    }

    fn push_audio(&mut self, pcm: &crate::AudioBuffer) -> Result<(), MediaError> {
        let (Some(stream), Some(settings)) = (self.audio_stream, &self.settings.audio) else {
            return Err(MediaError::Unsupported("no audio stream".into()));
        };
        settings.validate_input(pcm)?;
        if pcm.frames() == 0 {
            return Ok(());
        }
        let bytes: Vec<u8> = pcm
            .samples
            .iter()
            .flat_map(|v| ((v.clamp(-1.0, 1.0) * 32768.0).round() as i16).to_le_bytes())
            .collect();
        let at =
            |sample: i64| ((i128::from(sample) * 10_000_000) / i128::from(pcm.sample_rate)) as i64;
        let end = self.audio_frames + pcm.frames() as i64;
        self.write_sample(
            stream,
            &bytes,
            at(self.audio_frames),
            at(end) - at(self.audio_frames),
        )?;
        self.audio_frames = end;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<EncodeOutput, MediaError> {
        // SAFETY: all frames have been submitted to this synchronous sink.
        if let Err(error) = unsafe { self.writer.Finalize() } {
            self.abort();
            return Err(error.into());
        }
        Ok(EncodeOutput {
            files: vec![self.settings.path.clone()],
            frames: self.frames,
            stats: crate::EncodeStats {
                threads: 1,
                bytes: std::fs::metadata(&self.settings.path)
                    .map_err(|e| MediaError::io(&self.settings.path, e))?
                    .len(),
                ..Default::default()
            },
        })
    }
    fn abort(self: Box<Self>) {
        let path = self.settings.path.clone();
        drop(self);
        let _ = std::fs::remove_file(path);
    }
}

impl Writer {
    fn write_sample(
        &self,
        stream: u32,
        data: &[u8],
        pts: i64,
        duration: i64,
    ) -> Result<(), MediaError> {
        let len = u32::try_from(data.len())
            .map_err(|_| MediaError::Unsupported("sample exceeds UINT32".into()))?;
        // SAFETY: the allocated and locked buffer has exactly len writable bytes;
        // the sample owns it while the asynchronous encoder consumes the frame.
        unsafe {
            let mut buffer = None;
            (self.platform.buffer)(len, &mut buffer).ok()?;
            let buffer = buffer.unwrap();
            let mut dest = ptr::null_mut();
            buffer.Lock(&mut dest, None, None)?;
            ptr::copy_nonoverlapping(data.as_ptr(), dest, len as usize);
            buffer.Unlock()?;
            buffer.SetCurrentLength(len)?;
            let mut sample = None;
            (self.platform.sample)(&mut sample).ok()?;
            let sample = sample.unwrap();
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(pts)?;
            sample.SetSampleDuration(duration)?;
            self.writer.WriteSample(stream, &sample)?;
        }
        Ok(())
    }
}
