//! Executes a [`RenderGraph`] on the shared device: one command encoder per
//! frame, nodes in graph order, intermediates freed as soon as their last
//! consumer is encoded (their pooled textures are reused later in the frame).
//!
//! Composites batch consecutive plain solids into one instanced draw, keep
//! Normal / Add / Screen on fixed-function blending, and for backdrop-reading
//! modes (Multiply, Overlay, preserve transparency, clamped Add) end the pass,
//! copy the layer's destination bounds into a scratch texture and continue.
//!
//! 3D layers draw with homographies (perspective-correct, clipped at the
//! camera's near plane) and motion-blurred layers with a list of sample
//! placements the shader averages per pixel (see `gpu/composite.wgsl`); both
//! stay one draw per layer.
//!
//! Mask nodes run their own passes (`gpu/mask.wgsl`, see [`crate::mask`]):
//! coverage accumulation, resolve + horizontal feather, vertical feather, and –
//! with several masks – one accumulator pass merging them with blend states.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use glam::{DAffine2, DVec2};

use crate::blend::BlendMode;
use crate::error::RenderError;
use crate::geom::IRect;
use crate::gpu::{BlendKind, DrawData, Pipelines, SampleData, Variant, flags};
use crate::graph::{CompositeNode, LayerOp, Node, NodeId, NodeKind, OpInput, RenderGraph};
use rustc_hash::FxHashMap;

pub(crate) mod adjust;
pub(crate) mod dof;
mod masks;
pub(crate) mod mesh;
mod shadow;
pub(crate) mod shadowmap;
use crate::pool::TexturePool;
use crate::source::{EncodeContext, GpuImage, ImageAllocator, ImageAlpha};

/// A composite's shadow atlas and where each image caster sits in it.
type ShadowAtlas = (GpuImage, FxHashMap<NodeId, [u32; 2]>);

/// Per-renderer buffers of draw records and motion-blur samples (grown on
/// demand, reused across frames) and their bind group.
#[derive(Default)]
pub(crate) struct DrawBuffer {
    buffer: Option<DrawBuffers>,
}

struct DrawBuffers {
    draws: wgpu::Buffer,
    samples: wgpu::Buffer,
    lighting: wgpu::Buffer,
    group: wgpu::BindGroup,
    /// Bytes of each buffer.
    capacity: [u64; 3],
}

impl DrawBuffer {
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        draws: u64,
        samples: u64,
        lighting: u64,
    ) {
        let needed = [
            draws.max(64) * std::mem::size_of::<DrawData>() as u64,
            samples.max(16) * std::mem::size_of::<SampleData>() as u64,
            lighting.max(16) * 16,
        ];
        if self
            .buffer
            .as_ref()
            .is_some_and(|b| b.capacity.iter().zip(&needed).all(|(c, n)| c >= n))
        {
            return;
        }
        let create = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let capacity = needed.map(u64::next_power_of_two);
        let draws = create("bae-render.draws", capacity[0]);
        let samples = create("bae-render.samples", capacity[1]);
        let lighting = create("bae-render.lighting", capacity[2]);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.draws"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: draws.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: samples.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lighting.as_entire_binding(),
                },
            ],
        });
        self.buffer = Some(DrawBuffers {
            draws,
            samples,
            lighting,
            group,
            capacity,
        });
    }
}

/// A storage buffer grown on demand and reused across frames.
#[derive(Default)]
struct GrowBuffer {
    buffer: Option<(wgpu::Buffer, u64)>,
}

impl GrowBuffer {
    fn ensure(&mut self, device: &wgpu::Device, label: &'static str, bytes: u64) -> &wgpu::Buffer {
        let needed = bytes.max(256);
        if self.buffer.as_ref().is_none_or(|(_, cap)| *cap < needed) {
            let capacity = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.buffer = Some((buffer, capacity));
        }
        &self.buffer.as_ref().expect("ensured").0
    }
}

/// Per-renderer buffers of mask edges and mask pass records.
#[derive(Default)]
pub(crate) struct MaskBuffers {
    edges: GrowBuffer,
    passes: GrowBuffer,
}

/// What executing a graph produced.
pub(crate) struct Executed {
    pub image: GpuImage,
    pub draw_calls: usize,
    pub passes: usize,
    /// Something was still warming up and is missing from the image.
    pub incomplete: bool,
    /// CPU time encoding each node (by node index), when timed.
    pub node_times: Vec<std::time::Duration>,
    /// The frame converted into the requested RGBA8 target (when asked for),
    /// recorded in the frame's own submission.
    pub output: Option<crate::OutputTarget>,
}

struct PoolAllocator<'a> {
    pool: &'a mut TexturePool,
    device: &'a wgpu::Device,
}

impl ImageAllocator for PoolAllocator<'_> {
    fn allocate(
        &mut self,
        rect: IRect,
        pixel_scale: f64,
        format: wgpu::TextureFormat,
    ) -> Result<GpuImage, RenderError> {
        self.pool.acquire(self.device, format, rect, pixel_scale)
    }
}

/// Share whole-surface effect outputs between ROI requests in the renderer's
/// existing byte-limited frame cache. Entries are published after submission.
pub(crate) struct TileCache<'a> {
    pub feedback: bool,
    pub frames: &'a mut crate::cache::FrameCache<GpuImage>,
    pub request: u64,
    pub comp: &'a crate::RenderComp,
}

/// Per-node timing of a frame (the Render Time column).
pub(crate) struct Timing<'a> {
    /// CPU time per node.
    pub cpu: bool,
    /// GPU timestamps per node, when the device supports them.
    pub gpu: Option<&'a mut crate::gpu_timer::GpuTimer>,
}

/// Executes `graph` and submits it once. Returns the output node's image,
/// with `output` its conversion into the requested RGBA8 target recorded before the
/// same submission (the viewer path: one submit per frame), and, when
/// `timed`, the CPU time of every node.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute(
    gpu: &Arc<GpuContext>,
    pipelines: &mut Pipelines,
    pool: &mut TexturePool,
    draw_buffer: &mut DrawBuffer,
    mask_buffers: &mut MaskBuffers,
    graph: &RenderGraph,
    format: wgpu::TextureFormat,
    mut tiles: Option<TileCache<'_>>,
    shadow_maps: &mut shadowmap::ShadowMapCache,
    timing: Timing<'_>,
    output: Option<crate::renderer::DisplayOutput>,
) -> Result<Executed, RenderError> {
    let timed = timing.cpu;
    let mut gpu_timer = timing.gpu;
    let gpu_timed = gpu_timer
        .as_deref_mut()
        .is_some_and(|t| t.begin(graph.nodes.len()));
    let mut surfaces = Vec::new();
    let device = gpu.device();
    let total_draws: usize = graph
        .nodes
        .iter()
        .map(|n| match &n.kind {
            // Layer draws plus the shadow atlas copies.
            NodeKind::Composite(c) => c.ops.len() + shadow::casters(c, |_| false).len(),
            NodeKind::Fill(_) => 1,
            _ => 0,
        })
        .sum();
    let total_samples: usize = graph
        .nodes
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Composite(c) => c
                .ops
                .iter()
                .filter_map(|op| op.motion.as_ref())
                .map(|m| m.len())
                .sum(),
            _ => 0,
        })
        .sum();
    let total_lighting: usize = graph
        .nodes
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Composite(c) => c
                .ops
                .iter()
                .filter_map(|op| op.lighting.as_ref())
                .map(|l| l.record_len())
                .sum(),
            _ => 0,
        })
        .sum();
    draw_buffer.ensure(
        device,
        &pipelines.draws_layout,
        total_draws as u64,
        total_samples as u64,
        total_lighting as u64,
    );
    let buffers = draw_buffer.buffer.as_ref().expect("ensured");
    let blendable = format != wgpu::TextureFormat::Rgba32Float
        || gpu.features().contains(wgpu::Features::FLOAT32_BLENDABLE);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("bae-render.frame"),
    });
    let mut uses = graph.use_counts();
    let mut outputs: Vec<Option<GpuImage>> = vec![None; graph.nodes.len()];
    let mut ctx = ExecCtx {
        gpu,
        pipelines,
        buffer: &buffers.draws,
        samples_buffer: &buffers.samples,
        lighting_buffer: &buffers.lighting,
        draws_group: &buffers.group,
        next_draw: 0,
        next_sample: 0,
        next_lighting: 0,
        depth: graph.depth,
        format,
        blendable,
        draw_calls: 0,
        passes: 0,
        mask_outs: Vec::new(),
        incomplete: false,
        skipped_maps: Default::default(),
    };
    // Every mask of the frame at once, in shared atlases (they have no inputs).
    ctx.mask_outs = masks::encode(&mut ctx, &mut encoder, pool, mask_buffers, graph)?;

    let mut incomplete = false;
    let mut node_times = Vec::with_capacity(if timed { graph.nodes.len() } else { 0 });
    for (index, node) in graph.nodes.iter().enumerate() {
        let started = timed.then(std::time::Instant::now);
        if gpu_timed && let Some(t) = gpu_timer.as_deref() {
            t.write(&mut encoder, index);
        }
        let mut surface_key = tiles.as_ref().and_then(|cache| match &node.kind {
            NodeKind::Effect(fx)
                if fx.feedback.is_some() || (!cache.feedback && fx.effect.full_surface()) =>
            {
                Some(crate::cache::FrameKey {
                    comp: fx.instance.path.clone(),
                    time: fx.frame.comp_time,
                    request: cache.request,
                })
            }
            _ => None,
        });
        // A feedback pass depends on cache residency of the preceding frame.
        // Re-encode one pass; never walk or regenerate the history.
        let cached = surface_key
            .as_ref()
            .filter(|_| tiles.as_ref().is_some_and(|cache| !cache.feedback))
            .and_then(|key| {
                let frames = &mut tiles.as_mut()?.frames;
                (frames.content_hash(key) == Some(node.key))
                    .then(|| frames.get(key).cloned())
                    .flatten()
            });
        let image = match (&node.kind, cached) {
            (_, Some(image)) => {
                surface_key = None;
                image
            }
            (NodeKind::Composite(c), None) => {
                let px = node_pixels(node);
                let target = pool.acquire(device, format, px, node.pixel_scale)?;
                let scratch_needed = c.ops.iter().any(|op| ctx.needs_backdrop(c, op));
                let scratch = if scratch_needed {
                    Some(pool.acquire(device, format, px, node.pixel_scale)?)
                } else {
                    None
                };
                // Image layers casting shadows here, copied into one atlas.
                let casters = shadow::casters(c, |map| ctx.skipped_maps.contains(&map));
                let atlas = if casters.is_empty() {
                    None
                } else {
                    let (slots, size) = shadow::layout(&casters, &outputs);
                    let atlas = pool.acquire(device, format, shadow::atlas_rect(size), 1.0)?;
                    ctx.blit_atlas(&mut encoder, &atlas, &slots, &outputs)?;
                    Some((atlas, slots))
                };
                ctx.composite(
                    &mut encoder,
                    node,
                    c,
                    &target,
                    scratch.as_ref(),
                    &outputs,
                    atlas.as_ref(),
                )?;
                target
            }
            (NodeKind::Fill(fill), None) => {
                let target = pool.acquire(device, format, node_pixels(node), node.pixel_scale)?;
                ctx.fill(&mut encoder, node, fill.color, &fill.rect, &target)?;
                target
            }
            (NodeKind::Source(src), None) => {
                let mut alloc = PoolAllocator { pool, device };
                let mut ectx = EncodeContext {
                    gpu,
                    encoder: &mut encoder,
                    format,
                    allocator: &mut alloc,
                    effect_inputs: &[],
                    effect_masks: &[],
                };
                match src.source.render(&mut ectx, &src.request) {
                    Ok(image) => image,
                    // Not ready yet: nothing this frame (drawn again later).
                    Err(e) if e.is_warming_up() => {
                        incomplete = true;
                        let empty = pool.acquire(device, format, IRect::from_size(1, 1), 1.0)?;
                        clear(&mut encoder, &empty.view);
                        empty
                    }
                    Err(e) => return Err(e),
                }
            }
            (NodeKind::Effect(fx), None) => {
                let input = outputs[fx.input.index()]
                    .clone()
                    .ok_or_else(|| RenderError::Other("effect input missing".into()))?;
                let effect_inputs: Vec<_> = fx
                    .layers
                    .iter()
                    .filter_map(|(dep, id)| Some((dep.clone(), outputs[id.index()].clone()?)))
                    .collect();
                let effect_masks: Vec<_> = fx
                    .masks
                    .iter()
                    .filter_map(|(m, id)| Some((m.clone(), outputs[id.index()].clone()?)))
                    .collect();
                let mut params = fx.frame.params(&fx.instance);
                let previous = fx.feedback.as_ref().and_then(|path| {
                    let cache = tiles.as_mut()?;
                    let time = fx.frame.comp_time - fx.frame.time_step;
                    if time < fx.frame.layer_in {
                        return None;
                    }
                    cache
                        .frames
                        .get(&crate::FrameKey {
                            comp: path.clone(),
                            time,
                            request: cache.request,
                        })
                        .cloned()
                });
                params.previous_frame = previous.as_ref();
                params.scene = fx.scene.as_deref();
                params.sixteen_bpc = graph.depth == crate::BitDepth::Bpc16;
                if let Some(samples) = &fx.samples {
                    params.parameter_samples = &samples.parameter_samples;
                    params.tracks = &samples.tracks;
                    params.mask_paths = &samples.paths;
                    params.history = &samples.history;
                }
                let mut alloc = PoolAllocator { pool, device };
                let mut ectx = EncodeContext {
                    gpu,
                    encoder: &mut encoder,
                    format,
                    allocator: &mut alloc,
                    effect_inputs: &effect_inputs,
                    effect_masks: &effect_masks,
                };
                match fx.effect.render(&mut ectx, &params, &input, node.rect) {
                    Ok(image) => image,
                    // Pipeline not compiled yet: the layer without this effect
                    // this frame (drawn again when it is ready).
                    Err(e) if e.is_warming_up() => {
                        incomplete = true;
                        input
                    }
                    Err(e) => return Err(e),
                }
            }
            (NodeKind::Mix(mix), None) => {
                let input = |id: crate::graph::NodeId| {
                    outputs[id.index()]
                        .clone()
                        .ok_or_else(|| RenderError::Other("mix input missing".into()))
                };
                let images = [input(mix.base)?, input(mix.over)?, input(mix.coverage)?];
                let target = pool.acquire(device, format, node_pixels(node), node.pixel_scale)?;
                let mix = ctx.pipelines.mix();
                adjust::encode(
                    &mut mix.lock(),
                    device,
                    gpu.queue(),
                    &mut encoder,
                    format,
                    &target,
                    [&images[0], &images[1], &images[2]],
                );
                ctx.passes += 1;
                target
            }
            (NodeKind::Mesh(mesh), None) => {
                let target = pool.acquire(device, format, node_pixels(node), node.pixel_scale)?;
                let pipelines = ctx.pipelines.mesh();
                ctx.draw_calls += mesh::encode(
                    &mut pipelines.lock(),
                    device,
                    gpu.queue(),
                    &mut encoder,
                    format,
                    mesh,
                    &target,
                );
                ctx.passes += 1;
                target
            }
            (NodeKind::ShadowMap(map), None) => match shadow_maps.get(node.key) {
                Some(image) => image,
                // A new map (the layer, its lights or casters changed) on a
                // layer covering fewer output pixels than a map slot has
                // texels: this frame tests the casters per pixel, and the map
                // is drawn once the same key comes again (animated shadows
                // pay for maps only where they are cheaper, such as at 4K).
                None if !shadow_maps.stable(&map.layer, node.key)
                    && map.screen_texels <= {
                        let (_, [w, h]) = map.lighting.shadow_map_size();
                        u64::from(w) * u64::from(h)
                    } =>
                {
                    ctx.skipped_maps.insert(NodeId(index as u32));
                    pool.acquire_texels(
                        device,
                        shadowmap::FORMAT,
                        IRect::from_size(1, 1),
                        1.0,
                        [1, 1],
                    )?
                }
                None => {
                    let l = &map.lighting;
                    // Slots whose light did not change come from the cache.
                    let reuse: Vec<Option<(GpuImage, u32)>> =
                        map.slot_keys.iter().map(|k| shadow_maps.slot(*k)).collect();
                    let draw: Vec<bool> = reuse.iter().map(Option::is_none).collect();
                    let casters: Vec<NodeId> = l
                        .casters
                        .iter()
                        .filter_map(|c| match c.pixels {
                            crate::light::CasterPixels::Image(id) => Some(id),
                            crate::light::CasterPixels::Solid(_) => None,
                        })
                        .collect();
                    let atlas = if casters.is_empty() || !draw.contains(&true) {
                        None
                    } else {
                        let (slots, size) = shadow::layout(&casters, &outputs);
                        let atlas = pool.acquire(device, format, shadow::atlas_rect(size), 1.0)?;
                        ctx.blit_atlas(&mut encoder, &atlas, &slots, &outputs)?;
                        Some((atlas, slots))
                    };
                    let uv = |id: NodeId| {
                        let (image, slots) = atlas.as_ref()?;
                        let img = outputs[id.index()].as_ref()?;
                        Some(shadow::uv_map(img, *slots.get(&id)?, image.texture_size()))
                    };
                    // Packed in layer pixels; the hardest edge a layer pixel or
                    // a map texel wide (the map does not depend on the camera).
                    let mut record = Vec::new();
                    l.pack(&glam::DAffine2::IDENTITY, 1.0, &uv, false, &mut record);
                    let (k, [w, h]) = l.shadow_map_size();
                    let edge = k.max(1.0) * l.matrix.x_axis.truncate().length();
                    record[4][3] = -(edge as f32);
                    let slots = l.shadow_slots();
                    let mut lights = [0u32; 8];
                    for (slot, &li) in slots.iter().enumerate() {
                        lights[slot] = li as u32;
                    }
                    let n = slots.len() as u32;
                    let layout = shadowmap::MapLayout {
                        rect: [l.rect.x0, l.rect.y0, l.rect.x1, l.rect.y1].map(|v| v as f32),
                        slot: [w, h],
                        lights,
                        count: n,
                    };
                    let target = pool.acquire_texels(
                        device,
                        shadowmap::FORMAT,
                        IRect::new(0, 0, (w * n) as i32, h as i32),
                        1.0,
                        [w * n, h],
                    )?;
                    let pipeline = ctx.pipelines.shadow_map();
                    shadowmap::encode(
                        &mut pipeline.lock(),
                        device,
                        gpu.queue(),
                        &mut encoder,
                        &target,
                        &layout,
                        &record,
                        atlas.as_ref().map(|(a, _)| &a.view),
                        &draw,
                    );
                    ctx.passes += 1;
                    for (slot, cached) in reuse.iter().enumerate() {
                        let Some((from, from_slot)) = cached else {
                            continue;
                        };
                        let at = |texture, slot: u32| wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: slot * w,
                                y: 0,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        };
                        encoder.copy_texture_to_texture(
                            at(&from.texture, *from_slot),
                            at(&target.texture, slot as u32),
                            wgpu::Extent3d {
                                width: w,
                                height: h,
                                depth_or_array_layers: 1,
                            },
                        );
                    }
                    shadow_maps.insert(node.key, target.clone(), &map.slot_keys);
                    target
                }
            },
            (NodeKind::DepthBlur(blur), None) => {
                let input = outputs[blur.input.index()]
                    .clone()
                    .ok_or_else(|| RenderError::Other("depth of field input missing".into()))?;
                let target = pool.acquire(device, format, node_pixels(node), node.pixel_scale)?;
                let dof = ctx.pipelines.dof();
                ctx.passes += dof::encode(
                    &mut dof.lock(),
                    device,
                    gpu.queue(),
                    &mut encoder,
                    pool,
                    format,
                    blur,
                    &input,
                    &target,
                )?;
                target
            }
            (NodeKind::Mask(_), None) => ctx.mask_outs[index]
                .as_ref()
                .map(|m| m.image.clone())
                .ok_or_else(|| RenderError::Other("mask not encoded".into()))?,
        };
        if let Some(key) = surface_key {
            surfaces.push((key, node.key, image.clone()));
        }
        outputs[index] = Some(image);
        if let Some(started) = started {
            node_times.push(started.elapsed());
        }
        // Free inputs whose last consumer this was.
        node.kind.for_each_input(|id| {
            let u = &mut uses[id.index()];
            *u = u.saturating_sub(1);
            if *u == 0 {
                outputs[id.index()] = None;
            }
        });
    }
    let draw_calls = ctx.draw_calls;
    let passes = ctx.passes;
    let incomplete = incomplete || ctx.incomplete;
    if gpu_timed && let Some(t) = gpu_timer.as_deref_mut() {
        t.write(&mut encoder, graph.nodes.len());
        t.finish(&mut encoder, graph.nodes.len() + 1, &graph.owners);
    }
    let image = outputs[graph.output.index()]
        .take()
        .ok_or_else(|| RenderError::Other("graph produced no output".into()))?;
    let output = output.map(|out| {
        let incomplete = graph.incomplete || incomplete;
        out.record(device, ctx.pipelines, &mut encoder, &image, incomplete)
    });
    gpu.queue().submit([encoder.finish()]);
    ctx.pipelines.end_frame();
    if gpu_timed && let Some(t) = gpu_timer {
        t.submitted();
    }
    if !graph.incomplete
        && !incomplete
        && let Some(cache) = tiles
    {
        for (key, hash, image) in surfaces {
            let bytes = image.byte_size();
            cache.frames.insert_surface(
                key,
                image,
                bytes,
                cache.comp.dependencies(),
                hash,
                cache.feedback,
            );
        }
    }
    Ok(Executed {
        image,
        draw_calls,
        passes,
        incomplete,
        node_times,
        output,
    })
}

struct ExecCtx<'a> {
    gpu: &'a Arc<GpuContext>,
    pipelines: &'a mut Pipelines,
    buffer: &'a wgpu::Buffer,
    samples_buffer: &'a wgpu::Buffer,
    lighting_buffer: &'a wgpu::Buffer,
    draws_group: &'a wgpu::BindGroup,
    next_draw: u32,
    next_sample: u32,
    next_lighting: u32,
    depth: crate::request::BitDepth,
    format: wgpu::TextureFormat,
    blendable: bool,
    draw_calls: usize,
    passes: usize,
    /// Where each mask node's output lives (by node index).
    mask_outs: Vec<Option<masks::MaskOut>>,
    /// A pipeline was still compiling for the UI thread (incomplete frame).
    incomplete: bool,
    /// Shadow maps not drawn this frame: their layers test the casters.
    skipped_maps: rustc_hash::FxHashSet<NodeId>,
}

/// A planned draw of one op.
struct Planned {
    data: DrawData,
    kind: BlendKind,
    /// `None`: the shared empty texture group (batchable).
    textures: Option<wgpu::BindGroup>,
    /// Scissor (target texels), and whether the backdrop must be copied first.
    bounds: IRect,
    backdrop: bool,
    /// Fragment entry point (plain, lit, motion-blurred).
    variant: Variant,
}

impl ExecCtx<'_> {
    fn needs_backdrop(&self, c: &CompositeNode, op: &LayerOp) -> bool {
        BlendKind::for_mode(op.blend, c.clamp, op.preserve_transparency, self.blendable)
            == BlendKind::Replace
    }

    fn write_draws(&mut self, draws: &[DrawData]) -> u32 {
        let first = self.next_draw;
        if !draws.is_empty() {
            let offset = u64::from(first) * std::mem::size_of::<DrawData>() as u64;
            self.gpu
                .queue()
                .write_buffer(self.buffer, offset, bytemuck::cast_slice(draws));
            self.next_draw += draws.len() as u32;
        }
        first
    }

    #[allow(clippy::too_many_arguments)]
    fn composite(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        node: &Node,
        c: &CompositeNode,
        target: &GpuImage,
        scratch: Option<&GpuImage>,
        outputs: &[Option<GpuImage>],
        atlas: Option<&ShadowAtlas>,
    ) -> Result<(), RenderError> {
        let device = self.gpu.device();
        let tsize = target.texture_size();
        let mut planned: Vec<Planned> = Vec::with_capacity(c.ops.len());
        let mut samples: Vec<SampleData> = Vec::new();
        let mut lighting: Vec<[f32; 4]> = Vec::new();
        let mut drawn_any = false;
        for op in &c.ops {
            let mut kind =
                BlendKind::for_mode(op.blend, c.clamp, op.preserve_transparency, self.blendable);
            let mut backdrop = kind == BlendKind::Replace;
            if backdrop && !drawn_any && self.blendable && op.blend.source_over_empty() {
                // These modes reduce to Normal on a transparent backdrop, and
                // preserve-transparency shows nothing.
                if op.preserve_transparency {
                    continue;
                }
                kind = BlendKind::Normal;
                backdrop = false;
            }
            let Some(mut data) = plan_op(
                node,
                op,
                outputs,
                &self.mask_outs,
                tsize,
                &mut samples,
                &mut lighting,
                atlas,
                &self.skipped_maps,
            )?
            else {
                continue;
            };
            if op.motion.is_some() {
                data.extra[1] += self.next_sample;
            }
            if op.lighting.is_some() {
                data.extra[3] += self.next_lighting;
            }
            let mut fl = data.mode[1];
            if backdrop {
                fl |= flags::DST;
            }
            if op.preserve_transparency {
                fl |= flags::PRESERVE;
            }
            if c.clamp {
                fl |= flags::CLAMP;
            }
            data.mode[0] = op.blend.index();
            data.mode[1] = fl;
            data.mode[3] = op.dissolve_seed;
            data.target[3] = match self.depth {
                crate::request::BitDepth::Bpc8 => 255.0,
                crate::request::BitDepth::Bpc16 => 32768.0,
                crate::request::BitDepth::Float32 => 0.0,
            };
            data.fwd_x[3] = node.rect.x0 as f32;
            data.fwd_y[3] = node.rect.y0 as f32;
            let src_view = match op.input {
                OpInput::Image(id) => outputs[id.index()].as_ref().map(|i| &i.view),
                OpInput::Solid { .. } => None,
            };
            let matte_view = op
                .matte
                .and_then(|m| outputs[m.node.index()].as_ref().map(|i| &i.view));
            let mask_view = op
                .mask
                .and_then(|m| outputs[m.node.index()].as_ref().map(|i| &i.view));
            let dst_view = if backdrop {
                scratch.map(|s| &s.view)
            } else {
                None
            };
            // Lit layers with a shadow map read it; others with image shadow
            // casters read the shadow atlas.
            let shadow_view = match op
                .lighting
                .as_ref()
                .and_then(|l| l.shadow_map)
                .filter(|id| !self.skipped_maps.contains(id))
            {
                Some(id) => outputs[id.index()].as_ref().map(|i| &i.view),
                None => atlas
                    .filter(|_| op.lighting.as_ref().is_some_and(|l| !l.casters.is_empty()))
                    .map(|(image, _)| &image.view),
            };
            let textures = if src_view.is_none()
                && matte_view.is_none()
                && dst_view.is_none()
                && mask_view.is_none()
                && shadow_view.is_none()
            {
                None
            } else {
                Some(self.pipelines.textures(
                    device,
                    src_view,
                    matte_view,
                    dst_view,
                    mask_view,
                    shadow_view,
                ))
            };
            let bounds = if op.blend.is_stencil() {
                IRect::from_size(tsize[0], tsize[1])
            } else {
                IRect::new(
                    op.bounds.x0 - node.rect.x0,
                    op.bounds.y0 - node.rect.y0,
                    op.bounds.x1 - node.rect.x0,
                    op.bounds.y1 - node.rect.y0,
                )
                .intersect(&IRect::from_size(tsize[0], tsize[1]))
            };
            if bounds.is_empty() {
                continue;
            }
            let variant = if op.motion.is_some() {
                Variant::Motion
            } else if op.lighting.is_some() {
                Variant::Lit
            } else {
                Variant::Plain
            };
            // Metal can supply the affine image parameters from the vertex
            // stage rather than fetching the full draw record for each fragment.
            #[cfg(target_os = "macos")]
            let variant = if self.gpu.adapter_info().backend == wgpu::Backend::Metal
                && variant == Variant::Plain
                && kind == BlendKind::Normal
                && fl & !(flags::STRAIGHT | flags::CLAMP) == flags::TEXTURE
                && data.extra[0] == 0
            {
                // Batching uses one hardware scissor. Keep each instance's
                // original region in the unused matte slot.
                data.matte = [bounds.x0, bounds.y0, bounds.x1, bounds.y1].map(|v| v as f32);
                Variant::AffineImage
            } else {
                variant
            };
            planned.push(Planned {
                data,
                kind,
                textures,
                bounds,
                backdrop,
                variant,
            });
            drawn_any = true;
        }
        let datas: Vec<DrawData> = planned.iter().map(|p| p.data).collect();
        let first = self.write_draws(&datas);
        if !samples.is_empty() {
            let offset = u64::from(self.next_sample) * std::mem::size_of::<SampleData>() as u64;
            self.gpu.queue().write_buffer(
                self.samples_buffer,
                offset,
                bytemuck::cast_slice(&samples),
            );
            self.next_sample += samples.len() as u32;
        }
        if !lighting.is_empty() {
            self.gpu.queue().write_buffer(
                self.lighting_buffer,
                u64::from(self.next_lighting) * 16,
                bytemuck::cast_slice(&lighting),
            );
            self.next_lighting += lighting.len() as u32;
        }

        let format = self.format;
        let mut i = 0;
        let mut load = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        loop {
            if i < planned.len() && planned[i].backdrop {
                if matches!(load, wgpu::LoadOp::Clear(_)) {
                    // Initialise the target before copying its backdrop.
                    clear(encoder, &target.view);
                    self.passes += 1;
                    load = wgpu::LoadOp::Load;
                }
                let scratch =
                    scratch.ok_or_else(|| RenderError::Other("missing scratch texture".into()))?;
                let b = planned[i].bounds;
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &target.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: b.x0 as u32,
                            y: b.y0 as u32,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &scratch.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: b.x0 as u32,
                            y: b.y0 as u32,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: b.width(),
                        height: b.height(),
                        depth_or_array_layers: 1,
                    },
                );
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bae-render.composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.passes += 1;
            load = wgpu::LoadOp::Load;
            pass.set_bind_group(0, self.draws_group, &[]);
            let mut current = None;
            let start = i;
            while i < planned.len() {
                let p = &planned[i];
                if i > start && p.backdrop {
                    break;
                }
                if current != Some((p.kind, p.variant)) {
                    let Some(pipeline) =
                        self.pipelines.composite(device, format, p.kind, p.variant)
                    else {
                        // Still compiling for the UI thread: incomplete frame.
                        self.incomplete = true;
                        current = None;
                        i += 1;
                        continue;
                    };
                    pass.set_pipeline(pipeline);
                    current = Some((p.kind, p.variant));
                }
                // Adjacent affine images can instance the same bound texture
                // in painter's order, just as solids share the dummy texture.
                let batchable = p.textures.is_none();
                #[cfg(target_os = "macos")]
                let batchable = batchable || p.variant == Variant::AffineImage;
                let same_textures = |next: &Planned| {
                    #[cfg(target_os = "macos")]
                    if p.variant == Variant::AffineImage {
                        return next.textures == p.textures;
                    }
                    next.textures.is_none()
                };
                let mut end = i + 1;
                if batchable && !p.backdrop {
                    while end < planned.len()
                        && same_textures(&planned[end])
                        && !planned[end].backdrop
                        && planned[end].kind == p.kind
                        && planned[end].variant == p.variant
                    {
                        end += 1;
                    }
                }
                match &p.textures {
                    Some(group) => pass.set_bind_group(1, group, &[]),
                    None => pass.set_bind_group(1, &self.pipelines.empty_textures, &[]),
                }
                let scissor = if end == i + 1 {
                    p.bounds
                } else {
                    IRect::from_size(tsize[0], tsize[1])
                };
                pass.set_scissor_rect(
                    scissor.x0 as u32,
                    scissor.y0 as u32,
                    scissor.width(),
                    scissor.height(),
                );
                pass.draw(0..6, first + i as u32..first + end as u32);
                self.draw_calls += 1;
                i = end;
            }
            drop(pass);
            if i >= planned.len() {
                break;
            }
        }
        Ok(())
    }

    fn fill(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        node: &Node,
        color: [f32; 4],
        rect: &crate::geom::Rect,
        target: &GpuImage,
    ) -> Result<(), RenderError> {
        let device = self.gpu.device();
        let tsize = target.texture_size();
        let to_target = DAffine2::from_translation(DVec2::new(
            -f64::from(node.rect.x0),
            -f64::from(node.rect.y0),
        )) * DAffine2::from_scale(DVec2::splat(1.0 / node.pixel_scale));
        let mut data = DrawData::geometry(&to_target, [rect.x0, rect.y0, rect.x1, rect.y1], tsize)
            .ok_or_else(|| RenderError::Other("degenerate fill".into()))?;
        data.color = color;
        data.mode = [BlendMode::Normal.index(), 0, 0, 0];
        let first = self.write_draws(&[data]);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bae-render.fill"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.passes += 1;
        let kind = if self.blendable {
            BlendKind::Normal
        } else {
            BlendKind::Replace
        };
        let Some(pipeline) = self
            .pipelines
            .composite(device, self.format, kind, Variant::Plain)
        else {
            self.incomplete = true;
            return Ok(());
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, self.draws_group, &[]);
        pass.set_bind_group(1, &self.pipelines.empty_textures, &[]);
        pass.draw(0..6, first..first + 1);
        self.draw_calls += 1;
        Ok(())
    }
}

/// Shrinks `data.quad` (input texels, the content `rect` plus a margin) to the
/// part of `rect` inside every half-plane of `clips`, keeping the margin.
fn shrink_quad(data: &mut DrawData, rect: [f64; 4], clips: &[[f64; 4]]) {
    let q = data.quad.map(f64::from);
    let margin = [
        rect[0] - q[0],
        rect[1] - q[1],
        q[2] - rect[2],
        q[3] - rect[3],
    ];
    // The quad's margin is at least one target pixel in input texels; a cut
    // whose coverage reaches past it (the piece drawn first) moves out by two.
    let reach = 2.0 * margin.iter().copied().fold(0.0, f64::max);
    let mut poly = vec![
        DVec2::new(rect[0], rect[1]),
        DVec2::new(rect[2], rect[1]),
        DVec2::new(rect[2], rect[3]),
        DVec2::new(rect[0], rect[3]),
    ];
    for [a, b, c, r] in clips {
        let c = c + if *r > 0.0 { reach * a.hypot(*b) } else { 0.0 };
        let d = |p: DVec2| a * p.x + b * p.y + c;
        let mut out = Vec::with_capacity(poly.len() + 1);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (dp, dq) = (d(p), d(q));
            if dp >= 0.0 {
                out.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                out.push(p + (q - p) * (dp / (dp - dq)));
            }
        }
        poly = out;
        if poly.len() < 3 {
            return;
        }
    }
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in &poly {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    data.quad = [
        (lo.x - margin[0]).max(q[0]) as f32,
        (lo.y - margin[1]).max(q[1]) as f32,
        (hi.x + margin[2]).min(q[2]) as f32,
        (hi.y + margin[3]).min(q[3]) as f32,
    ];
}

/// A composite or fill node's region in space pixels (its `rect` is in texels
/// of `pixel_scale` pixels; composite scales are whole numbers).
fn node_pixels(node: &Node) -> IRect {
    let s = node.pixel_scale;
    let px = |v: i32| (f64::from(v) * s).round() as i32;
    IRect::new(
        px(node.rect.x0),
        px(node.rect.y0),
        px(node.rect.x1),
        px(node.rect.y1),
    )
}

fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

/// Geometry, colour and flags of one op (blend flags are added by the caller).
/// Motion-blurred ops append their sample placements to `samples`, lit ones
/// their lighting record to `lighting` (the caller offsets `extra.y` / `extra.w`
/// by where the composite's records land).
#[allow(clippy::too_many_arguments)]
fn plan_op(
    node: &Node,
    op: &LayerOp,
    outputs: &[Option<GpuImage>],
    mask_outs: &[Option<masks::MaskOut>],
    target_size: [u32; 2],
    samples: &mut Vec<SampleData>,
    lighting: &mut Vec<[f32; 4]>,
    atlas: Option<&ShadowAtlas>,
    skipped_maps: &rustc_hash::FxHashSet<NodeId>,
) -> Result<Option<DrawData>, RenderError> {
    // Composite texel → … : target texel = comp px / scale − rect origin.
    let to_target = DAffine2::from_translation(DVec2::new(
        -f64::from(node.rect.x0),
        -f64::from(node.rect.y0),
    )) * DAffine2::from_scale(DVec2::splat(1.0 / node.pixel_scale));
    // Input texel → input space px, the content rectangle in input texels and
    // the input image.
    let (from_texel, rect, image) = match &op.input {
        OpInput::Solid { rect, .. } => (
            DAffine2::IDENTITY,
            [rect.x0, rect.y0, rect.x1, rect.y1],
            None,
        ),
        OpInput::Image(id) => {
            let img = outputs[id.index()]
                .as_ref()
                .ok_or_else(|| RenderError::Other(format!("input of {} missing", op.layer)))?;
            // Input texel t → input space px: rect origin + t · pixel_scale.
            let origin = DVec2::new(f64::from(img.rect.x0), f64::from(img.rect.y0));
            let s = img.pixel_scale;
            // Coverage edges: the layer's content bounds (in input texels).
            let c = &op.content;
            (
                DAffine2::from_translation(origin) * DAffine2::from_scale(DVec2::splat(s)),
                [
                    (c.x0 - origin.x) / s,
                    (c.y0 - origin.y) / s,
                    (c.x1 - origin.x) / s,
                    (c.y1 - origin.y) / s,
                ],
                Some(img),
            )
        }
    };
    let near = op.projection.is_some().then_some(crate::camera::NEAR);
    let geometry = if let Some(placements) = &op.motion {
        let first = samples.len();
        for h in placements.iter() {
            let full = h.followed_by(&to_target).preceded_by(&from_texel);
            if let Some(sample) = SampleData::new(&full, near) {
                samples.push(sample);
            }
        }
        if samples.len() == first {
            return Ok(None);
        }
        let b = IRect::new(
            op.bounds.x0 - node.rect.x0,
            op.bounds.y0 - node.rect.y0,
            op.bounds.x1 - node.rect.x0,
            op.bounds.y1 - node.rect.y0,
        );
        let mut d = DrawData::motion(
            [
                f64::from(b.x0),
                f64::from(b.y0),
                f64::from(b.x1),
                f64::from(b.y1),
            ],
            rect,
            target_size,
        );
        d.extra[1] = first as u32;
        d.extra[2] = (samples.len() - first) as u32;
        if near.is_some() {
            d.mode[1] |= flags::PROJECTIVE;
        }
        Some(d)
    } else if let Some(h) = &op.projection {
        let full = h.followed_by(&to_target).preceded_by(&from_texel);
        DrawData::projective(&full, rect, target_size, crate::camera::NEAR)
    } else {
        DrawData::geometry(&(to_target * op.transform * from_texel), rect, target_size)
    };
    let Some(mut data) = geometry else {
        return Ok(None);
    };
    match (&op.input, image) {
        (OpInput::Solid { color, .. }, _) => data.color = *color,
        (OpInput::Image(_), Some(img)) => {
            let [tw, th] = img.texture_size();
            data.src[0] = 1.0 / tw as f32;
            data.src[1] = 1.0 / th as f32;
            data.mode[1] |= flags::TEXTURE;
            match img.alpha {
                ImageAlpha::Premultiplied => {}
                ImageAlpha::Straight => data.mode[1] |= flags::STRAIGHT,
                ImageAlpha::Opaque => data.mode[1] |= flags::OPAQUE,
            }
        }
        (OpInput::Image(_), None) => unreachable!("images resolved above"),
    }
    if let Some(l) = &op.lighting {
        let map = |id: NodeId| {
            let (image, slots) = atlas?;
            let img = outputs[id.index()].as_ref()?;
            Some(shadow::uv_map(img, *slots.get(&id)?, image.texture_size()))
        };
        let per_sample = op.motion.as_ref().is_some_and(|m| {
            m.len() == l.samples.len() && l.samples.iter().all(|s| s.lights.len() == l.lights.len())
        });
        if per_sample {
            // Motion blur: the layer lit at every sample, records back to back.
            data.extra[3] = lighting.len() as u32;
            for s in l.samples.iter() {
                let at = crate::light::Lighting {
                    matrix: s.matrix,
                    lights: s.lights.clone(),
                    eye: s.eye,
                    samples: std::sync::Arc::new([]),
                    ..(**l).clone()
                };
                at.pack(&from_texel, node.pixel_scale, &map, false, lighting);
            }
            data.mode[1] |= flags::LIT_SAMPLES;
        } else {
            let use_map = l.shadow_map.is_some_and(|id| !skipped_maps.contains(&id));
            data.extra[3] = l.pack(&from_texel, node.pixel_scale, &map, use_map, lighting);
        }
        data.mode[1] |= flags::LIT;
    }
    if !op.clips.is_empty() {
        // Input px half-planes → input texels.
        let (o, s) = (from_texel.translation, from_texel.matrix2.x_axis.x);
        let clips: Vec<[f64; 4]> = op
            .clips
            .iter()
            .map(|[a, b, c, reach]| [a * s, b * s, a * o.x + b * o.y + c, *reach])
            .collect();
        data.set_clips(&clips);
        if op.motion.is_none() {
            // Draw only the piece: the rectangle cut by the half-planes, plus
            // the quad's anti-aliasing margin.
            shrink_quad(&mut data, rect, &clips);
        }
    }
    data.target[2] = op.opacity;
    if let Some(m) = op.matte {
        let matte = outputs[m.node.index()]
            .as_ref()
            .ok_or_else(|| RenderError::Other(format!("matte of {} missing", op.layer)))?;
        // The matte composite shares the target's space and scale.
        let [mw, mh] = matte.texel_size();
        data.matte = [
            (f64::from(matte.rect.x0) / matte.pixel_scale - f64::from(node.rect.x0)) as f32,
            (f64::from(matte.rect.y0) / matte.pixel_scale - f64::from(node.rect.y0)) as f32,
            mw as f32,
            mh as f32,
        ];
        data.mode[1] |= flags::MATTE;
        data.mode[2] = m.mode as u32;
    }
    if let Some(m) = op.mask {
        let mask = mask_outs
            .get(m.node.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| RenderError::Other(format!("mask of {} missing", op.layer)))?;
        // Input texel u → layer pixels: solids are drawn in layer pixels, images
        // map their texels; then layer pixels → the mask's atlas uv.
        let (in_scale, in_origin) = match &op.input {
            OpInput::Solid { .. } => (1.0, [0.0, 0.0]),
            OpInput::Image(id) => {
                let img = outputs[id.index()]
                    .as_ref()
                    .ok_or_else(|| RenderError::Other(format!("input of {} missing", op.layer)))?;
                (
                    img.pixel_scale,
                    [f64::from(img.rect.x0), f64::from(img.rect.y0)],
                )
            }
        };
        let [pw, ph] = mask.image.texture_size();
        let page = [f64::from(pw), f64::from(ph)];
        let mut uv = [0.0f32; 4];
        let mut clip = [0.0f32; 4];
        for i in 0..2 {
            uv[i] = (in_scale / (mask.scale[i] * page[i])) as f32;
            uv[i + 2] = (((in_origin[i] - mask.px_origin[i]) / mask.scale[i] + mask.origin[i])
                / page[i]) as f32;
            // Half a texel into the zero gutter around the region, no further.
            clip[i] = ((mask.origin[i] - 0.5) / page[i]) as f32;
            clip[i + 2] = ((mask.origin[i] + mask.texels[i] + 0.5) / page[i]) as f32;
        }
        data.mask_uv = uv;
        data.mask_clip = clip;
        data.mask_val = [m.value[0], m.value[1], 0.0, 0.0];
        data.mode[1] |= flags::MASK;
    }
    Ok(Some(data))
}

/// Records the output conversion of `image` into `target` (a `format` view).
pub(crate) fn record_output_pass(
    device: &wgpu::Device,
    pipelines: &mut Pipelines,
    encoder: &mut wgpu::CommandEncoder,
    image: &GpuImage,
    target: &wgpu::TextureView,
    format: wgpu::TextureFormat,
    uniform: &wgpu::Buffer,
) {
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bae-render.output"),
        layout: &pipelines.output_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&image.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let [w, h] = image.texel_size();
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.output"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipelines.output_for(device, format));
    pass.set_bind_group(0, &group, &[]);
    pass.set_scissor_rect(0, 0, w.max(1), h.max(1));
    pass.draw(0..3, 0..1);
}
