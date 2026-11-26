//! What an expression read, for callers that keep values derived from one
//! (a particle system's birth rate history, a cached frame) and need to know
//! whether an edit can change them
//! ([`crate::Evaluator::evaluate_attribute_traced`]).

use bae_model::{Change, PrimPath, Time};

use crate::schema::{comp as c, layer as l};

/// Which part of a prim was read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReadKind {
    /// One attribute.
    Attr(String),
    /// Any attribute of the prim itself, not of prims under it: a layer's
    /// transform, parent, name and timing (`toComp`, `thisLayer.inPoint`), a
    /// comp's or footage item's settings.
    Prim,
    /// The prim and everything under it: a layer's pixels (`sampleImage`).
    Subtree,
}

/// One read made while an attribute was evaluated.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprRead {
    /// The prim read: a layer, a prim under it (effect, mask, shape), a comp,
    /// a footage item or the project.
    pub prim: PrimPath,
    /// What of it.
    pub kind: ReadKind,
    /// Comp time of the read, in the comp that holds `prim`. `None`: not one
    /// time (keyframe lists, names, settings).
    pub time: Option<Time>,
}

/// Attributes that move every value under their prim in time: a layer's
/// timing, a comp's frame grid.
const TIMING: &[&str] = &[
    l::START_TIME,
    l::IN_POINT,
    l::OUT_POINT,
    l::STRETCH,
    l::TIME_REMAP_ENABLED,
    l::TIME_REMAP,
    c::FRAME_RATE,
    c::DURATION,
];

impl ExprRead {
    pub(crate) fn attr(prim: PrimPath, attr: &str, time: Option<Time>) -> Self {
        Self {
            prim,
            kind: ReadKind::Attr(attr.to_owned()),
            time,
        }
    }

    pub(crate) fn prim(prim: PrimPath, kind: ReadKind, time: Option<Time>) -> Self {
        Self { prim, kind, time }
    }

    /// Whether `change` may alter what was read. Conservative where expressions
    /// look things up: structural changes (prims created, removed, moved or
    /// reordered, retyped, resynced) and renames always count, since layers,
    /// effects and masks are found by name and index; so does a timing change of
    /// the read prim or an enclosing layer or comp (start time, in / out points,
    /// stretch, time remap, frame rate).
    pub fn touched_by(&self, change: &Change) -> bool {
        let Change::AttrChanged { path, attr, .. } = change else {
            return true;
        };
        if attr == l::NAME || (path.is_prefix_of(&self.prim) && TIMING.contains(&attr.as_str())) {
            return true;
        }
        match &self.kind {
            ReadKind::Attr(a) => path == &self.prim && a == attr,
            ReadKind::Prim => path == &self.prim,
            ReadKind::Subtree => self.prim.is_prefix_of(path),
        }
    }
}

/// Whether any change of `changes` may alter any of `reads`.
pub fn reads_touched<'a>(
    reads: &[ExprRead],
    changes: impl IntoIterator<Item = &'a Change>,
) -> bool {
    changes
        .into_iter()
        .any(|ch| reads.iter().any(|r| r.touched_by(ch)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::TimeRange;

    fn p(s: &str) -> PrimPath {
        PrimPath::new(s).unwrap()
    }

    fn set(path: &str, attr: &str) -> Change {
        Change::AttrChanged {
            path: p(path),
            attr: attr.into(),
            range: TimeRange::ALL,
        }
    }

    #[test]
    fn touched_by_matching_attr_timing_names_and_structure() {
        let l = "/comps/c/layers/a";
        let fx = "/comps/c/layers/a/effects/slider";
        let read = ExprRead::attr(p(fx), "param:0001", Some(Time::ZERO));
        assert!(read.touched_by(&set(fx, "param:0001")));
        assert!(!read.touched_by(&set(fx, "param:0002")));
        assert!(!read.touched_by(&set(l, "transform:position")));
        assert!(read.touched_by(&set(l, "start_time")), "layer timing");
        assert!(read.touched_by(&set("/comps/c", "frame_rate")), "comp grid");
        assert!(
            read.touched_by(&set("/comps/c/layers/b", "name")),
            "lookups by name"
        );
        assert!(read.touched_by(&Change::PrimCreated {
            path: p("/comps/c/layers/z")
        }));

        let whole = ExprRead::prim(p(l), ReadKind::Prim, None);
        assert!(whole.touched_by(&set(l, "transform:scale")));
        assert!(
            !whole.touched_by(&set(fx, "param:0001")),
            "not prims under it"
        );
        let pixels = ExprRead::prim(p(l), ReadKind::Subtree, Some(Time::ZERO));
        assert!(pixels.touched_by(&set(fx, "param:0001")));
        assert!(!pixels.touched_by(&set("/comps/c/layers/b", "transform:scale")));
        assert!(reads_touched(&[read, whole], [&set(l, "transform:scale")]));
    }
}
