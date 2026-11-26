//! The shadow atlas of a composite: the images of every image layer that casts
//! shadows there (footage, text, pre-comps after masks and effects), copied side
//! by side into one texture the lit layers sample (`t_shadow` in
//! `composite.wgsl`). Solid casters need no atlas (their coverage is analytic).
//! The copy is a draw through the composite pipeline, so sources of any format
//! and alpha mode land premultiplied in the working format.

use bae_gpu::wgpu;
use glam::{DAffine2, DVec2};
use rustc_hash::FxHashMap;

use crate::error::RenderError;
use crate::geom::IRect;
use crate::gpu::{BlendKind, DrawData, Variant, flags};
use crate::graph::{CompositeNode, NodeId};
use crate::light::CasterPixels;
use crate::source::{GpuImage, ImageAlpha};

/// Texels between atlas entries (bilinear lookups stay inside their entry).
const GUTTER: u32 = 2;
/// Widest atlas row before wrapping.
const ROW: u32 = 4096;
/// Largest atlas side; casters that do not fit cast no shadow.
const LIMIT: u32 = 8192;

/// The image casters of `c`, each once.
pub(crate) fn casters(c: &CompositeNode) -> Vec<NodeId> {
    let mut ids = Vec::new();
    for op in &c.ops {
        let Some(lighting) = &op.lighting else {
            continue;
        };
        for caster in lighting.casters.iter() {
            if let CasterPixels::Image(id) = caster.pixels
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
    }
    ids
}

/// Where each caster image sits in the atlas, and the atlas size (shelf
/// packing, tallest first).
pub(crate) fn layout(
    ids: &[NodeId],
    outputs: &[Option<GpuImage>],
) -> (FxHashMap<NodeId, [u32; 2]>, [u32; 2]) {
    let mut items: Vec<(NodeId, [u32; 2])> = ids
        .iter()
        .filter_map(|id| Some((*id, outputs[id.index()].as_ref()?.texel_size())))
        .collect();
    items.sort_by_key(|(_, [_, h])| std::cmp::Reverse(*h));
    let mut slots = FxHashMap::default();
    let (mut x, mut y, mut row_h, mut width) = (0u32, 0u32, 0u32, 1u32);
    for (id, [w, h]) in items {
        if x > 0 && x + w > ROW {
            y += row_h + GUTTER;
            x = 0;
            row_h = 0;
        }
        if x + w > LIMIT || y + h > LIMIT {
            continue;
        }
        slots.insert(id, [x, y]);
        x += w + GUTTER;
        row_h = row_h.max(h);
        width = width.max(x);
    }
    (slots, [width.max(1), (y + row_h).max(1)])
}

/// Layer px → atlas uv of caster image `img` placed at `slot` in an atlas whose
/// texture is `size` texels, and its uv clip rectangle (half a texel inside).
pub(crate) fn uv_map(img: &GpuImage, slot: [u32; 2], size: [u32; 2]) -> [[f32; 4]; 2] {
    let s = img.pixel_scale;
    let origin = DVec2::new(f64::from(img.rect.x0), f64::from(img.rect.y0));
    let off = DVec2::new(f64::from(slot[0]), f64::from(slot[1]));
    let page = DVec2::new(f64::from(size[0]), f64::from(size[1]));
    let [w, h] = img.texel_size();
    let scale = DVec2::splat(1.0 / s) / page;
    let shift = (off - origin / s) / page;
    let lo = (off + 0.5) / page;
    let hi = (off + DVec2::new(f64::from(w), f64::from(h)) - 0.5) / page;
    [
        [
            scale.x as f32,
            scale.y as f32,
            shift.x as f32,
            shift.y as f32,
        ],
        [lo.x as f32, lo.y as f32, hi.x as f32, hi.y as f32],
    ]
}

impl super::ExecCtx<'_> {
    /// Copies the caster images into `atlas` (cleared first).
    pub(crate) fn blit_atlas(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        atlas: &GpuImage,
        slots: &FxHashMap<NodeId, [u32; 2]>,
        outputs: &[Option<GpuImage>],
    ) -> Result<(), RenderError> {
        let device = self.gpu.device();
        let size = atlas.texture_size();
        let mut draws = Vec::with_capacity(slots.len());
        let mut groups = Vec::with_capacity(slots.len());
        for (id, slot) in slots {
            let Some(img) = outputs[id.index()].as_ref() else {
                continue;
            };
            let [w, h] = img.texel_size();
            let to = DAffine2::from_translation(DVec2::new(f64::from(slot[0]), f64::from(slot[1])));
            let Some(mut d) = DrawData::geometry(&to, [0.0, 0.0, f64::from(w), f64::from(h)], size)
            else {
                continue;
            };
            let [tw, th] = img.texture_size();
            d.src[0] = 1.0 / tw as f32;
            d.src[1] = 1.0 / th as f32;
            d.mode[1] = flags::TEXTURE
                | match img.alpha {
                    ImageAlpha::Premultiplied => 0,
                    ImageAlpha::Straight => flags::STRAIGHT,
                    ImageAlpha::Opaque => flags::OPAQUE,
                };
            draws.push(d);
            groups.push(
                self.pipelines
                    .textures(device, Some(&img.view), None, None, None, None),
            );
        }
        let first = self.write_draws(&draws);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bae-render.shadow-atlas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &atlas.view,
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
        for (i, group) in groups.iter().enumerate() {
            pass.set_bind_group(1, group, &[]);
            let n = first + i as u32;
            pass.draw(0..6, n..n + 1);
            self.draw_calls += 1;
        }
        Ok(())
    }
}

/// An atlas region in texels (for allocation).
pub(crate) fn atlas_rect(size: [u32; 2]) -> IRect {
    IRect::from_size(size[0], size[1])
}
