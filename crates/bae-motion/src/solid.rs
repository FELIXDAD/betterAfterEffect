//! Solid commands: `solid.set_settings` (AE Layer > Solid Settings,
//! Cmd/Ctrl+Shift+Y, and Solid Settings of a solid item in the Project panel).
//!
//! New solids come from `layer.create` (`kind: "solid"`), which creates the solid
//! item in the Solids folder and a layer that uses it.

use bae_model::{CommandError, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, SOLIDS_FOLDER};
use crate::util::{child, create, ensure_folder, fail, set, write_attr};

/// A solid item's settings.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct SolidInfo {
    /// The solid item (an `Asset` of kind `solid`).
    pub path: PrimPath,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    /// Straight RGBA 0–1.
    pub color: [f64; 4],
}

/// Settings of the solid item at `item`, or `None` when it is not one.
pub fn solid_info(stage: &bae_model::Stage, item: &PrimPath) -> Option<SolidInfo> {
    let view = stage.prim(item)?;
    if !view.is_a(&PrimType::ASSET)
        || query::static_value(&view, "kind") != Some(Value::Enum("solid".into()))
    {
        return None;
    }
    let num = |a: &str| query::static_value(&view, a).and_then(|v| v.as_f64());
    let color = match query::static_value(&view, "solid_color") {
        Some(Value::Color(c)) => c,
        _ => [0.5, 0.5, 0.5, 1.0],
    };
    Some(SolidInfo {
        path: item.clone(),
        name: query::display_name(&view),
        width: num("width").unwrap_or(0.0).max(0.0) as u32,
        height: num("height").unwrap_or(0.0).max(0.0) as u32,
        pixel_aspect: num("pixel_aspect").filter(|a| *a > 0.0).unwrap_or(1.0),
        color,
    })
}

/// The solid item a layer uses (solid and adjustment layers), if any.
pub fn layer_solid(stage: &bae_model::Stage, layer: &PrimPath) -> Option<PrimPath> {
    if !query::is_layer(stage, layer) {
        return None;
    }
    let view = stage.prim(layer)?;
    match query::static_value(&view, "source") {
        Some(Value::PrimRef(src)) if solid_info(stage, &src).is_some() => Some(src),
        _ => None,
    }
}

/// Every layer, in every comp, that uses the solid item `item`.
pub fn solid_users(stage: &bae_model::Stage, item: &PrimPath) -> Vec<PrimPath> {
    query::comps(stage)
        .iter()
        .flat_map(|c| query::comp_layers(stage, c))
        .filter(|l| layer_solid(stage, l).as_ref() == Some(item))
        .collect()
}

pub struct SetSolidSettings;

#[derive(Deserialize, JsonSchema)]
pub struct SolidSettingsParams {
    /// A layer that uses a solid (solid or adjustment layer), or a solid item.
    pub target: PrimPath,
    /// The solid's name; layers still showing the old name follow it.
    #[serde(default)]
    pub name: Option<String>,
    /// Size in pixels, 1–30000.
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// Pixel aspect ratio, 0.01–100.
    #[serde(default)]
    pub pixel_aspect: Option<f64>,
    /// Straight RGBA 0–1.
    #[serde(default)]
    pub color: Option<[f64; 4]>,
    /// Layer targets only: when other layers use the same solid, change it for
    /// all of them (true) or give this layer its own changed copy (false,
    /// AE's default). A solid item target always changes every layer.
    #[serde(default)]
    pub affect_all: bool,
}

/// Output of `solid.set_settings`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SolidOutput {
    /// The solid item that now has the settings (a new one when it was forked).
    pub solid: PrimPath,
    /// The layers that changed.
    pub layers: Vec<PrimPath>,
}

impl bae_model::Command for SetSolidSettings {
    type Params = SolidSettingsParams;
    type Output = SolidOutput;
    const ID: &'static str = "solid.set_settings";
    const TITLE: &'static str = "Solid Settings";
    const DESCRIPTION: &'static str = "Change a solid's name, size, pixel aspect ratio and color (AE Solid Settings, Cmd/Ctrl+Shift+Y). Target a solid or adjustment layer, or a solid item. For a layer whose solid other layers use too, affect_all=false gives the layer its own copy; true changes the shared solid. Anchor points scale with the size, so the solid stays in place.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<SolidOutput, CommandError> {
        let stage = tx.stage();
        let (item, layer) = if let Some(info) = solid_info(stage, &p.target) {
            (info.path, None)
        } else if let Some(item) = layer_solid(stage, &p.target) {
            (item, Some(p.target.clone()))
        } else {
            return Err(fail(format!(
                "{} is neither a solid item nor a layer that uses one",
                p.target
            )));
        };
        let old = solid_info(stage, &item).expect("checked above");
        let name = match p.name.as_deref().map(str::trim) {
            Some("") => return Err(fail("the solid name cannot be empty")),
            Some(n) => n.to_owned(),
            None => old.name.clone(),
        };
        let width = p.width.unwrap_or(old.width);
        let height = p.height.unwrap_or(old.height);
        if !(1..=30_000).contains(&width) || !(1..=30_000).contains(&height) {
            return Err(fail("solid size must be between 1 and 30000 pixels"));
        }
        let pixel_aspect = p.pixel_aspect.unwrap_or(old.pixel_aspect);
        if !(pixel_aspect.is_finite() && (0.01..=100.0).contains(&pixel_aspect)) {
            return Err(fail("pixel aspect ratio must be between 0.01 and 100"));
        }
        let color = p.color.unwrap_or(old.color);
        if color.iter().any(|c| !c.is_finite()) {
            return Err(fail("color channels must be numbers"));
        }
        let color = color.map(|c| c.clamp(0.0, 1.0));

        let users = solid_users(stage, &item);
        let fork = matches!(&layer, Some(l) if !p.affect_all && users.iter().any(|u| u != l));
        let (solid, affected) = if fork {
            let layer = layer.expect("fork needs a layer");
            let solids = PrimPath::new(SOLIDS_FOLDER).expect("valid");
            ensure_folder(tx, &solids)?;
            let copy = child(
                &solids,
                &query::unique_child_name(tx.stage(), &solids, &name),
            )?;
            create(tx, &copy, PrimType::ASSET, None)?;
            set(tx, &copy, "kind", Value::Enum("solid".into()))?;
            if let Some(label) = tx
                .stage()
                .prim(&item)
                .and_then(|v| query::static_value(&v, "label"))
            {
                set(tx, &copy, "label", label)?;
            }
            set(tx, &layer, "source", Value::PrimRef(copy.clone()))?;
            (copy, vec![layer])
        } else {
            (item, users)
        };
        set(tx, &solid, "name", Value::String(name.clone()))?;
        set(tx, &solid, "width", Value::Int(i64::from(width)))?;
        set(tx, &solid, "height", Value::Int(i64::from(height)))?;
        set(tx, &solid, "pixel_aspect", Value::Float(pixel_aspect))?;
        set(tx, &solid, "solid_color", Value::Color(color))?;

        // The layers follow: a name that was the solid's follows the new name,
        // and anchor points scale with the size (AE: [200,150] on 400x300
        // becomes [300,250] on 600x500), so the solid stays in place.
        let (nw, ow) = (f64::from(width), f64::from(old.width.max(1)));
        let (nh, oh) = (f64::from(height), f64::from(old.height.max(1)));
        tx.allow_locked(|tx| -> Result<(), CommandError> {
            for l in &affected {
                let layer_name = tx.stage().prim(l).map(|v| query::display_name(&v));
                if name != old.name && layer_name.as_deref() == Some(old.name.as_str()) {
                    set(tx, l, "name", Value::String(name.clone()))?;
                }
                if (nw, nh) == (ow, oh) {
                    continue;
                }
                const ANCHOR: &str = "transform:anchor_point";
                let Some(mut spec) = query::attr_spec(tx.stage(), l, ANCHOR).cloned() else {
                    continue;
                };
                let scale = |v: &Value| match v {
                    Value::Vec3([x, y, z]) => Value::Vec3([x * nw / ow, y * nh / oh, *z]),
                    Value::Vec2([x, y]) => Value::Vec2([x * nw / ow, y * nh / oh]),
                    other => other.clone(),
                };
                if let Some(v) = &mut spec.value {
                    *v = scale(v);
                }
                for k in spec.keyframes.values_mut() {
                    k.value = scale(&k.value);
                }
                write_attr(tx, l, ANCHOR, spec)?;
            }
            Ok(())
        })?;
        Ok(SolidOutput {
            solid,
            layers: affected,
        })
    }
}
