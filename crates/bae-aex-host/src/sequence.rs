//! Global, sequence and frame data → BAE's opaque per-instance state slot.
//!
//! AE effects keep three kinds of private data (SDK guide, "Effect Details ›
//! Global, Sequence, & Frame Data"):
//!
//! * **global data** — one per loaded module, created at global setup, never
//!   saved;
//! * **sequence data** — one per effect instance, created at sequence setup,
//!   *saved with the project* after `SEQUENCE_FLATTEN` turns it into a flat,
//!   byte-order-independent blob, rebuilt by `SEQUENCE_RESETUP` after load,
//!   duplication, precompose or a layer change;
//! * **frame data** — per legacy render call, never saved.
//!
//! Under Multi-Frame Rendering, sequence data is read-only during render and
//! is read through a dedicated suite; effects that must write it get a private
//! per-thread copy that is discarded after each render span; effects whose
//! sequence data needs flattening must provide a flattened *copy* on demand
//! (`GET_FLATTENED_SEQUENCE_DATA`) so the host can marshal it to render
//! threads (SDK guide, "Effect Details › Multi-Frame Rendering in AE").
//!
//! BAE mapping (request R02-03): each effect Prim owns an opaque state slot
//! [`StateBlob`] stored in the edit-target sublayer and synced like any other
//! attribute (last writer wins — it is not mergeable). The compat host keeps
//! the *live* (unflattened) form in memory per process; the slot only ever
//! holds the flat form.

use crate::pipl::{EffectVersion, MatchName};

/// Format tag of the bytes inside a [`StateBlob`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateFormat {
    /// Flattened AE sequence data exactly as the plug-in produced it.
    AeFlatSequenceData,
    /// A BAE native plug-in's own serialization.
    BaeNative,
}

/// Opaque per-effect-instance state as persisted in the project and synced
/// between collaborators. The bytes are never interpreted by BAE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateBlob {
    /// What the bytes are.
    pub format: StateFormat,
    /// Producer identity (to refuse feeding one plug-in's state to another).
    pub producer: MatchName,
    /// Producer version at flatten time (lets a newer plug-in detect old data;
    /// AE itself does not version sequence data, plug-ins do it internally).
    pub producer_version: EffectVersion,
    /// The flat bytes.
    pub bytes: Vec<u8>,
}

/// Why the host is (re)building live sequence data — determines the selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreCause {
    /// Effect newly applied: `SEQUENCE_SETUP` (no blob).
    Applied,
    /// Project opened: `SEQUENCE_RESETUP` from the saved blob.
    ProjectLoaded,
    /// Effect duplicated / copy-pasted: flatten source, `RESETUP` both.
    Duplicated,
    /// Undo/redo changed the slot: `RESETUP` from the restored blob.
    UndoRedo,
    /// A collaborator's edit changed the slot (CRDT update from 09):
    /// `RESETUP` from the new blob. Not an AE concept; BAE-specific.
    RemoteChange,
    /// Layer the effect sits on changed (AE sends `RESETUP`).
    LayerChanged,
}

/// How render threads may access sequence data for one effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderStateAccess {
    /// Module is not MFR-safe: one call at a time per module, live data used
    /// directly.
    SerialLive,
    /// MFR-safe, read-only: every render thread reads one immutable snapshot
    /// (const access through the sequence-data suite).
    SharedReadOnly,
    /// MFR-safe but mutates during render: a private copy per render thread,
    /// re-created from the flat form for every render span and discarded
    /// afterwards.
    PrivateCopyPerThread,
}

impl RenderStateAccess {
    /// Derive from capabilities.
    pub fn from_caps(c: &crate::caps::EffectCaps) -> Self {
        match (c.threaded_rendering, c.mutable_render_sequence_data) {
            (false, _) => RenderStateAccess::SerialLive,
            (true, false) => RenderStateAccess::SharedReadOnly,
            (true, true) => RenderStateAccess::PrivateCopyPerThread,
        }
    }
}

/// Policy decision: when must the host flatten an instance?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlattenTrigger {
    /// Project save / autosave / `.baez` packaging.
    Save,
    /// Copy to clipboard or duplicate.
    Copy,
    /// Out-of-process marshalling or MFR render-thread copy (uses the
    /// non-destructive flattened-copy selector when available).
    Marshal,
    /// After a supervised param change or UI event that changed sequence data,
    /// so the new state reaches the slot (undo + sync). BAE-specific policy:
    /// AE only persists at save time, but BAE syncs continuously.
    StateChanged,
}

/// Whether the plug-in can produce a flat copy without disturbing live data.
pub fn can_snapshot_nondestructively(c: &crate::caps::EffectCaps) -> bool {
    c.get_flattened_sequence_data
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::EffectCaps;

    #[test]
    fn access_modes() {
        let mut c = EffectCaps::default();
        assert_eq!(
            RenderStateAccess::from_caps(&c),
            RenderStateAccess::SerialLive
        );
        c.threaded_rendering = true;
        assert_eq!(
            RenderStateAccess::from_caps(&c),
            RenderStateAccess::SharedReadOnly
        );
        c.mutable_render_sequence_data = true;
        assert_eq!(
            RenderStateAccess::from_caps(&c),
            RenderStateAccess::PrivateCopyPerThread
        );
        assert!(!can_snapshot_nondestructively(&c));
    }
}
