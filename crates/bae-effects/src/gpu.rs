//! Per-device GPU state shared by every built-in effect (and by declarative
//! plugin effects run through [`crate::DeclarativeEffect`]): compute pipelines,
//! a uniform ring with dynamic offsets and a placeholder texture.
//!
//! Hot-path rules (plugin spec §22.2): no buffer is created per frame (uniforms
//! go through the ring), pipelines are created once per (kernel, format) and
//! looked up by key, and every effect encodes all of its dispatches into one
//! compute pass.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use bae_gpu::{GpuContext, wgpu};
use bae_render::RenderError;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

/// Size of the uniform ring. 4 MiB = 16384 slots of 256 bytes: far more than
/// one frame (or several frames in flight) can use before wrapping.
const RING_SIZE: u64 = 4 << 20;

/// What a binding of a kernel is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Binding {
    /// A uniform struct of `size` bytes, bound with a dynamic offset into the ring.
    Uniform(u32),
    /// A sampled float texture (read with `textureLoad`, never filtered).
    Texture,
    /// Read-only structured data used by a compute kernel.
    ReadBuffer,
    /// An unfiltered 3D lookup texture.
    Texture3d,
    /// A write-only storage texture.
    Storage(wgpu::TextureFormat),
    /// Read-write structured data used by a compute kernel.
    ReadWriteBuffer,
}

/// A compute pipeline and its bind group layout.
#[derive(Debug)]
pub(crate) struct Kernel {
    pub pipeline: wgpu::ComputePipeline,
    pub layout: wgpu::BindGroupLayout,
    pub bindings: Vec<(u32, Binding)>,
    pub workgroup: [u32; 3],
}

impl Kernel {
    /// Workgroups needed to cover `size` invocations.
    pub fn groups(&self, size: [u32; 2]) -> [u32; 3] {
        [
            size[0].div_ceil(self.workgroup[0]),
            size[1].div_ceil(self.workgroup[1]),
            1,
        ]
    }
}

/// Everything needed to build a kernel.
pub(crate) struct KernelSource {
    pub label: &'static str,
    pub wgsl: String,
    pub entry: &'static str,
    pub bindings: Vec<(u32, Binding)>,
    pub workgroup: [u32; 3],
}

/// Cache key of a kernel: owner (effect id or module name), entry point, and
/// the variant (target format or another discriminator).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KernelKey {
    pub owner: Arc<str>,
    pub entry: Arc<str>,
    pub variant: u64,
}

/// Kernel-cache discriminator of a texture format.
pub(crate) fn format_key(format: wgpu::TextureFormat) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    format.hash(&mut h);
    h.finish()
}

/// A resource bound to a kernel binding.
#[derive(Clone, Copy)]
pub(crate) enum Resource<'a> {
    /// Ring uniform (the dynamic offset is passed at dispatch).
    Uniform,
    View(&'a wgpu::TextureView),
    Buffer(&'a wgpu::Buffer),
}

/// Uniform ring: one buffer, slots handed out round-robin and written with
/// `queue.write_buffer` (ordered before the next submission).
struct UniformRing {
    buffer: wgpu::Buffer,
    align: u64,
    cursor: AtomicU64,
}

impl UniformRing {
    fn push(&self, queue: &wgpu::Queue, bytes: &[u8]) -> u32 {
        let len = (bytes.len().max(16) as u64).div_ceil(self.align) * self.align;
        loop {
            let at = self.cursor.fetch_add(len, Ordering::Relaxed) % RING_SIZE;
            if at + len <= RING_SIZE {
                queue.write_buffer(&self.buffer, at, bytes);
                return at as u32;
            }
        }
    }
}

/// GPU state for one device.
pub(crate) struct Runtime {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    ring: UniformRing,
    placeholder: wgpu::TextureView,
    kernels: Mutex<FxHashMap<KernelKey, Arc<Kernel>>>,
    scratch: Mutex<FxHashMap<&'static str, wgpu::Buffer>>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("kernels", &self.kernels.lock().len())
            .finish_non_exhaustive()
    }
}

/// Runtimes by GPU context. Keyed by the context itself, not by
/// `wgpu::Device` equality: device ids are per wgpu instance, so devices of
/// two contexts can compare equal.
static RUNTIMES: Mutex<Vec<(Weak<GpuContext>, Arc<Runtime>)>> = Mutex::new(Vec::new());

/// The runtime of `gpu` (created on first use, dropped with the context).
pub(crate) fn runtime(gpu: &Arc<GpuContext>) -> Arc<Runtime> {
    let mut list = RUNTIMES.lock();
    list.retain(|(ctx, _)| ctx.strong_count() > 0);
    if let Some((_, rt)) = list
        .iter()
        .find(|(ctx, _)| std::ptr::eq(ctx.as_ptr(), Arc::as_ptr(gpu)))
    {
        return rt.clone();
    }
    let rt = Arc::new(Runtime::new(gpu.device().clone(), gpu.queue().clone()));
    list.push((Arc::downgrade(gpu), rt.clone()));
    rt
}

impl Runtime {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae.effects.uniform_ring"),
            size: RING_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            // Created mapped (zeroed) so wgpu considers the whole ring
            // initialized: otherwise its lazy zero fill of "uninitialized"
            // bound ranges can race with slots written by other threads.
            mapped_at_creation: true,
        });
        buffer.unmap();
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(16);
        let placeholder = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae.effects.placeholder"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            device,
            queue,
            ring: UniformRing {
                buffer,
                align,
                cursor: AtomicU64::new(0),
            },
            placeholder,
            kernels: Mutex::new(FxHashMap::default()),
            scratch: Mutex::new(FxHashMap::default()),
        }
    }

    /// The grow-only storage buffer `name` of at least `size` bytes, shared by
    /// every user on this device. Only for passes that fill it on the GPU (or
    /// with `clear_buffer`) in the command buffer that reads it: commands on
    /// one queue run in order, so sharing is safe. Never write it from the CPU.
    pub fn scratch(&self, name: &'static str, size: u64) -> wgpu::Buffer {
        let mut buffers = self.scratch.lock();
        if let Some(buffer) = buffers.get(name)
            && buffer.size() >= size
        {
            return buffer.clone();
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(name),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        buffers.insert(name, buffer.clone());
        buffer
    }

    /// Writes `bytes` into a ring slot; returns its dynamic offset.
    pub fn uniform(&self, bytes: &[u8]) -> u32 {
        self.ring.push(&self.queue, bytes)
    }

    /// The kernel for `key`, built from `source` on first use.
    pub fn kernel(
        &self,
        key: &KernelKey,
        source: impl FnOnce() -> Result<KernelSource, RenderError>,
    ) -> Result<Arc<Kernel>, RenderError> {
        if let Some(k) = self.kernels.lock().get(key) {
            return Ok(k.clone());
        }
        let src = source()?;
        let kernel = Arc::new(self.build(&src)?);
        self.kernels.lock().insert(key.clone(), kernel.clone());
        Ok(kernel)
    }

    fn build(&self, src: &KernelSource) -> Result<Kernel, RenderError> {
        let dev = &self.device;
        let scope = dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(src.label),
            source: wgpu::ShaderSource::Wgsl(src.wgsl.as_str().into()),
        });
        let entries: Vec<wgpu::BindGroupLayoutEntry> = src
            .bindings
            .iter()
            .map(|&(binding, kind)| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: match kind {
                    Binding::Uniform(size) => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(u64::from(size)),
                    },
                    Binding::Texture | Binding::Texture3d => wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: if matches!(kind, Binding::Texture3d) {
                            wgpu::TextureViewDimension::D3
                        } else {
                            wgpu::TextureViewDimension::D2
                        },
                        multisampled: false,
                    },
                    Binding::ReadBuffer | Binding::ReadWriteBuffer => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: matches!(kind, Binding::ReadBuffer),
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    Binding::Storage(format) => wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                },
                count: None,
            })
            .collect();
        let layout = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(src.label),
            entries: &entries,
        });
        let pipeline_layout = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(src.label),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(src.label),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(src.entry),
            // Every kernel writes its workgroup memory before reading it; the
            // default zero fill costs a serial store loop per workgroup.
            compilation_options: wgpu::PipelineCompilationOptions {
                zero_initialize_workgroup_memory: false,
                ..Default::default()
            },
            cache: None,
        });
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Other(format!(
                "{} ({}): pipeline creation failed: {err}",
                src.label, src.entry
            )));
        }
        Ok(Kernel {
            pipeline,
            layout,
            bindings: src.bindings.clone(),
            workgroup: src.workgroup,
        })
    }

    /// A bind group for `kernel` with `resources` in binding order.
    pub fn bind_group(&self, kernel: &Kernel, resources: &[Resource<'_>]) -> wgpu::BindGroup {
        debug_assert_eq!(kernel.bindings.len(), resources.len());
        let entries: Vec<wgpu::BindGroupEntry<'_>> = kernel
            .bindings
            .iter()
            .zip(resources)
            .map(|(&(binding, kind), res)| wgpu::BindGroupEntry {
                binding,
                resource: match (kind, res) {
                    (Binding::Uniform(size), _) => {
                        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.ring.buffer,
                            offset: 0,
                            size: NonZeroU64::new(u64::from(size)),
                        })
                    }
                    (_, Resource::Buffer(b)) => b.as_entire_binding(),
                    (_, Resource::View(v)) => wgpu::BindingResource::TextureView(v),
                    (_, Resource::Uniform) => wgpu::BindingResource::TextureView(&self.placeholder),
                },
            })
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &kernel.layout,
            entries: &entries,
        })
    }
}

/// Little-endian packing of uniform structs.
#[derive(Default)]
pub(crate) struct UniformWriter {
    pub bytes: Vec<u8>,
}

impl UniformWriter {
    pub fn with_size(size: usize) -> Self {
        Self {
            bytes: vec![0; size.div_ceil(16) * 16],
        }
    }

    fn put(&mut self, offset: usize, data: &[u8]) {
        let end = offset + data.len();
        if self.bytes.len() < end {
            self.bytes.resize(end.div_ceil(16) * 16, 0);
        }
        self.bytes[offset..end].copy_from_slice(data);
    }

    pub fn f32(&mut self, offset: usize, v: f32) -> &mut Self {
        self.put(offset, &v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, offset: usize, v: i32) -> &mut Self {
        self.put(offset, &v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, offset: usize, v: u32) -> &mut Self {
        self.put(offset, &v.to_le_bytes());
        self
    }

    pub fn f32s(&mut self, offset: usize, values: &[f32]) -> &mut Self {
        for (i, x) in values.iter().enumerate() {
            self.f32(offset + 4 * i, *x);
        }
        self
    }

    pub fn i32s(&mut self, offset: usize, v: &[i32]) -> &mut Self {
        for (i, x) in v.iter().enumerate() {
            self.i32(offset + 4 * i, *x);
        }
        self
    }
}

/// Uploads a uniform struct and returns its dynamic offset.
pub(crate) fn push_uniform(rt: &Runtime, w: &UniformWriter) -> u32 {
    rt.uniform(&w.bytes)
}

/// Records one dispatch in an open compute pass.
pub(crate) fn dispatch(
    pass: &mut wgpu::ComputePass<'_>,
    kernel: &Kernel,
    group: &wgpu::BindGroup,
    offsets: &[u32],
    size: [u32; 2],
) {
    if size[0] == 0 || size[1] == 0 {
        return;
    }
    let [x, y, z] = kernel.groups(size);
    pass.set_pipeline(&kernel.pipeline);
    pass.set_bind_group(0, group, offsets);
    pass.dispatch_workgroups(x, y, z);
}
