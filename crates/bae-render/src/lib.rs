//! BAE GPU compositor (workstream 04): render graph, layer compositing, blend
//! modes, track mattes, caches and headless rendering.
//!
//! ```text
//! Stage ──► CompEvaluator (bae-anim; MockEvaluator until 03 lands) ──► RenderComp
//!       ──► graph::build (nodes, ROI negotiation, culling, cache keys) ──► RenderGraph
//!       ──► Renderer (shared GpuContext, RGBA16F premultiplied buffers) ──► texture
//!       ──► viewport (zero copy) / export (async readback ──► 08's encoders)
//! ```
//!
//! Public interface for other workstreams:
//!
//! * [`RenderComp`] / [`RenderLayer`] – the renderer's input (plain data, no GPU
//!   types); 03 produces it or an adapter converts 03's `EvaluatedComp`.
//! * [`CompEvaluator`] – evaluation entry point: [`AnimEvaluator`] adapts 03's
//!   `bae-anim`; [`MockEvaluator`] (static values, linear keys) is kept for tests.
//! * [`BlendMode`], [`TrackMatteMode`], [`MaskMode`] – AE modes and project tokens.
//! * [`RenderRequest`] – resolution, ROI, bit depth, working format, colour context.
//! * [`LayerSource`] / [`SourceProvider`] – how footage (08), shapes and text (05)
//!   and plugin generators deliver layer pixels.
//! * [`Effect`] / [`EffectLibrary`] – GPU effects (07, plugins from 19); parameter
//!   definitions are `bae_model::EffectDef`.
//! * [`graph`] – the node model and ROI negotiation.
//! * [`FrameCache`] – RAM-preview cache with precise `StageEvent` invalidation.
//! * [`camera`], [`three_d`], [`light`], [`motion`] – 3D layers seen through AE's
//!   camera model (projection, 3D groups, depth order), lights (data), motion
//!   blur samples.
//!
//! Pixel conventions: working buffers are premultiplied RGBA16F (RGBA32F
//! optional) holding the working space's encoded values; AE's straight-alpha
//! formulas (`docs/spec/ae/compositing-3d-color.md`) are evaluated in shaders on
//! unpremultiplied values where needed. Comp space has y down, pixel `(x, y)`
//! covers `[x, x + 1) × [y, y + 1)`.

pub mod anim;
mod assets;
pub mod blend;
pub mod cache;
pub mod camera;
pub mod effect;
mod error;
pub mod eval;
mod exec;
pub mod export;
pub mod fixture;
pub mod footage;
mod frame_blend;
pub mod geom;
mod gpu;
pub mod graph;
pub mod input;
pub mod light;
pub mod mask;
mod module;
pub mod motion;
pub mod png_out;
pub mod pool;
mod renderer;
pub mod request;
pub mod source;
pub mod testing;
pub mod three_d;

pub use anim::AnimEvaluator;
pub use assets::AssetFiles;
pub use blend::{BlendMode, MaskMode, TrackMatteMode};
pub use cache::{CacheStats, FrameCache, FrameKey};
pub use camera::Camera;
pub use effect::{Effect, EffectLibrary, EffectParams};
pub use error::RenderError;
pub use eval::{CompEvaluator, MockEvaluator};
pub use export::{
    ExportAlpha, ExportFormat, ExportJob, ExportReadback, ExportReport, ExportSettings,
    ExportStats, ExportedFrame, READBACK_DEPTH, ReadFrame, ReadbackFormat, export_frames,
    frame_file, has_frame_run,
};
pub use geom::{IRect, Rect};
pub use gpu::CompositorWarmUp;
pub use graph::EffectFrame;
pub use graph::{RenderGraph, SourceResolver};
pub use input::{
    CustomContent, Dependency, FootageRef, FrameBlend, LayerContent, RenderComp, RenderEffect,
    RenderLayer, RenderMask, TrackMatte,
};
pub use light::{Light, LightKind, Material};
pub use module::RenderModule;
pub use motion::{CompMotionBlur, LayerSample};
pub use pool::PoolStats;
pub use renderer::{FrameStats, OutputAlpha, RenderedFrame, Renderer, RendererConfig};
pub use request::{
    BitDepth, BlendSpace, ColorContext, RenderPurpose, RenderRequest, Resolution, WorkFormat,
};
pub use source::{
    EncodeContext, GpuImage, ImageAlpha, LayerSource, SourceProvider, SourceProviders,
    SourceRequest,
};

thread_local! {
    static INTERACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks the calling thread as interactive (the UI thread) or not. On an
/// interactive thread, first-use work that is not ready yet (the warm-up,
/// `bae_gpu::warm_up`, is still preparing it) is not waited for: the frame is
/// drawn without it and marked incomplete ([`RenderedFrame::incomplete`]),
/// and the app redraws when it is ready.
pub fn set_interactive_thread(on: bool) {
    INTERACTIVE.with(|c| c.set(on));
}

/// Whether the calling thread is interactive ([`set_interactive_thread`]).
pub fn is_interactive_thread() -> bool {
    INTERACTIVE.with(|c| c.get())
}

/// The error for `what` still being prepared by the warm-up.
pub fn warming_up(what: &str) -> RenderError {
    RenderError::WarmingUp(what.to_owned())
}
