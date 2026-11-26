//! BAE project data model (ARCHITECTURE §4).
//!
//! - [`Stage`]: an open project — a stack of [`Sublayer`]s composed strongest-first.
//! - [`Sublayer`]: one set of opinions ("工程层"), stored in a Loro CRDT document.
//! - [`PrimPath`], [`PrimType`], [`PrimSpec`], [`AttrSpec`], [`Keyframe`], [`Value`],
//!   [`Time`] (integer flicks).
//! - Every edit is an [`Op`]; [`Command`]s run in a [`Transaction`] and are registered
//!   in the [`CommandRegistry`]. Undo/redo is per author. Every commit emits a
//!   [`StageEvent`].
//!
//! This crate has no GPU, window or platform dependencies.

pub mod asset;
mod attr;
mod command;
pub mod commands;
pub mod compose;
pub mod effect;
mod error;
mod event;
mod ids;
pub mod layer_styles;
mod op;
mod path;
mod prim;
pub mod project;
pub mod schema;
mod stage;
mod sublayer;
mod text;
mod time;
mod value;

pub use asset::{AssetFile, AssetResolver, PathMapping, Resolution};
pub use attr::{AttrSpec, Ease, Expression, Interpolation, Keyframe, KeyframeFlags};
pub use command::{Command, CommandError, CommandInfo, CommandOutput, CommandRegistry, DynCommand};
pub use compose::{Opinion, PrimView};
pub use effect::{
    EffectDef, EffectParamDef, EffectRegistry, EffectStatus, ParamFlags, ParamKind, PluginRef,
};
pub use error::{ModelError, Result, validate_attr_name};
pub use event::{Change, EventOrigin, StageEvent};
pub use ids::{AuthorId, InvalidId, SublayerId};
pub use op::{Edit, Op};
pub use path::{PathError, PrimPath, validate_name};
pub use prim::{LayerKind, PrimSpec, PrimTree, PrimType, Reference, Specifier};
pub use project::{
    BaezAsset, OpenOptions, Project, ProjectError, ProjectManifest, export_flattened_json,
    export_prim_json, export_sublayer_json, extract_baez,
};
pub use stage::{DEFAULT_UNDO_LIMIT, Stage, SubscriptionId, Transaction, editable_on_locked_layer};
pub use sublayer::Sublayer;
pub use text::{BaselineOption, ParagraphStyle, StyleRun, TextDocument, TextStyle};
pub use time::{FrameRate, Time, TimeRange};
pub use value::{BezierPath, FeatherPoint, Value, ValueType};

/// Re-exported for `bae-sync` (version vectors, export modes).
pub use loro;
