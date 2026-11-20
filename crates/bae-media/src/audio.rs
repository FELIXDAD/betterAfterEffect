//! Audio interface (reserved: symphonia decoding, cpal playback and waveform
//! generation land in a later PR). Stream metadata is [`AudioInfo`].

use crate::meta::AudioInfo;
use crate::source::MediaError;

/// Decoded audio: interleaved 32-bit float samples.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioBuffer {
    /// Samples per second.
    pub sample_rate: u32,
    /// Channels per sample frame.
    pub channels: u16,
    /// `frames * channels` interleaved samples.
    pub samples: Vec<f32>,
}

/// An audio stream with random access by sample frame.
pub trait AudioSource: Send {
    /// Stream description.
    fn info(&self) -> &AudioInfo;

    /// Decodes `count` sample frames starting at sample frame `start`.
    fn read(&mut self, start: u64, count: u64) -> Result<AudioBuffer, MediaError>;
}

/// Min/max peaks for drawing a waveform: one pair per bucket per channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Waveform {
    /// Sample frames summarized by one bucket.
    pub frames_per_bucket: u32,
    /// Channels.
    pub channels: u16,
    /// `buckets * channels` (min, max) pairs, interleaved by channel.
    pub peaks: Vec<(f32, f32)>,
}
