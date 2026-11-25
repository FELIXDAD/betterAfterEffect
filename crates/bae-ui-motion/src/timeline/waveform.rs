//! Peak envelopes for visible sound layers. Decode one file at a time off the UI thread.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use super::*;
use bae_media::{FileAudio, Waveform};

#[derive(Default)]
pub(super) struct Waveforms {
    // A failed read is cached too, so a missing source isn't reopened every repaint.
    files: HashMap<PathBuf, Vec<Waveform>>,
    loading: Option<(PathBuf, Receiver<Vec<Waveform>>)>,
}

impl Waveforms {
    pub(super) fn draw(
        &mut self,
        ui: &Ui,
        stage: &Stage,
        layer: &PrimPath,
        axis: &TimeAxis,
        rect: Rect,
    ) {
        let Some(file) = crate::audio::layer_file(stage, layer) else {
            return;
        };
        if let Some((_, rx)) = &self.loading {
            let levels = match rx.try_recv() {
                Ok(levels) => Some(levels),
                Err(mpsc::TryRecvError::Disconnected) => Some(Vec::new()),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(levels) = levels {
                let (file, _) = self.loading.take().unwrap();
                self.files.insert(file, levels);
            }
        }
        let Some(levels) = self.files.get(&file) else {
            if self.loading.is_none() {
                let (tx, rx) = mpsc::channel();
                self.loading = Some((file.clone(), rx));
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let mut levels = Vec::new();
                    if let Ok(mut audio) = FileAudio::open(&file)
                        && let Ok(wave) = Waveform::from_source(&mut audio, 128)
                    {
                        levels.push(wave);
                        while levels.last().is_some_and(|w| w.buckets() > 2) {
                            levels.push(levels.last().unwrap().downsample(2));
                        }
                    }
                    let _ = tx.send(levels);
                    ctx.request_repaint();
                });
            }
            return;
        };
        let Some(fine) = levels.first() else { return };
        let timing = query::layer_timing(stage, layer);
        let start = rect
            .left()
            .max(axis.x(timing.in_point.min(timing.out_point)))
            .ceil() as i32;
        let end = rect
            .right()
            .min(axis.x(timing.in_point.max(timing.out_point)))
            .ceil() as i32;
        let at = |x| timing.layer_time(axis.t(x));
        let seconds_per_point = (at(start as f32 + 1.0) - at(start as f32))
            .as_seconds()
            .abs();
        let samples = seconds_per_point * f64::from(fine.sample_rate);
        let wave = levels
            .iter()
            .rev()
            .find(|w| f64::from(w.frames_per_bucket) <= samples)
            .unwrap_or(fine);
        let painter = ui.painter().with_clip_rect(rect);
        let channels = usize::from(wave.channels);
        let height = rect.height() / channels as f32;
        for x in start..end {
            let a = at(x as f32);
            let b = at(x as f32 + 1.0);
            let range = wave.bucket_range(a.min(b), a.max(b));
            for ch in 0..channels {
                let (mut lo, mut hi) = (0.0_f32, 0.0_f32);
                for i in range.clone() {
                    let peak = wave.peaks[i * channels + ch];
                    lo = lo.min(peak[0]);
                    hi = hi.max(peak[1]);
                }
                let mid = rect.top() + height * (ch as f32 + 0.5);
                painter.vline(
                    x as f32,
                    mid - hi * height * 0.47..=mid - lo * height * 0.47,
                    Stroke::new(1.0, color::TEXT),
                );
            }
        }
    }
}
