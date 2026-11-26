//! PSD and PSB footage. Source channels stay at their original precision until upload.
use bae_model::psd::{LayerImage, LayerKey, PixelMask};
use format::{ColorMode, LayerRecord, PsdError, PsdFile, Rect, Result};
pub use rust_motion_psd as format;
use std::borrow::Cow;
pub mod import;
mod styles;

pub fn layer_key(layer: &LayerRecord, index: usize) -> LayerKey {
    layer
        .layer_id()
        .map_or(LayerKey::Index(index as u32), LayerKey::Id)
}

pub fn resolve(file: &PsdFile, key: LayerKey) -> Result<&LayerRecord> {
    let layer = match key {
        LayerKey::Id(id) => file
            .layers()
            .iter()
            .find(|layer| layer.layer_id() == Some(id)),
        LayerKey::Index(i) => file.layers().get(i as usize),
    };
    layer.ok_or_else(|| PsdError::Invalid(format!("missing PSD layer {key:?}")))
}

pub fn bounds(file: &PsdFile, selection: Option<LayerImage>) -> Result<Rect> {
    let canvas = Rect::from_xywh(0, 0, file.header.width, file.header.height);
    match selection {
        Some(sel) => {
            let layer = resolve(file, sel.key)?;
            if let PixelMask::Group { origin, size, .. } = sel.mask {
                let end = |v: i32, n: u32| {
                    i32::try_from(i64::from(v) + i64::from(n))
                        .map_err(|_| PsdError::LimitExceeded("group mask bounds"))
                };
                Ok(Rect {
                    left: origin[0],
                    top: origin[1],
                    right: end(origin[0], size[0])?,
                    bottom: end(origin[1], size[1])?,
                })
            } else if sel.canvas {
                Ok(canvas)
            } else {
                let (w, h) = layer.rect.size()?;
                Ok(Rect::from_xywh(
                    layer.rect.left,
                    layer.rect.top,
                    w.max(1) as u32,
                    h.max(1) as u32,
                ))
            }
        }
        None => Ok(canvas),
    }
}

struct RasterMask {
    channel: i16,
    rect: Rect,
    flags: u8,
    background: u8,
    density: u8,
}

/// When Photoshop saves a rendered mask and its editable inputs, channel -3 is
/// the original user mask. Applying the rendered copy as well would mask twice.
fn raster_mask(layer: &LayerRecord) -> Option<RasterMask> {
    let m = layer.layer_mask()?;
    let (channel, rect, flags, background) = if let Some(real) = m.real
        && layer.channel(-3).is_some()
    {
        (-3, real.rect, real.flags, real.background)
    } else {
        layer.channel(-2)?;
        if m.flags & 8 != 0 && (layer.block(b"vmsk").is_some() || layer.block(b"vsms").is_some()) {
            return None;
        }
        (-2, m.rect, m.flags, m.default_color)
    };
    let density = m
        .parameters
        .as_ref()
        .and_then(|p| p.user_density)
        .unwrap_or(255);
    Some(RasterMask {
        channel,
        rect,
        flags,
        background,
        density,
    })
}
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<[f32; 4]>,
}

fn sample(plane: &[u8], depth: u16, i: usize) -> f32 {
    match depth {
        8 => f32::from(plane[i]) / 255.,
        16 => f32::from(u16::from_be_bytes([plane[2 * i], plane[2 * i + 1]])) / 65535.,
        _ => f32::from_be_bytes(
            plane[4 * i..4 * i + 4]
                .try_into()
                .expect("validated sample size"),
        ),
    }
}

/// Decode the saved composite or a single layer. Opacity, blending, vector masks
/// and layer effects belong to the composition and are not baked into these pixels.
pub fn decode(file: &PsdFile, selection: Option<LayerImage>) -> Result<Pixels> {
    let header = &file.header;
    let depth = header.depth;
    if !matches!(depth, 8 | 16 | 32) {
        return Err(PsdError::Unsupported(format!("{depth}-bit PSD")));
    }
    let colors: usize = match header.color_mode {
        ColorMode::Rgb => 3,
        ColorMode::Grayscale | ColorMode::Indexed => 1,
        ColorMode::Cmyk => 4,
        mode => return Err(PsdError::Unsupported(format!("PSD color mode {mode:?}"))),
    };
    let output = bounds(file, selection)?;
    let (ow, oh) = output.size()?;
    let count = ow
        .checked_mul(oh)
        .filter(|n| *n <= (1 << 27))
        .ok_or(PsdError::LimitExceeded("PSD output pixels"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(count)
        .map_err(|_| PsdError::LimitExceeded("PSD output pixels"))?;
    rgba.resize(count, [0.; 4]);
    let layer = selection.map(|s| resolve(file, s.key)).transpose()?;
    let mask_setting = selection.map_or(PixelMask::AsSaved, |s| s.mask);
    let only_mask = matches!(mask_setting, PixelMask::Group { .. });
    let src = if only_mask {
        output
    } else {
        layer.map_or(output, |l| l.rect)
    };
    let (sw, sh) = src.size()?;
    let n = sw
        .checked_mul(sh)
        .filter(|n| *n <= (1 << 27))
        .ok_or(PsdError::LimitExceeded("PSD layer pixels"))?;
    let bytes = n * usize::from(depth / 8);
    let merged;
    let mut planes = Vec::new();
    let mut mask = None;
    if let Some(l) = layer {
        if !only_mask && n != 0 && l.channel(0).is_some() {
            for channel in 0..colors {
                planes.push(Cow::Owned(l.decode_channel(
                    channel as i16,
                    depth,
                    header.version,
                )?));
            }
            if l.channel(-1).is_some() {
                planes.push(Cow::Owned(l.decode_channel(-1, depth, header.version)?));
            }
        }
        if let Some(m) = raster_mask(l)
            && !matches!(mask_setting, PixelMask::Disabled)
            && (m.flags & 2 == 0
                || matches!(
                    mask_setting,
                    PixelMask::Channel { .. } | PixelMask::Group { .. }
                ))
        {
            let (mw, mh) = m.rect.size()?;
            let data = l.decode_channel(m.channel, depth, header.version)?;
            if data.len() != mw * mh * usize::from(depth / 8) {
                return Err(PsdError::Invalid("PSD mask size".into()));
            }
            mask = Some((m, mw, data));
        }
    } else {
        if file.has_real_merged_data() == Some(false) {
            return Err(PsdError::Invalid("document has no saved composite".into()));
        }
        merged = file.decode_merged()?;
        planes.extend(
            merged
                .chunks_exact(bytes)
                .take(colors + usize::from(file.merged_has_alpha()))
                .map(Cow::Borrowed),
        );
    }
    if only_mask || !planes.is_empty() {
        if !only_mask && (planes.len() < colors || planes.iter().any(|p| p.len() != bytes)) {
            return Err(PsdError::Invalid("PSD channel size".into()));
        }
        let x0 = src.left.max(output.left);
        let x1 = src.right.min(output.right);
        for y in src.top.max(output.top)..src.bottom.min(output.bottom) {
            for x in x0..x1 {
                let i = (y - src.top) as usize * sw + (x - src.left) as usize;
                let v = |c: usize| sample(&planes[c], depth, i);
                let mut a = if planes.len() > colors { v(colors) } else { 1. };
                if let Some((m, mw, data)) = &mask {
                    let strength = if x >= m.rect.left
                        && x < m.rect.right
                        && y >= m.rect.top
                        && y < m.rect.bottom
                    {
                        sample(
                            data,
                            depth,
                            (y - m.rect.top) as usize * mw + (x - m.rect.left) as usize,
                        )
                    } else {
                        f32::from(m.background) / 255.
                    };
                    let (inverted, density) = match mask_setting {
                        PixelMask::Channel { inverted, density }
                        | PixelMask::Group {
                            inverted, density, ..
                        } => (inverted, f32::from(density) / 65535.),
                        _ => (m.flags & 4 != 0, f32::from(m.density) / 255.),
                    };
                    let value = if inverted { 1. - strength } else { strength };
                    a *= 1. - (1. - value) * density;
                }
                let dest = (y - output.top) as usize * ow + (x - output.left) as usize;
                if only_mask {
                    rgba[dest] = [1., 1., 1., a];
                    continue;
                }
                let rgb = match header.color_mode {
                    ColorMode::Rgb => [v(0), v(1), v(2)],
                    ColorMode::Cmyk => [v(0) * v(3), v(1) * v(3), v(2) * v(3)],
                    ColorMode::Indexed => {
                        let index = planes[0][i] as usize;
                        if file.color_mode_data.len() < 768 {
                            return Err(PsdError::Invalid("PSD palette".into()));
                        }
                        [0, 256, 512]
                            .map(|offset| f32::from(file.color_mode_data[offset + index]) / 255.)
                    }
                    _ => [v(0); 3],
                };
                // Photoshop stores its transparent RGB composite matted against
                // white at every depth; 32-bit keeps values above 1.
                let [r, g, b] =
                    if layer.is_none() && header.color_mode == ColorMode::Rgb && a > 0. && a < 1. {
                        let top = if depth == 32 { f32::MAX } else { 1. };
                        rgb.map(|v| ((v - (1. - a)) / a).clamp(0., top))
                    } else {
                        rgb
                    };
                rgba[dest] = [r, g, b, a];
            }
        }
    }
    Ok(Pixels {
        width: ow as u32,
        height: oh as u32,
        rgba,
    })
}
