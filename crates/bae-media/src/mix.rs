//! Stereo composition mix, shared by the preview device and file encoders.
mod sinc;
use sinc::Sinc;

use bae_anim::{Curve, CurveKind, LayerTiming};
use bae_model::{PrimPath, Stage, Time, Value};
use std::path::PathBuf;

use crate::audio::sample_to_time;
use crate::{AudioSource, FileAudio, MediaError};

/// The file referenced by an audio-bearing footage layer.
pub fn layer_file(stage: &Stage, layer: &PrimPath) -> Option<PathBuf> {
    let Value::PrimRef(asset) = stage.prim(layer)?.value("source")? else {
        return None;
    };
    let asset = stage.prim(asset)?;
    if asset.value("has_audio")?.as_bool() != Some(true) {
        return None;
    }
    Some(PathBuf::from(asset.value("file:original")?.as_str()?))
}

struct Track {
    file: PathBuf,
    source: Option<FileAudio>,
    kernel: Option<Sinc>,
    timing: LayerTiming,
    levels: [f64; 2],
    automation: Option<Curve>,
    effects: crate::audio_fx::Rack,
}

/// A snapshot of layer switches, trims, stretch and stereo dB automation.
/// Each instance owns independent streaming decoders.
#[derive(Default)]
pub struct AudioMix {
    tracks: Vec<Track>,
}

impl AudioMix {
    /// Capture the active audio layers without doing disk I/O on the caller's thread.
    pub fn from_stage(stage: &Stage, comp: &PrimPath) -> Self {
        let layers = stage.children(&comp.child("layers").expect("valid name"));
        let duration = stage
            .prim(comp)
            .and_then(|v| v.value("duration"))
            .and_then(Value::as_time)
            .unwrap_or(Time::MAX);
        let flag = |layer: &PrimPath, key, default| {
            stage
                .prim(layer)
                .and_then(|v| v.value(key))
                .and_then(Value::as_bool)
                .unwrap_or(default)
        };
        let solo = layers.iter().any(|l| flag(l, "solo", false));
        let tracks = layers
            .iter()
            .filter_map(|l| {
                if !flag(l, "audio_enabled", true) || (solo && !flag(l, "solo", false)) {
                    return None;
                }
                let file = layer_file(stage, l)?;
                let view = stage.prim(l)?;
                let levels = match view.value("audio:levels") {
                    Some(Value::Vec2(db)) => *db,
                    _ => [0.0; 2],
                };
                let automation = view
                    .attr("audio:levels")
                    .and_then(|s| Curve::new(&s.keyframes, CurveKind::PerDimension));
                Some(Track {
                    file,
                    source: None,
                    kernel: None,
                    timing: bae_anim::layer_timing(&view, duration),
                    levels,
                    automation,
                    effects: crate::audio_fx::Rack::read(stage, l),
                })
            })
            .collect();
        Self { tracks }
    }

    /// Probe enabled sources on the preview producer, before the first seek.
    pub(crate) fn prepare_sources(&mut self, rate: u32) {
        for track in &mut self.tracks {
            if let Ok(mut decoder) = FileAudio::open(&track.file) {
                let source_rate = f64::from(decoder.info().sample_rate);
                let step = source_step(track.timing, source_rate, rate);
                if (step.abs() - 1.0).abs() > 1e-9 {
                    track.kernel = Some(Sinc::new((1.0 / step.abs()).min(1.0)));
                }
                let at = track
                    .timing
                    .layer_time(track.timing.in_point.min(track.timing.out_point));
                let first = (at.as_seconds() * source_rate).max(0.0) as u64;
                let _ = decoder.read_window(first, 1);
                track.source = Some(decoder);
            }
        }
    }

    /// Whether the composition contains at least one enabled audio source.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Render a consecutive block. The start is a sample index on the composition clock.
    /// Samples retain floating-point headroom until the device or encoder boundary.
    pub fn render(
        &mut self,
        start: i64,
        rate: u32,
        out: &mut [[f32; 2]],
    ) -> Result<(), MediaError> {
        out.fill([0.0; 2]);
        for track in &mut self.tracks {
            track.add(start, rate, out)?;
        }
        Ok(())
    }
}

impl Track {
    fn add(&mut self, start: i64, rate: u32, out: &mut [[f32; 2]]) -> Result<(), MediaError> {
        let first_at = |time: Time| {
            (i128::from(time.0) * i128::from(rate) + i128::from(Time::FLICKS_PER_SECOND) - 1)
                .div_euclid(i128::from(Time::FLICKS_PER_SECOND)) as i64
        };
        let offset = |time| (first_at(time) - start).clamp(0, out.len() as i64) as usize;
        let a = offset(self.timing.in_point.min(self.timing.out_point));
        let b = offset(self.timing.out_point.max(self.timing.in_point));
        if a >= b {
            return Ok(());
        }
        if self.source.is_none() {
            self.source = Some(FileAudio::open(&self.file)?);
        }
        let source = self.source.as_mut().unwrap();
        let src_rate = f64::from(source.info().sample_rate);
        let layer_time = |i| {
            self.timing
                .layer_time(sample_to_time(start + i as i64, rate))
        };
        self.effects.begin(start + a as i64, start + b as i64, rate);
        let step = source_step(self.timing, src_rate, rate);
        let (head, step) = if self.effects.reverse {
            let mirror = first_at(self.timing.in_point) + first_at(self.timing.out_point) - 1;
            let time = sample_to_time(mirror - start - a as i64, rate);
            (self.timing.layer_time(time).as_seconds() * src_rate, -step)
        } else {
            (layer_time(a).as_seconds() * src_rate, step)
        };
        let tail = head + (b - a - 1) as f64 * step;
        let aligned = (step.abs() - 1.0).abs() < 1e-9 && (head - head.round()).abs() < 1e-5;
        let kernel = if aligned {
            None
        } else {
            let cutoff = (1.0 / step.abs()).min(1.0);
            if self.kernel.as_ref().is_none_or(|k| k.cutoff != cutoff) {
                self.kernel = Some(Sinc::new(cutoff));
            }
            self.kernel.as_ref()
        };
        let radius = kernel.map_or(0, |k| k.radius as u64);
        let base = (head.min(tail).max(0.0).floor() as u64).saturating_sub(radius);
        let last = head.max(tail).ceil() as u64 + radius + 1;
        let channels = usize::from(source.info().channels);
        let data = if head.max(tail) < 0.0 || source.info().frame_count.is_some_and(|n| base >= n) {
            &[][..]
        } else {
            source.read_window(base, last - base)?
        };
        let frames = data.len() / channels;
        let static_gain = self.levels.map(db_gain);
        let gain_at = |i| match self
            .automation
            .as_ref()
            .map(|curve| curve.sample(layer_time(i)))
        {
            Some(Value::Vec2(db)) => db.map(db_gain),
            _ => static_gain,
        };
        // Evaluate keyframes at short block boundaries, then ramp in linear gain.
        for offset in (a..b).step_by(32) {
            let end = (offset + 32).min(b);
            let from = gain_at(offset);
            let to = if self.automation.is_some() {
                gain_at(end)
            } else {
                from
            };
            for (i, dst) in out[offset..end].iter_mut().enumerate() {
                let pos = head + (offset + i - a) as f64 * step - base as f64;
                let sample = if frames == 0 || pos < -1e-5 || pos >= frames as f64 {
                    [0.0; 2]
                } else if let Some(k) = kernel {
                    k.sample(data, channels, pos.max(0.0))
                } else {
                    let frame = (pos.round() as usize).min(frames - 1) * channels;
                    [data[frame], data[frame + 1.min(channels - 1)]]
                };
                let sample = self.effects.process(sample, layer_time(offset + i), rate);
                let t = i as f32 / (end - offset) as f32;
                for c in 0..2 {
                    dst[c] += sample[c] * (from[c] + (to[c] - from[c]) * t);
                }
            }
        }
        Ok(())
    }
}

fn source_step(timing: LayerTiming, source_rate: f64, output_rate: u32) -> f64 {
    let second = timing.start_time + Time::from_seconds(1.0);
    timing.layer_time(second).as_seconds() * source_rate / f64::from(output_rate)
}

fn db_gain(db: f64) -> f32 {
    10.0_f32.powf(db as f32 / 20.0)
}
