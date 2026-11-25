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
}

/// Real-time playback of one comp.
#[derive(Clone, Debug)]
pub struct Playback {
    pub comp: PrimPath,
    started: Instant,
    from: i64,
    rate: FrameRate,
    ordinal: u64,
    revision: Option<u64>,
    /// Range played (the work area); playback loops or stops at its end.
    pub range: TimeRange,
}

impl Playback {
    fn ordinal_at(&self, now: Instant) -> u64 {
        let nanos = now.saturating_duration_since(self.started).as_nanos();
        (nanos * u128::from(self.rate.num) / (1_000_000_000 * u128::from(self.rate.den))) as u64
    }

    fn deadline(&self) -> Instant {
        let nanos = ((u128::from(self.ordinal) + 1) * 1_000_000_000 * u128::from(self.rate.den))
            .div_ceil(u128::from(self.rate.num));
        self.started + Duration::from_nanos(nanos as u64)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackStats {
    pub fps: f64,
    pub dropped: u64,
    matched: bool,
    drawn: Option<Time>,
    last_present: Option<(u64, Time)>,
    sample: Option<Instant>,
    frames: u32,
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
    pub playback_stats: PlaybackStats,
    /// Loop playback (Preview panel).
    pub loop_playback: bool,
    /// The viewer's resolution is on Auto: it follows the magnification. The
    /// chosen resolution itself is the comp's (`resolution`, as in Composition
    /// Settings).
    pub auto_resolution: bool,
    pub(crate) cached_preview: Option<(PrimPath, u64, Vec<TimeRange>)>,
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
        let started = Instant::now();
        self.times
            .insert(info.path.clone(), info.frame_rate.snap(from));
        self.playback = Some(Playback {
            comp: info.path.clone(),
            started,
            from: info.frame_rate.time_to_frame(from),
            rate: info.frame_rate,
            ordinal: 0,
            revision: None,
            range,
        });
        self.playback_stats = PlaybackStats {
            sample: Some(started),
            ..Default::default()
        };
    }

    pub fn stop(&mut self) {
        self.playback = None;
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
        if pb.range != info.work_area || pb.rate != info.frame_rate {
            self.play(&info);
            if let Some(pb) = self.playback.as_mut() {
                pb.revision = Some(stage.revision());
            }
        }
    }

    pub fn playback_deadline(&self) -> Option<Instant> {
        self.playback.as_ref().map(Playback::deadline)
    }

    /// Advance once before building the workspace. Ordinals count scheduled
    /// frames across loops, even when the event loop wakes several frames late.
    pub fn tick(&mut self, now: Instant) {
        let Some(pb) = self.playback.as_mut() else {
            return;
        };
        let ordinal = pb.ordinal_at(now);
        let stats = &mut self.playback_stats;
        stats.drawn = None;
        if ordinal == pb.ordinal {
            return;
        }
        let start = pb.rate.time_to_frame(pb.range.start);
        let last = pb
            .rate
            .time_to_frame(Time(pb.range.end.0.saturating_sub(1)))
            .max(start);
        let frame = pb.from + ordinal as i64;
        let steps = if self.loop_playback {
            ordinal - pb.ordinal
        } else {
            (ordinal - pb.ordinal).min((last - pb.from + 1).max(0) as u64 - pb.ordinal)
        };
        stats.dropped += steps.saturating_sub(u64::from(stats.matched));
        stats.matched = false;
        pb.ordinal = ordinal;
        let ended = !self.loop_playback && frame > last;
        let frame = if ended {
            last
        } else {
            start + (frame - start).rem_euclid(last - start + 1)
        };
        self.times
            .insert(pb.comp.clone(), pb.rate.frame_to_time(frame));
        if ended {
            self.stop();
        }
    }

    /// Called by the viewer for the actual texture used in this UI pass.
    pub fn drew_preview(&mut self, actual: Time) {
        self.playback_stats.drawn = Some(actual);
    }

    /// Count only frames submitted to the native presentation surface.
    pub fn frame_presented(&mut self, now: Instant) {
        let Some(pb) = &self.playback else { return };
        let expected = self.times.get(&pb.comp).copied();
        let stats = &mut self.playback_stats;
        if let Some(time) = stats.drawn.take() {
            let on_time = expected == Some(time) && pb.ordinal_at(now) == pb.ordinal;
            stats.matched |= on_time;
            let changed = stats
                .last_present
                .is_none_or(|(ordinal, prior)| prior != time || (on_time && ordinal != pb.ordinal));
            if changed {
                stats.frames += 1;
                stats.last_present = Some((pb.ordinal, time));
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
    fn fractional_clock_counts_missed_frames_across_a_loop() {
        let mut s = Session {
            loop_playback: true,
            ..Default::default()
        };
        let mut i = info();
        i.frame_rate = FrameRate::FPS_59_94;
        i.work_area = TimeRange::new(Time::ZERO, i.frame_rate.frame_to_time(3));
        s.play(&i);
        let start = s.playback.as_ref().unwrap().started;
        let next = s.playback_deadline().unwrap();
        s.tick(next - Duration::from_nanos(1));
        assert_eq!(s.time(&i.path), Time::ZERO);
        s.drew_preview(Time::ZERO);
        s.frame_presented(next - Duration::from_nanos(1));
        s.tick(start + Duration::from_millis(51));
        assert_eq!(s.time(&i.path), Time::ZERO);
        assert_eq!(s.playback_stats.dropped, 2);
        s.tick(start + Duration::from_millis(68));
        assert_eq!(s.time(&i.path), i.frame_rate.frame_to_time(1));
        assert_eq!(s.playback_stats.dropped, 3);
    }

    #[test]
    fn playback_starts_in_work_area() {
        let mut s = Session::default();
        let mut i = info();
        i.work_area = TimeRange::new(Time::from_seconds(1.0), Time::from_seconds(2.0));
        s.play(&i);
        assert!(s.is_playing());
        assert_eq!(s.time(&i.path), Time::from_seconds(1.0));
        assert!(s.playback_deadline().is_some());
        s.stop();
        assert!(s.playback_deadline().is_none());
    }
}
