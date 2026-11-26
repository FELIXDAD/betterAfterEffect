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
//! | `comp.set_settings` | Composition > Composition Settings (Cmd/Ctrl+K) |
//! | `comp.set_work_area` | B / N (work area start / end) |
//! | `comp.trim_to_work_area` | Composition > Trim Comp to Work Area |
//! | `item.import` | File > Import > File (Cmd/Ctrl+I); files dropped on the Project panel |
//! | `folder.create` | Project panel > New Folder |
//! | `layer.create` | Layer > New > Shape / Text / Solid / Null / Camera / Light / Adjustment; drag an item into a comp |
//! | `camera.set_settings` | Layer > Camera Settings (Layer Settings, Cmd/Ctrl+Shift+Y, on a camera) |
//! | `light.set_settings` | Layer > Light Settings (Layer Settings on a light) |
//! | `solid.set_settings` | Layer > Solid Settings (Cmd/Ctrl+Shift+Y); Solid Settings of a solid item |
//! | `layer.precompose` | Layer > Pre-compose (Cmd/Ctrl+Shift+C) |
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
//! | `key.edit` | Graph Editor drags (key time / value, Bezier handles, transform box), keyframe dialogs |
//! | `transform.separate_dimensions` | Animation > Separate Dimensions (Position → X / Y / Z Position) |
//! | `shape.add` | drawing with a shape tool or the Pen tool in (or into a new) shape layer |
//! | `mask.create` | drawing with a shape tool or the Pen tool on a layer that gets a mask |
//! | `path.set_vertices` | Pen tool clicks, Convert Vertex tool, dragging path vertices and handles |
//! | `path.insert_vertex` | Add Vertex tool (every keyframe) |
//! | `path.delete_vertex` | Delete Vertex tool (every keyframe) |
//! | `text.create` | typing new point or paragraph text with the Type tool |
//! | `text.set_text` | editing a text layer's characters with the Type tool |
//! | `effect.apply` | Effect menu; double-click or drag in Effects & Presets |
//! | `effect.remove`, `effect.remove_all` | Delete with effects selected; Effect > Remove All |
//! | `effect.reorder` | dragging effects in Effect Controls or the timeline |
//! | `effect.set_enabled` | an effect's fx switch |
//! | `effect.reset` | Reset in Effect Controls |
//! | `effect.duplicate` | Edit > Duplicate with effects selected |
//! | `layer.time_remap` | Layer > Time > Enable Time Remapping (Cmd/Ctrl+Alt+T) |
//! | `layer.time_stretch` | Layer > Time > Time Stretch (stretch or new duration, hold in point / current frame / out point) |
//! | `layer.time_reverse` | Layer > Time > Time-Reverse Layer (Cmd/Ctrl+Alt+R) |
//! | `layer.freeze_frame` | Layer > Time > Freeze Frame |
//! | `layer.freeze_on_last_frame` | Layer > Time > Freeze On Last Frame |
//!
//! **Times** are flicks (705,600,000 per second). Layer in/out points, split and work
//! area times are comp times. Keyframe times are the stored keyframe times — layer
//! time for layer attributes (`docs/spec/data-model.md` §10); [`query::to_key_time`]
//! converts.
//!
//! Register the commands into a [`CommandRegistry`] with [`register_commands`], or add
//! [`MotionModule`] to the app: it publishes them in `bae_app::Registry<dyn DynCommand>`
//! (id = command id), from which the application builds its command registry.

pub mod camera;
pub mod comp;
pub mod content;
pub mod dimensions;
pub mod effect;
pub mod expressions;
pub mod import;
pub mod keys;
pub mod layer;
pub mod markers;
pub mod matte;
pub mod names;
pub mod orient;
pub mod paint;
pub mod path;
pub mod precompose;
pub mod project_settings;
pub mod query;
pub mod shape;
pub mod solid;
pub mod text;
pub mod text_animators;
pub mod text_path;
pub mod time;
pub mod tracker;
mod util;

use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};
use bae_model::{CommandRegistry, DynCommand};

pub use camera::{CameraSettings, LightSettings, SetCameraSettings, SetLightSettings};
pub use comp::{
    CompOutput, CreateComp, CreateFolder, ResizeAnchor, SetCompSettings, SetWorkArea,
    TrimCompToWorkArea,
};
pub use content::{CopiedContent, DeleteContent, DuplicateContent, PasteContent, copy_content};
pub use dimensions::SeparateDimensions;
pub use expressions::{BakeExpression, ExpressionEdit, SetExpressions};
pub use import::{ImportFile, ImportItems, ImportOutput, ReplaceFile};
pub use keys::{
    CopiedKey, DeleteKeys, EaseKeys, EaseMode, EditKeys, KeyEdit, KeyRef, KeysOutput, MoveKeys,
    PasteKeys, ReverseKeys, SetAnimated, SetAt, SetInterpolation, SetMany, ValueEdit,
    apply_key_edits, set_default_spatial_linear,
};
pub use layer::{
    CopiedLayer, CreateLayer, DeleteItems, DeleteLayers, DuplicateLayers, LAYER_SWITCH_ATTRS,
    LayerOutput, LayersOutput, PasteLayers, ReorderLayers, SetLayerAttr, SetParent, ShapePreset,
    ShiftLayers, SlipLayers, SplitLayers, TrimLayers, copy_layers,
};
pub use markers::{AddMarker, DeleteMarkers, EditMarker, MarkerFields, MarkerOutput, MoveMarkers};
pub use matte::SetTrackMatte;
pub use orient::AutoOrient;
pub use path::{DeleteVertex, InsertVertex, SetVertices};
pub use precompose::{Precompose, PrecomposeMode, PrecomposeOutput};
pub use project_settings::SetProjectSettings;
pub use shape::{AddShape, CreateMask, Geometry, ShapeKind};
pub use solid::{SetSolidSettings, SolidInfo, SolidOutput};
pub use text::{CreateText, SetText};
pub use text_animators::{AddTextSelector, AnimateText, RemoveTextAnimators};
pub use text_path::SetTextPath;
pub use time::{
    FreezeFrame, FreezeOnLastFrame, SetTimeRemap, StretchHold, TimeReverse, TimeStretch,
};

/// Register every motion command.
pub fn register_commands(r: &mut CommandRegistry) {
    let results = [
        r.register(CreateComp),
        r.register(paint::AddStroke),
        r.register(tracker::CreateTracker),
        r.register(tracker::AddPoint),
        r.register(tracker::ConfigureTracker),
        r.register(tracker::RecordTrack),
        r.register(tracker::ResetTracker),
        r.register(SetCompSettings),
        r.register(SetWorkArea),
        r.register(TrimCompToWorkArea),
        r.register(CreateFolder),
        r.register(ImportItems),
        r.register(ReplaceFile),
        r.register(CreateLayer),
        r.register(Precompose),
        r.register(SetSolidSettings),
        r.register(SetCameraSettings),
        r.register(SetLightSettings),
        r.register(ReorderLayers),
        r.register(DuplicateLayers),
        r.register(PasteLayers),
        r.register(SplitLayers),
        r.register(TrimLayers),
        r.register(ShiftLayers),
        r.register(SlipLayers),
        r.register(SetTrackMatte),
        r.register(DeleteLayers),
        r.register(DeleteItems),
        r.register(SetParent),
        r.register(SetLayerAttr),
        r.register(SetAnimated),
        r.register(SetAt),
        r.register(SetMany),
        r.register(EaseKeys),
        r.register(SetInterpolation),
        r.register(MoveKeys),
        r.register(DeleteKeys),
        r.register(ReverseKeys),
        r.register(PasteKeys),
        r.register(EditKeys),
        r.register(SeparateDimensions),
        r.register(AddMarker),
        r.register(EditMarker),
        r.register(MoveMarkers),
        r.register(DeleteMarkers),
        r.register(AddShape),
        r.register(CreateMask),
        r.register(SetVertices),
        r.register(InsertVertex),
        r.register(DeleteVertex),
        r.register(CreateText),
        r.register(AutoOrient),
        r.register(SetProjectSettings),
        r.register(SetText),
        r.register(SetExpressions),
        r.register(DuplicateContent),
        r.register(PasteContent),
        r.register(DeleteContent),
        r.register(BakeExpression),
        r.register(AnimateText),
        r.register(AddTextSelector),
        r.register(RemoveTextAnimators),
        r.register(SetTextPath),
        r.register(SetTimeRemap),
        r.register(TimeStretch),
        r.register(TimeReverse),
        r.register(FreezeFrame),
        r.register(FreezeOnLastFrame),
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
        let defs = app.registry::<bae_model::EffectDef>();
        let registry = app.registry::<dyn DynCommand>();
        registry.batch(|tx| {
            for cmd in commands().into_iter().chain(effect::effect_commands(defs)) {
                let id = cmd.info().id.clone();
                if !tx.contains(&id) {
                    let _ = tx.register_arc(owner.clone(), id, cmd);
                }
            }
        });
    }
}
