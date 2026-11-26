//! Motion workspace session state shared by its panels: the active comp, the current
//! time of every open comp, selections, playback and live drag previews.
//!
//! This is view state (what AE keeps per session: CTI, selection, open comps), not
//! project data; nothing here is saved in the project or synchronized. Project data
//! changes only through commands ([`bae_ui::PanelContext::invoke_with`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use bae_model::{PrimPath, Stage, Time, TimeRange, Value};
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

/// Real-time playback of one comp.
#[derive(Clone, Debug)]
pub struct Playback {
    pub comp: PrimPath,
    started: Instant,
    from: Time,
    /// Range played (Preview panel ▸ Range); playback loops or stops at its end.
    pub range: TimeRange,
    /// The current time before playing (kept on stop when Move Time To
    /// Preview Time is off).
    before: Time,
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
    pub path: PrimPath,
    pub attr: String,
    pub value: Value,
    /// The same property on the other selected layers (or selected across
    /// layers), changing with it: path and its value.
    pub peers: Vec<(PrimPath, Value)>,
}

impl Scrub {
    /// Every prim the drag changes, with its value.
    pub fn targets(&self) -> impl Iterator<Item = (&PrimPath, &Value)> {
        std::iter::once((&self.path, &self.value)).chain(self.peers.iter().map(|(p, v)| (p, v)))
    }

    /// The previewed value of `attr` of `path`, if the drag changes it.
    pub fn value_for(&self, path: &PrimPath, attr: &str) -> Option<&Value> {
        if self.attr != attr {
            return None;
        }
        self.targets().find(|(p, _)| *p == path).map(|(_, v)| v)
    }
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
}

#[derive(Debug, Default)]
pub struct Session {
    /// Last UI frame in which a visible timeline requested per-layer CPU timings.
    pub(crate) timing_column_frame: Option<u64>,
    pub(crate) layer_times: HashMap<PrimPath, std::time::Duration>,
    /// Comp shown in the viewer and timeline.
    pub active_comp: Option<PrimPath>,
    /// Comps opened in the timeline (tabs), in opening order.
    pub open_comps: Vec<PrimPath>,
    pub(crate) mini_flow: Option<crate::timeline::navigation::MiniFlow>,
    times: HashMap<PrimPath, Time>,
    pub selected_layers: Vec<PrimPath>,
    pub selected_keys: Vec<KeyRef>,
    /// Properties selected in the timeline outline (`path`, `attr`).
    pub selected_props: Vec<(PrimPath, String)>,
    /// Selected group rows of the Timeline (their row ids, e.g. a shape group,
    /// mask, effect or text animator; [`group_prim`] gives the prim).
    pub selected_groups: Vec<String>,
    /// Effects selected in Effect Controls: the viewer draws their point
    /// controls (AE).
    pub selected_effects: Vec<PrimPath>,
    /// The Graph Editor Set: properties whose graph switch is on (`path`, `attr`).
    pub graph_set: Vec<(PrimPath, String)>,
    /// The Timeline shows the Graph Editor instead of layer bars (Shift+F3).
    pub graph_editor: bool,
    /// The open keyframe dialog (Keyframe Interpolation / Velocity).
    pub key_dialog: Option<crate::timeline::KeyDialog>,
    /// Project panel selection.
    pub selected_items: Vec<PrimPath>,
    pub playback: Option<Playback>,
    pub(crate) output: Option<bae_media::playback::AudioOutput>,
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
    /// The layer the Layer panel shows (opened from the Timeline).
    pub(crate) layer_panel: Option<PrimPath>,
    pub reveal: HashMap<PrimPath, Reveal>,
    pub scrub: Option<Scrub>,
    pub clipboard: Option<KeyClipboard>,
    /// Layers on the clipboard (Edit > Copy with layers selected); the last
    /// copy wins over [`Session::clipboard`].
    pub layer_clipboard: Vec<bae_motion::CopiedLayer>,
    /// Layer contents on the clipboard (Copy with groups selected); the last copy
    /// wins over the other clipboards.
    pub content_clipboard: Vec<bae_motion::CopiedContent>,
    pending: Vec<(Pending, u32)>,
    /// Names of items commands are about to create (path, name), so default
    /// names given in the same frame stay unique.
    reserved: Vec<(PrimPath, String)>,
    /// When U was last pressed (UU detection).
    pub(crate) last_u: Option<Instant>,
    /// When L was last pressed to reveal the audio waveform on a second press.
    pub(crate) last_l: Option<Instant>,
    /// Motion Sketch armed (Start Capture): the next drag in the Composition
    /// panel records this layer's position.
    pub motion_sketch: Option<SketchArm>,
    /// What the viewer reports to the Info panel.
    pub info: crate::ae_panels::info::Readout,
    /// The footage item the Footage panel shows.
    pub footage_view: Option<PrimPath>,
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

    /// Called on a background thread. Device negotiation may block in the OS.
    pub(crate) fn open_audio(&self) -> Result<(), String> {
        let device = bae_media::playback::AudioOutput::open();
        let mut s = self.lock();
        s.opening_audio = false;
        match device {
            Ok(output) => {
                output.set_gain(10.0_f32.powf(s.master_db / 20.0));
                let previous = s.output.replace(output);
                s.mix_revision = None;
                s.retry_audio_after = None;
                drop(s);
                drop(previous);
                Ok(())
            }
            Err(error) => {
                s.retry_audio_after = Some(Instant::now() + std::time::Duration::from_secs(2));
                Err(error)
            }
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
        if let Some(pb) = &mut self.playback
            && pb.comp == info.path
        {
            if pb.range.contains(at) {
                pb.from = at;
                pb.started = Instant::now();
            } else {
                self.stop();
            }
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
        self.active_comp = Some(comp.clone());
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
        let pending = std::mem::take(&mut self.pending);
        for (what, age) in pending {
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
                Pending::NewLayers { .. } | Pending::NewEffects { .. } => {
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
                    Pending::NewLayers { .. } | Pending::NewEffects { .. } => {}
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
                Pending::NewLayers { .. } | Pending::NewEffects { .. } => None,
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
        self.selected_layers
            .retain(|l| stage.has_prim(l) && !query::is_locked(stage, l));
        self.selected_items.retain(|l| stage.has_prim(l));
        self.selected_keys.retain(|k| {
            query::attr_spec(stage, &k.path, &k.attr)
                .is_some_and(|s| s.keyframes.contains_key(&k.time))
        });
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

    /// Start playing the active comp (Preview panel options): from the current
    /// time or the start of the range, through the range.
    pub fn play(&mut self, info: &CompInfo) {
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
        self.times.insert(info.path.clone(), from);
        self.playback = Some(Playback {
            comp: info.path.clone(),
            started: Instant::now(),
            from,
            range,
            before: now,
        });
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
        // Another preview frame rate plays at another speed: the sound does
        // not drive the clock then (AE plays it only at the comp's rate).
        let comp_rate = self
            .preview
            .fps
            .is_none_or(|fps| (fps - info.frame_rate.as_f64()).abs() < 1e-3);
        self.audio_clock = comp_rate && self.sound(pb.from, pb.range, self.loop_playback);
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

    pub(crate) fn update_preview(
        &mut self,
        cx: &bae_ui::PanelContext<'_>,
        info: &CompInfo,
    ) -> bool {
        if self.prepare_mix(cx, info) && self.is_playing() {
            // Pick up a late device or a live layer edit at the current CTI.
            let from = self.time(&info.path);
            let pb = self.playback.as_mut().unwrap();
            pb.from = from;
            pb.started = Instant::now();
            let range = pb.range;
            self.audio_clock = self.sound(from, range, self.loop_playback);
        }
        self.tick(info)
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

    /// Advance playback to the wall-clock time. Returns whether it is still playing
    /// (the caller then requests a repaint).
    pub fn tick(&mut self, info: &CompInfo) -> bool {
        if self
            .playback
            .as_ref()
            .is_some_and(|p| p.comp == info.path && p.range != info.work_area)
        {
            self.play(info);
        }
        let Some(pb) = self.playback.clone() else {
            return false;
        };
        if pb.comp != info.path {
            return true;
        }
        let elapsed = if self.audio_clock {
            let output = self.output.as_ref().unwrap();
            output.set_loop(self.loop_playback);
            if output.finished() {
                let at = pb.from + output.elapsed();
                self.stop();
                self.set_time(info, at.min(pb.range.end - info.frame_duration()));
                return false;
            }
            output.elapsed()
        } else {
            // Frame Rate: a lower rate plays slower, as in AE.
            let speed = self
                .preview
                .fps
                .map_or(1.0, |fps| fps / info.frame_rate.as_f64().max(1e-6));
            Time::from_seconds(pb.started.elapsed().as_secs_f64() * speed)
        };
        let span = pb.range.duration();
        let mut t = pb.from + elapsed;
        // Skip: only every (skip + 1)th frame is shown.
        if self.preview.skip > 0 {
            let step = i64::from(self.preview.skip) + 1;
            let f = info.frame_rate.time_to_frame(t - pb.from);
            t = pb.from + info.frame_rate.frame_to_time(f - f.rem_euclid(step));
        }
        if t >= pb.range.end {
            if self.loop_playback && span.0 > 0 {
                t = pb.range.start + Time((t - pb.range.start).0.rem_euclid(span.0));
            } else {
                self.stop();
                self.set_time(info, pb.range.end - info.frame_duration());
                return false;
            }
        }
        self.update_time(info, t);
        true
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
        assert!(s.tick(&i));
        let seek = i.frame_rate.frame_to_time(42);
        s.set_time(&i, seek);
        assert_eq!(s.playback.as_ref().unwrap().from, seek);
        s.set_time(&i, Time::ZERO);
        assert!(!s.is_playing());
        assert!(!s.tick(&i));
    }
}
