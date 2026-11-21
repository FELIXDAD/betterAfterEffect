//! Render Queue (AE: Composition ▸ Add to Render Queue, Cmd/Ctrl+M; Window ▸
//! Render Queue): comps queued with an output format, path and frame range
//! (the work area by default), rendered one after another on a background
//! thread through 04's export pipeline ([`bae_render::export_frames`]: async
//! readback, parallel encoders), with progress, cancel and "show in Finder".
//!
//! Rendering never blocks the UI: the stage is snapshotted when Render is
//! pressed (later edits do not affect the running render, as in AE), and the
//! worker has its own renderer on the shared GPU context. The queue is runtime
//! state of the app ([`SharedRenderQueue`] resource), not project data.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use bae_app::{Registry, Resources};
use bae_gpu::GpuContext;
use bae_model::{FrameRate, PrimPath, Stage, Sublayer, SublayerId, Time};
use bae_motion::query::CompInfo;
use bae_render::{
    AnimEvaluator, ExportFormat, ExportJob, ExportSettings, RenderError, RenderPurpose,
    RenderRequest, Renderer, RendererConfig, SourceProvider, export_frames, frame_file,
};
use bae_ui::Accessible as _;
use bae_ui::{Panel, PanelContext, TabInfo, tr};

use crate::gpu_preview::PreviewSources;

/// Formats offered in the panel (movies come with 08's Media Foundation work
/// on Windows; `bae render --format` has them on macOS).
pub const FORMATS: [ExportFormat; 3] = [ExportFormat::Png, ExportFormat::Tiff, ExportFormat::Exr];

/// State of a queued item.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemStatus {
    /// Waiting for Render.
    Queued,
    /// Being rendered: frames handed to the encoder so far.
    Rendering {
        /// Frames done.
        done: usize,
        /// Frames in the range.
        total: usize,
    },
    /// Finished.
    Done {
        /// Files written.
        files: usize,
        /// The first file (shown in Finder / Explorer).
        first: PathBuf,
        /// Wall time.
        seconds: f64,
    },
    /// Failed with this message.
    Failed(String),
    /// Stopped by the user; the frames rendered before are kept (AE).
    Stopped {
        /// Frames written.
        done: usize,
        /// Frames in the range.
        total: usize,
        /// The first file written, if any.
        first: Option<PathBuf>,
    },
}

/// One comp in the queue.
#[derive(Clone, Debug)]
pub struct QueueItem {
    /// Unique within the queue.
    pub id: u64,
    /// The comp.
    pub comp: PrimPath,
    /// Its name when queued.
    pub comp_name: String,
    /// Its frame rate.
    pub rate: FrameRate,
    /// Output format.
    pub format: ExportFormat,
    /// Output file; for sequences a pattern (`name_[####].png`).
    pub output: String,
    /// First and last frame (comp frames, inclusive).
    pub start: i64,
    /// Last frame (inclusive).
    pub end: i64,
    /// Current state.
    pub status: ItemStatus,
    cancel: Arc<AtomicBool>,
}

impl QueueItem {
    /// Frames in the range.
    pub fn frame_count(&self) -> usize {
        (self.end - self.start + 1).max(0) as usize
    }

    /// Switches the format and the output's extension with it.
    pub fn set_format(&mut self, format: ExportFormat) {
        self.format = format;
        let path = Path::new(&self.output);
        if path.extension().is_some_and(|e| {
            ExportFormat::parse(&e.to_string_lossy().to_ascii_lowercase()).is_some()
        }) {
            self.output = path
                .with_extension(format.extension())
                .to_string_lossy()
                .into_owned();
        }
    }
}

/// The render queue (shared between the panel, the menu action and the
/// worker thread).
#[derive(Debug, Default)]
pub struct RenderQueue {
    items: Vec<QueueItem>,
    next_id: u64,
    running: bool,
    /// Items of the running render (queued when Render was pressed; later ones
    /// wait for the next Render, the snapshot does not have their edits).
    batch: Vec<u64>,
    repaint: Option<egui::Context>,
}

impl RenderQueue {
    /// The items in order.
    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    /// Mutable access to an item (output settings of queued items).
    pub fn item_mut(&mut self, id: u64) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// Whether the worker is rendering.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Queues `comp` with AE's defaults: PNG sequence, the work area, a folder
    /// named after the comp under `output_dir`. Returns the item id.
    pub fn add(&mut self, comp: &CompInfo, output_dir: &Path) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let format = ExportFormat::Png;
        let name = sanitize(&comp.name);
        let output = output_dir
            .join(&name)
            .join(format!("{name}_[####].{}", format.extension()));
        let rate = comp.frame_rate;
        let start = rate.time_to_frame(comp.work_area.start);
        // The work area's end is exclusive.
        let end = (rate.time_to_frame(comp.work_area.end) - 1).max(start);
        self.items.push(QueueItem {
            id,
            comp: comp.path.clone(),
            comp_name: comp.name.clone(),
            rate,
            format,
            output: output.to_string_lossy().into_owned(),
            start,
            end,
            status: ItemStatus::Queued,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        id
    }

    /// Removes an item that is not rendering.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items
            .retain(|i| i.id != id || matches!(i.status, ItemStatus::Rendering { .. }));
        self.items.len() != before
    }

    /// Stops a rendering item; a queued one is canceled and leaves the
    /// running render (queued again, it waits for the next Render).
    pub fn cancel(&mut self, id: u64) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.cancel.store(true, Ordering::Relaxed);
            if item.status == ItemStatus::Queued {
                item.status = ItemStatus::Stopped {
                    done: 0,
                    total: item.frame_count(),
                    first: None,
                };
                self.batch.retain(|b| *b != id);
            }
        }
    }

    /// Puts a finished, failed or canceled item back in the queue.
    pub fn requeue(&mut self, id: u64) {
        if let Some(item) = self.item_mut(id)
            && !matches!(item.status, ItemStatus::Rendering { .. })
        {
            item.status = ItemStatus::Queued;
            item.cancel = Arc::new(AtomicBool::new(false));
        }
    }

    /// Items waiting for Render.
    pub fn queued(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == ItemStatus::Queued)
            .count()
    }

    /// Starts a render of the items queued now. `false` when already
    /// rendering or nothing is queued.
    fn begin(&mut self) -> bool {
        if self.running || self.queued() == 0 {
            return false;
        }
        self.running = true;
        self.batch = self
            .items
            .iter()
            .filter(|i| i.status == ItemStatus::Queued)
            .map(|i| i.id)
            .collect();
        true
    }

    /// Takes the next item of the running render.
    fn take_next(&mut self) -> Option<Job> {
        let batch = &mut self.batch;
        let item = self
            .items
            .iter_mut()
            .find(|i| i.status == ItemStatus::Queued && batch.contains(&i.id))?;
        batch.retain(|id| *id != item.id);
        item.status = ItemStatus::Rendering {
            done: 0,
            total: item.frame_count(),
        };
        Some(Job {
            id: item.id,
            comp: item.comp.clone(),
            rate: item.rate,
            format: item.format,
            output: PathBuf::from(&item.output),
            start: item.start,
            end: item.end,
            cancel: item.cancel.clone(),
        })
    }

    fn set_status(&mut self, id: u64, status: ItemStatus) {
        if let Some(item) = self.item_mut(id) {
            item.status = status;
        }
        if let Some(ctx) = &self.repaint {
            ctx.request_repaint();
        }
    }
}

/// The queue as an app resource.
#[derive(Clone, Debug, Default)]
pub struct SharedRenderQueue(Arc<Mutex<RenderQueue>>);

impl SharedRenderQueue {
    /// Locks the queue.
    pub fn lock(&self) -> MutexGuard<'_, RenderQueue> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The app's queue (a per-thread one where the app has none: tests).
pub(crate) fn shared(resources: &Resources) -> SharedRenderQueue {
    resources
        .get::<SharedRenderQueue>()
        .cloned()
        .unwrap_or_else(|| {
            thread_local! {
                static LOCAL: SharedRenderQueue = SharedRenderQueue::default();
            }
            LOCAL.with(Clone::clone)
        })
}

/// Where new items write: the user's Movies (macOS) / Videos (Windows)
/// folder, else the home folder, else the current directory.
pub fn default_output_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    match home {
        Some(home) => ["Movies", "Videos"]
            .iter()
            .map(|d| home.join(d))
            .find(|d| d.is_dir())
            .unwrap_or(home),
        None => PathBuf::from("."),
    }
}

/// A file-name-safe version of a comp name.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_owned();
    if cleaned.is_empty() {
        "comp".into()
    } else {
        cleaned
    }
}

/// What the worker renders for one item.
#[derive(Clone, Debug)]
pub(crate) struct Job {
    id: u64,
    comp: PrimPath,
    rate: FrameRate,
    format: ExportFormat,
    output: PathBuf,
    start: i64,
    end: i64,
    cancel: Arc<AtomicBool>,
}

/// The project as of now, for the worker thread (sublayer snapshots, strongest
/// first).
pub(crate) struct StageSnapshot(Vec<(SublayerId, Vec<u8>)>);

impl StageSnapshot {
    pub(crate) fn take(stage: &Stage) -> Result<Self, String> {
        stage
            .layer_stack()
            .iter()
            .map(|id| {
                let sublayer = stage
                    .sublayer(id)
                    .ok_or_else(|| format!("sublayer {id:?} missing"))?;
                let bytes = sublayer.export_snapshot().map_err(|e| e.to_string())?;
                Ok((id.clone(), bytes))
            })
            .collect::<Result<_, String>>()
            .map(StageSnapshot)
    }

    pub(crate) fn restore(&self) -> Result<Stage, String> {
        let sublayers = self
            .0
            .iter()
            .map(|(id, bytes)| Sublayer::from_snapshot(id.clone(), bytes))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Stage::from_sublayers(sublayers).map_err(|e| e.to_string())
    }
}

/// Renders one item; `progress` gets the frames done so far.
pub(crate) fn render_job(
    renderer: &mut Renderer,
    evaluator: &mut AnimEvaluator,
    stage: &Stage,
    job: &Job,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ItemStatus, RenderError> {
    let started = Instant::now();
    let info = bae_motion::query::comp_info(stage, &job.comp)
        .ok_or_else(|| RenderError::Other(tr("The composition no longer exists.").to_owned()))?;
    let output = output_path(&job.output, job.format, &info.name)?;
    // AE limits the range to the comp.
    let last = info.frame_count() - 1;
    let (start, end) = (job.start.max(0), job.end.min(last));
    if start > end {
        return Err(RenderError::Other(
            tr("The frames are outside the composition (0–{last}).")
                .replace("{last}", &last.to_string()),
        ));
    }
    let times: Vec<(Option<i64>, Time)> = (start..=end)
        .map(|f| (Some(f), job.rate.frame_to_time(f)))
        .collect();
    // The progress counts the frames actually rendered (the limited range).
    let total = times.len();
    progress(0, total);
    // One frame of a sequence pattern still gets a numbered file.
    let path = if times.len() == 1 && job.format.is_image() {
        frame_file(&output, start)
    } else {
        output
    };
    let settings = ExportSettings::new(job.format, path);
    let request = RenderRequest {
        purpose: RenderPurpose::Final,
        ..RenderRequest::default()
    };
    settings.validate(job.rate, &request)?;
    let report = export_frames(
        renderer,
        evaluator,
        &ExportJob {
            stage,
            comp: &job.comp,
            times: &times,
            rate: job.rate,
            request: &request,
            settings: &settings,
            encoders: None,
        },
        Some(&job.cancel),
        &mut |frame| progress(frame.index + 1, total),
    )?;
    let first = report.frames.first().map(|f| f.file.clone());
    Ok(if report.stopped {
        ItemStatus::Stopped {
            done: report.frames.len(),
            total: times.len(),
            first,
        }
    } else {
        ItemStatus::Done {
            files: report.stats.files,
            first: first.unwrap_or_default(),
            seconds: started.elapsed().as_secs_f64(),
        }
    })
}

/// The output file to write (AE rules):
/// - an empty one is refused (it would land in the process's working
///   directory, `/` for an app started from Finder);
/// - a folder (ending in a separator, or an existing directory) gets
///   `<comp>_[####].<ext>` inside it;
/// - the format decides the extension: a missing one is added, another image
///   or movie extension is replaced (the panel says so,
///   [`extension_follows_format`]).
fn output_path(output: &Path, format: ExportFormat, comp: &str) -> Result<PathBuf, RenderError> {
    let text = output.as_os_str().to_string_lossy();
    if text.trim().is_empty() {
        return Err(RenderError::Other(
            tr("Choose where to render: Output To is empty.").to_owned(),
        ));
    }
    if text.ends_with(['/', '\\']) || output.is_dir() || output.file_name().is_none() {
        let name = sanitize(comp);
        return Ok(output.join(format!("{name}_[####].{}", format.extension())));
    }
    let known = output
        .extension()
        .and_then(|e| ExportFormat::parse(&e.to_string_lossy().to_ascii_lowercase()));
    Ok(match known {
        Some(f) if f == format => output.to_owned(),
        Some(_) => output.with_extension(format.extension()),
        None => {
            let mut name = output.file_name().unwrap_or_default().to_os_string();
            name.push(".");
            name.push(format.extension());
            output.with_file_name(name)
        }
    })
}

/// The extension the output will get when the typed one does not match the
/// format (`None` when it matches or there is none yet).
fn extension_follows_format(output: &str, format: ExportFormat) -> Option<&'static str> {
    let ext = Path::new(output.trim()).extension()?;
    let typed = ExportFormat::parse(&ext.to_string_lossy().to_ascii_lowercase())?;
    (typed != format).then(|| format.extension())
}

/// Starts rendering the queued items on a background thread, one after
/// another. Does nothing when already rendering or nothing is queued.
pub fn start(
    queue: &SharedRenderQueue,
    stage: &Stage,
    resources: &Resources,
) -> Result<(), String> {
    let gpu = resources
        .get::<Arc<GpuContext>>()
        .cloned()
        .ok_or_else(|| tr("Rendering needs a GPU."))?;
    let providers: Vec<Arc<dyn SourceProvider>> = resources
        .get::<PreviewSources>()
        .map(|s| provider_list(&s.0))
        .unwrap_or_default();
    if !queue.lock().begin() {
        return Ok(());
    }
    let snapshot = match StageSnapshot::take(stage) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            queue.lock().running = false;
            return Err(error);
        }
    };
    let queue = queue.clone();
    std::thread::Builder::new()
        .name("bae-render-queue".into())
        .spawn(move || run_worker(&queue, &snapshot, gpu, &providers))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn provider_list(registry: &Registry<dyn SourceProvider>) -> Vec<Arc<dyn SourceProvider>> {
    registry
        .snapshot()
        .iter()
        .map(|e| e.item().clone())
        .collect()
}

/// The worker: renders queued items until none is left.
pub(crate) fn run_worker(
    queue: &SharedRenderQueue,
    snapshot: &StageSnapshot,
    gpu: Arc<GpuContext>,
    providers: &[Arc<dyn SourceProvider>],
) {
    let stage = snapshot.restore();
    let mut renderer = Renderer::with_config(
        gpu,
        RendererConfig {
            frame_cache_bytes: 0,
            ..RendererConfig::default()
        },
    );
    for provider in providers {
        renderer.providers_mut().register(provider.clone());
    }
    let mut evaluator = AnimEvaluator::new(None);
    loop {
        let Some(job) = queue.lock().take_next() else {
            break;
        };
        let status = match &stage {
            Err(error) => ItemStatus::Failed(error.clone()),
            Ok(stage) => {
                let mut progress = |done: usize, total: usize| {
                    let mut q = queue.lock();
                    q.set_status(job.id, ItemStatus::Rendering { done, total });
                };
                match render_job(&mut renderer, &mut evaluator, stage, &job, &mut progress) {
                    Ok(status) => status,
                    Err(error) => ItemStatus::Failed(error.to_string()),
                }
            }
        };
        queue.lock().set_status(job.id, status);
    }
    let mut q = queue.lock();
    q.running = false;
    q.batch.clear();
    if let Some(ctx) = &q.repaint {
        ctx.request_repaint();
    }
}

/// Shows `path` selected in Finder / Explorer (its folder elsewhere).
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(windows)]
    let result = {
        use std::os::windows::process::CommandExt;
        // explorer parses its own command line: a quoted "/select,…" argument
        // (what `arg` makes of a path with spaces) is not recognised and it
        // opens Documents instead. Quote only the path.
        std::process::Command::new("explorer")
            .raw_arg(explorer_select_arg(path))
            .spawn()
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    if let Err(error) = result {
        tracing::warn!("cannot show {}: {error}", path.display());
    }
}

/// explorer.exe's argument selecting `path`: `/select,"<path>"`.
#[cfg_attr(not(windows), allow(dead_code))]
fn explorer_select_arg(path: &Path) -> String {
    format!("/select,\"{}\"", path.display())
}

/// "Show in Finder" in the platform's words.
fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(windows) {
        "Show in Explorer"
    } else {
        "Show in Folder"
    }
}

/// The Render Queue panel.
#[derive(Debug, Default)]
pub struct RenderQueuePanel {
    error: Option<String>,
}

impl Panel for RenderQueuePanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Render Queue"))
    }

    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        let queue = shared(cx.resources);
        let (running, queued) = {
            let mut q = queue.lock();
            q.repaint = Some(ui.ctx().clone());
            (q.running, q.queued())
        };
        let mut render = false;
        ui.horizontal(|ui| {
            render = ui
                .add_enabled(!running && queued > 0, egui::Button::new(tr("Render")))
                .clicked();
            if running {
                ui.spinner();
                ui.label(tr("Rendering"));
            }
        });
        if render {
            // `start` locks the queue itself.
            self.error = start(&queue, cx.stage, cx.resources).err();
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.separator();
        let mut q = queue.lock();
        if q.items.is_empty() {
            ui.weak(tr(
                "Add a composition with Composition > Add to Render Queue.",
            ));
            return;
        }
        let mut remove = None;
        let mut cancel = None;
        let mut requeue = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, item) in q.items.iter_mut().enumerate() {
                let last =
                    bae_motion::query::comp_info(cx.stage, &item.comp).map(|c| c.frame_count() - 1);
                ui.push_id(item.id, |ui| {
                    item_ui(
                        ui,
                        i + 1,
                        item,
                        last,
                        &mut remove,
                        &mut cancel,
                        &mut requeue,
                    );
                });
                ui.separator();
            }
        });
        if let Some(id) = remove {
            q.remove(id);
        }
        if let Some(id) = cancel {
            q.cancel(id);
        }
        if let Some(id) = requeue {
            q.requeue(id);
        }
        if q.running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn item_ui(
    ui: &mut egui::Ui,
    number: usize,
    item: &mut QueueItem,
    last: Option<i64>,
    remove: &mut Option<u64>,
    cancel: &mut Option<u64>,
    requeue: &mut Option<u64>,
) {
    let editable = matches!(
        item.status,
        ItemStatus::Queued | ItemStatus::Failed(_) | ItemStatus::Stopped { .. }
    );
    // Screen-reader names carry the item ("Render Item 1 “Comp 1” · Output To"):
    // two items show the same controls, possibly for the same comp (#128).
    let item_ctx = tr("Render Item {n} “{comp}”")
        .replace("{n}", &number.to_string())
        .replace("{comp}", &item.comp_name);
    let name = |what: &str| bae_ui::a11y::context(&[&item_ctx, tr(what)]);
    // Name, status and actions wrap in a narrow panel instead of running
    // off its edge.
    ui.horizontal_wrapped(|ui| {
        ui.strong(&item.comp_name);
        match &item.status {
            ItemStatus::Queued => {
                ui.label(tr("Queued"));
            }
            ItemStatus::Rendering { done, total } => {
                let fraction = if *total == 0 {
                    0.0
                } else {
                    *done as f32 / *total as f32
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width(ui.available_width().clamp(60.0, 160.0))
                        .text(format!("{done} / {total}")),
                );
                if ui
                    .button(tr("Stop"))
                    .labeled(egui::WidgetType::Button, &name("Stop"))
                    .clicked()
                {
                    *cancel = Some(item.id);
                }
            }
            ItemStatus::Done {
                files,
                first,
                seconds,
            } => {
                ui.label(format!("{} · {files} · {seconds:.1} s", tr("Done")));
                if ui
                    .button(tr(reveal_label()))
                    .labeled(egui::WidgetType::Button, &name(reveal_label()))
                    .clicked()
                {
                    reveal(first);
                }
            }
            ItemStatus::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, tr("Failed"))
                    .on_hover_text(message);
            }
            ItemStatus::Stopped { done, total, first } => {
                ui.label(
                    tr("Stopped ({done}/{total} frames)")
                        .replace("{done}", &done.to_string())
                        .replace("{total}", &total.to_string()),
                );
                if let Some(first) = first
                    && ui
                        .button(tr(reveal_label()))
                        .labeled(egui::WidgetType::Button, &name(reveal_label()))
                        .clicked()
                {
                    reveal(first);
                }
            }
        }
        if !matches!(
            item.status,
            ItemStatus::Rendering { .. } | ItemStatus::Queued
        ) && ui
            .button(tr("Render Again"))
            .labeled(egui::WidgetType::Button, &name("Render Again"))
            .clicked()
        {
            *requeue = Some(item.id);
        }
        if !matches!(item.status, ItemStatus::Rendering { .. })
            && ui
                .button(tr("Remove"))
                .labeled(egui::WidgetType::Button, &name("Remove"))
                .clicked()
        {
            *remove = Some(item.id);
        }
    });
    if let ItemStatus::Failed(message) = &item.status {
        ui.colored_label(ui.visuals().error_fg_color, message);
    }
    ui.add_enabled_ui(editable, |ui| {
        egui::Grid::new("output").num_columns(2).show(ui, |ui| {
            // Fields are named after the label in front of them and the item
            // (screen readers read "… · Format: PNG", issue #128).
            ui.label(tr("Format"));
            let mut format = item.format;
            egui::ComboBox::from_id_salt("format")
                .selected_text(format_label(format))
                .show_ui(ui, |ui| {
                    for f in FORMATS {
                        ui.selectable_value(&mut format, f, format_label(f));
                    }
                })
                .response
                .labeled_value(
                    egui::WidgetType::ComboBox,
                    &name("Format"),
                    format_label(item.format),
                );
            if format != item.format {
                item.set_format(format);
            }
            ui.end_row();
            ui.label(tr("Output To"));
            ui.vertical(|ui| {
                let output = item.output.clone();
                ui.add(egui::TextEdit::singleline(&mut item.output).desired_width(f32::INFINITY))
                    .labeled_value(egui::WidgetType::TextEdit, &name("Output To"), &output);
                // AE: the format decides the extension.
                if let Some(ext) = extension_follows_format(&item.output, item.format) {
                    ui.weak(
                        tr("The file will be saved as .{ext} to match the format.")
                            .replace("{ext}", ext),
                    );
                }
            });
            ui.end_row();
            ui.label(tr("Frames"));
            // Typed and dragged values stay inside the comp (AE); a range set
            // otherwise (or a comp shortened later) is limited, or refused when
            // entirely outside, at render time.
            let last = last.unwrap_or(i64::MAX).max(0);
            ui.horizontal(|ui| {
                let start = item.start.to_string();
                ui.add(
                    egui::DragValue::new(&mut item.start)
                        .range(0..=item.end.clamp(0, last))
                        .clamp_existing_to_range(false),
                )
                .labeled_value(
                    egui::WidgetType::DragValue,
                    &bae_ui::a11y::context(&[&item_ctx, tr("Frames"), tr("Start")]),
                    &start,
                );
                ui.label("–");
                let end = item.end.to_string();
                ui.add(
                    egui::DragValue::new(&mut item.end)
                        .range(item.start.clamp(0, last)..=last)
                        .clamp_existing_to_range(false),
                )
                .labeled_value(
                    egui::WidgetType::DragValue,
                    &bae_ui::a11y::context(&[&item_ctx, tr("Frames"), tr("End")]),
                    &end,
                );
            });
            ui.end_row();
        });
    });
}

fn format_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Png => "PNG",
        ExportFormat::Tiff => "TIFF",
        ExportFormat::Exr => "OpenEXR",
        ExportFormat::H264 => "H.264",
        ExportFormat::Hevc => "HEVC",
        ExportFormat::ProRes => "ProRes 422 HQ",
        ExportFormat::ProRes4444 => "ProRes 4444",
    }
}

#[cfg(test)]
mod tests {
    use bae_model::{Project, TimeRange, Value};

    use super::*;

    fn comp_info(path: &str, name: &str) -> CompInfo {
        CompInfo {
            path: PrimPath::new(path).unwrap(),
            name: name.into(),
            width: 40,
            height: 24,
            pixel_aspect: 1.0,
            frame_rate: FrameRate::FPS_30,
            duration: Time::ONE_SECOND,
            // Frames 3–8: the work area's end is exclusive.
            work_area: TimeRange::new(
                FrameRate::FPS_30.frame_to_time(3),
                FrameRate::FPS_30.frame_to_time(9),
            ),
            bg_color: [0.0, 0.0, 0.0, 1.0],
            display_start: Time::ZERO,
        }
    }

    #[test]
    fn queue_bookkeeping() {
        let mut q = RenderQueue::default();
        let dir = Path::new("/renders");
        let a = q.add(&comp_info("/comps/main", "Main: v2"), dir);
        let b = q.add(&comp_info("/comps/other", "Other"), dir);
        let item = &q.items()[0];
        assert_eq!((item.start, item.end, item.frame_count()), (3, 8, 6));
        assert_eq!(item.format, ExportFormat::Png);
        assert_eq!(
            PathBuf::from(&item.output),
            dir.join("Main_ v2/Main_ v2_[####].png")
        );
        q.item_mut(a).unwrap().set_format(ExportFormat::Tiff);
        assert!(q.items()[0].output.ends_with("_[####].tif"));
        assert_eq!(q.queued(), 2);

        // Taking a job marks it rendering; it cannot be removed meanwhile.
        assert!(q.begin());
        assert!(!q.begin(), "already rendering");
        let late = q.add(&comp_info("/comps/late", "Late"), dir);
        let job = q.take_next().unwrap();
        assert_eq!((job.id, job.start, job.end), (a, 3, 8));
        assert!(matches!(
            q.items()[0].status,
            ItemStatus::Rendering { done: 0, total: 6 }
        ));
        assert!(!q.remove(a));
        q.cancel(a);
        assert!(
            job.cancel.load(Ordering::Relaxed),
            "the worker sees the stop"
        );

        // A queued item that is stopped is canceled; it can be queued again.
        q.cancel(b);
        assert_eq!(
            q.items()[1].status,
            ItemStatus::Stopped {
                done: 0,
                total: 6,
                first: None
            }
        );
        q.requeue(b);
        assert_eq!(q.items()[1].status, ItemStatus::Queued);
        assert!(!q.items()[1].cancel.load(Ordering::Relaxed));
        // Queued after Render was pressed: waits for the next render.
        assert!(q.take_next().is_none());
        assert_eq!(q.items()[2].status, ItemStatus::Queued);
        assert!(q.remove(late));
        assert!(q.remove(b));
        assert_eq!(q.items().len(), 1);
        assert_eq!(sanitize("  ../a#b  "), "_a_b");
        assert_eq!(sanitize("..."), "comp");
    }

    /// Issue #110: an empty output is refused; a name without the format's
    /// extension gets it; one with an image or movie extension is kept.
    #[test]
    fn output_path_rules() {
        assert!(output_path(Path::new(""), ExportFormat::Png, "C").is_err());
        assert!(output_path(Path::new("  "), ExportFormat::Png, "C").is_err());
        let p = |s: &str, f| output_path(Path::new(s), f, "Comp 1").unwrap();
        assert_eq!(
            p("/r/noext_[####]", ExportFormat::Png),
            Path::new("/r/noext_[####].png")
        );
        assert_eq!(
            p("/r/v1.2_[####]", ExportFormat::Tiff),
            Path::new("/r/v1.2_[####].tif")
        );
        assert_eq!(
            p("/r/a_####.exr", ExportFormat::Exr),
            Path::new("/r/a_####.exr")
        );
        // The format decides the extension (AE).
        assert_eq!(
            p("/r/a_####.png", ExportFormat::Tiff),
            Path::new("/r/a_####.tif")
        );
        assert_eq!(
            p("/r/a_####.tiff", ExportFormat::Tiff),
            Path::new("/r/a_####.tiff")
        );
        // A folder: the comp's name inside it.
        assert_eq!(
            p("/r/out/", ExportFormat::Png),
            Path::new("/r/out/Comp 1_[####].png")
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            p(dir.path().to_str().unwrap(), ExportFormat::Exr),
            dir.path().join("Comp 1_[####].exr")
        );
        assert_eq!(
            extension_follows_format("/r/a.png", ExportFormat::Tiff),
            Some("tif")
        );
        assert_eq!(
            extension_follows_format("/r/a.tif", ExportFormat::Tiff),
            None
        );
        assert_eq!(extension_follows_format("/r/a", ExportFormat::Tiff), None);
    }

    #[test]
    fn explorer_selects_paths_with_spaces() {
        // The whole argument must not be quoted, only the path.
        assert_eq!(
            explorer_select_arg(Path::new(r"E:\bae-test\rq final\png\Comp1_0000.png")),
            r#"/select,"E:\bae-test\rq final\png\Comp1_0000.png""#
        );
    }

    /// Stop keeps the frames rendered before it (AE): the item shows
    /// "Stopped (N/M frames)" and the N files are on disk.
    #[test]
    fn stop_keeps_the_rendered_frames() {
        let Some(gpu) = gpu() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (stage, comp) = project();
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let mut q = RenderQueue::default();
        let id = q.add(&info, dir.path());
        q.item_mut(id).unwrap().output = dir
            .path()
            .join("stop/f_####.png")
            .to_string_lossy()
            .into_owned();
        assert!(q.begin());
        let job = q.take_next().unwrap();
        let mut renderer = Renderer::new(gpu);
        let mut evaluator = AnimEvaluator::new(None);
        let cancel = job.cancel.clone();
        let status = render_job(
            &mut renderer,
            &mut evaluator,
            &stage,
            &job,
            &mut |done, _| {
                if done == 5 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        let ItemStatus::Stopped { done, total, first } = status else {
            panic!("{status:?}");
        };
        assert_eq!(total, 30);
        assert!((5..30).contains(&done), "{done}");
        assert_eq!(first, Some(dir.path().join("stop/f_0000.png")));
        let on_disk = std::fs::read_dir(dir.path().join("stop")).unwrap().count();
        assert_eq!(on_disk, done, "every rendered frame is kept");
    }

    #[test]
    fn start_needs_a_gpu() {
        let queue = SharedRenderQueue::default();
        queue
            .lock()
            .add(&comp_info("/comps/main", "Main"), Path::new("/x"));
        let error = start(&queue, &Stage::new(), &Resources::new()).unwrap_err();
        assert!(error.contains("GPU"), "{error}");
        assert!(!queue.lock().is_running());
    }

    /// A small animated comp (the stage goes through a snapshot like the
    /// panel's).
    fn project() -> (Stage, PrimPath) {
        let mut b = bae_render::fixture::SceneBuilder::new();
        let comp = b
            .comp("main", 40, 24, FrameRate::FPS_30, Time::ONE_SECOND)
            .unwrap();
        let solid = b
            .solid_layer(&comp, "red", [1.0, 0.0, 0.0, 1.0], [10, 10], [0.0, 0.0])
            .unwrap();
        b.key(
            &solid,
            "transform:position",
            Time::ZERO,
            Value::Vec3([5.0, 5.0, 0.0]),
        )
        .unwrap();
        b.key(
            &solid,
            "transform:position",
            Time::ONE_SECOND,
            Value::Vec3([35.0, 19.0, 0.0]),
        )
        .unwrap();
        let stage = Project::from_stage(b.into_stage()).stage;
        (stage, comp)
    }

    fn gpu() -> Option<Arc<GpuContext>> {
        bae_gpu::testing::shared_context()
    }

    /// The worker renders every queued item in order (PNG and TIFF), with
    /// progress; stopped and failing items end canceled / failed without
    /// output, and the queue goes idle.
    #[test]
    fn worker_renders_cancels_and_reports_errors() {
        let Some(gpu) = gpu() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (stage, comp) = project();
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let queue = SharedRenderQueue::default();
        let (png, tiff, stopped, bad) = {
            let mut q = queue.lock();
            let png = q.add(&info, dir.path());
            let tiff = q.add(&info, dir.path());
            let stopped = q.add(&info, dir.path());
            let bad = q.add(&info, dir.path());
            q.item_mut(png).unwrap().output =
                dir.path().join("png/f_[####].png").to_string_lossy().into();
            let t = q.item_mut(tiff).unwrap();
            t.set_format(ExportFormat::Tiff);
            t.output = dir.path().join("tif/f_####.tif").to_string_lossy().into();
            (t.start, t.end) = (4, 5);
            q.item_mut(stopped).unwrap().output = dir
                .path()
                .join("stopped/f_####.png")
                .to_string_lossy()
                .into();
            // A regular file where the output folder should be.
            std::fs::write(dir.path().join("blocker"), b"x").unwrap();
            q.item_mut(bad).unwrap().output = dir
                .path()
                .join("blocker/f_####.png")
                .to_string_lossy()
                .into();
            assert!(q.begin());
            (png, tiff, stopped, bad)
        };
        // Stopped before its turn: canceled, nothing written.
        queue
            .lock()
            .item_mut(stopped)
            .unwrap()
            .cancel
            .store(true, Ordering::Relaxed);

        let snapshot = StageSnapshot::take(&stage).unwrap();
        run_worker(&queue, &snapshot, gpu, &[]);

        let q = queue.lock();
        assert!(!q.is_running());
        let status = |id: u64| {
            q.items()
                .iter()
                .find(|i| i.id == id)
                .unwrap()
                .status
                .clone()
        };
        match status(png) {
            ItemStatus::Done { files, first, .. } => {
                assert_eq!(files, 30, "the work area: frames 0–29 of a 1 s comp");
                assert_eq!(first, dir.path().join("png/f_0000.png"));
            }
            other => panic!("png: {other:?}"),
        }
        match status(tiff) {
            ItemStatus::Done { files, first, .. } => {
                assert_eq!(files, 2);
                assert_eq!(first, dir.path().join("tif/f_0004.tif"));
                assert!(first.is_file());
            }
            other => panic!("tiff: {other:?}"),
        }
        assert_eq!(
            status(stopped),
            ItemStatus::Stopped {
                done: 0,
                total: 30,
                first: None
            }
        );
        assert!(!dir.path().join("stopped/f_0000.png").exists());
        match status(bad) {
            ItemStatus::Failed(message) => assert!(message.contains("blocker"), "{message}"),
            other => panic!("bad: {other:?}"),
        }
        let mut pngs: Vec<_> = std::fs::read_dir(dir.path().join("png"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        pngs.sort();
        assert_eq!(pngs.len(), 30);
        assert_eq!(pngs.first().map(String::as_str), Some("f_0000.png"));
    }
}
