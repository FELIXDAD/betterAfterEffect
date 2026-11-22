//! `layer.precompose`: AE's Layer > Pre-compose (Cmd/Ctrl+Shift+C), with both
//! of its options (docs/spec/ae AE-PRJ-401/402):
//!
//! - **Move all attributes** into the new composition: the selected layers
//!   move into a new comp of the original comp's size, frame rate and
//!   duration (or their time span with `adjust_duration`), and one pre-comp
//!   layer takes the topmost one's place.
//! - **Leave all attributes** in the comp (one layer with a source: solid,
//!   adjustment, footage, pre-comp): a new comp of the layer's size holds the
//!   source; the layer keeps its transforms, masks, effects and keyframes and
//!   uses the new comp as its source.
//!
//! Links that would cross the comps afterwards (parents, track mattes, layer
//! references) are removed. One undo step.

use std::collections::BTreeMap;

use bae_model::{Command, CommandError, LayerKind, Op, PrimPath, Stage, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::comp::{CreateComp, CreateCompParams};
use crate::layer::{
    CreateLayer, CreateLayerParams, SetParent, SetParentParams, ShiftLayers, ShiftLayersParams,
};
use crate::query;
use crate::util::{ensure_unlocked, fail, set, write_attr};

/// How the selected layers are pre-composed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrecomposeMode {
    /// Move all attributes into the new composition (several layers, any kind).
    #[default]
    Move,
    /// Leave all attributes in the comp (one layer with a source).
    Leave,
}

pub struct Precompose;

#[derive(Deserialize, JsonSchema)]
pub struct PrecomposeParams {
    /// Layers of one comp.
    pub layers: Vec<PrimPath>,
    /// The new comp's name (default: "<layer> Comp N" for one layer, "Pre-comp N").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mode: PrecomposeMode,
    /// Move only: the new comp lasts from the earliest in point to the latest
    /// out point of the layers, and the pre-comp layer starts there.
    #[serde(default)]
    pub adjust_duration: bool,
    /// Comp time (flicks) whose transforms are kept when a parent link across
    /// the comps is removed (default 0; the UI passes the current time).
    #[serde(default)]
    pub time: Option<Time>,
}

/// Output of `layer.precompose`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PrecomposeOutput {
    /// The new comp.
    pub comp: PrimPath,
    /// The layer in the original comp that now shows it.
    pub layer: PrimPath,
}

/// Whether `layer` can be pre-composed leaving its attributes (AE 26: one
/// footage, solid or pre-comp layer; not shape, text, null, camera, light or
/// adjustment layers).
pub fn can_leave_attributes(stage: &Stage, layer: &PrimPath) -> bool {
    let kind = query::layer_kind(stage, layer);
    matches!(kind.as_deref(), Some("solid" | "footage" | "pre_comp"))
        && source_of(stage, layer).is_some()
}

fn source_of(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    match stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "source"))
    {
        Some(Value::PrimRef(p)) => Some(p),
        _ => None,
    }
}

/// AE's default name for the new comp: "<layer name> Comp N" for one layer,
/// "Pre-comp N" for several (N: the first number no comp uses yet).
pub fn default_precomp_name(stage: &Stage, layers: &[PrimPath]) -> String {
    // AE names them in the UI language: "<layer> 合成 1", "预合成 1".
    let base = match layers {
        [one] => format!(
            "{} {}",
            query::display_name_of(stage, one),
            crate::names::localized("Comp")
        ),
        _ => crate::names::localized("Pre-comp").to_owned(),
    };
    let taken = |n: &str| {
        query::comps(stage)
            .iter()
            .any(|c| query::display_name_of(stage, c) == n)
    };
    (1..)
        .map(|n| format!("{base} {n}"))
        .find(|n| !taken(n))
        .expect("unbounded")
}

/// The comp-relative timing of a layer: (in point, out point).
fn span(stage: &Stage, layer: &PrimPath) -> (Time, Time) {
    let t = query::layer_timing(stage, layer);
    (t.in_point, t.out_point)
}

/// Clear (or retarget) every `PrimRef` in the subtree at `path` whose target
/// is under one of `from`: retarget to the matching path in `moved` when the
/// target moved along, otherwise remove the link.
fn fix_refs(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    affected: &[PrimPath],
    moved: &BTreeMap<PrimPath, PrimPath>,
) -> Result<(), CommandError> {
    let mut stack = vec![path.clone()];
    while let Some(p) = stack.pop() {
        stack.extend(tx.stage().children(&p).iter().cloned());
        let Some(view) = tx.stage().prim(&p) else {
            continue;
        };
        let mut edits = Vec::new();
        for name in view.attr_names() {
            let Some(spec) = view.attr(name) else {
                continue;
            };
            let mut spec = spec.clone();
            let mut changed = false;
            let mut fix = |v: &mut Option<Value>| {
                if let Some(Value::PrimRef(t)) = v.as_ref() {
                    let Some(base) = affected.iter().find(|a| a.is_prefix_of(t)) else {
                        return;
                    };
                    match moved.get(base).and_then(|to| t.reparent(base, to)) {
                        Some(n) => *v = Some(Value::PrimRef(n)),
                        None => *v = None,
                    }
                    changed = true;
                }
            };
            fix(&mut spec.value);
            for k in spec.keyframes.values_mut() {
                let mut v = Some(k.value.clone());
                fix(&mut v);
                if let Some(v) = v {
                    k.value = v;
                }
            }
            if changed {
                edits.push((name.to_owned(), spec));
            }
        }
        for (name, spec) in edits {
            write_attr(tx, &p, &name, spec)?;
        }
    }
    Ok(())
}

impl Command for Precompose {
    type Params = PrecomposeParams;
    type Output = PrecomposeOutput;
    const ID: &'static str = "layer.precompose";
    const TITLE: &'static str = "Pre-compose";
    const DESCRIPTION: &'static str = "Pre-compose layers (AE Layer > Pre-compose, Cmd/Ctrl+Shift+C). mode=move puts the layers into a new comp (original comp's size, frame rate, duration; or their time span with adjust_duration) shown by one pre-comp layer at the topmost layer's place. mode=leave (one solid, footage or pre-comp layer) puts the layer's source into a new comp of the layer's size and makes it the layer's source, keeping the layer's attributes. Parent, track matte and other links that would cross comps are removed.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<PrecomposeOutput, CommandError> {
        if p.layers.is_empty() {
            return Err(fail("no layers given"));
        }
        ensure_unlocked(tx.stage(), &p.layers)?;
        let stage = tx.stage();
        let mut comp: Option<PrimPath> = None;
        for l in &p.layers {
            if !query::is_layer(stage, l) {
                return Err(fail(format!("{l} is not a layer")));
            }
            let c = query::owning_comp(stage, l)
                .ok_or_else(|| fail(format!("{l} is not in a comp")))?;
            if comp.as_ref().is_some_and(|prev| prev != &c) {
                return Err(fail("layers must belong to the same comp"));
            }
            comp = Some(c);
        }
        let comp = comp.expect("not empty");
        let info = query::comp_info(stage, &comp).expect("owning comp");
        // Stack order (top first) of the selected layers.
        let all = query::comp_layers(stage, &comp);
        let mut layers: Vec<(usize, PrimPath)> = p
            .layers
            .iter()
            .filter_map(|l| all.iter().position(|a| a == l).map(|i| (i, l.clone())))
            .collect();
        layers.sort();
        layers.dedup();
        let name = match p.name.as_deref().map(str::trim) {
            Some("") => return Err(fail("the comp name cannot be empty")),
            Some(n) => n.to_owned(),
            None => default_precomp_name(stage, &p.layers),
        };
        let folder = comp.parent();
        let new_comp = |tx: &mut Transaction<'_>, width, height, duration| {
            CreateComp.run(
                tx,
                CreateCompParams {
                    name: Some(name.clone()),
                    parent: folder.clone(),
                    width,
                    height,
                    fps: info.frame_rate.as_f64(),
                    duration: Some(duration),
                    bg_color: None,
                    pixel_aspect: Some(info.pixel_aspect),
                    drop_frame: None,
                    resolution: None,
                    start_timecode: None,
                    shutter_angle: None,
                    shutter_phase: None,
                    samples_per_frame: None,
                    adaptive_sample_limit: None,
                    preserve_frame_rate: None,
                    preserve_resolution: None,
                    renderer: None,
                    label: None,
                },
            )
        };

        if p.mode == PrecomposeMode::Leave {
            let [(_, layer)] = layers.as_slice() else {
                return Err(fail("leave all attributes needs exactly one layer"));
            };
            if !can_leave_attributes(tx.stage(), layer) {
                return Err(fail(
                    "leave all attributes needs a solid, footage or pre-comp layer",
                ));
            }
            let source = source_of(tx.stage(), layer).expect("checked");
            let (size, kind) = match query::comp_info(tx.stage(), &source) {
                Some(c) => ([c.width, c.height], LayerKind::PreComp),
                None => {
                    let view = tx.stage().prim(&source);
                    let num = |a: &str| {
                        view.as_ref()
                            .and_then(|v| query::static_value(v, a))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                    };
                    let kind = match view.as_ref().and_then(|v| query::static_value(v, "kind")) {
                        Some(Value::Enum(k)) if k == "solid" => LayerKind::Solid,
                        _ => LayerKind::Footage,
                    };
                    (
                        [num("width").max(1.0) as u32, num("height").max(1.0) as u32],
                        kind,
                    )
                }
            };
            let created = new_comp(tx, size[0], size[1], info.duration)?;
            // AE: the inner layer starts at 0 and spans the new comp; the
            // layer's own timing stays on the layer outside.
            CreateLayer.run(
                tx,
                CreateLayerParams::with_source(&created.path, kind, &source, Time::ZERO),
            )?;
            // The layer now shows the new comp; a name that was its source's
            // becomes the new comp's.
            let layer_name = query::display_name_of(tx.stage(), layer);
            if layer_name == query::display_name_of(tx.stage(), &source) {
                set(tx, layer, "name", Value::String(name.clone()))?;
            }
            set(tx, layer, "source", Value::PrimRef(created.path.clone()))?;
            if query::layer_kind(tx.stage(), layer).as_deref() != Some("adjustment") {
                set(tx, layer, "kind", Value::Enum("pre_comp".into()))?;
            }
            return Ok(PrecomposeOutput {
                comp: created.path,
                layer: layer.clone(),
            });
        }

        // Move all attributes.
        let (s, e) = layers
            .iter()
            .map(|(_, l)| span(tx.stage(), l))
            .fold((Time(i64::MAX), Time(i64::MIN)), |(s, e), (a, b)| {
                (s.min(a), e.max(b))
            });
        let duration = if p.adjust_duration && e > s {
            e - s
        } else {
            info.duration
        };
        // A parent link that will cross the comps is removed first, keeping the
        // layer where it is on screen (AE bakes the parent's transform in:
        // parent at [300,200], child at [100,50] → [300,150] inside).
        let moving: Vec<PrimPath> = layers.iter().map(|(_, l)| l.clone()).collect();
        let time = p.time.unwrap_or(Time::ZERO);
        for l in query::comp_layers(tx.stage(), &comp) {
            let parent = match tx
                .stage()
                .prim(&l)
                .and_then(|v| query::static_value(&v, "parent"))
            {
                Some(Value::PrimRef(p)) => p,
                _ => continue,
            };
            if moving.contains(&l) != moving.contains(&parent) {
                tx.allow_locked(|tx| {
                    SetParent.run(
                        tx,
                        SetParentParams {
                            layers: vec![l.clone()],
                            parent: None,
                            keep_transform: true,
                            time,
                        },
                    )
                })?;
            }
        }
        // An unset out point means "the comp's end": pin it before the layers
        // move into a comp of another duration.
        for (_, l) in &layers {
            let authored = tx
                .stage()
                .prim(l)
                .and_then(|v| v.value("out_point"))
                .is_some();
            if !authored {
                let out = span(tx.stage(), l).1;
                set(tx, l, "out_point", Value::Time(out))?;
            }
        }
        let created = new_comp(tx, info.width, info.height, duration)?;
        let top = layers[0].0;
        let mut moved = BTreeMap::new();
        for (i, (_, layer)) in layers.iter().enumerate() {
            let to = created
                .layers
                .child(layer.name())
                .map_err(|e| fail(e.to_string()))?;
            tx.apply(Op::MovePrim {
                from: layer.clone(),
                to: to.clone(),
                index: Some(i),
            })?;
            if tx.stage().has_prim(layer) {
                return Err(fail(format!(
                    "{layer} has opinions in another sublayer and cannot move"
                )));
            }
            moved.insert(layer.clone(), to);
        }
        let affected: Vec<PrimPath> = moved.keys().cloned().collect();
        // Links inside the new comp follow their targets; links across the
        // boundary are removed (both directions).
        for to in moved.values() {
            fix_refs(tx, to, &affected, &moved)?;
        }
        let outside: Vec<PrimPath> = query::comp_layers(tx.stage(), &comp);
        let moved_new: Vec<PrimPath> = moved.values().cloned().collect();
        for l in &outside {
            fix_refs(tx, l, &affected, &BTreeMap::new())?;
        }
        for to in &moved_new {
            let outside_refs: Vec<PrimPath> = outside.clone();
            fix_refs(tx, to, &outside_refs, &BTreeMap::new())?;
        }
        if p.adjust_duration && e > s && s != Time::ZERO {
            ShiftLayers.run(
                tx,
                ShiftLayersParams {
                    layers: moved_new.clone(),
                    delta: Time(-s.0),
                },
            )?;
        }
        let start = if p.adjust_duration { s } else { Time::ZERO };
        let mut params =
            CreateLayerParams::with_source(&comp, LayerKind::PreComp, &created.path, start);
        params.index = Some(top);
        params.name = Some(name);
        let out = CreateLayer.run(tx, params)?;
        Ok(PrecomposeOutput {
            comp: created.path,
            layer: out.path,
        })
    }
}
