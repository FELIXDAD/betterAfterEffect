//! Modal dialogs of the motion workspace (AE's Composition Settings, …), drawn by
//! the workbench through [`bae_ui::DialogHost`].
//!
//! The open dialog and the values AE remembers between uses ("last used"
//! settings) live in the [`SharedDialogs`] app resource. A dialog only collects
//! values; OK runs one `bae-motion` command (one undo step), Cancel runs none.

mod camera_settings;
mod comp_settings;
pub(crate) mod fields;
mod go_to_time;
mod interpret_footage;
mod layer_time;
mod light_settings;
mod mask_value;
pub mod memory;
pub(crate) mod photoshop;
mod precompose;
mod project_settings;
mod render_output;
mod solid_settings;
mod transform_value;

use std::sync::{Arc, Mutex, MutexGuard};

use bae_app::Resources;
use bae_model::PrimPath;
use bae_motion::query;
use bae_ui::PanelContext;

pub use camera_settings::{CameraSettings, CameraSettingsMemory, CameraValues, FilmUnits};
pub use comp_settings::{CompSettings, CompSettingsMemory};
pub use light_settings::{LightSettings, LightSettingsMemory, LightValues};
pub use mask_value::{Kind as MaskKind, MaskValue};
pub use precompose::{PrecomposeDialog, PrecomposeMemory};
pub use solid_settings::{SolidSettings, SolidSettingsMemory, Units};
pub use transform_value::{Kind as TransformKind, TransformValue};

use crate::session::SharedSession;

/// The dialog that is open.
#[derive(Clone, Debug)]
pub enum Dialog {
    Photoshop(Box<photoshop::PhotoshopDialog>),
    /// Analysis of the composition work area for the Keyframe Assistant.
    AudioAnalysis(crate::audio_analysis::Analysis),
    /// Display units and slider floor of the Audio panel.
    AudioOptions(crate::panels::audio::AudioOptionsDialog),
    /// Composition > New Composition (Cmd/Ctrl+N) and Composition Settings
    /// (Cmd/Ctrl+K).
    CompSettings(Box<CompSettings>),
    /// Layer > New > Solid (Cmd/Ctrl+Y) and Solid Settings (Cmd/Ctrl+Shift+Y).
    SolidSettings(Box<SolidSettings>),
    /// Layer > New > Camera and Camera Settings.
    CameraSettings(Box<CameraSettings>),
    /// Layer > New > Light and Light Settings.
    LightSettings(Box<LightSettings>),
    /// Layer > Pre-compose (Cmd/Ctrl+Shift+C).
    Precompose(Box<PrecomposeDialog>),
    /// File > Interpret Footage > Main (Cmd/Ctrl+Alt+G).
    InterpretFootage(Box<interpret_footage::InterpretFootage>),
    /// Layer > Time > Time Stretch.
    TimeStretch(Box<layer_time::TimeStretch>),
    /// Layer > Transform > Auto-Orient (Cmd/Ctrl+Alt+O).
    AutoOrient(Box<layer_time::AutoOrient>),
    /// View > Go to Time (Alt+Shift+J).
    GoToTime(Box<go_to_time::GoToTime>),
    /// File > Project Settings (Cmd/Ctrl+Alt+Shift+K).
    ProjectSettings(Box<project_settings::ProjectSettings>),
    EffectManager(Box<crate::effects::manager::EffectManager>),
    /// Layer ▸ Transform ▸ Position… and the other value dialogs, and a
    /// keyframe's Edit Value….
    TransformValue(Box<TransformValue>),
    /// Layer ▸ Mask ▸ Mask Shape… / Feather… / Opacity… / Expansion….
    MaskValue(Box<MaskValue>),
    /// A Render Queue item's Render Settings.
    RenderSettings(Box<render_output::RenderSettings>),
    /// A Render Queue item's Output Module Settings.
    OutputModule(Box<render_output::OutputModule>),
    /// An alert: a message and OK (import errors …).
    Alert {
        title: String,
        message: String,
    },
    /// View ▸ View Options (Cmd/Ctrl+Alt+U).
    ViewOptions(Box<crate::viewer::view_options::LayerControls>),
    /// View ▸ Panel Background Color ▸ Select Custom Background Color…
    BackgroundColor([u8; 3]),
    /// View ▸ Resolution ▸ Custom…: pixel steps (horizontal, vertical);
    /// `nested`: also for the comps nested in the active one.
    CustomResolution {
        steps: [u32; 2],
        nested: bool,
    },
}

/// Open dialog and remembered values.
#[derive(Debug, Default)]
pub struct Dialogs {
    pub open: Option<Dialog>,
    pub(crate) photoshop_queue: std::collections::VecDeque<photoshop::PhotoshopDialog>,
    pub(crate) frame_checks: crate::import::FrameChecks,
    /// Settings of the last comp created through the dialog (AE starts the next
    /// New Composition from them).
    pub comp: CompSettingsMemory,
    /// Where the remembered values are saved (`dialogs.json`), if anywhere.
    pub file: Option<std::path::PathBuf>,
    /// The file was looked for (on the first dialog: the config folder is
    /// known only after start-up).
    pub loaded: bool,
    /// Color and options of the last new solid.
    pub solid: SolidSettingsMemory,
    /// Lens and options of the last new camera.
    pub camera: CameraSettingsMemory,
    /// Settings of the last new light.
    pub light: LightSettingsMemory,
    /// Pre-compose options.
    pub precompose: PrecomposeMemory,
}

/// The dialogs as an app resource (inserted by [`crate::MotionUiModule`]).
#[derive(Clone, Debug, Default)]
pub struct SharedDialogs(Arc<Mutex<Dialogs>>);

impl SharedDialogs {
    /// Dialogs that remember their values in `file` across runs (loaded now).
    pub fn with_file(file: std::path::PathBuf) -> Self {
        let mut d = Dialogs::default();
        memory::load(&file).apply(&mut d);
        d.file = Some(file);
        d.loaded = true;
        SharedDialogs(Arc::new(Mutex::new(d)))
    }

    pub fn lock(&self) -> MutexGuard<'_, Dialogs> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether a motion dialog is open.
    pub fn is_open(&self) -> bool {
        let d = self.lock();
        d.open.is_some() || !d.photoshop_queue.is_empty()
    }
}

/// Read the remembered values from the config folder the first time a
/// dialog opens (the app knows the folder only after start-up; tests have
/// none and remember nothing across runs).
fn remember(resources: &Resources, d: &mut Dialogs) {
    if d.loaded {
        return;
    }
    d.loaded = true;
    if let Some(dirs) = resources.get::<bae_platform::AppDirs>() {
        let file = dirs.config_file(memory::FILE_NAME);
        memory::load(&file).apply(d);
        d.file = Some(file);
    }
}

/// The dialogs resource; a private one when the app did not insert it (tests).
pub(crate) fn shared(resources: &Resources) -> SharedDialogs {
    resources
        .get::<SharedDialogs>()
        .cloned()
        .unwrap_or_else(|| {
            thread_local! {
                static LOCAL: SharedDialogs = SharedDialogs::default();
            }
            LOCAL.with(Clone::clone)
        })
}

/// Open Composition Settings for a new comp (in `folder`, default /comps).
pub(crate) fn new_comp(cx: &PanelContext<'_>, session: &SharedSession, folder: Option<PrimPath>) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    // One dialog at a time (two Cmd+N in one frame open it once).
    if d.open.is_some() {
        return;
    }
    remember(cx.resources, &mut d);
    let settings = CompSettings::new_comp(cx.stage, session, &d.comp, folder);
    d.open = Some(Dialog::CompSettings(Box::new(settings)));
}

/// Open Composition Settings for an existing comp.
pub(crate) fn comp_settings(cx: &PanelContext<'_>, comp: &PrimPath) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    remember(cx.resources, &mut d);
    if let Some(settings) = CompSettings::edit(cx.stage, comp, &d.comp) {
        d.open = Some(Dialog::CompSettings(Box::new(settings)));
    }
}

/// Open Solid Settings for a new solid in the active comp.
pub(crate) fn new_solid(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    remember(cx.resources, &mut d);
    if let Some(s) = SolidSettings::new_solid(cx.stage, session, &d.solid) {
        d.open = Some(Dialog::SolidSettings(Box::new(s)));
    }
}

/// Open the settings dialog of a layer or a solid item: Solid Settings
/// (solid and adjustment layers, solid items), Camera Settings or Light
/// Settings.
pub(crate) fn layer_settings(cx: &PanelContext<'_>, session: &SharedSession, target: &PrimPath) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    let stage = cx.stage;
    let open = match query::layer_kind(stage, target).as_deref() {
        Some("camera") => CameraSettings::edit(stage, session, target)
            .map(|s| Dialog::CameraSettings(Box::new(s))),
        Some("light") => {
            LightSettings::edit(stage, session, target).map(|s| Dialog::LightSettings(Box::new(s)))
        }
        _ => SolidSettings::edit(stage, session, &d.solid, target)
            .map(|s| Dialog::SolidSettings(Box::new(s))),
    };
    if open.is_some() {
        d.open = open;
    }
}

/// Open `dialog` unless one is open already.
pub(crate) fn open(resources: &Resources, dialog: Dialog) {
    let dialogs = shared(resources);
    let mut d = dialogs.lock();
    if d.open.is_none() {
        d.open = Some(dialog);
    }
}

/// Show an alert (AE's error dialogs): `message` and OK. Waits for the open
/// dialog to close first when one is open.
pub(crate) fn alert(cx: &PanelContext<'_>, title: &str, message: String) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none() {
        d.open = Some(Dialog::Alert {
            title: title.to_owned(),
            message,
        });
    }
}

/// Draw an alert; returns whether it stays open.
fn alert_ui(ctx: &egui::Context, title: &str, message: &str) -> bool {
    use bae_ui::dialog::{self, Outcome};
    let id = egui::Id::new("bae-ui-motion.alert");
    let shown = dialog::window(ctx, id, title, |ui| {
        ui.set_max_width(420.0);
        ui.label(message);
        ui.add_space(8.0);
        let mut ok = false;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 24.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let b = egui::Button::new(bae_ui::tr("OK")).min_size(egui::vec2(72.0, 22.0));
                ok = ui.add(b).clicked();
            },
        );
        // Enter and Esc both close it.
        ok || dialog::keys(ui.ctx(), true) != Outcome::None
    });
    match shown {
        Some(done) => !done,
        None => !dialog::closed_by_title_bar(ctx, id),
    }
}

/// Open Pre-compose for `layers` (unlocked layers of one comp).
pub(crate) fn precompose(cx: &PanelContext<'_>, layers: Vec<PrimPath>) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() || layers.is_empty() {
        return;
    }
    if let Some(p) = PrecomposeDialog::new(cx.stage, layers, &d.precompose) {
        d.open = Some(Dialog::Precompose(Box::new(p)));
    }
}

/// Open Interpret Footage for the footage selected in the Project panel.
pub(crate) fn interpret_footage(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    let selected = session.lock().selected_items.clone();
    if d.open.is_none()
        && let Some(f) = interpret_footage::InterpretFootage::new(cx.stage, &selected)
    {
        d.open = Some(Dialog::InterpretFootage(Box::new(f)));
    }
}

/// Open Time Stretch for the selected layers.
pub(crate) fn time_stretch(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none()
        && let Some(t) = layer_time::TimeStretch::new(cx.stage, session)
    {
        d.open = Some(Dialog::TimeStretch(Box::new(t)));
    }
}

/// Open Auto-Orient for the selected layers.
pub(crate) fn auto_orient(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none()
        && let Some(a) = layer_time::AutoOrient::new(cx.stage, session)
    {
        d.open = Some(Dialog::AutoOrient(Box::new(a)));
    }
}

/// Open Go to Time for the active comp.
pub(crate) fn go_to_time(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none()
        && let Some(g) = go_to_time::GoToTime::new(cx.stage, session)
    {
        d.open = Some(Dialog::GoToTime(Box::new(g)));
    }
}

/// Open Project Settings.
pub(crate) fn project_settings(cx: &PanelContext<'_>) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none() {
        d.open = Some(Dialog::ProjectSettings(Box::new(
            project_settings::ProjectSettings::new(cx.stage),
        )));
    }
}

pub(crate) fn audio_options(
    cx: &PanelContext<'_>,
    options: crate::panels::audio::AudioOptionsDialog,
) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none() {
        d.open = Some(Dialog::AudioOptions(options));
    }
}

/// Open Render Settings for render queue item `item`.
pub(crate) fn render_settings(cx: &PanelContext<'_>, item: u64) {
    let queue = crate::render_queue::shared(cx.resources);
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none()
        && let Some(r) = render_output::RenderSettings::new(cx.stage, &queue, item)
    {
        d.open = Some(Dialog::RenderSettings(Box::new(r)));
    }
}

/// Open Output Module Settings for render queue item `item`.
pub(crate) fn output_module(cx: &PanelContext<'_>, item: u64) {
    let queue = crate::render_queue::shared(cx.resources);
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_none()
        && let Some(o) = render_output::OutputModule::new(&queue, item)
    {
        d.open = Some(Dialog::OutputModule(Box::new(o)));
    }
}

/// Open Camera Settings for a new camera in the active comp.
pub(crate) fn new_camera(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    if let Some(s) = CameraSettings::new_camera(cx.stage, session, &d.camera) {
        d.open = Some(Dialog::CameraSettings(Box::new(s)));
    }
}

/// Open Light Settings for a new light in the active comp.
pub(crate) fn new_light(cx: &PanelContext<'_>, session: &SharedSession) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    if let Some(s) = LightSettings::new_light(cx.stage, session, &d.light) {
        d.open = Some(Dialog::LightSettings(Box::new(s)));
    }
}

/// Open the value dialog of `kind` for `layers` at comp time `now`, or for one
/// keyframe of `key_attr` (`layers`: its layer, `now`: its comp time).
pub(crate) fn transform_value(
    cx: &PanelContext<'_>,
    comp: &PrimPath,
    layers: &[PrimPath],
    now: bae_model::Time,
    kind: TransformKind,
    key_attr: Option<&str>,
) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    if let Some(v) = TransformValue::new(cx.stage, comp, layers, now, kind, key_attr) {
        d.open = Some(Dialog::TransformValue(Box::new(v)));
    }
}

/// Open the mask value dialog of `kind` for `masks` at comp time `now`.
pub(crate) fn mask_value(
    cx: &PanelContext<'_>,
    comp: &PrimPath,
    masks: &[PrimPath],
    now: bae_model::Time,
    kind: MaskKind,
) {
    let dialogs = shared(cx.resources);
    let mut d = dialogs.lock();
    if d.open.is_some() {
        return;
    }
    if let Some(v) = MaskValue::new(cx.stage, comp, masks, now, kind) {
        d.open = Some(Dialog::MaskValue(Box::new(v)));
    }
}

/// AE's initial New Solid color (tests).
#[cfg(test)]
pub(crate) fn solid_settings_default_color() -> [f64; 4] {
    solid_settings::DEFAULT_COLOR
}

/// English strings of the dialogs (i18n completeness test).
#[cfg(test)]
pub(crate) fn strings() -> Vec<&'static str> {
    [
        comp_settings::STRINGS,
        solid_settings::STRINGS,
        camera_settings::STRINGS,
        light_settings::STRINGS,
        precompose::STRINGS,
        interpret_footage::STRINGS,
        layer_time::STRINGS,
        go_to_time::STRINGS,
        project_settings::STRINGS,
        transform_value::STRINGS,
        mask_value::STRINGS,
        render_output::STRINGS,
    ]
    .concat()
}

/// Draws the open motion dialog (registered in `Registry<dyn DialogHost>`).
#[derive(Debug, Default)]
pub struct MotionDialogs;

impl bae_ui::DialogHost for MotionDialogs {
    fn is_open(&self, resources: &Resources) -> bool {
        shared(resources).is_open()
            || crate::common::session(resources)
                .lock()
                .loading_lut
                .is_some()
    }

    fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>) {
        if crate::effects::lut::poll(ctx, cx) {
            return;
        }
        let dialogs = shared(cx.resources);
        // Draw without holding the lock (a dialog may read the memory).
        let open = {
            let mut d = dialogs.lock();
            d.frame_checks.repaint_while_running(ctx);
            d.open
                .take()
                .or_else(|| d.frame_checks.next_notice())
                .or_else(|| {
                    d.photoshop_queue
                        .pop_front()
                        .map(|job| Dialog::Photoshop(Box::new(job)))
                })
        };
        let Some(mut open) = open else { return };
        let session = crate::common::session(cx.resources);
        let keep = match &mut open {
            Dialog::Photoshop(d) => d.ui(ctx, cx),
            Dialog::AudioAnalysis(d) => d.ui(ctx, cx),
            Dialog::AudioOptions(d) => d.ui(ctx),
            Dialog::CompSettings(d) => d.ui(ctx, cx, &session, &dialogs),
            Dialog::SolidSettings(d) => d.ui(ctx, cx, &session, &dialogs),
            Dialog::CameraSettings(d) => d.ui(ctx, cx, &session, &dialogs),
            Dialog::LightSettings(d) => d.ui(ctx, cx, &session, &dialogs),
            Dialog::Precompose(d) => d.ui(ctx, cx, &session, &dialogs),
            Dialog::InterpretFootage(d) => d.ui(ctx, cx),
            Dialog::TimeStretch(d) => d.ui(ctx, cx),
            Dialog::AutoOrient(d) => d.ui(ctx, cx),
            Dialog::GoToTime(d) => d.ui(ctx, cx, &session),
            Dialog::ProjectSettings(d) => d.ui(ctx, cx),
            Dialog::EffectManager(d) => d.ui(ctx, cx),
            Dialog::TransformValue(d) => d.ui(ctx, cx),
            Dialog::MaskValue(d) => d.ui(ctx, cx),
            Dialog::RenderSettings(d) => d.ui(ctx, cx),
            Dialog::OutputModule(d) => d.ui(ctx, cx),
            Dialog::Alert { title, message } => alert_ui(ctx, title, message),
            Dialog::ViewOptions(v) => crate::viewer::view_options::view_options_ui(ctx, cx, v),
            Dialog::BackgroundColor(c) => {
                crate::viewer::view_options::background_color_ui(ctx, cx, *c)
            }
            Dialog::CustomResolution { steps, nested } => {
                crate::viewer::view_options::custom_resolution_ui(ctx, cx, steps, *nested)
            }
        };
        let mut lock = dialogs.lock();
        if keep && lock.open.is_none() {
            lock.open = Some(open);
        }
    }
}

mod zh;
pub(crate) use zh::ZH;

/// Registers this module's Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    bae_ui::i18n::register(photoshop::ZH);
}
