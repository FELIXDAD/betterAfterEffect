use bae_gpu::{GpuContext, wgpu};
use bae_motion::camera_track::Sample;
const MAX_POINTS: u64 = 24 * 16;
pub(super) struct Features {
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    tracks: wgpu::Buffer,
    staging: wgpu::Buffer,
}
impl Features {
    pub fn new(d: &wgpu::Device) -> Self {
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Camera track points"),
            source: wgpu::ShaderSource::Wgsl(include_str!("features.wgsl").into()),
        });
        let pipeline = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Camera feature tracking"),
            layout: None,
            module: &module,
            entry_point: Some("follow"),
            compilation_options: Default::default(),
            cache: None,
        });
        let buffer = |label, size, usage| {
            d.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        use wgpu::BufferUsages as U;
        Self {
            pipeline,
            params: buffer("Camera analysis settings", 16, U::UNIFORM | U::COPY_DST),
            tracks: buffer("Camera features", MAX_POINTS * 16, U::STORAGE | U::COPY_SRC),
            staging: buffer(
                "Camera measurements",
                MAX_POINTS * 16,
                U::MAP_READ | U::COPY_DST,
            ),
        }
    }
    /// Called only by the background analysis holding the tracking renderer. Only
    /// sparse measurements cross to the CPU; both full frames stay on the GPU.
    pub fn measure(
        &self,
        gpu: &GpuContext,
        previous: &wgpu::Texture,
        current: &wgpu::Texture,
        frame: u32,
        detailed: bool,
        ids: &mut Vec<u32>,
    ) -> Result<Vec<Sample>, String> {
        let (cols, rows, radius) = if detailed {
            (24u32, 16u32, 15u32)
        } else {
            (16, 12, 10)
        };
        let size = (cols * rows) as usize;
        let first = u32::from(frame == 0);
        let words = [cols, rows, first, radius];
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        gpu.queue().write_buffer(&self.params, 0, &bytes);
        let device = gpu.device();
        let prev = previous.create_view(&Default::default());
        let cur = current.create_view(&Default::default());
        let bindings = [
            wgpu::BindingResource::TextureView(&prev),
            wgpu::BindingResource::TextureView(&cur),
            self.tracks.as_entire_binding(),
            self.params.as_entire_binding(),
        ];
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera analysis"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &std::array::from_fn::<_, 4, _>(|i| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: bindings[i].clone(),
            }),
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(cols, rows, 1);
        }
        let len = size as u64 * 16;
        encoder.copy_buffer_to_buffer(&self.tracks, 0, &self.staging, 0, len);
        let submission = gpu.queue().submit([encoder.finish()]);
        let (send, recv) = std::sync::mpsc::channel();
        self.staging
            .map_async(wgpu::MapMode::Read, ..len, move |result| {
                let _ = send.send(result);
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|e| e.to_string())?;
        recv.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = self
            .staging
            .get_mapped_range(..len)
            .map_err(|e| e.to_string())?;
        ids.resize(size, 0);
        let mut out = Vec::with_capacity(size);
        for (i, chunk) in mapped.as_chunks::<16>().0.iter().enumerate() {
            let [x, y, score, new] = std::array::from_fn(|k| {
                f32::from_le_bytes(chunk[k * 4..k * 4 + 4].try_into().unwrap())
            });
            if score <= 0. {
                continue;
            }
            if new > 0. {
                ids[i] = frame
                    .checked_mul(size as u32)
                    .and_then(|v| v.checked_add(i as u32))
                    .ok_or("Too many camera frames")?;
            }
            out.push(Sample {
                id: ids[i],
                pixel: [
                    x + 0.5 - current.width() as f32 * 0.5,
                    y + 0.5 - current.height() as f32 * 0.5,
                ],
            });
        }
        drop(mapped);
        self.staging.unmap();
        Ok(out)
    }
}
