use bae_gpu::{
    GpuContext, WarmUp,
    wgpu::{self, util::DeviceExt},
};
use bae_render::{Renderer, RendererConfig};
use std::sync::{Arc, Mutex};

pub(super) struct Ready {
    pub gpu: Arc<GpuContext>,
    pub matcher: Matcher,
    pub renderer: Mutex<Renderer>,
}
#[derive(Default)]
pub(super) struct TrackingGpu(Mutex<Option<Arc<Ready>>>);
impl TrackingGpu {
    pub fn ready(&self, gpu: &Arc<GpuContext>) -> Option<Arc<Ready>> {
        self.0
            .lock()
            .ok()?
            .as_ref()
            .filter(|r| Arc::ptr_eq(&r.gpu, gpu) && !gpu.is_lost())
            .cloned()
    }
}
impl WarmUp for TrackingGpu {
    fn name(&self) -> &str {
        "motion tracker"
    }
    fn priority(&self) -> u8 {
        15
    }
    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String> {
        let ready = Ready {
            gpu: gpu.clone(),
            matcher: Matcher::new(gpu.device()),
            renderer: Mutex::new(Renderer::with_config(
                gpu.clone(),
                RendererConfig {
                    frame_cache_bytes: 0,
                    pool_idle_bytes: 32 << 20,
                },
            )),
        };
        *self.0.lock().map_err(|e| e.to_string())? = Some(Arc::new(ready));
        Ok(())
    }
}
pub(super) struct Matcher {
    scores: wgpu::ComputePipeline,
    peak: wgpu::ComputePipeline,
}
pub(super) struct Region<'a> {
    pub reference: &'a wgpu::Texture,
    pub field: u32,
    pub center: [i32; 2],
    pub search_center: [i32; 2],
    pub feature: [u32; 2],
    pub search: [u32; 2],
}
#[derive(Clone, Copy)]
pub(super) struct Options {
    pub channel: u32,
    pub subpixel: bool,
    pub enhance: u32,
    pub field: u32,
    pub radius: u32,
}

impl Matcher {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("motion tracker"),
            source: wgpu::ShaderSource::Wgsl(include_str!("match.wgsl").into()),
        });
        let build = |name| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: None,
                module: &module,
                entry_point: Some(name),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            scores: build("correlate"),
            peak: build("peak"),
        }
    }
    /// Runs on the analysis thread. One readback and one submission for all active points.
    pub fn analyze(
        &self,
        gpu: &GpuContext,
        current: &wgpu::Texture,
        regions: &[Region<'_>],
        opts: Options,
    ) -> Result<Vec<[f32; 3]>, String> {
        if regions.is_empty() || regions.len() > 8 {
            return Err("invalid track point count".into());
        }
        let d = gpu.device();
        let buf = |label, size, usage| {
            d.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let staging = buf(
            "tracking samples",
            16 * regions.len() as u64,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let cur = current.create_view(&Default::default());
        let mut enc = d.create_command_encoder(&Default::default());
        for (n, r) in regions.iter().enumerate() {
            let f = r.feature.map(|n| n.max(2));
            let search = [r.search[0].max(f[0]), r.search[1].max(f[1])];
            let grid = [search[0] - f[0] + 1, search[1] - f[1] + 1];
            let bytes = u64::from(grid[0]) * u64::from(grid[1]) * 4;
            if bytes > d.limits().max_storage_buffer_binding_size
                || grid
                    .iter()
                    .any(|v| v.div_ceil(8) > d.limits().max_compute_workgroups_per_dimension)
            {
                return Err("tracking search region exceeds device limits".into());
            }
            let origin = [
                r.search_center[0] - (grid[0] / 2) as i32,
                r.search_center[1] - (grid[1] / 2) as i32,
            ];
            let words = [
                r.center[0] as u32,
                r.center[1] as u32,
                origin[0] as u32,
                origin[1] as u32,
                f[0],
                f[1],
                grid[0],
                grid[1],
                opts.channel,
                u32::from(opts.subpixel),
                opts.enhance,
                opts.radius,
                r.field,
                opts.field,
                0,
                0,
            ];
            let data: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
            let params = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("tracking window"),
                contents: &data,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let scores = buf("tracking scores", bytes, wgpu::BufferUsages::STORAGE);
            let peak = buf(
                "tracking peak",
                16,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            );
            let reference = r.reference.create_view(&Default::default());
            let entries = [
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&reference),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cur),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: scores.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: peak.as_entire_binding(),
                },
            ];
            for (pipeline, bindings, x, y) in [
                (
                    &self.scores,
                    &entries[..4],
                    grid[0].div_ceil(8),
                    grid[1].div_ceil(8),
                ),
                (&self.peak, &entries[2..], 1, 1),
            ] {
                let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("tracking"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: bindings,
                });
                let mut pass = enc.begin_compute_pass(&Default::default());
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(x, y, 1);
            }
            enc.copy_buffer_to_buffer(&peak, 0, &staging, 16 * n as u64, 16);
        }
        let submission = gpu.queue().submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        staging.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        d.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = staging.get_mapped_range(..).map_err(|e| e.to_string())?;
        Ok(data
            .as_chunks::<16>()
            .0
            .iter()
            .map(|chunk| {
                std::array::from_fn(|i| {
                    f32::from_le_bytes(chunk[4 * i..4 * i + 4].try_into().expect("four bytes"))
                })
            })
            .collect())
    }
}
