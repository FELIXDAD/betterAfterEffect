//! Native output for preview and CTI scrubbing. The audio thread only copies
//! prepared samples. Decoding, resampling and automation run on a producer.
use crate::audio::{sample_to_time, time_to_sample};
use crate::mix::AudioMix;
use bae_model::{Time, TimeRange};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamInstant};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

#[derive(Default)]
struct Sink {
    queue: Option<Receiver<Vec<[f32; 2]>>>,
    block: std::vec::IntoIter<[f32; 2]>,
    consumed: i64,
    stamps: VecDeque<(StreamInstant, i64, i64)>,
    finished: bool,
    gain: f32,
    peaks: [f32; 2],
}
impl Sink {
    fn pull<T: Sample + FromSample<f32>>(
        &mut self,
        data: &mut [T],
        channels: usize,
        when: StreamInstant,
    ) {
        let start = self.consumed;
        let mut peaks = [0.0_f32; 2];
        for out in data.chunks_exact_mut(channels) {
            if self.block.len() == 0 {
                let Some(queue) = &self.queue else { break };
                match queue.try_recv() {
                    Ok(block) => self.block = block.into_iter(),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.finished = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
            let Some(frame) = self.block.next() else {
                break;
            };
            let frame = frame.map(|s| s * self.gain);
            for (p, s) in peaks.iter_mut().zip(frame) {
                *p = p.max(s.abs());
            }
            if channels == 1 {
                out[0] = T::from_sample(((frame[0] + frame[1]) * 0.5).clamp(-1.0, 1.0));
            } else {
                out[0] = T::from_sample(frame[0].clamp(-1.0, 1.0));
                out[1] = T::from_sample(frame[1].clamp(-1.0, 1.0));
            }
            self.consumed += 1;
        }
        self.peaks = peaks;
        if self.stamps.len() == 64 {
            self.stamps.pop_front();
        }
        self.stamps.push_back((when, start, self.consumed - start));
    }
}

/// One default output stream, reused across scrubs and previews.
pub struct AudioOutput {
    stream: cpal::Stream,
    sink: Arc<Mutex<Sink>>,
    failed: Arc<AtomicBool>,
    repeat: Arc<AtomicBool>,
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
    /// Opens the native system output using its negotiated default format.
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("No audio output device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate();
        let sink = Arc::new(Mutex::new(Sink {
            gain: 1.0,
            stamps: VecDeque::with_capacity(64),
            ..Default::default()
        }));
        let failed = Arc::new(AtomicBool::new(false));
        macro_rules! build {
            ($t:ty) => {
                stream::<$t>(&device, config.into(), sink.clone(), failed.clone())
            };
        }
        let stream = match config.sample_format() {
            SampleFormat::F32 => build!(f32),
            SampleFormat::I16 => build!(i16),
            SampleFormat::I24 => build!(cpal::I24),
            SampleFormat::I32 => build!(i32),
            SampleFormat::F64 => build!(f64),
            SampleFormat::U16 => build!(u16),
            other => return Err(format!("Unsupported audio output format: {other}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            sink,
            failed,
            repeat: Arc::default(),
            rate,
        })
    }

    /// Replace queued sound atomically. Dropping its receiver stops the old producer.
    pub fn start(&mut self, mut mix: AudioMix, from: Time, range: TimeRange, repeat: bool) {
        let (tx, rx) = mpsc::sync_channel(6);
        self.stop();
        self.sink.lock().queue = Some(rx);
        self.repeat = Arc::new(AtomicBool::new(repeat));
        let repeat = self.repeat.clone();
        let rate = self.rate;
        std::thread::spawn(move || {
            let first = time_to_sample(range.start, rate);
            let end = time_to_sample(range.end, rate);
            let mut cursor = time_to_sample(from, rate).clamp(first, end.max(first));
            if end <= first {
                return;
            }
            loop {
                if cursor >= end {
                    if !repeat.load(Ordering::Relaxed) {
                        break;
                    }
                    cursor = first;
                }
                let count = (end - cursor).min(512) as usize;
                let mut block = vec![[0.0; 2]; count];
                if let Err(error) = mix.render(cursor, rate, &mut block) {
                    tracing::warn!(%error, "Audio preview decode failed");
                    break;
                }
                cursor += count as i64;
                if tx.send(block).is_err() {
                    break;
                }
            }
        });
    }

    /// Samples heard since the start, using DAC timestamps and its stream clock.
    pub fn elapsed(&self) -> Time {
        let now = self.stream.now();
        let sink = self.sink.lock();
        for &(when, first, count) in sink.stamps.iter().rev() {
            if let Some(offset) = now.checked_duration_since(when) {
                let n = (offset.as_secs_f64() * f64::from(self.rate)) as i64;
                return sample_to_time(first + n.min(count), self.rate);
            }
        }
        Time::ZERO
    }

    /// True once the last sample has reached the output or the device is lost.
    pub fn finished(&self) -> bool {
        let (done, count) = {
            let s = self.sink.lock();
            (s.finished, s.consumed)
        };
        self.failed.load(Ordering::Relaxed)
            || (done && self.elapsed() >= sample_to_time(count, self.rate))
    }
    /// Set the loop switch while playing.
    pub fn set_loop(&self, repeat: bool) {
        self.repeat.store(repeat, Ordering::Relaxed);
    }
    /// Change the monitor gain in linear amplitude.
    pub fn set_gain(&self, gain: f32) {
        self.sink.lock().gain = gain;
    }
    /// Peak amplitudes after the monitor gain, before device clipping.
    pub fn peaks(&self) -> [f32; 2] {
        self.sink.lock().peaks
    }

    /// Stop delivering queued audio.
    pub fn stop(&mut self) {
        let mut s = self.sink.lock();
        s.queue = None;
        s.block = Vec::new().into_iter();
        s.stamps.clear();
        s.consumed = 0;
        s.finished = false;
        s.peaks = [0.0; 2];
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    sink: Arc<Mutex<Sink>>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error> {
    let channels = usize::from(config.channels);
    device.build_output_stream(
        config,
        move |data: &mut [T], info| {
            data.fill(T::EQUILIBRIUM);
            if let Some(mut sink) = sink.try_lock() {
                sink.pull(data, channels, info.timestamp().playback);
            }
        },
        move |err| {
            tracing::warn!(%err,"Audio device callback");
            if !matches!(
                err.kind(),
                cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied
            ) {
                failed.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
}
