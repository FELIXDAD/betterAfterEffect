//! Bake layer-space frames on a low-priority worker. Only finished sequences
//! reach the document. Dropping a job cancels it and removes partial output.
use crate::gpu_preview::{PreviewEffects, PreviewSources};
use bae_anim::Evaluator;
use bae_gpu::{GpuContext, WarmUp};
use bae_model::{PrimPath, Time, Value};
use bae_motion::{import::ImportFile, query};
use bae_render::{
    AnimEvaluator, OutputAlpha, RenderPurpose, RenderRequest, Renderer, RendererConfig,
};
use bae_ui::{PanelContext, tr};
use std::{
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
use tempfile::TempDir;

#[derive(Default)]
struct BakeRenderer(Mutex<Option<Arc<Mutex<Renderer>>>>);
impl WarmUp for BakeRenderer {
    fn name(&self) -> &str {
        "vision sequence renderer"
    }
    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String> {
        let renderer = Renderer::with_config(
            gpu.clone(),
            RendererConfig {
                frame_cache_bytes: 0,
                pool_idle_bytes: 64 << 20,
            },
        );
        *self.0.lock().map_err(|e| e.to_string())? = Some(Arc::new(Mutex::new(renderer)));
        Ok(())
    }
}
pub(super) fn register(app: &mut bae_app::AppBuilder) {
    let renderer = Arc::new(BakeRenderer::default());
    app.insert_resource(renderer.clone());
    let _ = app.register_arc::<dyn WarmUp>("vision.sequence", renderer);
    bae_ui::i18n::register(&[
        ("Sequence renderer stopped.", "序列渲染已停止。"),
        (
            "The sequence renderer is warming up.",
            "序列渲染器正在预热。",
        ),
        (
            "The project changed during generation.",
            "生成期间工程已更改。",
        ),
    ]);
}
#[derive(Clone)]
pub(super) enum Mode {
    Freeze(PrimPath),
    Fill(PrimPath, bae_vision::FillSettings),
}

struct Sequence {
    dir: TempDir,
    file: ImportFile,
    origin: [f64; 2],
}
enum Update {
    Frame(u64),
    End(Result<Sequence, String>),
}
pub(super) struct Job {
    updates: mpsc::Receiver<Update>,
    stop: Arc<AtomicBool>,
    revision: u64,
    mode: Mode,
    pub done: u64,
    pub frames: u64,
    start: Time,
    duration: Time,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Job {
    pub fn poll(&mut self, cx: &mut PanelContext<'_>) -> Option<Result<(), String>> {
        if cx.stage.revision() != self.revision {
            return Some(Err(tr("The project changed during generation.").to_owned()));
        }
        loop {
            match self.updates.try_recv() {
                Ok(Update::Frame(n)) => self.done = n,
                Ok(Update::End(result)) => {
                    return Some(result.map(|seq| {
                        let _directory = seq.dir.keep();
                        match &self.mode {
                            Mode::Freeze(effect) => cx.invoke_with(
                                "roto.freeze",
                                serde_json::json!({
                                    "effect": effect, "file": seq.file,
                                    "start": self.start, "duration": self.duration
                                }),
                            ),
                            Mode::Fill(layer, _) => cx.invoke_with(
                                "vision.fill_layer",
                                serde_json::json!({
                                    "layer": layer, "file": seq.file, "origin": seq.origin,
                                    "start": self.start, "duration": self.duration
                                }),
                            ),
                        }
                    }));
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Some(Err(tr("Sequence renderer stopped.").to_owned()));
                }
            }
        }
    }
}
pub(super) fn start(
    cx: &PanelContext<'_>,
    ui: &egui::Context,
    layer: PrimPath,
    mode: Mode,
    range: [Time; 2],
) -> Result<Job, String> {
    let comp = query::owning_comp(cx.stage, &layer).ok_or("Missing composition")?;
    let info = query::comp_info(cx.stage, &comp).ok_or("Missing composition settings")?;
    let step = info.frame_duration().0;
    let first = (range[0].0.max(0) as u64).div_ceil(step as u64) as i64;
    let end = (range[1].min(info.duration).0.max(0) as u64).div_ceil(step as u64) as i64;
    if end <= first {
        return Err("Empty frame range".into());
    }
    let frames = (end - first) as u64;
    let ready = cx
        .resources
        .get::<Arc<BakeRenderer>>()
        .and_then(|r| r.0.lock().ok()?.clone())
        .ok_or_else(|| tr("The sequence renderer is warming up.").to_owned())?;
    let sources = cx.resources.get::<PreviewSources>().cloned();
    let effects = cx.resources.get::<PreviewEffects>().cloned();
    let folder = cx
        .resources
        .get::<bae_ui::files::ProjectFile>()
        .and_then(|p| p.get())
        .and_then(|p| p.parent().map(Path::to_owned))
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .map(|p| p.join("Movies"))
        })
        .ok_or("Missing generated media directory")?
        .join("BAE Generated");
    let snapshot = cx.stage.snapshot_reader();
    let (tx, updates) = mpsc::channel();
    let job = Job {
        updates,
        stop: Arc::default(),
        mode: mode.clone(),
        revision: cx.stage.revision(),
        done: 0,
        frames,
        start: Time(first * step),
        duration: Time((end - first) * step),
    };
    let stop = job.stop.clone();
    let ui = ui.clone();
    let name = match mode {
        Mode::Freeze(_) => tr("Frozen Matte").to_owned(),
        Mode::Fill(..) => tr("Fill Layer").to_owned(),
    };
    std::thread::Builder::new()
        .name("vision-bake".into())
        .spawn(move || {
            bae_gpu::warm_up::lower_priority();
            let render = || -> Result<Sequence, String> {
                let stage = snapshot().map_err(|e| e.to_string())?;
                let mut renderer = ready.lock().map_err(|e| e.to_string())?;
                if let Some(sources) = sources {
                    for e in sources.0.snapshot().iter() {
                        renderer.providers_mut().register(e.item().clone());
                    }
                }
                if let Some(effects) = effects {
                    renderer.sync_effects(&effects.0);
                }
                fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let dir = tempfile::Builder::new()
                    .prefix("sequence-")
                    .tempdir_in(folder)
                    .map_err(|e| e.to_string())?;
                let mut eval = Evaluator::new();
                let mut anim = AnimEvaluator::new(None).with_purpose(RenderPurpose::Final);
                let mut read_source =
                    |renderer: &mut Renderer,
                     n: i64|
                     -> Result<(bae_render::RenderedFrame, [f64; 2]), String> {
                        let t = Time(n * step);
                        let evaluated = eval
                            .evaluate_comp(&stage, &comp, t)
                            .map_err(|e| e.to_string())?;
                        let src = evaluated.layer(&layer).ok_or("Missing source layer")?;
                        let freeze = match &mode {
                            Mode::Freeze(effect) => Some(
                                src.effects
                                    .iter()
                                    .position(|e| &e.path == effect)
                                    .ok_or("Missing Roto Brush")?,
                            ),
                            Mode::Fill(..) => None,
                        };
                        let (mut one, min) = crate::layer_panel::alone(
                            &evaluated,
                            &layer,
                            freeze.map_or(src.effects.len(), |i| i + 1),
                        )
                        .ok_or("Empty layer bounds")?;
                        if let Some(i) = freeze {
                            one.layers[0].effects[i]
                                .params
                                .insert(format!("{:04}", bae_vision::BAKE), Value::Bool(true));
                        }
                        one.layers[0].masks = src.masks.clone();
                        let rc = anim.convert(&stage, &one, 0).map_err(|e| e.to_string())?;
                        let mut samples = bae_render::eval::EvaluatorFrames {
                            stage: &stage,
                            evaluator: &mut anim,
                        };
                        let frame = loop {
                            if stop.load(Ordering::Acquire) {
                                return Err("Generation cancelled".into());
                            }
                            let out = renderer
                                .render_frame_with(&rc, &RenderRequest::full(), Some(&mut samples))
                                .map_err(|e| e.to_string())?;
                            if !out.incomplete {
                                break out;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        };
                        if !frame.warnings.is_empty() {
                            return Err(frame.warnings.join("\n"));
                        }
                        Ok((frame, min.to_array()))
                    };
                let mut size = None;
                let mut origin = None;
                let mut cached = std::collections::BTreeMap::new();
                let settings = match &mode {
                    Mode::Fill(_, s) => Some(*s),
                    Mode::Freeze(_) => None,
                };
                let mut target = settings.map(|_| {
                    bae_effects::Standalone::new(
                        renderer.gpu().clone(),
                        renderer.work_format(&RenderRequest::full()),
                    )
                });
                let timing = query::layer_timing(&stage, &layer);
                let first_visible = (timing.in_point.0.max(0) as u64).div_ceil(step as u64) as i64;
                let last_visible = (timing.out_point.min(info.duration).0 - 1).div_euclid(step);
                for n in first..end {
                    if stop.load(Ordering::Acquire) {
                        return Err("Generation cancelled".into());
                    }
                    let temporal =
                        settings.is_some_and(|s| s.method == bae_vision::FillMethod::Object);
                    let left = if temporal {
                        (n - 1).max(first_visible).min(n)
                    } else {
                        n
                    };
                    let right = if temporal {
                        (n + 1).min(last_visible).max(n)
                    } else {
                        n
                    };
                    cached.retain(|&t, _| t >= left);
                    for t in left..=right {
                        if let std::collections::btree_map::Entry::Vacant(slot) = cached.entry(t) {
                            slot.insert(read_source(&mut renderer, t)?);
                        }
                    }
                    let (mut frame, min) = cached[&n].clone();
                    if origin.replace(min).is_some_and(|old| old != min) {
                        return Err("Layer bounds changed within the sequence".into());
                    }
                    if let (Some(s), Some(target)) = (settings, target.as_mut()) {
                        let neighbor = |t| {
                            let (f, pos) = &cached[&t];
                            if *pos == min && f.image.rect == frame.image.rect {
                                &f.image
                            } else {
                                &cached[&n].0.image
                            }
                        };
                        frame.image = bae_effects::content_aware_fill(
                            target,
                            &frame.image,
                            neighbor(left),
                            neighbor(right),
                            s,
                        )
                        .map_err(|e| e.to_string())?;
                        target.submit();
                    }
                    let pixels = renderer
                        .read_rgba8(&frame, OutputAlpha::Straight)
                        .map_err(|e| e.to_string())?;
                    if size
                        .replace([pixels.width, pixels.height])
                        .is_some_and(|s| s != [pixels.width, pixels.height])
                    {
                        return Err("Layer size changed within the sequence".into());
                    }
                    let file = dir.path().join(format!("frame_{:08}.png", n - first));
                    let stream = BufWriter::new(fs::File::create(file).map_err(|e| e.to_string())?);
                    let mut enc = png::Encoder::new(stream, pixels.width, pixels.height);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
                    writer
                        .write_image_data(&pixels.data)
                        .map_err(|e| e.to_string())?;
                    writer.finish().map_err(|e| e.to_string())?;
                    tx.send(Update::Frame((n - first + 1) as u64))
                        .map_err(|e| e.to_string())?;
                    ui.request_repaint();
                }
                let first = dir.path().join("frame_00000000.png");
                let media = bae_media::MediaFormats::builtin()
                    .open(&first, &bae_media::OpenOptions::sequence(info.frame_rate))
                    .map_err(|e| e.to_string())?;
                let file = ImportFile {
                    path: first.to_string_lossy().into_owned(),
                    name: Some(name),
                    folder: None,
                    attrs: media
                        .info()
                        .asset_attributes()
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect(),
                };
                Ok(Sequence {
                    dir,
                    file,
                    origin: origin.ok_or("Empty frame range")?,
                })
            };
            let _ = tx.send(Update::End(render()));
            ui.request_repaint();
        })
        .map_err(|e| e.to_string())?;
    Ok(job)
}
