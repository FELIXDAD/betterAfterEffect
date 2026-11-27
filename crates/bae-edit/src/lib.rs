//! BAE editing domain commands (workstream 14, framework stage): sequences, tracks
//! and clips as `bae-model` [`Command`](bae_model::Command)s.
//!
//! The edit workspace UI (`bae-ui-edit`), shortcuts, scripts and agents all run these
//! commands, so every edit is one undo step with an author and is available to agents
//! through MCP (ARCHITECTURE §2, rules 1 and 2). No GPU, window or platform deps.
//!
//! | Command id | Premiere equivalent |
//! |---|---|
//! | `sequence.create` | File > New > Sequence |
//! | `sequence.add_marker` | Add Marker (M) |
//! | `marks.set` | Mark In / Mark Out (I / O), clear marks |
//! | `track.add` / `track.remove` | Add Tracks / Delete Tracks |
//! | `track.set_state` | lock, toggle track output, mute, solo, rename |
//! | `clip.insert` / `clip.overwrite` | Insert (,) / Overwrite (.), three-point editing |
//! | `clip.lift` / `clip.ripple_delete` | Clear (Delete) / Ripple Delete (Shift+Delete) |
//! | `clip.razor` | Razor tool (C), Add Edit (Cmd/Ctrl+K) |
//! | `clip.move` | Selection tool drag |
//! | `clip.trim` / `clip.ripple_trim` / `clip.roll` | trim, Ripple Edit (B), Rolling Edit (N) |
//! | `clip.slip` / `clip.slide` | Slip (Y), Slide (U) |
//! | `clip.link` / `clip.unlink` | Link / Unlink |
//! | `clip.nest` | Nest |
//! | `clip.set_transition` | Cross Dissolve / Dip to Black |
//!
//! **Model** (prim types from `bae-model`): `/sequences/<name>` is a `Sequence`; its
//! `Track` children are video tracks V1, V2 … then audio tracks A1, A2 … (labels come
//! from position, `track_kind` says which); `Clip` prims live under their track with
//! `source` (asset, comp — a live reference to the motion comp — or nested sequence),
//! `start` (sequence time), `duration` and `source_in` (source time). Extra
//! attributes are listed in [`query::attrs`]. Times are flicks, snapped to the
//! sequence frame rate.
//!
//! Register the commands with [`register_commands`], or add [`EditModule`] to the app:
//! it publishes them in `bae_app::Registry<dyn DynCommand>`.

pub mod clip;
pub mod nest;
pub mod query;
pub mod sequence;
pub mod trim;
mod util;

use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};
use bae_model::{CommandRegistry, DynCommand};

pub use clip::{
    ClipsOutput, Edge, EditRange, InsertClip, LiftClips, LinkClips, MoveClips, OverwriteClip,
    PlaceParams, Razor, RippleDelete, SetTransition, UnlinkClips, resolve_three_point,
};
pub use nest::{NestClips, NestOutput};
pub use sequence::{
    AddMarker, AddTrack, CreateSequence, PathOutput, RemoveTracks, SequenceOutput, SetMarks,
    SetTrackState,
};
pub use trim::{RippleTrim, RollEdit, SlideClip, SlipClip, TrimClip};

/// Register every edit command.
pub fn register_commands(r: &mut CommandRegistry) {
    let results = [
        r.register(CreateSequence),
        r.register(AddMarker),
        r.register(SetMarks),
        r.register(AddTrack),
        r.register(RemoveTracks),
        r.register(SetTrackState),
        r.register(InsertClip),
        r.register(OverwriteClip),
        r.register(LiftClips),
        r.register(RippleDelete),
        r.register(Razor),
        r.register(MoveClips),
        r.register(TrimClip),
        r.register(RippleTrim),
        r.register(RollEdit),
        r.register(SlipClip),
        r.register(SlideClip),
        r.register(LinkClips),
        r.register(UnlinkClips),
        r.register(NestClips),
        r.register(SetTransition),
    ];
    for res in results {
        res.expect("edit command IDs are unique");
    }
}

/// The edit commands as type-erased commands, in registration order.
pub fn commands() -> Vec<Arc<dyn DynCommand>> {
    let mut r = CommandRegistry::new();
    register_commands(&mut r);
    let ids: Vec<String> = r.infos().map(|i| i.id.clone()).collect();
    ids.iter().filter_map(|id| r.get(id).cloned()).collect()
}

/// App module: publishes the edit commands in `Registry<dyn DynCommand>`.
#[derive(Debug, Default)]
pub struct EditModule;

impl AppModule for EditModule {
    fn name(&self) -> &str {
        "bae-edit"
    }

    fn build(&self, app: &mut AppBuilder) {
        let owner = app.current_owner();
        let registry = app.registry::<dyn DynCommand>();
        registry.batch(|tx| {
            for cmd in commands() {
                let id = cmd.info().id.clone();
                if !tx.contains(&id) {
                    let _ = tx.register_arc(owner.clone(), id, cmd);
                }
            }
        });
    }
}
