//! Motion workspace session state shared by its panels: the active comp, the current
//! time of every open comp, selections, playback and live drag previews.
//!
//! This is view state (what AE keeps per session: CTI, selection, open comps), not
//! project data; nothing here is saved in the project or synchronized. Project data
//! changes only through commands ([`bae_ui::PanelContext::invoke_with`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use bae_model::{FrameRate, PrimPath, Stage, Time, TimeRange, Value};
use bae_motion::KeyRef;
use bae_motion::query::{self, CompInfo};

/// What the timeline reveals under a layer (AE U / UU / P S R T A).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// U: properties with keyframes or expressions.
    Animated,
    /// UU: properties whose value differs from the default.
    Modified,
    /// P, S, R, T, A (Shift adds): exactly these layer attributes.
    Attrs(Vec<String>),
    /// Exactly these properties (Reveal Expression Errors).
    Props(Vec<(PrimPath, String)>),
}

/// Window > Motion Sketch, armed by Start Capture.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchArm {
    pub layer: PrimPath,
    /// Capture speed, percent of real time.
    pub speed: f64,
    /// Smoothing: how far (pixels) the recorded path may stray from the kept
    /// keyframes.
    pub smoothing: f64,
}

/// The prim of a Timeline group row (`layer#/…/Rectangle_1`, `…#animator`), if
/// it stands for one (not Contents, Transform …).
pub fn group_prim(id: &str) -> Option<PrimPath> {
    for suffix in ["#animator", "#selector"] {
        if let Some(p) = id.strip_suffix(suffix) {
            return PrimPath::new(p).ok();
        }
    }
    let (_, rest) = id.split_once('#')?;
    (rest.starts_with('/') && !rest.contains('#'))
        .then(|| PrimPath::new(rest).ok())
        .flatten()
}

/// The top-level shape groups of the shape layer `layer`.
pub(crate) fn shape_groups(stage: &Stage, layer: &PrimPath) -> Vec<PrimPath> {
    layer.child(query::CONTENTS).map_or_else(
        |_| Vec::new(),
        |contents| {
            stage
                .children(&contents)
                .into_iter()
                .filter(|g| {
                    stage
                        .prim(g)
                        .is_some_and(|v| v.is_a(&bae_model::PrimType::SHAPE_GROUP))
                })
                .collect()
        },
    )
}

/// Real-time playback of one comp.
#[derive(Clone, Debug)]
pub struct Playback {
    pub comp: PrimPath,
    started: Instant,
    /// The frame shown and the frame this run started from (comp frames).
    frame: i64,
    start: i64,
    rate: FrameRate,
    /// Frames per second of the clock: Preview ▸ Frame Rate (the comp's on Auto).
    fps: f64,
    ordinal: u64,
    /// The work area this run was started for: editing it restarts playback.
    work_area: TimeRange,
    revision: Option<u64>,
    /// Range played (Preview panel ▸ Range); playback loops or stops at its end.
    pub range: TimeRange,
    /// The current time before playing (kept on stop when Move Time To
    /// Preview Time is off).
    before: Time,
    /// When playback was asked for: the pre-roll waits a little at most.
    requested: Instant,
}

/// Longest the clock waits for playback's first frame (a cold start).
const PREROLL_MAX: Duration = Duration::from_millis(250);

impl Playback {
    fn ordinal_at(&self, now: Instant) -> u64 {
        (now.saturating_duration_since(self.started).as_secs_f64() * self.fps) as u64
    }

    fn frame_range(&self) -> (i64, i64) {
        let first = self.rate.time_to_frame(self.range.start);
        let last = self
            .rate
            .time_to_frame(Time(self.range.end.0 - 1))
            .max(first);
        (first, last)
    }

    fn deadline(&self) -> Instant {
        self.started + Duration::from_secs_f64((self.ordinal + 1) as f64 / self.fps)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PlaybackStats {
    pub fps: f64,
    pub dropped: u64,
    drawn: Option<Time>,
    next_source: u64,
    sample: Option<Instant>,
    frames: u32,
}

/// Preview panel ▸ Range (AE's choices for the Spacebar shortcut).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewRange {
    WorkArea,
    #[default]
    WorkAreaExtended,
    EntireDuration,
}

/// The Preview panel's playback options (AE: per shortcut; BAE: Spacebar's).
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewOptions {
    /// Prefer an open Active Camera view when previewing a 3D composition.
    pub favor_active_camera: bool,
    pub range: PreviewRange,
    /// Play From: the current time (else the start of the range).
    pub from_current: bool,
    /// Frame Rate: `None` = Auto (the comp's).
    pub fps: Option<f64>,
    /// Frames skipped between the frames played.
    pub skip: u32,
    /// Resolution while playing: `None` = Auto (the viewer's), else 1 (Full),
    /// 2 (Half), 3 (Third), 4 (Quarter).
    pub resolution: Option<u32>,
    /// On stop, the current time moves to the last frame played.
    pub move_time: bool,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        PreviewOptions {
            favor_active_camera: true,
            range: PreviewRange::default(),
            from_current: true,
            fps: None,
            skip: 0,
            resolution: None,
            move_time: true,
        }
    }
}

/// A value being dragged in a panel, previewed by the viewer before the command runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Scrub {
    pub auto_key: bool,
    pub path: PrimPath,
    pub attr: String,
    pub value: Value,
    /// The same property on the other selected layers (or selected across
    /// layers), changing with it: path and its value.
    pub peers: Vec<(PrimPath, Value)>,
    /// Comp time of the drag (a keyframe there when the property is animated).
    pub time: Time,
    /// The drag ended and its command was sent: still shown until it runs
    /// (the stage revision then changes), so the panels never flash the old
    /// value. With the revision it was sent at.
    pub committed: Option<(u64, Instant)>,
}

impl Scrub {
    /// Every prim the drag changes, with its value.
    pub fn targets(&self) -> impl Iterator<Item = (&PrimPath, &Value)> {
        std::iter::once((&self.path, &self.value)).chain(self.peers.iter().map(|(p, v)| (p, v)))
    }

    /// What the evaluators show while the drag lasts: each target's attribute
    /// as the commit will leave it ([`bae_motion::keys::spec_with_value_at`]).
    pub fn overrides(&self, stage: &Stage) -> Vec<bae_anim::AttrOverride> {
        self.targets()
            .filter(|(p, _)| stage.has_prim(p))
            .map(|(p, v)| bae_anim::AttrOverride {
                prim: p.clone(),
                attr: self.attr.clone(),
                spec: bae_motion::keys::spec_with_value_at(
                    stage,
                    p,
                    &self.attr,
                    query::to_key_time(stage, p, self.time),
                    v.clone(),
                    self.auto_key,
                ),
            })
            .collect()
    }

    /// The previewed value of `attr` of `path`, if the drag changes it.
    pub fn value_for(&self, path: &PrimPath, attr: &str) -> Option<&Value> {
        if self.attr != attr {
            return None;
        }
        self.targets().find(|(p, _)| *p == path).map(|(_, v)| v)
    }
}

/// The properties a keyframe drag changes, as they will be when it ends.
#[derive(Clone, Debug)]
pub struct KeyPreview {
    /// The panel mode dragging ("graph", "bars"): only it clears an
    /// unfinished preview.
    pub owner: &'static str,
    pub specs: Vec<bae_anim::AttrOverride>,
    /// The drag ended and its command was sent (stage revision then): shown
    /// until it has run, like [`Scrub::committed`].
    pub committed: Option<(u64, Instant)>,
}

/// Copied keyframes (one attribute at a time, like AE's paste target).
#[derive(Clone, Debug)]
pub struct KeyClipboard {
    pub attr: String,
    pub keys: Vec<bae_motion::CopiedKey>,
}

/// Something a command is about to create, selected once it exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Open the comp in the viewer and timeline.
    Comp(PrimPath),
    /// Select the layer (and open its comp).
    Layer(PrimPath),
    /// Select the layers a command adds to the comp (duplicate, split): those
    /// not in `before`.
    NewLayers {
        /// The comp.
        comp: PrimPath,
        /// Its layers before the command.
        before: Vec<PrimPath>,
    },
    /// Select the effects a command adds to layers (Effect menu, Effects &
    /// Presets): those not in `before`.
    NewEffects {
        /// The layers.
        layers: Vec<PrimPath>,
        /// Their effects before the command.
        before: Vec<PrimPath>,
    },
    /// Open the comps a command creates (those not in `before`; New Comp
    /// from Selection) and, with `queue`, add them to the Render Queue.
    NewComps { before: Vec<PrimPath>, queue: bool },
    /// Select the shape group a command adds to a shape layer (AE: a shape
    /// tool's new group is selected, its box shown): the first of `layer`'s
    /// groups not in `before`.
    NewGroup {
        /// The shape layer.
        layer: PrimPath,
        /// Its groups before the command.
        before: Vec<PrimPath>,
    },
    /// Keep these keys selected although they do not exist yet: the selection
    /// already names where a queued command (a keyframe drag) puts them, and
    /// panels drawn before it runs must not drop them (#551).
    Keys(Vec<KeyRef>),
}

#[derive(Debug, Default)]
pub struct Session {
    pub(crate) loading_lut: Option<crate::effects::lut::Load>,
    /// Automatically key animatable UI value edits.
    pub auto_key: bool,
    pub(crate) held_viewer_time: Option<(PrimPath, Time)>,
    pub(crate) channel_frame: Option<Arc<crate::gpu_preview::ChannelFrame>>,
    /// Last UI frame in which a visible timeline requested per-layer CPU timings.
    pub(crate) timing_column_frame: Option<u64>,
    pub(crate) layer_times: HashMap<PrimPath, std::time::Duration>,
    /// Comp shown in the viewer and timeline.
    pub active_comp: Option<PrimPath>,
    /// Comps opened in the timeline (tabs), in opening order.
    pub open_comps: Vec<PrimPath>,
    /// The Composition Navigator's path: the active comp and the comps it was
    /// opened through (double-clicking pre-comp layers), the containing comp
    /// first.
    pub(crate) comp_flow: Vec<PrimPath>,
    pub(crate) mini_flow: Option<crate::timeline::navigation::MiniFlow>,
    times: HashMap<PrimPath, Time>,
    pub selected_layers: Vec<PrimPath>,
    pub selected_keys: Vec<KeyRef>,
    /// Properties selected in the timeline outline (`path`, `attr`).
    pub selected_props: Vec<(PrimPath, String)>,
    /// The mask or shape path whose vertices the Composition viewer has
    /// selected (Ctrl+T then runs Free Transform Points, AE).
    pub(crate) vertex_path: Option<PrimPath>,
    /// Selected group rows of the Timeline (their row ids, e.g. a shape group,
    /// mask, effect or text animator; [`group_prim`] gives the prim).
    pub selected_groups: Vec<String>,
    pub(crate) reveal_tracker: Option<PrimPath>,
    /// Effects selected in Effect Controls: the viewer draws their point
    /// controls (AE).
    pub selected_effects: Vec<PrimPath>,
    /// Latest request to re-evaluate a generated source without editing project data.
    pub(crate) solid_reload: Option<(u64, PrimPath)>,
    /// The Graph Editor Set: properties whose graph switch is on (`path`, `attr`).
    pub graph_set: Vec<(PrimPath, String)>,
    /// The Timeline shows the Graph Editor instead of layer bars (Shift+F3).
    pub graph_editor: bool,
    /// The open keyframe dialog (Keyframe Interpolation / Velocity).
    pub key_dialog: Option<crate::timeline::KeyDialog>,
    /// Project panel selection.
    pub selected_items: Vec<PrimPath>,
    pub playback: Option<Playback>,
    pub(crate) playback_stats: PlaybackStats,
    pub(crate) output: Option<bae_media::playback::AudioOutput>,
    output_config: bae_media::playback::OutputConfig,
    output_devices: Arc<bae_media::playback::OutputDevices>,
    refreshing_devices: bool,
    audio_mapping: (bool, bool),
    pub(crate) master_db: f32,
    retry_audio_after: Option<Instant>,
    opening_audio: bool,
    audio_clock: bool,
    last_scrub: Option<Time>,
    mix_revision: Option<(PrimPath, u64)>,
    mix_has_audio: bool,
    /// Loop playback (Preview panel).
    pub loop_playback: bool,
    /// Preview panel options (range, play from, frame rate, skip …).
    pub preview: PreviewOptions,
    /// The viewer's resolution is on Auto: it follows the magnification. The
    /// chosen resolution itself is the comp's (`resolution`, as in Composition
    /// Settings).
    pub auto_resolution: bool,
    /// View ▸ rulers, guides, grid and safe areas (the viewer's options).
    pub(crate) view_options: crate::viewer::guides::ViewOptions,
    /// Each comp's 3D views (View ▸ Switch 3D View).
    pub(crate) views3d: HashMap<PrimPath, crate::viewer::view3d::Views>,
    /// What F10, F11 and F12 switch to (View ▸ Assign 3D View Shortcut).
    pub(crate) view_shortcuts: crate::viewer::view3d::Shortcuts,
    /// View ▸ View Options: the layer controls the viewers draw.
    pub(crate) layer_controls: crate::viewer::view_options::LayerControls,
    /// View ▸ Panel Background Color.
    pub(crate) panel_background: crate::viewer::view_options::PanelBackground,
    /// The layer the Layer panel shows (opened from the Timeline).
    pub(crate) layer_panel: Option<PrimPath>,
    pub(crate) open_liquify: Option<PrimPath>,
    pub(crate) liquify_tool: usize,
    /// An expression just added (Alt+click on the stopwatch, Animation ▸ Add
    /// Expression): the Timeline focuses its field with the text selected, so
    /// typing replaces it (AE). Dropped after a second if never shown.
    pub(crate) focus_expression: Option<(PrimPath, String, Instant)>,
    /// A project item to rename in the Project panel (the Timeline's Rename
    /// on empty space renames its comp there, AE 26).
    pub(crate) rename_item: Option<PrimPath>,
    /// Frames in the GPU preview cache (the timeline's green bar): comp,
    /// stage revision, cached spans.
    pub(crate) cached_preview: Option<(PrimPath, u64, Vec<TimeRange>)>,
    pub reveal: HashMap<PrimPath, Reveal>,
    pub(crate) reveal_style_groups: Vec<String>,
    pub scrub: Option<Scrub>,
    /// A keyframe drag in progress (Graph Editor, layer bars): the dragged
    /// properties as the release will leave them, evaluated live like a value
    /// drag (#593).
    pub key_preview: Option<KeyPreview>,
    pub clipboard: Option<KeyClipboard>,
    /// Layers on the clipboard (Edit > Copy with layers selected); the last
    /// copy wins over [`Session::clipboard`].
    pub layer_clipboard: Vec<bae_motion::CopiedLayer>,
    /// Layer contents on the clipboard (Copy with groups selected); the last copy
    /// wins over the other clipboards.
    pub content_clipboard: Vec<bae_motion::CopiedContent>,
    /// Copy Expression Only on a property: Paste puts it on the selected
    /// properties (AE).
    pub(crate) expression_clipboard: Option<String>,
    pending: Vec<(Pending, u32)>,
    /// New comps to add to the Render Queue (the dialogs host takes them).
    pub(crate) queue_comps: Vec<PrimPath>,
    /// Names of items commands are about to create (path, name), so default
    /// names given in the same frame stay unique.
    reserved: Vec<(PrimPath, String)>,
    /// When U was last pressed (UU detection).
    pub(crate) last_u: Option<Instant>,
    /// When L was last pressed to reveal the audio waveform on a second press.
    pub(crate) last_l: Option<Instant>,
    /// When M was last pressed (MM: every mask property).
    pub(crate) last_m: Option<Instant>,
    /// When E was last pressed (EE: the expressions).
    pub(crate) last_e: Option<Instant>,
    /// Motion Sketch armed (Start Capture): the next drag in the Composition
    /// panel records this layer's position.
    pub motion_sketch: Option<SketchArm>,
    /// What the viewer reports to the Info panel.
    pub info: crate::ae_panels::info::Readout,
    /// Timeline actions asked for while another panel had focus (F4 Switches /
    /// Modes, Shift+F4 Parent & Link, Select All Visible Keyframes …): the
    /// Timeline runs them on its next frame.
    pub timeline_toggles: Vec<&'static str>,
    /// The frame the Composition viewer shows and its key (Lumetri Scopes).
    pub scope_frame: Option<(bae_gpu::wgpu::Texture, u64)>,
    /// Working pixels before the display conversion, when a scope requests them.
    pub scope_source: Option<bae_render::GpuImage>,
    /// The Character panel's settings for new text (set while no text layer is
    /// selected, as in AE; `None`: the defaults).
    pub text_defaults: Option<bae_model::TextDocument>,
    /// Viewer selection at a specific source-text frame, retained across panel focus changes.
    pub(crate) text_selection: Option<(PrimPath, Time, std::ops::Range<usize>)>,
    /// The footage item the Footage panel shows.
    pub footage_view: Option<PrimPath>,
    /// Composition displayed at the root of the Flowchart panel.
    pub flowchart_root: Option<PrimPath>,
    /// Expression errors of the active comp at the current time (the viewer's
    /// evaluation): the error banner and the timeline's warning icons.
    pub expr_errors: Vec<bae_anim::AttrError>,
}

/// The session as an app resource (`Resources::get::<SharedSession>()`).
#[derive(Clone, Debug)]
pub struct SharedSession(Arc<Mutex<Session>>);

impl Default for SharedSession {
    fn default() -> Self {
        SharedSession(Arc::new(Mutex::new(Session {
            loop_playback: true,
            ..Session::default()
        })))
    }
}

impl SharedSession {
    pub fn lock(&self) -> MutexGuard<'_, Session> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A new or opened project in the same window: fresh view state on this
    /// same session. Panels, the script host and the audio warm-up all reach
    /// the session through it, so the audio output (an application device)
    /// stays open; the loop setting is the user's, not the project's.
    pub fn replace_project(&self) {
        let mut s = self.lock();
        s.stop();
        let fresh = Session {
            output: s.output.take(),
            output_config: std::mem::take(&mut s.output_config),
            output_devices: std::mem::take(&mut s.output_devices),
            refreshing_devices: s.refreshing_devices,
            audio_mapping: s.audio_mapping,
            retry_audio_after: s.retry_audio_after.take(),
            opening_audio: s.opening_audio,
            loop_playback: s.loop_playback,
            ..Session::default()
        };
        *s = fresh;
    }

    /// Called on a background thread. Device negotiation may block in the OS.
    fn open_audio(&self) -> Result<(), String> {
        bae_gpu::warm_up::lower_priority();
        let devices = bae_media::playback::OutputDevices::enumerate();
        self.lock().output_devices = Arc::new(devices);
        loop {
            let config = self.lock().output_config.clone();
            let device = bae_media::playback::AudioOutput::open_with(&config);
            let mut s = self.lock();
            if s.output_config != config {
                drop(s);
                continue;
            }
            s.opening_audio = false;
            return match device {
                Ok(output) => {
                    output.set_channel_mapping(s.audio_mapping.0, s.audio_mapping.1);
                    output.set_gain(10.0_f32.powf(s.master_db / 20.0));
                    let previous = s.output.replace(output);
                    s.mix_revision = None;
                    s.retry_audio_after = None;
                    drop(s);
                    drop(previous);
                    Ok(())
                }
                Err(error) => {
                    let previous = s.output.take();
                    s.audio_clock = false;
                    s.mix_revision = None;
                    s.retry_audio_after = Some(Instant::now() + std::time::Duration::from_secs(2));
                    drop(s);
                    drop(previous);
                    Err(error)
                }
            };
        }
    }

    /// Reconnect when committed audio preferences change. Keep driver calls off the UI thread.
    pub fn configure_output(&self, device_id: &str, latency_ms: u32) {
        let mut s = self.lock();
        if s.output_config.device_id == device_id && s.output_config.latency_ms == latency_ms {
            return;
        }
        s.output_config = bae_media::playback::OutputConfig {
            device_id: device_id.to_owned(),
            latency_ms,
        };
        if s.opening_audio {
            return;
        }
        s.opening_audio = true;
        drop(s);
        self.spawn_audio_worker();
    }

    /// Refresh the device picker when opened after a device was plugged in.
    pub fn refresh_output_devices(&self) {
        let mut s = self.lock();
        if s.refreshing_devices || s.opening_audio {
            return;
        }
        s.refreshing_devices = true;
        drop(s);
        let session = self.clone();
        if std::thread::Builder::new()
            .name("bae-audio-endpoints".into())
            .spawn(move || {
                bae_gpu::warm_up::lower_priority();
                let devices = Arc::new(bae_media::playback::OutputDevices::enumerate());
                let mut s = session.lock();
                s.output_devices = devices;
                s.refreshing_devices = false;
            })
            .is_err()
        {
            self.lock().refreshing_devices = false;
        }
    }

    /// Opens the output device on its own thread (OS negotiation may block). It
    /// needs no GPU. Does nothing while a device is open or opening.
    pub(crate) fn start_audio(&self) {
        {
            let mut s = self.lock();
            if s.opening_audio
                || s.output
                    .as_ref()
                    .is_some_and(|output| !output.device_lost())
            {
                return;
            }
            s.opening_audio = true;
        }
        self.spawn_audio_worker();
    }

    fn spawn_audio_worker(&self) {
        let session = self.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("bae-audio-device".into())
            .spawn(move || {
                if let Err(error) = session.open_audio() {
                    tracing::warn!(%error, "Audio output unavailable");
                }
            })
        {
            self.lock().opening_audio = false;
            tracing::warn!(%error, "Could not start the audio device worker");
        }
    }

    /// A user can connect a device after starting the application.
    pub(crate) fn retry_audio(&self) {
        let needed = {
            let s = self.lock();
            match s.retry_audio_after {
                Some(at) => Instant::now() >= at,
                // Never opened or lost: open it now rather than wait for a failure.
                None => s
                    .output
                    .as_ref()
                    .is_none_or(bae_media::playback::AudioOutput::device_lost),
            }
        };
        if needed {
            self.start_audio();
        }
    }
}

/// Module setup already started the device. As the first warm-up task this
/// only reopens a failed or lost one, on its own thread, so it never holds up
/// the GPU tasks.
impl bae_gpu::WarmUp for SharedSession {
    fn name(&self) -> &str {
        "audio output"
    }

    fn priority(&self) -> u8 {
        0
    }

    fn run(&self, _gpu: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        self.start_audio();
        Ok(())
    }
}

/// Frames to wait for a pending item before giving up (the command failed).
const PENDING_FRAMES: u32 = 30;

impl Session {
    /// What panels evaluate in place of the stage while something is dragged:
    /// the value drag ([`Scrub`]) and the keyframe drag ([`KeyPreview`]).
    pub fn live_overrides(&self, stage: &Stage) -> Vec<bae_anim::AttrOverride> {
        let mut out: Vec<bae_anim::AttrOverride> = self
            .key_preview
            .iter()
            .flat_map(|k| k.specs.iter().cloned())
            .filter(|o| stage.has_prim(&o.prim))
            .collect();
        if let Some(s) = &self.scrub {
            for o in s.overrides(stage) {
                out.retain(|x| !(x.prim == o.prim && x.attr == o.attr));
                out.push(o);
            }
        }
        out
    }

    /// The latest background enumeration for the audio preferences.
    pub fn output_devices(&self) -> &bae_media::playback::OutputDevices {
        &self.output_devices
    }
    /// The device worker will publish an updated stream or picker.
    pub fn audio_device_pending(&self) -> bool {
        self.opening_audio || self.refreshing_devices
    }
    /// Device-reported buffer duration of the active output stream.
    pub fn audio_buffer_ms(&self) -> Option<f64> {
        let output = self.output.as_ref().filter(|o| !o.device_lost())?;
        Some(1000.0 * f64::from(output.buffer_frames()?) / f64::from(output.sample_rate()))
    }
    /// Apply the channel mapping to the active stream and any replacement device.
    pub fn set_audio_mapping(&mut self, left_to_second: bool, right_to_first: bool) {
        self.audio_mapping = (left_to_second, right_to_first);
        if let Some(output) = &self.output {
            output.set_channel_mapping(left_to_second, right_to_first);
        }
    }

    /// The warmed device the preview is currently using.
    pub fn audio_output_info(&self) -> Option<(&str, u16, u32)> {
        let output = self.output.as_ref().filter(|o| !o.device_lost())?;
        let (name, channels) = output.device_info();
        Some((name, channels, output.sample_rate()))
    }

    pub(crate) fn text_range(&self, layer: Option<&PrimPath>) -> Option<std::ops::Range<usize>> {
        let (owner, time, range) = self.text_selection.as_ref()?;
        (Some(owner) == layer
            && *time == self.active_time()
            && self.selected_layers.as_slice() == std::slice::from_ref(owner))
        .then(|| range.clone())
    }

    /// Current time of `comp` (comp time, on a frame boundary).
    pub fn time(&self, comp: &PrimPath) -> Time {
        self.times.get(comp).copied().unwrap_or(Time::ZERO)
    }

    /// Current time of the active comp.
    pub fn active_time(&self) -> Time {
        self.active_comp
            .as_ref()
            .map_or(Time::ZERO, |c| self.time(c))
    }

    /// Move the current time of `comp`, snapped to a frame and kept inside the comp.
    pub fn set_time(&mut self, info: &CompInfo, t: Time) {
        let at = self.update_time(info, t);
        let playing = self
            .playback
            .as_ref()
            .filter(|pb| pb.comp == info.path)
            .map(|pb| pb.range);
        match playing {
            Some(range) if range.contains(at) => {
                self.restart_clock(at);
                if self.audio_clock {
                    self.audio_clock = self.sound(at, range, self.loop_playback);
                }
            }
            Some(_) => self.stop(),
            None => {}
        }
    }

    /// Restart the frame clock at `t`: playback continues from there.
    /// Pre-roll: while playback has not left its first frame and the viewer
    /// cannot show that frame yet (a cold start right after a seek), the
    /// clock waits for it ([`PREROLL_MAX`] at most) instead of skipping it.
    /// Sound drives its own clock and is not held.
    pub(crate) fn hold_first_frame(&mut self, ready: bool) {
        if self.audio_clock {
            return;
        }
        if let Some(pb) = &mut self.playback
            && !ready
            && pb.ordinal == 0
            && pb.requested.elapsed() < PREROLL_MAX
        {
            pb.started = Instant::now();
        }
    }

    fn restart_clock(&mut self, t: Time) {
        if let Some(pb) = &mut self.playback {
            pb.frame = pb.rate.time_to_frame(t);
            pb.start = pb.frame;
            pb.started = Instant::now();
            pb.ordinal = 0;
            self.playback_stats = PlaybackStats {
                sample: Some(pb.started),
                ..Default::default()
            };
        }
    }

    fn update_time(&mut self, info: &CompInfo, t: Time) -> Time {
        let at = info.playhead_time(t);
        self.times.insert(info.path.clone(), at);
        at
    }

    /// Make `comp` the active comp (opening a timeline tab for it).
    pub fn open_comp(&mut self, comp: &PrimPath) {
        if self.active_comp.as_ref() != Some(comp) {
            self.mini_flow = None;
            self.selected_layers.clear();
            self.selected_keys.clear();
            self.selected_props.clear();
            self.stop();
        }
        if !self.open_comps.contains(comp) {
            self.open_comps.push(comp.clone());
        }
        // A comp on the navigator's path keeps the path; any other starts one.
        if !self.comp_flow.contains(comp) {
            self.comp_flow = vec![comp.clone()];
        }
        self.active_comp = Some(comp.clone());
    }

    /// Open `comp` from a pre-comp layer in `parent`: the navigator's path runs
    /// through `parent` to `comp`.
    pub fn open_from(&mut self, parent: &PrimPath, comp: &PrimPath) {
        match self.comp_flow.iter().position(|c| c == parent) {
            Some(i) => self.comp_flow.truncate(i + 1),
            None => self.comp_flow = vec![parent.clone()],
        }
        self.comp_flow.push(comp.clone());
        self.open_comp(comp);
    }

    /// Close a timeline tab.
    pub fn close_comp(&mut self, comp: &PrimPath) {
        self.open_comps.retain(|c| c != comp);
        if self.active_comp.as_ref() == Some(comp) {
            self.mini_flow = None;
            self.active_comp = self.open_comps.last().cloned();
            self.selected_layers.clear();
            self.selected_keys.clear();
            self.selected_props.clear();
            self.stop();
        }
    }

    /// Select something once a command has created it.
    pub fn expect(&mut self, what: Pending) {
        self.pending.push((what, 0));
    }

    /// Keep `name` taken until the command creating `path` has run.
    pub fn reserve_name(&mut self, path: PrimPath, name: String) {
        self.reserved.push((path, name));
    }

    /// Names reserved for items about to be created under `parent`.
    pub fn reserved_names<'a>(&'a self, parent: &'a PrimPath) -> impl Iterator<Item = String> + 'a {
        self.reserved
            .iter()
            .filter(move |(p, _)| p.parent().as_ref() == Some(parent))
            .map(|(_, n)| n.clone())
    }

    /// Drop references to prims that no longer exist and resolve pending items.
    /// Called once per frame by the panels (cheap).
    pub fn sync(&mut self, stage: &Stage) {
        if let Some((revision, at)) = self.key_preview.as_ref().and_then(|k| k.committed)
            && (stage.revision() != revision || at.elapsed() > Duration::from_millis(500))
        {
            self.key_preview = None;
        }
        if let Some((revision, at)) = self.scrub.as_ref().and_then(|s| s.committed)
            && (stage.revision() != revision
                || at.elapsed() > std::time::Duration::from_millis(500))
        {
            self.scrub = None;
        }
        let pending = std::mem::take(&mut self.pending);
        for (what, age) in pending {
            if let Pending::NewComps { before, queue } = &what {
                let added: Vec<PrimPath> = query::comps(stage)
                    .into_iter()
                    .filter(|c| !before.contains(c))
                    .collect();
                if let Some(first) = added.first() {
                    self.open_comp(first);
                    if *queue {
                        self.queue_comps.extend(added);
                    }
                } else if age < PENDING_FRAMES {
                    self.pending.push((what, age + 1));
                }
                continue;
            }
            if let Pending::Keys(keys) = &what {
                let landed = keys.iter().all(|k| {
                    query::attr_spec(stage, &k.path, &k.attr)
                        .is_some_and(|s| s.keyframes.contains_key(&k.time))
                });
                if !landed && age < PENDING_FRAMES {
                    self.pending.push((what, age + 1));
                }
                continue;
            }
            if let Pending::NewEffects { layers, before } = &what {
                let added: Vec<PrimPath> = layers
                    .iter()
                    .flat_map(|l| bae_motion::effect::effects_of(stage, l))
                    .filter(|e| !before.contains(e))
                    .collect();
                if !added.is_empty() {
                    self.selected_effects = added;
                } else if age < PENDING_FRAMES {
                    self.pending.push((what, age + 1));
                }
                continue;
            }
            if let Pending::NewGroup { layer, before } = &what {
                match shape_groups(stage, layer)
                    .into_iter()
                    .find(|g| !before.contains(g))
                {
                    Some(g) => self.selected_groups = vec![format!("{layer}#{g}")],
                    None if age < PENDING_FRAMES => self.pending.push((what, age + 1)),
                    None => {}
                }
                continue;
            }
            if let Pending::NewLayers { comp, before } = &what {
                let added: Vec<PrimPath> = query::comp_layers(stage, comp)
                    .into_iter()
                    .filter(|l| !before.contains(l))
                    .collect();
                if !added.is_empty() {
                    self.selected_layers = added;
                    self.selected_keys.clear();
                } else if age < PENDING_FRAMES {
                    self.pending.push((what, age + 1));
                }
                continue;
            }
            let path = match &what {
                Pending::Comp(p) | Pending::Layer(p) => p,
                Pending::NewLayers { .. }
                | Pending::NewEffects { .. }
                | Pending::NewGroup { .. }
                | Pending::Keys(_)
                | Pending::NewComps { .. } => {
                    unreachable!("handled above")
                }
            };
            if stage.has_prim(path) {
                match what {
                    Pending::Comp(c) => self.open_comp(&c),
                    Pending::Layer(l) => {
                        if let Some(c) = query::owning_comp(stage, &l) {
                            self.open_comp(&c);
                        }
                        self.selected_layers = vec![l];
                        self.selected_keys.clear();
                    }
                    Pending::NewLayers { .. }
                    | Pending::NewEffects { .. }
                    | Pending::NewGroup { .. }
                    | Pending::Keys(_)
                    | Pending::NewComps { .. } => {}
                }
            } else if age < PENDING_FRAMES {
                self.pending.push((what, age + 1));
            }
        }
        // A reservation ends when its item exists or its command gave up.
        let waiting: Vec<PrimPath> = self
            .pending
            .iter()
            .filter_map(|(w, _)| match w {
                Pending::Comp(p) | Pending::Layer(p) => Some(p.clone()),
                Pending::NewLayers { .. }
                | Pending::NewEffects { .. }
                | Pending::NewGroup { .. }
                | Pending::Keys(_)
                | Pending::NewComps { .. } => None,
            })
            .collect();
        self.reserved
            .retain(|(p, _)| !stage.has_prim(p) && waiting.contains(p));
        self.open_comps
            .retain(|c| query::comp_info(stage, c).is_some());
        if self
            .active_comp
            .as_ref()
            .is_some_and(|c| query::comp_info(stage, c).is_none())
        {
            self.active_comp = self.open_comps.last().cloned();
            self.stop();
        }
        // AE: a locked layer cannot be selected, so locking one deselects it.
        if let Some((owner, time, _)) = &self.text_selection
            && (self.selected_layers.as_slice() != std::slice::from_ref(owner)
                || *time != self.active_time())
        {
            self.text_selection = None;
        }
        self.selected_layers
            .retain(|l| stage.has_prim(l) && !query::is_locked(stage, l));
        self.selected_items.retain(|l| stage.has_prim(l));
        let expected: Vec<&KeyRef> = self
            .pending
            .iter()
            .filter_map(|(w, _)| match w {
                Pending::Keys(keys) => Some(keys.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        let selected = std::mem::take(&mut self.selected_keys);
        self.selected_keys = selected
            .into_iter()
            .filter(|k| {
                expected.contains(&k)
                    || query::attr_spec(stage, &k.path, &k.attr)
                        .is_some_and(|s| s.keyframes.contains_key(&k.time))
            })
            .collect();
        self.reveal.retain(|l, _| stage.has_prim(l));
        self.selected_props
            .retain(|(p, _)| stage.has_prim(p) && query::locked_layer_of(stage, p).is_none());
        self.graph_set.retain(|(p, _)| stage.has_prim(p));
        self.selected_effects.retain(|e| stage.has_prim(e));
    }

    /// Keep the meters moving through the last audible scrub sample.
    pub(crate) fn monitoring_audio(&self) -> bool {
        (self.audio_clock || self.last_scrub.is_some())
            && self
                .output
                .as_ref()
                .is_some_and(|output| !output.finished())
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_some()
    }

    /// Playback follows the audio device's clock (it plays the comp's mix).
    #[cfg(test)]
    pub(crate) fn audio_clock(&self) -> bool {
        self.audio_clock
    }

    /// The device is being opened; an output device is open.
    #[cfg(test)]
    pub(crate) fn audio_device(&self) -> (bool, bool) {
        (self.opening_audio, self.output.is_some())
    }

    /// The range Preview ▸ Range plays for the current time `now`.
    pub fn preview_range(&self, info: &CompInfo, now: Time) -> TimeRange {
        let wa = info.work_area;
        match self.preview.range {
            PreviewRange::WorkArea => wa,
            // The work area, grown to take in the current time (AE).
            PreviewRange::WorkAreaExtended => {
                let end = (info.duration).max(info.frame_duration());
                let start = wa.start.min(now);
                let stop = if now >= wa.end {
                    (now + info.frame_duration()).min(end)
                } else {
                    wa.end
                };
                TimeRange::new(start, stop)
            }
            PreviewRange::EntireDuration => TimeRange::new(Time::ZERO, info.duration),
        }
    }

    /// Where playback of `info` would start and the range it plays (Preview
    /// panel options): the current time inside the range (not on its last
    /// frame) when playing from the current time, else the range's start.
    pub fn play_start(&self, info: &CompInfo) -> (Time, TimeRange) {
        let now = self.time(&info.path);
        let range = self.preview_range(info, now);
        let from = if self.preview.from_current
            && range.contains(now)
            && now + info.frame_duration() < range.end
        {
            now
        } else {
            range.start
        };
        (from, range)
    }

    /// Start playing the active comp (Preview panel options): from the current
    /// time or the start of the range, through the range.
    pub fn play(&mut self, info: &CompInfo) {
        let now = self.time(&info.path);
        let (from, range) = self.play_start(info);
        let started = Instant::now();
        self.times.insert(info.path.clone(), from);
        let frame = info.frame_rate.time_to_frame(from);
        self.playback = Some(Playback {
            comp: info.path.clone(),
            started,
            frame,
            start: frame,
            rate: info.frame_rate,
            fps: self
                .preview
                .fps
                .unwrap_or_else(|| info.frame_rate.as_f64())
                .max(1e-3),
            ordinal: 0,
            work_area: info.work_area,
            revision: None,
            range,
            before: now,
            requested: started,
        });
        self.playback_stats = PlaybackStats {
            sample: Some(started),
            ..Default::default()
        };
    }

    /// Another preview frame rate plays at another speed: the sound does not
    /// drive the clock then (AE plays it only at the comp's rate).
    fn sound_clocks(&self, info: &CompInfo) -> bool {
        self.preview
            .fps
            .is_none_or(|fps| (fps - info.frame_rate.as_f64()).abs() < 1e-3)
    }

    /// Refresh the producer's snapshot once per composition edit. Timeline seeks
    /// reuse the same producer, snapshot and open decoders.
    fn prepare_mix(&mut self, cx: &bae_ui::PanelContext<'_>, info: &CompInfo) -> bool {
        let Some(output) = &self.output else {
            return false;
        };
        let revision = cx
            .resources
            .get::<Arc<bae_ui::history::HistoryLog>>()
            .map_or(0, |h| h.last_seq());
        if self
            .mix_revision
            .as_ref()
            .is_some_and(|(comp, seq)| comp == &info.path && *seq == revision)
        {
            return false;
        }
        let mix = bae_media::mix::AudioMix::from_stage(cx.stage, &info.path);
        self.mix_has_audio = !mix.is_empty();
        output.prepare(mix);
        self.mix_revision = Some((info.path.clone(), revision));
        true
    }

    fn sound(&self, at: Time, range: TimeRange, repeat: bool) -> bool {
        if self.mix_has_audio
            && let Some(output) = &self.output
        {
            output.seek(at, range, repeat);
            return true;
        }
        false
    }

    pub(crate) fn preview(&mut self, cx: &bae_ui::PanelContext<'_>, info: &CompInfo) {
        self.prepare_mix(cx, info);
        self.play(info);
        let pb = self.playback.as_ref().unwrap();
        let at = pb.rate.frame_to_time(pb.frame);
        self.audio_clock = self.sound_clocks(info) && self.sound(at, pb.range, self.loop_playback);
    }

    pub(crate) fn scrub_to(
        &mut self,
        cx: &bae_ui::PanelContext<'_>,
        info: &CompInfo,
        time: Time,
        sound: bool,
    ) {
        if self.is_playing() || !sound {
            self.stop();
        }
        self.set_time(info, time);
        let at = self.time(&info.path);
        if sound && self.last_scrub != Some(at) {
            self.prepare_mix(cx, info);
            let end = (at + Time::from_seconds(0.08)).min(info.duration);
            if self.sound(at, TimeRange::new(at, end), false) {
                self.last_scrub = Some(at);
            }
        }
    }

    pub fn stop(&mut self) {
        if (self.audio_clock || self.last_scrub.is_some())
            && let Some(output) = &self.output
        {
            output.stop();
        }
        self.playback = None;
        self.last_scrub = None;
        self.audio_clock = false;
    }

    /// Keep the audio mix current (once per composition edit). While playing,
    /// a new mix restarts the device and the clock at the current frame: a late
    /// device or a live layer edit. The shell ticks the clock.
    pub(crate) fn update_preview(&mut self, cx: &bae_ui::PanelContext<'_>, info: &CompInfo) {
        if self.prepare_mix(cx, info)
            && let Some(pb) = &self.playback
        {
            let (from, range) = (pb.rate.frame_to_time(pb.frame), pb.range);
            self.restart_clock(from);
            self.audio_clock =
                self.sound_clocks(info) && self.sound(from, range, self.loop_playback);
        }
    }

    /// Stop playing (Spacebar): the current time stays on the last frame shown,
    /// or goes back when Move Time To Preview Time is off.
    pub fn stop_preview(&mut self, info: &CompInfo) {
        if let Some(pb) = self.playback.take()
            && !self.preview.move_time
            && pb.comp == info.path
        {
            self.set_time(info, pb.before);
        }
    }

    /// Edits to the work area or frame rate take effect without restarting the
    /// UI clock on unrelated project changes.
    pub fn sync_playback(&mut self, stage: &Stage) {
        let Some(pb) = self.playback.as_mut() else {
            return;
        };
        if pb.revision == Some(stage.revision()) {
            return;
        }
        pb.revision = Some(stage.revision());
        let Some(info) = query::comp_info(stage, &pb.comp) else {
            self.stop();
            return;
        };
        if pb.work_area != info.work_area || pb.rate != info.frame_rate {
            self.play(&info);
            if let Some(pb) = self.playback.as_mut() {
                pb.revision = Some(stage.revision());
                let (at, range) = (pb.rate.frame_to_time(pb.frame), pb.range);
                if self.audio_clock {
                    self.audio_clock = self.sound(at, range, self.loop_playback);
                }
            }
        }
    }

    pub fn playback_deadline(&self) -> Option<Instant> {
        self.playback.as_ref().map(Playback::deadline)
    }

    /// Advance once before building the workspace. Ordinals count scheduled
    /// frames across loops, even when the event loop wakes several frames late.
    /// While the device plays the mix it is the clock: the wall clock start is
    /// re-anchored to its elapsed time, so deadlines follow the audio too.
    pub fn tick(&mut self, now: Instant) {
        let Some(pb) = self.playback.as_mut() else {
            return;
        };
        let mut finished = false;
        if let Some(output) = self.output.as_ref().filter(|_| self.audio_clock) {
            output.set_loop(self.loop_playback);
            finished = output.finished();
            let elapsed = Duration::from_secs_f64(output.elapsed().as_seconds().max(0.));
            pb.started = now.checked_sub(elapsed).unwrap_or(now);
        }
        let ordinal = pb.ordinal_at(now).max(pb.ordinal);
        let stats = &mut self.playback_stats;
        stats.drawn = None;
        if ordinal == pb.ordinal && !finished {
            return;
        }
        let (first, last) = pb.frame_range();
        let delta = ordinal - pb.ordinal;
        let until_end = (last - pb.frame + 1) as u64;
        let previous_ordinal = pb.ordinal;
        pb.ordinal = ordinal;
        // The device stops by itself at the end of a range it does not loop.
        let ended = finished || (!self.loop_playback && delta >= until_end);
        pb.frame = if ended {
            last
        } else {
            first + (pb.frame - first + delta as i64).rem_euclid(last - first + 1)
        };
        // Skip: only every (skip + 1)th frame is shown.
        let step = i64::from(self.preview.skip) + 1;
        let shown = pb.frame - (pb.frame - pb.start).rem_euclid(step);
        self.times
            .insert(pb.comp.clone(), pb.rate.frame_to_time(shown.max(first)));
        if ended {
            stats.dropped += (previous_ordinal + until_end).saturating_sub(stats.next_source);
            self.stop();
        }
    }

    /// Called by the viewer for the actual texture used in this UI pass.
    pub(crate) fn drew_preview(&mut self, actual: Time) {
        self.playback_stats.drawn = Some(actual);
    }

    /// Count only frames submitted to the native presentation surface.
    pub fn frame_presented(&mut self, now: Instant) {
        let Some(pb) = &self.playback else { return };
        let stats = &mut self.playback_stats;
        if let Some(time) = stats.drawn.take() {
            let (first, last) = pb.frame_range();
            let behind = (pb.frame - pb.rate.time_to_frame(time)).rem_euclid(last - first + 1);
            // A late frame still counts when it reaches the surface. A gap is
            // final only after a newer source frame has actually been shown.
            if let Some(source) = pb.ordinal.checked_sub(behind as u64)
                && source >= stats.next_source
            {
                stats.dropped += source - stats.next_source;
                stats.next_source = source + 1;
                stats.frames += 1;
            }
        }
        let elapsed = now.duration_since(*stats.sample.get_or_insert(now));
        if elapsed >= Duration::from_millis(500) {
            stats.fps = f64::from(stats.frames) / elapsed.as_secs_f64();
            stats.frames = 0;
            stats.sample = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::FrameRate;

    fn info() -> CompInfo {
        CompInfo {
            path: PrimPath::new("/c").unwrap(),
            name: "c".into(),
            width: 100,
            height: 100,
            pixel_aspect: 1.0,
            frame_rate: FrameRate::FPS_30,
            duration: Time::from_seconds(2.0),
            work_area: TimeRange::new(Time::ZERO, Time::from_seconds(2.0)),
            bg_color: [0.0; 4],
            display_start: Time::ZERO,
        }
    }

    #[test]
    fn preview_range_play_from_and_stop_follow_the_preview_panel() {
        let mut info = info();
        info.work_area = TimeRange::new(Time::from_seconds(0.5), Time::from_seconds(1.0));
        let mut s = Session::default();
        let now = Time::from_seconds(1.5);
        // AE's default: the work area grown to the current time.
        let r = s.preview_range(&info, now);
        assert_eq!(r.start, Time::from_seconds(0.5));
        assert!(r.end > now);
        s.preview.range = PreviewRange::WorkArea;
        assert_eq!(s.preview_range(&info, now), info.work_area);
        s.preview.range = PreviewRange::EntireDuration;
        assert_eq!(s.preview_range(&info, now).end, info.duration);
        // Play From: Start of Range.
        s.preview.range = PreviewRange::WorkArea;
        s.preview.from_current = false;
        s.set_time(&info, Time::from_seconds(0.7));
        s.play(&info);
        assert_eq!(s.time(&info.path), Time::from_seconds(0.5));
        // Move time to preview time off: Spacebar goes back to where it was.
        s.preview.move_time = false;
        s.stop_preview(&info);
        assert!(!s.is_playing());
        assert_eq!(
            s.time(&info.path),
            info.frame_rate.snap(Time::from_seconds(0.7))
        );
    }

    #[test]
    fn time_is_snapped_and_clamped() {
        let mut s = Session::default();
        let i = info();
        s.set_time(&i, Time::from_seconds(0.51));
        assert_eq!(s.time(&i.path), FrameRate::FPS_30.frame_to_time(15));
        s.set_time(&i, Time::from_seconds(5.0));
        assert_eq!(s.time(&i.path), FrameRate::FPS_30.frame_to_time(59));
        s.set_time(&i, Time(-5));
        assert_eq!(s.time(&i.path), Time::ZERO);
    }

    #[test]
    fn playback_starts_in_work_area() {
        let mut s = Session::default();
        let mut i = info();
        i.work_area = TimeRange::new(Time::from_seconds(1.0), Time::from_seconds(2.0));
        s.preview.range = PreviewRange::WorkArea;
        s.play(&i);
        assert!(s.is_playing());
        assert_eq!(s.time(&i.path), Time::from_seconds(1.0));
        let seek = i.frame_rate.frame_to_time(42);
        s.set_time(&i, seek);
        assert_eq!(s.playback.as_ref().unwrap().frame, 42);
        s.set_time(&i, Time::ZERO);
        assert!(!s.is_playing());
    }

    #[test]
    fn presentation_counts_distinct_frames_across_lateness_loops_and_seeks() {
        let mut i = info();
        i.frame_rate = FrameRate::FPS_59_94;
        let time = |f| i.frame_rate.frame_to_time(f);
        i.work_area = TimeRange::new(Time::ZERO, time(3));
        let mut s = Session {
            loop_playback: true,
            ..Default::default()
        };
        s.play(&i);
        let start = s.playback.as_ref().unwrap().started;
        let next = s.playback_deadline().unwrap();
        s.tick(next - Duration::from_nanos(1));
        assert_eq!(s.time(&i.path), time(0));
        s.tick(next);
        // The first texture arrives after its deadline, then is presented twice.
        for _ in 0..2 {
            s.drew_preview(time(0));
            s.frame_presented(next);
        }
        assert_eq!((s.playback_stats.frames, s.playback_stats.dropped), (1, 0));
        s.tick(start + Duration::from_millis(51));
        assert_eq!(s.time(&i.path), time(0));
        s.drew_preview(time(0));
        s.frame_presented(start + Duration::from_millis(51));
        assert_eq!((s.playback_stats.frames, s.playback_stats.dropped), (2, 2));
        s.set_time(&i, time(1));
        let seek = s.playback.as_ref().unwrap().started;
        s.drew_preview(time(1));
        s.frame_presented(seek);
        assert_eq!((s.playback_stats.frames, s.playback_stats.dropped), (1, 0));
        s.loop_playback = false;
        s.tick(seek + Duration::from_secs(1));
        assert!(!s.is_playing());
        assert_eq!(s.time(&i.path), time(2));
        assert_eq!(s.playback_stats.dropped, 1);
    }

    /// Waits for a device open started by `retry_audio` (no device in CI).
    fn settle(shared: &SharedSession) {
        let start = Instant::now();
        while shared.lock().opening_audio && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn a_replaced_project_keeps_the_audio_device_and_play_opens_a_missing_one() {
        let shared = SharedSession::default();
        // Nothing opened yet and no failure recorded: Play opens the device.
        shared.retry_audio();
        settle(&shared);
        let opened = shared.lock().output.is_some();
        assert!(
            opened || shared.lock().retry_audio_after.is_some(),
            "an open was attempted"
        );
        shared.lock().open_comp(&PrimPath::new("/c").unwrap());
        shared.replace_project();
        let s = shared.lock();
        assert!(s.open_comps.is_empty());
        assert_eq!(s.output.is_some(), opened);
    }
}
