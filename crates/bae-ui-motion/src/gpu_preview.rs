//! The viewer's GPU preview: 04's compositor (`bae_render::Renderer`) renders the
//! evaluated comp, converts it to an 8-bit texture on the GPU and publishes it in
//! the shell's `ViewportTextureRegistry`; the viewer draws it with
//! `egui::TextureId::User`. Pixels never leave the GPU.
//!
//! The comp is converted from the viewer's own `EvaluatedComp` (with live drag
//! overrides applied), so dragging previews through the real compositor too.

use std::sync::Arc;

use bae_anim::EvaluatedComp;
use bae_app::{Registry, Resources};
use bae_gpu::GpuContext;
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::wgpu;
use bae_model::{Stage, Time};
use bae_render::{
    AnimEvaluator, IRect, OutputAlpha, OutputTarget, RenderPurpose, RenderRequest, Renderer,
    RendererConfig, Resolution, SourceProvider,
};

/// Layer source providers registered by other modules (05's shape layers, plugin
/// generators), shared with the viewer as an app resource.
#[derive(Clone)]
pub struct PreviewSources(pub Arc<Registry<dyn SourceProvider>>);

impl std::fmt::Debug for PreviewSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewSources").finish_non_exhaustive()
    }
}

/// Effect implementations registered by other modules (07's built-ins, plugin
/// effects), shared with the viewer and the render queue as an app resource.
#[derive(Clone)]
pub struct PreviewEffects(pub Arc<Registry<dyn bae_render::Effect>>);

impl std::fmt::Debug for PreviewEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewEffects").finish_non_exhaustive()
    }
}

/// What the viewer draws: an egui user texture covering `rect` (comp pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PreviewImage {
    pub handle: u64,
    pub rect: IRect,
}

pub(crate) struct GpuPreview {
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    renderer: Renderer,
    anim: AnimEvaluator,
    sources_gen: Option<u64>,
    /// `(content hash, time, resolution)` of the published frame.
    last: Option<(u64, Time, u32)>,
    rect: Option<IRect>,
    /// The published frame (the Info panel reads pixels from it) and the last
    /// pixel read: (texel, value).
    target: Option<OutputTarget>,
    sampled: Option<((u32, u32, u64), [u8; 4])>,
    /// Text layers have a source provider (otherwise the viewer draws them).
    pub has_text: bool,
}

impl GpuPreview {
    /// A preview on the app's GPU context, if the app has one (the headless test
    /// harness and agents do not).
    pub fn new(resources: &Resources) -> Option<Self> {
        let gpu = resources.get::<Arc<GpuContext>>()?.clone();
        let registry = resources.get::<Arc<ViewportTextureRegistry>>()?.clone();
        Some(Self::with(gpu, registry))
    }

    pub fn with(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        let id = registry.create("motion composition viewer");
        let renderer = Renderer::with_config(
            gpu.clone(),
            RendererConfig {
                frame_cache_bytes: 512 << 20,
                pool_idle_bytes: 128 << 20,
            },
        );
        GpuPreview {
            gpu,
            registry,
            id,
            renderer,
            anim: AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview),
            sources_gen: None,
            last: None,
            rect: None,
            target: None,
            sampled: None,
            has_text: false,
        }
    }

    /// Whether this preview still uses the app's current GPU context (the shell
    /// recreates it after a device loss).
    pub fn is_current(&self, resources: &Resources) -> bool {
        resources
            .get::<Arc<GpuContext>>()
            .is_some_and(|g| Arc::ptr_eq(g, &self.gpu))
            && !self.gpu.is_lost()
    }

    fn sync_sources(&mut self, resources: &Resources) {
        let Some(PreviewSources(registry)) = resources.get::<PreviewSources>() else {
            return;
        };
        let generation = registry.generation();
        if self.sources_gen == Some(generation) {
            return;
        }
        self.sources_gen = Some(generation);
        let providers = self.renderer.providers_mut();
        for entry in registry.snapshot().iter() {
            providers.register(entry.item().clone());
        }
        self.has_text = providers.get(bae_render::anim::TEXT_PROVIDER).is_some();
        self.last = None;
    }

    fn sync_effects(&mut self, resources: &Resources) {
        if let Some(PreviewEffects(registry)) = resources.get::<PreviewEffects>()
            && self.renderer.sync_effects(registry)
        {
            self.last = None;
        }
    }

    /// Render `comp` (if it changed) and return the texture to draw. `None` until
    /// the shell has registered the texture with egui (the frame after the first
    /// publish) or when rendering failed.
    pub fn frame(
        &mut self,
        resources: &Resources,
        stage: &Stage,
        comp: &EvaluatedComp,
        resolution: u32,
        camera: Option<bae_render::Camera>,
    ) -> Option<PreviewImage> {
        self.sync_sources(resources);
        self.sync_effects(resources);
        let mut rc = match self.anim.convert(stage, comp, 0) {
            Ok(rc) => rc,
            Err(e) => {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
        };
        // A 3D view other than Active Camera (the output never changes).
        if let Some(camera) = camera {
            rc.camera = Some(camera);
            if let Some(blur) = &mut rc.motion_blur {
                let blur = std::sync::Arc::make_mut(blur);
                blur.cameras.fill(Some(camera));
            }
        }
        let key = (rc.content_hash(), rc.time, resolution);
        if self.last != Some(key) {
            let request = RenderRequest {
                resolution: match resolution {
                    0 | 1 => Resolution::Full,
                    2 => Resolution::Half,
                    3 => Resolution::Third,
                    4 => Resolution::Quarter,
                    n => Resolution::Custom(n),
                },
                purpose: RenderPurpose::Preview,
                ..RenderRequest::full()
            };
            // Effects reading other frames evaluate them like this one.
            let mut frames = bae_render::eval::EvaluatorFrames {
                stage,
                evaluator: &mut self.anim,
            };
            let old_size = self.target.as_ref().map(OutputTarget::size);
            let (frame, target) = match self.renderer.render_frame_output(
                &rc,
                &request,
                Some(&mut frames),
                OutputAlpha::Premultiplied,
                self.target.take(),
            ) {
                Ok(result) => result,
                Err(e) => {
                    tracing_warn(&format!("preview: {e}"));
                    return None;
                }
            };
            // Display-encoded 8-bit values, sampled as they are (egui blends in
            // gamma space, see docs/spec/app-shell.md §5).
            // Egui keeps the same view until the render dimensions change.
            if old_size != Some(target.size())
                && let Err(e) = self.registry.publish_view(
                    self.id,
                    target.view().clone(),
                    target.size(),
                    target.view().texture().format(),
                    wgpu::FilterMode::Linear,
                )
            {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
            self.last = Some(key);
            self.rect = Some(frame.image.rect);
            self.target = Some(target);
            self.sampled = None;
        }
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect?,
        })
    }

    /// Straight (unpremultiplied) 8-bit RGBA of the published frame at comp
    /// point `p`, as shown (display-encoded); `None` outside the frame. Reads
    /// one texel back from the GPU (blocking, only when the pointer moved to
    /// another texel or the frame changed).
    pub fn pixel(&mut self, p: [f64; 2]) -> Option<[u8; 4]> {
        let (texture, rect) = (self.target.as_ref()?.view().texture(), self.rect?);
        let (w, h) = (texture.width(), texture.height());
        let (rw, rh) = (f64::from(rect.x1 - rect.x0), f64::from(rect.y1 - rect.y0));
        if rw <= 0.0 || rh <= 0.0 {
            return None;
        }
        let u = (p[0] - f64::from(rect.x0)) / rw;
        let v = (p[1] - f64::from(rect.y0)) / rh;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let texel = (
            ((u * f64::from(w)) as u32).min(w - 1),
            ((v * f64::from(h)) as u32).min(h - 1),
            self.last.map_or(0, |k| k.0 ^ u64::from(k.2)),
        );
        if let Some((t, c)) = self.sampled
            && t == texel
        {
            return Some(c);
        }
        let region = bae_gpu::transfer::TextureRegion {
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: texel.0,
                y: texel.1,
                z: 0,
            },
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        };
        let mut encoder =
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("bae-ui-motion.info-pixel"),
                });
        let readback =
            bae_gpu::transfer::TextureReadback::record(&self.gpu, &mut encoder, texture, region)
                .ok()?;
        self.gpu.queue().submit([encoder.finish()]);
        let image = readback.map().wait(&self.gpu).ok()?;
        let px = image.pixel(0, 0);
        let a = px[3];
        let straight = |c: u8| {
            if a == 0 {
                0
            } else {
                ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
            }
        };
        let c = [straight(px[0]), straight(px[1]), straight(px[2]), a];
        self.sampled = Some((texel, c));
        Some(c)
    }

    #[cfg(test)]
    pub fn renderer(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    #[cfg(test)]
    pub fn viewport(&self) -> ViewportTextureId {
        self.id
    }
}

impl Drop for GpuPreview {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

fn tracing_warn(message: &str) {
    // Once per distinct message: the viewer calls this every frame.
    thread_local! {
        static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    LAST.with(|l| {
        if *l.borrow() != message {
            tracing::warn!("{message}");
            *l.borrow_mut() = message.to_owned();
        }
    });
}
