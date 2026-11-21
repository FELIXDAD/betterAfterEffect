//! Trim commands: `clip.trim`, `clip.ripple_trim`, `clip.roll`, `clip.slip`,
//! `clip.slide`. Linked clips are trimmed with the clip by default.

use std::collections::BTreeSet;

use bae_model::{Command, CommandError, PrimPath, Time, Transaction};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::clip::Edge;
use crate::query::{self, ClipInfo, SequenceInfo};
use crate::util::{
    Placement, check_placement, expand_linked, fail, require_sequence, shift_from, snap,
    write_placement,
};

fn default_true() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
pub struct TrimParams {
    pub clip: PrimPath,
    /// in (head) or out (tail).
    pub edge: Edge,
    /// How far the edge moves (flicks, snapped to frames); positive = later.
    pub delta: Time,
    /// Trim linked clips too (default true).
    #[serde(default = "default_true")]
    pub linked: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct ShiftParams {
    pub clip: PrimPath,
    /// Offset (flicks, snapped to frames); positive = later.
    pub delta: Time,
    /// Apply to linked clips too (default true).
    #[serde(default = "default_true")]
    pub linked: bool,
}

/// The clip, its linked partners and the sequence; `delta` snapped.
fn gather(
    tx: &Transaction<'_>,
    clip: &PrimPath,
    linked: bool,
    delta: Time,
) -> Result<(SequenceInfo, Vec<ClipInfo>, Time), CommandError> {
    let clips = expand_linked(tx.stage(), std::slice::from_ref(clip), linked)?;
    let seq_path = query::owning_sequence(tx.stage(), clip)
        .ok_or_else(|| fail("clip outside a sequence"))?;
    let seq = require_sequence(tx.stage(), &seq_path)?;
    let delta = snap(&seq, delta);
    Ok((seq, clips, delta))
}

/// The neighbor ending exactly where `c` starts / starting exactly where it ends.
fn neighbors(tx: &Transaction<'_>, c: &ClipInfo) -> (Option<ClipInfo>, Option<ClipInfo>) {
    let list = query::clips(tx.stage(), &c.track);
    let prev = list
        .iter()
        .find(|o| o.path != c.path && o.end() == c.start)
        .cloned();
    let next = list
        .iter()
        .find(|o| o.path != c.path && o.start == c.end())
        .cloned();
    (prev, next)
}

/// The free time on `c`'s track before and after it (to the nearest clip, or the
/// sequence start).
fn free_space(tx: &Transaction<'_>, c: &ClipInfo) -> (Time, Option<Time>) {
    let list = query::clips(tx.stage(), &c.track);
    let before = list
        .iter()
        .filter(|o| o.path != c.path && o.end() <= c.start)
        .map(|o| c.start - o.end())
        .min()
        .unwrap_or(c.start);
    let after = list
        .iter()
        .filter(|o| o.path != c.path && o.start >= c.end())
        .map(|o| o.start - c.end())
        .min();
    (before, after)
}

fn trim_edge(p: &Placement, edge: Edge, delta: Time) -> Placement {
    match edge {
        Edge::In => Placement {
            start: p.start + delta,
            duration: p.duration - delta,
            source_in: p.source_in + delta,
        },
        Edge::Out => Placement {
            duration: p.duration + delta,
            ..*p
        },
    }
}

// ---------------------------------------------------------------------------

pub struct TrimClip;

impl Command for TrimClip {
    type Params = TrimParams;
    type Output = ();
    const ID: &'static str = "clip.trim";
    const TITLE: &'static str = "Trim";
    const DESCRIPTION: &'static str = "Move a clip's in or out edge (Selection tool trim), leaving or filling a gap. The edge cannot run into the neighboring clip or past the source media.";

    fn run(&self, tx: &mut Transaction<'_>, p: TrimParams) -> Result<(), CommandError> {
        let (seq, clips, delta) = gather(tx, &p.clip, p.linked, p.delta)?;
        let mut plans = Vec::new();
        for c in &clips {
            let (before, after) = free_space(tx, c);
            match p.edge {
                Edge::In if -delta > before => {
                    return Err(fail(format!("{} would overlap the clip before", c.name)));
                }
                Edge::Out if after.is_some_and(|a| delta > a) => {
                    return Err(fail(format!("{} would overlap the clip after", c.name)));
                }
                _ => {}
            }
            let np = trim_edge(&Placement::of(c), p.edge, delta);
            check_placement(tx.stage(), &seq, c, &np)?;
            plans.push((c.clone(), np));
        }
        for (c, np) in plans {
            write_placement(tx, &c, &np)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct RippleTrim;

impl Command for RippleTrim {
    type Params = TrimParams;
    type Output = ();
    const ID: &'static str = "clip.ripple_trim";
    const TITLE: &'static str = "Ripple Edit";
    const DESCRIPTION: &'static str = "Ripple edit (Ripple Edit tool, B): move a clip's in or out edge and shift everything after it on the track so no gap opens or closes. Trimming the in edge keeps the clip's start; later clips move by the change in length.";

    fn run(&self, tx: &mut Transaction<'_>, p: TrimParams) -> Result<(), CommandError> {
        let (seq, clips, delta) = gather(tx, &p.clip, p.linked, p.delta)?;
        let mut plans = Vec::new();
        for c in &clips {
            let old = Placement::of(c);
            let np = match p.edge {
                // Content changes at the head; the clip stays where it starts.
                Edge::In => Placement {
                    start: old.start,
                    duration: old.duration - delta,
                    source_in: old.source_in + delta,
                },
                Edge::Out => trim_edge(&old, Edge::Out, delta),
            };
            check_placement(tx.stage(), &seq, c, &np)?;
            plans.push((c.clone(), np));
        }
        for (c, np) in plans {
            let change = np.duration - c.duration;
            let skip: BTreeSet<PrimPath> = [c.path.clone()].into();
            shift_from(tx, &c.track, c.end(), change, &skip)?;
            write_placement(tx, &c, &np)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct RollEdit;

#[derive(Deserialize, JsonSchema)]
pub struct RollParams {
    /// The outgoing clip: the edit point is its out edge (and the next clip's in edge).
    pub clip: PrimPath,
    /// How far the edit point moves (flicks, snapped to frames); positive = later.
    pub delta: Time,
    /// Roll linked clips too (default true).
    #[serde(default = "default_true")]
    pub linked: bool,
}

impl Command for RollEdit {
    type Params = RollParams;
    type Output = ();
    const ID: &'static str = "clip.roll";
    const TITLE: &'static str = "Rolling Edit";
    const DESCRIPTION: &'static str = "Rolling edit (Rolling Edit tool, N): move the cut between a clip and the clip right after it; the total length does not change. Without a clip after it, the out edge moves into the gap.";

    fn run(&self, tx: &mut Transaction<'_>, p: RollParams) -> Result<(), CommandError> {
        let (seq, clips, delta) = gather(tx, &p.clip, p.linked, p.delta)?;
        let mut plans = Vec::new();
        for c in &clips {
            let (_, next) = neighbors(tx, c);
            let out = trim_edge(&Placement::of(c), Edge::Out, delta);
            check_placement(tx.stage(), &seq, c, &out)?;
            match next {
                Some(n) => {
                    let np = trim_edge(&Placement::of(&n), Edge::In, delta);
                    check_placement(tx.stage(), &seq, &n, &np)?;
                    plans.push((n, np));
                }
                None => {
                    let (_, after) = free_space(tx, c);
                    if after.is_some_and(|a| delta > a) {
                        return Err(fail(format!("{} would overlap the clip after", c.name)));
                    }
                }
            }
            plans.push((c.clone(), out));
        }
        for (c, np) in plans {
            write_placement(tx, &c, &np)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SlipClip;

impl Command for SlipClip {
    type Params = ShiftParams;
    type Output = ();
    const ID: &'static str = "clip.slip";
    const TITLE: &'static str = "Slip";
    const DESCRIPTION: &'static str = "Slip (Slip tool, Y): show a later (positive delta) or earlier part of the source in the same place and length.";

    fn run(&self, tx: &mut Transaction<'_>, p: ShiftParams) -> Result<(), CommandError> {
        let (seq, clips, delta) = gather(tx, &p.clip, p.linked, p.delta)?;
        let mut plans = Vec::new();
        for c in &clips {
            let old = Placement::of(c);
            let np = Placement {
                source_in: old.source_in + delta,
                ..old
            };
            check_placement(tx.stage(), &seq, c, &np)?;
            plans.push((c.clone(), np));
        }
        for (c, np) in plans {
            write_placement(tx, &c, &np)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SlideClip;

impl Command for SlideClip {
    type Params = ShiftParams;
    type Output = ();
    const ID: &'static str = "clip.slide";
    const TITLE: &'static str = "Slide";
    const DESCRIPTION: &'static str = "Slide (Slide tool, U): move a clip in time keeping its content; the clip before gets longer or shorter at its tail and the clip after at its head, so the total length does not change.";

    fn run(&self, tx: &mut Transaction<'_>, p: ShiftParams) -> Result<(), CommandError> {
        let (seq, clips, delta) = gather(tx, &p.clip, p.linked, p.delta)?;
        let mut plans = Vec::new();
        for c in &clips {
            let (prev, next) = neighbors(tx, c);
            let (before, after) = free_space(tx, c);
            match &prev {
                Some(pr) => {
                    let np = trim_edge(&Placement::of(pr), Edge::Out, delta);
                    check_placement(tx.stage(), &seq, pr, &np)?;
                    plans.push((pr.clone(), np));
                }
                None if -delta > before => {
                    return Err(fail(format!("{} would overlap the clip before", c.name)));
                }
                None => {}
            }
            match &next {
                Some(nx) => {
                    let np = trim_edge(&Placement::of(nx), Edge::In, delta);
                    check_placement(tx.stage(), &seq, nx, &np)?;
                    plans.push((nx.clone(), np));
                }
                None if after.is_some_and(|a| delta > a) => {
                    return Err(fail(format!("{} would overlap the clip after", c.name)));
                }
                None => {}
            }
            let old = Placement::of(c);
            let np = Placement {
                start: old.start + delta,
                ..old
            };
            check_placement(tx.stage(), &seq, c, &np)?;
            plans.push((c.clone(), np));
        }
        for (c, np) in plans {
            write_placement(tx, &c, &np)?;
        }
        Ok(())
    }
}
