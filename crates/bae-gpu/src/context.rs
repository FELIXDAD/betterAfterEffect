//! [`GpuContext`]: the one wgpu instance / adapter / device / queue per process.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;
use serde::Serialize;

use crate::caps::{GpuCapabilities, optional_features};

/// Environment variable selecting the backend: `metal`, `dx12`, `vulkan`, `gl` or `all`.
pub const BACKEND_ENV: &str = "BAE_GPU_BACKEND";
/// Environment variable selecting the adapter: a case-insensitive substring of the
/// adapter name, or `software` for a software rasterizer (WARP, llvmpipe …).
pub const ADAPTER_ENV: &str = "BAE_GPU_ADAPTER";
/// Environment variable selecting the power preference: `high` (default) or `low`.
pub const POWER_ENV: &str = "BAE_GPU_POWER";
/// Environment variable enabling (`1`) or disabling (`0`) backend API validation
/// (Metal validation layer / D3D12 debug layer). Defaults to on in debug builds.
pub const VALIDATION_ENV: &str = "BAE_GPU_VALIDATION";

/// How the adapter (physical GPU) is chosen.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum AdapterSelection {
    /// Prefer a discrete GPU, then integrated, then virtual, then software.
    #[default]
    HighPerformance,
    /// Prefer an integrated GPU (battery), then discrete, then virtual, then software.
    LowPower,
    /// The first adapter whose name contains this case-insensitive substring;
    /// falls back to [`Self::HighPerformance`] ordering if none matches.
    Named(String),
    /// Only software rasterizers (WARP, llvmpipe, SwiftShader). For CI.
    Software,
}

/// Which device limits to request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LimitsPreference {
    /// Everything the adapter supports (largest textures and buffers). The right
    /// choice for a desktop compositor: 8K/16K comps and big Vello buffers.
    #[default]
    Adapter,
    /// wgpu's default (WebGPU baseline) limits.
    Default,
    /// Downlevel limits for very old or software adapters.
    Downlevel,
}

/// Options for creating a [`GpuContext`].
#[derive(Debug, Clone)]
pub struct GpuContextDescriptor {
    /// Debug label of the device.
    pub label: &'static str,
    /// Backends to consider. Default: Metal on Apple platforms, D3D12 on Windows.
    pub backends: wgpu::Backends,
    /// Adapter choice.
    pub adapter: AdapterSelection,
    /// Fall back to a software adapter when no hardware adapter is usable.
    pub allow_software_fallback: bool,
    /// Features the device must have; creation fails without them.
    pub required_features: wgpu::Features,
    /// Features enabled when the adapter supports them (never required).
    pub optional_features: wgpu::Features,
    /// Limits to request.
    pub limits: LimitsPreference,
    /// Memory allocation strategy.
    pub memory_hints: wgpu::MemoryHints,
    /// Enable backend API validation (Metal validation / D3D12 debug layer).
    /// wgpu's own WebGPU validation is always on.
    pub validation: bool,
    /// Panic on uncaptured wgpu errors instead of logging them. Defaults to on in
    /// debug builds so bugs surface in tests; release builds log and continue.
    pub panic_on_uncaptured_error: bool,
}

impl Default for GpuContextDescriptor {
    fn default() -> Self {
        Self {
            label: "bae",
            backends: primary_backends(),
            adapter: AdapterSelection::default(),
            allow_software_fallback: true,
            required_features: wgpu::Features::empty(),
            optional_features: optional_features(),
            limits: LimitsPreference::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            validation: cfg!(debug_assertions),
            panic_on_uncaptured_error: cfg!(debug_assertions),
        }
    }
}

impl GpuContextDescriptor {
    /// Applies the `BAE_GPU_*` environment variables (see the `*_ENV` constants).
    /// Unknown values are logged and ignored.
    #[must_use]
    pub fn with_env(mut self) -> Self {
        if let Some(value) = env_value(BACKEND_ENV) {
            match parse_backends(&value) {
                Some(backends) => self.backends = backends,
                None => tracing::warn!("{BACKEND_ENV}={value:?} is not a known backend"),
            }
        }
        if let Some(value) = env_value(ADAPTER_ENV) {
            self.adapter = if value.eq_ignore_ascii_case("software") {
                AdapterSelection::Software
            } else {
                AdapterSelection::Named(value)
            };
        }
        if let Some(value) = env_value(POWER_ENV) {
            match value.to_ascii_lowercase().as_str() {
                "high" | "high-performance" | "discrete" => {
                    if !matches!(self.adapter, AdapterSelection::Named(_)) {
                        self.adapter = AdapterSelection::HighPerformance;
                    }
                }
                "low" | "low-power" | "integrated" => {
                    if !matches!(self.adapter, AdapterSelection::Named(_)) {
                        self.adapter = AdapterSelection::LowPower;
                    }
                }
                _ => tracing::warn!("{POWER_ENV}={value:?} is not `high` or `low`"),
            }
        }
        if let Some(value) = env_value(VALIDATION_ENV) {
            self.validation = matches!(value.as_str(), "1" | "true" | "on" | "yes");
        }
        self
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Parses a backend list such as `metal`, `dx12,vulkan` or `all`.
pub fn parse_backends(value: &str) -> Option<wgpu::Backends> {
    let mut backends = wgpu::Backends::empty();
    for part in value.split([',', '|', ' ']).filter(|p| !p.is_empty()) {
        backends |= match part.to_ascii_lowercase().as_str() {
            "metal" | "mtl" => wgpu::Backends::METAL,
            "dx12" | "d3d12" => wgpu::Backends::DX12,
            "vulkan" | "vk" => wgpu::Backends::VULKAN,
            "gl" | "gles" | "opengl" => wgpu::Backends::GL,
            "primary" => wgpu::Backends::PRIMARY,
            "all" => wgpu::Backends::all(),
            _ => return None,
        };
    }
    (!backends.is_empty()).then_some(backends)
}

/// The backends BAE uses by default on this platform.
pub fn primary_backends() -> wgpu::Backends {
    if cfg!(target_vendor = "apple") {
        wgpu::Backends::METAL
    } else if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::VULKAN | wgpu::Backends::GL
    }
}

/// Why the device was lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceLostInfo {
    /// `true` if the device was destroyed on purpose (`Device::destroy`, shutdown).
    pub destroyed: bool,
    /// Driver / wgpu message.
    pub message: String,
}

impl fmt::Display for DeviceLostInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.destroyed {
            write!(f, "device destroyed: {}", self.message)
        } else {
            write!(f, "device lost: {}", self.message)
        }
    }
}

/// Errors from GPU setup and surface management.
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    /// No adapter matched the descriptor.
    #[error("no usable GPU adapter found (backends: {backends}; seen: {seen})")]
    NoAdapter {
        /// Backends that were searched.
        backends: String,
        /// Adapters that exist but were rejected, for diagnostics.
        seen: String,
    },
    /// The adapter lacks required features.
    #[error("adapter `{adapter}` lacks required features: {missing}")]
    MissingFeatures {
        /// Adapter name.
        adapter: String,
        /// Missing features.
        missing: wgpu::Features,
    },
    /// Device creation failed.
    #[error("failed to create the GPU device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    /// Surface creation failed.
    #[error("failed to create a window surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),
    /// The surface cannot be presented by the selected adapter.
    #[error("the window surface is not supported by adapter `{0}`")]
    SurfaceUnsupported(String),
    /// The surface was lost and must be recreated from the window.
    #[error("the window surface was lost")]
    SurfaceLost,
    /// A surface / texture configuration is not supported.
    #[error("unsupported configuration: {0}")]
    Unsupported(String),
    /// The device was lost; recreate the context.
    #[error("{0}")]
    DeviceLost(DeviceLostInfo),
    /// Mapping a buffer for readback failed.
    #[error("buffer mapping failed: {0}")]
    Map(String),
    /// Waiting for the GPU failed.
    #[error("waiting for the GPU failed: {0}")]
    Poll(#[from] wgpu::PollError),
}

type LostCallback = Arc<dyn Fn(&DeviceLostInfo) + Send + Sync>;

/// Shared device-lost state, written by wgpu's callback.
#[derive(Default)]
struct LostState {
    lost: AtomicBool,
    info: Mutex<Option<DeviceLostInfo>>,
    next_id: AtomicU64,
    callbacks: Mutex<Vec<(u64, LostCallback)>>,
}

impl LostState {
    fn notify(&self, info: DeviceLostInfo) {
        if info.destroyed {
            tracing::debug!("{info}");
        } else {
            tracing::error!("{info}");
        }
        *self.info.lock() = Some(info.clone());
        self.lost.store(true, Ordering::Release);
        // Clone the list so callbacks may (un)subscribe without deadlocking.
        let callbacks: Vec<LostCallback> = self
            .callbacks
            .lock()
            .iter()
            .map(|(_, cb)| cb.clone())
            .collect();
        for callback in callbacks {
            callback(&info);
        }
    }
}

/// Handle returned by [`GpuContext::on_device_lost`]; pass it to
/// [`GpuContext::remove_device_lost_callback`] to unsubscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceLostSubscription(u64);

/// The process-wide GPU context: instance, adapter, device and queue, plus what
/// the device can do. Create it once and share it as `Arc<GpuContext>` between
/// the renderer, the UI and plugins so textures never leave the GPU.
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    features: wgpu::Features,
    limits: wgpu::Limits,
    downlevel: wgpu::DownlevelCapabilities,
    capabilities: GpuCapabilities,
    descriptor: GpuContextDescriptor,
    adapters: Vec<wgpu::AdapterInfo>,
    lost: Arc<LostState>,
    uncaptured_errors: Arc<AtomicU64>,
}

impl fmt::Debug for GpuContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuContext")
            .field("adapter", &self.info.name)
            .field("backend", &self.info.backend)
            .field("device_type", &self.info.device_type)
            .field("lost", &self.is_lost())
            .finish_non_exhaustive()
    }
}

/// Contexts (devices) alive in this process; see [`GpuContext::live_count`].
static LIVE_CONTEXTS: AtomicUsize = AtomicUsize::new(0);

impl Drop for GpuContext {
    fn drop(&mut self) {
        LIVE_CONTEXTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl GpuContext {
    /// Creates a context without a window (render, export, tests, `--headless`).
    /// Blocks on wgpu's async adapter/device requests.
    pub fn new_headless(desc: GpuContextDescriptor) -> Result<Arc<Self>, GpuError> {
        pollster::block_on(Self::new_headless_async(desc))
    }

    /// Async variant of [`Self::new_headless`].
    pub async fn new_headless_async(desc: GpuContextDescriptor) -> Result<Arc<Self>, GpuError> {
        let instance = create_instance(&desc);
        Self::from_instance(instance, desc, None, false).await
    }

    /// Creates a context whose adapter can present to `target` (normally an
    /// `Arc<winit::window::Window>`), and the surface for it. Configure the surface
    /// with [`crate::WindowSurface::new`].
    pub fn new_with_surface(
        desc: GpuContextDescriptor,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
    ) -> Result<(Arc<Self>, wgpu::Surface<'static>), GpuError> {
        let instance = create_instance(&desc);
        let surface = instance.create_surface(target)?;
        let ctx = pollster::block_on(Self::from_instance(instance, desc, Some(&surface), false))?;
        Ok((ctx, surface))
    }

    /// Creates another surface (an extra window) on this context's instance.
    pub fn create_surface(
        &self,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
    ) -> Result<wgpu::Surface<'static>, GpuError> {
        let surface = self.instance.create_surface(target)?;
        if !self.adapter.is_surface_supported(&surface) {
            return Err(GpuError::SurfaceUnsupported(self.info.name.clone()));
        }
        Ok(surface)
    }

    /// Creates a fresh context with the same descriptor and instance — the
    /// recovery path after a device loss. Every resource (textures, pipelines,
    /// egui renderer) must be recreated on the new device, and window surfaces
    /// re-created with [`Self::create_surface`].
    pub fn recreate(&self) -> Result<Arc<Self>, GpuError> {
        pollster::block_on(Self::from_instance(
            self.instance.clone(),
            self.descriptor.clone(),
            None,
            true,
        ))
    }

    /// How many contexts (GPU devices) are alive in this process. The app needs one;
    /// a second one while another lives is logged as a warning (see
    /// [`crate::testing::shared_context`] for tests).
    pub fn live_count() -> usize {
        LIVE_CONTEXTS.load(Ordering::Relaxed)
    }

    /// `replacing`: [`Self::recreate`] after a device loss, where the lost context
    /// may still be alive while its replacement is created.
    async fn from_instance(
        instance: wgpu::Instance,
        desc: GpuContextDescriptor,
        surface: Option<&wgpu::Surface<'_>>,
        replacing: bool,
    ) -> Result<Arc<Self>, GpuError> {
        let all = instance.enumerate_adapters(desc.backends).await;
        let adapters: Vec<wgpu::AdapterInfo> = all.iter().map(wgpu::Adapter::get_info).collect();
        let mut candidates: Vec<wgpu::Adapter> = all
            .into_iter()
            .filter(|adapter| surface.is_none_or(|s| adapter.is_surface_supported(s)))
            .filter(|adapter| adapter.features().contains(desc.required_features))
            .collect();
        sort_adapters(&mut candidates, &desc.adapter);
        if !desc.allow_software_fallback && desc.adapter != AdapterSelection::Software {
            candidates.retain(|a| a.get_info().device_type != wgpu::DeviceType::Cpu);
        }
        let adapter = match candidates.into_iter().next() {
            Some(adapter) => adapter,
            None if desc.allow_software_fallback => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: true,
                    compatible_surface: surface,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|_| no_adapter(&desc, &adapters))?,
            None => return Err(no_adapter(&desc, &adapters)),
        };
        let info = adapter.get_info();
        let adapter_features = adapter.features();
        if !adapter_features.contains(desc.required_features) {
            return Err(GpuError::MissingFeatures {
                adapter: info.name,
                missing: desc.required_features - adapter_features,
            });
        }
        let features = desc.required_features | (desc.optional_features & adapter_features);
        let limits = match desc.limits {
            LimitsPreference::Adapter => adapter.limits(),
            LimitsPreference::Default => wgpu::Limits::default().using_resolution(adapter.limits()),
            LimitsPreference::Downlevel => {
                wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())
            }
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some(desc.label),
                required_features: features,
                required_limits: limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: desc.memory_hints.clone(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let lost = Arc::new(LostState::default());
        {
            let lost = Arc::downgrade(&lost);
            device.set_device_lost_callback(move |reason, message| {
                if let Some(lost) = lost.upgrade() {
                    lost.notify(DeviceLostInfo {
                        destroyed: reason == wgpu::DeviceLostReason::Destroyed,
                        message,
                    });
                }
            });
        }
        let uncaptured_errors = Arc::new(AtomicU64::new(0));
        {
            let counter = uncaptured_errors.clone();
            let panic = desc.panic_on_uncaptured_error;
            device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
                counter.fetch_add(1, Ordering::Relaxed);
                if panic {
                    panic!("uncaptured wgpu error: {error}");
                }
                tracing::error!("uncaptured wgpu error: {error}");
            }));
        }

        let features = device.features();
        let limits = device.limits();
        let downlevel = adapter.get_downlevel_capabilities();
        let capabilities = GpuCapabilities::detect(&adapter, &info, features, &limits, &queue);
        tracing::info!(
            adapter = %info.name,
            backend = ?info.backend,
            device_type = ?info.device_type,
            driver = %info.driver,
            "GPU device created"
        );
        let others = LIVE_CONTEXTS.fetch_add(1, Ordering::Relaxed);
        if others > 0 && !replacing {
            // The app shares one context. Several devices at once in one process
            // crashed Windows CI under the D3D12 debug layer (#154); tests share
            // `testing::shared_context()` instead.
            tracing::warn!(
                others,
                "another GPU device is created while {others} already exist in this process; \
                 share one Arc<GpuContext> instead"
            );
        }
        Ok(Arc::new(Self {
            instance,
            adapter,
            device,
            queue,
            info,
            features,
            limits,
            downlevel,
            capabilities,
            descriptor: desc,
            adapters,
            lost,
            uncaptured_errors,
        }))
    }

    /// The wgpu instance (create more surfaces with [`Self::create_surface`]).
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// The selected adapter.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// The shared device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The shared queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Information about the selected adapter.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }

    /// Every adapter that was enumerated on the configured backends.
    pub fn available_adapters(&self) -> &[wgpu::AdapterInfo] {
        &self.adapters
    }

    /// Features enabled on the device.
    pub fn features(&self) -> wgpu::Features {
        self.features
    }

    /// Limits of the device.
    pub fn limits(&self) -> &wgpu::Limits {
        &self.limits
    }

    /// Downlevel capabilities of the adapter.
    pub fn downlevel(&self) -> &wgpu::DownlevelCapabilities {
        &self.downlevel
    }

    /// Capability flags derived from features, limits and format support.
    pub fn capabilities(&self) -> &GpuCapabilities {
        &self.capabilities
    }

    /// The descriptor the context was created with.
    pub fn descriptor(&self) -> &GpuContextDescriptor {
        &self.descriptor
    }

    /// `true` once the device was lost or destroyed.
    pub fn is_lost(&self) -> bool {
        self.lost.lost.load(Ordering::Acquire)
    }

    /// Why the device was lost, if it was.
    pub fn lost_info(&self) -> Option<DeviceLostInfo> {
        self.lost.info.lock().clone()
    }

    /// Number of uncaptured wgpu errors so far (logged unless
    /// [`GpuContextDescriptor::panic_on_uncaptured_error`]).
    pub fn uncaptured_error_count(&self) -> u64 {
        self.uncaptured_errors.load(Ordering::Relaxed)
    }

    /// Calls `callback` when the device is lost. Runs on whatever thread wgpu
    /// detects the loss on: keep it short (set a flag, wake the event loop) and
    /// recreate resources from the owning thread via [`Self::recreate`]. If the
    /// device is already lost, `callback` runs immediately.
    pub fn on_device_lost(
        &self,
        callback: impl Fn(&DeviceLostInfo) + Send + Sync + 'static,
    ) -> DeviceLostSubscription {
        let id = self.lost.next_id.fetch_add(1, Ordering::Relaxed);
        let callback: LostCallback = Arc::new(callback);
        self.lost.callbacks.lock().push((id, callback.clone()));
        if let Some(info) = self.lost_info() {
            callback(&info);
        }
        DeviceLostSubscription(id)
    }

    /// Removes a callback added with [`Self::on_device_lost`].
    pub fn remove_device_lost_callback(&self, subscription: DeviceLostSubscription) -> bool {
        let mut callbacks = self.lost.callbacks.lock();
        let before = callbacks.len();
        callbacks.retain(|(id, _)| *id != subscription.0);
        callbacks.len() != before
    }

    /// Blocks until all submitted work has finished.
    pub fn wait_idle(&self) -> Result<(), GpuError> {
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    /// Processes finished work (map callbacks, resource cleanup) without blocking.
    /// The event loop calls this once per frame.
    pub fn poll(&self) {
        // A non-blocking poll only fails if the device is lost, which the
        // device-lost callback reports.
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    /// One-line description for status bars and logs, e.g.
    /// `Apple M2 Pro (Metal, integrated)`.
    pub fn summary(&self) -> String {
        format!(
            "{} ({}, {})",
            self.info.name,
            backend_name(self.info.backend),
            device_type_name(self.info.device_type)
        )
    }
}

fn create_instance(desc: &GpuContextDescriptor) -> wgpu::Instance {
    let mut flags = wgpu::InstanceFlags::from_build_config().with_env();
    flags.set(wgpu::InstanceFlags::VALIDATION, desc.validation);
    if !desc.validation {
        flags.remove(wgpu::InstanceFlags::GPU_BASED_VALIDATION);
    }
    let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_desc.backends = desc.backends;
    instance_desc.flags = flags;
    instance_desc.backend_options = wgpu::BackendOptions::from_env_or_default();
    instance_desc.backend_options.dx12.shader_compiler =
        dx12_compiler(std::env::var(DX12_COMPILER_ENV).ok().as_deref());
    if !flags
        .intersects(wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::GPU_BASED_VALIDATION)
    {
        return wgpu::Instance::new(instance_desc);
    }
    // With validation, every D3D12 instance calls `ID3D12Debug::EnableDebugLayer`,
    // which the runtime does not allow once a device exists (it may remove existing
    // devices). So a process creates each validating instance once and reuses it
    // for later contexts (#154). Release builds don't validate by default.
    static VALIDATING: Mutex<Vec<(String, wgpu::Instance)>> = Mutex::new(Vec::new());
    let key = format!(
        "{:?} {:?} {:?}",
        desc.backends, flags, instance_desc.backend_options.dx12.shader_compiler
    );
    let mut instances = VALIDATING.lock();
    if let Some((_, instance)) = instances.iter().find(|(k, _)| *k == key) {
        return instance.clone();
    }
    let instance = wgpu::Instance::new(instance_desc);
    instances.push((key, instance.clone()));
    instance
}

/// wgpu's environment variable choosing the DX12 shader compiler (`fxc`, `dxc`,
/// `staticdxc`, `auto`); honoured for testing and diagnosis.
pub const DX12_COMPILER_ENV: &str = "WGPU_DX12_COMPILER";

/// The DX12 shader compiler for `env` (the value of [`DX12_COMPILER_ENV`]):
/// FXC (`d3dcompiler_47.dll`, part of Windows) unless the variable names a
/// valid compiler.
///
/// wgpu's own default (`Auto`) loads whatever `dxcompiler.dll` comes first on
/// `PATH`. Other applications put old copies there, and those crash the process
/// inside DXC (0xc000001d) while compiling naga's HLSL, so which compiler runs
/// would depend on what else is installed. Shipping a known DXC would add
/// several MiB to the installer; revisit when shader performance needs it
/// (ARCHITECTURE §12).
pub fn dx12_compiler(env: Option<&str>) -> wgpu::Dx12Compiler {
    env.and_then(|v| v.trim().parse().ok())
        .unwrap_or(wgpu::Dx12Compiler::Fxc)
}

fn no_adapter(desc: &GpuContextDescriptor, adapters: &[wgpu::AdapterInfo]) -> GpuError {
    let seen = if adapters.is_empty() {
        "none".to_owned()
    } else {
        adapters
            .iter()
            .map(|a| format!("{} ({:?}, {:?})", a.name, a.backend, a.device_type))
            .collect::<Vec<_>>()
            .join("; ")
    };
    GpuError::NoAdapter {
        backends: format!("{:?}", desc.backends),
        seen,
    }
}

/// Orders adapters best-first for `selection`.
fn sort_adapters(adapters: &mut [wgpu::Adapter], selection: &AdapterSelection) {
    adapters.sort_by_cached_key(|adapter| {
        let info = adapter.get_info();
        adapter_rank(&info, selection)
    });
}

/// Lower is better.
pub(crate) fn adapter_rank(info: &wgpu::AdapterInfo, selection: &AdapterSelection) -> (u8, u8) {
    use wgpu::DeviceType as T;
    let performance = match info.device_type {
        T::DiscreteGpu => 0,
        T::IntegratedGpu => 1,
        T::VirtualGpu => 2,
        T::Other => 3,
        T::Cpu => 4,
    };
    match selection {
        AdapterSelection::HighPerformance => (0, performance),
        AdapterSelection::LowPower => {
            let low = match info.device_type {
                T::IntegratedGpu => 0,
                T::DiscreteGpu => 1,
                T::VirtualGpu => 2,
                T::Other => 3,
                T::Cpu => 4,
            };
            (0, low)
        }
        AdapterSelection::Named(name) => {
            let matches = info.name.to_lowercase().contains(&name.to_lowercase());
            (u8::from(!matches), performance)
        }
        AdapterSelection::Software => (u8::from(info.device_type != T::Cpu), performance),
    }
}

/// Human-readable backend name.
pub fn backend_name(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Dx12 => "D3D12",
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::BrowserWebGpu => "WebGPU",
        wgpu::Backend::Noop => "no-op",
    }
}

/// Human-readable device type.
pub fn device_type_name(device_type: wgpu::DeviceType) -> &'static str {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => "discrete",
        wgpu::DeviceType::IntegratedGpu => "integrated",
        wgpu::DeviceType::VirtualGpu => "virtual",
        wgpu::DeviceType::Cpu => "software",
        wgpu::DeviceType::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, device_type: wgpu::DeviceType) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: name.to_owned(),
            vendor: 0,
            device: 0,
            device_type,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend: wgpu::Backend::Metal,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: None,
            limit_bucket: None,
        }
    }

    #[test]
    fn ranking_prefers_requested_class() {
        let discrete = info("RTX 3060", wgpu::DeviceType::DiscreteGpu);
        let integrated = info("Intel UHD", wgpu::DeviceType::IntegratedGpu);
        let cpu = info("WARP", wgpu::DeviceType::Cpu);
        let high = AdapterSelection::HighPerformance;
        assert!(adapter_rank(&discrete, &high) < adapter_rank(&integrated, &high));
        assert!(adapter_rank(&integrated, &high) < adapter_rank(&cpu, &high));
        let low = AdapterSelection::LowPower;
        assert!(adapter_rank(&integrated, &low) < adapter_rank(&discrete, &low));
        let named = AdapterSelection::Named("uhd".into());
        assert!(adapter_rank(&integrated, &named) < adapter_rank(&discrete, &named));
        let software = AdapterSelection::Software;
        assert!(adapter_rank(&cpu, &software) < adapter_rank(&discrete, &software));
    }

    #[test]
    fn dx12_shader_compiler_defaults_to_fxc() {
        assert!(matches!(dx12_compiler(None), wgpu::Dx12Compiler::Fxc));
        assert!(matches!(
            dx12_compiler(Some("bogus")),
            wgpu::Dx12Compiler::Fxc
        ));
        // The environment variable still overrides it.
        assert!(matches!(
            dx12_compiler(Some("DXC")),
            wgpu::Dx12Compiler::DynamicDxc { .. }
        ));
        assert!(matches!(
            dx12_compiler(Some("auto")),
            wgpu::Dx12Compiler::Auto
        ));
    }

    #[test]
    fn backend_parsing() {
        assert_eq!(parse_backends("metal"), Some(wgpu::Backends::METAL));
        assert_eq!(
            parse_backends("DX12, vulkan"),
            Some(wgpu::Backends::DX12 | wgpu::Backends::VULKAN)
        );
        assert_eq!(parse_backends("all"), Some(wgpu::Backends::all()));
        assert_eq!(parse_backends("glide"), None);
        assert_eq!(parse_backends(""), None);
    }
}
