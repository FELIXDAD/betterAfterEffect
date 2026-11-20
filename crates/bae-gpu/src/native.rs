//! Native Metal / D3D12 handles for plugin interop (ARCHITECTURE §14, §15).
//!
//! Native plugins (C ABI, workstream 19) and the future AE-plugin host (20) get
//! the *same* device the renderer uses, so they can read and write our textures
//! without copies. Handles are raw, borrowed pointers:
//!
//! | API | `device` | `queue` | `texture` |
//! |---|---|---|---|
//! | Metal | `id<MTLDevice>` | `id<MTLCommandQueue>` | `id<MTLTexture>` |
//! | Direct3D 12 | `ID3D12Device*` | `ID3D12CommandQueue*` | `ID3D12Resource*` |
//!
//! They are **not** retained for the caller: they stay valid while the
//! [`GpuContext`] (device, queue) or the `wgpu::Texture` (texture) is alive. A
//! plugin that keeps one longer must retain it itself (`CFRetain` / `AddRef`) and
//! must never release ours. Work submitted on the native queue must be ordered
//! with wgpu's submissions by the plugin host (e.g. by submitting on the same
//! queue from the render thread between wgpu submissions).

use std::ffi::c_void;

use serde::Serialize;

use crate::context::GpuContext;

/// The native graphics API behind the shared device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum NativeApi {
    /// Apple Metal.
    Metal,
    /// Microsoft Direct3D 12.
    Direct3D12,
    /// Vulkan (handles not exposed yet).
    Vulkan,
    /// OpenGL / GLES (handles not exposed).
    OpenGl,
    /// Anything else.
    Other,
}

/// Raw device and queue handles; see the [module docs](self) for lifetimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeDevice {
    /// The API the pointers belong to.
    pub api: NativeApi,
    /// `id<MTLDevice>` or `ID3D12Device*`.
    pub device: *mut c_void,
    /// `id<MTLCommandQueue>` or `ID3D12CommandQueue*`.
    pub queue: *mut c_void,
}

// SAFETY: MTLDevice, MTLCommandQueue, ID3D12Device and ID3D12CommandQueue are
// documented as free-threaded objects; the struct only carries the pointers.
unsafe impl Send for NativeDevice {}
// SAFETY: see `Send` above.
unsafe impl Sync for NativeDevice {}

/// A raw texture handle; see the [module docs](self) for lifetimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTexture {
    /// The API the pointer belongs to.
    pub api: NativeApi,
    /// `id<MTLTexture>` or `ID3D12Resource*`.
    pub texture: *mut c_void,
}

// SAFETY: MTLTexture and ID3D12Resource are free-threaded objects.
unsafe impl Send for NativeTexture {}
// SAFETY: see `Send` above.
unsafe impl Sync for NativeTexture {}

impl GpuContext {
    /// The native API of the shared device.
    pub fn native_api(&self) -> NativeApi {
        match self.adapter_info().backend {
            wgpu::Backend::Metal => NativeApi::Metal,
            wgpu::Backend::Dx12 => NativeApi::Direct3D12,
            wgpu::Backend::Vulkan => NativeApi::Vulkan,
            wgpu::Backend::Gl => NativeApi::OpenGl,
            _ => NativeApi::Other,
        }
    }

    /// Raw device and queue handles, or `None` on backends without native
    /// interop (Vulkan, GL, software).
    ///
    /// # Safety
    ///
    /// The caller must not release the objects, must not use them after this
    /// context is dropped, and must keep work submitted through them ordered with
    /// wgpu's own submissions. All of wgpu-hal's safety rules apply.
    pub unsafe fn native_device(&self) -> Option<NativeDevice> {
        match self.native_api() {
            #[cfg(target_vendor = "apple")]
            NativeApi::Metal => {
                // SAFETY: the guards are dropped before returning; the pointers stay
                // valid as long as `self` (which owns the device and queue).
                let device = unsafe { self.device().as_hal::<wgpu::hal::api::Metal>() }?;
                // SAFETY: as above.
                let queue = unsafe { self.queue().as_hal::<wgpu::hal::api::Metal>() }?;
                Some(NativeDevice {
                    api: NativeApi::Metal,
                    device: std::ptr::from_ref(&**device.raw_device())
                        .cast::<c_void>()
                        .cast_mut(),
                    queue: std::ptr::from_ref(queue.as_raw())
                        .cast::<c_void>()
                        .cast_mut(),
                })
            }
            #[cfg(windows)]
            NativeApi::Direct3D12 => {
                use windows::core::Interface as _;
                // SAFETY: the guard is dropped before returning; the COM pointers
                // stay valid as long as `self` owns the device.
                let device = unsafe { self.device().as_hal::<wgpu::hal::api::Dx12>() }?;
                Some(NativeDevice {
                    api: NativeApi::Direct3D12,
                    device: device.raw_device().as_raw(),
                    queue: device.raw_queue().as_raw(),
                })
            }
            _ => None,
        }
    }

    /// The native handle of a texture created on this context's device, or
    /// `None` on backends without native interop.
    ///
    /// # Safety
    ///
    /// `texture` must belong to this context's device. The caller must not
    /// release the object, must not use it after `texture` is dropped or
    /// destroyed, and must synchronize access with wgpu (no native access while
    /// wgpu work using the texture is in flight, and resource states/layouts left
    /// as wgpu expects them).
    pub unsafe fn native_texture(&self, texture: &wgpu::Texture) -> Option<NativeTexture> {
        let _ = texture;
        match self.native_api() {
            #[cfg(target_vendor = "apple")]
            NativeApi::Metal => {
                // SAFETY: guard dropped before returning; see the method contract.
                let hal = unsafe { texture.as_hal::<wgpu::hal::api::Metal>() }?;
                Some(NativeTexture {
                    api: NativeApi::Metal,
                    texture: std::ptr::from_ref(hal.raw_handle())
                        .cast::<c_void>()
                        .cast_mut(),
                })
            }
            #[cfg(windows)]
            NativeApi::Direct3D12 => {
                use windows::core::Interface as _;
                // SAFETY: guard dropped before returning; see the method contract.
                let hal = unsafe { texture.as_hal::<wgpu::hal::api::Dx12>() }?;
                // SAFETY: the resource is alive while `texture` is.
                let resource = unsafe { hal.raw_resource() };
                Some(NativeTexture {
                    api: NativeApi::Direct3D12,
                    texture: resource.as_raw(),
                })
            }
            _ => None,
        }
    }

    /// Full wgpu-hal access to the device for backend-specific interop (shared
    /// events, external memory, …). `A` is one of `wgpu::hal::api::{Metal, Dx12,
    /// Vulkan}`; returns `None` if the device is on another backend.
    ///
    /// # Safety
    ///
    /// All of wgpu-hal's safety rules apply; do not destroy anything wgpu owns and
    /// do not hold on to references beyond `f`.
    pub unsafe fn with_hal_device<A: wgpu::hal::Api, R>(
        &self,
        f: impl FnOnce(&A::Device) -> R,
    ) -> Option<R> {
        // SAFETY: forwarded to the caller.
        let device = unsafe { self.device().as_hal::<A>() }?;
        Some(f(&device))
    }
}
