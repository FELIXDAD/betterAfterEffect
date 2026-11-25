//! Native preview and CTI scrubbing. One producer fills an SPSC ring. The device
//! callback copies those samples and publishes the DAC clock without taking locks.
mod clock;
mod producer;

use crate::audio::sample_to_time;
use crate::mix::AudioMix;
use bae_model::{Time, TimeRange};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rtrb::{Consumer, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};

const BLOCK_FRAMES: usize = 512;
struct Block {
    epoch: u64,
    len: usize,
    samples: [[f32; 2]; BLOCK_FRAMES],
}
impl Default for Block {
    fn default() -> Self {
        Self {
            epoch: 0,
            len: 0,
            samples: [[0.0; 2]; BLOCK_FRAMES],
        }
    }
}

#[derive(Default)]
struct Monitor {
    epoch: AtomicU64,
    repeat: AtomicBool,
    failed: AtomicBool,
    ended_epoch: AtomicU64,
    ended_at: AtomicU64,
    clock: clock::Clock,
    gain: AtomicU32,
    peaks: AtomicU64,
}

struct Sink {
    ring: Consumer<Block>,
    monitor: Arc<Monitor>,
    epoch: u64,
    offset: usize,
    consumed: u64,
}
impl Sink {
    fn pull<T: Sample + FromSample<f32>>(&mut self, data: &mut [T], channels: usize, nanos: u64) {
        data.fill(T::EQUILIBRIUM);
        let epoch = self.monitor.epoch.load(Ordering::Acquire);
        if epoch != self.epoch {
            self.epoch = epoch;
            self.offset = 0;
            self.consumed = 0;
        }
        let gain = f32::from_bits(self.monitor.gain.load(Ordering::Relaxed));
        let mut peaks = [0.0_f32; 2];
        let first = self.consumed;
        let mut output = data.chunks_exact_mut(channels);
        while output.len() > 0 {
            let Ok(block) = self.ring.peek() else { break };
            if block.epoch > epoch {
                break;
            }
            if block.epoch != epoch {
                self.ring.pop().expect("peeked block");
                self.offset = 0;
                continue;
            }
            if block.len == 0 {
                self.monitor
                    .ended_at
                    .store(self.consumed, Ordering::Relaxed);
                self.monitor.ended_epoch.store(epoch, Ordering::Release);
                self.ring.pop().expect("end marker");
                break;
            }
            let count = (block.len - self.offset).min(output.len());
            for (frame, out) in block.samples[self.offset..self.offset + count]
                .iter()
                .zip(output.by_ref())
            {
                let frame = frame.map(|v| v * gain);
                for (peak, sample) in peaks.iter_mut().zip(frame) {
                    *peak = peak.max(sample.abs());
                }
                if channels == 1 {
                    out[0] = T::from_sample(((frame[0] + frame[1]) * 0.5).clamp(-1.0, 1.0));
                } else {
                    out[0] = T::from_sample(frame[0].clamp(-1.0, 1.0));
                    out[1] = T::from_sample(frame[1].clamp(-1.0, 1.0));
                }
            }
            self.consumed += count as u64;
            self.offset += count;
            if self.offset == block.len {
                self.ring.pop().expect("consumed block");
                self.offset = 0;
            }
        }
        let bits = peaks.map(f32::to_bits);
        self.monitor.peaks.store(
            u64::from(bits[0]) | (u64::from(bits[1]) << 32),
            Ordering::Relaxed,
        );
        if self.consumed != first {
            self.monitor
                .clock
                .publish(epoch, nanos, first, self.consumed - first);
        }
    }
}

/// Default device with a reusable decoding/mixing thread.
pub struct AudioOutput {
    stream: cpal::Stream,
    commands: Sender<producer::Command>,
    monitor: Arc<Monitor>,
    rate: u32,
}
impl std::fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutput")
            .field("rate", &self.rate)
            .finish_non_exhaustive()
    }
}

impl AudioOutput {
    /// Negotiate the device's native sample format and start the producer.
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("No audio output device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate();
        let (writer, reader) = RingBuffer::new(8);
        let monitor = Arc::new(Monitor {
            gain: AtomicU32::new(1.0_f32.to_bits()),
            ..Monitor::default()
        });
        let sink = Sink {
            ring: reader,
            monitor: monitor.clone(),
            epoch: 0,
            consumed: 0,
            offset: 0,
        };
        macro_rules! build {
            ($t:ty) => {
                stream::<$t>(&device, config.into(), sink)
            };
        }
        let stream = match config.sample_format() {
            SampleFormat::F32 => build!(f32),
            SampleFormat::I16 => build!(i16),
            SampleFormat::I24 => build!(cpal::I24),
            SampleFormat::U16 => build!(u16),
            SampleFormat::I32 => build!(i32),
            SampleFormat::F64 => build!(f64),
            format => return Err(format!("Unsupported output format: {format}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        let (commands, messages) = mpsc::channel();
        let worker_monitor = monitor.clone();
        std::thread::Builder::new()
            .name("bae-audio-mix".into())
            .spawn(move || producer::run(&messages, writer, &worker_monitor, rate))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            commands,
            monitor,
            rate,
        })
    }

    /// Install a snapshot on the producer. Media probing happens there.
    pub fn prepare(&self, mix: AudioMix) {
        self.monitor.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.commands.send(producer::Command::Prepare(mix));
    }
    /// Start or seek within the prepared mix; retain its decoders.
    pub fn seek(&self, from: Time, range: TimeRange, repeat: bool) {
        self.set_loop(repeat);
        let epoch = self.monitor.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.commands.send(producer::Command::Seek {
            epoch,
            at: from,
            range,
        });
    }
    fn heard(&self) -> u64 {
        let now = self.stream.now().as_nanos() as u64;
        self.monitor
            .clock
            .heard(self.monitor.epoch.load(Ordering::Acquire), now, self.rate)
    }
    /// Samples heard since the latest seek, measured against DAC presentation time.
    pub fn elapsed(&self) -> Time {
        sample_to_time(self.heard() as i64, self.rate)
    }
    /// The operating system invalidated the audio stream.
    pub fn device_lost(&self) -> bool {
        self.monitor.failed.load(Ordering::Relaxed)
    }
    /// The last queued sample has reached the speakers, or the device was lost.
    pub fn finished(&self) -> bool {
        let epoch = self.monitor.epoch.load(Ordering::Acquire);
        self.device_lost()
            || (epoch != 0
                && self.monitor.ended_epoch.load(Ordering::Acquire) == epoch
                && self.heard() >= self.monitor.ended_at.load(Ordering::Relaxed))
    }
    /// Change the active work area's repeat mode.
    pub fn set_loop(&self, repeat: bool) {
        self.monitor.repeat.store(repeat, Ordering::Relaxed);
    }
    /// Change the monitor gain in linear amplitude.
    pub fn set_gain(&self, gain: f32) {
        self.monitor.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    /// Stereo peaks after monitor gain and before device clipping.
    pub fn peaks(&self) -> [f32; 2] {
        let bits = self.monitor.peaks.load(Ordering::Relaxed);
        [
            f32::from_bits(bits as u32),
            f32::from_bits((bits >> 32) as u32),
        ]
    }

    /// Silence queued audio while retaining the device and the prepared sources.
    pub fn stop(&self) {
        self.monitor.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.commands.send(producer::Command::Stop);
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut sink: Sink,
) -> Result<cpal::Stream, cpal::Error> {
    let channels = usize::from(config.channels);
    let monitor = sink.monitor.clone();
    device.build_output_stream(
        config,
        move |data: &mut [T], info| {
            sink.pull(data, channels, info.timestamp().playback.as_nanos() as u64)
        },
        move |error| {
            tracing::warn!(%error, "Audio device callback");
            if !matches!(
                error.kind(),
                cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied
            ) {
                monitor.failed.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_discards_queued_audio_and_underruns_hold_the_dac_clock() {
        let (mut writer, ring) = RingBuffer::new(4);
        let monitor = Arc::new(Monitor {
            gain: AtomicU32::new(1.0_f32.to_bits()),
            ..Monitor::default()
        });
        let mut sink = Sink {
            ring,
            monitor: monitor.clone(),
            epoch: 0,
            offset: 0,
            consumed: 0,
        };
        for (epoch, value) in [(1, 0.25), (2, 0.75)] {
            let mut chunk = writer.write_chunk(1).unwrap();
            let block = &mut chunk.as_mut_slices().0[0];
            block.epoch = epoch;
            block.len = 3;
            block.samples[..3].fill([value; 2]);
            chunk.commit_all();
        }
        let mut mono = [0.0; 2];
        monitor.epoch.store(1, Ordering::Release);
        sink.pull(&mut mono, 1, 1_000_000);
        assert_eq!(mono, [0.25; 2]);
        monitor.epoch.store(2, Ordering::Release);
        sink.pull(&mut mono, 1, 3_000_000);
        assert_eq!(mono, [0.75; 2]);
        sink.pull(&mut mono, 1, 5_000_000);
        assert_eq!(mono, [0.75, 0.0]);
        assert_eq!(monitor.clock.heard(2, 2_500_000, 1000), 0);
        assert_eq!(monitor.clock.heard(2, 4_000_000, 1000), 1);
        assert_eq!(monitor.clock.heard(2, 8_000_000, 1000), 3);

        // The master fader changes the actual samples. Meters see each peak before clipping.
        monitor.gain.store(2.0_f32.to_bits(), Ordering::Relaxed);
        let mut chunk = writer.write_chunk(1).unwrap();
        let block = &mut chunk.as_mut_slices().0[0];
        block.epoch = 2;
        block.len = 1;
        block.samples[0] = [-0.75, 0.25];
        chunk.commit_all();
        let mut stereo = [0.0; 2];
        sink.pull(&mut stereo, 2, 9_000_000);
        assert_eq!(stereo, [-1.0, 0.5]);
        let bits = monitor.peaks.load(Ordering::Relaxed);
        assert_eq!(f32::from_bits(bits as u32), 1.5);
        assert_eq!(f32::from_bits((bits >> 32) as u32), 0.5);
    }
}
