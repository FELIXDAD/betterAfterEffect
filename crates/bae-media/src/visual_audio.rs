//! Sample the chosen layer for the two audio-driven Generate effects.
use crate::audio::sample_to_time;
use crate::{AudioSource, FileAudio, MediaError};
use bae_anim::LayerTiming;
use bae_model::Time;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use symphonia::core::dsp::{complex::Complex, fft::Fft};

pub const RATE: u32 = 44_100;
#[derive(Default)]
pub struct AudioWindows(HashMap<PathBuf, FileAudio>);
impl std::fmt::Debug for AudioWindows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioWindows")
            .field("sources", &self.0.len())
            .finish()
    }
}
impl AudioWindows {
    /// A window on the composition clock. Muting a layer's speakers leaves its source available.
    pub fn read(
        &mut self,
        path: &Path,
        timing: LayerTiming,
        start: Time,
        count: usize,
        channel: i64,
    ) -> Result<Vec<f32>, MediaError> {
        let source = match self.0.entry(path.to_owned()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => e.insert(FileAudio::open(path)?),
        };
        Self::read_source(source, timing, start, count, channel)
    }

    /// Sample an already prepared source, including its decoder's PCM read-ahead.
    pub fn read_source(
        source: &mut FileAudio,
        timing: LayerTiming,
        start: Time,
        count: usize,
        channel: i64,
    ) -> Result<Vec<f32>, MediaError> {
        let sr = f64::from(source.info().sample_rate);
        let time = |i: usize| start + sample_to_time(i as i64, RATE);
        let pos = |i| timing.layer_time(time(i)).as_seconds() * sr;
        let mut out = vec![0.0; count];
        for (chunk, window) in out.chunks_mut(1024).enumerate() {
            let i = chunk * 1024;
            let Some(a) = (i..i + window.len()).find(|&j| timing.is_active(time(j))) else {
                continue;
            };
            let b = (a..i + window.len())
                .take_while(|&j| timing.is_active(time(j)))
                .last()
                .unwrap_or(a);
            let x = pos(a);
            let y = pos(b);
            if x.max(y) < 0.0 {
                continue;
            }
            let first = x.min(y).max(0.0).floor() as u64;
            if source.info().frame_count.is_some_and(|n| first >= n) {
                continue;
            }
            let pcm = source.read(first, x.max(y).floor() as u64 - first + 2)?;
            let ch = usize::from(pcm.channels);
            for j in a..=b {
                let p = pos(j) - first as f64;
                if !(0.0..pcm.frames() as f64).contains(&p) {
                    continue;
                }
                let ix = p.floor() as usize;
                let read = |row: usize| {
                    let left = pcm.samples[row * ch];
                    let right = pcm.samples[row * ch + 1.min(ch - 1)];
                    match channel {
                        2 => left,
                        3 => right,
                        _ => (left + right) * 0.5,
                    }
                };
                let l = read(ix);
                window[j - i] = l + (read((ix + 1).min(pcm.frames() - 1)) - l) * p.fract() as f32;
            }
        }
        Ok(out)
    }
}

pub fn waveform(pcm: &[f32], points: usize) -> Vec<f64> {
    (0..points)
        .map(|i| {
            if pcm.is_empty() {
                return 0.0;
            }
            let p = i as f64 * (pcm.len() - 1) as f64 / points.saturating_sub(1).max(1) as f64;
            let k = p.floor() as usize;
            f64::from(pcm[k])
                + (f64::from(pcm[(k + 1).min(pcm.len() - 1)]) - f64::from(pcm[k])) * p.fract()
        })
        .collect()
}

/// FFT plans are private to the evaluator, so an export cannot hold up a preview.
#[derive(Default)]
pub struct SpectrumPlans(HashMap<usize, Fft>);

impl SpectrumPlans {
    /// Prepare both the permutation plans and Symphonia's lazy twiddle tables.
    pub fn prewarm() -> Self {
        let mut plans = Self::default();
        for size in (6..=14).map(|power| 1 << power) {
            let mut plan = Fft::new(size);
            plan.fft_inplace(&mut vec![Complex::new(0.0, 0.0); size]);
            plans.0.insert(size, plan);
        }
        plans
    }

    pub fn spectrum(
        &mut self,
        pcm: &[f32],
        bands: usize,
        start: f64,
        end: f64,
        average: bool,
    ) -> Vec<f64> {
        let mut result = vec![0.0_f64; bands];
        if pcm.is_empty() || bands == 0 {
            return result;
        }
        let fft_size = pcm
            .len()
            .min(8192)
            .max(bands * 2)
            .next_power_of_two()
            .clamp(64, 16384);
        let fft = self.0.entry(fft_size).or_insert_with(|| Fft::new(fft_size));
        let mut bins = vec![Complex::new(0.0, 0.0); fft_size];
        let width = pcm.len().min(fft_size);
        let weights: Vec<f32> = (0..width)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / width.max(2) as f32).cos())
            .collect();
        let norm = 2.0 / weights.iter().sum::<f32>().max(1.0);
        let mut blocks = 0;
        for offset in (0..pcm.len()).step_by((width / 2).max(1)) {
            bins.fill(Complex::new(0.0, 0.0));
            for ((z, &s), &w) in bins.iter_mut().zip(&pcm[offset..]).zip(&weights) {
                z.re = s * w;
            }
            fft.fft_inplace(&mut bins);
            for (i, output) in result.iter_mut().enumerate() {
                let k = |f: f64| {
                    ((f / f64::from(RATE) * fft_size as f64).round() as usize).min(fft_size / 2)
                };
                let x = k(start + (end - start) * i as f64 / bands as f64);
                let y = k(start + (end - start) * (i + 1) as f64 / bands as f64);
                let peak = bins[x.min(y)..=x.max(y)]
                    .iter()
                    .map(|v| v.norm_sqr())
                    .fold(0.0_f32, f32::max)
                    .sqrt()
                    * norm;
                if average {
                    *output += f64::from(peak)
                } else {
                    *output = output.max(f64::from(peak))
                }
            }
            blocks += 1;
            if offset + width >= pcm.len() {
                break;
            }
        }
        if average {
            for v in &mut result {
                *v /= f64::from(blocks);
            }
        }
        result
    }
}

impl std::fmt::Debug for SpectrumPlans {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpectrumPlans")
            .field("plans", &self.0.len())
            .finish()
    }
}
