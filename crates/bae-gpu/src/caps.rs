//! Capability detection and the `bae gpu-info` report.

use std::fmt::{self, Write as _};

use serde::Serialize;

use crate::context::{GpuContext, backend_name, device_type_name};

/// Features BAE enables whenever the adapter supports them. None is required:
/// code must check [`GpuContext::features`] / [`GpuCapabilities`] before use.
///
/// | Feature | Used for |
/// |---|---|
/// | `TIMESTAMP_QUERY`, `…_INSIDE_ENCODERS`, `…_INSIDE_PASSES` | [`crate::timer::GpuTimer`], render-graph profiling, benchmarks |
/// | `SHADER_F16` | half-precision effect shaders (RGBA16F working buffers) |
/// | `FLOAT32_FILTERABLE`, `FLOAT32_BLENDABLE` | 32-bit float comps ("32 bpc" in AE) |
/// | `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` | storage access to RGBA16F etc. beyond WebGPU's minimum |
/// | `RG11B10UFLOAT_RENDERABLE` | compact HDR intermediates |
/// | `BGRA8UNORM_STORAGE` | compute writes straight into BGRA surfaces / exports |
/// | `TEXTURE_FORMAT_16BIT_NORM` | 16-bit PNG/TIFF and AE 16 bpc import/export |
/// | `TEXTURE_FORMAT_NV12`, `TEXTURE_FORMAT_P010` | zero-copy hardware video decode (workstream 08) |
/// | `DUAL_SOURCE_BLENDING` | blend modes in one pass |
/// | `CLEAR_TEXTURE` | cheap cache invalidation |
/// | `ADDRESS_MODE_CLAMP_TO_BORDER`, `…_TO_ZERO` | AE "transparent edges" sampling |
/// | `IMMEDIATES` | small per-draw parameters (push constants) |
/// | `SUBGROUP` | fast reductions (histograms, scopes, auto-levels) |
/// | `INDIRECT_FIRST_INSTANCE` | GPU-driven layer batching |
/// | `DEPTH32FLOAT_STENCIL8` | 3D layers with stencil masks |
/// | `PIPELINE_CACHE` | faster startup (Vulkan) |
pub fn optional_features() -> wgpu::Features {
    wgpu::Features::TIMESTAMP_QUERY
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
        | wgpu::Features::SHADER_F16
        | wgpu::Features::FLOAT32_FILTERABLE
        | wgpu::Features::FLOAT32_BLENDABLE
        | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        | wgpu::Features::RG11B10UFLOAT_RENDERABLE
        | wgpu::Features::BGRA8UNORM_STORAGE
        | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
        | wgpu::Features::TEXTURE_FORMAT_NV12
        | wgpu::Features::TEXTURE_FORMAT_P010
        | wgpu::Features::DUAL_SOURCE_BLENDING
        | wgpu::Features::CLEAR_TEXTURE
        | wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
        | wgpu::Features::ADDRESS_MODE_CLAMP_TO_ZERO
        | wgpu::Features::IMMEDIATES
        | wgpu::Features::SUBGROUP
        | wgpu::Features::INDIRECT_FIRST_INSTANCE
        | wgpu::Features::DEPTH32FLOAT_STENCIL8
        | wgpu::Features::PIPELINE_CACHE
}

/// Texture formats whose support matters to BAE, reported by `bae gpu-info`.
pub const REPORTED_FORMATS: &[wgpu::TextureFormat] = &[
    wgpu::TextureFormat::Rgba8Unorm,
    wgpu::TextureFormat::Rgba8UnormSrgb,
    wgpu::TextureFormat::Bgra8Unorm,
    wgpu::TextureFormat::Bgra8UnormSrgb,
    wgpu::TextureFormat::Rgb10a2Unorm,
    wgpu::TextureFormat::Rg11b10Ufloat,
    wgpu::TextureFormat::Rgba16Unorm,
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::Rgba32Float,
    wgpu::TextureFormat::R16Float,
    wgpu::TextureFormat::R32Float,
    wgpu::TextureFormat::R8Unorm,
];

/// What the shared device can do, in terms the rest of BAE cares about.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuCapabilities {
    /// Backend name (`Metal`, `D3D12`, …).
    pub backend: &'static str,
    /// Device class (`discrete`, `integrated`, `software`, …).
    pub device_type: &'static str,
    /// A software rasterizer (WARP, llvmpipe, SwiftShader): expect it to be slow.
    pub software: bool,
    /// CPU and GPU share memory (integrated GPUs, Apple silicon): uploads and
    /// readbacks are cheap.
    pub unified_memory: bool,
    /// Timestamps on pass boundaries.
    pub timestamps: bool,
    /// Timestamps anywhere in a command encoder.
    pub timestamps_inside_encoders: bool,
    /// Timestamps inside passes.
    pub timestamps_inside_passes: bool,
    /// Nanoseconds per timestamp tick.
    pub timestamp_period_ns: f32,
    /// `f16` in shaders.
    pub shader_f16: bool,
    /// Subgroup (wave/SIMD-group) operations.
    pub subgroups: bool,
    /// Push-constant style immediates.
    pub immediates: bool,
    /// RGBA32F can be filtered.
    pub float32_filterable: bool,
    /// RGBA32F can be blended.
    pub float32_blendable: bool,
    /// RGBA16F supports read-write storage access (compute effects in place).
    pub rgba16f_storage: bool,
    /// RGBA32F supports read-write storage access.
    pub rgba32f_storage: bool,
    /// BGRA8 supports storage writes.
    pub bgra8_storage: bool,
    /// 16-bit normalized formats.
    pub norm16_formats: bool,
    /// NV12 / P010 textures (hardware video decode interop).
    pub video_formats: bool,
    /// Largest 2D texture side.
    pub max_texture_dimension_2d: u32,
    /// Largest buffer.
    pub max_buffer_size: u64,
    /// Largest storage binding (Vello needs big ones).
    pub max_storage_buffer_binding_size: u64,
    /// Compute workgroup limits.
    pub max_compute_invocations_per_workgroup: u32,
    /// Compute workgroup storage.
    pub max_compute_workgroup_storage_size: u32,
}

impl GpuCapabilities {
    pub(crate) fn detect(
        adapter: &wgpu::Adapter,
        info: &wgpu::AdapterInfo,
        features: wgpu::Features,
        limits: &wgpu::Limits,
        queue: &wgpu::Queue,
    ) -> Self {
        let format_flags = |format| {
            if features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES) {
                adapter.get_texture_format_features(format).flags
            } else {
                format.guaranteed_format_features(features).flags
            }
        };
        let read_write = wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE;
        let timestamps = features.contains(wgpu::Features::TIMESTAMP_QUERY);
        Self {
            backend: backend_name(info.backend),
            device_type: device_type_name(info.device_type),
            software: info.device_type == wgpu::DeviceType::Cpu,
            unified_memory: matches!(
                info.device_type,
                wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu
            ),
            timestamps,
            timestamps_inside_encoders: timestamps
                && features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
            timestamps_inside_passes: timestamps
                && features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES),
            timestamp_period_ns: if timestamps {
                queue.get_timestamp_period()
            } else {
                0.0
            },
            shader_f16: features.contains(wgpu::Features::SHADER_F16),
            subgroups: features.contains(wgpu::Features::SUBGROUP),
            immediates: features.contains(wgpu::Features::IMMEDIATES),
            float32_filterable: features.contains(wgpu::Features::FLOAT32_FILTERABLE),
            float32_blendable: features.contains(wgpu::Features::FLOAT32_BLENDABLE),
            rgba16f_storage: format_flags(wgpu::TextureFormat::Rgba16Float).contains(read_write),
            rgba32f_storage: format_flags(wgpu::TextureFormat::Rgba32Float).contains(read_write),
            bgra8_storage: features.contains(wgpu::Features::BGRA8UNORM_STORAGE),
            norm16_formats: features.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM),
            video_formats: features.contains(
                wgpu::Features::TEXTURE_FORMAT_NV12 | wgpu::Features::TEXTURE_FORMAT_P010,
            ),
            max_texture_dimension_2d: limits.max_texture_dimension_2d,
            max_buffer_size: limits.max_buffer_size,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_compute_invocations_per_workgroup: limits.max_compute_invocations_per_workgroup,
            max_compute_workgroup_storage_size: limits.max_compute_workgroup_storage_size,
        }
    }
}

/// One adapter in the report.
#[derive(Debug, Clone, Serialize)]
pub struct AdapterReport {
    /// Adapter name.
    pub name: String,
    /// Backend.
    pub backend: &'static str,
    /// Device class.
    pub device_type: &'static str,
    /// PCI vendor id.
    pub vendor: u32,
    /// PCI device id.
    pub device: u32,
    /// Driver name.
    pub driver: String,
    /// Driver version / details.
    pub driver_info: String,
    /// `true` for the adapter BAE uses.
    pub selected: bool,
}

impl AdapterReport {
    fn new(info: &wgpu::AdapterInfo, selected: bool) -> Self {
        Self {
            name: info.name.clone(),
            backend: backend_name(info.backend),
            device_type: device_type_name(info.device_type),
            vendor: info.vendor,
            device: info.device,
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
            selected,
        }
    }
}

/// Support for one texture format.
#[derive(Debug, Clone, Serialize)]
pub struct FormatReport {
    /// Format name.
    pub format: String,
    /// Can be sampled with filtering.
    pub filterable: bool,
    /// Can be a render target.
    pub render: bool,
    /// Supports blending.
    pub blendable: bool,
    /// Supports 4× MSAA.
    pub msaa4: bool,
    /// Storage read-write access.
    pub storage_read_write: bool,
    /// Storage write-only access.
    pub storage_write: bool,
}

/// Everything `bae gpu-info` prints. Serializable for `--json` and crash reports.
#[derive(Debug, Clone, Serialize)]
pub struct GpuReport {
    /// wgpu version the binary was built with.
    pub wgpu_version: &'static str,
    /// Backends compiled into this binary.
    pub compiled_backends: Vec<&'static str>,
    /// Backends that were searched.
    pub searched_backends: String,
    /// Every enumerated adapter.
    pub adapters: Vec<AdapterReport>,
    /// The selected adapter.
    pub selected: AdapterReport,
    /// Derived capabilities.
    pub capabilities: GpuCapabilities,
    /// Enabled device features.
    pub enabled_features: Vec<String>,
    /// Optional features the adapter does not support.
    pub unsupported_optional_features: Vec<String>,
    /// Device limits.
    pub limits: wgpu::Limits,
    /// Downlevel flags (missing WebGPU-level functionality), if any.
    pub downlevel_flags: Vec<String>,
    /// Shader model reported by the adapter.
    pub shader_model: String,
    /// Texture format support.
    pub formats: Vec<FormatReport>,
}

impl GpuReport {
    /// Builds the report for a context.
    pub fn new(ctx: &GpuContext) -> Self {
        let info = ctx.adapter_info();
        let adapters = ctx
            .available_adapters()
            .iter()
            .map(|a| AdapterReport::new(a, same_adapter(a, info)))
            .collect();
        let features = ctx.features();
        let check_formats =
            features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
        let formats = REPORTED_FORMATS
            .iter()
            .map(|&format| {
                let flags = if check_formats {
                    ctx.adapter().get_texture_format_features(format)
                } else {
                    format.guaranteed_format_features(features)
                };
                let f = flags.flags;
                use wgpu::TextureFormatFeatureFlags as F;
                FormatReport {
                    format: format!("{format:?}"),
                    filterable: f.contains(F::FILTERABLE),
                    render: flags
                        .allowed_usages
                        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT),
                    blendable: f.contains(F::BLENDABLE),
                    msaa4: f.contains(F::MULTISAMPLE_X4),
                    storage_read_write: f.contains(F::STORAGE_READ_WRITE),
                    storage_write: f.contains(F::STORAGE_WRITE_ONLY)
                        || flags
                            .allowed_usages
                            .contains(wgpu::TextureUsages::STORAGE_BINDING),
                }
            })
            .collect();
        let downlevel = ctx.downlevel();
        Self {
            wgpu_version: WGPU_VERSION,
            compiled_backends: compiled_backends(),
            searched_backends: format!("{:?}", ctx.descriptor().backends),
            adapters,
            selected: AdapterReport::new(info, true),
            capabilities: ctx.capabilities().clone(),
            enabled_features: feature_names(features),
            unsupported_optional_features: feature_names(
                ctx.descriptor().optional_features - ctx.adapter().features(),
            ),
            limits: ctx.limits().clone(),
            downlevel_flags: if downlevel.is_webgpu_compliant() {
                Vec::new()
            } else {
                (wgpu::DownlevelFlags::compliant() - downlevel.flags)
                    .iter_names()
                    .map(|(name, _)| name.to_owned())
                    .collect()
            },
            shader_model: format!("{:?}", downlevel.shader_model),
            formats,
        }
    }

    /// Serializes the report as pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

/// wgpu version compiled in (kept in sync with the workspace pin).
pub const WGPU_VERSION: &str = "30";

fn same_adapter(a: &wgpu::AdapterInfo, b: &wgpu::AdapterInfo) -> bool {
    a.name == b.name && a.backend == b.backend && a.device == b.device && a.vendor == b.vendor
}

/// Names of the set features, e.g. `["TIMESTAMP_QUERY", "SHADER_F16"]`.
pub fn feature_names(features: wgpu::Features) -> Vec<String> {
    features
        .iter_names()
        .map(|(name, _)| name.to_owned())
        .collect()
}

/// Backends compiled into this binary.
pub fn compiled_backends() -> Vec<&'static str> {
    let compiled = wgpu::Instance::enabled_backend_features();
    [
        (wgpu::Backends::METAL, "Metal"),
        (wgpu::Backends::DX12, "D3D12"),
        (wgpu::Backends::VULKAN, "Vulkan"),
        (wgpu::Backends::GL, "OpenGL"),
    ]
    .into_iter()
    .filter(|(b, _)| compiled.contains(*b))
    .map(|(_, name)| name)
    .collect()
}

fn yes(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn bytes(value: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if value >= 1024 * MIB {
        format!("{:.1} GiB", value as f64 / (1024 * MIB) as f64)
    } else if value >= MIB {
        format!("{} MiB", value / MIB)
    } else {
        format!("{value} B")
    }
}

impl fmt::Display for GpuReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let caps = &self.capabilities;
        writeln!(
            f,
            "wgpu {} — compiled backends: {} — searched: {}",
            self.wgpu_version,
            self.compiled_backends.join(", "),
            self.searched_backends
        )?;
        writeln!(f, "\nAdapters:")?;
        for a in &self.adapters {
            writeln!(
                f,
                "  {} {} [{} / {}] vendor {:#06x} device {:#06x} driver {} {}",
                if a.selected { "*" } else { " " },
                a.name,
                a.backend,
                a.device_type,
                a.vendor,
                a.device,
                a.driver,
                a.driver_info
            )?;
        }
        if !self.adapters.iter().any(|a| a.selected) {
            writeln!(
                f,
                "  * {} [{} / {}] (fallback adapter)",
                self.selected.name, self.selected.backend, self.selected.device_type
            )?;
        }
        writeln!(f, "\nCapabilities:")?;
        let rows: [(&str, String); 19] = [
            ("software rasterizer", yes(caps.software).into()),
            ("unified memory", yes(caps.unified_memory).into()),
            (
                "timestamps (pass / encoder / in-pass)",
                format!(
                    "{} / {} / {}",
                    yes(caps.timestamps),
                    yes(caps.timestamps_inside_encoders),
                    yes(caps.timestamps_inside_passes)
                ),
            ),
            (
                "timestamp period",
                format!("{} ns", caps.timestamp_period_ns),
            ),
            ("shader f16", yes(caps.shader_f16).into()),
            ("subgroups", yes(caps.subgroups).into()),
            ("immediates", yes(caps.immediates).into()),
            ("float32 filterable", yes(caps.float32_filterable).into()),
            ("float32 blendable", yes(caps.float32_blendable).into()),
            (
                "rgba16f storage read-write",
                yes(caps.rgba16f_storage).into(),
            ),
            (
                "rgba32f storage read-write",
                yes(caps.rgba32f_storage).into(),
            ),
            ("bgra8 storage", yes(caps.bgra8_storage).into()),
            ("16-bit norm formats", yes(caps.norm16_formats).into()),
            ("NV12 / P010", yes(caps.video_formats).into()),
            (
                "max texture 2D",
                format!(
                    "{} × {}",
                    caps.max_texture_dimension_2d, caps.max_texture_dimension_2d
                ),
            ),
            ("max buffer", bytes(caps.max_buffer_size)),
            (
                "max storage binding",
                bytes(caps.max_storage_buffer_binding_size),
            ),
            (
                "max invocations / workgroup",
                caps.max_compute_invocations_per_workgroup.to_string(),
            ),
            ("shader model", self.shader_model.clone()),
        ];
        for (name, value) in rows {
            writeln!(f, "  {name:<38} {value}")?;
        }
        writeln!(f, "\nEnabled features ({}):", self.enabled_features.len())?;
        writeln!(f, "{}", wrap(&self.enabled_features, "  "))?;
        if !self.unsupported_optional_features.is_empty() {
            writeln!(f, "\nOptional features not supported:")?;
            writeln!(f, "{}", wrap(&self.unsupported_optional_features, "  "))?;
        }
        if !self.downlevel_flags.is_empty() {
            writeln!(f, "\nMissing WebGPU functionality:")?;
            writeln!(f, "{}", wrap(&self.downlevel_flags, "  "))?;
        }
        writeln!(f, "\nTexture formats:")?;
        writeln!(
            f,
            "  {:<16} {:>6} {:>6} {:>6} {:>6} {:>8} {:>8}",
            "format", "filter", "render", "blend", "msaa4", "storage", "st-rw"
        )?;
        for fr in &self.formats {
            writeln!(
                f,
                "  {:<16} {:>6} {:>6} {:>6} {:>6} {:>8} {:>8}",
                fr.format,
                yes(fr.filterable),
                yes(fr.render),
                yes(fr.blendable),
                yes(fr.msaa4),
                yes(fr.storage_write),
                yes(fr.storage_read_write)
            )?;
        }
        Ok(())
    }
}

/// Joins names into lines of at most ~96 characters.
fn wrap(names: &[String], indent: &str) -> String {
    let mut out = String::new();
    let mut line = String::from(indent);
    for (i, name) in names.iter().enumerate() {
        let piece = if i + 1 < names.len() {
            format!("{name}, ")
        } else {
            name.clone()
        };
        if line.len() + piece.len() > 96 && line.len() > indent.len() {
            let _ = writeln!(out, "{}", line.trim_end());
            line = String::from(indent);
        }
        line.push_str(&piece);
    }
    out.push_str(line.trim_end());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_features_include_timestamps_and_f16() {
        let f = optional_features();
        assert!(f.contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::SHADER_F16));
        let names = feature_names(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::SHADER_F16);
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|n| n == "TIMESTAMP_QUERY"));
    }

    #[test]
    fn wrapping_keeps_every_name() {
        let names: Vec<String> = (0..40).map(|i| format!("FEATURE_{i}")).collect();
        let text = wrap(&names, "  ");
        assert!(text.lines().all(|l| l.len() <= 100));
        assert_eq!(text.matches("FEATURE_").count(), 40);
    }

    #[test]
    fn compiled_backends_match_platform() {
        let backends = compiled_backends();
        if cfg!(target_vendor = "apple") {
            assert!(backends.contains(&"Metal"));
        }
        if cfg!(windows) {
            assert!(backends.contains(&"D3D12"));
        }
    }
}
