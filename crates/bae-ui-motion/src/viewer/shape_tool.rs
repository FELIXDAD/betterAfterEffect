//! Shape tools in the Composition viewer (AE-UI-315, AE-SHP-051..054): drag out a
//! rectangle, rounded rectangle, ellipse, polygon or star. Where it goes is
//! decided when the drag starts ([`Target`]); the drag is previewed live and
//! becomes one `shape.add` or `mask.create` on release (one undo step).
//!
//! Modifiers while dragging: Shift = square / circle / unrotated polystar;
//! Cmd/Ctrl = rectangles and ellipses from the center, polystars keep their inner
//! radius; Space = move the shape being drawn; Up/Down = rounded rectangle
//! roundness or polystar points; Left/Right = roundness 0 / maximum, polystar
//! outer roundness; Page Up/Down = star inner roundness; Esc cancels. Alt/Option
//! when the drag starts (or the Bezier Path option) makes a Bezier path.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{PrimPath, Stage, Value};
use bae_motion::query::{self, CompInfo};
use bae_motion::shape::{Geometry, ShapeKind, mask_color, takes_masks};
use bae_ui::PanelContext;
use bae_ui::theme;
use egui::{Color32, Event, Key, Painter, PointerButton, Response, Shape, Stroke, Ui};
use serde_json::json;

use super::overlay;
use crate::common;
use crate::render::View;
use crate::session::{Pending, SharedSession};
use crate::tools::{FillRequest, PaintKind, SharedTools, Tool, ToolOptions};

/// Where a drawn shape goes (AE-SHP-051).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// No layer selected (or one that cannot take masks): a new shape layer.
    NewLayer,
    /// One shape layer selected, Tool Creates Shape: a new group in it.
    Shape(PrimPath),
    /// A layer that gets a mask: any other AV layer, or a shape layer with Tool
    /// Creates Mask.
    Mask(PrimPath),
}

impl Target {
    /// The target for the current selection. Several selected layers: a new
    /// shape layer above the topmost one (AE 26, R1).
    pub(crate) fn of(stage: &Stage, comp: &PrimPath, selected: &[PrimPath], mask: bool) -> Target {
        if selected.len() > 1 {
            return Target::NewLayer;
        }
        let Some(layer) = query::comp_layers(stage, comp)
            .into_iter()
            .find(|l| selected.contains(l))
        else {
            return Target::NewLayer;
        };
        match query::layer_kind(stage, &layer).as_deref() {
            Some("shape") if !mask => Target::Shape(layer),
            Some(kind) if takes_masks(kind) => Target::Mask(layer),
            _ => Target::NewLayer,
        }
    }

    /// The pointer shows a small star when the drag makes a shape (not a mask).
    pub(crate) fn makes_shape(&self) -> bool {
        !matches!(self, Target::Mask(_))
    }

    fn layer(&self) -> Option<&PrimPath> {
        match self {
            Target::NewLayer => None,
            Target::Shape(l) | Target::Mask(l) => Some(l),
        }
    }
}

/// A key that changes the shape being drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
}

impl DragKey {
    fn of(key: Key) -> Option<DragKey> {
        Some(match key {
            Key::ArrowUp => DragKey::Up,
            Key::ArrowDown => DragKey::Down,
            Key::ArrowLeft => DragKey::Left,
            Key::ArrowRight => DragKey::Right,
            Key::PageUp => DragKey::PageUp,
            Key::PageDown => DragKey::PageDown,
            _ => return None,
        })
    }
}

/// Roundness step of the Up / Down arrows (pixels; AE's step 待核).
const ROUNDNESS_STEP: f64 = 1.0;
/// Polystar roundness step of the arrows and Page Up / Down (percent; AE 26
/// measured 10 % for Left / Right, Page Up / Down 待核).
const STAR_ROUNDNESS_STEP: f64 = 10.0;
/// Default inner / outer radius ratio of a new star (AE-SHP-053, 待核).
const STAR_RATIO: f64 = 0.5;
/// Scrolled points per Up / Down step while drawing (one wheel notch).
const WHEEL_STEP: f32 = 20.0;

/// A shape being dragged out.
pub(crate) struct ShapeDrag {
    pub tool: Tool,
    pub target: Target,
    /// Target layer space → comp space at the start of the drag (for a new
    /// layer: the comp center as origin).
    to_comp: DMat4,
    /// Drag start and pointer, comp space.
    start: DVec2,
    current: DVec2,
    shift: bool,
    from_center: bool,
    bezier: bool,
    pub roundness: f64,
    pub points: f64,
    outer_roundness: f64,
    inner_roundness: f64,
    inner_ratio: f64,
    /// Cmd/Ctrl held on a star: the inner radius it keeps (target space).
    fixed_inner: Option<f64>,
    /// Outline color of the preview.
    color: Color32,
    /// Mouse wheel scrolling not yet turned into steps.
    wheel: f32,
}

/// Target layer space → comp space.
pub(crate) fn target_matrix(info: &CompInfo, comp: &EvaluatedComp, target: &Target) -> DMat4 {
    match target.layer().and_then(|l| comp.layer(l)) {
        Some(l) => l.world_matrix,
        None => DMat4::from_translation(DVec3::new(
            f64::from(info.width) / 2.0,
            f64::from(info.height) / 2.0,
            0.0,
        )),
    }
}

impl ShapeDrag {
    pub(crate) fn new(
        tool: Tool,
        target: Target,
        to_comp: DMat4,
        start: DVec2,
        options: &ToolOptions,
        alt: bool,
        color: Color32,
    ) -> Self {
        ShapeDrag {
            tool,
            target,
            to_comp,
            start,
            current: start,
            shift: false,
            from_center: false,
            bezier: options.bezier_path || alt,
            roundness: if tool == Tool::RoundedRectangle {
                options.roundness.max(0.0)
            } else {
                0.0
            },
            points: options.points.max(3.0),
            outer_roundness: 0.0,
            inner_roundness: 0.0,
            inner_ratio: STAR_RATIO,
            fixed_inner: None,
            color,
            wheel: 0.0,
        }
    }

    fn polystar(&self) -> bool {
        matches!(self.tool, Tool::Polygon | Tool::Star)
    }

    /// Follow the pointer (comp space) and the modifiers held this frame.
    pub(crate) fn update(&mut self, pointer: DVec2, shift: bool, command: bool, space: bool) {
        if space {
            // Space moves the whole shape.
            self.start += pointer - self.current;
        }
        self.current = pointer;
        self.shift = shift;
        if self.polystar() {
            match (command, self.fixed_inner) {
                (true, None) => {
                    let (outer, _) = self.radii();
                    self.fixed_inner = Some(outer * self.inner_ratio);
                }
                (false, Some(inner)) => {
                    let (outer, _) = self.radii();
                    if outer > 0.0 {
                        self.inner_ratio = inner / outer;
                    }
                    self.fixed_inner = None;
                }
                _ => {}
            }
        } else {
            self.from_center = command;
        }
    }

    pub(crate) fn key(&mut self, key: DragKey) {
        match (self.tool, key) {
            (Tool::RoundedRectangle, DragKey::Up) => self.roundness += ROUNDNESS_STEP,
            (Tool::RoundedRectangle, DragKey::Down) => {
                self.roundness = (self.roundness - ROUNDNESS_STEP).max(0.0);
            }
            (Tool::RoundedRectangle, DragKey::Left) => self.roundness = 0.0,
            (Tool::RoundedRectangle, DragKey::Right) => {
                if let Some((_, size)) = self.rect_in_target() {
                    self.roundness = size[0].abs().min(size[1].abs()) / 2.0;
                }
            }
            (Tool::Polygon | Tool::Star, DragKey::Up) => self.points += 1.0,
            (Tool::Polygon | Tool::Star, DragKey::Down) => {
                self.points = (self.points - 1.0).max(3.0);
            }
            (Tool::Polygon | Tool::Star, DragKey::Left) => {
                self.outer_roundness -= STAR_ROUNDNESS_STEP;
            }
            (Tool::Polygon | Tool::Star, DragKey::Right) => {
                self.outer_roundness += STAR_ROUNDNESS_STEP;
            }
            (Tool::Star, DragKey::PageUp) => self.inner_roundness += STAR_ROUNDNESS_STEP,
            (Tool::Star, DragKey::PageDown) => self.inner_roundness -= STAR_ROUNDNESS_STEP,
            _ => {}
        }
    }

    fn to_target(&self, p: DVec2) -> DVec2 {
        let v = self
            .to_comp
            .inverse()
            .transform_point3(DVec3::new(p.x, p.y, 0.0));
        DVec2::new(v.x, v.y)
    }

    /// Rectangle or ellipse: `(center, size)` in target space.
    fn rect_in_target(&self) -> Option<(DVec2, [f64; 2])> {
        let mut d = self.current - self.start;
        if self.shift {
            let m = d.x.abs().max(d.y.abs());
            d = DVec2::new(m.copysign(d.x), m.copysign(d.y));
        }
        let (a, b) = if self.from_center {
            (self.start - d, self.start + d)
        } else {
            (self.start, self.start + d)
        };
        let (a, b) = (self.to_target(a), self.to_target(b));
        let size = [(b.x - a.x).abs(), (b.y - a.y).abs()];
        Some(((a + b) / 2.0, size))
    }

    /// Polystar: `(outer radius, rotation)` in target space.
    fn radii(&self) -> (f64, f64) {
        let c = self.to_target(self.start);
        let v = self.to_target(self.current) - c;
        let rotation = if self.shift {
            0.0
        } else {
            v.y.atan2(v.x).to_degrees() + 90.0
        };
        (v.length(), rotation)
    }

    /// The shape in target space; `None` while it has no size.
    pub(crate) fn geometry(&self) -> Option<Geometry> {
        let mut g = empty_geometry();
        if self.polystar() {
            let (outer, rotation) = self.radii();
            if outer < 0.5 {
                return None;
            }
            let c = self.to_target(self.start);
            g.kind = if self.tool == Tool::Star {
                ShapeKind::Star
            } else {
                ShapeKind::Polygon
            };
            g.position = [c.x, c.y];
            g.points = Some(self.points);
            g.outer_radius = Some(outer);
            g.inner_radius = Some(self.fixed_inner.unwrap_or(outer * self.inner_ratio));
            g.rotation = rotation;
            g.outer_roundness = self.outer_roundness;
            g.inner_roundness = self.inner_roundness;
        } else {
            let (c, size) = self.rect_in_target()?;
            if size[0] < 0.5 && size[1] < 0.5 {
                return None;
            }
            g.kind = if self.tool == Tool::Ellipse {
                ShapeKind::Ellipse
            } else {
                ShapeKind::Rect
            };
            g.position = [c.x, c.y];
            g.size = Some(size);
            g.roundness = self.roundness.min(size[0].min(size[1]) / 2.0);
        }
        Some(g)
    }

    /// The live outline.
    pub(crate) fn paint(&self, painter: &Painter, view: &View) {
        let Some(g) = self.geometry() else {
            return;
        };
        let Ok(path) = g.to_path(g.position) else {
            return;
        };
        let stroke = Stroke::new(1.0, self.color);
        let shapes: Vec<Shape> = overlay::path_shapes(&path, &self.to_comp, view, stroke);
        painter.extend(shapes);
    }

    /// Run the command for the finished drag.
    pub(crate) fn commit(
        &self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        options: &ToolOptions,
    ) {
        if let Some(g) = self.geometry() {
            run(cx, session, info, &self.target, g, self.bezier, options);
        }
    }
}

fn paint_json(p: &crate::tools::Paint) -> Option<serde_json::Value> {
    (p.kind != PaintKind::None).then(|| json!({"color": p.color, "opacity": p.opacity}))
}

/// Create the shape or mask `g` (target space) for `target`; new shape layers are
/// selected once they exist. Returns the layer that gets it (for a new layer, the
/// path it will have).
pub(crate) fn run(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    target: &Target,
    g: Geometry,
    bezier: bool,
    options: &ToolOptions,
) -> Option<PrimPath> {
    let stage = cx.stage;
    let mut params = serde_json::to_value(&g).unwrap_or_default();
    let obj = params.as_object_mut().expect("geometry is an object");
    match target {
        Target::Mask(layer) => {
            // "Mask N" / "蒙版 N": named by the command in the naming language.
            obj.insert("layer".into(), json!(layer));
            cx.invoke_with("mask.create", params);
            Some(layer.clone())
        }
        Target::Shape(layer) => {
            obj.insert("layer".into(), json!(layer));
            add_paint(obj, bezier, options);
            cx.invoke_with("shape.add", params);
            Some(layer.clone())
        }
        Target::NewLayer => {
            let folder = query::layers_folder(&info.path);
            let name = common::numbered(stage, session, &folder, "Shape Layer");
            let path = folder
                .child(&query::unique_child_name(stage, &folder, &name))
                .ok()?;
            // Above the selected layer (a null or camera that cannot take a mask).
            let index = {
                let s = session.lock();
                let layers = query::comp_layers(stage, &info.path);
                s.selected_layers
                    .iter()
                    .filter_map(|l| layers.iter().position(|x| x == l))
                    .min()
                    .unwrap_or(0)
            };
            obj.insert("comp".into(), json!(info.path));
            obj.insert("name".into(), json!(name));
            obj.insert("index".into(), json!(index));
            obj.insert(
                "center_anchor".into(),
                json!(prefs::CENTER_ANCHOR_IN_NEW_SHAPES),
            );
            add_paint(obj, bezier, options);
            cx.invoke_with("shape.add", params);
            let mut s = session.lock();
            s.expect(Pending::Layer(path.clone()));
            s.reserve_name(path.clone(), name);
            Some(path)
        }
    }
}

fn add_paint(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    bezier: bool,
    options: &ToolOptions,
) {
    obj.insert("bezier".into(), json!(bezier));
    if let Some(f) = paint_json(&options.fill) {
        obj.insert("fill".into(), f);
    }
    if let Some(mut s) = paint_json(&options.stroke) {
        s["width"] = json!(options.stroke_width);
        obj.insert("stroke".into(), s);
    }
}

/// Settings other workstreams will provide (D's Preferences); AE defaults until
/// then, in one place.
pub(crate) mod prefs {
    /// General > "Center Anchor Point in New Shape Layers" (off in AE's defaults
    /// as the tutorials show; 待核).
    pub const CENTER_ANCHOR_IN_NEW_SHAPES: bool = false;
}

/// Size of a layer's own pixels (solid, footage, pre-comp source) in layer space;
/// `None` for layers whose bounds are their content (shape, text).
fn source_size(stage: &Stage, layer: &PrimPath) -> Option<[f64; 2]> {
    let src = stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "source"))?;
    let Value::PrimRef(src) = src else {
        return None;
    };
    if let Some(info) = query::comp_info(stage, &src) {
        return Some([f64::from(info.width), f64::from(info.height)]);
    }
    let v = stage.prim(&src)?;
    let n = |a: &str| query::static_value(&v, a).and_then(|x| x.as_f64());
    Some([n("width")?, n("height")?])
}

/// Double-clicking a shape tool button (AE-SHP-054): a shape or mask the size of
/// the target — the comp frame for a new or selected shape layer, the layer's
/// pixels for a mask. Shift: the largest square / circle that fits (AE
/// tutorials T05@04:05). Also resets the tool's remembered roundness / points.
pub(crate) fn fill_target(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    request: FillRequest,
    tools: &SharedTools,
) {
    let FillRequest { tool, shift } = request;
    let options = {
        let mut t = tools.lock();
        let defaults = ToolOptions::default();
        match tool {
            Tool::RoundedRectangle => t.options.roundness = defaults.roundness,
            Tool::Polygon | Tool::Star => t.options.points = defaults.points,
            _ => {}
        }
        t.options.clone()
    };
    let selected = session.lock().selected_layers.clone();
    let target = Target::of(cx.stage, &info.path, &selected, options.creates_mask);
    let to_comp = target_matrix(info, comp, &target);
    let (w, h) = (f64::from(info.width), f64::from(info.height));
    // The frame in target space: a mask uses the layer's pixels when it has them.
    let source = target.layer().and_then(|l| source_size(cx.stage, l));
    let (center, mut size) = match (&target, source) {
        (Target::Mask(_), Some([lw, lh])) => (DVec2::new(lw / 2.0, lh / 2.0), [lw, lh]),
        _ => {
            let inv = to_comp.inverse();
            let a = inv.transform_point3(DVec3::ZERO);
            let b = inv.transform_point3(DVec3::new(w, h, 0.0));
            (
                DVec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                [(b.x - a.x).abs(), (b.y - a.y).abs()],
            )
        }
    };
    if shift {
        let m = size[0].min(size[1]);
        size = [m, m];
    }
    let radius = size[0].min(size[1]) / 2.0;
    let mut g = empty_geometry();
    g.position = [center.x, center.y];
    match tool {
        Tool::Polygon | Tool::Star => {
            g.kind = if tool == Tool::Star {
                ShapeKind::Star
            } else {
                ShapeKind::Polygon
            };
            g.points = Some(options.points.max(3.0));
            g.outer_radius = Some(radius);
            g.inner_radius = Some(radius * STAR_RATIO);
        }
        _ => {
            g.kind = if tool == Tool::Ellipse {
                ShapeKind::Ellipse
            } else {
                ShapeKind::Rect
            };
            g.size = Some(size);
            if tool == Tool::RoundedRectangle {
                g.roundness = options.roundness.max(0.0).min(radius);
            }
        }
    }
    run(cx, session, info, &target, g, options.bezier_path, &options);
}

fn empty_geometry() -> Geometry {
    Geometry {
        kind: ShapeKind::Rect,
        position: [0.0, 0.0],
        size: None,
        roundness: 0.0,
        points: None,
        outer_radius: None,
        inner_radius: None,
        rotation: 0.0,
        outer_roundness: 0.0,
        inner_roundness: 0.0,
        path: None,
    }
}

/// Outline color of the shape being drawn: the next mask color on a mask target,
/// the shape layer's label color otherwise (AE draws the rubber band in it).
pub(super) fn preview_color(stage: &Stage, target: &Target) -> Color32 {
    match target {
        Target::Mask(layer) => {
            let count = layer
                .child("masks")
                .map(|m| stage.children(&m).len())
                .unwrap_or(0);
            let [r, g, b, _] = mask_color(count, true);
            let u = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Color32::from_rgb(u(r), u(g), u(b))
        }
        Target::Shape(layer) => {
            let label = stage
                .prim(layer)
                .and_then(|v| query::static_value(&v, "label"))
                .and_then(|v| v.as_f64())
                .map_or(SHAPE_LABEL, |v| v as usize);
            theme::label_color(label)
        }
        Target::NewLayer => theme::label_color(SHAPE_LABEL),
    }
}

/// Default label of new shape layers (AE: Blue).
const SHAPE_LABEL: usize = 8;

/// Shape-tool input for one viewer frame: start a drag with a shape tool, follow
/// the pointer, modifiers and keys, commit on release, cancel with Esc.
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    slot: &mut Option<ShapeDrag>,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
) {
    let tools = crate::tools::shared(cx.resources);
    if slot.is_none() && tool.is_shape() && resp.drag_started_by(PointerButton::Primary) {
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos());
        if let Some(pos) = origin {
            let options = tools.lock().options.clone();
            let selected = session.lock().selected_layers.clone();
            let target = Target::of(cx.stage, &info.path, &selected, options.creates_mask);
            let to_comp = target_matrix(info, comp, &target);
            let alt = ui.input(|i| i.modifiers.alt);
            let color = preview_color(cx.stage, &target);
            session.lock().stop();
            *slot = Some(ShapeDrag::new(
                tool,
                target,
                to_comp,
                view.to_comp(pos),
                &options,
                alt,
                color,
            ));
        }
    }
    let Some(drag) = slot.as_mut() else {
        return;
    };
    let (shift, command, space, middle, escape, keys, wheel, down) = ui.input(|i| {
        let keys: Vec<DragKey> = i
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    key, pressed: true, ..
                } => DragKey::of(*key),
                _ => None,
            })
            .collect();
        (
            i.modifiers.shift,
            i.modifiers.command,
            i.key_down(Key::Space),
            i.pointer.middle_down(),
            i.key_pressed(Key::Escape),
            keys,
            i.events
                .iter()
                .map(|e| match e {
                    Event::MouseWheel { unit, delta, .. } => match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * WHEEL_STEP,
                        egui::MouseWheelUnit::Page => delta.y * WHEEL_STEP * 10.0,
                    },
                    _ => 0.0,
                })
                .sum::<f32>(),
            i.pointer.primary_down(),
        )
    });
    if escape {
        *slot = None;
        return;
    }
    for k in keys {
        drag.key(k);
    }
    // The mouse wheel works like Up / Down (AE-SHP-052/053): one step per
    // wheel notch, trackpad scrolling accumulated.
    drag.wheel += wheel;
    while drag.wheel.abs() >= WHEEL_STEP {
        let up = drag.wheel > 0.0;
        drag.key(if up { DragKey::Up } else { DragKey::Down });
        drag.wheel -= WHEEL_STEP.copysign(drag.wheel);
    }
    if let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) {
        drag.update(view.to_comp(pos), shift, command, space || middle);
    }
    if resp.drag_stopped() || !down {
        let d = slot.take().expect("checked above");
        let options = tools.lock().options.clone();
        d.commit(cx, session, info, &options);
        // Each tool remembers its last roundness / points (AE-SHP-052/053).
        let mut t = tools.lock();
        match d.tool {
            Tool::RoundedRectangle => t.options.roundness = d.roundness,
            Tool::Polygon | Tool::Star => t.options.points = d.points,
            _ => {}
        }
    }
}
