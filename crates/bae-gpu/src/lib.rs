//! BAE GPU layer.
//!
//! One [`GpuContext`] (instance, adapter, device, queue) is created per process and
//! shared — as `Arc<GpuContext>` — by the renderer, the UI and plugins. Frames
//! therefore stay on the GPU end to end: the compositor renders into a texture,
//! the UI samples that same texture ([`viewport::ViewportTextureRegistry`]), and a
//! native plugin can get the underlying Metal / D3D12 objects ([`native`]).
//!
//! * [`GpuContext::new_headless`] / [`GpuContext::new_with_surface`] — creation with
//!   a high-performance adapter preference, software fallback and optional
//!   features ([`caps::optional_features`]); device-lost callbacks and
//!   [`GpuContext::recreate`] for recovery.
//! * [`caps`] — capability flags and the `bae gpu-info` report.
//! * [`surface::WindowSurface`] — swapchain configuration, resize, present mode,
//!   frame latency, lost/outdated handling, HDR/wide-gamut format discovery.
//! * [`timer::GpuTimer`] — GPU timestamps (when supported).
//! * [`transfer`] — texture → CPU readback (row padding handled) and CPU → texture
//!   upload (plugin CPU nodes, thumbnails, Agent screenshots).
//! * [`native`] — raw Metal / D3D12 handles for plugin interop.
//! * [`GpuModule`] — the app module (resources + `bae gpu-info`).
//! * [`testing::shared_context`] — one context per test process for GPU tests.
//!
//! `wgpu` is re-exported so dependants use exactly the pinned version.

pub mod caps;
mod context;
mod module;
pub mod native;
pub mod surface;
pub mod testing;
pub mod timer;
pub mod transfer;
pub mod viewport;
pub mod warm_up;

pub use caps::{GpuCapabilities, GpuReport, optional_features};
pub use context::{
    ADAPTER_ENV, AdapterSelection, BACKEND_ENV, DX12_COMPILER_ENV, DeviceLostInfo,
    DeviceLostSubscription, GpuContext, GpuContextDescriptor, GpuError, LimitsPreference,
    POWER_ENV, VALIDATION_ENV, backend_name, device_type_name, dx12_compiler, parse_backends,
    primary_backends,
};
pub use module::GpuModule;
pub use native::{NativeApi, NativeDevice, NativeTexture};
pub use surface::{
    PresentModePreference, SurfaceColorInfo, SurfaceColorPreference, SurfaceFrame, SurfaceOptions,
    WindowSurface,
};
pub use viewport::{ViewportTextureId, ViewportTextureRegistry};
pub use warm_up::WarmUp;
pub use wgpu;
