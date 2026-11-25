//! File ▸ New Comp from Selection… (AE, Alt+\): one comp with the selected
//! Project items as layers, or one comp per item; Sequence Layers puts them one
//! after another, optionally overlapping with a dissolve. One undo step.

use bae_model::{Command, CommandError, LayerKind, PrimPath, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::comp::{CompOutput, CreateComp};
use crate::keys::{SetAnimated, SetAnimatedParams, SetAt, SetAtParams};
use crate::layer::CreateLayer;
use crate::query;
use crate::util::fail;

pub struct CompFromItems;

/// Sequence Layers ▸ Transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    /// Off.
    #[default]
    Off,
    /// Dissolve Front Layer: each layer fades out over the overlap.
    DissolveFront,
    /// Cross Dissolve Front and Back Layers: the next one fades in as well.
    CrossDissolve,
}

#[derive(Deserialize, JsonSchema)]
pub struct CompFromItemsParams {
    /// Project items (footage, solids, comps) in the order they become layers
    /// (the first on top, and first in a sequence).
    pub items: Vec<PrimPath>,
    /// Multiple Compositions: one comp per item (default: a single comp).
    #[serde(default)]
    pub multiple: bool,
    /// Single comp: the item whose size, pixel aspect and frame rate it takes
    /// and whose name it gets (default the first item).
    #[serde(default)]
    pub dimensions_from: Option<PrimPath>,
    /// Length of stills and solids (default 5 seconds).
    #[serde(default)]
    pub still_duration: Option<Time>,
    /// Frame rate when the item has none (stills; default 30).
    #[serde(default)]
    pub fps: Option<f64>,
    /// Sequence Layers (single comp).
    #[serde(default)]
    pub sequence: bool,
    /// Sequence Layers ▸ Overlap: how long consecutive layers overlap.
    #[serde(default)]
    pub overlap: Option<Time>,
    #[serde(default)]
    pub transition: Transition,
    /// Label of the new comps (the UI passes the Preferences > Labels default).
    #[serde(default)]
    pub label: Option<i64>,
}

#[derive(Serialize, JsonSchema)]
pub struct CompFromItemsOutput {
    pub comps: Vec<PrimPath>,
}

/// What a Project item brings to a comp.
struct Item {
    path: PrimPath,
    name: String,
    size: [u32; 2],
    pixel_aspect: f64,
    fps: Option<f64>,
    /// Its own length (movies, sequences, comps); `None` for stills.
    duration: Option<Time>,
    kind: LayerKind,
}

fn item(tx: &Transaction<'_>, path: &PrimPath) -> Result<Item, CommandError> {
    let stage = tx.stage();
    let name = query::display_name_of(stage, path);
    if let Some(c) = query::comp_info(stage, path) {
        return Ok(Item {
            path: path.clone(),
            name,
            size: [c.width, c.height],
            pixel_aspect: c.pixel_aspect,
            fps: Some(c.frame_rate.as_f64()),
            duration: Some(c.duration),
            kind: LayerKind::PreComp,
        });
    }
    let view = stage
        .prim(path)
        .filter(|v| v.is_a(&bae_model::PrimType::ASSET))
        .ok_or_else(|| fail(format!("{path} is not a comp or a footage item")))?;
    let get = |a: &str| query::static_value(&view, a);
    let int = |a: &str| get(a).and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    let kind = match get("kind") {
        Some(Value::Enum(k)) if k == "solid" => LayerKind::Solid,
        _ if get("has_video").and_then(|v| v.as_bool()) == Some(false)
            && get("has_audio").and_then(|v| v.as_bool()) == Some(true) =>
        {
            LayerKind::Audio
        }
        _ => LayerKind::Footage,
    };
    let duration = match get("duration") {
        Some(Value::Time(t)) if t > Time::ZERO => Some(t),
        _ => None,
    };
    let fps = match get("frame_rate") {
        Some(Value::FrameRate(r)) if duration.is_some() => Some(r.as_f64()),
        _ => None,
    };
    Ok(Item {
        path: path.clone(),
        name,
        size: [int("width").max(1), int("height").max(1)],
        pixel_aspect: get("pixel_aspect").and_then(|v| v.as_f64()).unwrap_or(1.0),
        fps,
        duration,
        kind,
    })
}

impl Command for CompFromItems {
    type Params = CompFromItemsParams;
    type Output = CompFromItemsOutput;
    const ID: &'static str = "comp.create_from_items";
    const TITLE: &'static str = "New Comp from Selection";
    const DESCRIPTION: &'static str = "AE File > New Comp from Selection (Alt+\\): a comp with the given Project items as layers (size, pixel aspect and frame rate from `dimensions_from`), or one comp per item (`multiple`); `sequence` places the layers one after another, `overlap` and `transition` (off, dissolve_front, cross_dissolve) fade between them. One undo step.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<CompFromItemsOutput, CommandError> {
        if p.items.is_empty() {
            return Err(fail("no items given"));
        }
        let items = p
            .items
            .iter()
            .map(|i| item(tx, i))
            .collect::<Result<Vec<_>, _>>()?;
        let still = p.still_duration.unwrap_or(Time::from_seconds(5.0));
        let fps = |i: &Item| i.fps.or(p.fps).unwrap_or(30.0);
        let length = |i: &Item| i.duration.unwrap_or(still);
        let groups: Vec<Vec<&Item>> = if p.multiple {
            items.iter().map(|i| vec![i]).collect()
        } else {
            vec![items.iter().collect()]
        };
        let mut comps = Vec::new();
        for group in groups {
            let dims = match (&p.dimensions_from, p.multiple) {
                (Some(d), false) => group.iter().find(|i| i.path == *d).copied(),
                _ => None,
            }
            .unwrap_or(group[0]);
            let rate = bae_model::FrameRate::from_fps(fps(dims));
            let overlap = if p.sequence && !p.multiple {
                p.overlap.unwrap_or(Time::ZERO).max(Time::ZERO)
            } else {
                Time::ZERO
            };
            // Start of each layer, and the comp's length.
            let mut starts = Vec::with_capacity(group.len());
            let mut end = Time::ZERO;
            let mut t = Time::ZERO;
            for i in &group {
                let start = if p.sequence { t } else { Time::ZERO };
                starts.push(start);
                end = end.max(start + length(i));
                t = (start + length(i) - overlap).max(start + rate.frame_to_time(1));
            }
            let params = serde_json::from_value(json!({
                "name": dims.name,
                "width": dims.size[0],
                "height": dims.size[1],
                "pixel_aspect": dims.pixel_aspect,
                "fps": rate.as_f64(),
                "duration": end.max(rate.frame_to_time(1)),
                "label": p.label,
            }))
            .map_err(|e| fail(e.to_string()))?;
            let CompOutput { path: comp, .. } = CreateComp.run(tx, params)?;
            for (n, (i, start)) in group.iter().zip(&starts).enumerate() {
                let params = serde_json::from_value(json!({
                    "comp": comp,
                    "kind": i.kind,
                    "source": i.path,
                    "name": i.name,
                    "index": n,
                    "start": start,
                }))
                .map_err(|e| fail(e.to_string()))?;
                let layer = CreateLayer.run(tx, params)?.path;
                let out = *start + length(i);
                crate::util::set(tx, &layer, "out_point", Value::Time(out))?;
                if !p.sequence || overlap == Time::ZERO || p.transition == Transition::Off {
                    continue;
                }
                let key = |tx: &Transaction<'_>, t: Time| query::to_key_time(tx.stage(), &layer, t);
                let opacity = "transform:opacity".to_owned();
                // The front layer fades out at its end (not the last one).
                if n + 1 < group.len() {
                    let (a, b) = (key(tx, out - overlap), key(tx, out));
                    fade(tx, &layer, &opacity, a, b, 100.0, 0.0)?;
                }
                // Cross dissolve: the back layer fades in at its start.
                if p.transition == Transition::CrossDissolve && n > 0 {
                    let (a, b) = (key(tx, *start), key(tx, *start + overlap));
                    fade(tx, &layer, &opacity, a, b, 0.0, 100.0)?;
                }
            }
            comps.push(comp);
        }
        Ok(CompFromItemsOutput { comps })
    }
}

/// Opacity keyframes `from` at key time `a` and `to` at `b`.
fn fade(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    attr: &str,
    a: Time,
    b: Time,
    from: f64,
    to: f64,
) -> Result<(), CommandError> {
    let animated = query::attr_spec(tx.stage(), layer, attr).is_some_and(|s| s.is_animated());
    if !animated {
        SetAnimated.run(
            tx,
            SetAnimatedParams {
                path: layer.clone(),
                attr: attr.to_owned(),
                animated: true,
                time: a,
            },
        )?;
    }
    for (time, value) in [(a, from), (b, to)] {
        SetAt.run(
            tx,
            SetAtParams {
                path: layer.clone(),
                attr: attr.to_owned(),
                time,
                value: Value::Float(value),
            },
        )?;
    }
    Ok(())
}
