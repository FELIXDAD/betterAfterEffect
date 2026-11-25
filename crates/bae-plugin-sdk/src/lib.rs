//! # bae-plugin-sdk — the BAE plugin contract
//!
//! This crate is shared by **plugin authors** (native plugins written in Rust)
//! and by the **host** (`bae-plugin`, the render graph in `bae-render`, the
//! built-in effects in `bae-effects`). It contains no host logic and has no
//! dependencies, so a plugin builds in seconds.
//!
//! Normative design: `docs/spec/plugin-architecture.md` (cited as "spec §N").
//! C ABI: `sdk/c/bae_plugin.h` (authoritative until it is generated from
//! [`abi`] with cbindgen).
//!
//! | module | contents | spec |
//! |---|---|---|
//! | [`version`] | plugin API version, host/plugin negotiation | §6.6, §14 |
//! | [`kind`] | contribution kinds and implementation forms | §3, §4 |
//! | [`abi`] | `#[repr(C)]` mirror of `bae_plugin.h` (ABI 1.0) | §14 |
//! | [`effect`] | writing native CPU effects in Rust: [`effect::CpuEffect`], [`export_plugin!`] | §8 |
#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod abi;
pub mod effect;
pub mod kind;
pub mod version;

pub use kind::{ContributionKind, Implementation, NativeKind};
pub use version::{ApiMismatch, ApiVersion, HostApiSupport};
