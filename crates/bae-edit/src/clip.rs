//! Clip commands: `clip.insert`, `clip.overwrite` (three-point editing),
//! `clip.lift`, `clip.ripple_delete`, `clip.razor`, `clip.move`, `clip.link`,
//! `clip.unlink`, `clip.set_transition`.

use std::collections::{BTreeMap, BTreeSet};

use bae_model::{Command, CommandError, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{
    self, ClipInfo, DEFAULT_STILL_DURATION, DEFAULT_TRANSITION_DURATION, SequenceInfo, TrackKind,
    TransitionKind, attrs,
};
use crate::util::{
    Splits, child, clear_range, create, delete, expand_linked, fail, move_prim, new_clip_name,
    new_link_id, require_sequence, require_unlocked, set, set_time, shift_from, snap, unset,
};

/// Output of commands that create or split clips.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ClipsOutput {
    /// New clips (for insert/overwrite: the video or main clip first, then its
    /// linked audio clip).
    pub clips: Vec<PrimPath>,
}

// ---------------------------------------------------------------------------
// Three-point editing

/// Where a source range goes, from up to four points (Premiere's three-point
/// editing). Any point not given falls back to the source / sequence marks.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct PlaceParams {
    /// Track for the clip (video or audio; must not be locked).
    pub track: PrimPath,
    /// What to place: an asset (footage, still, audio), a comp (live reference to the
    /// motion comp, no intermediate render) or another sequence (nested).
    pub source: PrimPath,
    /// Source in point (source time). Default: the source's in mark, else 0 — or
    /// back-timed from `source_out`.
    #[serde(default)]
    pub source_in: Option<Time>,
    /// Source out point (source time, exclusive). Default: the source's out mark.
    #[serde(default)]
    pub source_out: Option<Time>,
    /// Sequence time where the clip starts. Default: the sequence's in mark, else
    /// back-timed from `sequence_out`, else the end of the sequence (append).
    #[serde(default)]
    pub sequence_in: Option<Time>,
    /// Sequence time where the clip ends (exclusive). Default: the sequence's out mark.
    #[serde(default)]
    pub sequence_out: Option<Time>,
    /// Also place the source's sound on this audio track, linked to the new clip
    /// (when `track` is a video track and the source has audio).
    #[serde(default)]
    pub audio_track: Option<PrimPath>,
    /// Clip name (default the source's name).
    #[serde(default)]
    pub name: Option<String>,
}

/// A resolved three-point edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditRange {
    /// Sequence time of the first frame.
    pub start: Time,
    /// Source time of the first frame.
    pub source_in: Time,
    pub duration: Time,
}

/// Resolve a three-point edit. `source_len` is `None` for sources without a length
/// (stills); `append_at` is used when no sequence point is known. When all four
/// points are given, the source range wins (the sequence out point is ignored).
pub fn resolve_three_point(
    source_in: Option<Time>,
    source_out: Option<Time>,
    sequence_in: Option<Time>,
    sequence_out: Option<Time>,
    source_len: Option<Time>,
    append_at: Time,
) -> EditRange {
    let src_dur = match (source_in, source_out) {
        (Some(i), Some(o)) => Some(o - i),
        _ => None,
    };
    let seq_dur = match (sequence_in, sequence_out) {
        (Some(i), Some(o)) => Some(o - i),
        _ => None,
    };
    let duration = src_dur.or(seq_dur).unwrap_or_else(|| {
        let from = source_in.unwrap_or(Time::ZERO);
        match (source_out, source_len) {
            (Some(o), _) => o, // back-timed from the out point to source start
            (None, Some(len)) => len - from,
            (None, None) => DEFAULT_STILL_DURATION,
        }
    });
    let src_in = source_in
        .or_else(|| source_out.map(|o| o - duration))
        .unwrap_or(Time::ZERO);
    let start = sequence_in
        .or_else(|| sequence_out.map(|o| o - duration))
        .unwrap_or(append_at);
    EditRange {
        start,
        source_in: src_in,
        duration,
    }
}

struct Placed {
    seq: SequenceInfo,
    range: EditRange,
    /// (track, is the linked audio part).
    targets: Vec<PrimPath>,
}

fn plan(tx: &Transaction<'_>, p: &PlaceParams) -> Result<Placed, CommandError> {
    let stage = tx.stage();
    let track = require_unlocked(stage, &p.track)?;
    let seq_path = p.track.parent().expect("tracks have a parent");
    let seq = require_sequence(stage, &seq_path)?;
    let kind = query::source_kind(stage, &p.source)
        .ok_or_else(|| fail(format!("{} is not an asset, comp or sequence", p.source)))?;
    if kind == query::SourceKind::Sequence
        && (p.source == seq.path || nests(stage, &p.source, &seq.path))
    {
        return Err(fail("a sequence cannot contain itself"));
    }
    let (has_video, has_audio) = query::source_streams(stage, &p.source);
    let mut targets = vec![p.track.clone()];
    if let Some(a) = &p.audio_track {
        let at = require_unlocked(stage, a)?;
        if a.parent().as_ref() != Some(&seq.path) || at.kind != TrackKind::Audio {
            return Err(fail(format!(
                "{a} is not an audio track of {}",
                seq.path
            )));
        }
        if track.kind == TrackKind::Video && has_audio {
            targets.push(a.clone());
        }
    }
    if track.kind == TrackKind::Video && !has_video && has_audio && p.audio_track.is_none() {
        return Err(fail("the source has no picture; place it on an audio track"));
    }
    let (src_mark_in, src_mark_out) = query::marks(stage, &p.source);
    let source_len = query::source_duration(stage, &p.source);
    let sn = |t: Option<Time>| t.map(|t| snap(&seq, t));
    let append_at = targets
        .iter()
        .flat_map(|t| query::clips(stage, t))
        .map(|c| c.end())
        .max()
        .unwrap_or(Time::ZERO);
    let range = resolve_three_point(
        sn(p.source_in.or(src_mark_in)),
        sn(p.source_out.or(src_mark_out)),
        sn(p.sequence_in.or(seq.mark_in)),
        sn(p.sequence_out.or(seq.mark_out)),
        source_len,
        append_at,
    );
    if range.duration < seq.frame_duration() {
        return Err(fail("the edit is shorter than one frame"));
    }
    if range.start < Time::ZERO {
        return Err(fail("the edit would start before the sequence"));
    }
    if range.source_in < Time::ZERO {
        return Err(fail("the source in point is before the start of the source"));
    }
    if let Some(len) = source_len
        && range.source_in + range.duration > len
    {
        return Err(fail("not enough source media for the edit"));
    }
    Ok(Placed {
        seq,
        range,
        targets,
    })
}

/// Whether sequence `outer` (directly or deeply) contains a clip of `inner`.
fn nests(stage: &bae_model::Stage, outer: &PrimPath, inner: &PrimPath) -> bool {
    let mut stack = vec![outer.clone()];
    let mut seen = BTreeSet::new();
    while let Some(s) = stack.pop() {
        if !seen.insert(s.clone()) {
            continue;
        }
        for c in query::sequence_clips(stage, &s) {
            if let Some(src) = c.source
                && query::is_sequence(stage, &src)
            {
                if &src == inner {
                    return true;
                }
                stack.push(src);
            }
        }
    }
    false
}

fn place(
    tx: &mut Transaction<'_>,
    p: PlaceParams,
    ripple: bool,
) -> Result<ClipsOutput, CommandError> {
    let Placed {
        seq,
        range,
        targets,
    } = plan(tx, &p)?;
    let end = range.start + range.duration;
    let mut splits = Splits::default();
    let none = BTreeSet::new();
    for t in &targets {
        if ripple {
            if let Some(c) = query::clip_at(tx.stage(), t, range.start)
                && c.start < range.start
            {
                splits.split(tx, &seq.path, &c, range.start)?;
            }
            shift_from(tx, t, range.start, range.duration, &none)?;
        } else {
            clear_range(tx, &mut splits, &seq.path, t, range.start, end, &none)?;
        }
    }
    splits.finish(tx, &seq.path)?;
    let mut names = BTreeSet::new();
    let link = (targets.len() > 1).then(|| new_link_id(tx.stage(), &seq.path, &BTreeSet::new()));
    let mut out = ClipsOutput::default();
    for t in &targets {
        let name = new_clip_name(tx.stage(), &seq.path, &names);
        names.insert(name.clone());
        let path = child(t, &name)?;
        create(tx, &path, PrimType::CLIP, None)?;
        set(tx, &path, "source", Value::PrimRef(p.source.clone()))?;
        set_time(tx, &path, "start", range.start)?;
        set_time(tx, &path, "duration", range.duration)?;
        if range.source_in != Time::ZERO {
            set_time(tx, &path, "source_in", range.source_in)?;
        }
        if let Some(n) = p.name.as_ref().filter(|n| !n.trim().is_empty()) {
            set(tx, &path, attrs::NAME, Value::String(n.clone()))?;
        }
        if let Some(l) = &link {
            set(tx, &path, attrs::LINK, Value::String(l.clone()))?;
        }
        out.clips.push(path);
    }
    Ok(out)
}

pub struct InsertClip;

impl Command for InsertClip {
    type Params = PlaceParams;
    type Output = ClipsOutput;
    const ID: &'static str = "clip.insert";
    const TITLE: &'static str = "Insert";
    const DESCRIPTION: &'static str = "Insert edit (Premiere ','): place a source range on a track and push everything after the edit point later. Three-point editing: give any three of source in/out and sequence in/out (missing points come from the marks); with no sequence point the clip is appended.";

    fn run(&self, tx: &mut Transaction<'_>, p: PlaceParams) -> Result<ClipsOutput, CommandError> {
        place(tx, p, true)
    }
}

pub struct OverwriteClip;

impl Command for OverwriteClip {
    type Params = PlaceParams;
    type Output = ClipsOutput;
    const ID: &'static str = "clip.overwrite";
    const TITLE: &'static str = "Overwrite";
    const DESCRIPTION: &'static str = "Overwrite edit (Premiere '.'): place a source range on a track, replacing whatever is there. Three-point editing like clip.insert.";

    fn run(&self, tx: &mut Transaction<'_>, p: PlaceParams) -> Result<ClipsOutput, CommandError> {
        place(tx, p, false)
    }
}

// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
pub struct ClipsParams {
    pub clips: Vec<PrimPath>,
    /// Include the clips linked to them (default true, like Premiere's linked selection).
    #[serde(default = "default_true")]
    pub linked: bool,
}

pub struct LiftClips;

impl Command for LiftClips {
    type Params = ClipsParams;
    type Output = ();
    const ID: &'static str = "clip.lift";
    const TITLE: &'static str = "Clear";
    const DESCRIPTION: &'static str =
        "Delete clips and leave a gap (Premiere Clear / Delete key).";

    fn run(&self, tx: &mut Transaction<'_>, p: ClipsParams) -> Result<(), CommandError> {
        let clips = expand_linked(tx.stage(), &p.clips, p.linked)?;
        for c in &clips {
            delete(tx, &c.path)?;
        }
        unlink_orphans(tx, &clips)
    }
}

pub struct RippleDelete;

impl Command for RippleDelete {
    type Params = ClipsParams;
    type Output = ();
    const ID: &'static str = "clip.ripple_delete";
    const TITLE: &'static str = "Ripple Delete";
    const DESCRIPTION: &'static str = "Delete clips and close the gap: later clips on the same tracks move earlier (Premiere Shift+Delete).";

    fn run(&self, tx: &mut Transaction<'_>, p: ClipsParams) -> Result<(), CommandError> {
        let clips = expand_linked(tx.stage(), &p.clips, p.linked)?;
        let mut by_track: BTreeMap<PrimPath, Vec<ClipInfo>> = BTreeMap::new();
        for c in &clips {
            by_track.entry(c.track.clone()).or_default().push(c.clone());
        }
        let none = BTreeSet::new();
        for (track, mut list) in by_track {
            // Latest first, so each shift also carries the gaps closed after it.
            list.sort_by_key(|c| std::cmp::Reverse(c.start));
            for c in list {
                delete(tx, &c.path)?;
                shift_from(tx, &track, c.end(), -c.duration, &none)?;
            }
        }
        unlink_orphans(tx, &clips)
    }
}

/// After deleting `removed`, unlink partners that are now alone in their group.
fn unlink_orphans(tx: &mut Transaction<'_>, removed: &[ClipInfo]) -> Result<(), CommandError> {
    let mut groups: BTreeMap<(PrimPath, String), ()> = BTreeMap::new();
    for c in removed {
        if let (Some(l), Some(seq)) = (&c.link, query::owning_sequence(tx.stage(), &c.track)) {
            groups.insert((seq, l.clone()), ());
        }
    }
    for (seq, link) in groups.into_keys() {
        let rest: Vec<_> = query::sequence_clips(tx.stage(), &seq)
            .into_iter()
            .filter(|c| c.link.as_deref() == Some(link.as_str()))
            .collect();
        if rest.len() == 1 {
            unset(tx, &rest[0].path, attrs::LINK)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------

pub struct Razor;

#[derive(Deserialize, JsonSchema)]
pub struct RazorParams {
    pub sequence: PrimPath,
    /// Sequence time of the cut; snapped to frames.
    pub time: Time,
    /// Tracks to cut (default every unlocked track, Premiere Shift+Cmd/Ctrl+K). Clips
    /// linked to a cut clip are cut too.
    #[serde(default)]
    pub tracks: Option<Vec<PrimPath>>,
}

impl Command for Razor {
    type Params = RazorParams;
    type Output = ClipsOutput;
    const ID: &'static str = "clip.razor";
    const TITLE: &'static str = "Add Edit";
    const DESCRIPTION: &'static str = "Cut clips in two at a time (Razor tool, Premiere Add Edit Cmd/Ctrl+K). Returns the new right-hand clips.";

    fn run(&self, tx: &mut Transaction<'_>, p: RazorParams) -> Result<ClipsOutput, CommandError> {
        let seq = require_sequence(tx.stage(), &p.sequence)?;
        let t = snap(&seq, p.time);
        let tracks: Vec<PrimPath> = match p.tracks {
            Some(list) => {
                for tr in &list {
                    if tr.parent().as_ref() != Some(&seq.path) {
                        return Err(fail(format!("{tr} is not a track of {}", seq.path)));
                    }
                    require_unlocked(tx.stage(), tr)?;
                }
                list
            }
            None => query::tracks(tx.stage(), &seq.path)
                .into_iter()
                .filter(|t| !t.locked)
                .map(|t| t.path)
                .collect(),
        };
        let mut cut: Vec<PrimPath> = tracks
            .iter()
            .filter_map(|tr| query::clip_at(tx.stage(), tr, t))
            .filter(|c| c.start < t)
            .map(|c| c.path)
            .collect();
        // Linked partners that span the cut.
        let partners: Vec<PrimPath> = cut
            .iter()
            .flat_map(|c| query::linked_clips(tx.stage(), c))
            .collect();
        for c in partners {
            let Some(info) = query::clip_info(tx.stage(), &c) else {
                continue;
            };
            if info.start < t && t < info.end() && !cut.contains(&c) {
                require_unlocked(tx.stage(), &info.track)?;
                cut.push(c);
            }
        }
        let mut splits = Splits::default();
        for c in &cut {
            let info = query::clip_info(tx.stage(), c).expect("clip");
            splits.split(tx, &seq.path, &info, t)?;
        }
        Ok(ClipsOutput {
            clips: splits.finish(tx, &seq.path)?,
        })
    }
}

// ---------------------------------------------------------------------------

pub struct MoveClips;

#[derive(Deserialize, JsonSchema)]
pub struct MoveClipsParams {
    pub clips: Vec<PrimPath>,
    /// Time offset (flicks; snapped to frames).
    #[serde(default)]
    pub delta: Time,
    /// Move the given clips this many tracks up (video) or down (audio); linked
    /// partners only move in time.
    #[serde(default)]
    pub track_offset: i32,
    /// Move linked clips too (default true).
    #[serde(default = "default_true")]
    pub linked: bool,
}

impl Command for MoveClips {
    type Params = MoveClipsParams;
    type Output = ClipsOutput;
    const ID: &'static str = "clip.move";
    const TITLE: &'static str = "Move Clips";
    const DESCRIPTION: &'static str = "Move clips in time and/or to other tracks (Selection tool drag). The moved clips overwrite what is at their destination. Returns the clips' new paths.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: MoveClipsParams,
    ) -> Result<ClipsOutput, CommandError> {
        let clips = expand_linked(tx.stage(), &p.clips, p.linked)?;
        let Some(first) = clips.first() else {
            return Ok(ClipsOutput::default());
        };
        let seq_path = query::owning_sequence(tx.stage(), &first.track)
            .ok_or_else(|| fail("clip outside a sequence"))?;
        let seq = require_sequence(tx.stage(), &seq_path)?;
        let delta = snap(&seq, p.delta);
        let all_tracks = query::tracks(tx.stage(), &seq.path);
        // Destination track of each clip.
        let mut plan = Vec::new();
        for c in &clips {
            if query::owning_sequence(tx.stage(), &c.track).as_ref() != Some(&seq.path) {
                return Err(fail("all clips must be in the same sequence"));
            }
            let src = all_tracks
                .iter()
                .find(|t| t.path == c.track)
                .expect("track of the sequence");
            let explicit = p.clips.contains(&c.path);
            let dest = if explicit && p.track_offset != 0 {
                let idx = src.index as i64 + i64::from(p.track_offset);
                all_tracks
                    .iter()
                    .find(|t| t.kind == src.kind && t.index as i64 == idx)
                    .ok_or_else(|| fail("no track there"))?
            } else {
                src
            };
            if dest.locked {
                return Err(fail(format!("track {} is locked", dest.label)));
            }
            if c.start + delta < Time::ZERO {
                return Err(fail(format!("{} would start before the sequence", c.name)));
            }
            plan.push((c.clone(), dest.path.clone()));
        }
        let moving: BTreeSet<PrimPath> = clips.iter().map(|c| c.path.clone()).collect();
        let mut splits = Splits::default();
        for (c, dest) in &plan {
            clear_range(
                tx,
                &mut splits,
                &seq.path,
                dest,
                c.start + delta,
                c.end() + delta,
                &moving,
            )?;
        }
        splits.finish(tx, &seq.path)?;
        let mut out = ClipsOutput::default();
        for (c, dest) in plan {
            if delta != Time::ZERO {
                set_time(tx, &c.path, "start", c.start + delta)?;
            }
            let to = if dest == c.track {
                c.path.clone()
            } else {
                let name = if tx.stage().has_prim(&child(&dest, c.path.name())?) {
                    new_clip_name(tx.stage(), &seq.path, &BTreeSet::new())
                } else {
                    c.path.name().to_owned()
                };
                let to = child(&dest, &name)?;
                move_prim(tx, &c.path, &to)?;
                to
            };
            out.clips.push(to);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------

pub struct LinkClips;

#[derive(Deserialize, JsonSchema)]
pub struct LinkParams {
    pub clips: Vec<PrimPath>,
}

impl Command for LinkClips {
    type Params = LinkParams;
    type Output = ();
    const ID: &'static str = "clip.link";
    const TITLE: &'static str = "Link";
    const DESCRIPTION: &'static str = "Link clips (usually a video clip and its audio) so they are selected, moved, trimmed and deleted together.";

    fn run(&self, tx: &mut Transaction<'_>, p: LinkParams) -> Result<(), CommandError> {
        let clips = expand_linked(tx.stage(), &p.clips, true)?;
        if clips.len() < 2 {
            return Err(fail("link needs at least two clips"));
        }
        let seq = query::owning_sequence(tx.stage(), &clips[0].track)
            .ok_or_else(|| fail("clip outside a sequence"))?;
        if clips
            .iter()
            .any(|c| query::owning_sequence(tx.stage(), &c.track).as_ref() != Some(&seq))
        {
            return Err(fail("linked clips must be in the same sequence"));
        }
        let id = new_link_id(tx.stage(), &seq, &BTreeSet::new());
        for c in &clips {
            set(tx, &c.path, attrs::LINK, Value::String(id.clone()))?;
        }
        Ok(())
    }
}

pub struct UnlinkClips;

impl Command for UnlinkClips {
    type Params = LinkParams;
    type Output = ();
    const ID: &'static str = "clip.unlink";
    const TITLE: &'static str = "Unlink";
    const DESCRIPTION: &'static str = "Unlink clips and everything linked to them.";

    fn run(&self, tx: &mut Transaction<'_>, p: LinkParams) -> Result<(), CommandError> {
        let clips = expand_linked(tx.stage(), &p.clips, true)?;
        for c in &clips {
            if c.link.is_some() {
                unset(tx, &c.path, attrs::LINK)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SetTransition;

/// Which end of a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    /// The head (in point).
    In,
    /// The tail (out point).
    Out,
}

#[derive(Deserialize, JsonSchema)]
pub struct SetTransitionParams {
    pub clip: PrimPath,
    /// in (head) or out (tail).
    pub edge: Edge,
    /// cross_dissolve or dip_to_black; omit to remove the transition.
    #[serde(default)]
    pub kind: Option<TransitionKind>,
    /// Duration (default 1 second); snapped to frames.
    #[serde(default)]
    pub duration: Option<Time>,
}

impl Command for SetTransition {
    type Params = SetTransitionParams;
    type Output = ();
    const ID: &'static str = "clip.set_transition";
    const TITLE: &'static str = "Apply Transition";
    const DESCRIPTION: &'static str = "Put a Cross Dissolve or Dip to Black on the head or tail of a clip, or remove it. At a cut the transition is centered on the cut.";

    fn run(&self, tx: &mut Transaction<'_>, p: SetTransitionParams) -> Result<(), CommandError> {
        let c = crate::util::require_clip(tx.stage(), &p.clip)?;
        require_unlocked(tx.stage(), &c.track)?;
        let (k, d) = match p.edge {
            Edge::In => (attrs::TRANSITION_IN_KIND, attrs::TRANSITION_IN_DURATION),
            Edge::Out => (attrs::TRANSITION_OUT_KIND, attrs::TRANSITION_OUT_DURATION),
        };
        match p.kind {
            None => {
                unset(tx, &p.clip, k)?;
                unset(tx, &p.clip, d)?;
            }
            Some(kind) => {
                let seq = query::owning_sequence(tx.stage(), &c.track)
                    .and_then(|s| query::sequence_info(tx.stage(), &s))
                    .ok_or_else(|| fail("clip outside a sequence"))?;
                let dur = snap(&seq, p.duration.unwrap_or(DEFAULT_TRANSITION_DURATION));
                if dur < seq.frame_duration() {
                    return Err(fail("a transition lasts at least one frame"));
                }
                set(tx, &p.clip, k, Value::Enum(kind.as_str().to_owned()))?;
                set_time(tx, &p.clip, d, dur)?;
            }
        }
        Ok(())
    }
}
