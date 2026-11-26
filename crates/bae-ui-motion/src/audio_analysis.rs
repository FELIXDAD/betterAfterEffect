//! Analyze the selected work area on a worker, then commit all keyframes together.
use bae_media::{audio::time_to_sample, mix::AudioMix};
use bae_motion::query::CompInfo;
use bae_ui::{PanelContext, tr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

#[derive(Debug, Clone)]
pub struct Analysis {
    comp: bae_model::PrimPath,
    first: i64,
    result: Arc<Mutex<mpsc::Receiver<AnalysisResult>>>,
    cancelled: Arc<AtomicBool>,
}

pub(crate) fn convert(cx: &PanelContext<'_>, info: &CompInfo) {
    let mix = AudioMix::from_stage(cx.stage, &info.path);
    if mix.is_empty() {
        super::dialogs::alert(
            cx,
            tr("Convert Audio to Keyframes"),
            tr("The composition has no enabled audio layers.").to_string(),
        );
        return;
    }
    let dialogs = super::dialogs::shared(cx.resources);
    let mut dialogs = dialogs.lock();
    if dialogs.open.is_some() {
        return;
    }
    let first = info.frame_rate.time_to_frame(info.work_area.start);
    let (tx, rx) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    dialogs.open = Some(super::dialogs::Dialog::AudioAnalysis(Analysis {
        comp: info.path.clone(),
        first,
        result: Arc::new(Mutex::new(rx)),
        cancelled: cancelled.clone(),
    }));
    let info = info.clone();
    std::thread::spawn(move || {
        let _ = tx.send(analyze(mix, &info, first, &cancelled));
    });
}

type Amplitudes = Vec<[f64; 2]>;
type AnalysisResult = Result<Amplitudes, String>;
fn analyze(
    mut mix: AudioMix,
    info: &CompInfo,
    first: i64,
    cancelled: &AtomicBool,
) -> Result<Amplitudes, String> {
    const RATE: u32 = 48_000;
    let mut amplitudes = Vec::new();
    let mut pcm = Vec::new();
    for frame in first..info.frame_count() {
        let start = info.frame_rate.frame_to_time(frame);
        if start >= info.work_area.end || cancelled.load(Ordering::Relaxed) {
            break;
        }
        let begin = time_to_sample(start.max(info.work_area.start), RATE);
        let end = time_to_sample(
            (start + info.frame_duration()).min(info.work_area.end),
            RATE,
        );
        pcm.resize((end - begin).max(0) as usize, [0.0; 2]);
        mix.render(begin, RATE, &mut pcm)
            .map_err(|e| e.to_string())?;
        let mut energy = [0.0_f64; 2];
        for pair in &pcm {
            for (sum, &sample) in energy.iter_mut().zip(pair) {
                *sum += f64::from(sample).powi(2);
            }
        }
        amplitudes.push(energy.map(|sum| 100.0 * (sum / pcm.len().max(1) as f64).sqrt()));
    }
    Ok(amplitudes)
}

impl Analysis {
    pub fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let title = tr("Convert Audio to Keyframes");
        let result = self.result.lock().unwrap().try_recv();
        match result {
            Ok(Ok(amplitudes)) => {
                if !amplitudes.is_empty() && cx.stage.prim(&self.comp).is_some() {
                    cx.invoke_with("audio.create_amplitude_layer",serde_json::json!({"comp":self.comp,"first":self.first,"amplitudes":amplitudes,
                        "layer_name":tr("Audio Amplitude"),"channel_names":[tr("Left Channel"),tr("Right Channel"),tr("Both Channels")]}));
                }
                false
            }
            Ok(Err(error)) => {
                super::dialogs::alert(cx, title, error);
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                super::dialogs::alert(cx, title, tr("Audio analysis stopped.").to_string());
                false
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(40));
                let id = egui::Id::new("bae-motion.audio-analysis");
                let cancel = bae_ui::dialog::window(ctx, id, title, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Analyzing audio…"));
                    });
                    ui.button(tr("Cancel")).clicked()
                })
                .unwrap_or_else(|| bae_ui::dialog::closed_by_title_bar(ctx, id));
                if cancel {
                    self.cancelled.store(true, Ordering::Relaxed);
                }
                !cancel
            }
        }
    }
}
