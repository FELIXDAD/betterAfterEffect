//! Peak envelopes for visible sound layers. A single worker checks and decodes their sources.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use super::*;
use bae_media::{FileAudio, Waveform, source_id_for};

const CHECK_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_secs(5);

struct Cached {
    peaks: Option<(u64, Vec<Waveform>)>,
    // None while a check is queued or running.
    next_check: Option<Instant>,
}

enum PeakUpdate {
    Unchanged,
    Decoded(u64, Vec<Waveform>),
    Unavailable,
}

fn check_file(path: &Path, previous: Option<u64>) -> PeakUpdate {
    let id = source_id_for(path);
    if previous == Some(id) {
        return PeakUpdate::Unchanged;
    }
    let Ok(mut audio) = FileAudio::open(path) else {
        return PeakUpdate::Unavailable;
    };
    let Ok(wave) = Waveform::from_source(&mut audio, 128) else {
        return PeakUpdate::Unavailable;
    };
    let mut levels = vec![wave];
    while levels.last().unwrap().buckets() > 2 {
        levels.push(levels.last().unwrap().downsample(2));
    }
    PeakUpdate::Decoded(id, levels)
}

struct Worker {
    requests: Sender<(PathBuf, Option<u64>)>,
    results: Receiver<(PathBuf, PeakUpdate)>,
}

impl Worker {
    fn start(ctx: egui::Context) -> Self {
        let (requests, jobs) = mpsc::channel::<(PathBuf, Option<u64>)>();
        let (complete, results) = mpsc::channel();
        std::thread::spawn(move || {
            for (path, previous) in jobs {
                let update = check_file(&path, previous);
                if complete.send((path, update)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self { requests, results }
    }
}

impl Cached {
    fn finish(&mut self, update: PeakUpdate, now: Instant) {
        match update {
            PeakUpdate::Unchanged => {}
            PeakUpdate::Decoded(id, levels) => self.peaks = Some((id, levels)),
            PeakUpdate::Unavailable => self.peaks = None,
        }
        let delay = if self.peaks.is_some() {
            CHECK_INTERVAL
        } else {
            RETRY_INTERVAL
        };
        self.next_check = Some(now + delay);
    }
}

#[derive(Default)]
pub(super) struct Waveforms {
    files: HashMap<PathBuf, Cached>,
    worker: Option<Worker>,
}

impl Waveforms {
    fn levels(&mut self, ctx: &egui::Context, file: PathBuf, now: Instant) -> &[Waveform] {
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::start(ctx.clone()));
        for (path, result) in worker.results.try_iter() {
            if let Some(entry) = self.files.get_mut(&path) {
                entry.finish(result, now);
            }
        }
        let entry = self.files.entry(file.clone()).or_insert(Cached {
            peaks: None,
            next_check: Some(now),
        });
        if let Some(due) = entry.next_check {
            if now >= due {
                let previous = entry.peaks.as_ref().map(|(id, _)| *id);
                if worker.requests.send((file, previous)).is_ok() {
                    entry.next_check = None;
                }
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
        entry.peaks.as_ref().map_or(&[], |(_, levels)| levels)
    }

    pub(super) fn draw(
        &mut self,
        ui: &Ui,
        stage: &Stage,
        layer: &PrimPath,
        axis: &TimeAxis,
        rect: Rect,
    ) {
        let Some(file) = bae_media::audio::layer_file(stage, layer) else {
            return;
        };
        let levels = self.levels(ui.ctx(), file.to_owned(), Instant::now());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_and_replaced_wav_refresh_after_the_check_interval() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.wav");
        let write_wav = |sample: i16, seconds: u64| {
            let mut wav = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x02\0\0\0".to_vec();
            wav.extend(sample.to_le_bytes());
            std::fs::write(&file, wav).unwrap();
            let modified = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
            std::fs::File::options()
                .write(true)
                .open(&file)
                .unwrap()
                .set_modified(modified)
                .unwrap();
        };
        let ctx = egui::Context::default();
        let mut waveforms = Waveforms::default();
        let start = Instant::now();
        let complete = |cache: &mut Waveforms, now| {
            let (path, result) = cache
                .worker
                .as_ref()
                .unwrap()
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            cache.files.get_mut(&path).unwrap().finish(result, now);
        };
        assert!(waveforms.levels(&ctx, file.clone(), start).is_empty());
        complete(&mut waveforms, start);
        write_wav(8192, 100);
        assert!(waveforms.levels(&ctx, file.clone(), start).is_empty());
        assert!(waveforms.files[&file].next_check.is_some());
        let restored = start + RETRY_INTERVAL;
        waveforms.levels(&ctx, file.clone(), restored);
        complete(&mut waveforms, restored);
        assert_eq!(
            waveforms.levels(&ctx, file.clone(), restored)[0].peaks[0][1],
            0.25
        );
        write_wav(-16384, 200);
        let replaced = restored + CHECK_INTERVAL;
        waveforms.levels(&ctx, file.clone(), replaced);
        complete(&mut waveforms, replaced);
        assert_eq!(waveforms.levels(&ctx, file, replaced)[0].peaks[0][0], -0.5);
    }
}
