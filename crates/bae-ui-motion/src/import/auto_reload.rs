//! Scan linked footage when the application regains focus. All filesystem work runs on one worker.
use super::{ImportDefaults, Probed};
use bae_app::Resources;
use bae_model::{AssetFile, PrimPath};
use bae_ui::PanelContext;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::SystemTime;

struct Source {
    item: PrimPath,
    file: AssetFile,
    sequence: bool,
    mode: bae_media::SequenceMode,
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
    /// The changed files of one scan (reloaded as one undo step).
    results: Mutex<mpsc::Receiver<Vec<Updated>>>,
}

#[derive(Default)]
pub(super) struct FocusReload {
    worker: OnceLock<Result<Worker, String>>,
    focused: AtomicBool,
}

impl FocusReload {
    fn worker(&self) -> Result<&Worker, &String> {
        self.worker
            .get_or_init(|| {
                let (send, scans) = mpsc::channel::<Scan>();
                let (complete, results) = mpsc::channel();
                std::thread::Builder::new()
                    .name("bae-focus-footage".into())
                    .spawn(move || run(scans, complete))
                    .map_err(|e| e.to_string())?;
                Ok(Worker {
                    send,
                    results: Mutex::new(results),
                })
            })
            .as_ref()
    }
}

#[derive(PartialEq)]
struct SequenceStamp {
    first: PathBuf,
    members: Vec<(i64, u64, Option<SystemTime>)>,
}

impl SequenceStamp {
    fn read(path: &Path) -> Option<Self> {
        let seq = bae_media::sequence::detect_sequence(path).ok()??;
        let first = seq.path_for(seq.first());
        let members = seq
            .frames
            .iter()
            .map(|n| {
                let info = std::fs::metadata(seq.path_for(*n)).ok()?;
                Some((*n, info.len(), info.modified().ok()))
            })
            .collect::<Option<_>>()?;
        Some(Self { first, members })
    }
}

fn run(scans: mpsc::Receiver<Scan>, complete: mpsc::Sender<Vec<Updated>>) {
    let mut sequences: HashMap<String, Arc<SequenceStamp>> = HashMap::new();
    for scan in scans {
        let assets = match &scan.project {
            Some(path) => match bae_render::AssetFiles::for_location(path, vec![]) {
                Ok(assets) => assets,
                Err(_) => continue,
            },
            None => bae_render::AssetFiles::new(None),
        };
        let mut next = HashMap::new();
        let mut updated = Vec::new();
        for source in scan.sources {
            let location = assets.locator().locate(&source.file);
            let linked = location.path().or_else(|| {
                // If the old first frame disappeared, locate the surviving numbered frames.
                (source.sequence && location.is_missing()).then(|| Path::new(&source.file.original))
            });
            let Some(mut path) = linked else { continue };
            let mut changed = false;
            if source.sequence {
                let original = &source.file.original;
                let stamp = match next.entry(original.clone()) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => {
                        let stamp = if !scan.focused
                            && let Some(prev) = sequences.get(original)
                        {
                            prev.clone()
                        } else if let Some(stamp) = SequenceStamp::read(path) {
                            Arc::new(stamp)
                        } else {
                            continue;
                        };
                        e.insert(stamp)
                    }
                };
                changed = sequences.get(original).is_some_and(|old| old != stamp)
                    || Path::new(&source.file.original).file_name() != stamp.first.file_name();
                path = &stamp.first;
            }
            if !scan.focused {
                continue;
            }
            let Ok(current) = AssetFile::from_path(path, None, false) else {
                continue;
            };
            if !changed
                && current.size == source.file.size
                && current.mtime_ms == source.file.mtime_ms
            {
                continue;
            }
            let mut probed = Probed::default();
            super::probe_file(
                path,
                None,
                source.mode,
                &scan.defaults,
                &mut HashSet::new(),
                &mut probed,
            );
            updated.push(Updated {
                source,
                revision: scan.revision,
                probed,
            });
        }
        sequences = next;
        if !updated.is_empty() {
            if complete.send(updated).is_err() {
                return;
            }
            scan.ctx.request_repaint();
        }
    }
}

impl bae_gpu::WarmUp for FocusReload {
    fn name(&self) -> &str {
        "footage focus watcher"
    }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        self.worker().map(|_| ()).map_err(Clone::clone)
    }
}

/// Drops the shared decoder's frames of `path`. Renderers reopen only that file:
/// Reload Footage raises the item's file revision.
pub(super) fn invalidate(resources: &Resources, path: &str) {
    if let Some(library) = resources.get::<Arc<bae_media::MediaLibrary>>() {
        library.invalidate(Path::new(path));
    }
}

pub(crate) fn on_focus(ctx: &egui::Context, cx: &mut PanelContext<'_>) {
    let Some(w) = cx.resources.get::<Arc<FocusReload>>() else {
        return;
    };
    let focused = ctx.input(|i| i.focused);
    let transition = w.focused.swap(focused, Ordering::Relaxed) != focused;
    let prefs = crate::prefs::current(cx.resources);
    let mode = prefs.str("import.auto_reload_mode");
    let all = mode == "all_footage_types";
    if transition && mode != "off" && (focused || all) {
        let sources = cx
            .stage
            .traverse()
            .into_iter()
            .filter_map(|item| {
                let prim = cx.stage.prim(&item)?;
                if !prim.is_a(&bae_model::PrimType::ASSET) {
                    return None;
                }
                let sequence =
                    prim.value("kind").and_then(bae_model::Value::as_str) == Some("sequence");
                if (!focused || sequence) && !all {
                    return None;
                }
                Some(Source {
                    file: AssetFile::from_prim(&prim)?,
                    mode: super::sequence_mode(&prim),
                    item,
                    sequence,
                })
            })
            .collect::<Vec<_>>();
        if !sources.is_empty()
            && let Ok(worker) = w.worker()
        {
            let revision = cx.stage.revision();
            let _ = cx.stage.changes_since(revision);
            let _ = worker.send.send(Scan {
                revision,
                sources,
                focused,
                project: cx
                    .resources
                    .get::<bae_ui::files::ProjectFile>()
                    .and_then(|p| p.get()),
                defaults: ImportDefaults::of(&prefs),
                ctx: ctx.clone(),
            });
        }
    }
    if let Some(Ok(worker)) = w.worker.get() {
        let batches: Vec<_> = worker.results.lock().unwrap().try_iter().collect();
        let mut reloads = Vec::new();
        for mut result in batches.into_iter().flatten() {
            if mode == "off"
                || result.source.sequence && !all
                || cx.stage.changes_since(result.revision).is_none()
            {
                continue;
            }
            let current = cx
                .stage
                .prim(&result.source.item)
                .and_then(|prim| AssetFile::from_prim(&prim));
            if current.as_ref() != Some(&result.source.file) {
                continue;
            }
            if let Some((path, _)) = result.probed.photoshop.pop() {
                // A changed layered PSD goes through the Photoshop import, as
                // File > Reload Footage does (its layers are rebuilt).
                let defaults = ImportDefaults::of(&prefs);
                super::queue_psd_replace(cx, &path, &result.source.item, &defaults, true);
            } else if let Some(file) = result.probed.files.pop() {
                reloads.push(bae_motion::ReloadItem {
                    item: result.source.item,
                    file,
                });
            }
            super::verify::report(cx, bae_ui::tr("Reload Footage"), result.probed);
        }
        super::apply_reloads(cx, reloads);
    }
}
