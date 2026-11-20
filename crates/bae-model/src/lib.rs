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

mod attr;
mod command;
pub mod commands;
pub mod compose;
mod error;
mod event;
mod ids;
mod op;
mod path;
mod prim;
mod stage;
mod sublayer;
mod time;
mod value;

pub use attr::{AttrSpec, Ease, Expression, Interpolation, Keyframe, KeyframeFlags};
pub use command::{Command, CommandError, CommandInfo, CommandOutput, CommandRegistry, DynCommand};
pub use compose::{Opinion, PrimView};
pub use error::{ModelError, Result, validate_attr_name};
pub use event::{Change, EventOrigin, StageEvent};
pub use ids::{AuthorId, InvalidId, SublayerId};
pub use op::{Edit, Op};
pub use path::{PathError, PrimPath, validate_name};
pub use prim::{LayerKind, PrimSpec, PrimTree, PrimType, Reference, Specifier};
pub use stage::{Stage, SubscriptionId, Transaction};
pub use sublayer::Sublayer;
pub use time::{FrameRate, Time, TimeRange};
pub use value::{BezierPath, Value, ValueType};

/// Re-exported for `bae-sync` (version vectors, export modes).
pub use loro;
