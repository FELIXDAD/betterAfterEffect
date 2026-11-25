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
}

/// Real-time playback of one comp.
#[derive(Clone, Debug)]
pub struct Playback {
    pub comp: PrimPath,
    started: Instant,
    from: Time,
    /// Range played (the work area); playback loops or stops at its end.
    pub range: TimeRange,
}

/// A value being dragged in a panel, previewed by the viewer before the command runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Scrub {
    pub path: PrimPath,
    pub attr: String,
    pub value: Value,
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
}

#[derive(Debug, Default)]
pub struct Session {
    pub(crate) loading_lut: Option<crate::effects::lut::Load>,
    /// Comp shown in the viewer and timeline.
    pub active_comp: Option<PrimPath>,
    /// Comps opened in the timeline (tabs), in opening order.
    pub open_comps: Vec<PrimPath>,
    times: HashMap<PrimPath, Time>,
    pub selected_layers: Vec<PrimPath>,
    pub selected_keys: Vec<KeyRef>,
    /// Properties selected in the timeline outline (`path`, `attr`).
    pub selected_props: Vec<(PrimPath, String)>,
    /// The Graph Editor Set: properties whose graph switch is on (`path`, `attr`).
    pub graph_set: Vec<(PrimPath, String)>,
    /// The Timeline shows the Graph Editor instead of layer bars (Shift+F3).
    pub graph_editor: bool,
    /// The open keyframe dialog (Keyframe Interpolation / Velocity).
    pub key_dialog: Option<crate::timeline::KeyDialog>,
    /// Project panel selection.
    pub selected_items: Vec<PrimPath>,
    pub playback: Option<Playback>,
    /// Loop playback (Preview panel).
    pub loop_playback: bool,
    /// The viewer's resolution is on Auto: it follows the magnification. The
    /// chosen resolution itself is the comp's (`resolution`, as in Composition
    /// Settings).
    pub auto_resolution: bool,
    /// Each comp's 3D views (View ▸ Switch 3D View).
    pub(crate) views3d: HashMap<PrimPath, crate::viewer::view3d::Views>,
    /// What F10, F11 and F12 switch to (View ▸ Assign 3D View Shortcut).
    pub(crate) view_shortcuts: crate::viewer::view3d::Shortcuts,
    pub reveal: HashMap<PrimPath, Reveal>,
    pub scrub: Option<Scrub>,
    pub clipboard: Option<KeyClipboard>,
    /// Layers on the clipboard (Edit > Copy with layers selected); the last
    /// copy wins over [`Session::clipboard`].
    pub layer_clipboard: Vec<bae_motion::CopiedLayer>,
    pending: Vec<(Pending, u32)>,
    /// Names of items commands are about to create (path, name), so default
    /// names given in the same frame stay unique.
    reserved: Vec<(PrimPath, String)>,
    /// When U was last pressed (UU detection).
    pub(crate) last_u: Option<Instant>,
    /// What the viewer reports to the Info panel.
    pub info: crate::ae_panels::info::Readout,
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
        let last = info.duration - info.frame_duration();
        let t = info
            .frame_rate
            .snap(t.clamp(Time::ZERO, last.max(Time::ZERO)));
        self.times.insert(info.path.clone(), t);
    }

    /// Make `comp` the active comp (opening a timeline tab for it).
    pub fn open_comp(&mut self, comp: &PrimPath) {
        if self.active_comp.as_ref() != Some(comp) {
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
                Pending::NewLayers { .. } => unreachable!("handled above"),
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
                    Pending::NewLayers { .. } => {}
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
                Pending::NewLayers { .. } => None,
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
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_some()
    }

    /// Start playing the active comp from the current time through the work area
    /// (from the work area start when the current time is outside it).
    pub fn play(&mut self, info: &CompInfo) {
        let range = info.work_area;
        let now = self.time(&info.path);
        let from = if range.contains(now) && now + info.frame_duration() < range.end {
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
        });
    }

    pub fn stop(&mut self) {
        self.playback = None;
    }

    /// Advance playback to the wall-clock time. Returns whether it is still playing
    /// (the caller then requests a repaint).
    pub fn tick(&mut self, info: &CompInfo) -> bool {
        let Some(pb) = self.playback.clone() else {
            return false;
        };
        if pb.comp != info.path {
            return true;
        }
        let elapsed = Time::from_seconds(pb.started.elapsed().as_secs_f64());
        let span = pb.range.duration();
        let mut t = pb.from + elapsed;
        if t >= pb.range.end {
            if self.loop_playback && span.0 > 0 {
                t = pb.range.start + Time((t - pb.range.start).0.rem_euclid(span.0));
            } else {
                self.playback = None;
                self.set_time(info, pb.range.end - info.frame_duration());
                return false;
            }
        }
        self.set_time(info, t);
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
        s.play(&i);
        assert!(s.is_playing());
        assert_eq!(s.time(&i.path), Time::from_seconds(1.0));
        assert!(s.tick(&i));
        s.stop();
        assert!(!s.tick(&i));
    }
}
