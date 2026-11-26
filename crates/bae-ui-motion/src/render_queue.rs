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
use bae_model::{FrameRate, PrimPath, Stage, Time};
use bae_motion::query::CompInfo;
use bae_render::{
    AnimEvaluator, ExportAlpha, ExportFormat, ExportJob, ExportSettings, RenderError,
    RenderPurpose, RenderRequest, Renderer, RendererConfig, SourceProvider, export_frames,
};
use bae_ui::Accessible as _;
use bae_ui::{Panel, PanelContext, TabInfo, tr};

use crate::gpu_preview::{PreviewEffects, PreviewSources};
use crate::panel_columns::{Column, Columns};

mod filenames;
mod item_menu;
pub(crate) use filenames::add as add_to_queue;
mod templates;
use item_menu::{ItemAction, LogMode, RenderLog};
use templates::{Library, NameDialog, Span, Store, Template};

/// Formats offered in the panel and by `bae render --format`.
pub const FORMATS: &[ExportFormat] = &[
    ExportFormat::Wav,
    ExportFormat::Png,
    ExportFormat::Tiff,
    ExportFormat::Exr,
    ExportFormat::H264,
    ExportFormat::Hevc,
    #[cfg(target_os = "macos")]
    ExportFormat::QuickTimeH264,
    #[cfg(target_os = "macos")]
    ExportFormat::QuickTimeHevc,
    #[cfg(target_os = "macos")]
    ExportFormat::ProRes422,
    ExportFormat::ProRes,
    ExportFormat::ProRes4444,
];

/// AE 26's Render Settings templates in its menu order; BAE renders at Best
/// Settings, the others are shown greyed.
const RS_TEMPLATES: [&str; 5] = [
    "Best Settings",
    "DV Settings",
    "Multi-Machine Settings",
    "Current Settings",
    "Draft Settings",
];

/// What BAE writes for an Output Module template: format, channels, video
/// bit rate.
type OmSettings = (ExportFormat, Channels, f64);

/// AE 26's Output Module templates in its menu order and what BAE writes for
/// them (`None`: greyed, a format BAE does not write).
const OM_TEMPLATES: [(&str, Option<OmSettings>); 12] = [
    ("Multi-Machine Sequence", None),
    (
        "TIFF Sequence with Alpha",
        Some((
            ExportFormat::Tiff,
            Channels::RgbAlpha,
            DEFAULT_BIT_RATE_MBPS,
        )),
    ),
    ("AIFF 48kHz", None),
    (
        "H.264 - Match Render Settings - 40 Mbps",
        Some((ExportFormat::H264, Channels::Rgb, 40.0)),
    ),
    (
        "H.264 - Match Render Settings - 15 Mbps",
        Some((ExportFormat::H264, Channels::Rgb, 15.0)),
    ),
    (
        "H.264 - Match Render Settings - 5 Mbps",
        Some((ExportFormat::H264, Channels::Rgb, 5.0)),
    ),
    ("Photoshop", None),
    (
        "High Quality with Alpha",
        Some((
            ExportFormat::ProRes4444,
            Channels::RgbAlpha,
            DEFAULT_BIT_RATE_MBPS,
        )),
    ),
    ("Lossless with Alpha", None),
    ("Alpha Only", None),
    ("Lossless", None),
    (
        "High Quality",
        Some((ExportFormat::ProRes, Channels::Rgb, DEFAULT_BIT_RATE_MBPS)),
    ),
];

/// The Output Module template an item's settings match, if any.
fn om_template(item: &QueueOutput) -> Option<&'static str> {
    OM_TEMPLATES.iter().find_map(|(name, t)| {
        let (f, c, rate) = (*t)?;
        let rate_matches = !has_bit_rate(f) || (item.bit_rate_mbps - rate).abs() < 1e-6;
        (item.format == f && item.channels == c && rate_matches).then_some(*name)
    })
}

enum TemplatePick {
    Name(String),
    Custom,
    Save,
}

/// The template arrow as AE lays it out: the current template first, a
/// separator, the others in AE's order (user templates after the built-ins),
/// Custom..., a separator and Make Template.... No check marks.
fn template_menu(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    items: Vec<(&str, bool)>,
    selected: Option<&str>,
    can_save: bool,
) -> Option<TemplatePick> {
    let resp = ui
        .add(egui::Button::new("▾").frame(false).small())
        .labeled(egui::WidgetType::Button, label);
    let mut picked = None;
    egui::Popup::menu(&resp).id(id).show(|ui| {
        let current = items.iter().find(|(name, _)| Some(*name) == selected);
        let others = items.iter().filter(|(name, _)| Some(*name) != selected);
        if let Some(&(name, _)) = current {
            if ui.button(name).clicked() {
                picked = Some(TemplatePick::Name(name.to_owned()));
                ui.close();
            }
            ui.separator();
        }
        for &(name, enabled) in others {
            if ui.add_enabled(enabled, egui::Button::new(name)).clicked() {
                picked = Some(TemplatePick::Name(name.to_owned()));
                ui.close();
            }
        }
        if ui.button(tr("Custom...")).clicked() {
            picked = Some(TemplatePick::Custom);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(can_save, egui::Button::new(tr("Make Template...")))
            .clicked()
        {
            picked = Some(TemplatePick::Save);
            ui.close();
        }
    });
    picked
}

/// Formats the host can encode.
pub fn format_available(format: ExportFormat) -> bool {
    #[cfg(windows)]
    match format {
        ExportFormat::H264 => {
            return bae_media::media_foundation::encoder_available(bae_media::VideoCodec::H264);
        }
        ExportFormat::Hevc => {
            return bae_media::media_foundation::encoder_available(bae_media::VideoCodec::Hevc);
        }
        _ => {}
    }
    format == ExportFormat::Wav || format.is_image() || cfg!(target_os = "macos")
}

#[cfg(windows)]
static MOVIE_ENCODERS: std::sync::OnceLock<[bool; 2]> = std::sync::OnceLock::new();

/// The menu never waits for Media Foundation enumeration on the UI thread.
pub(crate) fn ui_format_available(format: ExportFormat) -> bool {
    #[cfg(windows)]
    if matches!(format, ExportFormat::H264 | ExportFormat::Hevc) {
        return MOVIE_ENCODERS
            .get()
            .is_some_and(|m| m[usize::from(format == ExportFormat::Hevc)]);
    }
    format_available(format)
}

/// The output's channels (AE output module "Channels").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    /// Color over the comp background, no alpha.
    Rgb,
    /// Color and straight alpha (EXR: premultiplied, as EXR requires).
    RgbAlpha,
}

/// Whether `format` can carry an alpha channel.
pub fn supports_alpha(format: ExportFormat) -> bool {
    match format {
        ExportFormat::QuickTimeHevc | ExportFormat::Hevc => {
            bae_media::VideoCodec::Hevc.supports_alpha()
        }
        _ => !matches!(
            format,
            ExportFormat::Wav
                | ExportFormat::H264
                | ExportFormat::QuickTimeH264
                | ExportFormat::ProRes422
                | ExportFormat::ProRes
        ),
    }
}

/// The channels a new output module of `format` starts with: RGB, as AE's
/// output module (transparent areas flattened over the comp background,
/// black by default, #225); ProRes 4444 RGB + Alpha, as AE's "High Quality
/// with Alpha" template.
pub fn default_channels(format: ExportFormat) -> Channels {
    match format {
        ExportFormat::ProRes4444 => Channels::RgbAlpha,
        _ => Channels::Rgb,
    }
}

/// Target bit rate of a new H.264 / HEVC output module, in Mbps (AE's
/// default "H.264 - Match Render Settings - 40 Mbps").
pub const DEFAULT_BIT_RATE_MBPS: f64 = 40.0;

/// Whether `format` has a target bit rate option.
pub fn has_bit_rate(format: ExportFormat) -> bool {
    matches!(
        format,
        ExportFormat::H264
            | ExportFormat::Hevc
            | ExportFormat::QuickTimeH264
            | ExportFormat::QuickTimeHevc
    )
}

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
    /// All destinations, in the order shown under this render item.
    pub outputs: Vec<QueueOutput>,
    log: LogMode,
    log_file: Option<PathBuf>,
    render_template: Option<String>,
    /// First and last frame (comp frames, inclusive).
    pub start: i64,
    /// Last frame (inclusive).
    pub end: i64,
    /// Current state.
    pub status: ItemStatus,
    /// The Render column's check box: unchecked items stay in the queue but
    /// are not rendered (AE: Unqueued).
    pub render: bool,
    pub comment: String,
    pub notify: bool,
    pub label: usize,
    /// When its render started (AE's Started column).
    pub started: Option<std::time::SystemTime>,
    cancel: Arc<AtomicBool>,
}

impl QueueItem {
    /// Frames in the range.
    pub fn frame_count(&self) -> usize {
        (self.end - self.start + 1).max(0) as usize
    }
}

#[derive(Clone, Debug)]
pub struct QueueOutput {
    naming: filenames::Naming,
    template: Option<String>,
    /// Output format.
    pub format: ExportFormat,
    /// Output file; for sequences a pattern (`name_[####].png`).
    pub output: String,
    /// Channels written.
    pub channels: Channels,
    /// H.264 / HEVC target bit rate in Mbps.
    pub bit_rate_mbps: f64,
    /// QuickTime PCM audio, or AAC when unchecked.
    pub pcm_audio: bool,
}

impl QueueOutput {
    /// Switches the format, the output's extension and its frame numbering
    /// with it (a movie is one file, a sequence numbers its files), and the
    /// channels to the format's default.
    pub fn set_format(&mut self, format: ExportFormat) {
        let from = self.format;
        self.format = format;
        self.channels = default_channels(format);
        let path = Path::new(&self.output);
        if !path.extension().is_some_and(|e| {
            ExportFormat::parse(&e.to_string_lossy().to_ascii_lowercase()).is_some()
        }) {
            return;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = match (from.is_image(), format.is_image()) {
            (true, false) => without_frame_run(&stem),
            (false, true) if !stem.contains('#') => format!("{stem}_[####]"),
            _ => stem,
        };
        self.output = path
            .with_file_name(format!("{stem}.{}", format.extension()))
            .to_string_lossy()
            .into_owned();
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
    sequence_files_per_folder: Option<usize>,
    render_threads: Option<usize>,
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

    /// Apply the output preference to the item just added from the workbench.
    pub(super) fn assign_output(&mut self, automatic: bool) {
        let Some((item, previous)) = self.items.split_last_mut() else {
            return;
        };
        let Some(module) = item.outputs.first_mut() else {
            return;
        };
        if !automatic {
            module.output.clear();
            return;
        }
        let Some(last) = previous
            .iter()
            .rev()
            .flat_map(|i| i.outputs.first())
            .find(|o| !o.output.trim().is_empty())
        else {
            return;
        };
        let last_path = Path::new(&last.output);
        let dir = if last.output.ends_with(['/', '\\']) {
            last_path
        } else {
            last_path.parent().unwrap_or(Path::new(""))
        };
        let current = Path::new(&module.output);
        let filename = current.file_name().unwrap_or_default().to_string_lossy();
        let split = filename
            .find('#')
            .map(|i| filename[..i].trim_end_matches(['[', '_']).len())
            .unwrap_or_else(|| {
                current
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .len()
            });
        let (prefix, suffix) = filename.split_at(split);
        let mut candidate = dir.join(filename.as_ref());
        let mut copy = 1;
        while previous
            .iter()
            .flat_map(|i| &i.outputs)
            .any(|o| Path::new(&o.output) == candidate)
        {
            candidate = dir.join(format!("{prefix}_{copy}{suffix}"));
            copy += 1;
        }
        module.output = candidate.to_string_lossy().into_owned();
    }

    /// Whether the worker is rendering.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Queues `comp` over its work area, using H.264 on macOS and PNG elsewhere, in a folder
    /// named after the comp under `output_dir`. Returns the item id.
    pub fn add(&mut self, comp: &CompInfo, output_dir: &Path) -> u64 {
        let format = if cfg!(target_os = "macos") {
            ExportFormat::H264
        } else {
            ExportFormat::Png
        };
        self.add_with_format(comp, output_dir, format)
    }

    fn add_with_format(&mut self, comp: &CompInfo, output_dir: &Path, format: ExportFormat) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let name = sanitize(&comp.name);
        let output = output_dir
            .join(&name)
            .join(default_file_name(&name, format));
        let rate = comp.frame_rate;
        let start = rate.time_to_frame(comp.work_area.start);
        // The work area's end is exclusive.
        let end = (rate.time_to_frame(comp.work_area.end) - 1).max(start);
        self.items.push(QueueItem {
            id,
            comp: comp.path.clone(),
            comp_name: comp.name.clone(),
            rate,
            render_template: None,
            outputs: vec![QueueOutput {
                naming: filenames::Naming::in_comp_folder(output_dir),
                template: None,
                format,
                output: output.to_string_lossy().into_owned(),
                channels: default_channels(format),
                bit_rate_mbps: DEFAULT_BIT_RATE_MBPS,
                pcm_audio: true,
            }],
            log: LogMode::Errors,
            log_file: None,
            start,
            end,
            status: ItemStatus::Queued,
            render: true,
            comment: String::new(),
            notify: false,
            label: 0,
            started: None,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        id
    }

    /// Composition ▸ Save Frame As ▸ File… (Cmd/Ctrl+Alt+S): `comp`'s frame
    /// `frame` as one still, named like AE's ("Comp 1 (0;00;01;00).png").
    pub fn add_frame(&mut self, comp: &CompInfo, output_dir: &Path, frame: i64) -> u64 {
        let id = self.add_with_format(comp, output_dir, ExportFormat::Png);
        let base = comp.frame_rate.as_f64().round().max(1.0) as i64;
        let f = frame.max(0);
        let (secs, ff) = (f / base, f % base);
        let code = format!(
            "{};{:02};{:02};{:02}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60,
            ff
        );
        if let Some(item) = self.item_mut(id) {
            let format = ExportFormat::Png;
            item.outputs[0].output = output_dir
                .join(format!(
                    "{} ({code}).{}",
                    sanitize(&comp.name),
                    format.extension()
                ))
                .to_string_lossy()
                .into_owned();
            item.outputs[0].naming.pattern = None;
            item.start = f;
            item.end = f;
        }
        id
    }

    /// Removes an item that is not rendering.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items
            .retain(|i| i.id != id || matches!(i.status, ItemStatus::Rendering { .. }));
        self.items.len() != before
    }

    /// Empties the queue for another project (AE: the render queue belongs to
    /// the project). A running render is stopped; the worker finishes the
    /// frame it is on and finds nothing left.
    pub fn clear(&mut self) {
        for item in &self.items {
            item.cancel.store(true, Ordering::Relaxed);
        }
        self.items.clear();
        self.batch.clear();
        if let Some(ctx) = &self.repaint {
            ctx.request_repaint();
        }
    }

    fn duplicate(&mut self, id: u64, keep_names: bool) {
        let Some(index) = self.items.iter().position(|it| it.id == id) else {
            return;
        };
        let mut copy = self.items[index].clone();
        copy.id = self.next_id;
        self.next_id += 1;
        copy.render = true;
        copy.started = None;
        copy.log_file = None;
        copy.cancel = Arc::default();
        copy.status = ItemStatus::Queued;
        if !keep_names {
            let mut used = self.output_names();
            for out in &mut copy.outputs {
                out.output = unique_name(&out.output, &used);
                out.naming.pattern = None;
                used.insert(output_key(Path::new(&out.output)));
            }
        }
        self.items.insert(index + 1, copy);
    }

    fn output_names(&self) -> std::collections::HashSet<String> {
        self.items
            .iter()
            .flat_map(|i| i.outputs.iter().map(|o| output_key(Path::new(&o.output))))
            .collect()
    }

    fn add_output(&mut self, id: u64) {
        let used = self.output_names();
        if let Some(it) = self.item_mut(id)
            && !matches!(it.status, ItemStatus::Rendering { .. })
        {
            let mut module = it.outputs[0].clone();
            module.output = unique_name(&module.output, &used);
            module.naming.pattern = None;
            it.outputs.push(module);
        }
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
            .filter(|i| i.status == ItemStatus::Queued && i.render)
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
            .filter(|i| i.status == ItemStatus::Queued && i.render)
            .map(|i| i.id)
            .collect();
        true
    }

    /// Takes the next item of the running render. `comp_frames` gives a
    /// comp's frame count: the progress counts the frames that will render
    /// (the range limited to the comp) from the start (issue #157).
    fn take_next(&mut self, comp_frames: &dyn Fn(&PrimPath) -> Option<i64>) -> Option<Job> {
        let batch = &mut self.batch;
        let item = self
            .items
            .iter_mut()
            .find(|i| i.status == ItemStatus::Queued && batch.contains(&i.id))?;
        batch.retain(|id| *id != item.id);
        // A missing comp or a range outside it renders nothing (and fails).
        let total = comp_frames(&item.comp)
            .and_then(|frames| limited_range(item.start, item.end, frames))
            .map_or(0, |(start, end)| (end - start + 1) as usize);
        let total = total * item.outputs.len();
        item.status = ItemStatus::Rendering { done: 0, total };
        item.started = Some(std::time::SystemTime::now());
        Some(Job {
            id: item.id,
            comp: item.comp.clone(),
            rate: item.rate,
            outputs: item.outputs.clone(),
            log: item.log,
            sequence_files_per_folder: self.sequence_files_per_folder,
            render_threads: self.render_threads,
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
    output_dir_from(
        bae_platform::dirs::videos_dir(),
        bae_platform::dirs::home_dir(),
    )
}

/// The user's videos folder (Windows: the Videos known folder, even when the
/// user moved it, #216; macOS: ~/Movies), else the home folder.
fn output_dir_from(videos: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    videos.or(home).unwrap_or_else(|| PathBuf::from("."))
}

/// Keep a sequence's trailing frame token when assigning a unique destination.
fn output_key(path: &Path) -> String {
    let name = path.to_string_lossy();
    if cfg!(windows) {
        name.replace('/', "\\").to_lowercase()
    } else {
        name.into_owned()
    }
}

fn unique_name(output: &str, used: &std::collections::HashSet<String>) -> String {
    if output.is_empty() {
        return String::new();
    }
    let path = Path::new(output);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let before_number = stem.find('#').map_or(stem.as_ref(), |start| {
        stem[..start]
            .trim_end_matches('[')
            .trim_end_matches(['_', '-', ' ', '.'])
    });
    let (prefix, suffix) = stem.split_at(before_number.len());
    for i in 1.. {
        let mut name = format!("{prefix}_{i}{suffix}");
        if let Some(ext) = path.extension() {
            name.push('.');
            name.push_str(&ext.to_string_lossy());
        }
        let candidate = path.with_file_name(name).to_string_lossy().into_owned();
        if !used.contains(&output_key(Path::new(&candidate))) && !Path::new(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
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
    outputs: Vec<QueueOutput>,
    log: LogMode,
    sequence_files_per_folder: Option<usize>,
    render_threads: Option<usize>,
    start: i64,
    end: i64,
    cancel: Arc<AtomicBool>,
}

fn render_threads(prefs: &bae_prefs::Preferences, cores: usize) -> usize {
    if !prefs.bool("memory_cpu.enable_multi_frame_rendering") {
        return 1;
    }
    let reserved = prefs
        .int("memory_cpu.reserved_cpu_power_percent")
        .clamp(0, 90) as usize;
    (cores.saturating_mul(100 - reserved) / 100).max(1)
}

/// The export settings of a job's output module; `threads` from Memory &
/// CPU (multi-frame rendering).
fn export_settings(job: &QueueOutput, output: PathBuf, threads: Option<usize>) -> ExportSettings {
    let mut settings = ExportSettings::new(job.format, output);
    if let Some(threads) = threads {
        settings.threads = Some(threads);
        settings.frames_in_flight = threads.min(bae_render::export::READBACK_DEPTH);
    }
    settings.pcm_audio = job.pcm_audio && job.format.is_quicktime();
    settings.alpha = match job.channels {
        Channels::RgbAlpha if supports_alpha(job.format) => ExportAlpha::Straight,
        _ => ExportAlpha::Background,
    };
    if has_bit_rate(job.format) {
        settings.bit_rate = Some((job.bit_rate_mbps.clamp(0.1, 800.0) * 1e6) as u64);
    }
    settings
}

/// A movie's file name stem from a sequence pattern's: the frame number run
/// (`_[####]`, `####`) goes.
fn without_frame_run(stem: &str) -> String {
    let Some(start) = stem.find('#') else {
        return stem.to_owned();
    };
    let end = start + stem[start..].chars().take_while(|&c| c == '#').count();
    let (mut prefix, mut suffix) = (&stem[..start], &stem[end..]);
    if prefix.ends_with('[') && suffix.starts_with(']') {
        prefix = &prefix[..prefix.len() - 1];
        suffix = &suffix[1..];
    }
    let prefix = if suffix.is_empty() {
        prefix.trim_end_matches(['_', '-', ' ', '.'])
    } else {
        prefix
    };
    let joined = format!("{prefix}{suffix}");
    if joined.is_empty() {
        "output".into()
    } else {
        joined
    }
}

/// The file an output folder gets: `name_[####].png` for a sequence,
/// `name.mp4` for a movie.
fn default_file_name(comp: &str, format: ExportFormat) -> String {
    let name = sanitize(comp);
    if format.is_image() {
        format!("{name}_[####].{}", format.extension())
    } else {
        format!("{name}.{}", format.extension())
    }
}

/// The part of `start..=end` a comp of `frames` frames renders: AE limits the
/// range to the comp. `None` when nothing of it is inside.
pub(crate) fn limited_range(start: i64, end: i64, frames: i64) -> Option<(i64, i64)> {
    let (start, end) = (start.max(0), end.min(frames - 1));
    (start <= end).then_some((start, end))
}

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
    // AE limits the range to the comp.
    let Some((start, end)) = limited_range(job.start, job.end, info.frame_count()) else {
        return Err(RenderError::Other(
            tr("The frames are outside the composition (0–{last}).")
                .replace("{last}", &(info.frame_count() - 1).to_string()),
        ));
    };
    let times: Vec<(Option<i64>, Time)> = (start..=end)
        .map(|f| (Some(f), job.rate.frame_to_time(f)))
        .collect();
    // The progress counts the frames actually rendered (the limited range).
    let total = times.len();
    // A `#` run in the name numbers the frames, one frame too (#207);
    // export_frames decides, like `bae render`.
    let request = RenderRequest {
        purpose: RenderPurpose::Final,
        ..RenderRequest::default()
    };
    let destinations = job
        .outputs
        .iter()
        .map(|o| {
            let path = output_path(Path::new(&o.output), o.format, &info.name)?;
            let settings = export_settings(o, path.clone(), job.render_threads);
            settings.validate(job.rate, &request)?;
            Ok((path, settings))
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    let mut names = std::collections::HashSet::new();
    if destinations
        .iter()
        .any(|(p, _)| !names.insert(output_key(p)))
    {
        return Err(RenderError::Other(
            tr("Output modules must use different files.").to_owned(),
        ));
    }
    let total = total * destinations.len();
    let mut first = None;
    let mut done = 0;
    let mut files = 0;
    progress(0, total);
    let per_output = times.len();
    for (path, mut settings) in destinations {
        // Preferences > Output: sequences split into folders of N files.
        let segment = job
            .sequence_files_per_folder
            .filter(|limit| settings.format.is_image() && *limit < per_output);
        let segment_name = segment.map(|_| {
            let pattern = bae_render::export::explicit_pattern(&path);
            let stem =
                without_frame_run(&pattern.file_stem().unwrap_or_default().to_string_lossy());
            (pattern, stem)
        });
        for (index, chunk) in times.chunks(segment.unwrap_or(per_output)).enumerate() {
            if let Some((pattern, stem)) = &segment_name {
                settings.path = pattern
                    .with_file_name(format!("{stem}_{:04}", index + 1))
                    .join(pattern.file_name().unwrap());
            }
            let report = export_frames(
                renderer,
                evaluator,
                &ExportJob {
                    stage,
                    comp: &job.comp,
                    times: chunk,
                    rate: job.rate,
                    request: &request,
                    settings: &settings,
                    encoders: None,
                },
                Some(&job.cancel),
                &mut |frame| progress(done + frame.index + 1, total),
            )?;
            if first.is_none() {
                first = report.frames.first().map(|f| f.file.clone());
            }
            done += report.frames.len();
            files += report.stats.files;
            if report.stopped {
                return Ok(ItemStatus::Stopped { done, total, first });
            }
        }
    }
    Ok(ItemStatus::Done {
        files,
        first: first.unwrap_or_default(),
        seconds: started.elapsed().as_secs_f64(),
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
        return Ok(output.join(default_file_name(comp, format)));
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
    (typed.extension() != format.extension()).then(|| format.extension())
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
    let effects = resources.get::<PreviewEffects>().map(|e| e.0.clone());
    {
        let mut queue = queue.lock();
        if !queue.begin() {
            return Ok(());
        }
        let prefs = crate::prefs::current(resources);
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        queue.render_threads = Some(render_threads(&prefs, cores));
        queue.sequence_files_per_folder = prefs
            .bool("export.segment_sequences_enabled")
            .then(|| prefs.int("export.segment_sequences_files").clamp(1, 99999) as usize);
    }
    let snapshot = crate::preview_snapshot::PreviewSnapshot::take(stage);
    let queue = queue.clone();
    std::thread::Builder::new()
        .name("bae-render-queue".into())
        .spawn(move || run_worker(&queue, &snapshot, gpu, &providers, effects.as_deref()))
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
    snapshot: &crate::preview_snapshot::PreviewSnapshot,
    gpu: Arc<GpuContext>,
    providers: &[Arc<dyn SourceProvider>],
    effects: Option<&Registry<dyn bae_render::Effect>>,
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
    if let Some(effects) = effects {
        renderer.sync_effects(effects);
    }
    let mut evaluator = AnimEvaluator::new(None);
    let comp_frames = |comp: &PrimPath| {
        let stage = stage.as_ref().ok()?;
        bae_motion::query::comp_info(stage, comp).map(|c| c.frame_count())
    };
    loop {
        let Some(job) = queue.lock().take_next(&comp_frames) else {
            break;
        };
        let mut log = RenderLog::new(&job);
        let status = match &stage {
            Err(error) => ItemStatus::Failed(error.clone()),
            Ok(stage) => {
                let mut progress = |done: usize, total: usize| {
                    log.progress(&job, done, total);
                    let mut q = queue.lock();
                    q.set_status(job.id, ItemStatus::Rendering { done, total });
                };
                match render_job(&mut renderer, &mut evaluator, stage, &job, &mut progress) {
                    Ok(status) => status,
                    Err(error) => ItemStatus::Failed(error.to_string()),
                }
            }
        };
        let log_file = log.finish(&job, &status);
        let mut q = queue.lock();
        if let Some(it) = q.item_mut(job.id) {
            it.log_file = log_file;
        }
        if q.items()
            .iter()
            .any(|item| item.id == job.id && item.notify)
            && let Some(ctx) = &q.repaint
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
        q.set_status(job.id, status);
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

const COLUMNS: &[Column] = &[
    Column::new("Render", 54.0, true, false),
    Column::new("Label", 37.0, true, false),
    Column::new("#", 28.0, true, false),
    Column::new("Comp Name", 130.0, true, true),
    Column::new("Status", 104.0, true, false),
    Column::new("Started", 90.0, true, false),
    Column::new("Render Time", 82.0, true, false),
    Column::new("Comment", 132.0, false, false),
    Column::new("Notify", 56.0, false, false),
    Column::new("Comp Size", 90.0, false, false),
    Column::new("Frames", 54.0, false, false),
    Column::new("Duration", 88.0, false, false),
    Column::new("Estimated Time", 100.0, false, false),
];

/// The Render Queue panel.
#[derive(Debug, Default)]
pub struct RenderQueuePanel {
    columns: Columns,
    error: Option<String>,
    naming: Option<(u64, Option<usize>, NameDialog)>,
    file_editor: Option<filenames::Editor>,
}

impl Panel for RenderQueuePanel {
    fn on_action(&mut self, inv: &bae_ui::Invocation, cx: &mut PanelContext<'_>) -> bool {
        if let Some(id) = inv.params["queue_item"].as_u64() {
            let queue = shared(cx.resources);
            let mut queue = queue.lock();
            let Some(item) = queue.item_mut(id) else {
                return true;
            };
            let action = match inv.id.as_str() {
                crate::actions::ids::DUPLICATE
                    if inv.params["keep_names"].as_bool() == Some(true) =>
                {
                    ItemAction::DuplicateWithName
                }
                crate::actions::ids::DUPLICATE => ItemAction::Duplicate,
                "motion.comp.add_output_module" => ItemAction::AddOutput,
                "motion.layer.reveal_composition_in_project" => ItemAction::Reveal,
                _ => return false,
            };
            if action.enabled(
                matches!(item.status, ItemStatus::Rendering { .. }),
                cx.stage.has_prim(&item.comp),
            ) {
                match action {
                    ItemAction::Duplicate | ItemAction::DuplicateWithName => {
                        queue.duplicate(id, action == ItemAction::DuplicateWithName)
                    }
                    ItemAction::AddOutput => queue.add_output(id),
                    ItemAction::Reveal => {
                        crate::common::session(cx.resources).lock().selected_items =
                            vec![item.comp.clone()];
                        cx.open_panel("project");
                    }
                }
            }
            return true;
        }
        if inv.id != CHOOSE_OUTPUT {
            return false;
        }
        let (Some(id), Some(index)) = (
            inv.params["item"].as_u64(),
            inv.params["module"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok()),
        ) else {
            return true;
        };
        let queue = shared(cx.resources);
        let request = queue
            .lock()
            .item_mut(id)
            .and_then(|i| i.outputs.get(index))
            .map(|module| {
                let initial = Path::new(&module.output);
                bae_ui::files::SaveRequest {
                    title: tr("Output Movie To").to_owned(),
                    filters: vec![(
                        tr(format_label(module.format)).to_owned(),
                        vec![module.format.extension().to_owned()],
                    )],
                    directory: initial
                        .parent()
                        .filter(|dir| !dir.as_os_str().is_empty())
                        .map(Path::to_owned),
                    file_name: initial
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                }
            });
        if let Some(request) = request
            && let Some(picker) = cx.resources.get::<bae_ui::files::SharedFilePicker>()
            && let Some(path) = picker.0.save(&request)
            && let Some(module) = queue
                .lock()
                .item_mut(id)
                .and_then(|i| i.outputs.get_mut(index))
        {
            module.output = path.to_string_lossy().into_owned();
            module.naming = Default::default();
        }
        true
    }

    fn menu(&mut self, ui: &mut egui::Ui, _cx: &mut PanelContext<'_>) {
        self.columns.menu(ui, COLUMNS);
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.columns).ok()
    }

    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Render Queue"))
    }

    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        // Inset from the panel's edges like the other panels (#228).
        let pad = bae_ui::theme::metrics::PANEL_PADDING as i8;
        egui::Frame::new()
            .inner_margin(egui::Margin::same(pad))
            .show(ui, |ui| self.contents(ui, cx));
    }
}

impl RenderQueuePanel {
    fn contents(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        ui.spacing_mut().item_spacing.y = 2.0;
        let queue = shared(cx.resources);
        let (running, queued) = {
            let mut q = queue.lock();
            q.repaint = Some(ui.ctx().clone());
            (q.running, q.queued())
        };
        let store = cx.resources.get::<Arc<Store>>();
        let library = store.and_then(|s| s.snapshot());
        if let (Some(editor), Some(store)) = (&mut self.file_editor, store)
            && editor.show(ui, cx, store, &queue)
        {
            self.file_editor = None;
        }
        let project = cx
            .resources
            .get::<bae_ui::files::ProjectFile>()
            .and_then(|p| p.get());
        if let Some((id, module, dialog)) = &mut self.naming
            && let Some(store) = store
            && dialog.show(ui, store)
        {
            if dialog.saved
                && let Some(item) = queue.lock().item_mut(*id)
            {
                let name = Some(dialog.name.trim().to_owned());
                if let Some(index) = module {
                    if let Some(output) = item.outputs.get_mut(*index) {
                        output.template = name;
                    }
                } else {
                    item.render_template = name;
                }
            }
            self.naming = None;
        }
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
        let mut open = None;
        let mut q = queue.lock();
        let panel_width = ui.available_width();
        egui::ScrollArea::horizontal()
            .id_salt("queue_columns")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_min_width(panel_width);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for col in COLUMNS {
                        if !self.columns.visible(col) {
                            continue;
                        }
                        let resp = ui.add_sized(
                            [col.width, 19.0],
                            egui::Label::new(tr(col.title)).sense(egui::Sense::click()),
                        );
                        crate::common::context_menu(&resp, &mut |ui| {
                            self.columns.header_menu(ui, COLUMNS, col)
                        });
                    }
                });
                if q.items.is_empty() {
                    ui.weak(tr(
                        "Add a composition with Composition > Add to Render Queue.",
                    ));
                    return;
                }
                let mut remove = None;
                let mut cancel = None;
                let mut requeue = None;
                let mut command = None;
                // No scroll area of its own: the dock scrolls a panel that does not fit
                // (AE opens the Render Queue in the short Timeline frame).
                for (i, item) in q.items.iter_mut().enumerate() {
                    let comp = bae_motion::query::comp_info(cx.stage, &item.comp);
                    ui.push_id(item.id, |ui| {
                        item_ui(
                            ui,
                            i + 1,
                            item,
                            comp.as_ref(),
                            &self.columns,
                            library.as_deref(),
                            project.as_deref(),
                            &mut remove,
                            &mut cancel,
                            &mut requeue,
                            &mut command,
                            &mut open,
                        );
                    });
                    ui.separator();
                }
                if let Some(id) = remove {
                    q.remove(id);
                }
                if let Some(id) = cancel {
                    q.cancel(id);
                }
                if let Some(id) = requeue {
                    q.requeue(id);
                }
                if let Some((id, action)) = command {
                    match action {
                        // On the queue itself: the global Duplicate is routed by
                        // focus and could duplicate the selected layers instead.
                        ItemAction::Duplicate | ItemAction::DuplicateWithName => {
                            q.duplicate(id, action == ItemAction::DuplicateWithName)
                        }
                        _ => cx.invoke_with(
                            action.action_id(),
                            serde_json::json!({ "queue_item": id }),
                        ),
                    }
                }
            });
        if q.running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        drop(q);
        // The item's settings links (the dialogs read the queue themselves).
        match open {
            Some((id, QueueLink::RenderSettings)) => crate::dialogs::render_settings(cx, id),
            Some((id, QueueLink::Output(module))) => crate::dialogs::output_module(cx, id, module),
            Some((_, QueueLink::NameFile(editor))) => self.file_editor = Some(editor),
            Some((_, QueueLink::DefaultNaming(pattern))) => {
                if let Some(store) = store
                    && let Err(e) = store.save("", &Template::DefaultFilename(pattern))
                {
                    self.error = Some(e);
                }
            }
            Some((id, QueueLink::Make(module, template))) => {
                self.naming = Some((id, module, NameDialog::new(template)));
            }
            Some((id, QueueLink::File(module))) => cx.invoke_with(
                CHOOSE_OUTPUT,
                serde_json::json!({"item": id, "module":module}),
            ),
            None => {}
        }
    }
}

enum QueueLink {
    RenderSettings,
    Output(usize),
    File(usize),
    Make(Option<usize>, Template),
    NameFile(filenames::Editor),
    DefaultNaming(String),
}

const CHOOSE_OUTPUT: &str = "motion.render_queue.choose_output";

#[allow(clippy::too_many_arguments)]
fn item_ui(
    ui: &mut egui::Ui,
    number: usize,
    item: &mut QueueItem,
    comp: Option<&CompInfo>,
    columns: &Columns,
    library: Option<&Library>,
    project: Option<&Path>,
    remove: &mut Option<u64>,
    cancel: &mut Option<u64>,
    requeue: &mut Option<u64>,
    command: &mut Option<(u64, ItemAction)>,
    open: &mut Option<(u64, QueueLink)>,
) {
    let last = comp.map(|c| c.frame_count() - 1);
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
    // AE's columns, the row a right click opens the item menu on.
    let row = ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for col in COLUMNS.iter().filter(|col| columns.visible(col)) {
                ui.allocate_ui_with_layout(
                    egui::vec2(col.width, 20.0),
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        let text = match col.title {
                            "Render" => {
                                ui.add_enabled_ui(
                                    !matches!(item.status, ItemStatus::Rendering { .. }),
                                    |ui| {
                                        let mut render = item.render;
                                        if bae_ui::widgets::checkbox(ui, &mut render, "")
                                            .labeled_toggle(
                                                egui::WidgetType::Checkbox,
                                                &name("Render"),
                                                render,
                                            )
                                            .changed()
                                        {
                                            item.render = render;
                                        }
                                    },
                                );
                                return;
                            }
                            "Label" => {
                                let (r, resp) = ui.allocate_exact_size(
                                    egui::vec2(15.0, 15.0),
                                    egui::Sense::click(),
                                );
                                bae_ui::icons::swatch(
                                    ui.painter(),
                                    r.center(),
                                    10.0,
                                    bae_ui::theme::label_color(item.label),
                                );
                                egui::Popup::menu(&resp).show(|ui| {
                                    for i in 0..bae_ui::theme::LABELS.len() {
                                        if ui
                                            .selectable_value(
                                                &mut item.label,
                                                i,
                                                bae_ui::theme::label_name(i),
                                            )
                                            .clicked()
                                        {
                                            ui.close();
                                        }
                                    }
                                });
                                return;
                            }
                            "#" => number.to_string(),
                            "Comp Name" => item.comp_name.clone(),
                            "Status" => match &item.status {
                                ItemStatus::Queued => {
                                    tr(if item.render { "Queued" } else { "Unqueued" }).to_owned()
                                }
                                ItemStatus::Rendering { done, total } => {
                                    format!("{} {done}/{total}", tr("Rendering"))
                                }
                                ItemStatus::Done { .. } => tr("Done").to_owned(),
                                ItemStatus::Failed(_) => tr("Failed").to_owned(),
                                ItemStatus::Stopped { done, total, .. } => {
                                    tr("Stopped ({done}/{total} frames)")
                                        .replace("{done}", &done.to_string())
                                        .replace("{total}", &total.to_string())
                                }
                            },
                            "Started" => item
                                .started
                                .map(bae_platform::utc_label)
                                .unwrap_or_default(),
                            "Render Time" => {
                                let secs = match item.status {
                                    ItemStatus::Done { seconds, .. } => Some(seconds),
                                    ItemStatus::Rendering { .. } => item
                                        .started
                                        .and_then(|t| t.elapsed().ok())
                                        .map(|t| t.as_secs_f64()),
                                    _ => None,
                                };
                                secs.map(|s| format!("{s:.1} s")).unwrap_or_default()
                            }
                            "Estimated Time" => match item.status {
                                ItemStatus::Rendering { done, total } if done > 0 => item
                                    .started
                                    .and_then(|t| t.elapsed().ok())
                                    .map(|t| {
                                        format!(
                                            "{:.1} s",
                                            t.as_secs_f64() * (total - done) as f64 / done as f64
                                        )
                                    })
                                    .unwrap_or_default(),
                                _ => String::new(),
                            },
                            "Comment" => {
                                ui.add(
                                    egui::TextEdit::singleline(&mut item.comment)
                                        .desired_width(col.width),
                                );
                                return;
                            }
                            "Notify" => {
                                ui.checkbox(&mut item.notify, "");
                                return;
                            }
                            "Comp Size" => comp
                                .map(|c| format!("{} × {}", c.width, c.height))
                                .unwrap_or_default(),
                            "Frames" => item.frame_count().to_string(),
                            "Duration" => bae_ui::widgets::timecode(
                                item.frame_count() as i64,
                                item.rate.as_f64(),
                            ),
                            _ => unreachable!(),
                        };
                        ui.add(egui::Label::new(text).truncate());
                    },
                );
            }
        });
    });
    crate::common::context_menu(&row.response, &mut |ui| {
        if let Some(action) = ItemAction::menu(
            ui,
            matches!(item.status, ItemStatus::Rendering { .. }),
            last.is_some(),
        ) {
            *command = Some((item.id, action));
        }
    });
    if let ItemStatus::Failed(message) = &item.status {
        ui.colored_label(ui.visuals().error_fg_color, message);
    }
    ui.horizontal_wrapped(|ui| {
        match &item.status {
            ItemStatus::Queued | ItemStatus::Failed(_) => {}
            ItemStatus::Rendering { .. } => {
                if ui
                    .button(tr("Stop"))
                    .labeled(egui::WidgetType::Button, &name("Stop"))
                    .clicked()
                {
                    *cancel = Some(item.id);
                }
            }
            ItemStatus::Done { first, .. } => {
                if ui
                    .button(tr(reveal_label()))
                    .labeled(egui::WidgetType::Button, &name(reveal_label()))
                    .clicked()
                {
                    reveal(first);
                }
            }
            ItemStatus::Stopped { first, .. } => {
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
    // Render Settings and Log (AE's templates; BAE renders at Best Settings).
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal_wrapped(|ui| {
            let span = comp.map(|comp| Span::capture(item, comp));
            let mut rs: Vec<_> = RS_TEMPLATES
                .iter()
                .map(|t| (tr(t), *t == "Best Settings" && comp.is_some()))
                .collect();
            if let Some(lib) = library {
                rs.extend(lib.render.keys().map(|n| (n.as_str(), comp.is_some())));
            }
            let selected = item
                .render_template
                .as_deref()
                .filter(|n| {
                    library
                        .and_then(|l| l.render.get(*n))
                        .is_some_and(|s| Some(s) == span.as_ref())
                })
                .or_else(|| (span == Some(Span::WorkArea)).then(|| tr("Best Settings")));
            let render_label = selected.unwrap_or_else(|| tr("Custom")).to_owned();
            let id = egui::Id::new(("rq-rs-templates", item.id));
            match template_menu(
                ui,
                id,
                &name("Render Settings Templates"),
                rs,
                selected,
                library.is_some() && span.is_some(),
            ) {
                Some(TemplatePick::Name(n)) => {
                    if let Some(comp) = comp {
                        let preset = library.and_then(|l| l.render.get(&n));
                        (item.start, item.end) = preset.unwrap_or(&Span::WorkArea).range(comp);
                        item.render_template = preset.map(|_| n);
                    }
                }
                Some(TemplatePick::Custom) => *open = Some((item.id, QueueLink::RenderSettings)),
                Some(TemplatePick::Save) => {
                    if let Some(s) = span {
                        *open = Some((item.id, QueueLink::Make(None, Template::Render(s))));
                    }
                }
                None => {}
            }
            ui.label(tr("Render Settings:"));
            if bae_ui::widgets::link(ui, &render_label)
                .labeled(egui::WidgetType::Link, &name("Render Settings"))
                .clicked()
            {
                *open = Some((item.id, QueueLink::RenderSettings));
            }
            ui.add_space(12.0);
            ui.label(tr("Log:"));
            let label = LogMode::ALL.iter().find(|(m, _)| *m == item.log).unwrap().1;
            bae_ui::widgets::menu_button(ui, tr(label), |ui| {
                for (mode, title) in LogMode::ALL {
                    if ui.radio_value(&mut item.log, mode, tr(title)).clicked() {
                        ui.close();
                    }
                }
            });
        });
    });
    if let Some(path) = &item.log_file
        && ui.link(path.display().to_string()).clicked()
    {
        reveal(path);
    }
    let id = item.id;
    let multiple = item.outputs.len() > 1;
    let mut remove_output = None;
    for (index, module) in item.outputs.iter_mut().enumerate() {
        ui.push_id(("output_module", index), |ui| {
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut om: Vec<_> = OM_TEMPLATES
                        .iter()
                        .map(|(n, t)| (tr(n), t.is_some_and(|(f, ..)| ui_format_available(f))))
                        .collect();
                    if let Some(lib) = library {
                        om.extend(lib.outputs.iter().map(|(name, p)| {
                            (
                                name.as_str(),
                                p.settings().is_some_and(|(f, ..)| ui_format_available(f)),
                            )
                        }));
                    }
                    let selected = module
                        .template
                        .as_deref()
                        .filter(|n| {
                            library
                                .and_then(|l| l.outputs.get(*n))
                                .and_then(|p| p.settings())
                                .is_some_and(|(f, c, b)| {
                                    module.format == f
                                        && module.channels == c
                                        && (!has_bit_rate(f)
                                            || (module.bit_rate_mbps - b).abs() < 1e-6)
                                })
                        })
                        .or_else(|| om_template(module).map(tr));
                    let output_label = selected
                        .unwrap_or_else(|| tr(format_label(module.format)))
                        .to_owned();
                    let popup_id = egui::Id::new(("rq-om-templates", id, index));
                    match template_menu(
                        ui,
                        popup_id,
                        &name("Output Module Templates"),
                        om,
                        selected,
                        library.is_some(),
                    ) {
                        Some(TemplatePick::Custom) => *open = Some((id, QueueLink::Output(index))),
                        Some(TemplatePick::Save) => {
                            *open = Some((
                                id,
                                QueueLink::Make(
                                    Some(index),
                                    Template::Output(templates::Output::capture(module)),
                                ),
                            ))
                        }
                        Some(TemplatePick::Name(name)) => {
                            let user = library.and_then(|l| l.outputs.get(&name));
                            let settings = user.and_then(|p| p.settings()).or_else(|| {
                                OM_TEMPLATES
                                    .iter()
                                    .find(|(n, _)| tr(n) == name)
                                    .and_then(|(_, s)| *s)
                            });
                            if let Some((f, c, rate)) = settings {
                                module.set_format(f);
                                module.channels = c;
                                module.bit_rate_mbps = rate;
                                module.template = user.map(|_| name);
                            }
                        }
                        None => {}
                    }
                    ui.label(tr("Output Module:"));
                    let header = bae_ui::widgets::link(ui, &output_label)
                        .labeled(egui::WidgetType::Link, &name("Output Module"));
                    if header.clicked() {
                        *open = Some((id, QueueLink::Output(index)));
                    }
                    // AE's Output Module header menu: Reveal (once the file
                    // exists) and Add Output Module.
                    crate::common::context_menu(&header, &mut |ui| {
                        let file = Path::new(&module.output);
                        if ui
                            .add_enabled(file.exists(), egui::Button::new(tr("Reveal in Explorer")))
                            .clicked()
                        {
                            bae_platform::reveal(file);
                            ui.close();
                        }
                        if ui.button(tr("Add Output Module")).clicked() {
                            *command = Some((id, ItemAction::AddOutput));
                            ui.close();
                        }
                    });
                });
            });
            ui.add_enabled_ui(editable, |ui| {
                egui::Grid::new("output").num_columns(2).show(ui, |ui| {
                    // Fields are named after the label in front of them and the item
                    // (screen readers read "… · Format: PNG", issue #128).
                    ui.label(tr("Format"));
                    let mut format = module.format;
                    // As wide as the column allows; long names are shortened rather
                    // than widening a narrow panel.
                    let combo_w = ui.available_width().clamp(60.0, 220.0);
                    egui::ComboBox::from_id_salt("format")
                        .width(combo_w)
                        .truncate()
                        .selected_text(tr(format_label(format)))
                        .show_ui(ui, |ui| {
                            for &f in FORMATS {
                                let available = ui_format_available(f);
                                let r = ui.add_enabled(
                                    available,
                                    egui::Button::selectable(format == f, tr(format_label(f))),
                                );
                                if r.clicked() {
                                    format = f;
                                }
                                if !available {
                                    let reason = match f {
                                        ExportFormat::ProRes | ExportFormat::ProRes4444 => {
                                            "ProRes export requires macOS."
                                        }
                                        ExportFormat::Hevc => {
                                            "A hardware HEVC encoder is required."
                                        }
                                        _ => "The system H.264 encoder is unavailable.",
                                    };
                                    r.on_disabled_hover_text(tr(reason));
                                }
                            }
                        })
                        .response
                        .labeled_value(
                            egui::WidgetType::ComboBox,
                            &name("Format"),
                            tr(format_label(module.format)),
                        );
                    if format != module.format {
                        module.set_format(format);
                    }
                    ui.end_row();
                    // AE: Channels RGB / RGB + Alpha (RGB + Alpha only where the
                    // codec has alpha).
                    if module.format == ExportFormat::Wav {
                        ui.label(tr("Audio"));
                        ui.label(tr("Stereo, 48 kHz, 24-bit PCM"));
                        ui.end_row();
                    } else {
                        ui.label(tr("Channels"));
                        let mut channels = module.channels;
                        egui::ComboBox::from_id_salt("channels")
                            .width(combo_w)
                            .truncate()
                            .selected_text(channels_label(channels))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut channels,
                                    Channels::Rgb,
                                    channels_label(Channels::Rgb),
                                );
                                let alpha = supports_alpha(module.format);
                                let r = ui.add_enabled(
                                    alpha,
                                    egui::Button::selectable(
                                        channels == Channels::RgbAlpha,
                                        channels_label(Channels::RgbAlpha),
                                    ),
                                );
                                if r.clicked() {
                                    channels = Channels::RgbAlpha;
                                }
                                if !alpha {
                                    r.on_disabled_hover_text(tr(
                                        "This format has no alpha channel.",
                                    ));
                                }
                            })
                            .response
                            .labeled_value(
                                egui::WidgetType::ComboBox,
                                &name("Channels"),
                                channels_label(module.channels),
                            );
                        module.channels = channels;
                        ui.end_row();
                    }
                    if has_bit_rate(module.format) {
                        // AE's H.264 format options: VBR, target bitrate.
                        ui.label(tr("Target Bitrate"));
                        let value = format!("{} Mbps", module.bit_rate_mbps);
                        ui.add(
                            egui::DragValue::new(&mut module.bit_rate_mbps)
                                .range(1.0..=800.0)
                                .speed(0.5)
                                .suffix(" Mbps"),
                        )
                        .labeled_value(
                            egui::WidgetType::DragValue,
                            &name("Target Bitrate"),
                            &value,
                        );
                        ui.end_row();
                    }
                    ui.horizontal(|ui| {
                        ui.label(tr("Output To"));
                        if let Some(comp) = comp {
                            filenames::menu(
                                ui,
                                id,
                                index,
                                module,
                                &filenames::Context {
                                    comp,
                                    start: item.start,
                                    end: item.end,
                                    project,
                                },
                                library,
                                open,
                            );
                        }
                    });
                    let target = Path::new(&module.output)
                        .file_name()
                        .map(|p| p.to_string_lossy())
                        .unwrap_or_default();
                    let label = if target.is_empty() {
                        tr("Not yet specified")
                    } else {
                        target.as_ref()
                    };
                    let file_link = bae_ui::widgets::link(ui, label).labeled_value(
                        egui::WidgetType::Link,
                        &name("Output To"),
                        &module.output,
                    );
                    // The path is the user's, not UI text to translate.
                    bae_ui::a11y::value_source(&file_link, bae_ui::a11y::VERBATIM_VALUE);
                    let follows = extension_follows_format(&module.output, module.format);
                    if let Some(extension) = follows {
                        file_link.clone().on_hover_text(
                            Path::new(&module.output)
                                .with_extension(extension)
                                .display()
                                .to_string(),
                        );
                    }
                    if file_link.clicked() {
                        *open = Some((id, QueueLink::File(index)));
                    }
                    if multiple
                        && ui
                            .small_button("−")
                            .on_hover_text(tr("Remove Output Module"))
                            .clicked()
                    {
                        remove_output = Some(index);
                    }
                    ui.end_row();
                    // AE: the format decides the extension.
                    if let Some(extension) = follows {
                        ui.label("");
                        ui.weak(
                            tr("The file will be saved as .{ext} to match the format.")
                                .replace("{ext}", extension),
                        );
                        ui.end_row();
                    }
                });
            });
        });
    }
    if let Some(i) = remove_output {
        item.outputs.remove(i);
    }
    ui.add_enabled_ui(editable, |ui| {
        egui::Grid::new("render_range")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(tr("Frames"));
                // Typed and dragged values stay inside the comp (AE); a range set
                // otherwise (or a comp shortened later) is limited, or refused when
                // entirely outside, at render time.
                let last = last.unwrap_or(i64::MAX).max(0);
                // Start – end wrap in a narrow panel instead of widening it.
                ui.horizontal_wrapped(|ui| {
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

/// The format's name in AE's output module (translated with `tr`).
pub(crate) fn format_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Png => "PNG Sequence",
        ExportFormat::Tiff => "TIFF Sequence",
        ExportFormat::Wav => "Waveform Audio (WAV)",
        ExportFormat::Exr => "OpenEXR Sequence",
        ExportFormat::H264 => "H.264",
        ExportFormat::Hevc => "HEVC",
        ExportFormat::QuickTimeH264 => "QuickTime (H.264)",
        ExportFormat::QuickTimeHevc => "QuickTime (HEVC)",
        ExportFormat::ProRes422 => "QuickTime (Apple ProRes 422)",
        ExportFormat::ProRes => "QuickTime (Apple ProRes 422 HQ)",
        ExportFormat::ProRes4444 => "QuickTime (Apple ProRes 4444)",
    }
}

/// AE's names for the channels (the same in its Chinese UI).
pub(crate) fn channels_label(channels: Channels) -> &'static str {
    match channels {
        Channels::Rgb => "RGB",
        Channels::RgbAlpha => "RGB + Alpha",
    }
}

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Duplicate (Render Item)", "复制"),
    ("Duplicate With File Name", "与文件名一起复制"),
    ("Remove Output Module", "移除输出模块"),
    ("Plus Settings", "增加设置"),
    ("Plus Per Frame Info", "增加每帧信息"),
    ("Not yet specified", "未指定"),
    ("Output Movie To", "将影片输出到"),
    (
        "Output modules must use different files.",
        "每个输出模块需要使用不同的文件。",
    ),
    ("Video Codec", "视频编码器"),
    ("24-bit PCM", "24 位 PCM"),
    ("Save Frame As File", "帧另存为文件"),
    // AE 26's Render Queue (渲染队列) terms.
    ("Unqueued", "未加入队列"),
    ("Comp Name", "合成名称"),
    ("Estimated Time", "估计时间"),
    ("Started", "已启动"),
    ("Notify", "通知"),
    ("Status", "状态"),
    ("Render Time:", "渲染时间:"),
    ("Render Settings", "渲染设置"),
    ("Log", "日志"),
    ("Output Module", "输出模块"),
    ("Output Module:", "输出模块:"),
    ("Render Settings:", "渲染设置:"),
    ("Best Settings", "最佳设置"),
    ("DV Settings", "DV 设置"),
    ("Draft Settings", "草图设置"),
    ("Multi-Machine Settings", "多机设置"),
    ("Render Settings Templates", "渲染设置模板"),
    ("Output Module Templates", "输出模块模板"),
    ("Make Template...", "创建模板..."),
    ("File Name and Location Templates", "文件名称和位置模板"),
    ("Comp and Output Module Name", "合成和输出模块名称"),
    ("Project and Comp Name", "项目和合成名称"),
    ("Comp Name and Dimensions", "合成名称和尺寸"),
    ("Comp and Frame Range", "合成和帧范围"),
    ("Comp Name and Date/Time", "合成名称和日期/时间"),
    ("Comp Folder and Name", "合成文件夹和名称"),
    ("Comp Name and Aspect Ratio", "合成名称和长宽比"),
    ("Render Next to Project File", "项目文件旁边的渲染"),
    ("Add Property...", "添加属性..."),
    ("Template:", "模板:"),
    ("Please wait.", "请稍候。"),
    ("AIFF 48kHz", "AIFF 48kHz"),
    ("Alpha Only", "仅 Alpha"),
    (
        "H.264 - Match Render Settings - 15 Mbps",
        "H.264 - 匹配渲染设置 - 15 Mbps",
    ),
    (
        "H.264 - Match Render Settings - 40 Mbps",
        "H.264 - 匹配渲染设置 - 40 Mbps",
    ),
    (
        "H.264 - Match Render Settings - 5 Mbps",
        "H.264 - 匹配渲染设置 - 5 Mbps",
    ),
    ("High Quality", "高品质"),
    ("High Quality with Alpha", "具有 Alpha 的高品质"),
    ("Lossless", "无损"),
    ("Lossless with Alpha", "使用 Alpha 无损耗"),
    ("Multi-Machine Sequence", "多机序列"),
    ("Photoshop", "Photoshop"),
    ("TIFF Sequence with Alpha", "带有 Alpha 的 TIFF 序列"),
    ("Log:", "日志:"),
    ("Errors Only", "仅错误"),
    // Render Queue (AE 简体中文界面用词)
    ("Window", "窗口"),
    ("Render Queue", "渲染队列"),
    ("Add to Render Queue", "添加到渲染队列"),
    ("Render", "渲染"),
    ("Rendering", "正在渲染"),
    ("Queued", "已加入队列"),
    ("Done", "完成"),
    ("Failed", "失败"),
    (
        "Stopped ({done}/{total} frames)",
        "已停止（{done}/{total} 帧）",
    ),
    ("Stop", "停止"),
    ("Render Again", "重新渲染"),
    ("Format", "格式"),
    ("Output To", "输出到"),
    // Output module (AE 26 输出模块设置)
    ("PNG Sequence", "PNG 序列"),
    ("TIFF Sequence", "TIFF 序列"),
    ("OpenEXR Sequence", "OpenEXR 序列"),
    ("Channels", "通道"),
    ("Target Bitrate", "目标比特率"),
    (
        "This format has no alpha channel.",
        "此格式没有 Alpha 通道。",
    ),
    ("ProRes export requires macOS.", "ProRes 导出需要 macOS。"),
    (
        "HEVC decoding is unsupported. Check that Windows HEVC Video Extensions are installed.",
        "系统不支持此 HEVC 视频。请检查是否已安装 Windows「HEVC 视频扩展」。",
    ),
    (
        "A hardware HEVC encoder is required.",
        "导出 HEVC 需要硬件编码器。",
    ),
    (
        "The system H.264 encoder is unavailable.",
        "系统 H.264 编码器不可用。",
    ),
    ("Frames", "帧"),
    ("Show in Finder", "在访达中显示"),
    ("Show in Explorer", "在资源管理器中显示"),
    ("Show in Folder", "在文件夹中显示"),
    ("Rendering needs a GPU.", "渲染需要 GPU。"),
    (
        "Choose where to render: Output To is empty.",
        "请选择渲染输出位置：“输出到”为空。",
    ),
    (
        "The frames are outside the composition (0–{last}).",
        "帧范围超出了合成（0–{last}）。",
    ),
    ("The composition no longer exists.", "合成已不存在。"),
    (
        "The file will be saved as .{ext} to match the format.",
        "文件会按格式保存为 .{ext}。",
    ),
    (
        "Add a composition with Composition > Add to Render Queue.",
        "用“合成 > 添加到渲染队列”添加合成。",
    ),
];

/// Registers this module's Chinese strings and panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    templates::register(app);
    crate::register_panel(
        app,
        "render_queue",
        bae_ui::PanelKind::new("Render Queue", |spawn| {
            Box::new(RenderQueuePanel {
                columns: spawn
                    .state
                    .and_then(|s| serde_json::from_value(s.clone()).ok())
                    .unwrap_or_default(),
                ..Default::default()
            })
        }),
    );
}

#[cfg(test)]
mod motion_blur_test;

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
    fn output_names_expand_in_the_selected_module_and_keep_the_folder_stable() {
        let comp = comp_info("/comps/a", "Main: Shot");
        let mut queue = RenderQueue::default();
        let dir = Path::new("render-here");
        let id = queue.add_with_format(&comp, dir, ExportFormat::Png);
        queue.add_output(id);
        let item = queue.item_mut(id).unwrap();
        let first = item.outputs[0].output.clone();
        let module = &mut item.outputs[1];
        let mut ctx = filenames::Context {
            comp: &comp,
            start: 3,
            end: 8,
            project: None,
        };
        assert!(!ctx.apply(module, "[projectFolder]/[compName].[extension]"));
        assert!(ctx.apply(module, "[compName]/[width]x[height].[extension]"));
        let named = module.output.clone();
        assert_eq!(
            Path::new(&named),
            dir.join("Main_ Shot").join("40x24_[####].png")
        );
        assert!(ctx.apply(module, "[compName]/[width]x[height].[extension]"));
        assert_eq!(module.output, named);
        let project = std::path::absolute("project/Test.bae").unwrap();
        ctx.project = Some(&project);
        assert!(ctx.apply(
            module,
            "[projectFolder]/[projectName]_[startFrame]-[endFrame].[extension]"
        ));
        assert_eq!(
            Path::new(&module.output),
            project.with_file_name("Test_3-8_[####].png")
        );
        assert_eq!(item.outputs[0].output, first);
    }

    #[test]
    fn user_templates_round_trip_and_apply_to_another_composition() {
        let source = comp_info("/comps/a", "A");
        let mut target = comp_info("/comps/b", "B");
        target.frame_rate = FrameRate::FPS_24;
        target.work_area = bae_model::TimeRange::new(Time::ZERO, Time::ONE_SECOND);
        let mut queue = RenderQueue::default();
        let id = queue.add(&source, Path::new("output"));
        let item = queue.item_mut(id).unwrap();
        let work = Span::capture(item, &source);
        assert_eq!(work.range(&target), (0, 23));
        item.start = 0;
        item.end = 14;
        let custom = Span::capture(item, &source);
        item.outputs[0].set_format(ExportFormat::Tiff);
        item.outputs[0].channels = Channels::RgbAlpha;
        let mut lib = Library::default();
        lib.render.insert("半秒".into(), custom);
        lib.outputs.insert(
            "带透明".into(),
            templates::Output::capture(&item.outputs[0]),
        );
        let restored: Library = serde_json::from_value(serde_json::to_value(lib).unwrap()).unwrap();
        assert_eq!(restored.render["半秒"].range(&target), (0, 11));
        assert_eq!(
            restored.outputs["带透明"].settings().unwrap(),
            (
                ExportFormat::Tiff,
                Channels::RgbAlpha,
                DEFAULT_BIT_RATE_MBPS
            )
        );
    }

    /// Save Frame As ▸ File…: one PNG of the current frame named like AE's;
    /// an Output Module template sets the format, channels and bit rate.
    #[test]
    fn save_frame_as_and_output_module_templates() {
        let mut q = RenderQueue::default();
        let id = q.add_frame(&comp_info("/comps/a", "Main"), Path::new("/renders"), 45);
        let item = q.item_mut(id).unwrap();
        assert_eq!((item.start, item.end), (45, 45));
        assert!(
            item.outputs[0].output.ends_with("Main (0;00;01;15).png"),
            "{}",
            item.outputs[0].output
        );
        assert_eq!(om_template(&item.outputs[0]), None);
        item.outputs[0].set_format(ExportFormat::Tiff);
        item.outputs[0].channels = Channels::RgbAlpha;
        assert_eq!(
            om_template(&item.outputs[0]),
            Some("TIFF Sequence with Alpha")
        );
    }

    #[test]
    fn status_header_can_hide_and_restore_while_comp_name_stays_visible() {
        let status = COLUMNS.iter().find(|c| c.title == "Status").unwrap();
        let name = COLUMNS.iter().find(|c| c.title == "Comp Name").unwrap();
        let mut columns = Columns::default();
        columns.toggle(status);
        assert!(!columns.visible(status));
        columns.toggle(status);
        columns.toggle(name);
        assert!(columns.visible(status) && columns.visible(name));
        assert!(ZH.contains(&(status.title, "状态")));
        assert!(crate::project::ZH.contains(&("Rename This...", "重命名此项...")));
    }

    #[test]
    fn unchecked_items_are_not_rendered() {
        let mut q = RenderQueue::default();
        let dir = Path::new("/renders");
        let a = q.add(&comp_info("/comps/a", "A"), dir);
        let b = q.add(&comp_info("/comps/b", "B"), dir);
        q.item_mut(a).unwrap().render = false;
        assert_eq!(q.queued(), 1);
        assert!(q.begin());
        let job = q.take_next(&|_| Some(30)).unwrap();
        assert_eq!(job.id, b, "A is unchecked (AE: Unqueued)");
        assert!(q.items()[1].started.is_some());
        assert!(q.take_next(&|_| Some(30)).is_none());
    }

    #[test]
    fn duplicate_and_add_output_keep_module_settings_and_allocate_independent_paths() {
        let mut queue = RenderQueue::default();
        let id = queue.add_with_format(
            &comp_info("/comps/a", "A"),
            Path::new("renders"),
            ExportFormat::Png,
        );
        queue.add_output(id);
        queue.items[0].outputs[1].set_format(ExportFormat::Tiff);
        let paths = |item: &QueueItem| {
            item.outputs
                .iter()
                .map(|output| output.output.clone())
                .collect::<Vec<_>>()
        };
        let original = paths(&queue.items[0]);
        assert_ne!(original[0], original[1]);
        queue.duplicate(id, false);
        let unique = paths(&queue.items[1]);
        assert!(
            unique
                .iter()
                .all(|path| !original.contains(path) && path.contains("[####]"))
        );
        queue.duplicate(id, true);
        assert_eq!(paths(&queue.items[1]), original);
        assert_eq!(queue.items[2].outputs[1].format, ExportFormat::Tiff);
    }

    #[test]
    fn queue_bookkeeping() {
        let mut q = RenderQueue::default();
        let dir = Path::new("/renders");
        let a = q.add(&comp_info("/comps/main", "Main: v2"), dir);
        let b = q.add(&comp_info("/comps/other", "Other"), dir);
        let item = &q.items()[0];
        assert_eq!((item.start, item.end, item.frame_count()), (3, 8, 6));
        let first = &item.outputs[0];
        if cfg!(target_os = "macos") {
            assert_eq!(first.format, ExportFormat::H264);
            assert_eq!(
                PathBuf::from(&first.output),
                dir.join("Main_ v2/Main_ v2.mp4")
            );
        } else {
            assert_eq!(first.format, ExportFormat::Png);
            assert_eq!(
                PathBuf::from(&first.output),
                dir.join("Main_ v2/Main_ v2_[####].png")
            );
        }
        q.item_mut(a).unwrap().outputs[0].set_format(ExportFormat::Tiff);
        assert!(q.items()[0].outputs[0].output.ends_with("_[####].tif"));
        assert_eq!(q.queued(), 2);

        // Taking a job marks it rendering; it cannot be removed meanwhile.
        assert!(q.begin());
        assert!(!q.begin(), "already rendering");
        let late = q.add(&comp_info("/comps/late", "Late"), dir);
        let job = q.take_next(&|_| Some(30)).unwrap();
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
        assert!(q.take_next(&|_| Some(30)).is_none());
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
        let id = q.add_with_format(&info, dir.path(), ExportFormat::Png);
        q.item_mut(id).unwrap().outputs[0].output = dir
            .path()
            .join("stop/f_####.png")
            .to_string_lossy()
            .into_owned();
        assert!(q.begin());
        let job = q
            .take_next(&|c| bae_motion::query::comp_info(&stage, c).map(|i| i.frame_count()))
            .unwrap();
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

    /// The app's render queue renders on the app's `Arc<GpuContext>` resource and
    /// never creates a device of its own (one device per process, #154).
    #[test]
    fn start_renders_on_the_apps_gpu_context() {
        let Some(gpu) = gpu() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (stage, comp) = project();
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let queue = SharedRenderQueue::default();
        let id = {
            let mut q = queue.lock();
            let id = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            q.item_mut(id).unwrap().outputs[0].output =
                dir.path().join("f_####.png").to_string_lossy().into();
            id
        };
        let mut resources = Resources::new();
        resources.insert(gpu);
        let extra_before = GpuContext::extra_devices_created();
        start(&queue, &stage, &resources).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while queue.lock().is_running() {
            assert!(
                std::time::Instant::now() < deadline,
                "render queue timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let q = queue.lock();
        let item = q.items().iter().find(|i| i.id == id).unwrap();
        assert!(
            matches!(item.status, ItemStatus::Done { .. }),
            "{:?}",
            item.status
        );
        assert_eq!(GpuContext::extra_devices_created(), extra_before);
    }

    #[test]
    fn progress_counts_the_limited_range_from_the_start() {
        // #157: 1780–1900 of an 1800-frame comp renders 20 frames. The item
        // shows “0 / 20” as soon as the worker takes it, not “0 / 121” until
        // render_job has limited the range.
        let mut q = RenderQueue::default();
        let dir = Path::new("/renders");
        let add = |q: &mut RenderQueue, comp: &str, start: i64, end: i64| {
            let id = q.add(&comp_info(comp, "C"), dir);
            let item = q.item_mut(id).unwrap();
            (item.start, item.end) = (start, end);
            id
        };
        let past_end = add(&mut q, "/comps/long", 1780, 1900);
        let outside = add(&mut q, "/comps/long", 1900, 1950);
        let gone = add(&mut q, "/comps/deleted", 0, 10);
        let frames = |c: &PrimPath| (c.as_str() == "/comps/long").then_some(1800);
        assert!(q.begin());
        let status = |q: &RenderQueue, id: u64| {
            q.items()
                .iter()
                .find(|i| i.id == id)
                .unwrap()
                .status
                .clone()
        };
        for (id, total) in [(past_end, 20), (outside, 0), (gone, 0)] {
            let job = q.take_next(&frames).unwrap();
            assert_eq!(job.id, id);
            assert_eq!(status(&q, id), ItemStatus::Rendering { done: 0, total });
        }
        // render_job uses the same limits.
        assert_eq!(limited_range(1780, 1900, 1800), Some((1780, 1799)));
        assert_eq!(limited_range(-5, 3, 10), Some((0, 3)));
        assert_eq!(limited_range(10, 20, 10), None);
        assert_eq!(limited_range(4, 4, 10), Some((4, 4)));
    }

    /// #207: one frame of a `[####]` / `####` output is numbered (AE); a name
    /// without `#` stays a single image.
    #[test]
    fn one_frame_of_a_pattern_gets_its_number() {
        let Some(gpu) = gpu() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (stage, comp) = project();
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let mut q = RenderQueue::default();
        let outputs = ["a_[####].png", "b_####.png", "still.png"];
        for name in outputs {
            let id = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            let item = q.item_mut(id).unwrap();
            item.outputs[0].output = dir.path().join(name).to_string_lossy().into_owned();
            (item.start, item.end) = (5, 5);
        }
        assert!(q.begin());
        let mut renderer = Renderer::new(gpu);
        let mut evaluator = AnimEvaluator::new(None);
        let frames =
            |c: &PrimPath| bae_motion::query::comp_info(&stage, c).map(|i| i.frame_count());
        while let Some(job) = q.take_next(&frames) {
            let status =
                render_job(&mut renderer, &mut evaluator, &stage, &job, &mut |_, _| {}).unwrap();
            assert!(
                matches!(status, ItemStatus::Done { files: 1, .. }),
                "{status:?}"
            );
        }
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a_0005.png", "b_0005.png", "still.png"]);
    }

    #[test]
    fn wav_queue_writes_only_requested_samples_and_preserves_output_on_cancel() {
        use bae_media::{AudioSource, FileAudio};
        use bae_model::{AuthorId, CommandRegistry};
        use serde_json::json;
        let Some(gpu) = gpu() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        let output = dir.path().join("mix.wav");
        let mut wav = bae_media::wav::WavWriter::create(&source, 48_000).unwrap();
        for pair in [[0.0, 0.0], [0.25, -0.5], [0.5, -0.25]] {
            wav.write(&vec![pair; 1600]).unwrap();
        }
        wav.finish().unwrap();
        let (mut stage, comp) = project();
        let mut registry = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut registry);
        let author = AuthorId::new("test:wav").unwrap();
        registry.execute(&mut stage,&author,"item.import",json!({"files":[{"path":source,"name":"source.wav","attrs":{"has_audio":{"type":"bool","value":true}}}]})).unwrap();
        let asset = stage
            .children(&PrimPath::new(bae_motion::query::ASSETS_FOLDER).unwrap())
            .into_iter()
            .find(|p| bae_motion::query::display_name_of(&stage, p) == "source.wav")
            .unwrap();
        registry
            .execute(
                &mut stage,
                &author,
                "layer.create",
                json!({"comp":comp,"source":asset,"kind":"audio"}),
            )
            .unwrap();
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let mut queue = RenderQueue::default();
        let id = queue.add(&info, dir.path());
        let item = queue.item_mut(id).unwrap();
        item.outputs[0].set_format(ExportFormat::Wav);
        item.outputs[0].output = output.to_string_lossy().into_owned();
        (item.start, item.end) = (1, 2);
        assert!(queue.begin());
        let job = queue.take_next(&|_| Some(info.frame_count())).unwrap();
        let mut renderer = Renderer::new(gpu);
        let mut evaluator = AnimEvaluator::new(None);
        let status =
            render_job(&mut renderer, &mut evaluator, &stage, &job, &mut |_, _| {}).unwrap();
        assert!(
            matches!(status, ItemStatus::Done { files: 1, .. }),
            "{status:?}"
        );
        let mut decoded = FileAudio::open(&output).unwrap();
        let pcm = decoded.read(0, 4800).unwrap();
        assert_eq!(
            (pcm.frames(), pcm.sample_rate, pcm.channels),
            (3200, 48000, 2)
        );
        assert_eq!(&pcm.samples[..2], &[0.25, -0.5]);
        assert_eq!(&pcm.samples[3198..3202], &[0.25, -0.5, 0.5, -0.25]);
        drop(decoded);
        let original = std::fs::read(&output).unwrap();
        let cancel = job.cancel.clone();
        let status = render_job(
            &mut renderer,
            &mut evaluator,
            &stage,
            &job,
            &mut |done, _| {
                if done == 1 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        assert!(matches!(status, ItemStatus::Stopped { .. }));
        assert_eq!(std::fs::read(output).unwrap(), original);
    }

    #[test]
    fn default_output_dir_is_the_videos_folder() {
        let videos = PathBuf::from("D:/Media/Videos");
        let home = PathBuf::from("C:/Users/a");
        assert_eq!(
            output_dir_from(Some(videos.clone()), Some(home.clone())),
            videos
        );
        assert_eq!(output_dir_from(None, Some(home.clone())), home);
        assert_eq!(output_dir_from(None, None), PathBuf::from("."));
        // The system's own videos folder (Windows: the known folder, which
        // may live anywhere; macOS: ~/Movies).
        let dir = default_output_dir();
        assert_eq!(
            Some(dir.clone()),
            bae_platform::dirs::videos_dir().or(bae_platform::dirs::home_dir())
        );
        #[cfg(target_os = "macos")]
        assert_eq!(dir, bae_platform::dirs::home_dir().unwrap().join("Movies"));
        #[cfg(windows)]
        assert!(
            !dir.ends_with("Movies"),
            "{} is not the Videos known folder",
            dir.display()
        );
    }

    #[test]
    fn clearing_for_another_project_stops_and_empties() {
        let mut q = RenderQueue::default();
        q.add(&comp_info("/comps/a", "A"), Path::new("/r"));
        q.add(&comp_info("/comps/b", "B"), Path::new("/r"));
        assert!(q.begin());
        let job = q.take_next(&|_| Some(30)).unwrap();
        q.clear();
        assert!(q.items().is_empty());
        assert!(job.cancel.load(Ordering::Relaxed), "the running item stops");
        assert!(
            q.take_next(&|_| Some(30)).is_none(),
            "nothing left to render"
        );
    }

    #[test]
    fn movie_formats_channels_and_names() {
        for &f in FORMATS {
            #[cfg(windows)]
            if matches!(f, ExportFormat::H264 | ExportFormat::Hevc) {
                continue;
            }
            assert_eq!(
                format_available(f),
                f == ExportFormat::Wav || f.is_image() || cfg!(target_os = "macos"),
                "{f:?}"
            );
        }
        assert!(!supports_alpha(ExportFormat::H264) && !supports_alpha(ExportFormat::ProRes));
        assert!(supports_alpha(ExportFormat::ProRes4444) && supports_alpha(ExportFormat::Png));
        // A sequence pattern becomes one movie file and back; the channels
        // follow the format's AE default.
        let mut q = RenderQueue::default();
        let id = q.add(&comp_info("/comps/main", "Main"), Path::new("/r"));
        let item = q.item_mut(id).unwrap();
        assert_eq!(item.outputs[0].bit_rate_mbps, DEFAULT_BIT_RATE_MBPS);
        let steps = [
            (ExportFormat::H264, "/r/Main/Main.mp4", Channels::Rgb),
            (
                ExportFormat::ProRes4444,
                "/r/Main/Main.mov",
                Channels::RgbAlpha,
            ),
            (ExportFormat::ProRes, "/r/Main/Main.mov", Channels::Rgb),
            (ExportFormat::Tiff, "/r/Main/Main_[####].tif", Channels::Rgb),
            (ExportFormat::Hevc, "/r/Main/Main.mp4", Channels::Rgb),
        ];
        for (format, output, channels) in steps {
            item.outputs[0].set_format(format);
            assert_eq!(
                PathBuf::from(&item.outputs[0].output),
                PathBuf::from(output),
                "{format:?}"
            );
            assert_eq!(item.outputs[0].channels, channels, "{format:?}");
        }
        assert_eq!(without_frame_run("shot_####_v2"), "shot__v2");
        assert_eq!(without_frame_run("f[###]"), "f");
        assert_eq!(without_frame_run("####"), "output");
        assert_eq!(
            output_path(Path::new("/r/out/"), ExportFormat::ProRes4444, "Comp 1").unwrap(),
            Path::new("/r/out/Comp 1.mov")
        );
    }

    #[test]
    fn reserves_cpu_capacity_for_other_applications() {
        let mut prefs = bae_prefs::Preferences::default();
        prefs.set("memory_cpu.reserved_cpu_power_percent", 25);
        assert_eq!(render_threads(&prefs, 16), 12);
        assert_eq!(render_threads(&prefs, 1), 1);
        prefs.set("memory_cpu.enable_multi_frame_rendering", false);
        assert_eq!(render_threads(&prefs, 16), 1);
    }

    #[test]
    fn output_module_maps_to_export_settings() {
        let s = |format, channels| {
            export_settings(
                &QueueOutput {
                    naming: Default::default(),
                    template: None,
                    format,
                    channels,
                    output: "/r/x".into(),
                    bit_rate_mbps: 12.5,
                    pcm_audio: true,
                },
                PathBuf::from("/r/x"),
                Some(1),
            )
        };
        let png = s(ExportFormat::Png, Channels::RgbAlpha);
        assert_eq!((png.threads, png.frames_in_flight), (Some(1), 1));
        assert_eq!((png.alpha, png.bit_rate), (ExportAlpha::Straight, None));
        assert_eq!(
            s(ExportFormat::Png, Channels::Rgb).alpha,
            ExportAlpha::Background
        );
        // H.264 has no alpha: RGB over the background whatever is asked.
        let h264 = s(ExportFormat::H264, Channels::RgbAlpha);
        assert_eq!(
            (h264.alpha, h264.bit_rate),
            (ExportAlpha::Background, Some(12_500_000))
        );
        let p4444 = s(ExportFormat::ProRes4444, Channels::RgbAlpha);
        assert_eq!((p4444.alpha, p4444.bit_rate), (ExportAlpha::Straight, None));
    }

    /// The same queued jobs use the system encoders on both desktops.
    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn renders_available_movies() {
        let Some(gpu) = gpu() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let mut scene = bae_render::fixture::SceneBuilder::new();
        let comp = scene
            .comp("main", 320, 240, FrameRate::FPS_30, Time::ONE_SECOND)
            .unwrap();
        let stage = Project::from_stage(scene.into_stage()).stage;
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let mut q = RenderQueue::default();
        let formats: Vec<_> = [
            ExportFormat::H264,
            ExportFormat::ProRes4444,
            #[cfg(target_os = "macos")]
            ExportFormat::ProRes422,
            #[cfg(target_os = "macos")]
            ExportFormat::QuickTimeH264,
            #[cfg(target_os = "macos")]
            ExportFormat::QuickTimeHevc,
            ExportFormat::Hevc,
        ]
        .into_iter()
        .filter(|&format| format_available(format))
        .collect();
        if formats.is_empty() {
            eprintln!("SKIP: no system movie encoder");
            return;
        }
        for &format in &formats {
            let id = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            let item = q.item_mut(id).unwrap();
            item.outputs[0].set_format(format);
            item.outputs[0].output = dir
                .path()
                .join(format!("m_{}.{}", format.name(), format.extension()))
                .to_string_lossy()
                .into_owned();
            (item.start, item.end) = (0, 9);
        }
        assert!(q.begin());
        let mut renderer = Renderer::new(gpu);
        let mut evaluator = AnimEvaluator::new(None);
        let frames =
            |c: &PrimPath| bae_motion::query::comp_info(&stage, c).map(|i| i.frame_count());
        while let Some(job) = q.take_next(&frames) {
            let status =
                render_job(&mut renderer, &mut evaluator, &stage, &job, &mut |_, _| {}).unwrap();
            assert!(
                matches!(status, ItemStatus::Done { .. }),
                "{:?}: {status:?}",
                job.outputs[0].format
            );
        }
        for format in formats {
            let file = dir
                .path()
                .join(format!("m_{}.{}", format.name(), format.extension()));
            let len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            assert!(len > 0, "{} not written", file.display());
        }
    }

    /// #225: a new PNG item writes RGB (opaque, transparent areas over the
    /// comp background) like AE; RGB + Alpha keeps them transparent.
    #[test]
    fn png_defaults_to_rgb_and_rgb_alpha_keeps_transparency() {
        let Some(gpu) = gpu() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (stage, comp) = project();
        let info = bae_motion::query::comp_info(&stage, &comp).unwrap();
        let mut q = RenderQueue::default();
        for (name, channels) in [("rgb", None), ("alpha", Some(Channels::RgbAlpha))] {
            let id = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            let item = q.item_mut(id).unwrap();
            assert_eq!(
                (item.outputs[0].format, item.outputs[0].channels),
                (ExportFormat::Png, Channels::Rgb)
            );
            if let Some(c) = channels {
                item.outputs[0].channels = c;
            }
            item.outputs[0].output = dir
                .path()
                .join(format!("{name}.png"))
                .to_string_lossy()
                .into_owned();
            (item.start, item.end) = (0, 0);
        }
        assert!(q.begin());
        let mut renderer = Renderer::new(gpu);
        let mut evaluator = AnimEvaluator::new(None);
        let frames =
            |c: &PrimPath| bae_motion::query::comp_info(&stage, c).map(|i| i.frame_count());
        while let Some(job) = q.take_next(&frames) {
            render_job(&mut renderer, &mut evaluator, &stage, &job, &mut |_, _| {}).unwrap();
        }
        // The fixture's solid covers the top-left 10×10 of a 40×24 comp; the
        // bottom-right corner is empty.
        let corner = |file: &str| {
            let (w, h, px) = bae_render::png_out::read_rgba8(&dir.path().join(file)).unwrap();
            let i = (((h - 1) * w + (w - 1)) * 4) as usize;
            px[i..i + 4].to_vec()
        };
        assert_eq!(corner("rgb.png"), [0, 0, 0, 255], "RGB: over black");
        assert_eq!(corner("alpha.png")[3], 0, "RGB + Alpha: transparent");
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
            let png = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            let tiff = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            let stopped = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            let bad = q.add_with_format(&info, dir.path(), ExportFormat::Png);
            q.item_mut(png).unwrap().outputs[0].output =
                dir.path().join("png/f_[####].png").to_string_lossy().into();
            q.add_output(tiff);
            let t = q.item_mut(tiff).unwrap();
            t.log = LogMode::Frames;
            t.outputs[1].output = dir
                .path()
                .join("another/f_####.png")
                .to_string_lossy()
                .into_owned();
            t.outputs[0].set_format(ExportFormat::Tiff);
            t.outputs[0].output = dir.path().join("tif/f_####.tif").to_string_lossy().into();
            (t.start, t.end) = (4, 5);
            q.item_mut(stopped).unwrap().outputs[0].output = dir
                .path()
                .join("stopped/f_####.png")
                .to_string_lossy()
                .into();
            // A regular file where the output folder should be.
            std::fs::write(dir.path().join("blocker"), b"x").unwrap();
            q.item_mut(bad).unwrap().outputs[0].output = dir
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

        let snapshot = crate::preview_snapshot::PreviewSnapshot::take(&stage);
        run_worker(&queue, &snapshot, gpu, &[], None);

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
                assert_eq!(files, 4);
                assert!(dir.path().join("another/f_0005.png").is_file());
                let item = q.items().iter().find(|i| i.id == tiff).unwrap();
                let text = std::fs::read_to_string(item.log_file.as_ref().unwrap()).unwrap();
                assert!(text.contains("2 · 5 ·"));
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
