//! Audio: decoded sample access ([`AudioSource`]), sample ↔ [`Time`]
//! conversion and waveform data for the timeline ([`Waveform`]).
//!
//! File decoding (symphonia) lands in the next PR behind this interface;
//! [`MemoryAudio`] serves generated audio and tests. Playback (cpal) comes
//! later.
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
