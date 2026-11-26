//! Decode all flat parts together. Channel words bypass the beauty image's half conversion.
use super::*;
use bae_gpu::{
    channels::{Channel, ChannelInfo, FootageChannels},
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
    let total = width
        .checked_mul(height)
        .ok_or_else(|| MediaError::decode(path, "display window too large"))?;
    let rgba_len = total
        .checked_mul(4)
        .ok_or_else(|| MediaError::decode(path, "display window too large"))?;
    let mut rgba = vec![f16::ZERO; rgba_len];
    let beauty = image
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
        .min_by_key(|g| g.0);
    let count: usize = image
        .layer_data
        .iter()
        .map(|l| l.channel_data.list.len())
        .sum();
    let size = [width as u32, height as u32];
    let texture = if let Some(gpu) = gpu {
        let limits = gpu.context().device().limits();
        if count == 0
            || count > limits.max_texture_array_layers as usize
            || size.iter().any(|&x| x > limits.max_texture_dimension_2d)
        {
            return Err(MediaError::decode(
                path,
                "EXR exceeds GPU texture dimensions or array layer count",
            ));
        }
        Some(
            gpu.context()
                .device()
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("bae.media.exr_channels"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: count as u32,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R32Uint,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                }),
        )
    } else {
        None
    };
    let mut channels = Vec::with_capacity(count);
    let mut attributes = BTreeMap::new();
    let mut first_ids = BTreeMap::new();
    for (part, layer) in image.layer_data.iter().enumerate() {
        let pos = layer.attributes.layer_position;
        let dx = i64::from(pos.0) - i64::from(display.position.0);
        let dy = i64::from(pos.1) - i64::from(display.position.1);
        for (key, attr) in image.attributes.other.iter().chain(&layer.attributes.other) {
            let key = key.to_string();
            if key.starts_with("cryptomatte/")
                && let AttributeValue::Text(value) = attr
            {
                attributes.insert(key, value.to_string());
            }
        }
        for (idx, ch) in layer.channel_data.list.iter().enumerate() {
            let dest = beauty.and_then(|(_, p, ids)| {
                (p == part)
                    .then(|| ids.iter().position(|&i| i == Some(idx)))
                    .flatten()
            });
            if texture.is_none() && dest.is_none() {
                continue;
            }
            if ch.sampling != exr::math::Vec2(1, 1) || ch.sample_data.len() != layer.size.area() {
                return Err(MediaError::decode(
                    path,
                    "subsampled EXR channels are unsupported",
                ));
            }
            let mut words = texture.as_ref().map(|_| vec![0u32; total]);
            for y in (-dy).max(0)..(height as i64 - dy).min(layer.size.1 as i64) {
                for x in (-dx).max(0)..(width as i64 - dx).min(layer.size.0 as i64) {
                    let i = (y + dy) as usize * width + (x + dx) as usize;
                    let sample = ch
                        .sample_data
                        .value_by_flat_index(y as usize * layer.size.0 + x as usize);
                    if let Some(c) = dest {
                        rgba[i * 4 + c] = f16::from_f32(sample.to_f32());
                        if c == 0 && beauty.is_some_and(|(_, _, ids)| ids[3].is_none()) {
                            rgba[i * 4 + 3] = f16::ONE;
                        }
                    }
                    if let Some(words) = &mut words {
                        words[i] = match sample {
                            Sample::U32(n) => n,
                            _ => sample.to_f32().to_bits(),
                        };
                    }
                }
            }
            if let (Some(gpu), Some(tex), Some(words)) = (gpu, &texture, words) {
                let mut region = transfer::TextureRegion::full(tex);
                region.origin.z = channels.len() as u32;
                transfer::upload_texture(gpu.context(), tex, region, bytemuck::cast_slice(&words))?;
                let name = match &layer.attributes.layer_name {
                    Some(part_name) => format!("{part_name}.{}", ch.name),
                    None => ch.name.to_string(),
                };
                if let Some((group, component)) = name.rsplit_once('.')
                    && (component.eq_ignore_ascii_case("r")
                        || component.eq_ignore_ascii_case("red"))
                    && let Some(prefix) = group.strip_suffix("00")
                {
                    first_ids.insert(prefix.to_owned(), words);
                }
                channels.push(Channel {
                    name,
                    unsigned: matches!(ch.sample_data, FlatSamples::U32(_)),
                });
            }
        }
    }
    let channels = texture.map(|tex| {
        Arc::new(FootageChannels {
            view: tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
            info: Arc::new(ChannelInfo {
                crypto: crypto_layers(path, &attributes, &channels, first_ids),
                channels,
                size,
            }),
        })
    });
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

/// Build each rank table once at decode time, including optional sidecar manifests.
fn crypto_layers(
    path: &Path,
    attributes: &BTreeMap<String, String>,
    channels: &[Channel],
    mut first_ids: BTreeMap<String, Vec<u32>>,
) -> Vec<bae_gpu::channels::CryptoLayer> {
    let mut result = Vec::new();
    for (key, name) in attributes {
        let Some(root) = key.strip_suffix("/name") else {
            continue;
        };
        let Some(prefix) = first_ids
            .keys()
            .find(|s| *s == name || s.strip_suffix(name).is_some_and(|q| q.ends_with('.')))
            .cloned()
        else {
            continue;
        };
        let mut groups: BTreeMap<u8, [i32; 4]> = BTreeMap::new();
        for (index, channel) in channels.iter().enumerate() {
            let Some((pass, component)) = channel.name.rsplit_once('.') else {
                continue;
            };
            let Some(rank) = pass
                .strip_prefix(&prefix)
                .filter(|s| s.len() == 2)
                .and_then(|s| s.parse::<u8>().ok())
            else {
                continue;
            };
            let lane = match component.to_ascii_lowercase().as_str() {
                "r" | "red" => 0,
                "g" | "green" => 1,
                "b" | "blue" => 2,
                "a" | "alpha" => 3,
                _ => continue,
            };
            groups.entry(rank).or_insert([-1; 4])[lane] = index as i32;
        }
        let ranks = groups
            .into_values()
            .filter(|ids| ids[0] >= 0 && ids[1] >= 0)
            .collect::<Vec<_>>();
        if ranks.is_empty() {
            continue;
        }
        let sidecar = || {
            let filename = attributes.get(&format!("{root}/manif_file"))?;
            let base = path.parent()?.canonicalize().ok()?;
            let manifest = base.join(filename).canonicalize().ok()?;
            if !manifest.starts_with(&base) {
                return None;
            }
            std::fs::read_to_string(manifest).ok()
        };
        let manifest = attributes
            .get(&format!("{root}/manifest"))
            .cloned()
            .or_else(sidecar)
            .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(name, hex)| {
                let mut id = u32::from_str_radix(&hex, 16).ok()?;
                if matches!((id >> 23) & 255, 0 | 255) {
                    id ^= 1 << 23;
                }
                Some((id, name))
            })
            .collect();
        result.push(bae_gpu::channels::CryptoLayer {
            name: name.clone(),
            ranks,
            manifest,
            dominant: first_ids.remove(&prefix).unwrap(),
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}
