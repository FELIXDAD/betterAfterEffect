//! Layer commands: `layer.create`, `layer.reorder`, `layer.duplicate`, `layer.split`,
//! `layer.trim`, `layer.set_parent`, `layer.set_attr`.

use bae_anim::glam::{DMat4, DVec3};
use bae_model::{
    AttrSpec, Command, CommandError, LayerKind, ModelError, Op, PrimPath, PrimType, Stage,
    TextDocument, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, SOLIDS_FOLDER};
use crate::util::{
    child, composed_tree, create, ensure_folder, ensure_unlocked, fail, retarget_refs, set,
    write_attr,
};

/// Output of commands that create one layer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LayerOutput {
    pub path: PrimPath,
}

/// Output of commands that create several layers.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LayersOutput {
    pub paths: Vec<PrimPath>,
}

/// A parametric shape added to a new shape layer (like drawing with AE's shape tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShapePreset {
    Rect,
    Ellipse,
    Star,
}

/// AE default label colors per layer kind (index into the label palette).
fn default_label(kind: LayerKind) -> i64 {
    match kind {
        LayerKind::Shape => 8,
        LayerKind::Text | LayerKind::Solid | LayerKind::Null => 1,
        LayerKind::Camera => 4,
        LayerKind::Light => 6,
        LayerKind::Adjustment => 5,
        LayerKind::PreComp => 15,
        LayerKind::Footage => 3,
        LayerKind::Audio => 7,
    }
}

fn default_base_name(kind: LayerKind) -> &'static str {
    match kind {
        LayerKind::Shape => "Shape Layer",
        LayerKind::Text => "Text",
        LayerKind::Solid => "Solid",
        LayerKind::Null => "Null",
        LayerKind::Camera => "Camera",
        LayerKind::Light => "Light",
        LayerKind::Adjustment => "Adjustment Layer",
        LayerKind::PreComp => "Pre-comp",
        LayerKind::Footage => "Footage",
        LayerKind::Audio => "Audio",
    }
}

// ---------------------------------------------------------------------------

pub struct CreateLayer;

#[derive(Deserialize, JsonSchema)]
pub struct CreateLayerParams {
    /// The comp to add the layer to.
    pub comp: PrimPath,
    /// shape, text, solid, null, camera, light, adjustment, pre_comp, footage or audio.
    pub kind: LayerKind,
    /// Display name (default AE-style: "Shape Layer 1", "Solid 1", the text, the source name …).
    #[serde(default)]
    pub name: Option<String>,
    /// Comp (pre_comp) or Asset (footage, audio) providing the content; required for those kinds.
    #[serde(default)]
    pub source: Option<PrimPath>,
    /// Position in the layer stack, 0 = top (default 0, like AE).
    #[serde(default)]
    pub index: Option<usize>,
    /// Solid color, shape fill or text fill, straight RGBA 0–1.
    #[serde(default)]
    pub color: Option<[f64; 4]>,
    /// Solid size in pixels (default the comp size).
    #[serde(default)]
    pub size: Option<[u32; 2]>,
    /// Solid pixel aspect ratio (default the comp's).
    #[serde(default)]
    pub pixel_aspect: Option<f64>,
    /// Text layers: the text (default "Text").
    #[serde(default)]
    pub text: Option<String>,
    /// Text layers: font size in pixels (default 72).
    #[serde(default)]
    pub font_size: Option<f64>,
    /// Shape layers: add a rectangle, ellipse or star with a fill.
    #[serde(default)]
    pub shape: Option<ShapePreset>,
    /// Position in comp pixels (default the comp center).
    #[serde(default)]
    pub position: Option<[f64; 2]>,
    /// Comp time (flicks) where the layer starts: its start time and in point (default 0).
    #[serde(default)]
    pub start: Option<Time>,
    /// Camera layers: Camera Settings (type, zoom, depth of field …).
    #[serde(default)]
    pub camera: Option<crate::camera::CameraSettings>,
    /// Light layers: Light Settings (type, color, intensity …).
    #[serde(default)]
    pub light: Option<crate::camera::LightSettings>,
    /// Label color index (default AE's per kind; the UI passes the
    /// Preferences > Labels default).
    #[serde(default)]
    pub label: Option<i64>,
}

impl CreateLayerParams {
    /// A layer of `kind` in `comp` showing `source` (a comp or an asset),
    /// starting at comp time `start`; everything else default.
    pub fn with_source(comp: &PrimPath, kind: LayerKind, source: &PrimPath, start: Time) -> Self {
        CreateLayerParams {
            comp: comp.clone(),
            kind,
            name: None,
            source: Some(source.clone()),
            index: None,
            color: None,
            size: None,
            pixel_aspect: None,
            text: None,
            font_size: None,
            shape: None,
            position: None,
            start: Some(start),
            camera: None,
            light: None,
            label: None,
        }
    }
}

fn source_size_and_duration(stage: &Stage, source: &PrimPath) -> Option<([f64; 2], Option<Time>)> {
    if let Some(info) = query::comp_info(stage, source) {
        return Some((
            [f64::from(info.width), f64::from(info.height)],
            Some(info.duration),
        ));
    }
    let view = stage.prim(source)?;
    if !view.is_a(&PrimType::ASSET) {
        return None;
    }
    let num = |a: &str| query::static_value(&view, a).and_then(|v| v.as_f64());
    let size = [num("width").unwrap_or(0.0), num("height").unwrap_or(0.0)];
    let duration = query::static_value(&view, "duration")
        .and_then(|v| v.as_time())
        .filter(|d| d.0 > 0);
    Some((size, duration))
}

impl Command for CreateLayer {
    type Params = CreateLayerParams;
    type Output = LayerOutput;
    const ID: &'static str = "layer.create";
    const TITLE: &'static str = "New Layer";
    const DESCRIPTION: &'static str = "Add a layer to a comp (shape, text, solid, null, camera, light, adjustment, pre-comp, footage) with AE defaults: on top, anchored at its center and centered in the comp.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<LayerOutput, CommandError> {
        let info = query::comp_info(tx.stage(), &p.comp)
            .ok_or_else(|| fail(format!("{} is not a comp", p.comp)))?;
        let folder = query::layers_folder(&p.comp);
        if !tx.stage().has_prim(&folder) {
            create(tx, &folder, PrimType::FOLDER, None)?;
        }
        let kind = p.kind;
        let needs_source = matches!(
            kind,
            LayerKind::PreComp | LayerKind::Footage | LayerKind::Audio
        );
        let source_info = match &p.source {
            Some(src) => Some(
                source_size_and_duration(tx.stage(), src)
                    .ok_or_else(|| fail(format!("{src} is not a comp or an asset")))?,
            ),
            None if needs_source => {
                return Err(fail(format!("{} layers need a source", kind.as_str())));
            }
            None => None,
        };
        if let (LayerKind::PreComp, Some(src)) = (kind, &p.source)
            && (src == &p.comp || query::comp_info(tx.stage(), src).is_none())
        {
            return Err(fail("a pre-comp layer's source must be another comp"));
        }
        let name = match p.name.filter(|n| !n.trim().is_empty()) {
            Some(n) => n,
            None => match (kind, &p.source, &p.text) {
                (LayerKind::Text, _, Some(t)) if !t.trim().is_empty() => t
                    .lines()
                    .next()
                    .unwrap_or(crate::names::localized("Text"))
                    .chars()
                    .take(64)
                    .collect(),
                (_, Some(src), _) => query::display_name_of(tx.stage(), src),
                _ => query::next_numbered_name(
                    tx.stage(),
                    &folder,
                    crate::names::localized(default_base_name(kind)),
                ),
            },
        };
        let path = child(
            &folder,
            &query::unique_child_name(tx.stage(), &folder, &name),
        )?;
        let index = p.index.unwrap_or(0);
        let siblings = tx
            .stage()
            .sublayer(tx.target())
            .map_or(0, |s| s.children(&folder).len());
        create(tx, &path, PrimType::LAYER, Some(index.min(siblings)))?;
        set(tx, &path, "kind", Value::Enum(kind.as_str().to_owned()))?;
        set(tx, &path, "name", Value::String(name.clone()))?;
        let label = p
            .label
            .filter(|l| (0..=16).contains(l))
            .unwrap_or(default_label(kind));
        set(tx, &path, "label", Value::Int(label))?;
        let start = p.start.unwrap_or(Time::ZERO);
        if start != Time::ZERO {
            set(tx, &path, "start_time", Value::Time(start))?;
            set(tx, &path, "in_point", Value::Time(start))?;
        }

        let (w, h) = (f64::from(info.width), f64::from(info.height));
        let pos = p.position.unwrap_or([w / 2.0, h / 2.0]);
        let mut anchor = [0.0, 0.0];
        let mut position = [pos[0], pos[1], 0.0];

        match kind {
            LayerKind::Solid | LayerKind::Adjustment if p.source.is_some() => {
                // An existing solid item (dragged from the project panel).
                let (size, _) = source_info.expect("checked above");
                set(
                    tx,
                    &path,
                    "source",
                    Value::PrimRef(p.source.clone().expect("some")),
                )?;
                anchor = [size[0] / 2.0, size[1] / 2.0];
                if kind == LayerKind::Adjustment {
                    set(tx, &path, "adjustment_layer", Value::Bool(true))?;
                }
            }
            LayerKind::Solid | LayerKind::Adjustment => {
                let [sw, sh] = p.size.unwrap_or([info.width, info.height]);
                if sw == 0 || sh == 0 {
                    return Err(fail("solid size must be positive"));
                }
                let color = p.color.unwrap_or(if kind == LayerKind::Adjustment {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.5, 0.5, 0.5, 1.0]
                });
                let solids = PrimPath::new(SOLIDS_FOLDER).expect("valid");
                ensure_folder(tx, &solids)?;
                let asset = child(
                    &solids,
                    &query::unique_child_name(tx.stage(), &solids, &name),
                )?;
                create(tx, &asset, PrimType::ASSET, None)?;
                set(tx, &asset, "name", Value::String(name.clone()))?;
                set(tx, &asset, "kind", Value::Enum("solid".into()))?;
                set(tx, &asset, "solid_color", Value::Color(color))?;
                set(tx, &asset, "width", Value::Int(i64::from(sw)))?;
                set(tx, &asset, "height", Value::Int(i64::from(sh)))?;
                let par = p.pixel_aspect.unwrap_or(info.pixel_aspect);
                if !(par.is_finite() && (0.01..=100.0).contains(&par)) {
                    return Err(fail("pixel aspect ratio must be between 0.01 and 100"));
                }
                if par != 1.0 {
                    set(tx, &asset, "pixel_aspect", Value::Float(par))?;
                }
                set(tx, &path, "source", Value::PrimRef(asset))?;
                anchor = [f64::from(sw) / 2.0, f64::from(sh) / 2.0];
                if kind == LayerKind::Adjustment {
                    set(tx, &path, "adjustment_layer", Value::Bool(true))?;
                }
            }
            LayerKind::PreComp | LayerKind::Footage | LayerKind::Audio => {
                let src = p.source.clone().expect("checked above");
                let (size, duration) = source_info.expect("checked above");
                set(tx, &path, "source", Value::PrimRef(src))?;
                anchor = [size[0] / 2.0, size[1] / 2.0];
                if let Some(d) = duration {
                    set(tx, &path, "out_point", Value::Time(start + d))?;
                }
            }
            LayerKind::Null => {
                // AE 26: a 100x100 null anchored at its top-left corner, at 0%
                // opacity (docs/reference/ae/interaction/layers-timeline-dnd).
                set(tx, &path, "transform:opacity", Value::Float(0.0))?;
            }
            LayerKind::Text => {
                let text = p.text.clone().filter(|t| !t.is_empty()).unwrap_or(name);
                let mut doc = TextDocument::new(text, "Arial", p.font_size.unwrap_or(72.0));
                if let Some(c) = p.color {
                    doc.fill_color = c;
                }
                set(tx, &path, "text:source", Value::TextDocument(Box::new(doc)))?;
            }
            LayerKind::Shape => {
                if let Some(preset) = p.shape {
                    let color = p.color.unwrap_or([0.85, 0.2, 0.2, 1.0]);
                    let side = (w.min(h) / 3.0).round().max(1.0);
                    add_shape(tx, &path, preset, side, color)?;
                }
            }
            LayerKind::Camera | LayerKind::Light => {
                set(tx, &path, "three_d", Value::Bool(true))?;
                let zoom = bae_anim::default_camera_zoom(info.width);
                position = [pos[0], pos[1], -zoom];
                if kind == LayerKind::Camera {
                    set(tx, &path, "camera:zoom", Value::Float(zoom))?;
                    set(tx, &path, "camera:focus_distance", Value::Float(zoom))?;
                    if let Some(c) = &p.camera {
                        for (attr, value) in c.attrs()? {
                            set(tx, &path, attr, value)?;
                        }
                        // The camera stands `zoom` in front of the comp (AE).
                        if let Some(z) = c.zoom {
                            position = [pos[0], pos[1], -z];
                        }
                    }
                } else {
                    if let Some(l) = &p.light {
                        for (attr, value) in l.attrs()? {
                            set(tx, &path, attr, value)?;
                        }
                    }
                    // AE 26 puts a new light up and to the right of the comp
                    // center, aiming at it: [cx + 80, cy - 80, -666.67].
                    if p.position.is_none() {
                        position = [pos[0] + 80.0, pos[1] - 80.0, -2000.0 / 3.0];
                    }
                }
                set(
                    tx,
                    &path,
                    "transform:point_of_interest",
                    Value::Vec3([pos[0], pos[1], 0.0]),
                )?;
            }
        }
        if anchor != [0.0, 0.0] {
            set(
                tx,
                &path,
                "transform:anchor_point",
                Value::Vec3([anchor[0], anchor[1], 0.0]),
            )?;
        }
        set(tx, &path, "transform:position", Value::Vec3(position))?;
        Ok(LayerOutput { path })
    }
}

fn add_shape(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    preset: ShapePreset,
    side: f64,
    color: [f64; 4],
) -> Result<(), CommandError> {
    let (group_base, path_base, ty) = match preset {
        ShapePreset::Rect => ("Rectangle", "Rectangle Path", PrimType::RECT),
        ShapePreset::Ellipse => ("Ellipse", "Ellipse Path", PrimType::ELLIPSE),
        ShapePreset::Star => ("Polystar", "Polystar Path", PrimType::STAR),
    };
    // Paths keep the English names; the names follow the UI language (AE).
    let named = |base: &'static str| format!("{} 1", crate::names::localized(base));
    let contents = child(layer, query::CONTENTS)?;
    create(tx, &contents, PrimType::FOLDER, None)?;
    let group = child(&contents, &query::sanitize_name(&format!("{group_base} 1")))?;
    create(tx, &group, PrimType::SHAPE_GROUP, None)?;
    set(tx, &group, "name", Value::String(named(group_base)))?;
    let inner = child(&group, query::CONTENTS)?;
    create(tx, &inner, PrimType::FOLDER, None)?;
    let shape = child(&inner, &query::sanitize_name(&format!("{path_base} 1")))?;
    create(tx, &shape, ty.clone(), None)?;
    set(tx, &shape, "name", Value::String(named(path_base)))?;
    if ty == PrimType::STAR {
        set(tx, &shape, "outer_radius", Value::Float(side / 2.0))?;
        set(tx, &shape, "inner_radius", Value::Float(side / 4.0))?;
    } else {
        set(tx, &shape, "size", Value::Vec2([side, side]))?;
    }
    let fill = child(&inner, "Fill_1")?;
    create(tx, &fill, PrimType::FILL, None)?;
    set(tx, &fill, "name", Value::String(named("Fill")))?;
    set(tx, &fill, "color", Value::Color(color))?;
    Ok(())
}

// ---------------------------------------------------------------------------

/// Check that every path is a layer of one comp; returns the comp.
fn same_comp(stage: &Stage, layers: &[PrimPath]) -> Result<PrimPath, CommandError> {
    let mut comp: Option<PrimPath> = None;
    for l in layers {
        if !query::is_layer(stage, l) {
            return Err(fail(format!("{l} is not a layer")));
        }
        let c =
            query::owning_comp(stage, l).ok_or_else(|| fail(format!("{l} is not in a comp")))?;
        match &comp {
            Some(prev) if prev != &c => return Err(fail("layers must belong to the same comp")),
            _ => comp = Some(c),
        }
    }
    comp.ok_or_else(|| fail("no layers given"))
}

/// Reorder the specs under `folder` in every local sublayer so that the composed
/// order follows `order` (paths not in `order` keep their relative order at the end).
pub(crate) fn apply_order(
    tx: &mut Transaction<'_>,
    folder: &PrimPath,
    order: &[PrimPath],
) -> Result<(), CommandError> {
    let stack: Vec<_> = tx.stage().layer_stack().to_vec();
    for id in stack {
        let Some(layer) = tx.stage().sublayer(&id) else {
            continue;
        };
        if !layer.has_spec(folder) {
            continue;
        }
        let kids = layer.children(folder);
        let mut desired: Vec<(usize, usize, PrimPath)> = kids
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let rank = order.iter().position(|o| o == &p).unwrap_or(usize::MAX);
                (rank, i, p)
            })
            .collect();
        desired.sort();
        for (target, (_, _, p)) in desired.into_iter().enumerate() {
            let current = tx.stage().sublayer(&id).and_then(|s| s.child_index(&p));
            if current != Some(target) {
                tx.apply_to(
                    &id,
                    Op::MovePrim {
                        from: p.clone(),
                        to: p,
                        index: Some(target),
                    },
                )?;
            }
        }
    }
    Ok(())
}

pub struct ReorderLayers;

#[derive(Deserialize, JsonSchema)]
pub struct ReorderLayersParams {
    /// Layers to move (same comp); they keep their relative order.
    pub layers: Vec<PrimPath>,
    /// Where the first moved layer ends up in the stack, 0 = top. Larger values move to the bottom.
    pub index: usize,
}

impl Command for ReorderLayers {
    type Params = ReorderLayersParams;
    type Output = ();
    const ID: &'static str = "layer.reorder";
    const TITLE: &'static str = "Reorder Layers";
    const DESCRIPTION: &'static str = "Move layers up or down in their comp's layer stack (AE: drag in the timeline, Bring Forward / Send Backward).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        let comp = same_comp(tx.stage(), &p.layers)?;
        let current = query::comp_layers(tx.stage(), &comp);
        let moved: Vec<PrimPath> = current
            .iter()
            .filter(|l| p.layers.contains(l))
            .cloned()
            .collect();
        let mut order: Vec<PrimPath> = current
            .iter()
            .filter(|l| !p.layers.contains(l))
            .cloned()
            .collect();
        let at = p.index.min(order.len());
        order.splice(at..at, moved);
        if order == current {
            return Ok(());
        }
        apply_order(tx, &query::layers_folder(&comp), &order)
    }
}

// ---------------------------------------------------------------------------

/// AE duplicate naming: a trailing number is incremented, else " 2" is appended.
fn duplicate_name(stage: &Stage, folder: &PrimPath, name: &str) -> String {
    let (base, _) = match name.rsplit_once(' ') {
        Some((b, n)) if n.parse::<u32>().is_ok() => (b, n),
        _ => (name, ""),
    };
    let taken: Vec<String> = stage
        .children(folder)
        .iter()
        .map(|p| query::display_name_of(stage, p))
        .collect();
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.contains(n))
        .expect("unbounded")
}

/// Duplicate `layer` directly above itself; returns the new path.
fn duplicate_layer(tx: &mut Transaction<'_>, layer: &PrimPath) -> Result<PrimPath, CommandError> {
    duplicate_layer_at(tx, layer, false)
}

/// Duplicates `layer` directly above it, or directly below it (`below`).
fn duplicate_layer_at(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    below: bool,
) -> Result<PrimPath, CommandError> {
    let stage = tx.stage();
    let folder = layer.parent().ok_or(ModelError::RootNotEditable)?;
    let name = query::display_name_of(stage, layer);
    let new_name = duplicate_name(stage, &folder, &name);
    let new_path = child(
        &folder,
        &query::unique_child_name(stage, &folder, &new_name),
    )?;
    let mut tree = composed_tree(stage, layer);
    retarget_refs(&mut tree, layer, &new_path);
    let index = stage
        .sublayer(tx.target())
        .and_then(|s| s.child_index(layer))
        .map(|i| i + usize::from(below));
    tx.ensure_spec(&folder)?;
    tx.apply(Op::RestorePrim {
        path: new_path.clone(),
        tree,
        index,
    })?;
    set(tx, &new_path, "name", Value::String(new_name))?;
    Ok(new_path)
}

/// A layer on the clipboard (Edit > Copy): where it was and everything it holds
/// (properties, keyframes, effects, masks, contents), composed.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CopiedLayer {
    pub path: PrimPath,
    pub tree: bae_model::PrimTree,
}

/// The clipboard form of `layers` (top first), for [`PasteLayers`].
pub fn copy_layers(stage: &Stage, layers: &[PrimPath]) -> Vec<CopiedLayer> {
    let mut v: Vec<PrimPath> = layers.to_vec();
    // Top first, as in the comp.
    if let Some(folder) = v.first().and_then(PrimPath::parent) {
        let order = stage.children(&folder);
        v.sort_by_key(|l| order.iter().position(|o| o == l).unwrap_or(usize::MAX));
    }
    v.iter()
        .map(|l| CopiedLayer {
            path: l.clone(),
            tree: composed_tree(stage, l),
        })
        .collect()
}

pub struct PasteLayers;

#[derive(Deserialize, JsonSchema)]
pub struct PasteLayersParams {
    /// Comp to paste into (any comp, not only the one the layers came from).
    pub comp: PrimPath,
    /// The copied layers, top first (`copy_layers`).
    pub layers: Vec<CopiedLayer>,
    /// Paste directly above this layer of `comp`; default the top of the comp.
    #[serde(default)]
    pub above: Option<PrimPath>,
}

impl Command for PasteLayers {
    type Params = PasteLayersParams;
    type Output = LayersOutput;
    const ID: &'static str = "layer.paste";
    const TITLE: &'static str = "Paste";
    const DESCRIPTION: &'static str = "Paste copied layers into a comp (above a layer, or on top), named like Duplicate; links between the pasted layers follow them, links to layers of another comp are dropped.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<LayersOutput, CommandError> {
        tx.stage().require_prim(&p.comp)?;
        let folder = query::layers_folder(&p.comp);
        let start = match &p.above {
            Some(above) => tx
                .stage()
                .sublayer(tx.target())
                .and_then(|s| s.child_index(above))
                .unwrap_or(0),
            None => 0,
        };
        // New paths first, so links between the pasted layers can follow them.
        let mut names: Vec<(PrimPath, String)> = Vec::new();
        for c in &p.layers {
            let stage = tx.stage();
            let name = c
                .tree
                .spec
                .attrs
                .get("name")
                .and_then(|a| a.value.as_ref())
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| c.path.name().to_owned());
            let mut new_name = duplicate_name(stage, &folder, &name);
            // Names already given in this paste count as taken.
            while names.iter().any(|(_, n)| *n == new_name) {
                new_name = format!("{new_name} 2");
            }
            let mut leaf = query::unique_child_name(stage, &folder, &new_name);
            while names.iter().any(|(p, _)| p.name() == leaf) {
                leaf = format!("{leaf}_2");
            }
            names.push((child(&folder, &leaf)?, new_name));
        }
        tx.ensure_spec(&folder)?;
        let mut paths = Vec::new();
        for (i, (c, (new_path, new_name))) in p.layers.iter().zip(&names).enumerate() {
            let mut tree = c.tree.clone();
            tree.name = new_path.name().to_owned();
            for (other, (other_new, _)) in p.layers.iter().zip(&names) {
                retarget_refs(&mut tree, &other.path, other_new);
            }
            if c.path.parent().as_ref() != Some(&folder) {
                drop_refs_into(&mut tree, c.path.parent().as_ref());
            }
            tx.apply(Op::RestorePrim {
                path: new_path.clone(),
                tree,
                index: Some(start + i),
            })?;
            set(tx, new_path, "name", Value::String(new_name.clone()))?;
            paths.push(new_path.clone());
        }
        Ok(LayersOutput { paths })
    }
}

/// Clears links into `folder` (the source comp's layers) that pasting into
/// another comp would leave dangling (parent, track matte).
fn drop_refs_into(tree: &mut bae_model::PrimTree, folder: Option<&PrimPath>) {
    let Some(folder) = folder else {
        return;
    };
    let inside = |v: &Value| matches!(v, Value::PrimRef(r) if r.reparent(folder, folder).is_some());
    for a in tree.spec.attrs.values_mut() {
        if a.value.as_ref().is_some_and(inside) {
            a.value = None;
        }
    }
    for c in &mut tree.children {
        drop_refs_into(c, Some(folder));
    }
}

// ---------------------------------------------------------------------------

pub struct DuplicateLayers;

#[derive(Deserialize, JsonSchema)]
pub struct LayersParams {
    /// Layers of one comp.
    pub layers: Vec<PrimPath>,
}

impl Command for DuplicateLayers {
    type Params = LayersParams;
    type Output = LayersOutput;
    const ID: &'static str = "layer.duplicate";
    const TITLE: &'static str = "Duplicate";
    const DESCRIPTION: &'static str = "Duplicate layers (with keyframes, masks, effects and contents); each copy goes directly above its original.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<LayersOutput, CommandError> {
        same_comp(tx.stage(), &p.layers)?;
        let mut paths = Vec::new();
        for l in &p.layers {
            paths.push(duplicate_layer(tx, l)?);
        }
        Ok(LayersOutput { paths })
    }
}

// ---------------------------------------------------------------------------

pub struct SplitLayers;

#[derive(Deserialize, JsonSchema)]
pub struct SplitLayersParams {
    /// Layers of one comp.
    pub layers: Vec<PrimPath>,
    /// Comp time (flicks) to split at; layers not active at that time are skipped.
    pub time: Time,
    /// Put each new layer above its original (default, AE's Preferences >
    /// Composition > Create Split Layers Above Original Layer) or below it.
    #[serde(default = "default_true")]
    pub above: bool,
}

fn default_true() -> bool {
    true
}

impl Command for SplitLayers {
    type Params = SplitLayersParams;
    type Output = LayersOutput;
    const ID: &'static str = "layer.split";
    const TITLE: &'static str = "Split Layer";
    const DESCRIPTION: &'static str = "Split layers at a comp time: the original ends there and a duplicate above it (or below it, `above: false`) starts there.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<LayersOutput, CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        same_comp(tx.stage(), &p.layers)?;
        let mut paths = Vec::new();
        for l in &p.layers {
            let timing = query::layer_timing(tx.stage(), l);
            let (a, b) = (
                timing.in_point.min(timing.out_point),
                timing.in_point.max(timing.out_point),
            );
            if !(a < p.time && p.time < b) {
                continue;
            }
            let new = duplicate_layer_at(tx, l, !p.above)?;
            set(tx, l, "out_point", Value::Time(p.time))?;
            set(tx, &new, "in_point", Value::Time(p.time))?;
            paths.push(new);
        }
        if paths.is_empty() {
            return Err(fail("no layer is active at the split time"));
        }
        Ok(LayersOutput { paths })
    }
}

// ---------------------------------------------------------------------------

pub struct TrimLayers;

#[derive(Deserialize, JsonSchema)]
pub struct TrimLayersParams {
    /// Layers of one comp.
    pub layers: Vec<PrimPath>,
    /// New in point (comp time, flicks); omitted keeps it.
    #[serde(default)]
    pub in_point: Option<Time>,
    /// New out point (comp time, flicks, exclusive); omitted keeps it.
    #[serde(default)]
    pub out_point: Option<Time>,
}

impl Command for TrimLayers {
    type Params = TrimLayersParams;
    type Output = ();
    const ID: &'static str = "layer.trim";
    const TITLE: &'static str = "Trim Layer";
    const DESCRIPTION: &'static str = "Set the in and/or out point of layers (AE: Alt+[ and Alt+]). The in point must stay before the out point.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        same_comp(tx.stage(), &p.layers)?;
        for l in &p.layers {
            let t = query::layer_timing(tx.stage(), l);
            let in_point = p.in_point.unwrap_or(t.in_point);
            let out_point = p.out_point.unwrap_or(t.out_point);
            if in_point >= out_point {
                return Err(fail(format!(
                    "{l}: the in point must be before the out point"
                )));
            }
            if let Some(v) = p.in_point {
                set(tx, l, "in_point", Value::Time(v))?;
            }
            if let Some(v) = p.out_point {
                set(tx, l, "out_point", Value::Time(v))?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SetParent;

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
pub struct SetParentParams {
    /// Layers to parent (same comp).
    pub layers: Vec<PrimPath>,
    /// New parent layer in the same comp; null removes the parent.
    #[serde(default)]
    pub parent: Option<PrimPath>,
    /// Keep the layers where they are on screen by adjusting their position, rotation
    /// and scale (AE default; AE's Shift+pick whip jumps instead). Default true.
    #[serde(default = "yes")]
    pub keep_transform: bool,
    /// Comp time (flicks) whose transforms are matched when keep_transform is on (default 0).
    #[serde(default)]
    pub time: Time,
}

fn parent_of(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    stage
        .prim(layer)?
        .value("parent")?
        .as_prim_ref()
        .cloned()
        .filter(|p| query::is_layer(stage, p))
}

/// 2D part of a transform: rotation (degrees) and axis scales.
fn decompose(m: &DMat4) -> (f64, [f64; 2]) {
    let (a, b, c, d) = (m.x_axis.x, m.x_axis.y, m.y_axis.x, m.y_axis.y);
    let sx = (a * a + b * b).sqrt();
    let det = a * d - b * c;
    let sy = if sx > 0.0 { det / sx } else { 0.0 };
    (b.atan2(a).to_degrees(), [sx, sy])
}

fn map_values(spec: &mut AttrSpec, mut f: impl FnMut(&Value) -> Option<Value>) {
    if let Some(v) = spec.value.as_ref().and_then(&mut f) {
        spec.value = Some(v);
    }
    for k in spec.keyframes.values_mut() {
        if let Some(v) = f(&k.value) {
            k.value = v;
        }
    }
}

impl Command for SetParent {
    type Params = SetParentParams;
    type Output = ();
    const ID: &'static str = "layer.set_parent";
    const TITLE: &'static str = "Set Parent";
    const DESCRIPTION: &'static str = "Parent layers to another layer of the same comp, or remove the parent (AE pick whip). By default the layers keep their on-screen transform.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        if !tx.locks_allowed() {
            ensure_unlocked(tx.stage(), &p.layers)?;
        }
        let comp = same_comp(tx.stage(), &p.layers)?;
        if let Some(parent) = &p.parent {
            if query::owning_comp(tx.stage(), parent).as_ref() != Some(&comp)
                || !query::is_layer(tx.stage(), parent)
            {
                return Err(fail("the parent must be a layer of the same comp"));
            }
            // No cycles: walk up from the new parent.
            let mut cur = Some(parent.clone());
            let mut guard = 0;
            while let Some(c) = cur {
                if p.layers.contains(&c) {
                    return Err(fail("a layer cannot be parented to itself or its child"));
                }
                guard += 1;
                if guard > 10_000 {
                    break;
                }
                cur = parent_of(tx.stage(), &c);
            }
        }
        let evaluated = if p.keep_transform {
            bae_anim::evaluate_comp(tx.stage(), &comp, p.time).ok()
        } else {
            None
        };
        let world = |path: &Option<PrimPath>| -> DMat4 {
            match (path, &evaluated) {
                (Some(path), Some(e)) => e.layer(path).map_or(DMat4::IDENTITY, |l| l.world_matrix),
                _ => DMat4::IDENTITY,
            }
        };
        let new_world = world(&p.parent);
        for layer in &p.layers {
            let old_parent = parent_of(tx.stage(), layer);
            if old_parent == p.parent {
                continue;
            }
            if evaluated.is_some() {
                let delta = new_world.inverse() * world(&old_parent);
                compensate(tx, layer, &delta)?;
            }
            match &p.parent {
                Some(parent) => set(tx, layer, "parent", Value::PrimRef(parent.clone()))?,
                None => {
                    tx.ensure_spec(layer)?;
                    tx.apply(Op::SetValue {
                        path: layer.clone(),
                        attr: "parent".into(),
                        value: None,
                    })?;
                    if parent_of(tx.stage(), layer).is_some() {
                        return Err(fail(format!(
                            "{layer}: the parent is set in a stronger or weaker sublayer and cannot be removed from the edit target"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Rewrite a layer's position, rotation and scale (static values and keyframes) by
/// the parent-space change `delta`, so the layer stays where it is on screen.
fn compensate(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    delta: &DMat4,
) -> Result<(), CommandError> {
    if delta.abs_diff_eq(DMat4::IDENTITY, 1e-12) {
        return Ok(());
    }
    let (d_rot, d_scale) = decompose(delta);
    let stage = tx.stage();
    let view = stage
        .prim(layer)
        .ok_or_else(|| ModelError::NoSuchPrim(layer.clone()))?;
    let spec_or_default = |attr: &str| -> AttrSpec {
        view.attr(attr).cloned().unwrap_or_else(|| AttrSpec {
            value: query::static_value(&view, attr),
            ..Default::default()
        })
    };
    let mut position = spec_or_default("transform:position");
    let mut rotation = spec_or_default("transform:rotation");
    let mut scale = spec_or_default("transform:scale");
    map_values(&mut position, |v| match v {
        Value::Vec3(p) => {
            let q = delta.transform_point3(DVec3::from_array(*p));
            Some(Value::Vec3(q.to_array()))
        }
        Value::Vec2(p) => {
            let q = delta.transform_point3(DVec3::new(p[0], p[1], 0.0));
            Some(Value::Vec2([q.x, q.y]))
        }
        _ => None,
    });
    // Spatial tangents rotate and scale with the path.
    for k in position.keyframes.values_mut() {
        for t in [&mut k.spatial_in, &mut k.spatial_out]
            .into_iter()
            .flatten()
        {
            let v = delta.transform_vector3(DVec3::from_array(*t));
            *t = v.to_array();
        }
    }
    map_values(&mut rotation, |v| {
        v.as_f64().map(|r| Value::Float(r + d_rot))
    });
    map_values(&mut scale, |v| match v {
        Value::Vec3(s) => Some(Value::Vec3([s[0] * d_scale[0], s[1] * d_scale[1], s[2]])),
        Value::Vec2(s) => Some(Value::Vec2([s[0] * d_scale[0], s[1] * d_scale[1]])),
        _ => None,
    });
    write_attr(tx, layer, "transform:position", position)?;
    write_attr(tx, layer, "transform:rotation", rotation)?;
    write_attr(tx, layer, "transform:scale", scale)?;
    Ok(())
}

// ---------------------------------------------------------------------------

pub struct ShiftLayers;

#[derive(Deserialize, JsonSchema)]
pub struct ShiftLayersParams {
    /// Layers of one comp.
    pub layers: Vec<PrimPath>,
    /// Offset in flicks added to the start time, in point and out point (may be negative).
    pub delta: Time,
}

impl Command for ShiftLayers {
    type Params = ShiftLayersParams;
    type Output = ();
    const ID: &'static str = "layer.shift_time";
    const TITLE: &'static str = "Move Layer in Time";
    const DESCRIPTION: &'static str = "Move layers earlier or later in their comp (dragging the layer bar; AE [ and ] move the in or out point to the current time). Keyframes move with the layer.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        same_comp(tx.stage(), &p.layers)?;
        if p.delta == Time::ZERO {
            return Ok(());
        }
        for l in &p.layers {
            let t = query::layer_timing(tx.stage(), l);
            let has_out = tx
                .stage()
                .prim(l)
                .is_some_and(|v| v.value("out_point").is_some());
            set(tx, l, "start_time", Value::Time(t.start_time + p.delta))?;
            set(tx, l, "in_point", Value::Time(t.in_point + p.delta))?;
            if has_out {
                set(tx, l, "out_point", Value::Time(t.out_point + p.delta))?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

/// Delete prims from every sublayer that has them. Layers parented to a deleted
/// layer are unparented first, keeping their on-screen transform (at `time`).
fn delete_prims(
    tx: &mut Transaction<'_>,
    paths: &[PrimPath],
    time: Time,
) -> Result<(), CommandError> {
    // Children of deleted layers, per comp.
    let mut orphans: Vec<PrimPath> = Vec::new();
    for p in paths {
        if !query::is_layer(tx.stage(), p) {
            continue;
        }
        if let Some(comp) = query::owning_comp(tx.stage(), p) {
            for l in query::comp_layers(tx.stage(), &comp) {
                if !paths.contains(&l)
                    && parent_of(tx.stage(), &l).as_ref() == Some(p)
                    && !orphans.contains(&l)
                {
                    orphans.push(l);
                }
            }
        }
    }
    // AE unparents them in place, locked ones too (the parent is gone).
    for l in orphans {
        tx.allow_locked(|tx| {
            SetParent.run(
                tx,
                SetParentParams {
                    layers: vec![l],
                    parent: None,
                    keep_transform: true,
                    time,
                },
            )
        })?;
    }
    // Deepest first, so deleting a folder and its contents together works.
    let mut sorted: Vec<PrimPath> = paths.to_vec();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.depth()));
    for p in sorted {
        if !tx.stage().has_prim(&p) {
            continue;
        }
        if p.is_root() {
            return Err(ModelError::RootNotEditable.into());
        }
        if !tx.stage().has_local_def(&p) {
            return Err(ModelError::ProvidedByReference(p).into());
        }
        let layers: Vec<_> = tx
            .stage()
            .layer_stack()
            .iter()
            .filter(|id| tx.stage().sublayer(id).is_some_and(|s| s.has_spec(&p)))
            .cloned()
            .collect();
        for id in layers {
            tx.apply_to(&id, Op::DeletePrim { path: p.clone() })?;
        }
    }
    Ok(())
}

/// Attributes [`SetLayerAttr`] writes: the layer switches, quality and label.
pub const LAYER_SWITCH_ATTRS: &[&str] = &[
    "shy",
    "locked",
    "enabled",
    "effects_enabled",
    "audio_enabled",
    "solo",
    "collapse_transformation",
    "motion_blur",
    "adjustment_layer",
    "three_d",
    "quality",
    "label",
];

pub struct SetLayerAttr;

#[derive(Deserialize, JsonSchema)]
pub struct SetLayerAttrParams {
    /// Layers (or project items, for `label`).
    pub layers: Vec<PrimPath>,
    /// One of the layer switches (`shy`, `locked`, `enabled` = Video,
    /// `effects_enabled` = Effect (fx), `audio_enabled`, `solo`, `collapse_transformation`, `motion_blur`,
    /// `adjustment_layer`, `three_d`), `quality` (`best` / `draft` /
    /// `wireframe`) or `label` (0 = none, 1–16 = AE's label colours).
    pub attr: String,
    pub value: Value,
}

impl Command for SetLayerAttr {
    type Params = SetLayerAttrParams;
    type Output = ();
    const ID: &'static str = "layer.set_attr";
    const TITLE: &'static str = "Change Layers";
    const DESCRIPTION: &'static str = "Set a layer switch, the quality or the label of several layers in one step (AE: Layer > Switches, Quality, 3D Layer; Edit > Label).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        if !LAYER_SWITCH_ATTRS.contains(&p.attr.as_str()) {
            return Err(fail(format!(
                "layer.set_attr sets switches, quality and label, not `{}`",
                p.attr
            )));
        }
        for l in &p.layers {
            tx.stage().require_prim(l)?;
            set(tx, l, &p.attr, p.value.clone())?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct DeleteLayers;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteLayersParams {
    /// Layers of one comp.
    pub layers: Vec<PrimPath>,
    /// Comp time (flicks) at which children of deleted layers keep their place (default 0).
    #[serde(default)]
    pub time: Time,
}

impl Command for DeleteLayers {
    type Params = DeleteLayersParams;
    type Output = ();
    const ID: &'static str = "layer.delete";
    const TITLE: &'static str = "Delete Layers";
    const DESCRIPTION: &'static str = "Delete layers (with their keyframes, masks, effects and contents). Layers parented to them are unparented and stay where they are.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        same_comp(tx.stage(), &p.layers)?;
        delete_prims(tx, &p.layers, p.time)
    }
}

pub struct DeleteItems;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteItemsParams {
    /// Project items (comps, folders, footage, solids) or layers.
    pub paths: Vec<PrimPath>,
}

impl Command for DeleteItems {
    type Params = DeleteItemsParams;
    type Output = ();
    const ID: &'static str = "item.delete";
    const TITLE: &'static str = "Delete";
    const DESCRIPTION: &'static str = "Delete project items (comps, folders with their contents, footage, solids) or layers in one step.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for path in &p.paths {
            tx.stage().require_prim(path)?;
        }
        delete_prims(tx, &p.paths, Time::ZERO)
    }
}
