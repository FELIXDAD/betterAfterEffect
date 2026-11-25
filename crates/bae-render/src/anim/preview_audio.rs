//! Prepare the active composition's audio sources off the viewer thread.
use super::AssetFiles;
use bae_anim::{EvaluatedComp, LayerSource};
use bae_media::visual_audio::SpectrumPlans;
use bae_media::{AudioSource, FileAudio};
use bae_model::{AssetFile, PrimPath, Stage, Time, Value};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

type Prepared = Result<Option<FileAudio>, String>;
type Job = (AssetFile, Time, Sender<Prepared>);

enum Source {
    Opening(Receiver<Prepared>),
    Ready(Prepared, Instant),
}

impl Source {
    fn needs_retry(&self) -> bool {
        matches!(self, Self::Ready(Ok(None) | Err(_), at) if at.elapsed() >= Duration::from_secs(5))
    }
}

pub(super) struct PreviewAudio {
    jobs: Sender<Job>,
    plans: Receiver<SpectrumPlans>,
    sources: HashMap<PrimPath, (AssetFile, Source)>,
    pub(super) composition: Option<PrimPath>,
    pub(super) pending: bool,
}

impl std::fmt::Debug for PreviewAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewAudio")
            .field("sources", &self.sources.len())
            .finish()
    }
}

impl PreviewAudio {
    pub(super) fn new(files: AssetFiles, repaint: impl Fn() + Send + 'static) -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        let (plans_tx, plans) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("bae-audio-visual-prep".into())
            .spawn(move || {
                if plans_tx.send(SpectrumPlans::for_preview()).is_err() {
                    return;
                }
                for (asset, at, result) in receiver {
                    let prepared = files.resolve_file(&asset).and_then(|file| {
                        file.map(|path| {
                            let mut source = FileAudio::open(&path).map_err(|e| e.to_string())?;
                            // Cover the default centered analysis window around the playhead.
                            let rate = source.info().sample_rate;
                            let first =
                                ((at.as_seconds() - 0.125).max(0.0) * f64::from(rate)) as u64;
                            source
                                .read(first, u64::from(rate / 4).max(1))
                                .map_err(|e| e.to_string())?;
                            Ok(source)
                        })
                        .transpose()
                    });
                    if result.send(prepared).is_ok() {
                        repaint();
                    }
                }
            })
        {
            tracing::warn!(%error, "Audio preparation worker unavailable");
        }
        Self {
            jobs,
            plans,
            sources: HashMap::new(),
            composition: None,
            pending: false,
        }
    }

    pub(super) fn begin(&mut self, stage: &Stage, comp: &EvaluatedComp) {
        self.pending = false;
        if self.composition.as_ref() != Some(&comp.path) {
            let mut used = HashSet::new();
            self.collect(stage, comp, &mut used);
            self.sources.retain(|asset, _| used.contains(asset));
            self.composition = Some(comp.path.clone());
        }
    }

    fn collect(&mut self, stage: &Stage, comp: &EvaluatedComp, used: &mut HashSet<PrimPath>) {
        for layer in &comp.layers {
            match &layer.source {
                LayerSource::Asset { asset, time, .. } if used.insert(asset.clone()) => {
                    if let Some(view) = stage.prim(asset)
                        && view.value("has_audio").and_then(Value::as_bool) == Some(true)
                        && let Some(file) = AssetFile::from_prim(&view)
                    {
                        self.prepare(asset, file, *time);
                    }
                }
                LayerSource::Comp {
                    nested: Some(comp), ..
                } => self.collect(stage, comp, used),
                _ => {}
            }
        }
    }

    fn prepare(&mut self, asset: &PrimPath, file: AssetFile, at: Time) {
        if self
            .sources
            .get(asset)
            .is_some_and(|(known, source)| known == &file && !source.needs_retry())
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let source = if self.jobs.send((file.clone(), at, tx)).is_ok() {
            Source::Opening(rx)
        } else {
            Source::Ready(
                Err("Audio preparation worker stopped".into()),
                Instant::now(),
            )
        };
        self.sources.insert(asset.clone(), (file, source));
    }

    pub(super) fn source(
        &mut self,
        view: &bae_model::PrimView<'_>,
        at: Time,
        plans: &mut SpectrumPlans,
    ) -> Result<Option<&mut FileAudio>, String> {
        let asset = view.path();
        if self
            .sources
            .get(asset)
            .is_none_or(|(_, source)| source.needs_retry())
            && let Some(file) = AssetFile::from_prim(view)
        {
            self.prepare(asset, file, at);
        }
        let Some((_, source)) = self.sources.get_mut(asset) else {
            return Ok(None);
        };
        if let Source::Opening(rx) = source {
            *source = match rx.try_recv() {
                Ok(ready) => Source::Ready(ready, Instant::now()),
                Err(TryRecvError::Empty) => {
                    self.pending = true;
                    return Ok(None);
                }
                Err(TryRecvError::Disconnected) => Source::Ready(
                    Err("Audio preparation worker stopped".into()),
                    Instant::now(),
                ),
            };
        }
        if let Ok(ready) = self.plans.try_recv() {
            *plans = ready;
        }
        match source {
            Source::Ready(Ok(file), _) => Ok(file.as_mut()),
            Source::Ready(Err(e), _) => Err(e.clone()),
            Source::Opening(_) => unreachable!(),
        }
    }
}
