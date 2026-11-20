//! Executes a [`RenderGraph`] on the shared device: one command encoder per
//! frame, nodes in graph order, intermediates freed as soon as their last
//! consumer is encoded (their pooled textures are reused later in the frame).
//!
//! Composites batch consecutive plain solids into one instanced draw, keep
//! Normal / Add / Screen on fixed-function blending, and for backdrop-reading
//! modes (Multiply, Overlay, preserve transparency, clamped Add) end the pass,
//! copy the layer's destination bounds into a scratch texture and continue.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use glam::{DAffine2, DVec2};

use crate::blend::BlendMode;
use crate::effect::EffectParams;
use crate::error::RenderError;
use crate::geom::IRect;
use crate::gpu::{BlendKind, DrawData, Pipelines, flags};
use crate::graph::{CompositeNode, LayerOp, Node, NodeKind, OpInput, RenderGraph};
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
    };

    for (index, node) in graph.nodes.iter().enumerate() {
        let image = match &node.kind {
            NodeKind::Composite(c) => {
                let target = pool.acquire(device, format, node.rect, node.pixel_scale)?;
                let scratch_needed = c.ops.iter().any(|op| ctx.needs_backdrop(c, op));
                let scratch = if scratch_needed {
                    Some(pool.acquire(device, format, node.rect, node.pixel_scale)?)
                } else {
                    None
                };
                ctx.composite(&mut encoder, node, c, &target, scratch.as_ref(), &outputs)?;
                target
            }
            NodeKind::Fill(fill) => {
                let target = pool.acquire(device, format, node.rect, node.pixel_scale)?;
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
                let params = EffectParams {
                    effect: &fx.instance,
                    time: fx.time,
                    scale: fx.scale,
                };
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
            let dst_view = if backdrop {
                scratch.map(|s| &s.view)
            } else {
                None
            };
            let textures = if src_view.is_none() && matte_view.is_none() && dst_view.is_none() {
                None
            } else {
                Some(
                    self.pipelines
                        .textures(device, src_view, matte_view, dst_view),
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
            // Input texel t → input space px: (t + rect origin) · pixel_scale.
            let from_texel = DAffine2::from_scale(DVec2::splat(img.pixel_scale))
                * DAffine2::from_translation(DVec2::new(
                    f64::from(img.rect.x0),
                    f64::from(img.rect.y0),
                ));
            let t = to_target * from_texel;
            // Coverage edges: the layer's content bounds (in input texels).
            let c = &op.content;
            let s = img.pixel_scale;
            let rect = [
                c.x0 / s - f64::from(img.rect.x0),
                c.y0 / s - f64::from(img.rect.y0),
                c.x1 / s - f64::from(img.rect.x0),
                c.y1 / s - f64::from(img.rect.y0),
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
        data.matte = [
            (matte.rect.x0 - node.rect.x0) as f32,
            (matte.rect.y0 - node.rect.y0) as f32,
            matte.rect.width() as f32,
            matte.rect.height() as f32,
        ];
        data.mode[1] |= flags::MATTE;
        data.mode[2] = m.mode as u32;
    }
    Ok(Some(data))
}
