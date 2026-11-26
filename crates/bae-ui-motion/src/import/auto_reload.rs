//! Scan linked footage when the application regains focus. All filesystem work runs on one worker.
use super::{ImportDefaults, Probed};
use bae_app::Resources;
use bae_model::{AssetFile, PrimPath};
use bae_ui::PanelContext;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::SystemTime;

struct Source {
    item: PrimPath,
    file: AssetFile,
    sequence: bool,
}

struct Scan {
    revision: u64,
    project: Option<PathBuf>,
    sources: Vec<Source>,
    defaults: ImportDefaults,
    focused: bool,
    ctx: egui::Context,
}

struct Updated {
    source: Source,
    revision: u64,
    probed: Probed,
}

struct Worker {
    send: mpsc::Sender<Scan>,
    results: Mutex<mpsc::Receiver<Updated>>,
}

#[derive(Default)]
pub(super) struct FocusReload {
    worker: OnceLock<Result<Worker, String>>,
    focused: AtomicBool,
    generation: AtomicU64,
}

impl FocusReload {
    fn worker(&self) -> Result<&Worker, &String> {
        self.worker.get_or_init(|| {
            let (send, scans) = mpsc::channel::<Scan>();
            let (complete, results) = mpsc::channel();
            std::thread::Builder::new().name("bae-focus-footage".into()).spawn(move || {
                let mut sequences = HashMap::new();
                for scan in scans {
                    let assets = match &scan.project {
                        Some(path) => match bae_render::AssetFiles::for_location(path, vec![]) {
                            Ok(assets) => assets,
                            Err(_) => continue,
                        },
                        None => bae_render::AssetFiles::new(None),
                    };
                    let mut retained = HashSet::new();
                    for source in scan.sources {
                        let location = assets.locator().locate(&source.file);
                        let Some(path) = location.path() else { continue };
                        let mut changed = false;
                        if source.sequence {
                            retained.insert(path.to_owned());
                            if !scan.focused && sequences.contains_key(path) { continue }
                            let Some(stamp) = sequence_stamp(path) else { continue };
                            if scan.focused {
                                changed = sequences.get(path).is_some_and(|old| old != &stamp);
                            }
                            sequences.insert(path.to_owned(), stamp);
                        }
                        if !scan.focused { continue }
                        let Ok(current) = AssetFile::from_path(path, None, false) else { continue };
                        if !changed && current.size == source.file.size && current.mtime_ms == source.file.mtime_ms { continue }
                        let mut probed = Probed::default();
                        super::probe_file(path, None, source.sequence, &scan.defaults, &mut HashSet::new(), &mut probed);
                        if complete.send(Updated { source, revision: scan.revision, probed }).is_err() { return }
                        scan.ctx.request_repaint();
                    }
                    sequences.retain(|path, _| retained.contains(path));
                }
            }).map_err(|e| e.to_string())?;
            Ok(Worker { send, results: Mutex::new(results) })
        }).as_ref()
    }
}

fn sequence_stamp(path: &Path) -> Option<Vec<(i64, u64, Option<SystemTime>)>> {
    let seq = bae_media::sequence::detect_sequence(path).ok()??;
    seq.frames.iter().map(|n| {
        let info = std::fs::metadata(seq.path_for(*n)).ok()?;
        Some((*n, info.len(), info.modified().ok()))
    }).collect()
}

impl bae_gpu::WarmUp for FocusReload {
    fn name(&self) -> &str { "footage focus watcher" }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        self.worker().map(|_| ()).map_err(Clone::clone)
    }
}

pub(crate) fn generation(resources: &Resources) -> u64 {
    resources.get::<Arc<FocusReload>>().map_or(0, |w| w.generation.load(Ordering::Relaxed))
}

pub(super) fn invalidate(resources: &Resources) {
    if let Some(w) = resources.get::<Arc<FocusReload>>() {
        w.generation.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn on_focus(ctx: &egui::Context, cx: &mut PanelContext<'_>) {
    let Some(w) = cx.resources.get::<Arc<FocusReload>>() else { return };
    let focused = ctx.input(|i| i.focused);
    let transition = w.focused.swap(focused, Ordering::Relaxed) != focused;
    let prefs = crate::prefs::current(cx.resources);
    let mode = prefs.str("import.auto_reload_mode");
    let all = mode == "all_footage_types";
    if transition && mode != "off" && (focused || all) {
        let sources = cx.stage.traverse().into_iter().filter_map(|item| {
            let prim = cx.stage.prim(&item)?;
            if !prim.is_a(&bae_model::PrimType::ASSET) { return None }
            let sequence = prim.value("kind").and_then(bae_model::Value::as_str) == Some("sequence");
            if (!focused || sequence) && !all { return None }
            Some(Source { file: AssetFile::from_prim(&prim)?, item, sequence })
        }).collect::<Vec<_>>();
        if !sources.is_empty() && let Ok(worker) = w.worker() {
            let revision = cx.stage.revision();
            let _ = cx.stage.changes_since(revision);
            let _ = worker.send.send(Scan {
                revision,
                sources,
                focused,
                project: cx.resources.get::<bae_ui::files::ProjectFile>().and_then(|p| p.get()),
                defaults: ImportDefaults::of(&prefs),
                ctx: ctx.clone(),
            });
        }
    }
    if let Some(Ok(worker)) = w.worker.get() {
        for mut result in worker.results.lock().unwrap().try_iter() {
            if mode == "off" || result.source.sequence && !all || cx.stage.changes_since(result.revision).is_none() { continue }
            let current = cx.stage.prim(&result.source.item).and_then(|prim| AssetFile::from_prim(&prim));
            if current.as_ref() != Some(&result.source.file) { continue }
            if let Some(file) = result.probed.files.pop() {
                super::apply_reload(cx, result.source.item, file);
            }
            super::verify::report(cx, bae_ui::tr("Reload Footage"), result.probed);
        }
    }
}
