//! Audio: decoded sample access ([`AudioSource`]), sample ↔ [`Time`]
//! conversion and waveform data for the timeline ([`Waveform`]).
//!
//! [`FileAudio`] decodes files with symphonia (WAV, AIFF, MP4 / QuickTime /
//! M4A with AAC, MP3, FLAC); [`probe`] lists a file's audio streams without
//! decoding. [`MemoryAudio`] serves generated audio and tests. Playback (cpal)
//! comes later.
//!
//! 705,600,000 flicks per second is divisible by every common sample rate
//! (8 k … 192 kHz, 44.1 k family included), so sample positions convert to
//! [`Time`] exactly.

use crate::meta::AudioInfo;
use crate::source::MediaError;
use crate::time::Time;

/// Decoded audio: interleaved 32-bit float samples in `-1..=1`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioBuffer {
    /// Samples per second.
    pub sample_rate: u32,
    /// Channels per sample frame.
    pub channels: u16,
    /// `frames * channels` interleaved samples.
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    /// Sample frames in the buffer.
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    /// Duration of the buffer.
    pub fn duration(&self) -> Time {
        sample_to_time(self.frames() as i64, self.sample_rate)
    }
}

/// Start time of sample frame `sample` at `sample_rate`.
pub fn sample_to_time(sample: i64, sample_rate: u32) -> Time {
    let flicks =
        i128::from(sample) * i128::from(Time::FLICKS_PER_SECOND) / i128::from(sample_rate.max(1));
    Time::from_flicks(flicks as i64)
}

/// The sample frame playing at `time` (floor).
pub fn time_to_sample(time: Time, sample_rate: u32) -> i64 {
    (i128::from(time.flicks()) * i128::from(sample_rate))
        .div_euclid(i128::from(Time::FLICKS_PER_SECOND)) as i64
}

/// An audio stream with random access by sample frame.
pub trait AudioSource: Send {
    /// Stream description.
    fn info(&self) -> &AudioInfo;

    /// Decodes up to `count` sample frames starting at sample frame `start`;
    /// fewer at the end of the stream, none past it.
    fn read(&mut self, start: u64, count: u64) -> Result<AudioBuffer, MediaError>;
}

/// Audio held in memory (generated tones, tests, decoded clips).
#[derive(Clone, Debug)]
pub struct MemoryAudio {
    info: AudioInfo,
    samples: Vec<f32>,
}

impl MemoryAudio {
    /// Wraps interleaved samples.
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<f32>) -> Self {
        let channels = channels.max(1);
        MemoryAudio {
            info: AudioInfo {
                sample_rate,
                channels,
                frame_count: Some((samples.len() / usize::from(channels)) as u64),
                codec: "pcm_f32".into(),
            },
            samples,
        }
    }
}

impl AudioSource for MemoryAudio {
    fn info(&self) -> &AudioInfo {
        &self.info
    }

    fn read(&mut self, start: u64, count: u64) -> Result<AudioBuffer, MediaError> {
        let channels = usize::from(self.info.channels);
        let frames = self.samples.len() / channels;
        let start = (start as usize).min(frames);
        let end = start.saturating_add(count as usize).min(frames);
        Ok(AudioBuffer {
            sample_rate: self.info.sample_rate,
            channels: self.info.channels,
            samples: self.samples[start * channels..end * channels].to_vec(),
        })
    }
}

/// Audio streams of a file (empty when it has none or the container is not
/// supported). Reads headers only.
pub fn probe(path: &std::path::Path) -> Vec<AudioInfo> {
    let Ok(format) = open_format(path) else {
        return Vec::new();
    };
    format
        .tracks()
        .iter()
        .filter_map(|track| {
            let params = track.codec_params.as_ref()?.audio()?;
            Some(stream_info(track, params))
        })
        .enumerate()
        .map(|(i, mut info)| {
            // The gapless trim is read for the first audio track only.
            if i == 0
                && let Some(trim) = mp4_trim(path, info.sample_rate)
            {
                info.frame_count = trimmed_count(&info, trim);
            }
            info
        })
        .collect()
}

/// Encoder delay / padding of MP4-family files (symphonia does not apply it).
fn mp4_trim(path: &std::path::Path, sample_rate: u32) -> Option<crate::mp4::AudioTrim> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    matches!(ext.as_str(), "mp4" | "m4a" | "m4v" | "mov" | "3gp" | "qt")
        .then(|| crate::mp4::audio_trim(path, sample_rate))
        .flatten()
}

fn trimmed_count(info: &AudioInfo, trim: crate::mp4::AudioTrim) -> Option<u64> {
    trim.valid
        .or_else(|| info.frame_count.map(|n| n.saturating_sub(trim.priming)))
}

fn open_format(
    path: &std::path::Path,
) -> Result<Box<dyn symphonia::core::formats::FormatReader>, MediaError> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = std::fs::File::open(path).map_err(|e| MediaError::io(path, e))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| MediaError::decode(path, e))
}

fn stream_info(
    track: &symphonia::core::formats::Track,
    params: &symphonia::core::codecs::audio::AudioCodecParameters,
) -> AudioInfo {
    let sample_rate = params.sample_rate.unwrap_or(0);
    let codec = symphonia::default::get_codecs()
        .get_audio_decoder(params.codec)
        .map_or("unknown", |d| d.codec.info.short_name)
        .to_owned();
    AudioInfo {
        sample_rate,
        channels: params.channels.as_ref().map_or(0, |c| c.count()) as u16,
        frame_count: track
            .num_frames
            .map(|n| frames_from_ts(n as i64, track.time_base, sample_rate).max(0) as u64),
        codec,
    }
}

/// Track time-base units → sample frames.
fn frames_from_ts(ts: i64, time_base: Option<symphonia::core::units::TimeBase>, rate: u32) -> i64 {
    match time_base {
        Some(tb) if !(tb.numer.get() == 1 && tb.denom.get() == rate) => {
            (i128::from(ts) * i128::from(rate) * i128::from(tb.numer.get())
                / i128::from(tb.denom.get())) as i64
        }
        _ => ts,
    }
}

/// Sample frames → track time-base units.
fn ts_from_frames(
    frames: i64,
    time_base: Option<symphonia::core::units::TimeBase>,
    rate: u32,
) -> i64 {
    match time_base {
        Some(tb) if !(tb.numer.get() == 1 && tb.denom.get() == rate) => {
            (i128::from(frames) * i128::from(tb.denom.get())
                / (i128::from(rate.max(1)) * i128::from(tb.numer.get()))) as i64
        }
        _ => frames,
    }
}

/// An audio file decoded with symphonia: the first audio stream, random
/// access by sample frame. Reading forward continues decoding; a jump back or
/// far ahead seeks (sample accurate).
pub struct FileAudio {
    path: std::path::PathBuf,
    info: AudioInfo,
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
    track_id: u32,
    time_base: Option<symphonia::core::units::TimeBase>,
    /// Decoded samples (interleaved) starting at sample frame `start`.
    window: Vec<f32>,
    start: u64,
    /// No packets left.
    at_end: bool,
    /// Encoder delay to drop (decoded frame `priming` is frame 0).
    priming: u64,
}

impl std::fmt::Debug for FileAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileAudio")
            .field("path", &self.path)
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

/// How far ahead (in seconds) reading continues instead of seeking.
const READ_AHEAD_SECONDS: u64 = 2;

impl FileAudio {
    /// Opens the first audio stream of `path`.
    pub fn open(path: &std::path::Path) -> Result<Self, MediaError> {
        use symphonia::core::codecs::audio::AudioDecoderOptions;
        use symphonia::core::formats::TrackType;

        let format = open_format(path)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| MediaError::decode(path, "no audio stream"))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| MediaError::decode(path, "no audio parameters"))?;
        let info = stream_info(track, params);
        if info.sample_rate == 0 || info.channels == 0 {
            return Err(MediaError::decode(
                path,
                "audio stream without rate or channels",
            ));
        }
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|e| MediaError::decode(path, e))?;
        let (track_id, time_base) = (track.id, track.time_base);
        let mut info = info;
        let priming = match mp4_trim(path, info.sample_rate) {
            Some(trim) => {
                info.frame_count = trimmed_count(&info, trim);
                trim.priming
            }
            None => 0,
        };
        Ok(FileAudio {
            path: path.to_owned(),
            info,
            format,
            decoder,
            track_id,
            time_base,
            window: Vec::new(),
            start: 0,
            at_end: false,
            priming,
        })
    }

    fn channels(&self) -> usize {
        usize::from(self.info.channels)
    }

    /// End of the decoded window, in sample frames.
    fn window_end(&self) -> u64 {
        self.start + (self.window.len() / self.channels()) as u64
    }

    fn seek(&mut self, frame: u64) -> Result<(), MediaError> {
        use symphonia::core::formats::{SeekMode, SeekTo};
        use symphonia::core::units::Timestamp;

        let ts = ts_from_frames(
            (frame + self.priming) as i64,
            self.time_base,
            self.info.sample_rate,
        );
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: Timestamp::new(ts),
                    track_id: self.track_id,
                },
            )
            .map_err(|e| MediaError::decode(&self.path, e))?;
        self.decoder.reset();
        self.window.clear();
        self.start = (frames_from_ts(
            seeked.actual_ts.get(),
            self.time_base,
            self.info.sample_rate,
        ) - self.priming as i64)
            .max(0) as u64;
        self.at_end = false;
        Ok(())
    }

    /// Decodes the next packet of the stream into the window.
    fn decode_next(&mut self) -> Result<(), MediaError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => {
                    self.at_end = true;
                    return Ok(());
                }
                Err(e) => return Err(MediaError::decode(&self.path, e)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let pts = frames_from_ts(packet.pts.get(), self.time_base, self.info.sample_rate)
                - self.priming as i64;
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // A corrupt packet: skip it, like players do.
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(e) => return Err(MediaError::decode(&self.path, e)),
            };
            let mut samples = vec![0.0_f32; decoded.samples_interleaved()];
            decoded.copy_to_slice_interleaved(&mut samples);
            if self.window.is_empty() {
                // The packet's time stamp positions the window (after a seek
                // the first packet may start before the requested frame).
                // Frames before 0 are encoder delay.
                if pts < 0 {
                    let skip = ((-pts) as usize * self.channels()).min(samples.len());
                    samples.drain(..skip);
                    if samples.is_empty() {
                        continue;
                    }
                }
                self.start = pts.max(0) as u64;
            }
            self.window.extend_from_slice(&samples);
            return Ok(());
        }
    }
}

impl AudioSource for FileAudio {
    fn info(&self) -> &AudioInfo {
        &self.info
    }

    fn read(&mut self, start: u64, count: u64) -> Result<AudioBuffer, MediaError> {
        let channels = self.channels();
        // Encoder padding past the valid length is not audio.
        let end = start
            .saturating_add(count)
            .min(self.info.frame_count.unwrap_or(u64::MAX));
        let ahead = u64::from(self.info.sample_rate) * READ_AHEAD_SECONDS;
        if start < self.start || start > self.window_end() + ahead {
            self.seek(start)?;
        }
        while self.window_end() < end && !self.at_end {
            self.decode_next()?;
        }
        // Drop what lies before `start` to keep the window small.
        if start > self.start {
            let drop = ((start - self.start) as usize * channels).min(self.window.len());
            self.window.drain(..drop);
            self.start += (drop / channels) as u64;
        }
        let from = ((start.saturating_sub(self.start)) as usize * channels).min(self.window.len());
        let to = ((end.saturating_sub(self.start)) as usize * channels).min(self.window.len());
        Ok(AudioBuffer {
            sample_rate: self.info.sample_rate,
            channels: self.info.channels,
            samples: self.window[from..to].to_vec(),
        })
    }
}

/// Min/max peaks for drawing a waveform: one `[min, max]` pair per bucket per
/// channel. Build a fine level once with [`Waveform::from_source`] and derive
/// coarser zoom levels with [`Waveform::downsample`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Waveform {
    /// Sample rate of the source.
    pub sample_rate: u32,
    /// Sample frames summarized by one bucket.
    pub frames_per_bucket: u32,
    /// Channels.
    pub channels: u16,
    /// `buckets * channels` pairs, interleaved by channel.
    pub peaks: Vec<[f32; 2]>,
}

impl Waveform {
    /// Reads the whole source in chunks and summarizes `frames_per_bucket`
    /// sample frames per bucket (e.g. 256 at 48 kHz ≈ 5.3 ms).
    pub fn from_source(
        source: &mut dyn AudioSource,
        frames_per_bucket: u32,
    ) -> Result<Self, MediaError> {
        let frames_per_bucket = frames_per_bucket.max(1);
        let info = source.info().clone();
        let channels = usize::from(info.channels.max(1));
        let chunk = u64::from(frames_per_bucket) * 1024;
        let mut peaks = Vec::new();
        let mut bucket = vec![[f32::INFINITY, f32::NEG_INFINITY]; channels];
        let mut in_bucket = 0_u32;
        let mut position = 0_u64;
        loop {
            let buffer = source.read(position, chunk)?;
            let frames = buffer.frames();
            if frames == 0 {
                break;
            }
            for frame in buffer.samples.chunks_exact(channels) {
                for (peak, &sample) in bucket.iter_mut().zip(frame) {
                    peak[0] = peak[0].min(sample);
                    peak[1] = peak[1].max(sample);
                }
                in_bucket += 1;
                if in_bucket == frames_per_bucket {
                    peaks.extend_from_slice(&bucket);
                    bucket.fill([f32::INFINITY, f32::NEG_INFINITY]);
                    in_bucket = 0;
                }
            }
            position += frames as u64;
        }
        if in_bucket > 0 {
            peaks.extend_from_slice(&bucket);
        }
        Ok(Waveform {
            sample_rate: info.sample_rate,
            frames_per_bucket,
            channels: channels as u16,
            peaks,
        })
    }

    /// Number of buckets.
    pub fn buckets(&self) -> usize {
        self.peaks.len() / usize::from(self.channels.max(1))
    }

    /// Peak of `channel` in bucket `bucket`.
    pub fn peak(&self, bucket: usize, channel: u16) -> Option<[f32; 2]> {
        self.peaks
            .get(bucket * usize::from(self.channels) + usize::from(channel))
            .copied()
    }

    /// A coarser level combining `factor` buckets into one.
    pub fn downsample(&self, factor: u32) -> Waveform {
        let factor = factor.max(1) as usize;
        let channels = usize::from(self.channels.max(1));
        let mut peaks = Vec::with_capacity(self.peaks.len() / factor + channels);
        for group in self.peaks.chunks(factor * channels) {
            for c in 0..channels {
                let mut peak = [f32::INFINITY, f32::NEG_INFINITY];
                for bucket in group.chunks_exact(channels) {
                    peak[0] = peak[0].min(bucket[c][0]);
                    peak[1] = peak[1].max(bucket[c][1]);
                }
                peaks.push(peak);
            }
        }
        Waveform {
            sample_rate: self.sample_rate,
            frames_per_bucket: self.frames_per_bucket * factor as u32,
            channels: self.channels,
            peaks,
        }
    }

    /// The buckets covering `[start, end)` of source time (for drawing a
    /// visible part of a clip).
    pub fn bucket_range(&self, start: Time, end: Time) -> std::ops::Range<usize> {
        let per = i64::from(self.frames_per_bucket.max(1));
        let first = (time_to_sample(start, self.sample_rate).max(0) / per) as usize;
        let last = (time_to_sample(end, self.sample_rate).max(0) + per - 1) / per;
        first.min(self.buckets())..(last as usize).min(self.buckets())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_time_is_exact() {
        for rate in [8_000_u32, 44_100, 48_000, 96_000, 192_000] {
            for sample in [0_i64, 1, 12_345, 10 * i64::from(rate)] {
                assert_eq!(time_to_sample(sample_to_time(sample, rate), rate), sample);
            }
            assert_eq!(
                sample_to_time(i64::from(rate), rate),
                Time::from_flicks(Time::FLICKS_PER_SECOND)
            );
        }
    }

    #[test]
    fn memory_source_reads_ranges() {
        let mut audio = MemoryAudio::new(48_000, 2, (0..20).map(|i| i as f32).collect());
        assert_eq!(audio.info().frame_count, Some(10));
        let buffer = audio.read(8, 5).unwrap();
        assert_eq!(buffer.samples, [16.0, 17.0, 18.0, 19.0]);
        assert_eq!(buffer.frames(), 2);
        assert!(audio.read(10, 5).unwrap().samples.is_empty());
    }

    #[test]
    fn waveform_peaks_and_levels() {
        // Stereo: left ramps up, right is a square wave.
        let frames = 1000;
        let samples: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let left = i as f32 / frames as f32;
                let right = if i % 2 == 0 { 0.5 } else { -0.5 };
                [left, right]
            })
            .collect();
        let mut audio = MemoryAudio::new(1000, 2, samples);
        let wave = Waveform::from_source(&mut audio, 100).unwrap();
        assert_eq!(wave.buckets(), 10);
        assert_eq!(wave.peak(3, 0), Some([0.3, 0.399]));
        assert_eq!(wave.peak(3, 1), Some([-0.5, 0.5]));
        let coarse = wave.downsample(4);
        assert_eq!((coarse.buckets(), coarse.frames_per_bucket), (3, 400));
        assert_eq!(coarse.peak(2, 0), Some([0.8, 0.999]));
        // 0.25 s .. 0.55 s at 100 frames per bucket → buckets 2..6.
        let range = wave.bucket_range(Time::from_seconds(0.25), Time::from_seconds(0.55));
        assert_eq!(range, 2..6);
    }
}
