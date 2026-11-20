//! Sequence and track commands: `sequence.create`, `sequence.add_marker`,
//! `marks.set`, `track.add`, `track.remove`, `track.set_state`.

use bae_model::{Command, CommandError, FrameRate, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, MARKERS, SEQUENCES_FOLDER, TrackKind, attrs};
use crate::util::{
    child, create, delete, ensure_folder, fail, require_sequence, require_track, set, set_time,
    snap, unset,
};

/// Output of `sequence.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SequenceOutput {
    /// The new sequence.
    pub path: PrimPath,
    /// Its video tracks, V1 first.
    pub video_tracks: Vec<PrimPath>,
    /// Its audio tracks, A1 first.
    pub audio_tracks: Vec<PrimPath>,
}

/// Output of commands that create one prim.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PathOutput {
    pub path: PrimPath,
}

// ---------------------------------------------------------------------------

pub struct CreateSequence;

fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_fps() -> f64 {
    24.0
}
fn default_tracks() -> u32 {
    3
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateSequenceParams {
    /// Display name (default the next free "Sequence N").
    #[serde(default)]
    pub name: Option<String>,
    /// Folder to create the sequence in (default /sequences, created if missing).
    #[serde(default)]
    pub parent: Option<PrimPath>,
    /// Frame size in pixels (default 1920×1080).
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    /// Frames per second: 23.976, 24, 25, 29.97, 30, 50, 59.94, 60 … (default 24).
    #[serde(default = "default_fps")]
    pub fps: f64,
    /// Number of video tracks (default 3, like Premiere).
    #[serde(default = "default_tracks")]
    pub video_tracks: u32,
    /// Number of audio tracks (default 3).
    #[serde(default = "default_tracks")]
    pub audio_tracks: u32,
}

impl Command for CreateSequence {
    type Params = CreateSequenceParams;
    type Output = SequenceOutput;
    const ID: &'static str = "sequence.create";
    const TITLE: &'static str = "New Sequence";
    const DESCRIPTION: &'static str = "Create an edit sequence (frame size, frame rate) with empty video and audio tracks.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<SequenceOutput, CommandError> {
        if p.width == 0 || p.height == 0 || p.width > 30_000 || p.height > 30_000 {
            return Err(fail("frame size must be between 1 and 30000 pixels"));
        }
        if !(p.fps.is_finite() && p.fps > 0.0 && p.fps <= 999.0) {
            return Err(fail("fps must be between 0 and 999"));
        }
        if p.video_tracks > 99 || p.audio_tracks > 99 {
            return Err(fail("at most 99 tracks of each kind"));
        }
        let parent = match p.parent {
            Some(parent) => {
                tx.stage().require_prim(&parent)?;
                parent
            }
            None => {
                let folder = PrimPath::new(SEQUENCES_FOLDER).expect("valid");
                ensure_folder(tx, &folder)?;
                folder
            }
        };
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| next_sequence_name(tx));
        let path = child(
            &parent,
            &query::unique_child_name(tx.stage(), &parent, &name),
        )?;
        create(tx, &path, PrimType::SEQUENCE, None)?;
        set(tx, &path, "name", Value::String(name))?;
        set(tx, &path, "width", Value::Int(i64::from(p.width)))?;
        set(tx, &path, "height", Value::Int(i64::from(p.height)))?;
        set(
            tx,
            &path,
            "frame_rate",
            Value::FrameRate(FrameRate::from_fps(p.fps)),
        )?;
        let mut video_tracks = Vec::new();
        for _ in 0..p.video_tracks {
            video_tracks.push(add_track(tx, &path, TrackKind::Video, None, None)?);
        }
        let mut audio_tracks = Vec::new();
        for _ in 0..p.audio_tracks {
            audio_tracks.push(add_track(tx, &path, TrackKind::Audio, None, None)?);
        }
        Ok(SequenceOutput {
            path,
            video_tracks,
            audio_tracks,
        })
    }
}

fn next_sequence_name(tx: &Transaction<'_>) -> String {
    let stage = tx.stage();
    let max = query::sequences(stage)
        .iter()
        .filter_map(|s| {
            query::display_name(stage, s)
                .strip_prefix("Sequence ")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    format!("Sequence {}", max + 1)
}

/// Add a track of `kind` at `index` among the tracks of that kind (default last).
fn add_track(
    tx: &mut Transaction<'_>,
    seq: &PrimPath,
    kind: TrackKind,
    index: Option<usize>,
    name: Option<String>,
) -> Result<PrimPath, CommandError> {
    let all = query::tracks(tx.stage(), seq);
    let same: Vec<_> = all.iter().filter(|t| t.kind == kind).collect();
    let index = index.unwrap_or(same.len()).min(same.len());
    // Child position: video tracks come before audio tracks.
    let anchor = match same.get(index) {
        Some(t) => Some(&t.path),
        None if kind == TrackKind::Video => all
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| &t.path),
        None => None,
    };
    let sibling_index = anchor.and_then(|a| {
        tx.stage()
            .sublayer(tx.target())
            .and_then(|s| s.child_index(a))
    });
    let base = match kind {
        TrackKind::Video => "video",
        TrackKind::Audio => "audio",
    };
    let path = child(seq, &query::unique_child_name(tx.stage(), seq, base))?;
    create(tx, &path, PrimType::TRACK, sibling_index)?;
    set(tx, &path, "track_kind", Value::Enum(kind.as_str().to_owned()))?;
    if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
        set(tx, &path, "name", Value::String(n))?;
    }
    Ok(path)
}

// ---------------------------------------------------------------------------

pub struct AddTrack;

#[derive(Deserialize, JsonSchema)]
pub struct AddTrackParams {
    pub sequence: PrimPath,
    /// video or audio.
    pub kind: TrackKind,
    /// Position among the tracks of that kind, 0 = V1 / A1 (default: after the last).
    #[serde(default)]
    pub index: Option<usize>,
    /// Custom track name (default none: the track shows its label V1, A1 …).
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for AddTrack {
    type Params = AddTrackParams;
    type Output = PathOutput;
    const ID: &'static str = "track.add";
    const TITLE: &'static str = "Add Track";
    const DESCRIPTION: &'static str = "Add a video or audio track to a sequence. Tracks are numbered by position (V1 is the bottom video track, A1 the top audio track).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PathOutput, CommandError> {
        require_sequence(tx.stage(), &p.sequence)?;
        let path = add_track(tx, &p.sequence, p.kind, p.index, p.name)?;
        Ok(PathOutput { path })
    }
}

// ---------------------------------------------------------------------------

pub struct RemoveTracks;

#[derive(Deserialize, JsonSchema)]
pub struct RemoveTracksParams {
    /// Tracks to delete, with their clips.
    pub tracks: Vec<PrimPath>,
}

impl Command for RemoveTracks {
    type Params = RemoveTracksParams;
    type Output = ();
    const ID: &'static str = "track.remove";
    const TITLE: &'static str = "Delete Tracks";
    const DESCRIPTION: &'static str = "Delete tracks and the clips on them. Clips linked to deleted clips are unlinked.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for track in &p.tracks {
            require_track(tx.stage(), track)?;
            let seq = track.parent().expect("tracks have a parent");
            let removed: Vec<_> = query::clips(tx.stage(), track);
            delete(tx, track)?;
            // Unlink partners that are left alone.
            for c in removed {
                let Some(link) = c.link else { continue };
                let rest: Vec<_> = query::sequence_clips(tx.stage(), &seq)
                    .into_iter()
                    .filter(|o| o.link.as_deref() == Some(link.as_str()))
                    .collect();
                if rest.len() == 1 {
                    unset(tx, &rest[0].path, attrs::LINK)?;
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SetTrackState;

#[derive(Deserialize, JsonSchema)]
pub struct SetTrackStateParams {
    pub track: PrimPath,
    /// Lock the track (no edits on its clips).
    #[serde(default)]
    pub locked: Option<bool>,
    /// Video: track output on or off (the eye).
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Audio: mute.
    #[serde(default)]
    pub muted: Option<bool>,
    /// Audio: solo.
    #[serde(default)]
    pub solo: Option<bool>,
    /// Custom name; empty string clears it.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for SetTrackState {
    type Params = SetTrackStateParams;
    type Output = ();
    const ID: &'static str = "track.set_state";
    const TITLE: &'static str = "Set Track State";
    const DESCRIPTION: &'static str =
        "Lock or unlock a track, toggle video track output, mute or solo an audio track, or rename it.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_track(tx.stage(), &p.track)?;
        let t = &p.track;
        if let Some(v) = p.locked {
            set(tx, t, attrs::LOCKED, Value::Bool(v))?;
        }
        if let Some(v) = p.enabled {
            set(tx, t, attrs::ENABLED, Value::Bool(v))?;
        }
        if let Some(v) = p.muted {
            set(tx, t, "muted", Value::Bool(v))?;
        }
        if let Some(v) = p.solo {
            set(tx, t, attrs::SOLO, Value::Bool(v))?;
        }
        if let Some(n) = p.name {
            set(tx, t, "name", Value::String(n.trim().to_owned()))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SetMarks;

#[derive(Deserialize, JsonSchema)]
pub struct SetMarksParams {
    /// A sequence (program monitor marks) or a source item: asset, comp or sequence
    /// (source monitor marks).
    pub item: PrimPath,
    /// New in mark (I); snapped to frames.
    #[serde(default)]
    pub mark_in: Option<Time>,
    /// New out mark (O), exclusive; snapped to frames.
    #[serde(default)]
    pub mark_out: Option<Time>,
    /// Clear the in mark (Ctrl/Opt+Shift+I).
    #[serde(default)]
    pub clear_in: bool,
    /// Clear the out mark.
    #[serde(default)]
    pub clear_out: bool,
}

impl Command for SetMarks {
    type Params = SetMarksParams;
    type Output = ();
    const ID: &'static str = "marks.set";
    const TITLE: &'static str = "Mark In/Out";
    const DESCRIPTION: &'static str = "Set or clear the in and out marks of a sequence or a source item (asset, comp, sequence); insert and overwrite use them as three-point edit defaults.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let stage = tx.stage();
        if query::source_kind(stage, &p.item).is_none() {
            return Err(fail(format!(
                "{} is not a sequence, comp or asset",
                p.item
            )));
        }
        let rate = query::sequence_info(stage, &p.item)
            .map(|s| s.frame_rate)
            .or_else(|| {
                stage.prim(&p.item).and_then(|v| {
                    query::static_value(&v, "frame_rate").and_then(|v| match v {
                        Value::FrameRate(r) if r.num > 0 && r.den > 0 => Some(r),
                        _ => None,
                    })
                })
            })
            .unwrap_or(FrameRate::FPS_24);
        let (old_in, old_out) = query::marks(stage, &p.item);
        let new_in = if p.clear_in {
            None
        } else {
            p.mark_in.map(|t| query::snap(rate, t)).or(old_in)
        };
        let new_out = if p.clear_out {
            None
        } else {
            p.mark_out.map(|t| query::snap(rate, t)).or(old_out)
        };
        if let (Some(i), Some(o)) = (new_in, new_out)
            && o <= i
        {
            return Err(fail("the out mark must be after the in mark"));
        }
        if new_in.is_some_and(|t| t < Time::ZERO) || new_out.is_some_and(|t| t < Time::ZERO) {
            return Err(fail("marks cannot be negative"));
        }
        match new_in {
            Some(t) if Some(t) != old_in => set_time(tx, &p.item, attrs::MARK_IN, t)?,
            None if old_in.is_some() => unset(tx, &p.item, attrs::MARK_IN)?,
            _ => {}
        }
        match new_out {
            Some(t) if Some(t) != old_out => set_time(tx, &p.item, attrs::MARK_OUT, t)?,
            None if old_out.is_some() => unset(tx, &p.item, attrs::MARK_OUT)?,
            _ => {}
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct AddMarker;

#[derive(Deserialize, JsonSchema)]
pub struct AddMarkerParams {
    pub sequence: PrimPath,
    /// Sequence time; snapped to frames.
    pub time: Time,
    /// Marker comment.
    #[serde(default)]
    pub comment: Option<String>,
    /// Marker duration (default 0).
    #[serde(default)]
    pub duration: Option<Time>,
}

impl Command for AddMarker {
    type Params = AddMarkerParams;
    type Output = PathOutput;
    const ID: &'static str = "sequence.add_marker";
    const TITLE: &'static str = "Add Marker";
    const DESCRIPTION: &'static str = "Add a sequence marker (M) at a time.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PathOutput, CommandError> {
        let seq = require_sequence(tx.stage(), &p.sequence)?;
        let folder = child(&p.sequence, MARKERS)?;
        if !tx.stage().has_prim(&folder) {
            create(tx, &folder, PrimType::FOLDER, None)?;
        }
        let path = child(
            &folder,
            &query::unique_child_name(tx.stage(), &folder, "marker"),
        )?;
        create(tx, &path, PrimType::MARKER, None)?;
        set_time(tx, &path, "time", snap(&seq, p.time).max(Time::ZERO))?;
        if let Some(d) = p.duration.filter(|d| d.0 > 0) {
            set_time(tx, &path, "duration", snap(&seq, d))?;
        }
        if let Some(c) = p.comment.filter(|c| !c.is_empty()) {
            set(tx, &path, "comment", Value::String(c))?;
        }
        Ok(PathOutput { path })
    }
}
