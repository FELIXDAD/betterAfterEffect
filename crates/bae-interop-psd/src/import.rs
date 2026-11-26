//! Metadata passed from the file-reading worker to the undoable import command.
use crate::{
    format::{
        self, BlendMode, LayerNode, LayerRecord, PsdFile, VersionedDescriptor,
        descriptor::Value as DValue, path::VectorMaskBlock,
    },
    layer_key,
};
use bae_model::{BezierPath, TextDocument, Value, psd::LayerKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Document {
    pub size: [u32; 2],
    pub merged: bool,
    /// Bottom to top, as stored by Photoshop.
    pub layers: Vec<Layer>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Layer {
    pub key: LayerKey,
    pub name: String,
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub visible: bool,
    pub opacity: f64,
    pub pixel_mask: Option<(bool, bool, f64)>,
    pub blend: String,
    pub clipped: bool,
    pub children: Option<Vec<Layer>>,
    pub text: Option<(TextDocument, [f64; 2])>,
    pub masks: Vec<Mask>,
    pub styles: BTreeMap<u32, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Mask {
    pub shape: BezierPath,
    pub mode: String,
    pub inverted: bool,
}

pub fn describe(file: &PsdFile) -> format::Result<Document> {
    let size = [file.header.width, file.header.height];
    let mut layers = Vec::new();
    for node in file.layer_tree_max_depth(128) {
        layers.push(describe_node(file, &node, size)?);
    }
    Ok(Document {
        size,
        merged: file.has_real_merged_data() != Some(false),
        layers,
    })
}

impl Document {
    pub fn leaves(&self) -> Vec<&Layer> {
        fn visit<'a>(layers: &'a [Layer], out: &mut Vec<&'a Layer>) {
            for l in layers.iter().rev() {
                if let Some(children) = &l.children {
                    visit(children, out)
                } else {
                    out.push(l)
                }
            }
        }
        let mut leaves = Vec::new();
        visit(&self.layers, &mut leaves);
        leaves
    }
    /// Photoshop IDs survive renaming and stacking changes. Older files may only
    /// have record indices, so match their previous name before trusting an index.
    pub fn replacement(&self, key: LayerKey, name: Option<&str>) -> Option<LayerKey> {
        let layers = self.leaves();
        let same = layers.iter().find(|l| l.key == key);
        if matches!(key, LayerKey::Id(_)) && same.is_some() {
            return Some(key);
        }
        let mut named = layers.iter().filter(|l| Some(l.name.as_str()) == name);
        if let Some(first) = named.next()
            && named.next().is_none()
        {
            return Some(first.key);
        }
        same.filter(|l| Some(l.name.as_str()) == name)
            .map(|l| l.key)
    }
    pub fn find(&self, key: LayerKey) -> Option<&Layer> {
        self.leaves().into_iter().find(|l| l.key == key)
    }
}

fn describe_node(file: &PsdFile, node: &LayerNode, size: [u32; 2]) -> format::Result<Layer> {
    let index = node.index();
    let rec = &file.layers()[index];
    let (w, h) = rec.rect.size()?;
    let mut children: Option<Vec<Layer>> = None;
    if node.is_group() {
        children = Some(
            node.children()
                .iter()
                .map(|n| describe_node(file, n, size))
                .collect::<format::Result<_>>()?,
        );
    }
    let mode = rec
        .section_divider()
        .and_then(|d| d.blend_mode)
        .unwrap_or(rec.blend_mode);
    let masks = vector_masks(rec, size)?;
    let styles = crate::styles::read(rec, file)?;
    let text = read_text(rec).filter(|_| masks.is_empty() && rec.layer_mask().is_none());
    let mut origin = [rec.rect.left, rec.rect.top];
    let mut extent = [w.max(1) as u32, h.max(1) as u32];
    if let Some(kids) = &children
        && let Some(first) = kids.first()
    {
        origin = first.origin;
        let mut end = [i64::MIN; 2];
        for layer in kids {
            for i in 0..2 {
                origin[i] = origin[i].min(layer.origin[i]);
                end[i] = end[i].max(i64::from(layer.origin[i]) + i64::from(layer.size[i]));
            }
        }
        for i in 0..2 {
            extent[i] = u32::try_from(end[i] - i64::from(origin[i]))
                .map_err(|_| format::PsdError::LimitExceeded("group bounds"))?;
        }
    }
    Ok(Layer {
        key: layer_key(rec, index),
        name: rec.name(),
        origin,
        size: extent,
        visible: rec.is_visible(),
        opacity: f64::from(rec.opacity) * 100. / 255.,
        pixel_mask: rec
            .layer_mask()
            .filter(|_| rec.channel(-2).is_some())
            .map(|m| {
                (
                    m.flags & 2 == 0,
                    m.flags & 4 != 0,
                    f64::from(
                        m.parameters
                            .as_ref()
                            .and_then(|p| p.user_density)
                            .unwrap_or(255),
                    ) * 100.
                        / 255.,
                )
            }),
        blend: token(mode).into(),
        clipped: rec.clipping != 0,
        children,
        text,
        masks,
        styles,
    })
}

fn token(mode: BlendMode) -> &'static str {
    use BlendMode::*;
    match mode {
        PassThrough | Normal | Unknown(_) => "normal",
        Dissolve => "dissolve",
        Darken => "darken",
        Multiply => "multiply",
        ColorBurn => "color_burn",
        LinearBurn => "linear_burn",
        DarkerColor => "darker_color",
        Lighten => "lighten",
        Screen => "screen",
        ColorDodge => "color_dodge",
        LinearDodge => "linear_dodge",
        LighterColor => "lighter_color",
        Overlay => "overlay",
        SoftLight => "soft_light",
        HardLight => "hard_light",
        VividLight => "vivid_light",
        LinearLight => "linear_light",
        PinLight => "pin_light",
        HardMix => "hard_mix",
        Difference => "difference",
        Exclusion => "exclusion",
        Subtract => "subtract",
        Divide => "divide",
        Hue => "hue",
        Saturation => "saturation",
        Color => "color",
        Luminosity => "luminosity",
    }
}

fn vector_masks(rec: &LayerRecord, size: [u32; 2]) -> format::Result<Vec<Mask>> {
    let Some(block) = rec.block(b"vmsk").or_else(|| rec.block(b"vsms")) else {
        return Ok(vec![]);
    };
    let mask = VectorMaskBlock::from_bytes(&block.data)?;
    Ok(mask
        .path
        .subpaths()
        .into_iter()
        .filter(|p| p.knots.len() >= 2)
        .map(|path| {
            let mut shape = BezierPath {
                closed: path.closed,
                ..Default::default()
            };
            let px = |(x, y): (f64, f64)| [x * f64::from(size[0]), y * f64::from(size[1])];
            for k in path.knots {
                let pos = px(k.anchor);
                let tangent = |p| {
                    let p = px(p);
                    [p[0] - pos[0], p[1] - pos[1]]
                };
                shape.vertices.push(pos);
                shape.in_tangents.push(tangent(k.pre));
                shape.out_tangents.push(tangent(k.post));
            }
            let mode = if mask.flags & 4 != 0 || !shape.closed {
                "none"
            } else {
                match path.operation {
                    2 => "subtract",
                    3 => "intersect",
                    0 => "difference",
                    _ => "add",
                }
            };
            Mask {
                shape,
                mode: mode.into(),
                inverted: mask.flags & 1 != 0,
            }
        })
        .collect())
}

fn read_text(rec: &LayerRecord) -> Option<(TextDocument, [f64; 2])> {
    use effectcraft_psd::engine_data::{self, EValue};
    let bytes = &rec.block(b"TySh")?.data;
    let mut t = [0.; 6];
    for (i, chunk) in bytes.get(2..50)?.as_chunks::<8>().0.iter().enumerate() {
        t[i] = f64::from_be_bytes(*chunk);
    }
    if t[..4] != [1., 0., 0., 1.] {
        return None;
    }
    let (text, used) = VersionedDescriptor::parse_prefix(bytes.get(52..)?).ok()?;
    let (warp, _) = VersionedDescriptor::parse_prefix(bytes.get(52 + used + 2..)?).ok()?;
    if let Some(DValue::Enumerated { value, .. }) = warp.descriptor.get("warpStyle")
        && !value.is("warpNone")
    {
        return None;
    }
    let DValue::Text(content) = text.descriptor.get("Txt ")? else {
        return None;
    };
    let DValue::RawData(data) = text.descriptor.get("EngineData")? else {
        return None;
    };
    let engine = engine_data::parse(data);
    let style = engine_data::text_style(&engine);
    if style.box_text
        || matches!(engine.path("EngineDict/StyleRun/RunArray"),Some(EValue::Array(runs)) if runs.len()>1)
    {
        return None;
    }
    let mut doc = TextDocument::new(
        content
            .to_string_lossy()
            .trim_end_matches('\r')
            .replace('\r', "\n"),
        style.font?,
        style.size?,
    );
    if let Some([r, g, b]) = style.color {
        doc.fill_color = [r, g, b, 1.];
    }
    doc.justification = match style.justification {
        Some(1) => "right",
        Some(2) => "center",
        _ => "left",
    }
    .into();
    doc.leading = style.leading;
    doc.tracking = style.tracking.unwrap_or_default();
    doc.faux_bold = style.faux_bold;
    doc.faux_italic = style.faux_italic;
    Some((doc, [t[4], t[5]]))
}
