//! BAE motion domain commands (workstream 12): the After Effects operations of the
//! motion workspace, as `bae-model` [`Command`](bae_model::Command)s.
//!
//! The motion UI (`bae-ui-motion`), shortcuts, menus, scripts and agents all run these
//! commands, so every operation is one undo step with an author and is available to
//! agents through MCP (ARCHITECTURE §2, rules 1 and 2). This crate has no GPU, window
//! or platform dependencies.
//!
//! | Command id | AE equivalent |
//! |---|---|
//! | `comp.create` | Composition > New Composition (Cmd/Ctrl+N) |
//! | `comp.set_work_area` | B / N (work area start / end) |
//! | `comp.trim_to_work_area` | Composition > Trim Comp to Work Area |
//! | `folder.create` | Project panel > New Folder |
//! | `layer.create` | Layer > New > Shape / Text / Solid / Null / Camera / Light / Adjustment; drag an item into a comp |
//! | `layer.reorder` | drag layers; Bring Forward / Send Backward (Cmd/Ctrl+] / [) |
//! | `layer.duplicate` | Edit > Duplicate (Cmd/Ctrl+D) |
//! | `layer.split` | Edit > Split Layer (Cmd/Ctrl+Shift+D) |
//! | `layer.trim` | Alt+[ / Alt+] (trim in / out point) |
//! | `layer.shift_time` | dragging a layer bar; [ / ] (move in / out point to the current time) |
//! | `layer.set_parent` | Parent & Link column, pick whip |
//! | `layer.delete`, `item.delete` | Delete (layers; project items) |
//! | `attr.set_animated` | the stopwatch |
//! | `attr.set_at` | editing a value in the timeline at the current time |
//! | `attr.set_many` | several such edits at once (moving several layers in the viewer) |
//! | `key.ease` | Keyframe Assistant > Easy Ease (F9), Ease In (Shift+F9), Ease Out (Cmd/Ctrl+Shift+F9) |
//! | `key.set_interpolation` | Keyframe Interpolation (Cmd/Ctrl+Alt+K), Toggle Hold Keyframe |
//! | `key.move` | dragging keyframes |
//! | `key.delete` | deleting keyframes |
//! | `key.reverse` | Keyframe Assistant > Time-Reverse Keyframes |
//! | `key.paste` | pasting keyframes |
//! | `shape.add` | drawing with a shape tool or the Pen tool in (or into a new) shape layer |
//! | `mask.create` | drawing with a shape tool or the Pen tool on a layer that gets a mask |
//!
//! **Times** are flicks (705,600,000 per second). Layer in/out points, split and work
//! area times are comp times. Keyframe times are the stored keyframe times — layer
//! time for layer attributes (`docs/spec/data-model.md` §10); [`query::to_key_time`]
//! converts.
//!
//! Register the commands into a [`CommandRegistry`] with [`register_commands`], or add
//! [`MotionModule`] to the app: it publishes them in `bae_app::Registry<dyn DynCommand>`
//! (id = command id), from which the application builds its command registry.

pub mod comp;
pub mod keys;
pub mod layer;
pub mod query;
pub mod shape;
mod util;

use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};
use bae_model::{CommandRegistry, DynCommand};

pub use comp::{CompOutput, CreateComp, CreateFolder, SetWorkArea, TrimCompToWorkArea};
pub use keys::{
    CopiedKey, DeleteKeys, EaseKeys, EaseMode, KeyRef, KeysOutput, MoveKeys, PasteKeys,
    ReverseKeys, SetAnimated, SetAt, SetInterpolation, SetMany, ValueEdit,
};
pub use layer::{
    CreateLayer, DeleteItems, DeleteLayers, DuplicateLayers, LayerOutput, LayersOutput,
    ReorderLayers, SetParent, ShapePreset, ShiftLayers, SplitLayers, TrimLayers,
};
pub use shape::{AddShape, CreateMask, Geometry, ShapeKind};

/// Register every motion command.
pub fn register_commands(r: &mut CommandRegistry) {
    let results = [
        r.register(CreateComp),
        r.register(SetWorkArea),
        r.register(TrimCompToWorkArea),
        r.register(CreateFolder),
        r.register(CreateLayer),
        r.register(ReorderLayers),
        r.register(DuplicateLayers),
        r.register(SplitLayers),
        r.register(TrimLayers),
        r.register(ShiftLayers),
        r.register(DeleteLayers),
        r.register(DeleteItems),
        r.register(SetParent),
        r.register(SetAnimated),
        r.register(SetAt),
        r.register(SetMany),
        r.register(EaseKeys),
        r.register(SetInterpolation),
        r.register(MoveKeys),
        r.register(DeleteKeys),
        r.register(ReverseKeys),
        r.register(PasteKeys),
        r.register(AddShape),
        r.register(CreateMask),
    ];
    for res in results {
        res.expect("motion command IDs are unique");
    }
}

/// The motion commands as type-erased commands, in registration order.
pub fn commands() -> Vec<Arc<dyn DynCommand>> {
    let mut r = CommandRegistry::new();
    register_commands(&mut r);
    let ids: Vec<String> = r.infos().map(|i| i.id.clone()).collect();
    ids.iter().filter_map(|id| r.get(id).cloned()).collect()
}

/// App module: publishes the motion commands in `Registry<dyn DynCommand>`.
#[derive(Debug, Default)]
pub struct MotionModule;

impl AppModule for MotionModule {
    fn name(&self) -> &str {
        "bae-motion"
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
