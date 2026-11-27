//! Translate ActionDescriptor keys to the shared native layer-style controls.
use crate::format::{
    Descriptor, LayerRecord, PsdFile, ResourceData, Result, VersionedDescriptor,
    descriptor::Value as D, resources::ids,
};
use bae_model::{
    Value,
    layer_styles::{Control, Field, GROUPS},
};
use std::collections::BTreeMap;

pub(crate) fn read(layer: &LayerRecord, file: &PsdFile) -> Result<BTreeMap<u32, Value>> {
    let mut attrs = BTreeMap::new();
    let block = layer.block(b"lmfx").or_else(|| layer.block(b"lfx2"));
    let (all_ranges, _) = layer.blending_ranges.data.as_chunks::<8>();
    let ranges = &all_ranges[..all_ranges.len().min(4)];
    if ranges
        .iter()
        .any(|r| *r != [0, 0, 255, 255, 0, 0, 255, 255])
    {
        attrs.insert(
            9,
            Value::FloatList(
                ranges
                    .iter()
                    .flatten()
                    .map(|v| f64::from(*v) / 255.)
                    .collect(),
            ),
        );
    }
    if attrs.is_empty() && block.is_none() && layer.fill_opacity() == 255 {
        return Ok(attrs);
    }
    let desc = block
        .map(|b| VersionedDescriptor::parse_prefix(b.data.get(4..).unwrap_or_default()))
        .transpose()?
        .map(|(d, _)| d.descriptor);
    for group in GROUPS {
        if group.token == "blendingOptions" {
            attrs.insert(group.enabled, Value::Bool(true));
            for f in group.fields {
                let value = match f.match_name {
                    "ADBE Layer Fill Opacity2" => {
                        Value::Float(f64::from(layer.fill_opacity()) * 100. / 255.)
                    }
                    "ADBE Global Angle2" | "ADBE Global Altitude2" => {
                        let id = if f.match_name == "ADBE Global Angle2" {
                            ids::GLOBAL_ANGLE
                        } else {
                            ids::GLOBAL_ALTITUDE
                        };
                        match file
                            .resource(id)
                            .and_then(|r| r.parsed())
                            .and_then(Result::ok)
                        {
                            Some(
                                ResourceData::GlobalAngle(v) | ResourceData::GlobalAltitude(v),
                            ) => Value::Float(f64::from(v)),
                            _ => f.initial(),
                        }
                    }
                    _ => f.initial(),
                };
                attrs.insert(f.id, value);
            }
            continue;
        }
        let Some(root) = desc.as_ref() else { continue };
        let (key, multi) = match group.token {
            "dropShadow" => ("DrSh", "dropShadowMulti"),
            "innerShadow" => ("IrSh", "innerShadowMulti"),
            "outerGlow" => ("OrGl", "outerGlowMulti"),
            "innerGlow" => ("IrGl", "innerGlowMulti"),
            "bevelEmboss" => ("ebbl", "bevelEmbossMulti"),
            "chromeFX" => ("ChFX", "chromeFXMulti"),
            "solidFill" => ("SoFi", "solidFillMulti"),
            "gradientFill" => ("GrFl", "gradientFillMulti"),
            "frameFX" => ("FrFX", "frameFXMulti"),
            _ => continue,
        };
        let effect = match root.get(multi) {
            Some(D::List(list)) => list.iter().filter_map(obj).find(|d| on(d, "enab")),
            _ => root
                .get(key)
                .or_else(|| root.get(group.token))
                .and_then(obj),
        };
        let Some(effect) = effect else { continue };
        attrs.insert(
            group.enabled,
            Value::Bool(on(root, "masterFXSwitch") && on(effect, "enab")),
        );
        for field in group.fields {
            attrs.insert(
                field.id,
                field_value(field, effect).unwrap_or_else(|| field.initial()),
            );
        }
    }
    Ok(attrs)
}
fn on(d: &Descriptor, k: &str) -> bool {
    !matches!(d.get(k), Some(D::Boolean(false)))
}
fn obj(v: &D) -> Option<&Descriptor> {
    match v {
        D::Descriptor(d) | D::GlobalObject(d) => Some(d),
        _ => None,
    }
}
fn num(v: &D) -> Option<f64> {
    match v {
        D::Integer(n) => Some(f64::from(*n)),
        D::Double(n) | D::UnitFloat { value: n, .. } => Some(*n),
        _ => None,
    }
}
fn number(d: &Descriptor, k: &str) -> Option<f64> {
    num(d.get(k)?)
}
fn rgb(d: &Descriptor) -> Option<[f64; 4]> {
    Some([
        number(d, "Rd  ")? / 255.,
        number(d, "Grn ")? / 255.,
        number(d, "Bl  ")? / 255.,
        1.,
    ])
}
fn menu(id: &str) -> &str {
    match id {
        "Nrml" => "Normal",
        "Dslv" => "Dissolve",
        "Drkn" => "Darken",
        "Mltp" => "Multiply",
        "CBrn" => "Color Burn",
        "linearBurn" => "Linear Burn",
        "darkerColor" => "Darker Color",
        "Scrn" => "Screen",
        "CDdg" => "Color Dodge",
        "linearDodge" => "Linear Dodge",
        "lighterColor" => "Lighter Color",
        "Ovrl" => "Overlay",
        "SftL" => "Soft Light",
        "HrdL" => "Hard Light",
        "linearLight" => "Linear Light",
        "vividLight" => "Vivid Light",
        "pinLight" => "Pin Light",
        "hardMix" => "Hard Mix",
        "Dfrn" => "Difference",
        "Xclu" => "Exclusion",
        "blendSubtraction" => "Subtract",
        "blendDivide" => "Divide",
        "H   " => "Hue",
        "Strt" => "Saturation",
        "Clr " => "Color",
        "Lmns" => "Luminosity",
        "SfBL" => "Softer",
        "PrBL" => "Precise",
        "SrcE" => "Edge",
        "SrcC" => "Center",
        "OtrB" => "Outer Bevel",
        "InrB" => "Inner Bevel",
        "Embs" => "Emboss",
        "PlEb" => "Pillow Emboss",
        "strokeEmboss" => "Stroke Emboss",
        "Sfbl" => "Smooth",
        "ChlH" => "Chisel Hard",
        "Slmt" | "ChlS" => "Chisel Soft",
        "In  " => "Up",
        "Out " => "Down",
        "Lnr " => "Linear",
        "Rdl " => "Radial",
        "Angl" => "Angle",
        "Rflc" => "Reflected",
        "Dmnd" => "Diamond",
        "OutF" => "Outside",
        "InsF" => "Inside",
        "CtrF" => "Center",
        _ => id,
    }
}
fn field_value(f: &Field, d: &Descriptor) -> Option<Value> {
    let (_, name) = f.match_name.split_once('/')?;
    let key = match name {
        "mode2" => "Md  ",
        "color" => "Clr ",
        "opacity" => "Opct",
        "useGlobalAngle" => "uglg",
        "localLightingAngle" => "lagl",
        "distance" => "Dstn",
        "chokeMatte" => "Ckmt",
        "blur" => "blur",
        "noise" => "Nose",
        "layerConceals" => "layerConceals",
        "glowTechnique" => "GlwT",
        "innerGlowSource" => "glwS",
        "inputRange" => "Inpr",
        "shadingNoise" => "ShdN",
        "bevelStyle" => "bvlS",
        "bevelTechnique" => "bvlT",
        "strengthRatio" => "srgR",
        "bevelDirection" => "bvlD",
        "softness" => "Sftn",
        "localLightingAltitude" => "Lald",
        "highlightMode" => "hglM",
        "highlightColor" => "hglC",
        "highlightOpacity" => "hglO",
        "shadowMode" => "sdwM",
        "shadowColor" => "sdwC",
        "shadowOpacity" => "sdwO",
        "invert" => "Invr",
        "angle" => "Angl",
        "type" => "Type",
        "reverse" => "Rvrs",
        "align" => "Algn",
        "scale" => "Scl ",
        "offset" => "Ofst",
        "size" => "Sz  ",
        "style" => "Styl",
        "gradient" => "Grad",
        "gradientSmoothness" => {
            return Some(Value::Float(
                number(obj(d.get("Grad")?)?, "Intr")? * 100. / 4096.,
            ));
        }
        "AEColorChoice" => return Some(Value::Int(if d.get("Grad").is_some() { 2 } else { 1 })),
        _ => return None,
    };
    let v = d.get(key).or_else(|| d.get(name))?;
    Some(match f.kind {
        Control::Number(..) | Control::Angle(_) => Value::Float(num(v)?),
        Control::Switch(_) => {
            let D::Boolean(b) = v else { return None };
            Value::Bool(*b)
        }
        Control::Color(_) => Value::Color(rgb(obj(v)?)?),
        Control::Menu(_, items) => {
            let D::Enumerated { value, .. } = v else {
                return None;
            };
            let id = std::str::from_utf8(value.as_bytes()).ok()?;
            let label = match (name, id) {
                ("bevelTechnique", "SfBL") => "Smooth",
                ("bevelTechnique", "PrBL") => "Chisel Hard",
                _ => menu(id),
            };
            Value::Int(items.iter().position(|s| *s == label)? as i64 + 1)
        }
        Control::Point => {
            let p = obj(v)?;
            Value::Vec2([number(p, "Hrzn")?, number(p, "Vrtc")?])
        }
        Control::Gradient => Value::FloatList(gradient(obj(v)?)?),
    })
}
fn gradient(d: &Descriptor) -> Option<Vec<f64>> {
    let stops = |key| match d.get(key) {
        Some(D::List(v)) => Some(v.iter().filter_map(obj)),
        _ => None,
    };
    let mut colors = stops("Clrs")?
        .filter_map(|s| Some((number(s, "Lctn")? / 4096., rgb(obj(s.get("Clr ")?)?)?)))
        .collect::<Vec<_>>();
    let mut alpha = stops("Trns")
        .into_iter()
        .flatten()
        .filter_map(|s| Some((number(s, "Lctn")? / 4096., number(s, "Opct")? / 100.)))
        .collect::<Vec<_>>();
    colors.sort_by(|a, b| a.0.total_cmp(&b.0));
    alpha.sort_by(|a, b| a.0.total_cmp(&b.0));
    if colors.is_empty() {
        return None;
    }
    let mut positions: Vec<_> = colors
        .iter()
        .map(|s| s.0)
        .chain(alpha.iter().map(|s| s.0))
        .collect();
    positions.sort_by(f64::total_cmp);
    positions.dedup();
    fn interpolate(stops: &[(f64, f64)], x: f64) -> f64 {
        if stops.is_empty() {
            return 1.;
        }
        let i = stops.partition_point(|s| s.0 < x);
        if i == 0 {
            return stops[0].1;
        }
        if i == stops.len() {
            return stops[i - 1].1;
        }
        let (a, l) = stops[i - 1];
        let (b, r) = stops[i];
        l + (r - l) * (x - a) / (b - a)
    }
    let rgb: [Vec<_>; 3] =
        std::array::from_fn(|i| colors.iter().map(|(x, c)| (*x, c[i])).collect());
    Some(
        positions
            .into_iter()
            .flat_map(|x| {
                [
                    x,
                    interpolate(&rgb[0], x),
                    interpolate(&rgb[1], x),
                    interpolate(&rgb[2], x),
                    interpolate(&alpha, x),
                ]
            })
            .collect(),
    )
}
