//! Planning an import: an [`AiDocument`] → comps, shape layers and text layers,
//! as a list of writes that becomes a [`CommandBatch`] (or is applied inside the
//! `ai.import` command).
//!
//! The layout follows AE's "Import as Composition" followed by "Create Shapes
//! from Vector Layer":
//!
//! - every artboard becomes a comp of the artboard's size (1 pt = 1 px);
//! - every Illustrator layer becomes a shape layer with its anchor point at
//!   (0, 0) and its position at the comp centre, so the artwork sits around the
//!   layer origin (`docs/spec/ae/shapes.md`, bae-vector's convention);
//! - every group becomes a shape group "Group N"; every (compound) path becomes
//!   a group holding "Path 1…n" followed by its paints "Stroke 1", "Fill 1",
//!   "Gradient Fill 1" …;
//! - point text becomes a text layer above the shape layer it was in (until
//!   bae-text can outline it); text nested in groups is lifted out with a
//!   warning;
//! - a top-level clip group becomes its own shape layer whose clipping path is
//!   a layer mask. Nested clip groups are imported unclipped, with a warning.

use std::collections::HashSet;

use bae_model::{
    BezierPath, PrimPath, PrimType, Stage, TextDocument, Time, Transaction, Value, schema,
};
use bae_vector::{
    BlendMode, Brush, FillRule, Gradient, GradientKind, LineCap, LineJoin, Paint, PaintKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::batch::CommandBatch;
use crate::doc::{AiDocument, AiGroup, AiItem, AiLayer, AiPath, AiText, Artboard};
use crate::error::Error;

/// Folder new comps go to by default (the motion workspace's comps folder).
pub const COMPS_FOLDER: &str = "/comps";
/// Folder for `Artboard` prims.
pub const ARTBOARDS_FOLDER: &str = "/artboards";
/// Comp attribute pointing at the `Artboard` prim a comp was made from.
pub const ATTR_ARTBOARD: &str = "ai:artboard";

/// How point text is imported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextMode {
    /// A text layer with the text, font, size and colour (editable; styling
    /// beyond one style per text is not imported yet).
    #[default]
    Placeholder,
    /// Leave text out.
    Skip,
}

/// Import settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ImportOptions {
    /// Folder the new comps are created in (created if missing). Default `/comps`.
    pub comps_folder: PrimPath,
    /// Artboards to import, 0-based in Illustrator's order; empty = all.
    pub artboards: Vec<usize>,
    /// Frame rate of the new comps (default 30).
    pub fps: f64,
    /// Duration of the new comps in flicks (default 10 s).
    pub duration: Time,
    /// Also create an `Artboard` prim per artboard under `/artboards` (name, size,
    /// position in the document), linked from the comp's `ai:artboard`. Default on.
    pub artboard_prims: bool,
    /// How point text is imported (default: placeholder text layers).
    pub text: TextMode,
    /// Leave hidden layers and items out instead of importing them switched off.
    pub skip_hidden: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            comps_folder: PrimPath::new(COMPS_FOLDER).expect("valid path"),
            artboards: Vec::new(),
            fps: 30.0,
            duration: Time::from_seconds(10.0),
            artboard_prims: true,
            text: TextMode::Placeholder,
            skip_hidden: false,
        }
    }
}

/// What an import created (or will create), for the UI and agents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportReport {
    /// Document name.
    pub name: String,
    /// `ai`, `pdf`, `bridge`, `clipboard` or `code`.
    pub source: String,
    /// One comp per imported artboard.
    pub comps: Vec<PrimPath>,
    /// `Artboard` prims (when `artboard_prims` is on).
    pub artboards: Vec<PrimPath>,
    pub shape_layers: usize,
    pub text_layers: usize,
    /// Simple and compound paths.
    pub paths: usize,
    /// Reader and import warnings: what was approximated or left out.
    pub warnings: Vec<String>,
}

/// A planned import: the commands to run and what they create.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportPlan {
    pub batch: CommandBatch,
    pub report: ImportReport,
}

/// Plans the import of `doc` into `stage` (read only: names are chosen so they
/// do not collide with existing prims). Run the result with
/// [`CommandBatch::execute`].
pub fn plan_import(
    doc: &AiDocument,
    stage: &Stage,
    options: &ImportOptions,
) -> Result<ImportPlan, Error> {
    let (writes, report) = plan_writes(doc, stage, options)?;
    let mut batch = CommandBatch::new(format!("Import {}", display_or(&doc.name, "Illustrator")));
    for w in writes {
        w.push_to(&mut batch);
    }
    Ok(ImportPlan { batch, report })
}

/// One planned write; a `prim.create` or an `attr.set`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Write {
    Create(PrimPath, PrimType),
    Set(PrimPath, &'static str, Value),
}

impl Write {
    fn push_to(self, batch: &mut CommandBatch) {
        match self {
            Write::Create(path, prim_type) => batch.push(
                "prim.create",
                serde_json::json!({ "path": path, "prim_type": prim_type }),
            ),
            Write::Set(path, attr, value) => batch.push(
                "attr.set",
                serde_json::json!({ "path": path, "attr": attr, "value": value }),
            ),
        }
    }

    /// Applies the write in a running transaction with the same checks as
    /// `prim.create` / `attr.set`.
    pub(crate) fn apply(self, tx: &mut Transaction<'_>) -> Result<(), Error> {
        match self {
            Write::Create(path, prim_type) => {
                let parent = path
                    .parent()
                    .ok_or(bae_model::ModelError::RootNotEditable)?;
                tx.stage().require_prim(&parent)?;
                if tx.stage().has_prim(&path) {
                    return Err(bae_model::ModelError::PrimExists {
                        sublayer: tx.target().clone(),
                        path,
                    }
                    .into());
                }
                tx.ensure_spec(&parent)?;
                tx.apply(bae_model::Op::CreatePrim {
                    path,
                    specifier: bae_model::Specifier::Def,
                    prim_type: Some(prim_type),
                    index: None,
                })?;
            }
            Write::Set(path, attr, value) => {
                let ty = tx.stage().prim(&path).and_then(|p| p.prim_type().cloned());
                schema::check_value(&path, ty.as_ref(), attr, &value)?;
                tx.ensure_spec(&path)?;
                tx.apply(bae_model::Op::SetValue {
                    path,
                    attr: attr.to_owned(),
                    value: Some(value),
                })?;
            }
        }
        Ok(())
    }
}

pub(crate) fn plan_writes(
    doc: &AiDocument,
    stage: &Stage,
    options: &ImportOptions,
) -> Result<(Vec<Write>, ImportReport), Error> {
    if doc.artboards.is_empty() {
        return Err(Error::NoArtboards);
    }
    let selected: Vec<(usize, &Artboard)> = if options.artboards.is_empty() {
        doc.artboards.iter().enumerate().collect()
    } else {
        options
            .artboards
            .iter()
            .map(|&i| {
                doc.artboards
                    .get(i)
                    .map(|a| (i, a))
                    .ok_or(Error::NoSuchArtboard(i))
            })
            .collect::<Result<_, _>>()?
    };
    let mut p = Planner {
        names: Names {
            stage,
            planned: HashSet::new(),
        },
        writes: Vec::new(),
        options,
        report: ImportReport {
            name: doc.name.clone(),
            source: source_token(doc.source).to_owned(),
            warnings: doc.warnings.clone(),
            ..ImportReport::default()
        },
    };
    let multi = doc.artboards.len() > 1;
    for (index, artboard) in selected {
        let comp_name = if multi {
            format!("{} {}", display_or(&doc.name, "Illustrator"), artboard.name)
        } else {
            display_or(&doc.name, &artboard.name).to_owned()
        };
        p.artboard(index, artboard, &comp_name)?;
    }
    Ok((p.writes, p.report))
}

fn source_token(s: crate::doc::SourceFormat) -> &'static str {
    use crate::doc::SourceFormat as S;
    match s {
        S::AiPdf => "ai",
        S::Pdf => "pdf",
        S::Bridge => "bridge",
        S::Clipboard => "clipboard",
        S::Code => "code",
    }
}

fn display_or<'a>(name: &'a str, fallback: &'a str) -> &'a str {
    if name.trim().is_empty() {
        fallback
    } else {
        name.trim()
    }
}

/// A prim name from a display name: letters, digits, `_` and `-` (any script),
/// at most 64 characters, never starting with `-`.
pub(crate) fn sanitize_name(display: &str) -> String {
    let mut s: String = display
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    while s.starts_with('-') {
        s.remove(0);
    }
    if s.is_empty() { "item".to_owned() } else { s }
}

/// Unique prim names against the stage and everything planned so far.
struct Names<'s> {
    stage: &'s Stage,
    planned: HashSet<PrimPath>,
}

impl Names<'_> {
    fn exists(&self, path: &PrimPath) -> bool {
        self.planned.contains(path)
            || self.stage.has_prim(path)
            || self.stage.sublayers().any(|s| s.has_spec(path))
    }

    fn unique_child(&mut self, parent: &PrimPath, display: &str) -> Result<PrimPath, Error> {
        let base = sanitize_name(display);
        let mut candidate = parent.child(&base)?;
        let mut n = 2;
        while self.exists(&candidate) {
            candidate = parent.child(&format!("{base}_{n}"))?;
            n += 1;
        }
        self.planned.insert(candidate.clone());
        Ok(candidate)
    }

    /// A fixed child name (`layers`, `contents`, `masks`) under a parent this
    /// import created.
    fn fixed_child(&mut self, parent: &PrimPath, name: &str) -> Result<PrimPath, Error> {
        let path = parent.child(name)?;
        self.planned.insert(path.clone());
        Ok(path)
    }
}

/// AE-style display-name counters within one parent ("Group 1", "Path 2" …).
#[derive(Default)]
struct Counters {
    group: usize,
    path: usize,
    fill: usize,
    stroke: usize,
    gradient_fill: usize,
    gradient_stroke: usize,
}

impl Counters {
    fn next(counter: &mut usize, base: &str) -> String {
        *counter += 1;
        format!("{base} {counter}")
    }
}

/// What one Illustrator layer turns into, top first.
enum Unit<'d> {
    /// Consecutive vector items: one shape layer.
    Shapes(Vec<&'d AiItem>),
    /// A top-level clip group: a shape layer with the clip as masks.
    Clipped(&'d AiGroup),
    /// Point text; `true` when it was lifted out of a group.
    Text(&'d AiText, bool),
}

struct Planner<'s, 'o> {
    names: Names<'s>,
    writes: Vec<Write>,
    options: &'o ImportOptions,
    report: ImportReport,
}

impl Planner<'_, '_> {
    fn create(&mut self, path: &PrimPath, ty: PrimType) {
        self.writes.push(Write::Create(path.clone(), ty));
    }

    fn set(&mut self, path: &PrimPath, attr: &'static str, value: Value) {
        self.writes.push(Write::Set(path.clone(), attr, value));
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.report.warnings.push(msg.into());
    }

    /// Creates the missing folders of `path` (and `path` itself).
    fn ensure_folder(&mut self, path: &PrimPath) {
        if path.is_root() || self.names.exists(path) {
            return;
        }
        if let Some(parent) = path.parent() {
            self.ensure_folder(&parent);
        }
        self.names.planned.insert(path.clone());
        self.create(path, PrimType::FOLDER);
    }

    fn artboard(&mut self, index: usize, ab: &Artboard, comp_name: &str) -> Result<(), Error> {
        let [w, h] = ab.size;
        if !(w.is_finite() && h.is_finite() && w >= 1.0 && h >= 1.0) {
            self.warn(format!(
                "artboard {} ({}): size {w}×{h} is too small, skipped",
                index + 1,
                ab.name
            ));
            return Ok(());
        }
        let folder = self.options.comps_folder.clone();
        self.ensure_folder(&folder);
        let comp = self.names.unique_child(&folder, comp_name)?;
        self.create(&comp, PrimType::COMP);
        let layers = self.names.fixed_child(&comp, "layers")?;
        self.create(&layers, PrimType::FOLDER);
        self.set(&comp, "name", Value::String(comp_name.to_owned()));
        // AE rounds the artboard up to whole pixels.
        let (cw, ch) = (w.ceil().min(30_000.0), h.ceil().min(30_000.0));
        self.set(&comp, "width", Value::Int(cw as i64));
        self.set(&comp, "height", Value::Int(ch as i64));
        let fps = if self.options.fps.is_finite() && self.options.fps > 0.0 {
            self.options.fps
        } else {
            30.0
        };
        self.set(
            &comp,
            "frame_rate",
            Value::FrameRate(bae_model::FrameRate::from_fps(fps)),
        );
        let duration = if self.options.duration.0 > 0 {
            self.options.duration
        } else {
            Time::from_seconds(10.0)
        };
        self.set(&comp, "duration", Value::Time(duration));
        self.report.comps.push(comp.clone());

        if self.options.artboard_prims {
            let folder = PrimPath::new(ARTBOARDS_FOLDER).expect("valid path");
            self.ensure_folder(&folder);
            let art = self.names.unique_child(&folder, &ab.name)?;
            self.create(&art, PrimType::ARTBOARD);
            self.set(&art, "name", Value::String(ab.name.clone()));
            self.set(&art, "size", Value::Vec2([w, h]));
            self.set(&art, "origin", Value::Vec2(ab.origin));
            self.set(&comp, ATTR_ARTBOARD, Value::PrimRef(art.clone()));
            self.report.artboards.push(art);
        }

        // Layer content is drawn around the layer origin; the layer sits at
        // the comp centre (the centre of the artboard).
        let center = [cw / 2.0, ch / 2.0];
        for layer in &ab.layers {
            if self.options.skip_hidden && !layer.visible {
                continue;
            }
            self.ai_layer(&layers, layer, center)?;
        }
        Ok(())
    }

    fn units<'d>(&self, layer: &'d AiLayer) -> Vec<Unit<'d>> {
        let mut units = Vec::new();
        let mut run: Vec<&'d AiItem> = Vec::new();
        let mut lifted: Vec<&'d AiText> = Vec::new();
        let flush =
            |run: &mut Vec<&'d AiItem>, lifted: &mut Vec<&'d AiText>, units: &mut Vec<Unit<'d>>| {
                // Text lifted out of groups goes above the shapes it was in.
                units.extend(lifted.drain(..).map(|t| Unit::Text(t, true)));
                if !run.is_empty() {
                    units.push(Unit::Shapes(std::mem::take(run)));
                }
            };
        for item in &layer.items {
            if self.skipped(item) {
                continue;
            }
            match item {
                AiItem::Text(t) => {
                    flush(&mut run, &mut lifted, &mut units);
                    units.push(Unit::Text(t, false));
                }
                AiItem::Group(g) if g.clip.is_some() => {
                    flush(&mut run, &mut lifted, &mut units);
                    let mut nested = Vec::new();
                    self.collect_text(&g.items, &mut nested);
                    units.extend(nested.into_iter().map(|t| Unit::Text(t, true)));
                    units.push(Unit::Clipped(g));
                }
                AiItem::Group(g) => {
                    self.collect_text(&g.items, &mut lifted);
                    run.push(item);
                }
                AiItem::Path(_) => run.push(item),
            }
        }
        flush(&mut run, &mut lifted, &mut units);
        units
    }

    fn collect_text<'d>(&self, items: &'d [AiItem], out: &mut Vec<&'d AiText>) {
        for item in items {
            if self.skipped(item) {
                continue;
            }
            match item {
                AiItem::Text(t) => out.push(t),
                AiItem::Group(g) => self.collect_text(&g.items, out),
                AiItem::Path(_) => {}
            }
        }
    }

    fn skipped(&self, item: &AiItem) -> bool {
        let hidden = match item {
            AiItem::Group(g) => g.hidden,
            AiItem::Path(p) => p.hidden,
            AiItem::Text(t) => t.hidden,
        };
        (hidden && self.options.skip_hidden)
            || (matches!(item, AiItem::Text(_)) && self.options.text == TextMode::Skip)
    }

    fn ai_layer(
        &mut self,
        layers: &PrimPath,
        layer: &AiLayer,
        center: [f64; 2],
    ) -> Result<(), Error> {
        let mut shape_units = 0;
        let mut lifted_warned = false;
        for unit in self.units(layer) {
            match unit {
                Unit::Text(t, lifted) => {
                    if lifted && !lifted_warned {
                        self.warn(format!(
                            "layer {}: text inside groups was moved to its own text layer above the group",
                            layer.name
                        ));
                        lifted_warned = true;
                    }
                    self.text_layer(layers, t, layer)?;
                }
                Unit::Shapes(items) => {
                    shape_units += 1;
                    let name = numbered(&layer.name, shape_units);
                    let (_, contents) = self.shape_layer(layers, &name, layer, center)?;
                    let mut counters = Counters::default();
                    for item in items {
                        self.item(&contents, item, center, &mut counters)?;
                    }
                }
                Unit::Clipped(g) => {
                    shape_units += 1;
                    let display = g
                        .name
                        .clone()
                        .unwrap_or_else(|| numbered(&layer.name, shape_units));
                    let (lpath, contents) = self.shape_layer(layers, &display, layer, center)?;
                    self.masks(&lpath, g.clip.as_deref().unwrap_or(&[]), center)?;
                    if g.opacity < 1.0 {
                        self.set(&lpath, "transform:opacity", Value::Float(pct(g.opacity)));
                    }
                    if g.blend != BlendMode::Normal {
                        self.set(
                            &lpath,
                            "blend_mode",
                            Value::Enum(blend_token(g.blend).to_owned()),
                        );
                    }
                    if g.hidden {
                        self.set(&lpath, "enabled", Value::Bool(false));
                    }
                    let mut counters = Counters::default();
                    for item in &g.items {
                        self.item(&contents, item, center, &mut counters)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// A shape layer at the comp centre; returns the layer and its `contents`.
    fn shape_layer(
        &mut self,
        layers: &PrimPath,
        name: &str,
        layer: &AiLayer,
        center: [f64; 2],
    ) -> Result<(PrimPath, PrimPath), Error> {
        let path = self.names.unique_child(layers, name)?;
        self.create(&path, PrimType::LAYER);
        self.set(&path, "kind", Value::Enum("shape".into()));
        self.set(&path, "name", Value::String(name.to_owned()));
        self.set(&path, "label", Value::Int(8));
        if !layer.visible {
            self.set(&path, "enabled", Value::Bool(false));
        }
        if layer.locked {
            self.set(&path, "locked", Value::Bool(true));
        }
        self.set(
            &path,
            "transform:position",
            Value::Vec3([center[0], center[1], 0.0]),
        );
        let contents = self.names.fixed_child(&path, "contents")?;
        self.create(&contents, PrimType::FOLDER);
        self.report.shape_layers += 1;
        Ok((path, contents))
    }

    fn masks(
        &mut self,
        layer: &PrimPath,
        clip: &[BezierPath],
        center: [f64; 2],
    ) -> Result<(), Error> {
        if clip.is_empty() {
            return Ok(());
        }
        if clip.len() > 1 {
            self.warn(format!(
                "{layer}: compound clipping path imported as {} added masks (holes are not cut)",
                clip.len()
            ));
        }
        let folder = self.names.fixed_child(layer, "masks")?;
        self.create(&folder, PrimType::FOLDER);
        for (i, sub) in clip.iter().enumerate() {
            let display = format!("Mask {}", i + 1);
            let mask = self.names.unique_child(&folder, &display)?;
            self.create(&mask, PrimType::MASK);
            self.set(&mask, "name", Value::String(display));
            self.set(&mask, "path", Value::Bezier(offset_path(sub, center)));
            self.set(&mask, "mode", Value::Enum("add".into()));
        }
        Ok(())
    }

    fn item(
        &mut self,
        parent: &PrimPath,
        item: &AiItem,
        center: [f64; 2],
        counters: &mut Counters,
    ) -> Result<(), Error> {
        if self.skipped(item) {
            return Ok(());
        }
        match item {
            AiItem::Text(_) => Ok(()), // lifted to a text layer (see `units`)
            AiItem::Group(g) => {
                if g.clip.is_some() {
                    self.warn(format!(
                        "{parent}: nested clipping mask{} imported without clipping",
                        g.name
                            .as_deref()
                            .map(|n| format!(" ({n})"))
                            .unwrap_or_default()
                    ));
                }
                let display = g
                    .name
                    .clone()
                    .unwrap_or_else(|| Counters::next(&mut counters.group, "Group"));
                let inner = self.group(parent, &display, g.opacity, g.blend, g.hidden)?;
                let mut c = Counters::default();
                for child in &g.items {
                    self.item(&inner, child, center, &mut c)?;
                }
                Ok(())
            }
            AiItem::Path(p) => self.path(parent, p, center, counters),
        }
    }

    /// A shape group with its `contents` folder; returns the folder.
    fn group(
        &mut self,
        parent: &PrimPath,
        display: &str,
        opacity: f64,
        blend: BlendMode,
        hidden: bool,
    ) -> Result<PrimPath, Error> {
        let path = self.names.unique_child(parent, display)?;
        self.create(&path, PrimType::SHAPE_GROUP);
        self.set(&path, "name", Value::String(display.to_owned()));
        if opacity < 1.0 {
            self.set(&path, "transform:opacity", Value::Float(pct(opacity)));
        }
        if blend != BlendMode::Normal {
            self.set(
                &path,
                "blend_mode",
                Value::Enum(blend_token(blend).to_owned()),
            );
        }
        if hidden {
            self.set(&path, "enabled", Value::Bool(false));
        }
        let inner = self.names.fixed_child(&path, "contents")?;
        self.create(&inner, PrimType::FOLDER);
        Ok(inner)
    }

    fn path(
        &mut self,
        parent: &PrimPath,
        p: &AiPath,
        center: [f64; 2],
        counters: &mut Counters,
    ) -> Result<(), Error> {
        let display = p
            .name
            .clone()
            .unwrap_or_else(|| Counters::next(&mut counters.group, "Group"));
        let inner = self.group(parent, &display, p.opacity, p.blend, p.hidden)?;
        let mut c = Counters::default();
        for sub in &p.subpaths {
            let name = Counters::next(&mut c.path, "Path");
            let path = self.names.unique_child(&inner, &name)?;
            self.create(&path, PrimType::PATH);
            self.set(&path, "name", Value::String(name));
            self.set(&path, "path", Value::Bezier(offset_path(sub, center)));
        }
        for paint in &p.paints {
            self.paint(&inner, paint, center, &mut c)?;
        }
        if p.paints.is_empty() {
            self.warn(format!(
                "{display}: path without fill or stroke (invisible)"
            ));
        }
        self.report.paths += 1;
        Ok(())
    }

    fn paint(
        &mut self,
        parent: &PrimPath,
        paint: &Paint,
        center: [f64; 2],
        c: &mut Counters,
    ) -> Result<(), Error> {
        let (ty, name) = match (&paint.kind, &paint.brush) {
            (PaintKind::Fill(_), Brush::Solid(_)) => {
                (PrimType::FILL, Counters::next(&mut c.fill, "Fill"))
            }
            (PaintKind::Stroke(_), Brush::Solid(_)) => {
                (PrimType::STROKE, Counters::next(&mut c.stroke, "Stroke"))
            }
            (PaintKind::Fill(_), Brush::Gradient(_)) => (
                PrimType::GRADIENT_FILL,
                Counters::next(&mut c.gradient_fill, "Gradient Fill"),
            ),
            (PaintKind::Stroke(_), Brush::Gradient(_)) => (
                PrimType::GRADIENT_STROKE,
                Counters::next(&mut c.gradient_stroke, "Gradient Stroke"),
            ),
        };
        let path = self.names.unique_child(parent, &name)?;
        self.create(&path, ty);
        self.set(&path, "name", Value::String(name));
        match &paint.brush {
            Brush::Solid(color) => self.set(&path, "color", Value::Color(color64(*color))),
            Brush::Gradient(g) => self.gradient(&path, g, center),
        }
        if paint.opacity < 1.0 {
            self.set(&path, "opacity", Value::Float(pct(paint.opacity)));
        }
        if paint.blend != BlendMode::Normal {
            self.set(
                &path,
                "blend_mode",
                Value::Enum(blend_token(paint.blend).to_owned()),
            );
        }
        match &paint.kind {
            PaintKind::Fill(rule) => {
                if *rule == FillRule::EvenOdd {
                    self.set(&path, "fill_rule", Value::Enum("even_odd".into()));
                }
            }
            PaintKind::Stroke(s) => {
                self.set(&path, "width", Value::Float(s.width));
                if s.cap != LineCap::Butt {
                    self.set(&path, "line_cap", Value::Enum(cap_token(s.cap).into()));
                }
                if s.join != LineJoin::Miter {
                    self.set(&path, "line_join", Value::Enum(join_token(s.join).into()));
                }
                if s.miter_limit != 4.0 {
                    self.set(&path, "miter_limit", Value::Float(s.miter_limit));
                }
                if !s.dashes.is_empty() {
                    if matches!(paint.brush, Brush::Gradient(_)) {
                        self.warn(format!(
                            "{path}: dashes on a gradient stroke are not imported"
                        ));
                    } else {
                        self.set(&path, "dashes", Value::FloatList(s.dashes.clone()));
                        if s.dash_offset != 0.0 {
                            self.set(&path, "dash_offset", Value::Float(s.dash_offset));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn gradient(&mut self, path: &PrimPath, g: &Gradient, center: [f64; 2]) {
        let off = |p: kurbo::Point| [p.x - center[0], p.y - center[1]];
        if let GradientKind::Radial {
            highlight_length,
            highlight_angle,
        } = g.kind
        {
            self.set(path, "gradient_type", Value::Enum("radial".into()));
            if highlight_length != 0.0 {
                self.set(path, "highlight_length", Value::Float(highlight_length));
                self.set(path, "highlight_angle", Value::Float(highlight_angle));
            }
        }
        self.set(path, "start_point", Value::Vec2(off(g.start)));
        self.set(path, "end_point", Value::Vec2(off(g.end)));
        let (colors, count) = ae_gradient_colors(g);
        self.set(path, "colors", Value::FloatList(colors));
        self.set(path, "color_stops", Value::Int(count as i64));
    }

    fn text_layer(&mut self, layers: &PrimPath, t: &AiText, layer: &AiLayer) -> Result<(), Error> {
        let first_line: String = t
            .text
            .split(['\r', '\n'])
            .next()
            .unwrap_or_default()
            .chars()
            .take(64)
            .collect();
        let display = t
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                if first_line.trim().is_empty() {
                    "Text".to_owned()
                } else {
                    first_line
                }
            });
        let path = self.names.unique_child(layers, &display)?;
        self.create(&path, PrimType::LAYER);
        self.set(&path, "kind", Value::Enum("text".into()));
        self.set(&path, "name", Value::String(display));
        self.set(&path, "label", Value::Int(1));
        if !layer.visible || t.hidden {
            self.set(&path, "enabled", Value::Bool(false));
        }
        if layer.locked {
            self.set(&path, "locked", Value::Bool(true));
        }
        let mut doc = TextDocument::new(
            t.text.replace("\r\n", "\r").replace('\n', "\r"),
            t.font.clone(),
            t.size,
        );
        match t.fill {
            Some(c) => doc.fill_color = c,
            None => doc.fill_color = [0.0, 0.0, 0.0, 0.0],
        }
        if let Some((c, w)) = t.stroke {
            doc.stroke_color = Some(c);
            doc.stroke_width = w;
        }
        doc.justification = match t.justification.as_str() {
            "center" | "right" => t.justification.clone(),
            _ => "left".to_owned(),
        };
        self.set(&path, "text:source", Value::TextDocument(Box::new(doc)));
        self.set(
            &path,
            "transform:position",
            Value::Vec3([t.position[0], t.position[1], 0.0]),
        );
        if t.rotation != 0.0 {
            self.set(&path, "transform:rotation", Value::Float(t.rotation));
        }
        if t.scale != [100.0, 100.0] {
            self.set(
                &path,
                "transform:scale",
                Value::Vec3([t.scale[0], t.scale[1], 100.0]),
            );
        }
        self.report.text_layers += 1;
        Ok(())
    }
}

/// "Layer", "Layer 2", "Layer 3" … for the shape layers one Ai layer splits into.
fn numbered(name: &str, n: usize) -> String {
    let base = display_or(name, "Layer");
    if n <= 1 {
        base.to_owned()
    } else {
        format!("{base} {n}")
    }
}

fn pct(v: f64) -> f64 {
    (v.clamp(0.0, 1.0) * 100.0 * 1e6).round() / 1e6
}

/// f32 paint colour → project colour, without f32 noise (0.2f32 → 0.2).
fn color64(c: [f32; 4]) -> [f64; 4] {
    c.map(|v| (f64::from(v) * 1e6).round() / 1e6)
}

fn offset_path(p: &BezierPath, center: [f64; 2]) -> BezierPath {
    BezierPath {
        vertices: p
            .vertices
            .iter()
            .map(|v| [v[0] - center[0], v[1] - center[1]])
            .collect(),
        ..p.clone()
    }
}

/// AE's flat gradient layout: `[pos, r, g, b]` per stop, then `[pos, alpha]`
/// per stop when any stop is not opaque. Returns the data and the colour-stop
/// count.
pub fn ae_gradient_colors(g: &Gradient) -> (Vec<f64>, usize) {
    let mut out = Vec::with_capacity(g.stops.len() * 6);
    for s in &g.stops {
        let c = color64(s.color);
        out.extend([round6(f64::from(s.offset)), c[0], c[1], c[2]]);
    }
    if g.stops.iter().any(|s| s.color[3] < 1.0) {
        for s in &g.stops {
            out.extend([round6(f64::from(s.offset)), color64(s.color)[3]]);
        }
    }
    (out, g.stops.len())
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Project token of a shape blend mode (`bae_model::schema::BLEND_MODES`).
pub fn blend_token(b: BlendMode) -> &'static str {
    match b {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color_dodge",
        BlendMode::ColorBurn => "color_burn",
        BlendMode::HardLight => "hard_light",
        BlendMode::SoftLight => "soft_light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
    }
}

fn cap_token(c: LineCap) -> &'static str {
    match c {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Projecting => "projecting",
    }
}

fn join_token(j: LineJoin) -> &'static str {
    match j {
        LineJoin::Miter => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitized() {
        assert_eq!(sanitize_name("Layer 1"), "Layer_1");
        assert_eq!(sanitize_name("图层 1"), "图层_1");
        assert_eq!(sanitize_name("--a"), "a");
        assert_eq!(sanitize_name("  "), "item");
        assert_eq!(sanitize_name(&"x".repeat(100)).len(), 64);
    }

    #[test]
    fn gradient_layout_round_trips_through_bae_vector() {
        let g = Gradient {
            kind: GradientKind::Linear,
            start: kurbo::Point::ZERO,
            end: kurbo::Point::new(10.0, 0.0),
            stops: vec![
                bae_vector::GradientStop {
                    offset: 0.0,
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                bae_vector::GradientStop {
                    offset: 0.25,
                    color: [0.0, 1.0, 0.0, 0.5],
                },
                bae_vector::GradientStop {
                    offset: 1.0,
                    color: [0.0, 0.0, 1.0, 1.0],
                },
            ],
        };
        let (colors, n) = ae_gradient_colors(&g);
        assert_eq!(n, 3);
        assert_eq!(colors.len(), 3 * 4 + 3 * 2);
        assert_eq!(Gradient::stops_from_ae(&colors, n), g.stops);
        // Opaque gradients carry no opacity stops.
        let opaque = Gradient {
            stops: vec![g.stops[0], g.stops[2]],
            ..g
        };
        assert_eq!(ae_gradient_colors(&opaque).0.len(), 8);
    }
}
