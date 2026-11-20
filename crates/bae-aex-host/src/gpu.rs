//! GPU-rendering AE effects → BAE's wgpu device.
//!
//! AE GPU effects (AE 16.0+) opt in per device at `PF_Cmd_GPU_DEVICE_SETUP`,
//! say per request whether GPU rendering is possible during pre-render, and
//! render in `PF_Cmd_SMART_RENDER_GPU` into *buffers* (not textures) of
//! 128-bit float BGRA pixels owned by the host. The host exposes its native
//! device, context and command queue through a GPU device suite and
//! allocates GPU worlds on request (SDK guide, "Effect Basics › Command
//! Selectors" GPU entries; "Introduction › Building GPU Effects").
//!
//! BAE renders with wgpu on Metal (macOS) and D3D12 (Windows). The plan
//! (requests R01-01, R04-04, R19-04): expose the native Metal / D3D12 device
//! and queue through wgpu-hal, keep a pool of BGRA128 storage buffers, and
//! convert between RGBA16F textures and those buffers with
//! [`crate::shaders::CONVERT_WGSL`] — zero CPU involvement. CUDA (Windows,
//! NVIDIA) would need D3D12↔CUDA external-memory interop and is deferred;
//! OpenCL is not offered.

use core::num::NonZeroUsize;

/// GPU framework identifiers of the AE protocol (`PF_GPU_Framework_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuFramework {
    /// CPU render.
    None,
    /// NVIDIA CUDA (Windows).
    Cuda,
    /// OpenCL (legacy).
    OpenCl,
    /// Apple Metal (macOS).
    Metal,
    /// DirectX 12 (Windows).
    DirectX,
}

/// BAE GPU backend in use (mirror of the wgpu backend; no wgpu dependency).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaeBackend {
    /// Metal.
    Metal,
    /// Direct3D 12.
    Dx12,
    /// Vulkan.
    Vulkan,
}

/// Interop route between BAE's device and a framework.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteropRoute {
    /// Same native device and queue; buffers are shared directly.
    SharedDevice,
    /// Different API on the same adapter via exported memory + semaphores
    /// (e.g. D3D12 → CUDA). Deferred.
    ExternalMemory,
    /// Not offered.
    Unsupported,
}

/// Which route BAE would use for `fw` while running on `backend`.
pub fn interop_route(backend: BaeBackend, fw: GpuFramework) -> InteropRoute {
    match (backend, fw) {
        (BaeBackend::Metal, GpuFramework::Metal) | (BaeBackend::Dx12, GpuFramework::DirectX) => {
            InteropRoute::SharedDevice
        }
        (BaeBackend::Dx12 | BaeBackend::Vulkan, GpuFramework::Cuda) => InteropRoute::ExternalMemory,
        _ => InteropRoute::Unsupported,
    }
}

/// Frameworks the first compat host offers on a backend, in preference order.
pub fn offered_frameworks(backend: BaeBackend) -> &'static [GpuFramework] {
    match backend {
        BaeBackend::Metal => &[GpuFramework::Metal],
        BaeBackend::Dx12 => &[GpuFramework::DirectX],
        BaeBackend::Vulkan => &[],
    }
}

/// Raw native handles handed to a plug-in for one device. Values are opaque
/// pointer-sized handles (e.g. `id<MTLDevice>`, `ID3D12Device*`); they stay
/// valid between `GPU_DEVICE_SETUP` and `GPU_DEVICE_SETDOWN` for that device.
/// Only the P3 FFI shim converts them to pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeDeviceHandles {
    /// Framework these handles belong to.
    pub framework: GpuFramework,
    /// Host device index (stable for the session).
    pub device_index: u32,
    /// Native device.
    pub device: NonZeroUsize,
    /// Native command queue the plug-in must submit on (ordering with BAE's
    /// own work is then implicit on Metal/D3D12 single-queue setups).
    pub command_queue: NonZeroUsize,
    /// Framework context where applicable (CUDA context, CL context).
    pub context: Option<NonZeroUsize>,
}

/// Pixel layout of AE GPU worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPixelFormat {
    /// 4 × f32 per pixel, memory order B, G, R, A, premultiplied.
    Bgra128,
}

/// A host-allocated GPU world (device buffer) description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuWorldDesc {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Row pitch in bytes (BAE uses 256-byte alignment so the same buffer
    /// can be copied to/from textures without repacking).
    pub row_pitch: usize,
    /// Pixel format.
    pub format: GpuPixelFormat,
    /// Device index the buffer lives on.
    pub device_index: u32,
}

impl GpuWorldDesc {
    /// A BGRA128 world with BAE's row alignment.
    pub fn bgra128(width: u32, height: u32, device_index: u32) -> GpuWorldDesc {
        GpuWorldDesc {
            width,
            height,
            row_pitch: crate::pixel::aligned_rowbytes(
                width,
                crate::pixel::PixelDepth::F32,
                crate::pixel::HOST_ROW_ALIGN,
            ),
            format: GpuPixelFormat::Bgra128,
            device_index,
        }
    }

    /// Buffer size in bytes.
    pub fn byte_len(&self) -> usize {
        self.row_pitch * self.height as usize
    }
}

/// How GPU work of the host and the plug-in is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuSync {
    /// Plug-in encodes on the host's queue; submission order is execution
    /// order (Metal command queue, D3D12 direct queue). BAE must have
    /// *submitted* (not necessarily completed) the input conversion before
    /// calling the plug-in, and must submit the output conversion after it
    /// returns.
    SameQueue,
    /// Cross-API: wait on/signal shared semaphores or fences around the call.
    SharedFence,
}

/// Per-device answer of the plug-in at `GPU_DEVICE_SETUP`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceSupport {
    /// The plug-in accepted the device for 32-bit float GPU rendering.
    pub f32: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes() {
        assert_eq!(
            interop_route(BaeBackend::Metal, GpuFramework::Metal),
            InteropRoute::SharedDevice
        );
        assert_eq!(
            interop_route(BaeBackend::Dx12, GpuFramework::DirectX),
            InteropRoute::SharedDevice
        );
        assert_eq!(
            interop_route(BaeBackend::Dx12, GpuFramework::Cuda),
            InteropRoute::ExternalMemory
        );
        assert_eq!(
            interop_route(BaeBackend::Metal, GpuFramework::OpenCl),
            InteropRoute::Unsupported
        );
        assert_eq!(
            offered_frameworks(BaeBackend::Vulkan),
            &[] as &[GpuFramework]
        );
        let w = GpuWorldDesc::bgra128(1920, 1080, 0);
        assert_eq!(w.row_pitch, 1920 * 16);
        assert_eq!(w.byte_len(), 1920 * 16 * 1080);
    }
}
