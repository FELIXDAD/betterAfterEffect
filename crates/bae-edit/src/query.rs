//! Read-only helpers over the composed stage, shared by the edit commands, the edit
//! workspace UI (`bae-ui-edit`) and the renderer: sequence settings, tracks, clips,
//! what is visible or audible at a time, link groups and clip sources.
//!
//! Nothing here writes to the stage.

use bae_model::schema::attr_def;
use bae_model::{FrameRate, PrimPath, PrimType, PrimView, Stage, Time, TimeRange, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Folder new sequences go into when no parent is given (`/sequences/<name>`).
pub const SEQUENCES_FOLDER: &str = "/sequences";
/// Child folder of a sequence holding its markers (`Marker` prims).
pub const MARKERS: &str = "markers";
/// Duration of a clip made from a source without a length (stills, solids), like
/// Premiere's default still image duration.
pub const DEFAULT_STILL_DURATION: Time = Time(5 * Time::FLICKS_PER_SECOND);
/// Default transition duration (Premiere: 1 second).
pub const DEFAULT_TRANSITION_DURATION: Time = Time::ONE_SECOND;

/// Attribute names the edit commands write besides the `bae-model` schema ones.
/// They are accepted as custom attributes until 02 adds them to the schema.
pub mod attrs {
    /// Sequence or source item (asset, comp, sequence): in mark (I), time.
    pub const MARK_IN: &str = "mark_in";
    /// Sequence or source item: out mark (O), time, exclusive.
    pub const MARK_OUT: &str = "mark_out";
    /// Track: locked (no edits), bool.
    pub const LOCKED: &str = "locked";
    /// Video track: track output on (the eye), bool, default true.
    pub const ENABLED: &str = "enabled";
    /// Audio track: solo, bool.
    pub const SOLO: &str = "solo";
    /// Clip: display name, string (default the source name).
    pub const NAME: &str = "name";
    /// Clip: link group; clips sharing a non-empty value are linked (A/V), string.
    pub const LINK: &str = "link";
    /// Clip: transition at the head, enum `cross_dissolve` | `dip_to_black`.
    pub const TRANSITION_IN_KIND: &str = "transition_in:kind";
    /// Clip: duration of the head transition, time.
    pub const TRANSITION_IN_DURATION: &str = "transition_in:duration";
    /// Clip: transition at the tail.
    pub const TRANSITION_OUT_KIND: &str = "transition_out:kind";
    /// Clip: duration of the tail transition, time.
    pub const TRANSITION_OUT_DURATION: &str = "transition_out:duration";
}

/// Video or audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Audio,
}

impl TrackKind {
    /// The `track_kind` token.
    pub fn as_str(self) -> &'static str {
        match self {
            TrackKind::Video => "video",
            TrackKind::Audio => "audio",
        }
    }

    /// Track label prefix: `V` or `A` (Premiere's V1, A1 …).
    pub fn prefix(self) -> &'static str {
        match self {
            TrackKind::Video => "V",
            TrackKind::Audio => "A",
        }
    }
}

/// Basic transitions (framework: stored on the clip edge, not rendered yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Cross Dissolve (交叉溶解).
    CrossDissolve,
    /// Dip to Black (黑场过渡).
    DipToBlack,
}

impl TransitionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TransitionKind::CrossDissolve => "cross_dissolve",
            TransitionKind::DipToBlack => "dip_to_black",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cross_dissolve" => Some(TransitionKind::CrossDissolve),
            "dip_to_black" => Some(TransitionKind::DipToBlack),
            _ => None,
        }
    }
}

/// A transition on one edge of a clip. At a cut between two clips it is centered on
/// the cut; on a free edge it fades from / to nothing (black or silence).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: Time,
}

/// Which kind of item a clip plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Footage, still, solid … (`Asset` prim).
    Asset,
    /// A motion comp, referenced live (no intermediate render).
    Comp,
    /// A nested sequence.
    Sequence,
}

/// Settings of a sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceInfo {
    pub path: PrimPath,
    pub name: String,
    pub frame_rate: FrameRate,
    pub width: u32,
    pub height: u32,
    /// In mark (I), sequence time.
    pub mark_in: Option<Time>,
    /// Out mark (O), sequence time, exclusive.
    pub mark_out: Option<Time>,
    /// End of the last clip on any track.
    pub duration: Time,
}

impl SequenceInfo {
    pub fn frame_duration(&self) -> Time {
        self.frame_rate.frame_duration()
    }

    /// `t` rounded to the nearest frame boundary of the sequence.
    pub fn snap(&self, t: Time) -> Time {
        snap(self.frame_rate, t)
    }
}

/// One track of a sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackInfo {
    pub path: PrimPath,
    pub kind: TrackKind,
    /// 0-based position among the sequence's tracks of the same kind (V1 = 0).
    pub index: usize,
    /// `V1`, `V2` … / `A1`, `A2` … by position.
    pub label: String,
    /// Custom name, empty when the track only has its label.
    pub name: String,
    pub locked: bool,
    /// Video: track output (eye). Audio: always true.
    pub enabled: bool,
    pub muted: bool,
    pub solo: bool,
}

/// One clip on a track.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipInfo {
    pub path: PrimPath,
    pub track: PrimPath,
    /// Asset, comp or sequence the clip plays.
    pub source: Option<PrimPath>,
    pub name: String,
    /// Sequence time of the first frame.
    pub start: Time,
    pub duration: Time,
    /// Source time shown at `start`.
    pub source_in: Time,
    /// Link group (A/V clips that move, trim and delete together).
    pub link: Option<String>,
    /// The shot this clip belongs to.
    pub shot: Option<PrimPath>,
    pub transition_in: Option<Transition>,
    pub transition_out: Option<Transition>,
}

impl ClipInfo {
    /// Sequence time just after the last frame.
    pub fn end(&self) -> Time {
        self.start + self.duration
    }

    pub fn range(&self) -> TimeRange {
        TimeRange::new(self.start, self.end())
    }

    /// Source time just after the last frame.
    pub fn source_out(&self) -> Time {
        self.source_in + self.duration
    }

    /// Source time shown at sequence time `t`, if the clip covers `t`.
    pub fn source_time(&self, t: Time) -> Option<Time> {
        self.range()
            .contains(t)
            .then(|| self.source_in + (t - self.start))
    }
}

/// Static value of `attr` on `view`, falling back to the schema default.
pub fn static_value(view: &PrimView<'_>, attr: &str) -> Option<Value> {
    view.value(attr).cloned().or_else(|| {
        view.prim_type()
            .and_then(|t| attr_def(t, attr))
            .and_then(|d| d.default.to_value())
    })
}

fn time_attr(view: &PrimView<'_>, attr: &str) -> Option<Time> {
    static_value(view, attr).and_then(|v| v.as_time())
}

fn bool_attr(view: &PrimView<'_>, attr: &str, default: bool) -> bool {
    static_value(view, attr)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn string_attr(view: &PrimView<'_>, attr: &str) -> Option<String> {
    match view.value(attr) {
        Some(Value::String(s) | Value::Enum(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn int_attr(view: &PrimView<'_>, attr: &str, default: i64) -> i64 {
    match static_value(view, attr) {
        Some(Value::Int(v)) => v,
        Some(Value::Float(v)) => v as i64,
        _ => default,
    }
}

/// `t` rounded to the nearest frame boundary of `rate`.
pub fn snap(rate: FrameRate, t: Time) -> Time {
    let half = Time(rate.frame_duration().0 / 2);
    rate.frame_to_time(rate.time_to_frame(t + half))
}

/// Display name: the `name` attribute, else the prim name.
pub fn display_name(stage: &Stage, path: &PrimPath) -> String {
    stage
        .prim(path)
        .and_then(|v| string_attr(&v, "name"))
        .unwrap_or_else(|| path.name().to_owned())
}

fn is_a(stage: &Stage, path: &PrimPath, ty: &PrimType) -> bool {
    stage.prim(path).is_some_and(|v| v.is_a(ty))
}

/// Whether `path` is a `Sequence` prim.
pub fn is_sequence(stage: &Stage, path: &PrimPath) -> bool {
    is_a(stage, path, &PrimType::SEQUENCE)
}

/// Whether `path` is a `Track` prim.
pub fn is_track(stage: &Stage, path: &PrimPath) -> bool {
    is_a(stage, path, &PrimType::TRACK)
}

/// Whether `path` is a `Clip` prim.
pub fn is_clip(stage: &Stage, path: &PrimPath) -> bool {
    is_a(stage, path, &PrimType::CLIP)
}

/// Every sequence on the stage, in traversal order.
pub fn sequences(stage: &Stage) -> Vec<PrimPath> {
    stage
        .traverse()
        .into_iter()
        .filter(|p| is_sequence(stage, p))
        .collect()
}

/// The sequence a track or clip belongs to (itself for a sequence).
pub fn owning_sequence(stage: &Stage, path: &PrimPath) -> Option<PrimPath> {
    std::iter::once(path.clone())
        .chain(path.ancestors())
        .find(|p| !p.is_root() && is_sequence(stage, p))
}

/// Settings of a sequence, or `None` when `seq` is not a `Sequence`.
pub fn sequence_info(stage: &Stage, seq: &PrimPath) -> Option<SequenceInfo> {
    let view = stage.prim(seq)?;
    if !view.is_a(&PrimType::SEQUENCE) {
        return None;
    }
    let frame_rate = match static_value(&view, "frame_rate") {
        Some(Value::FrameRate(r)) if r.num > 0 && r.den > 0 => r,
        _ => FrameRate::FPS_24,
    };
    let duration = tracks(stage, seq)
        .iter()
        .flat_map(|t| clips(stage, &t.path))
        .map(|c| c.end())
        .max()
        .unwrap_or(Time::ZERO);
    Some(SequenceInfo {
        path: seq.clone(),
        name: display_name(stage, seq),
        frame_rate,
        width: int_attr(&view, "width", 1920).clamp(1, 30_000) as u32,
        height: int_attr(&view, "height", 1080).clamp(1, 30_000) as u32,
        mark_in: time_attr(&view, attrs::MARK_IN),
        mark_out: time_attr(&view, attrs::MARK_OUT),
        duration,
    })
}

fn track_kind(view: &PrimView<'_>) -> TrackKind {
    match static_value(view, "track_kind") {
        Some(Value::Enum(s) | Value::String(s)) if s == "audio" => TrackKind::Audio,
        _ => TrackKind::Video,
    }
}

/// Tracks of a sequence: video tracks V1, V2 … then audio tracks A1, A2 …
pub fn tracks(stage: &Stage, seq: &PrimPath) -> Vec<TrackInfo> {
    let mut video = Vec::new();
    let mut audio = Vec::new();
    for path in stage.children(seq) {
        let Some(view) = stage.prim(&path) else {
            continue;
        };
        if !view.is_a(&PrimType::TRACK) {
            continue;
        }
        let kind = track_kind(&view);
        let list = match kind {
            TrackKind::Video => &mut video,
            TrackKind::Audio => &mut audio,
        };
        let index = list.len();
        list.push(TrackInfo {
            label: format!("{}{}", kind.prefix(), index + 1),
            name: string_attr(&view, "name").unwrap_or_default(),
            locked: bool_attr(&view, attrs::LOCKED, false),
            enabled: bool_attr(&view, attrs::ENABLED, true),
            muted: bool_attr(&view, "muted", false),
            solo: bool_attr(&view, attrs::SOLO, false),
            path,
            kind,
            index,
        });
    }
    video.extend(audio);
    video
}

/// Tracks of one kind, in index order.
pub fn tracks_of(stage: &Stage, seq: &PrimPath, kind: TrackKind) -> Vec<TrackInfo> {
    tracks(stage, seq)
        .into_iter()
        .filter(|t| t.kind == kind)
        .collect()
}

/// One track's info.
pub fn track_info(stage: &Stage, track: &PrimPath) -> Option<TrackInfo> {
    let seq = track.parent()?;
    tracks(stage, &seq).into_iter().find(|t| &t.path == track)
}

fn transition(view: &PrimView<'_>, kind: &str, duration: &str) -> Option<Transition> {
    let kind = TransitionKind::parse(&string_attr(view, kind)?)?;
    let duration = time_attr(view, duration)
        .filter(|d| d.0 > 0)
        .unwrap_or(DEFAULT_TRANSITION_DURATION);
    Some(Transition { kind, duration })
}

/// One clip's info, or `None` when `clip` is not a `Clip`.
pub fn clip_info(stage: &Stage, clip: &PrimPath) -> Option<ClipInfo> {
    let view = stage.prim(clip)?;
    if !view.is_a(&PrimType::CLIP) {
        return None;
    }
    let source = view.value("source").and_then(|v| v.as_prim_ref()).cloned();
    let name = string_attr(&view, attrs::NAME).unwrap_or_else(|| {
        source
            .as_ref()
            .map_or_else(|| clip.name().to_owned(), |s| display_name(stage, s))
    });
    Some(ClipInfo {
        path: clip.clone(),
        track: clip.parent()?,
        name,
        start: time_attr(&view, "start").unwrap_or(Time::ZERO),
        duration: time_attr(&view, "duration").unwrap_or(Time::ONE_SECOND),
        source_in: time_attr(&view, "source_in").unwrap_or(Time::ZERO),
        link: string_attr(&view, attrs::LINK),
        shot: view.value("shot").and_then(|v| v.as_prim_ref()).cloned(),
        transition_in: transition(
            &view,
            attrs::TRANSITION_IN_KIND,
            attrs::TRANSITION_IN_DURATION,
        ),
        transition_out: transition(
            &view,
            attrs::TRANSITION_OUT_KIND,
            attrs::TRANSITION_OUT_DURATION,
        ),
        source,
    })
}

/// Clips of a track, sorted by start.
pub fn clips(stage: &Stage, track: &PrimPath) -> Vec<ClipInfo> {
    let mut out: Vec<ClipInfo> = stage
        .children(track)
        .iter()
        .filter_map(|c| clip_info(stage, c))
        .collect();
    out.sort_by_key(|c| (c.start, c.path.as_str().to_owned()));
    out
}

/// Every clip of a sequence, track by track.
pub fn sequence_clips(stage: &Stage, seq: &PrimPath) -> Vec<ClipInfo> {
    tracks(stage, seq)
        .iter()
        .flat_map(|t| clips(stage, &t.path))
        .collect()
}

/// The clip of `track` that covers sequence time `t`.
pub fn clip_at(stage: &Stage, track: &PrimPath, t: Time) -> Option<ClipInfo> {
    clips(stage, track)
        .into_iter()
        .find(|c| c.range().contains(t))
}

/// What the program monitor shows at `t`: the clips of enabled video tracks that
/// cover `t`, bottom (V1) first — composite them in this order.
pub fn video_stack_at(stage: &Stage, seq: &PrimPath, t: Time) -> Vec<ClipInfo> {
    tracks_of(stage, seq, TrackKind::Video)
        .iter()
        .filter(|tr| tr.enabled)
        .filter_map(|tr| clip_at(stage, &tr.path, t))
        .collect()
}

/// What is heard at `t`: the clips of audible audio tracks that cover `t` (solo
/// wins over mute, as in Premiere).
pub fn audio_at(stage: &Stage, seq: &PrimPath, t: Time) -> Vec<ClipInfo> {
    let audio = tracks_of(stage, seq, TrackKind::Audio);
    let any_solo = audio.iter().any(|tr| tr.solo);
    audio
        .iter()
        .filter(|tr| if any_solo { tr.solo } else { !tr.muted })
        .filter_map(|tr| clip_at(stage, &tr.path, t))
        .collect()
}

/// Clips in the same sequence linked to `clip` (sharing its link group), including
/// `clip` itself. Just `clip` when it is not linked.
pub fn linked_clips(stage: &Stage, clip: &PrimPath) -> Vec<PrimPath> {
    let Some(info) = clip_info(stage, clip) else {
        return Vec::new();
    };
    let (Some(link), Some(seq)) = (info.link, owning_sequence(stage, clip)) else {
        return vec![clip.clone()];
    };
    sequence_clips(stage, &seq)
        .into_iter()
        .filter(|c| c.link.as_deref() == Some(link.as_str()))
        .map(|c| c.path)
        .collect()
}

/// What kind of item `source` is, or `None` when it cannot be a clip source.
pub fn source_kind(stage: &Stage, source: &PrimPath) -> Option<SourceKind> {
    let view = stage.prim(source)?;
    if view.is_a(&PrimType::ASSET) {
        Some(SourceKind::Asset)
    } else if view.is_a(&PrimType::COMP) {
        Some(SourceKind::Comp)
    } else if view.is_a(&PrimType::SEQUENCE) {
        Some(SourceKind::Sequence)
    } else {
        None
    }
}

/// Length of a clip source; `None` for sources without one (stills, solids), which
/// can be used for any duration.
pub fn source_duration(stage: &Stage, source: &PrimPath) -> Option<Time> {
    let view = stage.prim(source)?;
    match source_kind(stage, source)? {
        SourceKind::Asset => time_attr(&view, "duration").filter(|d| d.0 > 0),
        SourceKind::Comp => Some(
            time_attr(&view, "duration")
                .filter(|d| d.0 > 0)
                .unwrap_or(Time::from_seconds(10.0)),
        ),
        SourceKind::Sequence => sequence_info(stage, source).map(|s| s.duration),
    }
}

/// Whether a source has a picture and/or sound: `(video, audio)`.
pub fn source_streams(stage: &Stage, source: &PrimPath) -> (bool, bool) {
    let Some(view) = stage.prim(source) else {
        return (false, false);
    };
    match source_kind(stage, source) {
        Some(SourceKind::Asset) => (
            bool_attr(&view, "has_video", true),
            bool_attr(&view, "has_audio", false),
        ),
        Some(SourceKind::Comp) => (true, false),
        Some(SourceKind::Sequence) => {
            let ts = tracks(stage, source);
            let has = |k: TrackKind| {
                ts.iter()
                    .any(|t| t.kind == k && !clips(stage, &t.path).is_empty())
            };
            (has(TrackKind::Video), has(TrackKind::Audio))
        }
        None => (false, false),
    }
}

/// In and out marks of a sequence or source item (the source monitor's I / O).
pub fn marks(stage: &Stage, item: &PrimPath) -> (Option<Time>, Option<Time>) {
    stage.prim(item).map_or((None, None), |v| {
        (
            time_attr(&v, attrs::MARK_IN),
            time_attr(&v, attrs::MARK_OUT),
        )
    })
}

/// The motion comp a clip opens on double-click: its source when that is a comp
/// (live reference), else the comp of its shot.
pub fn clip_comp(stage: &Stage, clip: &PrimPath) -> Option<PrimPath> {
    let info = clip_info(stage, clip)?;
    if let Some(src) = &info.source
        && source_kind(stage, src) == Some(SourceKind::Comp)
    {
        return Some(src.clone());
    }
    let shot = stage.prim(info.shot.as_ref()?)?;
    let comp = shot.value("comp")?.as_prim_ref()?.clone();
    is_a(stage, &comp, &PrimType::COMP).then_some(comp)
}

/// Sequence markers (`Marker` prims under `<seq>/markers`), by time.
pub fn markers(stage: &Stage, seq: &PrimPath) -> Vec<(PrimPath, Time, String)> {
    let Ok(folder) = seq.child(MARKERS) else {
        return Vec::new();
    };
    let mut out: Vec<_> = stage
        .children(&folder)
        .into_iter()
        .filter_map(|m| {
            let v = stage.prim(&m)?;
            v.is_a(&PrimType::MARKER).then(|| {
                let t = time_attr(&v, "time").unwrap_or(Time::ZERO);
                let comment = string_attr(&v, "comment").unwrap_or_default();
                (m.clone(), t, comment)
            })
        })
        .collect();
    out.sort_by_key(|(_, t, _)| *t);
    out
}

/// A prim name built from a display name (letters, digits, `_`, `-`).
pub fn sanitize_name(display: &str) -> String {
    let mut s: String = display
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while s.starts_with('-') {
        s.remove(0);
    }
    if s.is_empty() { "item".to_owned() } else { s }
}

/// A child name of `parent` based on `base` that no sublayer uses yet
/// (`base`, `base_2`, `base_3` …).
pub fn unique_child_name(stage: &Stage, parent: &PrimPath, base: &str) -> String {
    let base = sanitize_name(base);
    let taken = |name: &str| {
        parent
            .child(name)
            .is_ok_and(|p| stage.has_prim(&p) || stage.sublayers().any(|s| s.has_spec(&p)))
    };
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|i| format!("{base}_{i}"))
        .find(|n| !taken(n))
        .expect("unbounded")
}
