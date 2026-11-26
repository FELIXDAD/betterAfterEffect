//! Decode all flat parts together. Channel words bypass the beauty image's half conversion.
use super::*;
use bae_gpu::{
    channels::{Channel, ChannelKind, ChannelPlanes, FootageChannels},
    transfer, wgpu,
};
use exr::prelude::{AttributeValue, FlatSamples, Sample};
use std::collections::BTreeMap;

type DecodedExr = (CpuFrame, Option<Arc<FootageChannels>>);

/// Select RGB from an unqualified, dotted-pass or multipart image. Both the
/// header probe and the pixel reader must select exactly the same pass.
pub(super) fn beauty_group<'a>(
    part: Option<&exr::prelude::Text>,
    names: impl Iterator<Item = &'a exr::prelude::Text>,
) -> Option<(u8, [Option<usize>; 4])> {
    let names: Vec<_> = names.map(|n| n.to_string().to_ascii_lowercase()).collect();
    let part = part
        .map(ToString::to_string)
        .unwrap_or_default()
        .to_ascii_lowercase();
    names
        .iter()
        .filter_map(|name| {
            let (prefix, component) = name.rsplit_once('.').unwrap_or(("", name));
            if component != "r" {
                return None;
            }
            let ids = ["r", "g", "b", "a"].map(|c| {
                let key = if prefix.is_empty() {
                    c.to_owned()
                } else {
                    format!("{prefix}.{c}")
                };
                names.iter().position(|n| n == &key)
            });
            if ids[..3].contains(&None) {
                return None;
            }
            let group = format!("{part}.{prefix}");
            let rank = if group.contains("crypto") {
                3
            } else if group.split('.').any(|p| matches!(p, "beauty" | "combined")) {
                0
            } else if prefix.is_empty() {
                1
            } else {
                2
            };
            Some((rank, ids))
        })
        .min_by_key(|g| g.0)
}

pub(super) fn decode(
    path: &Path,
    video: &VideoInfo,
    gpu: Option<&crate::MediaGpu>,
) -> Result<DecodedExr, MediaError> {
    let image = exr::prelude::read_all_flat_layers_from_file(path)
        .map_err(|e| MediaError::decode(path, e))?;
    let display = image.attributes.display_window;
    let [width, height] = [display.size.0, display.size.1];
    let rgba_len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| MediaError::decode(path, "display window too large"))?;
    let mut rgba = vec![f16::ZERO; rgba_len];
    let beauty = beauty_of(&image);
    if let Some((_, part, ids)) = beauty {
        let layer = &image.layer_data[part];
        for (c, idx) in ids.iter().enumerate() {
            let Some(ch) = idx.map(|i| &layer.channel_data.list[i]) else {
                continue;
            };
            if ch.sampling != exr::math::Vec2(1, 1) || ch.sample_data.len() != layer.size.area() {
                return Err(MediaError::decode(
                    path,
                    "subsampled EXR channels are unsupported",
                ));
            }
            for_each_sample(&image, part, ch, |i, sample| {
                rgba[i * 4 + c] = f16::from_f32(sample.to_f32());
            });
        }
        if ids[3].is_none() {
            rgba.iter_mut()
                .skip(3)
                .step_by(4)
                .for_each(|a| *a = f16::ONE);
        }
    }
    let size = [width as u32, height as u32];
    let channels = gpu.and_then(|gpu| channels(path, &image, beauty, size, gpu));
    Ok((
        CpuFrame {
            width: size[0],
            height: size[1],
            format: PixelFormat::Rgba16Float,
            data: bytemuck::cast_slice(&rgba).to_vec(),
            alpha: video.alpha,
            color: video.color.clone(),
            pixel_aspect: video.pixel_aspect,
        },
        channels,
    ))
}

type FlatImage = exr::image::FlatImage;
type Beauty = (u8, usize, [Option<usize>; 4]);

fn beauty_of(image: &FlatImage) -> Option<Beauty> {
    image
        .layer_data
        .iter()
        .enumerate()
        .filter_map(|(part, layer)| {
            let (rank, ids) = beauty_group(
                layer.attributes.layer_name.as_ref(),
                layer.channel_data.list.iter().map(|c| &c.name),
            )?;
            Some((rank, part, ids))
        })
        .min_by_key(|g| g.0)
}

/// Calls `f` with the display-window index of each of a full-resolution
/// channel's samples inside the display window.
fn for_each_sample(
    image: &FlatImage,
    part: usize,
    ch: &exr::image::AnyChannel<FlatSamples>,
    mut f: impl FnMut(usize, Sample),
) {
    let display = image.attributes.display_window;
    let layer = &image.layer_data[part];
    let pos = layer.attributes.layer_position;
    let dx = i64::from(pos.0) - i64::from(display.position.0);
    let dy = i64::from(pos.1) - i64::from(display.position.1);
    let (width, height) = (display.size.0 as i64, display.size.1 as i64);
    for y in (-dy).max(0)..(height - dy).min(layer.size.1 as i64) {
        for x in (-dx).max(0)..(width - dx).min(layer.size.0 as i64) {
            let i = (y + dy) as usize * width as usize + (x + dx) as usize;
            f(
                i,
                ch.sample_data
                    .value_by_flat_index(y as usize * layer.size.0 + x as usize),
            );
        }
    }
}

/// The auxiliary (non-beauty) channels, uploaded when an effect first asks
/// (half as 16 bit). `None` when there are none or they do not fit the GPU's
/// array limits: the beauty pass still shows.
fn channels(
    path: &Path,
    image: &FlatImage,
    beauty: Option<Beauty>,
    size: [u32; 2],
    gpu: &crate::MediaGpu,
) -> Option<Arc<FootageChannels>> {
    let mut list = Vec::new();
    let mut sources = Vec::new();
    let mut counts = [0u32; 2];
    let mut attributes = BTreeMap::new();
    for (part, layer) in image.layer_data.iter().enumerate() {
        for (key, attr) in image.attributes.other.iter().chain(&layer.attributes.other) {
            let key = key.to_string();
            if key.starts_with("cryptomatte/")
                && let AttributeValue::Text(value) = attr
            {
                attributes.insert(key, value.to_string());
            }
        }
        for (idx, ch) in layer.channel_data.list.iter().enumerate() {
            let in_beauty = beauty.is_some_and(|(_, p, ids)| p == part && ids.contains(&Some(idx)));
            if in_beauty
                || ch.sampling != exr::math::Vec2(1, 1)
                || ch.sample_data.len() != layer.size.area()
            {
                continue;
            }
            let kind = match ch.sample_data {
                FlatSamples::F16(_) => ChannelKind::Half,
                FlatSamples::F32(_) => ChannelKind::Float,
                FlatSamples::U32(_) => ChannelKind::Uint,
            };
            let array = usize::from(kind == ChannelKind::Half);
            list.push(Channel {
                name: match &layer.attributes.layer_name {
                    Some(part_name) => format!("{part_name}.{}", ch.name),
                    None => ch.name.to_string(),
                },
                kind,
                slice: counts[array],
            });
            sources.push((part, idx));
            counts[array] += 1;
        }
    }
    let limits = gpu.context().device().limits();
    if list.is_empty()
        || counts.iter().any(|&n| n > limits.max_texture_array_layers)
        || size.iter().any(|&x| x > limits.max_texture_dimension_2d)
    {
        return None;
    }
    let path = path.to_owned();
    let ctx = gpu.context().clone();
    let kinds: Vec<_> = list.iter().map(|c| (c.kind, c.slice)).collect();
    Some(Arc::new(FootageChannels::new(
        list,
        attributes,
        size,
        move || {
            // Read again: keeping every frame's samples on the CPU until an
            // effect might ask would cost more than the occasional re-read.
            let image = exr::prelude::read_all_flat_layers_from_file(&path).ok()?;
            let display = image.attributes.display_window.size;
            if [display.0 as u32, display.1 as u32] != size {
                return None;
            }
            let array = |format, layers: u32| {
                let [w, h] = if layers == 0 { [1, 1] } else { size };
                ctx.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("bae.media.exr_channels"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: layers.max(1),
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
            };
            let words = array(wgpu::TextureFormat::R32Uint, counts[0]);
            let halves = array(wgpu::TextureFormat::R16Uint, counts[1]);
            let total = size[0] as usize * size[1] as usize;
            for (&(part, idx), &(kind, slice)) in sources.iter().zip(&kinds) {
                let ch = image.layer_data.get(part)?.channel_data.list.get(idx)?;
                let (tex, bytes) = match (kind, &ch.sample_data) {
                    (ChannelKind::Half, FlatSamples::F16(_)) => {
                        let mut plane = vec![0u16; total];
                        for_each_sample(&image, part, ch, |i, s| {
                            if let Sample::F16(h) = s {
                                plane[i] = h.to_bits();
                            }
                        });
                        (&halves, bytemuck::cast_slice(&plane).to_vec())
                    }
                    (ChannelKind::Float, FlatSamples::F32(_))
                    | (ChannelKind::Uint, FlatSamples::U32(_)) => {
                        let mut plane = vec![0u32; total];
                        for_each_sample(&image, part, ch, |i, s| {
                            plane[i] = match s {
                                Sample::U32(n) => n,
                                _ => s.to_f32().to_bits(),
                            };
                        });
                        (&words, bytemuck::cast_slice(&plane).to_vec())
                    }
                    _ => return None,
                };
                let mut region = transfer::TextureRegion::full(tex);
                region.origin.z = slice;
                transfer::upload_texture(&ctx, tex, region, &bytes).ok()?;
            }
            let view = |tex: &wgpu::Texture| {
                tex.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                })
            };
            Some(ChannelPlanes {
                words: view(&words),
                halves: view(&halves),
                bytes: total as u64 * (u64::from(counts[0]) * 4 + u64::from(counts[1]) * 2),
            })
        },
    )))
}
