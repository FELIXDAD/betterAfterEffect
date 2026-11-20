//! # bae-aex-host — reserved After Effects plug-in compatibility layer
//!
//! **Status: P1 design skeleton.** This crate loads nothing, calls nothing and
//! links nothing from Adobe. It exists so that the *translation layer* between
//! third-party After Effects effect plug-ins (Windows `.aex` DLLs, macOS
//! `.plugin` bundles) and BAE's own interfaces is designed now, while the core
//! (02 doc model, 04 render graph, 19 plug-in system) is still malleable.
//! The design (Chinese), the capability mapping table and the requests sent to
//! other workstreams are in `docs/spec/aex-compat.md`.
//!
//! ## Layering
//!
//! ```text
//!   AE plug-in binary  (EffectMain entry point, PiPL resources)
//!        ▲   P3: FFI shim (raw C structs, per-platform loader) — NOT IN THIS CRATE YET
//!        │
//!   bae-aex-host  (this crate: BAE-owned, documented Rust mirror of the AE
//!        │         effect protocol: selectors, capabilities, params, worlds,
//!        │         SmartFX checkouts, sequence data, GPU, UI events, suites)
//!        ▼
//!   BAE interfaces: bae-model (02) EffectDef / param:NNNN attrs / `state` bytes,
//!                   bae-plugin (19) BaeEffectVTable1 callbacks, suites, runner,
//!                   bae-render (04) `bae_render::Effect` node + CPU nodes + ROI
//! ```
//!
//! ## Design rules (enforced by review, see spec §1)
//!
//! 1. **Pay only where used.** Nothing in here is reachable from the render
//!    hot path of a project that has no AE plug-in nodes. The compat host is
//!    an optional module that is only loaded when a project references an AE
//!    effect match name that no native implementation or BAE plug-in claims.
//! 2. **BAE-owned vocabulary.** Every type here is named and shaped by BAE.
//!    AE identifiers appear only in documentation (as cross references to the
//!    public SDK guide). No struct here is layout-compatible with an AE struct
//!    and none is meant to be; the P3 FFI shim converts.
//! 3. **No ABI constants.** Selector numbers, flag bit positions, error codes
//!    and suite name strings are intentionally absent. See the legal checklist
//!    in the spec before adding any.
//! 4. **Pure and testable.** Conversions (time, fixed point, pixels, flags →
//!    render requirements) are pure functions with unit tests.
//!
//! ## Module map
//!
//! | module | AE concept | BAE counterpart |
//! |---|---|---|
//! | [`pipl`] | PiPL resources, match name, versions | plug-in manifest (19), effect type id (02) |
//! | [`selector`] | `PF_Cmd_*` selectors, call order | `BaeEffectVTable1` callbacks (19) |
//! | [`caps`] | `PF_OutFlag_*` / `PF_OutFlag2_*` | render-node requirements (04) |
//! | [`time`] | `current_time`/`time_step`/`time_scale` | flicks (`docs/ARCHITECTURE.md` §4.4) |
//! | [`geometry`] | `PF_LRect`, `PF_RationalScale`, `PF_Fixed` | render request rects, downsample |
//! | [`params`] | `PF_ParamDef`, `PF_Param_*`, param flags | effect param model (02) |
//! | [`model`] | (conversion) | `bae_model::EffectDef` + the gaps it cannot express yet |
//! | [`pixel`] | `PF_EffectWorld`, 8/16/32 bpc ARGB | CPU-node buffer contract (04) |
//! | [`smart`] | SmartFX pre-render / render, checkouts | ROI negotiation, dynamic deps (04) |
//! | [`sequence`] | global / sequence / frame data | opaque per-instance state slot (02) |
//! | [`gpu`] | GPU frameworks, BGRA128 GPU worlds | native device interop (01/04/19) |
//! | [`ui`] | `PF_Cmd_EVENT`, custom UI, Drawbot | custom-draw region + overlay (19, 11/12) |
//! | [`suites`] | PICA suites, AEGP suites | versioned C ABI suites (19) |
//! | [`process`] | (host choice) | in-process vs out-of-process hosting (19) |
//! | [`resolve`] | effect match-name resolution on import | effect registry (19), placeholders (02) |
//! | [`error`] | `PF_Err_*` | host error model |
//!
//! [`shaders::CONVERT_WGSL`] is a WGSL compute shader that converts between
//! BAE's RGBA16F working textures and packed AE world layouts on the GPU, so
//! the CPU never touches pixels it does not have to (spec §6).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod caps;
pub mod error;
pub mod geometry;
pub mod gpu;
pub mod model;
pub mod params;
pub mod pipl;
pub mod pixel;
pub mod process;
pub mod resolve;
pub mod selector;
pub mod sequence;
pub mod shaders;
pub mod smart;
pub mod suites;
pub mod time;
pub mod ui;

/// Version of the translation-layer design this crate implements. Bumped
/// whenever a reservation request to 02/04/19 changes shape.
pub const DESIGN_REVISION: u32 = 1;
