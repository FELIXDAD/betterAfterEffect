//! egui-wgpu on the shared [`GpuContext`], plus the viewport-texture bridge.

use std::collections::{HashMap, HashSet};

use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::{GpuContext, wgpu};

/// What one egui pass produced that the renderer needs.
pub struct UiOutput {
    /// Shapes to tessellate.
    pub shapes: Vec<egui::epaint::ClippedShape>,
    /// Font atlas / image updates.
    pub textures_delta: egui::TexturesDelta,
    /// Scale of the pass.
    pub pixels_per_point: f32,
}

/// Draws egui output into any texture view of the configured format, on the
/// device every other subsystem uses.
pub struct UiRenderer {
    renderer: egui_wgpu::Renderer,
    format: wgpu::TextureFormat,
    synced_generation: Option<u64>,
    viewports: HashMap<ViewportTextureId, (egui::TextureId, u64)>,
}

impl UiRenderer {
    /// Creates a renderer targeting `format`.
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let renderer = egui_wgpu::Renderer::new(
            gpu.device(),
            format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );
        Self {
            renderer,
            format,
            synced_generation: None,
            viewports: HashMap::new(),
        }
    }

    /// Target format.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Mirrors the [`ViewportTextureRegistry`] into egui-wgpu: newly published
    /// views are registered (zero-copy: egui samples the renderer's texture),
    /// republished ones re-bound, removed ones freed. Each entry's display handle
    /// is set to its egui `TextureId::User` value. Cheap when nothing changed.
    pub fn sync_viewports(&mut self, gpu: &GpuContext, registry: &ViewportTextureRegistry) {
        let generation = registry.generation();
        if self.synced_generation == Some(generation) {
            return;
        }
        let entries = registry.snapshot();
        let live: HashSet<ViewportTextureId> = entries
            .iter()
            .filter(|e| e.texture.is_some())
            .map(|e| e.id)
            .collect();
        let renderer = &mut self.renderer;
        self.viewports.retain(|id, (texture_id, _)| {
            let keep = live.contains(id);
            if !keep {
                renderer.free_texture(texture_id);
                registry.set_display_handle(*id, None);
            }
            keep
        });
        for entry in entries {
            let Some(texture) = entry.texture else {
                continue;
            };
            let texture_id = match self.viewports.get_mut(&entry.id) {
                Some((texture_id, synced)) => {
                    if *synced != texture.generation {
                        renderer.update_egui_texture_from_wgpu_texture(
                            gpu.device(),
                            &texture.view,
                            texture.filter,
                            *texture_id,
                        );
                        *synced = texture.generation;
                    }
                    *texture_id
                }
                None => {
                    let texture_id = renderer.register_native_texture(
                        gpu.device(),
                        &texture.view,
                        texture.filter,
                    );
                    self.viewports
                        .insert(entry.id, (texture_id, texture.generation));
                    texture_id
                }
            };
            if let egui::TextureId::User(handle) = texture_id
                && entry.display_handle != Some(handle)
            {
                registry.set_display_handle(entry.id, Some(handle));
            }
        }
        self.synced_generation = Some(generation);
    }

    /// Applies texture uploads and frees without drawing (a skipped frame must
    /// still keep egui's font atlas and images in sync).
    pub fn apply_textures(&mut self, gpu: &GpuContext, mut delta: egui::TexturesDelta) {
        self.upload_textures(gpu, &mut delta);
        self.free_textures(&mut delta);
    }

    fn upload_textures(&mut self, gpu: &GpuContext, delta: &mut egui::TexturesDelta) {
        // epaint asserts that every delta is consumed, so drain rather than iterate.
        for (id, images) in delta.set.drain() {
            for image in &images {
                self.renderer
                    .update_texture(gpu.device(), gpu.queue(), id, image);
            }
        }
    }

    fn free_textures(&mut self, delta: &mut egui::TexturesDelta) {
        for id in delta.free.drain() {
            self.renderer.free_texture(&id);
        }
    }

    /// Renders `output` into `target` (`size_in_pixels` must match it) and
    /// submits. `clear` is the background (gamma-space for non-sRGB targets).
    pub fn render(
        &mut self,
        gpu: &GpuContext,
        egui_ctx: &egui::Context,
        mut output: UiOutput,
        target: &wgpu::TextureView,
        size_in_pixels: [u32; 2],
        clear: egui::Color32,
    ) {
        let device = gpu.device();
        let queue = gpu.queue();
        self.upload_textures(gpu, &mut output.textures_delta);
        let mut paint_jobs = egui_ctx.tessellate(output.shapes, output.pixels_per_point);
        // Preferences > Appearance > Color Theme.
        bae_ui::theme::apply_appearance(&mut paint_jobs);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels,
            pixels_per_point: output.pixels_per_point,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae.ui"),
        });
        let mut commands =
            self.renderer
                .update_buffers(device, queue, &mut encoder, &paint_jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bae.ui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color(clear, self.format)),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &paint_jobs, &screen);
        }
        commands.push(encoder.finish());
        queue.submit(commands);
        self.free_textures(&mut output.textures_delta);
    }
}

fn clear_color(color: egui::Color32, format: wgpu::TextureFormat) -> wgpu::Color {
    if format.is_srgb() {
        let linear = egui::Rgba::from(color);
        wgpu::Color {
            r: f64::from(linear.r()),
            g: f64::from(linear.g()),
            b: f64::from(linear.b()),
            a: f64::from(linear.a()),
        }
    } else {
        let [r, g, b, a] = color.to_array();
        wgpu::Color {
            r: f64::from(r) / 255.0,
            g: f64::from(g) / 255.0,
            b: f64::from(b) / 255.0,
            a: f64::from(a) / 255.0,
        }
    }
}
