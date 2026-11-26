//! Mask execution (see [`crate::mask`]): every mask of a frame is rasterised and
//! feathered in shared atlas pages, so the whole frame's masks cost three render
//! passes (coverage, resolve + horizontal feather, vertical feather) plus one
//! accumulator pass for layers with several masks – instead of three per mask.
//!
//! Each raster (and each accumulator) is a region of a page with a one-texel
//! zero gutter around it; draws are scissored to their region and reads stay in
//! it, so regions never see each other.

use super::{ExecCtx, MaskBuffers};
use crate::error::RenderError;
use crate::gpu::{EdgeData, MASK_ACCUM_FORMAT, MASK_FORMAT, MaskBlend, MaskPassData};
use crate::graph::{MaskNode, MaskRaster, NodeKind, RenderGraph};
use crate::mask::{Expansion, MASK_EVEN_ODD, kernel_norm, kernel_radius};
use crate::pool::TexturePool;
use crate::source::GpuImage;
use crate::{IRect, MaskMode};
use bae_gpu::wgpu;

/// Where a mask node's output lives.
#[derive(Clone)]
pub(super) struct MaskOut {
    /// The atlas page (the region is part of it).
    pub image: GpuImage,
    /// Texel offset of the region in the page.
    pub origin: [f64; 2],
    /// Region size in texels.
    pub texels: [f64; 2],
    /// Layer pixel at the region's texel (0, 0) corner.
    pub px_origin: [f64; 2],
    /// Layer pixels per texel along x and y.
    pub scale: [f64; 2],
}

/// A region of an atlas page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Place {
    page: usize,
    origin: [u32; 2],
    size: [u32; 2],
}

/// Shelf packing (tallest first) with a one-texel gutter around every region.
/// Returns the places (in input order) and the page sizes.
fn pack(sizes: &[[u32; 2]], limit: u32) -> (Vec<Place>, Vec<[u32; 2]>) {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i][1]));
    let max_w = sizes.iter().map(|s| s[0]).max().unwrap_or(1);
    let area: f64 = sizes
        .iter()
        .map(|s| f64::from(s[0] + 1) * f64::from(s[1] + 1))
        .sum();
    let width = (area.sqrt().ceil() as u32)
        .max(max_w + 2)
        .max(256)
        .min(limit);
    let mut places = vec![Place::default(); sizes.len()];
    let mut pages = vec![[width, 1u32]];
    let (mut x, mut y, mut shelf) = (1u32, 1u32, 0u32);
    for i in order {
        let [w, h] = sizes[i];
        if x + w + 1 > width {
            y += shelf + 1;
            x = 1;
            shelf = 0;
        }
        if y + h + 1 > limit {
            pages.push([width, 1]);
            x = 1;
            y = 1;
            shelf = 0;
        }
        let page = pages.len() - 1;
        places[i] = Place {
            page,
            origin: [x, y],
            size: [w, h],
        };
        x += w + 1;
        shelf = shelf.max(h);
        pages[page][1] = pages[page][1].max(y + h + 1);
    }
    (places, pages)
}

fn src_of(img: &GpuImage) -> [f32; 4] {
    let [w, h] = img.texture_size();
    [w as f32, h as f32, 1.0 / w as f32, 1.0 / h as f32]
}

fn rect_of(place: &Place) -> [f32; 4] {
    [
        place.origin[0] as f32,
        place.origin[1] as f32,
        place.size[0] as f32,
        place.size[1] as f32,
    ]
}

/// One scissored draw of a pass: target region, record (or edge range).
#[derive(Clone, Copy)]
struct Draw {
    scissor: Place,
    first: u32,
    count: u32,
    blend: MaskBlend,
    /// Source page for the bind group (index into the pass's sources).
    source: usize,
}

/// Encodes every mask node of `graph`; returns their outputs by node index.
pub(super) fn encode(
    ctx: &mut ExecCtx<'_>,
    encoder: &mut wgpu::CommandEncoder,
    pool: &mut TexturePool,
    buffers: &mut MaskBuffers,
    graph: &RenderGraph,
) -> Result<Vec<Option<MaskOut>>, RenderError> {
    let mut outs: Vec<Option<MaskOut>> = vec![None; graph.nodes.len()];
    let nodes: Vec<(usize, &MaskNode)> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| match &n.kind {
            NodeKind::Mask(m) if !m.rasters.is_empty() => Some((i, m)),
            _ => None,
        })
        .collect();
    if nodes.is_empty() {
        return Ok(outs);
    }
    let gpu = ctx.gpu.clone();
    let device = gpu.device();
    let limit = device.limits().max_texture_dimension_2d;

    // Rasters, packed.
    let rasters: Vec<(usize, &MaskRaster)> = nodes
        .iter()
        .flat_map(|(n, m)| m.rasters.iter().map(move |r| (*n, r)))
        .collect();
    let sizes: Vec<[u32; 2]> = rasters
        .iter()
        .map(|(_, r)| r.texels().map(|v| v.clamp(1, limit - 2)))
        .collect();
    let (places, pages) = pack(&sizes, limit);
    let page_image = |pool: &mut TexturePool, format, size: [u32; 2]| {
        pool.acquire_texels(
            device,
            format,
            IRect::from_size(size[0], size[1]),
            1.0,
            size,
        )
    };
    let mut accum = Vec::with_capacity(pages.len());
    let mut resolved = Vec::with_capacity(pages.len());
    let mut feathered = Vec::with_capacity(pages.len());
    for size in &pages {
        accum.push(page_image(pool, MASK_ACCUM_FORMAT, *size)?);
        resolved.push(page_image(pool, MASK_FORMAT, *size)?);
        feathered.push(page_image(pool, MASK_FORMAT, *size)?);
    }

    // Edges (atlas texels) and pass records.
    let mut edges: Vec<EdgeData> = Vec::new();
    let mut records: Vec<MaskPassData> = Vec::new();
    let mut edge_draws: Vec<Vec<Draw>> = (0..pages.len()).map(|_| Vec::new()).collect();
    let mut h_draws: Vec<Vec<Draw>> = (0..pages.len()).map(|_| Vec::new()).collect();
    let mut v_draws: Vec<Vec<Draw>> = (0..pages.len()).map(|_| Vec::new()).collect();
    let rule = u32::from(MASK_EVEN_ODD);
    for ((_, r), place) in rasters.iter().zip(&places) {
        let [ox, oy] = place.origin.map(|v| v as f32);
        let size = {
            let [w, h] = accum[place.page].texture_size();
            [w as f32, h as f32]
        };
        let first = edges.len() as u32;
        edges.extend(r.edges.iter().map(|e| EdgeData {
            p0: [e.p0[0] + ox, e.p0[1] + oy],
            p1: [e.p1[0] + ox, e.p1[1] + oy],
            xmax: r.xmax + ox,
            chan: u32::from(e.stroke),
            size,
            weight: e.weight,
            _pad: 0.0,
        }));
        if !r.edges.is_empty() {
            edge_draws[place.page].push(Draw {
                scissor: *place,
                first,
                count: r.edges.len() as u32,
                blend: MaskBlend::Replace,
                source: 0,
            });
        }
        let [sx, sy] = r.sigma;
        let expansion = match r.expansion {
            Expansion::None => 0,
            Expansion::Grow => 1,
            Expansion::Shrink => 2,
        };
        let map = [1.0, 1.0, -ox, -oy];
        h_draws[place.page].push(Draw {
            scissor: *place,
            first: records.len() as u32,
            count: 1,
            blend: MaskBlend::Replace,
            source: place.page,
        });
        records.push(MaskPassData {
            map,
            src: src_of(&accum[place.page]),
            blur: [1.0, 0.0, sx, kernel_radius(sx) as f32],
            clip: r.clip.unwrap_or_default(),
            value: [0.0, 1.0, kernel_norm(sx), 0.0],
            mode: [0, rule, expansion, u32::from(r.clip.is_some())],
            src_rect: rect_of(place),
        });
        v_draws[place.page].push(Draw {
            scissor: *place,
            first: records.len() as u32,
            count: 1,
            blend: MaskBlend::Replace,
            source: place.page,
        });
        records.push(MaskPassData {
            map,
            src: src_of(&resolved[place.page]),
            blur: [0.0, 1.0, sy, kernel_radius(sy) as f32],
            value: [0.0, 1.0, kernel_norm(sy), 0.0],
            mode: [1, 0, 0, 0],
            src_rect: rect_of(place),
            ..MaskPassData::default()
        });
    }

    // Accumulators of layers with several masks, packed into their own pages.
    let multi: Vec<(usize, &MaskNode)> = nodes
        .iter()
        .filter(|(_, m)| !m.is_single())
        .copied()
        .collect();
    let acc_sizes: Vec<[u32; 2]> = multi
        .iter()
        .map(|(_, m)| {
            crate::source::texels_for(m.rect, m.pixel_scale).map(|v| v.clamp(1, limit - 2))
        })
        .collect();
    let (acc_places, acc_pages) = pack(&acc_sizes, limit);
    let acc_pages = if multi.is_empty() {
        Vec::new()
    } else {
        acc_pages
    };
    let mut acc_images = Vec::with_capacity(acc_pages.len());
    for size in &acc_pages {
        acc_images.push(page_image(pool, MASK_FORMAT, *size)?);
    }
    let mut acc_draws: Vec<Vec<Draw>> = (0..acc_pages.len()).map(|_| Vec::new()).collect();
    for ((n, m), place) in multi.iter().zip(&acc_places) {
        let ps = m.pixel_scale;
        let [ax, ay] = place.origin.map(f64::from);
        let first_raster = rasters
            .iter()
            .position(|(rn, _)| rn == n)
            .expect("rasters of a mask node");
        if m.init > 0.0 {
            // Start from 1: a constant draw (value 1 + 0 · sample).
            acc_draws[place.page].push(Draw {
                scissor: *place,
                first: records.len() as u32,
                count: 1,
                blend: MaskBlend::Replace,
                source: places[first_raster].page,
            });
            records.push(MaskPassData {
                map: [1.0, 1.0, 0.0, 0.0],
                src: src_of(&feathered[places[first_raster].page]),
                value: [1.0, 0.0, 1.0, 0.0],
                mode: [2, 0, 0, 0],
                src_rect: rect_of(&places[first_raster]),
                ..MaskPassData::default()
            });
        }
        for (k, r) in m.rasters.iter().enumerate() {
            let rp = &places[first_raster + k];
            let [sx, sy] = r.pixel_scale;
            acc_draws[place.page].push(Draw {
                scissor: *place,
                first: records.len() as u32,
                count: 1,
                blend: if r.mode == MaskMode::None {
                    MaskBlend::Mode(MaskMode::Add)
                } else {
                    MaskBlend::Mode(r.mode)
                },
                source: rp.page,
            });
            // Accumulator texel (page) → layer px → raster texel (region).
            records.push(MaskPassData {
                map: [
                    (ps / sx) as f32,
                    (ps / sy) as f32,
                    ((f64::from(m.rect.x0 - r.rect.x0) - ax * ps) / sx) as f32,
                    ((f64::from(m.rect.y0 - r.rect.y0) - ay * ps) / sy) as f32,
                ],
                src: src_of(&feathered[rp.page]),
                value: [r.value[0], r.value[1], 1.0, 0.0],
                mode: [2, 0, 0, 0],
                src_rect: rect_of(rp),
                ..MaskPassData::default()
            });
        }
        outs[*n] = Some(MaskOut {
            image: acc_images[place.page].clone(),
            origin: [ax, ay],
            texels: place.size.map(f64::from),
            px_origin: [f64::from(m.rect.x0), f64::from(m.rect.y0)],
            scale: [ps, ps],
        });
    }
    for ((n, r), place) in rasters.iter().zip(&places) {
        if outs[*n].is_none() {
            outs[*n] = Some(MaskOut {
                image: feathered[place.page].clone(),
                origin: place.origin.map(f64::from),
                texels: place.size.map(f64::from),
                px_origin: [f64::from(r.rect.x0), f64::from(r.rect.y0)],
                scale: r.pixel_scale,
            });
        }
    }

    // Upload.
    let queue = gpu.queue();
    let edge_buffer = buffers
        .edges
        .ensure(
            device,
            "bae-render.mask-edges",
            (edges.len() * std::mem::size_of::<EdgeData>()) as u64,
        )
        .clone();
    let pass_buffer = buffers
        .passes
        .ensure(
            device,
            "bae-render.mask-passes",
            (records.len() * std::mem::size_of::<MaskPassData>()) as u64,
        )
        .clone();
    if !edges.is_empty() {
        queue.write_buffer(&edge_buffer, 0, bytemuck::cast_slice(&edges));
    }
    queue.write_buffer(&pass_buffer, 0, bytemuck::cast_slice(&records));

    // Encode: per page coverage, resolve + x feather, y feather; then the
    // accumulators.
    let dummy = ctx.pipelines.dummy.clone();
    let masks = ctx.pipelines.masks();
    let mut run = |ctx: &mut ExecCtx<'_>,
                   target: &GpuImage,
                   sources: &[&wgpu::TextureView],
                   draws: &[Draw],
                   edges_pass: bool| {
        let mut slot = masks.lock();
        let pipelines = slot.get_or_insert_with(|| crate::gpu::MaskPipelines::new(device));
        let groups: Vec<wgpu::BindGroup> = sources
            .iter()
            .map(|view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bae-render.mask"),
                    layout: &pipelines.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: edge_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: pass_buffer.as_entire_binding(),
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
            })
            .collect();
        let edges_pipeline = pipelines.edges.clone();
        let mut pipeline_of = |blend| pipelines.pass(device, blend).clone();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bae-render.mask"),
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
        ctx.passes += 1;
        let mut current: Option<(MaskBlend, usize)> = None;
        for d in draws {
            if edges_pass {
                if current.is_none() {
                    pass.set_pipeline(&edges_pipeline);
                    pass.set_bind_group(0, &groups[0], &[]);
                    current = Some((MaskBlend::Replace, 0));
                }
            } else {
                if current.map(|c| c.0) != Some(d.blend) {
                    pass.set_pipeline(&pipeline_of(d.blend));
                }
                if current.map(|c| c.1) != Some(d.source) {
                    pass.set_bind_group(0, &groups[d.source], &[]);
                }
                current = Some((d.blend, d.source));
            }
            pass.set_scissor_rect(
                d.scissor.origin[0],
                d.scissor.origin[1],
                d.scissor.size[0],
                d.scissor.size[1],
            );
            let vertices = if edges_pass { 0..6 } else { 0..3 };
            pass.draw(vertices, d.first..d.first + d.count);
            ctx.draw_calls += 1;
        }
    };
    for page in 0..pages.len() {
        run(ctx, &accum[page], &[&dummy], &edge_draws[page], true);
        let h: Vec<Draw> = h_draws[page]
            .iter()
            .map(|d| Draw { source: 0, ..*d })
            .collect();
        run(ctx, &resolved[page], &[&accum[page].view], &h, false);
        let v: Vec<Draw> = v_draws[page]
            .iter()
            .map(|d| Draw { source: 0, ..*d })
            .collect();
        run(ctx, &feathered[page], &[&resolved[page].view], &v, false);
    }
    let sources: Vec<&wgpu::TextureView> = feathered.iter().map(|f| &f.view).collect();
    for (page, draws) in acc_draws.iter().enumerate() {
        run(ctx, &acc_images[page], &sources, draws, false);
    }
    Ok(outs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_keeps_gutters_and_spills_into_pages() {
        let sizes = [[100, 50], [30, 80], [100, 50], [10, 10]];
        let (places, pages) = pack(&sizes, 8192);
        assert_eq!(pages.len(), 1);
        for (i, a) in places.iter().enumerate() {
            assert!(a.origin[0] >= 1 && a.origin[1] >= 1);
            assert!(a.origin[0] + a.size[0] < pages[0][0]);
            assert!(a.origin[1] + a.size[1] < pages[0][1]);
            for b in &places[i + 1..] {
                // At least one texel of gutter between any two regions.
                let apart = a.origin[0] + a.size[0] < b.origin[0]
                    || b.origin[0] + b.size[0] < a.origin[0]
                    || a.origin[1] + a.size[1] < b.origin[1]
                    || b.origin[1] + b.size[1] < a.origin[1];
                assert!(apart, "{a:?} {b:?}");
            }
        }
        let (places, pages) = pack(&[[200, 150]; 3], 320);
        assert!(pages.len() >= 2, "{pages:?}");
        assert!(places.iter().all(|p| p.origin[1] + p.size[1] < 320));
    }
}
