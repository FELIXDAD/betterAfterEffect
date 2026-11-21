//! Executes a [`RenderGraph`] on the shared device: one command encoder per
//! frame, nodes in graph order, intermediates freed as soon as their last
//! consumer is encoded (their pooled textures are reused later in the frame).
//!
//! Composites batch consecutive plain solids into one instanced draw, keep
//! Normal / Add / Screen on fixed-function blending, and for backdrop-reading
//! modes (Multiply, Overlay, preserve transparency, clamped Add) end the pass,
//! copy the layer's destination bounds into a scratch texture and continue.
//!
//! Mask nodes run their own passes (`gpu/mask.wgsl`, see [`crate::mask`]):
//! coverage accumulation, resolve + horizontal feather, vertical feather, and –
//! with several masks – one accumulator pass merging them with blend states.

use std::ops::Not;
use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use glam::{DAffine2, DVec2};

use crate::blend::BlendMode;
use crate::error::RenderError;
use crate::geom::IRect;
use crate::gpu::{
    BlendKind, DrawData, EdgeData, MASK_ACCUM_FORMAT, MASK_FORMAT, MaskBlend, MaskPassData,
    Pipelines, flags,
};
use crate::graph::{CompositeNode, LayerOp, MaskNode, Node, NodeKind, OpInput, RenderGraph};
use crate::mask::{Expansion, MASK_EVEN_ODD, kernel_norm, kernel_radius};
use crate::pool::TexturePool;
use crate::source::{EncodeContext, GpuImage, ImageAllocator, ImageAlpha};

/// Per-renderer buffer of draw records (grown on demand, reused across frames).
#[derive(Default)]
pub(crate) struct DrawBuffer {
    buffer: Option<(wgpu::Buffer, wgpu::BindGroup, u64)>,
}

impl DrawBuffer {
    fn ensure(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout, draws: u64) {
        let needed = draws.max(64) * std::mem::size_of::<DrawData>() as u64;
        if self
            .buffer
            .as_ref()
            .is_some_and(|(_, _, cap)| *cap >= needed)
        {
            return;
        }
        let capacity = needed.next_power_of_two();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae-render.draws"),
            size: capacity,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.draws"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        self.buffer = Some((buffer, bind_group, capacity));
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
}

struct PoolAllocator<'a> {
    pool: &'a mut TexturePool,
    device: &'a wgpu::Device,
    format: wgpu::TextureFormat,
}

impl ImageAllocator for PoolAllocator<'_> {
    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        self.pool
            .acquire(self.device, self.format, rect, pixel_scale)
    }
}

/// Executes `graph` and submits it. Returns the output node's image.
pub(crate) fn execute(
    gpu: &Arc<GpuContext>,
    pipelines: &mut Pipelines,
    pool: &mut TexturePool,
    draw_buffer: &mut DrawBuffer,
    mask_buffers: &mut MaskBuffers,
    graph: &RenderGraph,
    format: wgpu::TextureFormat,
) -> Result<Executed, RenderError> {
    let device = gpu.device();
    let total_draws: usize = graph
        .nodes
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Composite(c) => c.ops.len(),
            NodeKind::Fill(_) => 1,
            _ => 0,
        })
        .sum();
    draw_buffer.ensure(device, &pipelines.draws_layout, total_draws as u64);
    let (buffer, draws_group, _) = draw_buffer.buffer.as_ref().expect("ensured");
    let (mut total_edges, mut total_passes) = (0u64, 0u64);
    for node in &graph.nodes {
        if let NodeKind::Mask(m) = &node.kind {
            total_edges += m.rasters.iter().map(|r| r.edges.len() as u64).sum::<u64>();
            total_passes += 3 * m.rasters.len() as u64;
        }
    }
    let masks = if total_passes > 0 {
        let edges = mask_buffers
            .edges
            .ensure(
                device,
                "bae-render.mask-edges",
                total_edges * std::mem::size_of::<EdgeData>() as u64,
            )
            .clone();
        let passes = mask_buffers
            .passes
            .ensure(
                device,
                "bae-render.mask-passes",
                total_passes * std::mem::size_of::<MaskPassData>() as u64,
            )
            .clone();
        Some(MaskCursor {
            edges,
            passes,
            next_edge: 0,
            next_pass: 0,
        })
    } else {
        None
    };

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
        buffer,
        draws_group,
        next_draw: 0,
        format,
        blendable,
        draw_calls: 0,
        passes: 0,
        masks,
    };

    for (index, node) in graph.nodes.iter().enumerate() {
        let image = match &node.kind {
            NodeKind::Composite(c) => {
                let px = node_pixels(node);
                let target = pool.acquire(device, format, px, node.pixel_scale)?;
                let scratch_needed = c.ops.iter().any(|op| ctx.needs_backdrop(c, op));
                let scratch = if scratch_needed {
                    Some(pool.acquire(device, format, px, node.pixel_scale)?)
                } else {
                    None
                };
                ctx.composite(&mut encoder, node, c, &target, scratch.as_ref(), &outputs)?;
                target
            }
            NodeKind::Fill(fill) => {
                let target = pool.acquire(device, format, node_pixels(node), node.pixel_scale)?;
                ctx.fill(&mut encoder, node, fill.color, &fill.rect, &target)?;
                target
            }
            NodeKind::Source(src) => {
                let mut alloc = PoolAllocator {
                    pool,
                    device,
                    format,
                };
                let mut ectx = EncodeContext {
                    gpu,
                    encoder: &mut encoder,
                    format,
                    allocator: &mut alloc,
                };
                src.source.render(&mut ectx, &src.request)?
            }
            NodeKind::Effect(fx) => {
                let input = outputs[fx.input.index()]
                    .clone()
                    .ok_or_else(|| RenderError::Other("effect input missing".into()))?;
                let params = fx.frame.params(&fx.instance);
                let mut alloc = PoolAllocator {
                    pool,
                    device,
                    format,
                };
                let mut ectx = EncodeContext {
                    gpu,
                    encoder: &mut encoder,
                    format,
                    allocator: &mut alloc,
                };
                fx.effect.render(&mut ectx, &params, &input, node.rect)?
            }
            NodeKind::Mask(m) => ctx.mask(&mut encoder, pool, m)?,
        };
        outputs[index] = Some(image);
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
    gpu.queue().submit([encoder.finish()]);
    let image = outputs[graph.output.index()]
        .take()
        .ok_or_else(|| RenderError::Other("graph produced no output".into()))?;
    Ok(Executed {
        image,
        draw_calls,
        passes,
    })
}

struct ExecCtx<'a> {
    gpu: &'a Arc<GpuContext>,
    pipelines: &'a mut Pipelines,
    buffer: &'a wgpu::Buffer,
    draws_group: &'a wgpu::BindGroup,
    next_draw: u32,
    format: wgpu::TextureFormat,
    blendable: bool,
    draw_calls: usize,
    passes: usize,
    masks: Option<MaskCursor>,
}

/// Where the next mask edges and pass records go in this frame's buffers.
struct MaskCursor {
    edges: wgpu::Buffer,
    passes: wgpu::Buffer,
    next_edge: u32,
    next_pass: u32,
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

    fn composite(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        node: &Node,
        c: &CompositeNode,
        target: &GpuImage,
        scratch: Option<&GpuImage>,
        outputs: &[Option<GpuImage>],
    ) -> Result<(), RenderError> {
        let device = self.gpu.device();
        let tsize = target.texture_size();
        let mut planned: Vec<Planned> = Vec::with_capacity(c.ops.len());
        let mut drawn_any = false;
        for op in &c.ops {
            let mut kind =
                BlendKind::for_mode(op.blend, c.clamp, op.preserve_transparency, self.blendable);
            let mut backdrop = kind == BlendKind::Replace;
            if backdrop && !drawn_any && self.blendable {
                // Over a transparent backdrop every mode reduces to Normal, and
                // preserve-transparency shows nothing.
                if op.preserve_transparency {
                    continue;
                }
                kind = BlendKind::Normal;
                backdrop = false;
            }
            let Some(mut data) = plan_op(node, op, outputs, tsize)? else {
                continue;
            };
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
            let textures = if src_view.is_none()
                && matte_view.is_none()
                && dst_view.is_none()
                && mask_view.is_none()
            {
                None
            } else {
                Some(
                    self.pipelines
                        .textures(device, src_view, matte_view, dst_view, mask_view),
                )
            };
            let bounds = IRect::new(
                op.bounds.x0 - node.rect.x0,
                op.bounds.y0 - node.rect.y0,
                op.bounds.x1 - node.rect.x0,
                op.bounds.y1 - node.rect.y0,
            )
            .intersect(&IRect::from_size(tsize[0], tsize[1]));
            if bounds.is_empty() {
                continue;
            }
            planned.push(Planned {
                data,
                kind,
                textures,
                bounds,
                backdrop,
            });
            drawn_any = true;
        }
        let datas: Vec<DrawData> = planned.iter().map(|p| p.data).collect();
        let first = self.write_draws(&datas);

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
            let mut current_kind = None;
            let start = i;
            while i < planned.len() {
                let p = &planned[i];
                if i > start && p.backdrop {
                    break;
                }
                if current_kind != Some(p.kind) {
                    pass.set_pipeline(self.pipelines.composite(device, format, p.kind));
                    current_kind = Some(p.kind);
                }
                // Batch a run of plain solids with the same pipeline.
                let mut end = i + 1;
                if p.textures.is_none() && !p.backdrop {
                    while end < planned.len()
                        && planned[end].textures.is_none()
                        && !planned[end].backdrop
                        && planned[end].kind == p.kind
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
        pass.set_pipeline(self.pipelines.composite(device, self.format, kind));
        pass.set_bind_group(0, self.draws_group, &[]);
        pass.set_bind_group(1, &self.pipelines.empty_textures, &[]);
        pass.draw(0..6, first..first + 1);
        self.draw_calls += 1;
        Ok(())
    }
}

impl ExecCtx<'_> {
    /// Encodes a mask node; returns the combined mask, or the single mask's
    /// coverage (see [`MaskNode::is_single`]).
    fn mask(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pool: &mut TexturePool,
        node: &MaskNode,
    ) -> Result<GpuImage, RenderError> {
        let gpu = self.gpu.clone();
        let device = gpu.device();
        let mut cursor = self
            .masks
            .take()
            .ok_or_else(|| RenderError::Other("mask buffers missing".into()))?;
        let rule = u32::from(MASK_EVEN_ODD);
        let mut finals: Vec<GpuImage> = Vec::with_capacity(node.rasters.len());
        for raster in &node.rasters {
            // 1. Exact-area coverage accumulation of the path (R) and the
            // expansion outline (G).
            let accum = pool.acquire(device, MASK_ACCUM_FORMAT, raster.rect, raster.pixel_scale)?;
            let [aw, ah] = accum.texture_size();
            let edges: Vec<EdgeData> = raster
                .edges
                .iter()
                .map(|e| EdgeData {
                    p0: e.p0,
                    p1: e.p1,
                    xmax: raster.xmax,
                    chan: u32::from(e.stroke),
                    size: [aw as f32, ah as f32],
                })
                .collect();
            self.mask_edges(encoder, &mut cursor, &accum.view, &edges);

            // 2. Resolve (fill rule, expansion, layer frame) + horizontal feather.
            let src = |img: &GpuImage| {
                let [w, h] = img.texture_size();
                [w as f32, h as f32, 1.0 / w as f32, 1.0 / h as f32]
            };
            let [sx, sy] = raster.sigma;
            let expansion = match raster.expansion {
                Expansion::None => 0,
                Expansion::Grow => 1,
                Expansion::Shrink => 2,
            };
            let resolved = pool.acquire(device, MASK_FORMAT, raster.rect, raster.pixel_scale)?;
            let record = MaskPassData {
                map: [1.0, 1.0, 0.0, 0.0],
                src: src(&accum),
                blur: [1.0, 0.0, sx, kernel_radius(sx) as f32],
                clip: raster.clip.unwrap_or_default(),
                value: [0.0, 1.0, kernel_norm(sx), 0.0],
                mode: [0, rule, expansion, u32::from(raster.clip.is_some())],
            };
            self.mask_passes(
                encoder,
                &mut cursor,
                &resolved.view,
                0.0,
                &[(record, MaskBlend::Replace, &accum.view)],
            );
            drop(accum);

            // 3. Vertical feather.
            let out = if kernel_radius(sy) > 0 {
                let blurred = pool.acquire(device, MASK_FORMAT, raster.rect, raster.pixel_scale)?;
                let record = MaskPassData {
                    map: [1.0, 1.0, 0.0, 0.0],
                    src: src(&resolved),
                    blur: [0.0, 1.0, sy, kernel_radius(sy) as f32],
                    value: [0.0, 1.0, kernel_norm(sy), 0.0],
                    mode: [1, 0, 0, 0],
                    ..MaskPassData::default()
                };
                self.mask_passes(
                    encoder,
                    &mut cursor,
                    &blurred.view,
                    0.0,
                    &[(record, MaskBlend::Replace, &resolved.view)],
                );
                blurred
            } else {
                resolved
            };
            finals.push(out);
        }

        let result = if node.is_single() {
            finals.pop()
        } else {
            // 4. Merge top to bottom into the accumulator (blend state per mode),
            // upsampling reduced rasters bilinearly.
            let acc = pool.acquire(device, MASK_FORMAT, node.rect, node.pixel_scale)?;
            let jobs: Vec<(MaskPassData, MaskBlend, &wgpu::TextureView)> = node
                .rasters
                .iter()
                .zip(&finals)
                .map(|(raster, img)| {
                    let [w, h] = img.texture_size();
                    let s = img.pixel_scale;
                    (
                        MaskPassData {
                            map: [
                                (acc.pixel_scale / s) as f32,
                                (acc.pixel_scale / s) as f32,
                                (f64::from(acc.rect.x0 - img.rect.x0) / s) as f32,
                                (f64::from(acc.rect.y0 - img.rect.y0) / s) as f32,
                            ],
                            src: [w as f32, h as f32, 1.0 / w as f32, 1.0 / h as f32],
                            value: [raster.value[0], raster.value[1], 1.0, 0.0],
                            mode: [2, 0, 0, 0],
                            ..MaskPassData::default()
                        },
                        MaskBlend::Mode(raster.mode),
                        &img.view,
                    )
                })
                .collect();
            self.mask_passes(encoder, &mut cursor, &acc.view, f64::from(node.init), &jobs);
            Some(acc)
        };
        self.masks = Some(cursor);
        result.ok_or_else(|| RenderError::Other("mask without rasters".into()))
    }

    /// Clears `target` and accumulates `edges` into it.
    fn mask_edges(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        cursor: &mut MaskCursor,
        target: &wgpu::TextureView,
        edges: &[EdgeData],
    ) {
        let device = self.gpu.device();
        let first = cursor.next_edge;
        if !edges.is_empty() {
            self.gpu.queue().write_buffer(
                &cursor.edges,
                u64::from(first) * std::mem::size_of::<EdgeData>() as u64,
                bytemuck::cast_slice(edges),
            );
            cursor.next_edge += edges.len() as u32;
        }
        let dummy = self.pipelines.dummy.clone();
        let group = edges
            .is_empty()
            .not()
            .then(|| self.mask_group(cursor, &dummy));
        let pipelines = self.pipelines.masks(device);
        let mut pass = begin_mask_pass(encoder, target, 0.0);
        self.passes += 1;
        if let Some(group) = group {
            pass.set_pipeline(&pipelines.edges);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..6, first..first + edges.len() as u32);
            self.draw_calls += 1;
        }
    }

    /// One render pass on `target` (cleared to `clear`) drawing a full-screen
    /// mask pass per job.
    fn mask_passes(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        cursor: &mut MaskCursor,
        target: &wgpu::TextureView,
        clear: f64,
        jobs: &[(MaskPassData, MaskBlend, &wgpu::TextureView)],
    ) {
        let device = self.gpu.device();
        let first = cursor.next_pass;
        let records: Vec<MaskPassData> = jobs.iter().map(|j| j.0).collect();
        self.gpu.queue().write_buffer(
            &cursor.passes,
            u64::from(first) * std::mem::size_of::<MaskPassData>() as u64,
            bytemuck::cast_slice(&records),
        );
        cursor.next_pass += jobs.len() as u32;
        let groups: Vec<wgpu::BindGroup> = jobs
            .iter()
            .map(|(_, _, view)| self.mask_group(cursor, view))
            .collect();
        let pipelines: Vec<wgpu::RenderPipeline> = jobs
            .iter()
            .map(|(_, blend, _)| self.pipelines.masks(device).pass(device, *blend).clone())
            .collect();
        let mut pass = begin_mask_pass(encoder, target, clear);
        self.passes += 1;
        for (i, (pipeline, group)) in pipelines.iter().zip(&groups).enumerate() {
            let index = first + i as u32;
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, index..index + 1);
            self.draw_calls += 1;
        }
    }

    fn mask_group(&mut self, cursor: &MaskCursor, view: &wgpu::TextureView) -> wgpu::BindGroup {
        let device = self.gpu.device();
        let pipelines = self.pipelines.masks(device);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.mask"),
            layout: &pipelines.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cursor.edges.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cursor.passes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&pipelines.sampler),
                },
            ],
        })
    }
}

fn begin_mask_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    clear: f64,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.mask"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: clear,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
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
fn plan_op(
    node: &Node,
    op: &LayerOp,
    outputs: &[Option<GpuImage>],
    target_size: [u32; 2],
) -> Result<Option<DrawData>, RenderError> {
    // Composite texel → … : target texel = comp px / scale − rect origin.
    let to_target = DAffine2::from_translation(DVec2::new(
        -f64::from(node.rect.x0),
        -f64::from(node.rect.y0),
    )) * DAffine2::from_scale(DVec2::splat(1.0 / node.pixel_scale))
        * op.transform;
    let mut data = match &op.input {
        OpInput::Solid { color, rect } => {
            let Some(mut d) = DrawData::geometry(
                &to_target,
                [rect.x0, rect.y0, rect.x1, rect.y1],
                target_size,
            ) else {
                return Ok(None);
            };
            d.color = *color;
            d
        }
        OpInput::Image(id) => {
            let img = outputs[id.index()]
                .as_ref()
                .ok_or_else(|| RenderError::Other(format!("input of {} missing", op.layer)))?;
            // Input texel t → input space px: rect origin + t · pixel_scale.
            let origin = DVec2::new(f64::from(img.rect.x0), f64::from(img.rect.y0));
            let s = img.pixel_scale;
            let from_texel =
                DAffine2::from_translation(origin) * DAffine2::from_scale(DVec2::splat(s));
            let t = to_target * from_texel;
            // Coverage edges: the layer's content bounds (in input texels).
            let c = &op.content;
            let rect = [
                (c.x0 - origin.x) / s,
                (c.y0 - origin.y) / s,
                (c.x1 - origin.x) / s,
                (c.y1 - origin.y) / s,
            ];
            let Some(mut d) = DrawData::geometry(&t, rect, target_size) else {
                return Ok(None);
            };
            let [tw, th] = img.texture_size();
            d.src[0] = 1.0 / tw as f32;
            d.src[1] = 1.0 / th as f32;
            d.mode[1] |= flags::TEXTURE;
            match img.alpha {
                ImageAlpha::Premultiplied => {}
                ImageAlpha::Straight => d.mode[1] |= flags::STRAIGHT,
                ImageAlpha::Opaque => d.mode[1] |= flags::OPAQUE,
            }
            d
        }
    };
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
        let mask = outputs[m.node.index()]
            .as_ref()
            .ok_or_else(|| RenderError::Other(format!("mask of {} missing", op.layer)))?;
        // Input texel u → layer pixels: solids are drawn in layer pixels, images
        // map their texels; then layer pixels → mask uv.
        let (in_scale, in_origin) = match &op.input {
            OpInput::Solid { .. } => (1.0, DVec2::ZERO),
            OpInput::Image(id) => {
                let img = outputs[id.index()]
                    .as_ref()
                    .ok_or_else(|| RenderError::Other(format!("input of {} missing", op.layer)))?;
                (
                    img.pixel_scale,
                    DVec2::new(f64::from(img.rect.x0), f64::from(img.rect.y0)),
                )
            }
        };
        let [mw, mh] = mask.texture_size();
        let s = mask.pixel_scale;
        let (sw, sh) = (s * f64::from(mw), s * f64::from(mh));
        data.mask_uv = [
            (in_scale / sw) as f32,
            (in_scale / sh) as f32,
            ((in_origin.x - f64::from(mask.rect.x0)) / sw) as f32,
            ((in_origin.y - f64::from(mask.rect.y0)) / sh) as f32,
        ];
        data.mask_val = [m.value[0], m.value[1], 0.0, 0.0];
        data.mode[1] |= flags::MASK;
    }
    Ok(Some(data))
}
