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

/// Formats offered in the panel and by `bae render --format`.
/// WAV comes last, as in AE's Format menu.
pub const FORMATS: &[ExportFormat] = &[
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
    ExportFormat::Wav,
];

/// AE 26's Render Settings templates; BAE renders at Best Settings, the
/// others are shown greyed.
const RS_TEMPLATES: [&str; 5] = [
    "Best Settings",
    "Current Settings",
    "DV Settings",
    "Draft Settings",
    "Multi-Machine Settings",
];

/// What BAE writes for an Output Module template: format, channels, video
/// bit rate.
type OmSettings = (ExportFormat, Channels, f64);

/// AE 26's Output Module templates and what BAE writes for them (`None`:
/// greyed, a format BAE does not write).
const OM_TEMPLATES: [(&str, Option<OmSettings>); 12] = [
    ("AIFF 48kHz", None),
    ("Alpha Only", None),
    (
        "H.264 - Match Render Settings - 15 Mbps",
        Some((ExportFormat::H264, Channels::Rgb, 15.0)),
    ),
    (
        "H.264 - Match Render Settings - 40 Mbps",
        Some((ExportFormat::H264, Channels::Rgb, 40.0)),
    ),
    (
        "H.264 - Match Render Settings - 5 Mbps",
        Some((ExportFormat::H264, Channels::Rgb, 5.0)),
    ),
    (
        "High Quality",
        Some((ExportFormat::ProRes, Channels::Rgb, DEFAULT_BIT_RATE_MBPS)),
    ),
    (
        "High Quality with Alpha",
        Some((
            ExportFormat::ProRes4444,
            Channels::RgbAlpha,
            DEFAULT_BIT_RATE_MBPS,
        )),
    ),
    ("Lossless", None),
    ("Lossless with Alpha", None),
    ("Multi-Machine Sequence", None),
    ("Photoshop", None),
    (
        "TIFF Sequence with Alpha",
        Some((
            ExportFormat::Tiff,
            Channels::RgbAlpha,
            DEFAULT_BIT_RATE_MBPS,
        )),
    ),
];

/// The Output Module template an item's settings match, if any.
fn om_template(item: &QueueItem) -> Option<&'static str> {
    OM_TEMPLATES.iter().find_map(|(name, t)| {
        let (f, c, rate) = (*t)?;
        let rate_matches = !has_bit_rate(f) || (item.bit_rate_mbps - rate).abs() < 1e-6;
        (item.format == f && item.channels == c && rate_matches).then_some(*name)
    })
}

/// A small ▾ button opening a template menu: `items` (name, enabled) sorted
/// by their shown names, then Custom… and the greyed Make Template….
/// Returns the picked item or `Some(None)` for Custom….
fn template_menu(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    items: &[(&'static str, bool)],
) -> Option<Option<&'static str>> {
    let resp = ui
        .add(egui::Button::new("▾").frame(false).small())
        .labeled(egui::WidgetType::Button, label);
    let mut picked = None;
    let mut sorted = items.to_vec();
    sorted.sort_by_key(|(n, _)| tr(n));
    egui::Popup::menu(&resp).id(id).show(|ui| {
        for (name, enabled) in &sorted {
            if ui
                .add_enabled(*enabled, egui::Button::new(tr(name)))
                .clicked()
            {
                picked = Some(Some(*name));
            }
        }
        ui.separator();
        if ui.button(tr("Custom...")).clicked() {
            picked = Some(None);
        }
        ui.add_enabled(false, egui::Button::new(tr("Make Template...")));
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
    /// First and last frame (comp frames, inclusive).
    pub start: i64,
    /// Last frame (inclusive).
    pub end: i64,
    /// Current state.
    pub status: ItemStatus,
    /// The Render column's check box: unchecked items stay in the queue but
    /// are not rendered (AE: Unqueued).
    pub render: bool,
    /// When its render started (AE's Started column).
    pub started: Option<std::time::SystemTime>,
    cancel: Arc<AtomicBool>,
}

impl QueueItem {
    /// Frames in the range.
    pub fn frame_count(&self) -> usize {
        (self.end - self.start + 1).max(0) as usize
    }

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
            format,
            output: output.to_string_lossy().into_owned(),
            channels: default_channels(format),
            bit_rate_mbps: DEFAULT_BIT_RATE_MBPS,
            pcm_audio: true,
            start,
            end,
            status: ItemStatus::Queued,
            render: true,
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
            item.output = output_dir
                .join(format!(
                    "{} ({code}).{}",
                    sanitize(&comp.name),
                    format.extension()
                ))
                .to_string_lossy()
                .into_owned();
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
        item.status = ItemStatus::Rendering { done: 0, total };
        item.started = Some(std::time::SystemTime::now());
        Some(Job {
            id: item.id,
            comp: item.comp.clone(),
            rate: item.rate,
            format: item.format,
            output: PathBuf::from(&item.output),
            channels: item.channels,
            bit_rate_mbps: item.bit_rate_mbps,
            pcm_audio: item.pcm_audio,
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
    channels: Channels,
    bit_rate_mbps: f64,
    pcm_audio: bool,
    start: i64,
    end: i64,
    cancel: Arc<AtomicBool>,
}

/// Renders one item; `progress` gets the frames done so far.
/// The export settings of a job's output module.
fn export_settings(job: &Job, output: PathBuf) -> ExportSettings {
    let mut settings = ExportSettings::new(job.format, output);
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
    let output = output_path(&job.output, job.format, &info.name)?;
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
    progress(0, total);
    // A `#` run in the name numbers the frames, one frame too (#207);
    // export_frames decides, like `bae render`.
    let settings = export_settings(job, output);
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
    if !queue.lock().begin() {
        return Ok(());
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
        // Inset from the panel's edges like the other panels (#228).
        let pad = bae_ui::theme::metrics::PANEL_PADDING as i8;
        egui::Frame::new()
            .inner_margin(egui::Margin::same(pad))
            .show(ui, |ui| self.contents(ui, cx));
    }
}

impl RenderQueuePanel {
    fn contents(&mut self, ui: &mut egui::Ui, cx: &PanelContext<'_>) {
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
        let mut open = None;
        // No scroll area of its own: the dock scrolls a panel that does not fit
        // (AE opens the Render Queue in the short Timeline frame).
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
        if q.running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        drop(q);
        // The item's settings links (the dialogs read the queue themselves).
        match open {
            Some((id, true)) => crate::dialogs::render_settings(cx, id),
            Some((id, false)) => crate::dialogs::output_module(cx, id),
            None => {}
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
    open: &mut Option<(u64, bool)>,
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
        // AE's Render column and the item's number.
        ui.add_enabled_ui(!matches!(item.status, ItemStatus::Rendering { .. }), |ui| {
            let mut render = item.render;
            if bae_ui::widgets::checkbox(ui, &mut render, "")
                .labeled_toggle(egui::WidgetType::Checkbox, &name("Render"), render)
                .changed()
            {
                item.render = render;
            }
        });
        ui.label(number.to_string());
        ui.strong(&item.comp_name);
        match &item.status {
            ItemStatus::Queued if !item.render => {
                ui.label(tr("Unqueued"));
            }
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
                ui.colored_label(ui.visuals().error_fg_color, tr("Failed"));
                ui.colored_label(ui.visuals().error_fg_color, message);
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
    // AE's Render Time column (time so far while rendering).
    if let Some(started) = item.started {
        let time = match &item.status {
            ItemStatus::Done { seconds, .. } => format!("{seconds:.1} s"),
            _ => started
                .elapsed()
                .map_or_else(|_| String::new(), |d| format!("{:.1} s", d.as_secs_f64())),
        };
        ui.label(
            egui::RichText::new(format!("{} {time}", tr("Render Time:")))
                .color(bae_ui::theme::color::TEXT_DIM),
        );
    }
    // Render Settings and Log (AE's templates; BAE renders at Best Settings).
    // AE's template links: Render Settings and Output Module open their
    // dialogs; Log keeps AE's default.
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal_wrapped(|ui| {
            let rs: Vec<(&str, bool)> = RS_TEMPLATES
                .iter()
                .map(|t| (*t, *t == "Best Settings"))
                .collect();
            let id = egui::Id::new(("rq-rs-templates", item.id));
            if let Some(None) = template_menu(ui, id, &name("Render Settings Templates"), &rs) {
                *open = Some((item.id, true));
            }
            ui.label(tr("Render Settings:"));
            if bae_ui::widgets::link(ui, tr("Best Settings"))
                .labeled(egui::WidgetType::Link, &name("Render Settings"))
                .clicked()
            {
                *open = Some((item.id, true));
            }
            ui.add_space(12.0);
            ui.add_enabled_ui(false, |ui| {
                ui.label(tr("Log:"));
                let mut j = 0;
                bae_ui::widgets::dropdown(ui, "log", &mut j, &[tr("Errors Only")], 110.0)
                    .labeled_value(egui::WidgetType::ComboBox, &name("Log"), tr("Errors Only"));
            });
            ui.add_space(12.0);
            let om: Vec<(&str, bool)> = OM_TEMPLATES
                .iter()
                .map(|(n, t)| (*n, t.is_some_and(|(f, ..)| format_available(f))))
                .collect();
            let id = egui::Id::new(("rq-om-templates", item.id));
            match template_menu(ui, id, &name("Output Module Templates"), &om) {
                Some(None) => *open = Some((item.id, false)),
                Some(Some(t)) => {
                    if let Some((_, Some((f, c, rate)))) =
                        OM_TEMPLATES.iter().find(|(n, _)| *n == t)
                    {
                        if item.format != *f {
                            item.set_format(*f);
                        }
                        item.channels = *c;
                        item.bit_rate_mbps = *rate;
                    }
                }
                None => {}
            }
            ui.label(tr("Output Module:"));
            let om_label = om_template(item).unwrap_or(format_label(item.format));
            if bae_ui::widgets::link(ui, tr(om_label))
                .labeled(egui::WidgetType::Link, &name("Output Module"))
                .clicked()
            {
                *open = Some((item.id, false));
            }
        });
    });
    ui.add_enabled_ui(editable, |ui| {
        egui::Grid::new("output").num_columns(2).show(ui, |ui| {
            // Fields are named after the label in front of them and the item
            // (screen readers read "… · Format: PNG", issue #128).
            ui.label(tr("Format"));
            let mut format = item.format;
            // As wide as the column allows; long names are shortened rather
            // than widening a narrow panel.
            let combo_w = ui.available_width().clamp(60.0, 220.0);
            egui::ComboBox::from_id_salt("format")
                .width(combo_w)
                .truncate()
                .selected_text(tr(format_label(format)))
                .show_ui(ui, |ui| {
                    for &f in FORMATS {
                        let available = format_available(f);
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
                                ExportFormat::Hevc => "A hardware HEVC encoder is required.",
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
                    tr(format_label(item.format)),
                );
            if format != item.format {
                item.set_format(format);
            }
            ui.end_row();
            // AE: Channels RGB / RGB + Alpha (RGB + Alpha only where the
            // codec has alpha).
            if item.format == ExportFormat::Wav {
                ui.label(tr("Audio"));
                ui.label(tr("Stereo, 48 kHz, 24-bit PCM"));
                ui.end_row();
            } else {
                ui.label(tr("Channels"));
                let mut channels = item.channels;
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
                        let alpha = supports_alpha(item.format);
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
                            r.on_disabled_hover_text(tr("This format has no alpha channel."));
                        }
                    })
                    .response
                    .labeled_value(
                        egui::WidgetType::ComboBox,
                        &name("Channels"),
                        channels_label(item.channels),
                    );
                item.channels = channels;
                ui.end_row();
            }
            if has_bit_rate(item.format) {
                // AE's H.264 format options: VBR, target bitrate.
                ui.label(tr("Target Bitrate"));
                let value = format!("{} Mbps", item.bit_rate_mbps);
                ui.add(
                    egui::DragValue::new(&mut item.bit_rate_mbps)
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
    ("Video Codec", "视频编码器"),
    ("24-bit PCM", "24 位 PCM"),
    ("Save Frame As File", "帧另存为文件"),
    // AE 26's Render Queue (渲染队列) terms.
    ("Unqueued", "未加入队列"),
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
    crate::register_panel(
        app,
        "render_queue",
        bae_ui::PanelKind::new("Render Queue", |_| Box::new(RenderQueuePanel::default())),
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

    /// Save Frame As ▸ File…: one PNG of the current frame named like AE's;
    /// an Output Module template sets the format, channels and bit rate.
    #[test]
    fn save_frame_as_and_output_module_templates() {
        let mut q = RenderQueue::default();
        let id = q.add_frame(&comp_info("/comps/a", "Main"), Path::new("/renders"), 45);
        let item = q.item_mut(id).unwrap();
        assert_eq!((item.start, item.end), (45, 45));
        assert!(
            item.output.ends_with("Main (0;00;01;15).png"),
            "{}",
            item.output
        );
        assert_eq!(om_template(item), None);
        item.set_format(ExportFormat::Tiff);
        item.channels = Channels::RgbAlpha;
        assert_eq!(om_template(item), Some("TIFF Sequence with Alpha"));
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
    fn queue_bookkeeping() {
        let mut q = RenderQueue::default();
        let dir = Path::new("/renders");
        let a = q.add(&comp_info("/comps/main", "Main: v2"), dir);
        let b = q.add(&comp_info("/comps/other", "Other"), dir);
        let item = &q.items()[0];
        assert_eq!((item.start, item.end, item.frame_count()), (3, 8, 6));
        if cfg!(target_os = "macos") {
            assert_eq!(item.format, ExportFormat::H264);
            assert_eq!(
                PathBuf::from(&item.output),
                dir.join("Main_ v2/Main_ v2.mp4")
            );
        } else {
            assert_eq!(item.format, ExportFormat::Png);
            assert_eq!(
                PathBuf::from(&item.output),
                dir.join("Main_ v2/Main_ v2_[####].png")
            );
        }
        q.item_mut(a).unwrap().set_format(ExportFormat::Tiff);
        assert!(q.items()[0].output.ends_with("_[####].tif"));
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
        q.item_mut(id).unwrap().output = dir
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
            q.item_mut(id).unwrap().output = dir.path().join("f_####.png").to_string_lossy().into();
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
            item.output = dir.path().join(name).to_string_lossy().into_owned();
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
        item.set_format(ExportFormat::Wav);
        item.output = output.to_string_lossy().into_owned();
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
        assert_eq!(item.bit_rate_mbps, DEFAULT_BIT_RATE_MBPS);
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
            item.set_format(format);
            assert_eq!(
                PathBuf::from(&item.output),
                PathBuf::from(output),
                "{format:?}"
            );
            assert_eq!(item.channels, channels, "{format:?}");
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
    fn output_module_maps_to_export_settings() {
        let job = |format, channels| Job {
            id: 0,
            comp: PrimPath::new("/c").unwrap(),
            rate: FrameRate::FPS_30,
            format,
            output: PathBuf::from("/r/x"),
            channels,
            bit_rate_mbps: 12.5,
            pcm_audio: true,
            start: 0,
            end: 0,
            cancel: Arc::default(),
        };
        let s = |format, channels| export_settings(&job(format, channels), PathBuf::from("/r/x"));
        let png = s(ExportFormat::Png, Channels::RgbAlpha);
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
            item.set_format(format);
            item.output = dir
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
                job.format
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
                (item.format, item.channels),
                (ExportFormat::Png, Channels::Rgb)
            );
            if let Some(c) = channels {
                item.channels = c;
            }
            item.output = dir
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
