//! Windows movie I/O through Media Foundation. The Source Reader uses DXVA on
//! D3D11; the BGRA result takes one upload into the compositor's wgpu device.

mod color;
mod encode;
mod platform;
#[cfg(test)]
mod tests;

pub use encode::{MfWriterFormat, encoder_available};
use platform::{Platform, frame_index, timestamp};

use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;
use windows::core::{GUID, Interface};

use crate::source::{check_index, sniff_iso_bmff};
use crate::{
    AlphaMode, AudioInfo, ColorInfo, ColorRange, CpuFrame, FrameRate, FrameRequest,
    MatrixCoefficients, MediaError, MediaFormat, MediaInfo, MediaKind, MediaSource, PixelAspect,
    PixelFormat, SampleType, VideoInfo,
};

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

/// QuickTime / MPEG-4 video through the system's H.264 and HEVC decoders.
#[derive(Debug, Default, Clone, Copy)]
pub struct MediaFoundationFormat;

impl MediaFormat for MediaFoundationFormat {
    fn id(&self) -> &str {
        "mediafoundation"
    }
    fn extensions(&self) -> &[&str] {
        &["mp4", "mov", "m4v"]
    }
    fn sniff(&self, header: &[u8]) -> bool {
        sniff_iso_bmff(header)
    }
    fn is_image(&self) -> bool {
        false
    }
    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        Ok(Box::new(Source::open(path)?))
    }
}

struct Source {
    reader: IMFSourceReader,
    info: MediaInfo,
    rate: FrameRate,
    next: Option<i64>,
    _platform: Platform,
}

// SAFETY: the synchronous MF reader is free-threaded; MediaSource serializes reads.
// The platform holds an MTA usage reference across thread moves.
unsafe impl Send for Source {}

fn pair(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}

fn decode_error(hevc: bool, error: windows::core::Error) -> MediaError {
    if hevc
        && matches!(
            error.code(),
            MF_E_TOPO_CODEC_NOT_FOUND | MF_E_INVALIDMEDIATYPE
        )
    {
        MediaError::HevcDecoderUnavailable
    } else {
        error.into()
    }
}

impl Source {
    fn open(path: &Path) -> Result<Self, MediaError> {
        let platform = Platform::new()?;
        let attrs = platform.attributes()?;
        // SAFETY: the reader retains its D3D manager and all supplied COM objects.
        unsafe {
            attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &platform.device_manager()?)?;
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
            let reader: IMFSourceReader = platform.open_url(&CLSID_MFSourceReader, path, &attrs)?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(VIDEO, true)?;
            let native = reader.GetNativeMediaType(VIDEO, 0)?;
            let subtype = native.GetGUID(&MF_MT_SUBTYPE)?;
            let codec = if subtype == MFVideoFormat_H264 {
                "h264"
            } else if subtype == MFVideoFormat_HEVC {
                "hevc"
            } else {
                return Err(MediaError::Unsupported(format!(
                    "Media Foundation video subtype {subtype:?}"
                )));
            };
            // MF exposes coded dimensions (e.g. 320x256 for a 320x240 HEVC
            // frame). Ask the video processor to output the display aperture.
            let mut area = [0; size_of::<MFVideoArea>()];
            let (width, height) = if native
                .GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut area, None)
                .is_ok()
            {
                let area = area.as_ptr().cast::<MFVideoArea>().read_unaligned();
                (area.Area.cx as u32, area.Area.cy as u32)
            } else {
                pair(native.GetUINT64(&MF_MT_FRAME_SIZE)?)
            };
            let (num, den) = pair(native.GetUINT64(&MF_MT_FRAME_RATE)?);
            let rate = FrameRate::new(num, den);
            if !crate::time::is_valid_rate(rate) {
                return Err(MediaError::decode(path, "missing video frame rate"));
            }
            let duration = match crate::mp4::video_duration(path) {
                Some(duration) => duration,
                None => i64::try_from(&reader.GetPresentationAttribute(
                    MF_SOURCE_READER_MEDIASOURCE.0 as u32,
                    &MF_PD_DURATION,
                )?)?,
            };
            let count = frame_index(duration, rate);
            let (x, y) = pair(native.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO).unwrap_or(0));
            let ten_bit = subtype == MFVideoFormat_HEVC
                && native.GetUINT32(&MF_MT_MPEG2_PROFILE).ok()
                    == Some(eAVEncH265VProfile_Main_420_10.0 as u32);
            let output = platform.video_type(if ten_bit {
                &MFVideoFormat_A2R10G10B10
            } else {
                &MFVideoFormat_ARGB32
            })?;
            output.SetUINT64(
                &MF_MT_FRAME_SIZE,
                (u64::from(width) << 32) | u64::from(height),
            )?;
            reader
                .SetCurrentMediaType(VIDEO, None, &output)
                .map_err(|e| decode_error(subtype == MFVideoFormat_HEVC, e))?;
            let video = VideoInfo {
                width,
                height,
                frame_rate: Some(rate),
                frame_count: Some(count as u64),
                pixel_aspect: PixelAspect::new(x, y),
                bit_depth: if ten_bit { 10 } else { 8 },
                sample_type: SampleType::UInt,
                color: color::read(&native),
                alpha: AlphaMode::Opaque,
                codec: codec.into(),
            };
            let info = MediaInfo {
                path: path.into(),
                kind: MediaKind::Movie,
                format: "mediafoundation".into(),
                video: Some(video),
                audio: audio_streams(&reader, path)?,
                timecode: None,
                sequence: None,
            };
            Ok(Self {
                reader,
                info,
                rate,
                next: Some(0),
                _platform: platform,
            })
        }
    }

    fn sample(&mut self, index: i64) -> Result<IMFSample, MediaError> {
        check_index(&self.info, index)?;
        // SAFETY: synchronous reads; MF seeks to the preceding keyframe. Discard
        // preroll by presentation timestamp, keeping this reader for the next call.
        unsafe {
            if self.next.take() != Some(index) {
                self.reader
                    .SetCurrentPosition(&GUID::zeroed(), &timestamp(index, self.rate).into())?;
            }
            loop {
                let mut sample = None;
                let mut pts = 0;
                let mut flags = 0;
                self.reader
                    .ReadSample(
                        VIDEO,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut pts),
                        Some(&mut sample),
                    )
                    .map_err(|e| {
                        decode_error(self.info.video.as_ref().unwrap().codec == "hevc", e)
                    })?;
                if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                    return Err(MediaError::decode(&self.info.path, "video reader failed"));
                }
                if let Some(sample) = sample
                    && frame_index(pts, self.rate) >= index
                {
                    self.next = Some(index + 1);
                    return Ok(sample);
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    return Err(MediaError::decode(
                        &self.info.path,
                        format!("end of video before frame {index}"),
                    ));
                }
            }
        }
    }
}

/// Read audio headers from the same demuxer without opening an audio decoder.
#[allow(non_upper_case_globals)] // Windows SDK format GUIDs.
fn audio_streams(reader: &IMFSourceReader, path: &Path) -> windows::core::Result<Vec<AudioInfo>> {
    let mut streams = Vec::new();
    // SAFETY: query native types by stream index until MF reports the last stream.
    unsafe {
        for index in 0.. {
            let ty = match reader.GetNativeMediaType(index, 0) {
                Ok(ty) => ty,
                Err(e) if e.code() == MF_E_INVALIDSTREAMNUMBER => break,
                Err(e) => return Err(e),
            };
            if ty.GetMajorType()? != MFMediaType_Audio {
                continue;
            }
            let codec = match ty.GetGUID(&MF_MT_SUBTYPE)? {
                MFAudioFormat_AAC => "aac",
                MFAudioFormat_MP3 => "mp3",
                MFAudioFormat_PCM => "pcm",
                MFAudioFormat_Float => "pcm_f32",
                MFAudioFormat_FLAC => "flac",
                MFAudioFormat_ALAC => "alac",
                _ => "unknown",
            };
            let sample_rate = ty.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0);
            let frame_count = if streams.is_empty() {
                crate::mp4::audio_trim(path, sample_rate).and_then(|trim| trim.valid)
            } else {
                None
            };
            streams.push(AudioInfo {
                sample_rate,
                channels: ty.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0) as u16,
                frame_count,
                codec: codec.into(),
            });
        }
    }
    Ok(streams)
}

impl MediaSource for Source {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn decode_cpu(&mut self, request: &FrameRequest) -> Result<CpuFrame, MediaError> {
        let sample = self.sample(request.index)?;
        let video = self.info.video.as_ref().unwrap();
        let format = if video.bit_depth > 8 {
            PixelFormat::Rgba16
        } else {
            PixelFormat::Bgra8
        };
        // SAFETY: the DXGI-backed video processor output is one 2D buffer.
        // ContiguousCopyTo removes row padding and checks the destination length.
        unsafe {
            let buffer: IMF2DBuffer = sample.GetBufferByIndex(0)?.cast()?;
            let mut data = vec![0; video.width as usize * video.height as usize * 4];
            buffer.ContiguousCopyTo(&mut data)?;
            if format == PixelFormat::Rgba16 {
                // DXGI_FORMAT_R10G10B10A2_UNORM, least significant bits first.
                let mut rgba = Vec::with_capacity(data.len() * 2);
                for &pixel in data.as_chunks::<4>().0 {
                    let packed = u32::from_le_bytes(pixel);
                    for shift in [0, 10, 20] {
                        let v = ((packed >> shift) & 1023u32) as u16;
                        rgba.extend_from_slice(&((v << 6) | (v >> 4)).to_le_bytes());
                    }
                    rgba.extend_from_slice(&u16::MAX.to_le_bytes());
                }
                data = rgba;
            }
            Ok(CpuFrame {
                width: video.width,
                height: video.height,
                format,
                data,
                alpha: AlphaMode::Opaque,
                color: ColorInfo {
                    matrix: MatrixCoefficients::Identity,
                    range: ColorRange::Full,
                    ..video.color.clone()
                },
                pixel_aspect: video.pixel_aspect,
            })
        }
    }
}
