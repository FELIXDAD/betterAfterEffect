//! Stereo composition mix, shared by the preview device and file encoders.
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
    timing: LayerTiming,
    levels: [f64; 2],
    automation: Option<Curve>,
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
                    timing: bae_anim::layer_timing(&view, duration),
                    levels,
                    automation,
                })
            })
            .collect();
        Self { tracks }
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
        let pos = |i| layer_time(i).as_seconds() * src_rate;
        let head = pos(a);
        let tail = pos(b - 1);
        if head.max(tail) < 0.0 {
            return Ok(());
        }
        let base = head.min(tail).max(0.0).floor() as u64;
        if source.info().frame_count.is_some_and(|n| base >= n) {
            return Ok(());
        }
        let last = head.max(tail).floor() as u64 + 2;
        let data = source.read(base, last - base)?;
        let channels = usize::from(data.channels);
        let frames = data.frames();
        let static_gain = self.levels.map(db_gain);
        for (i, dst) in (a..b).zip(&mut out[a..b]) {
            let p = pos(i) - base as f64;
            if p < 0.0 || p >= frames as f64 {
                continue;
            }
            let j = p.floor() as usize;
            let frac = p.fract() as f32;
            let gain = match self
                .automation
                .as_ref()
                .map(|curve| curve.sample(layer_time(i)))
            {
                Some(Value::Vec2(db)) => db.map(db_gain),
                _ => static_gain,
            };
            for c in 0..2 {
                let ch = c.min(channels - 1);
                let left = data.samples[j * channels + ch];
                let right = data.samples[(j + 1).min(frames - 1) * channels + ch];
                dst[c] += (left + (right - left) * frac) * gain[c];
            }
        }
        Ok(())
    }
}

fn db_gain(db: f64) -> f32 {
    10.0_f32.powf(db as f32 / 20.0)
}
