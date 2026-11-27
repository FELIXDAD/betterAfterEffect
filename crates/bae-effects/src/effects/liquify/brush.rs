use super::{Mesh, gpu};
use bae_gpu::{
    GpuContext,
    wgpu::{self, util::DeviceExt},
};
use bae_model::Value;
use bae_render::RenderError;
use glam::DVec2;
use gpu::{Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicU32, Ordering},
    mpsc,
};

const LABEL: &str = "bae.effects.liquify.brush";

pub(super) fn kernel(rt: &gpu::Runtime) -> Result<Arc<Kernel>, RenderError> {
    rt.kernel(
        &KernelKey {
            owner: LABEL.into(),
            entry: "brush".into(),
            variant: 0,
        },
        || {
            Ok(KernelSource {
                label: LABEL,
                entry: "brush",
                wgsl: include_str!("brush.wgsl").into(),
                bindings: vec![
                    (0, Binding::Uniform(64)),
                    (1, Binding::ReadBuffer),
                    (2, Binding::ReadBuffer),
                    (3, Binding::ReadWriteBuffer),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}

/// One brush sample in mesh coordinates. Tool and reconstruction mode are zero-based.
pub struct Brush {
    /// Selected tool in the effect toolbar.
    pub tool: u32,
    /// Current position in layer pixels, relative to the mesh offset.
    pub center: DVec2,
    /// Pointer movement since the previous sample, in layer pixels.
    pub drag: DVec2,
    /// Brush radius in layer pixels.
    pub radius: f64,
    /// Brush strength, from zero to one.
    pub pressure: f64,
    /// Turbulence frequency control, from zero to one.
    pub jitter: f64,
    /// Clone source or reconstruction anchor, in mesh coordinates.
    pub source: DVec2,
    /// Reconstruction mode in AE order.
    pub reconstruction: u32,
}

// Only the layer viewer carries this ticket. Documents contain vertex positions.
struct ActivePreview {
    id: u32,
    device: Weak<gpu::Runtime>,
    view: wgpu::TextureView,
    pub(super) origin: DVec2,
    cell: f64,
    size: [u32; 2],
}

static ACTIVE: parking_lot::Mutex<Vec<ActivePreview>> = parking_lot::Mutex::new(Vec::new());
static TICKET: AtomicU32 = AtomicU32::new(1);

fn row_vertices(cols: usize) -> usize {
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize / 16;
    cols.next_multiple_of(alignment)
}

// During a drag the shared viewer override holds a device surface ID and its revision.
pub(super) fn ticket(value: Option<&Value>) -> Option<u32> {
    let Some(Value::Vec2([id, _])) = value else {
        return None;
    };
    Some(*id as u32)
}

pub(super) fn preview(
    rt: &Arc<gpu::Runtime>,
    id: u32,
) -> Option<(wgpu::TextureView, DVec2, f64, [u32; 2])> {
    let entries = ACTIVE.lock();
    let p = entries
        .iter()
        .find(|p| p.id == id && p.device.ptr_eq(&Arc::downgrade(rt)))?;
    Some((p.view.clone(), p.origin, p.cell, p.size))
}

/// Working mesh for one gesture. Clone and reconstruction read the mesh as it
/// was on pointer-down. Only completed readbacks reach the document editor.
pub struct BrushStroke {
    rt: Arc<gpu::Runtime>,
    kernel: Arc<Kernel>,
    mesh: Mesh,
    original: wgpu::Buffer,
    current: wgpu::Buffer,
    next: wgpu::Buffer,
    staging: wgpu::Buffer,
    pending: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
    texture: wgpu::Texture,
    ticket: u32,
    revision: u32,
}

impl BrushStroke {
    /// Upload the initial mesh and the editable fraction of each vertex.
    pub fn new(
        gpu: &Arc<GpuContext>,
        mesh: Mesh,
        weight: impl Fn(DVec2) -> f32,
    ) -> Result<Self, RenderError> {
        let rt = gpu::runtime(gpu);
        let kernel = kernel(&rt)?;
        let stride = row_vertices(mesh.cols);
        let width = mesh.cols as u32;
        let height = mesh.rows as u32;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = rt.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(LABEL),
            size: extent,
            dimension: wgpu::TextureDimension::D2,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mut data = vec![0; mesh.rows * stride * 16];
        for (i, o) in mesh.offsets.iter().enumerate() {
            let (x, y) = (i % mesh.cols, i / mesh.cols);
            let weight = weight(mesh.origin + mesh.cell * DVec2::new(x as f64, y as f64));
            let at = (y * stride + x) * 16;
            let bytes = [o.x as f32, o.y as f32, weight, 0.].map(f32::to_le_bytes);
            data[at..at + 16].copy_from_slice(bytes.as_flattened());
        }
        let storage = wgpu::BufferUsages::STORAGE;
        let original = rt
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(LABEL),
                contents: &data,
                usage: storage | wgpu::BufferUsages::COPY_SRC,
            });
        let create = |usage| {
            rt.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(LABEL),
                size: original.size(),
                usage,
                mapped_at_creation: false,
            })
        };
        let current = create(storage | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST);
        let next = create(storage | wgpu::BufferUsages::COPY_SRC);
        let staging = create(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let mut enc = rt.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(&original, 0, &current, 0, original.size());
        rt.queue.submit([enc.finish()]);
        Ok(Self {
            rt,
            kernel,
            mesh,
            original,
            current,
            next,
            staging,
            pending: None,
            texture,
            ticket: 0,
            revision: 0,
        })
    }

    /// Submit one brush sample without waiting for the GPU.
    pub fn dab(&mut self, brush: &Brush) {
        let mut u = UniformWriter::with_size(64);
        for (i, point) in [self.mesh.origin, brush.center, brush.drag, brush.source]
            .into_iter()
            .enumerate()
        {
            u.f32(i * 8, point.x as f32).f32(i * 8 + 4, point.y as f32);
        }
        u.f32(32, self.mesh.cell as f32)
            .f32(36, brush.radius as f32)
            .f32(40, brush.pressure as f32)
            .f32(44, brush.jitter as f32)
            .u32(48, self.mesh.cols as u32)
            .u32(52, self.mesh.rows as u32)
            .u32(56, brush.tool)
            .u32(60, brush.reconstruction);
        let offset = gpu::push_uniform(&self.rt, &u);
        let group = self.rt.bind_group(
            &self.kernel,
            &[
                Resource::Uniform,
                Resource::Buffer(&self.original),
                Resource::Buffer(&self.current),
                Resource::Buffer(&self.next),
            ],
        );
        let mut enc = self.rt.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            gpu::dispatch(
                &mut pass,
                &self.kernel,
                &group,
                &[offset],
                [self.mesh.cols as u32, self.mesh.rows as u32],
            );
        }
        let stride = row_vertices(self.mesh.cols) as u32;
        enc.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &self.next,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride * 16),
                    rows_per_image: None,
                },
            },
            self.texture.as_image_copy(),
            self.texture.size(),
        );
        self.rt.queue.submit([enc.finish()]);
        std::mem::swap(&mut self.current, &mut self.next);
        if self.ticket == 0 {
            let id = TICKET.fetch_add(1, Ordering::Relaxed).max(1);
            ACTIVE.lock().push(ActivePreview {
                id,
                device: Arc::downgrade(&self.rt),
                origin: self.mesh.origin,
                cell: self.mesh.cell,
                size: [self.mesh.cols as u32, self.mesh.rows as u32],
                view: self.texture.create_view(&Default::default()),
            });
            self.ticket = id;
        }
        self.revision += 1;
    }

    /// Identify this stroke's GPU surface in an evaluated viewer frame.
    /// The revision invalidates a cached frame when the same brush paints another dab.
    pub fn preview_value(&self) -> Option<Value> {
        (self.ticket != 0).then_some(Value::Vec2([self.ticket.into(), self.revision.into()]))
    }

    /// Poll the single snapshot taken after pointer release.
    pub fn committed(&mut self) -> Result<Option<Value>, String> {
        if let Some(rx) = &self.pending {
            self.rt
                .device
                .poll(wgpu::PollType::Poll)
                .map_err(|e| e.to_string())?;
            match rx.try_recv() {
                Ok(status) => status.map_err(|e| e.to_string())?,
                Err(mpsc::TryRecvError::Empty) => return Ok(None),
                Err(e) => return Err(e.to_string()),
            }
            {
                let bytes = self
                    .staging
                    .get_mapped_range(..)
                    .map_err(|e| e.to_string())?;
                let stride = row_vertices(self.mesh.cols);
                for (i, offset) in self.mesh.offsets.iter_mut().enumerate() {
                    let at = (i / self.mesh.cols * stride + i % self.mesh.cols) * 16;
                    let v = &bytes[at..at + 16];
                    *offset = DVec2::new(
                        f32::from_le_bytes(v[0..4].try_into().unwrap()) as f64,
                        f32::from_le_bytes(v[4..8].try_into().unwrap()) as f64,
                    );
                }
            }
            self.staging.unmap();
            self.pending = None;
            return Ok(Some(self.mesh.to_value()));
        }
        {
            let mut enc = self.rt.device.create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(&self.current, 0, &self.staging, 0, self.current.size());
            self.rt.queue.submit([enc.finish()]);
            let (tx, rx) = mpsc::channel();
            self.staging.map_async(wgpu::MapMode::Read, .., move |r| {
                let _ = tx.send(r);
            });
            self.pending = Some(rx);
        }
        Ok(None)
    }
}

impl Drop for BrushStroke {
    fn drop(&mut self) {
        ACTIVE.lock().retain(|p| p.id != self.ticket);
    }
}

/// Fraction of the brush applied at a layer point outside or inside the freeze mask.
pub fn unfrozen_weight(
    shape: &bae_model::BezierPath,
    opacity: f64,
    feather: [f64; 2],
    expansion: f64,
    inverted: bool,
) -> impl Fn(DVec2) -> f32 + use<> {
    use kurbo::{ParamCurveNearest, Point, Shape};
    let path = bae_render::mask::to_bez_path(shape);
    let feather = (feather[0] + feather[1]) * 0.5;
    move |point| {
        let p = Point::new(point.x, point.y);
        let inside = path.contains(p);
        let coverage = if feather > 0. || expansion != 0. {
            let dist = path
                .segments()
                .map(|s| s.nearest(p, 0.5).distance_sq)
                .fold(f64::INFINITY, f64::min)
                .sqrt();
            let signed = if inside { dist } else { -dist };
            ((signed + expansion) / feather.max(0.001) + 0.5).clamp(0., 1.)
        } else {
            f64::from(inside)
        };
        (1. - opacity * if inverted { 1. - coverage } else { coverage }) as f32
    }
}
