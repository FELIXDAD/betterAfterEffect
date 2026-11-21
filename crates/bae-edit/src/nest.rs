//! `clip.nest`: replace clips with one clip of a new sequence that contains them.

use std::collections::{BTreeMap, BTreeSet};

use bae_model::{AttrSpec, Command, CommandError, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, SEQUENCES_FOLDER, TrackKind, attrs};
use crate::util::{
    Splits, child, clear_range, composed_tree, create, delete, ensure_folder, expand_linked, fail,
    new_link_id, require_sequence, restore, set, set_time,
};

pub struct NestClips;

#[derive(Deserialize, JsonSchema)]
pub struct NestParams {
    /// Clips to nest (all in one sequence). Linked clips are included.
    pub clips: Vec<PrimPath>,
    /// Name of the new sequence (default "Nested Sequence N").
    #[serde(default)]
    pub name: Option<String>,
}

/// Output of `clip.nest`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NestOutput {
    /// The new sequence.
    pub sequence: PrimPath,
    /// The clips that now play it in the original sequence (video first).
    pub clips: Vec<PrimPath>,
}

impl Command for NestClips {
    type Params = NestParams;
    type Output = NestOutput;
    const ID: &'static str = "clip.nest";
    const TITLE: &'static str = "Nest";
    const DESCRIPTION: &'static str = "Move clips into a new sequence (same settings, same track layout, starting at 0) and put one clip of that sequence where they were: on the lowest of their video tracks and, if any audio was nested, linked on the lowest audio track.";

    fn run(&self, tx: &mut Transaction<'_>, p: NestParams) -> Result<NestOutput, CommandError> {
        let clips = expand_linked(tx.stage(), &p.clips, true)?;
        let Some(first) = clips.first() else {
            return Err(fail("nothing to nest"));
        };
        let seq_path = query::owning_sequence(tx.stage(), &first.track)
            .ok_or_else(|| fail("clip outside a sequence"))?;
        let seq = require_sequence(tx.stage(), &seq_path)?;
        if clips
            .iter()
            .any(|c| query::owning_sequence(tx.stage(), &c.track).as_ref() != Some(&seq.path))
        {
            return Err(fail("nested clips must be in the same sequence"));
        }
        let tracks = query::tracks(tx.stage(), &seq.path);
        let start = clips.iter().map(|c| c.start).min().expect("non-empty");
        let end = clips.iter().map(|c| c.end()).max().expect("non-empty");

        // The new sequence, with the same frame settings and as many tracks of each
        // kind as needed to keep every clip on the same track number.
        let folder = PrimPath::new(SEQUENCES_FOLDER).expect("valid");
        ensure_folder(tx, &folder)?;
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| next_nest_name(tx));
        let nested = child(
            &folder,
            &query::unique_child_name(tx.stage(), &folder, &name),
        )?;
        create(tx, &nested, PrimType::SEQUENCE, None)?;
        set(tx, &nested, "name", Value::String(name))?;
        set(tx, &nested, "width", Value::Int(i64::from(seq.width)))?;
        set(tx, &nested, "height", Value::Int(i64::from(seq.height)))?;
        set(tx, &nested, "frame_rate", Value::FrameRate(seq.frame_rate))?;
        let track_of = |c: &query::ClipInfo| {
            tracks
                .iter()
                .find(|t| t.path == c.track)
                .expect("track of the sequence")
        };
        let mut new_tracks: BTreeMap<(TrackKind, usize), PrimPath> = BTreeMap::new();
        for kind in [TrackKind::Video, TrackKind::Audio] {
            let needed = clips
                .iter()
                .map(track_of)
                .filter(|t| t.kind == kind)
                .map(|t| t.index + 1)
                .max()
                .unwrap_or(0);
            for i in 0..needed {
                let base = match kind {
                    TrackKind::Video => "video",
                    TrackKind::Audio => "audio",
                };
                let path = child(
                    &nested,
                    &query::unique_child_name(tx.stage(), &nested, base),
                )?;
                create(tx, &path, PrimType::TRACK, None)?;
                set(tx, &path, "track_kind", Value::Enum(kind.as_str().to_owned()))?;
                new_tracks.insert((kind, i), path);
            }
        }

        // Move the clips (copies of their composed state, shifted to start at 0).
        for c in &clips {
            let t = track_of(c);
            let dest = &new_tracks[&(t.kind, t.index)];
            let mut tree = composed_tree(tx.stage(), &c.path);
            tree.spec.attrs.insert(
                "start".to_owned(),
                AttrSpec {
                    value: Some(Value::Time(c.start - start)),
                    ..AttrSpec::default()
                },
            );
            delete(tx, &c.path)?;
            restore(tx, &child(dest, c.path.name())?, tree)?;
        }

        // One clip of the nested sequence in place of the originals.
        let lowest = |kind: TrackKind| {
            clips
                .iter()
                .map(track_of)
                .filter(|t| t.kind == kind)
                .min_by_key(|t| t.index)
                .map(|t| t.path.clone())
        };
        let targets: Vec<PrimPath> = [lowest(TrackKind::Video), lowest(TrackKind::Audio)]
            .into_iter()
            .flatten()
            .collect();
        let mut splits = Splits::default();
        let none = BTreeSet::new();
        for t in &targets {
            clear_range(tx, &mut splits, &seq.path, t, start, end, &none)?;
        }
        splits.finish(tx, &seq.path)?;
        let link = (targets.len() > 1).then(|| new_link_id(tx.stage(), &seq.path, &none));
        let mut out = Vec::new();
        let mut names = BTreeSet::new();
        for t in targets {
            let name = crate::util::new_clip_name(tx.stage(), &seq.path, &names);
            names.insert(name.clone());
            let path = child(&t, &name)?;
            create(tx, &path, PrimType::CLIP, None)?;
            set(tx, &path, "source", Value::PrimRef(nested.clone()))?;
            set_time(tx, &path, "start", start)?;
            set_time(tx, &path, "duration", end - start)?;
            if let Some(l) = &link {
                set(tx, &path, attrs::LINK, Value::String(l.clone()))?;
            }
            out.push(path);
        }
        Ok(NestOutput {
            sequence: nested,
            clips: out,
        })
    }
}

fn next_nest_name(tx: &Transaction<'_>) -> String {
    let stage = tx.stage();
    let max = query::sequences(stage)
        .iter()
        .filter_map(|s| {
            query::display_name(stage, s)
                .strip_prefix("Nested Sequence ")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    format!("Nested Sequence {}", max + 1)
}
