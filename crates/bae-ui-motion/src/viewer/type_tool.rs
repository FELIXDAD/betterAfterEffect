//! Horizontal Type tool in the Composition viewer.
//!
//! Click: new point text at the click (the first baseline starts there); drag: a
//! paragraph text box; click a text layer: edit its text. Typing goes into an
//! egui `TextEdit` laid over the viewer, so input methods work (the candidate
//! window follows the caret through the workbench, see 11's IME report). Esc,
//! switching tools, or clicking away commits: one `text.create` for new text or
//! one `text.set_text` for an edit (one undo step). Empty new text creates
//! nothing. Layer > New > Text (E) starts editing through
//! [`crate::tools::ToolState::edit_text_request`].

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::PrimPath;
use bae_motion::query::{self, CompInfo};
use bae_motion::text::{DEFAULT_SIZE, TextReplacement};
use bae_ui::PanelContext;
use egui::{
    Color32, FontId, Id, Key, Painter, PointerButton, Pos2, Rect, Response, Stroke, Ui, vec2,
};
use serde_json::json;

use crate::render::View;
use crate::session::{Pending, SharedSession};
use crate::tools::Tool;

/// Text being typed or edited in the viewer.
pub(crate) struct Editing {
    comp: PrimPath,
    /// The text layer being edited; `None` for new text.
    pub(crate) layer: Option<PrimPath>,
    /// Layer space → comp space (new text: the origin is the click / box corner).
    to_comp: DMat4,
    /// Paragraph box in layer space (top-left, size).
    text_box: Option<([f64; 2], [f64; 2])>,
    pub(crate) buffer: String,
    original: String,
    edits: Vec<TextReplacement>,
    selection: Option<std::ops::Range<usize>>,
    drag_end: Option<Pos2>,
    font_size: f64,
    color: Color32,
    /// Focus was requested / the field had it (losing it commits).
    focused_once: bool,
    /// Where an existing text was clicked: the caret goes there (screen).
    caret_at: Option<Pos2>,
    /// The viewer was clicked outside the field: take the focus back.
    refocus: bool,
}

/// Type tool state of the viewer.
#[derive(Default)]
pub(crate) struct TypeState {
    pub(crate) editing: Option<Editing>,
    /// A press waiting for its release (screen position, the text layer under
    /// it): a click edits that layer or starts point text, a drag makes a
    /// paragraph box. Editing starts on release so the field keeps its focus.
    press: Option<(Pos2, Option<PrimPath>)>,
}

impl TypeState {
    /// The layer whose text is being edited (the viewer hides its rendering).
    pub(crate) fn edited_layer(&self) -> Option<&PrimPath> {
        self.editing.as_ref().and_then(|e| e.layer.as_ref())
    }
}

fn text_id() -> Id {
    Id::new("bae.viewer.type_tool")
}

fn color32(c: [f64; 4]) -> Color32 {
    let u = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(u(c[0]), u(c[1]), u(c[2]))
}

/// Start editing an existing text layer.
fn edit_layer(
    comp: &EvaluatedComp,
    info: &CompInfo,
    layer: &PrimPath,
    caret_at: Option<Pos2>,
) -> Option<Editing> {
    let l = comp.layer(layer)?;
    let doc = l.text.as_ref()?;
    let text_box = doc
        .box_size
        .map(|s| (doc.box_position.unwrap_or([0.0, 0.0]), s));
    Some(Editing {
        comp: info.path.clone(),
        layer: Some(layer.clone()),
        to_comp: l.world_matrix,
        text_box,
        buffer: doc.text.clone(),
        original: doc.text.clone(),
        edits: vec![],
        selection: None,
        drag_end: None,
        font_size: doc.style.font_size,
        color: color32(doc.style.fill_color),
        focused_once: false,
        caret_at,
        refocus: false,
    })
}

fn new_text(
    info: &CompInfo,
    origin: DVec2,
    text_box: Option<[f64; 2]>,
    defaults: Option<&bae_model::TextDocument>,
) -> Editing {
    // The Character panel's settings for new text (AE), else the defaults.
    let (font_size, color) = defaults.map_or((DEFAULT_SIZE, Color32::WHITE), |d| {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0) as u8;
        let f = d.style.fill_color;
        (
            d.style.font_size,
            Color32::from_rgb(c(f[0]), c(f[1]), c(f[2])),
        )
    });
    Editing {
        comp: info.path.clone(),
        layer: None,
        to_comp: DMat4::from_translation(DVec3::new(origin.x, origin.y, 0.0)),
        text_box: text_box.map(|s| ([0.0, 0.0], s)),
        buffer: String::new(),
        original: String::new(),
        edits: vec![],
        selection: None,
        drag_end: None,
        font_size,
        color,
        focused_once: false,
        caret_at: None,
        refocus: false,
    }
}

impl Editing {
    /// Points per layer pixel on screen.
    fn scale(&self, view: &View) -> f32 {
        let x = self.to_comp.transform_vector3(DVec3::X);
        (x.length() as f32) * view.scale
    }

    fn screen(&self, view: &View, p: [f64; 2]) -> Pos2 {
        let w = self.to_comp.transform_point3(DVec3::new(p[0], p[1], 0.0));
        view.to_screen(DVec2::new(w.x, w.y))
    }

    /// Where the text field goes on screen.
    fn rect(&self, view: &View, area: Rect) -> Rect {
        let s = self.scale(view);
        let size = (self.font_size as f32 * s).max(4.0);
        match self.text_box {
            Some((pos, b)) => {
                let a = self.screen(view, pos);
                Rect::from_min_size(
                    a,
                    vec2((b[0] as f32 * s).max(8.0), (b[1] as f32 * s).max(size)),
                )
            }
            None => {
                // The first baseline starts at the layer origin.
                let o = self.screen(view, [0.0, 0.0]);
                let lines = self.buffer.lines().count().max(1) as f32 + 1.0;
                let top = o.y - size * 0.8;
                Rect::from_min_max(
                    egui::pos2(o.x, top),
                    egui::pos2(area.right().max(o.x + 40.0), top + size * 1.25 * lines),
                )
            }
        }
    }

    fn commit(self, cx: &mut PanelContext<'_>, session: &SharedSession) {
        let stage = cx.stage;
        match &self.layer {
            Some(layer) => {
                if (!self.edits.is_empty() || self.buffer != self.original) && stage.has_prim(layer)
                {
                    let time = session.lock().time(&self.comp);
                    cx.invoke_with(
                        "text.set_text",
                        json!({"layer": layer, "text": self.buffer, "edits":self.edits,
                               "time": query::to_key_time(stage, layer, time)}),
                    );
                }
            }
            None => {
                let origin = self.to_comp.transform_point3(DVec3::ZERO);
                let folder = query::layers_folder(&self.comp);
                // AE names a text layer after its text.
                let name = self
                    .buffer
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                let name = if name.is_empty() {
                    // AE keeps a new text layer even without text.
                    bae_motion::names::localized("<empty text layer>").to_owned()
                } else {
                    name
                };
                let index = {
                    let s = session.lock();
                    let layers = query::comp_layers(stage, &self.comp);
                    s.selected_layers
                        .iter()
                        .filter_map(|l| layers.iter().position(|x| x == l))
                        .min()
                        .unwrap_or(0)
                };
                let mut params = json!({
                    "comp": self.comp, "text": self.buffer, "name": name,
                    "position": [origin.x, origin.y], "index": index,
                });
                if let Some((_, b)) = self.text_box {
                    params["box_size"] = json!(b);
                }
                if let Some(d) = session.lock().text_defaults.clone() {
                    params["font"] = json!(d.style.font);
                    params["font_size"] = json!(d.style.font_size);
                    params["fill_color"] = json!(d.style.fill_color);
                    params["justification"] = json!(d.paragraph.justification);
                }
                cx.invoke_with("text.create", params);
                if let Ok(path) = folder.child(&query::unique_child_name(stage, &folder, &name)) {
                    let mut s = session.lock();
                    if let Some(range) = self.selection {
                        s.text_selection = Some((path.clone(), s.time(&self.comp), range));
                    }
                    s.expect(Pending::Layer(path.clone()));
                    s.reserve_name(path, name);
                }
            }
        }
    }
}

/// The text layer under `pos`.
fn hit_text(
    stage: &bae_model::Stage,
    comp: &EvaluatedComp,
    view: &View,
    pos: Pos2,
) -> Option<PrimPath> {
    let hit = super::select_tool::hit_layer(stage, comp, view, pos)?;
    (comp.layer(&hit)?.kind == bae_model::LayerKind::Text).then_some(hit)
}

/// The editor reports exact replacements, including repeated identical letters.
struct EditBuffer<'a> {
    text: &'a mut String,
    edits: &'a mut Vec<TextReplacement>,
    deleted: bool,
}

impl egui::TextBuffer for EditBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        self.text
    }
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<EditBuffer<'static>>()
    }
    fn insert_text(&mut self, text: &str, at: egui::text::CharIndex) -> usize {
        let n = self.text.insert_text(text, at);
        if self.deleted
            && let Some(edit) = self.edits.last_mut()
            && edit.range.start == at.0
        {
            edit.text.push_str(text);
        } else {
            self.edits.push(TextReplacement {
                range: at.0..at.0,
                text: text.into(),
            });
        }
        self.deleted = false;
        n
    }
    fn delete_char_range(&mut self, range: std::ops::Range<egui::text::CharIndex>) {
        if !range.is_empty() {
            self.edits.push(TextReplacement {
                range: range.start.0..range.end.0,
                text: String::new(),
            });
            self.text.delete_char_range(range);
            self.deleted = true;
        }
    }
}

/// Type tool input and the text field for one viewer frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    state: &mut TypeState,
    ui: &mut Ui,
    resp: &Response,
    area: Rect,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &Painter,
) {
    let tools = crate::tools::shared(cx.resources);
    // Layer > New > Text and the like: edit this layer now.
    let request = tools.lock().edit_text_request.take();
    let request_started = request.is_some();
    if let Some(layer) = request {
        if let Some(e) = state.editing.take() {
            e.commit(cx, session);
        }
        if let Some(e) = edit_layer(comp, info, &layer, None) {
            tools.lock().set(Tool::TypeHorizontal);
            session.lock().selected_layers = vec![layer];
            state.editing = Some(e);
        }
    }
    let current = tools.lock().current();
    if current != Tool::TypeHorizontal || request_started {
        session.lock().text_selection = None;
    }
    // Switching tools or comps commits.
    if state
        .editing
        .as_ref()
        .is_some_and(|e| current != Tool::TypeHorizontal || e.comp != info.path)
    {
        let e = state.editing.take().expect("checked above");
        if e.comp == info.path {
            e.commit(cx, session);
        }
    }
    if let Some(e) = &state.editing
        && let Some(layer) = &e.layer
        && !cx.stage.has_prim(layer)
    {
        state.editing = None;
    }

    let field = state.editing.as_ref().map(|e| e.rect(view, area));
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    let press_pos = ui.input(|i| i.pointer.press_origin());
    if tool == Tool::TypeHorizontal
        && pressed
        && resp.hovered()
        && let Some(pos) = press_pos
    {
        let inside = field.is_some_and(|r| r.contains(pos));
        // While editing, a click elsewhere in the viewer keeps editing (AE 26:
        // Esc, switching tools or Cmd/Ctrl+Enter commit).
        match state.editing.as_mut() {
            Some(e) if !inside => e.refocus = true,
            Some(_) => {}
            None => {
                session.lock().text_selection = None;
                state.press = Some((pos, hit_text(cx.stage, comp, view, pos)));
            }
        }
    }
    if let Some((origin, hit)) = state.press.clone() {
        let pointer = resp
            .interact_pointer_pos()
            .or_else(|| ui.input(|i| i.pointer.latest_pos()));
        let down = ui.input(|i| i.pointer.primary_down());
        let dragged = hit.is_none() && pointer.is_some_and(|p| p.distance(origin) > 3.0);
        if let Some(p) = pointer
            && dragged
        {
            painter.rect_stroke(
                Rect::from_two_pos(origin, p),
                0.0,
                Stroke::new(1.0, Color32::from_gray(210)),
                egui::StrokeKind::Middle,
            );
        }
        if !down || resp.clicked_by(PointerButton::Primary) || resp.drag_stopped() {
            state.press = None;
            let a = view.to_comp(origin);
            state.editing = match hit {
                Some(layer) => {
                    session.lock().selected_layers = vec![layer.clone()];
                    edit_layer(comp, info, &layer, Some(origin)).map(|mut e| {
                        e.drag_end = pointer.filter(|p| p.distance(origin) > 3.);
                        e
                    })
                }
                None => {
                    let defaults = session.lock().text_defaults.clone();
                    Some(match pointer.filter(|_| dragged) {
                        Some(p) => {
                            let b = view.to_comp(p);
                            let (lo, hi) = (a.min(b), a.max(b));
                            new_text(
                                info,
                                lo,
                                Some([hi.x - lo.x, hi.y - lo.y]),
                                defaults.as_ref(),
                            )
                        }
                        None => new_text(info, a, None, defaults.as_ref()),
                    })
                }
            };
        }
    }

    // The text field.
    let Some(e) = state.editing.as_mut() else {
        return;
    };
    let rect = e.rect(view, area);
    if let Some((pos, b)) = e.text_box {
        let corners = [
            e.screen(view, pos),
            e.screen(view, [pos[0] + b[0], pos[1]]),
            e.screen(view, [pos[0] + b[0], pos[1] + b[1]]),
            e.screen(view, [pos[0], pos[1] + b[1]]),
        ];
        painter.add(egui::Shape::closed_line(
            corners.to_vec(),
            Stroke::new(1.0, Color32::from_gray(160)),
        ));
    }
    // Read before the field sees the keys (it would type the Enter).
    let (escape, command_enter) = ui.input_mut(|i| {
        (
            i.key_pressed(Key::Escape),
            i.consume_key(egui::Modifiers::COMMAND, Key::Enter),
        )
    });
    let size = (e.font_size as f32 * e.scale(view)).max(4.0);
    let scale = e.scale(view);
    let template = e
        .layer
        .as_ref()
        .and_then(|layer| comp.layer(layer))
        .and_then(|l| l.text.as_deref());
    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, width: f32| {
        let mut job = match template {
            Some(doc) if doc.text == buffer.as_str() => {
                crate::render::text_job(doc, scale, 1., &FontId::proportional)
            }
            Some(doc) => {
                let mut edited = doc.clone();
                edited.set_text(buffer.as_str());
                crate::render::text_job(&edited, scale, 1., &FontId::proportional)
            }
            None => egui::text::LayoutJob::simple(
                buffer.as_str().to_owned(),
                FontId::proportional(size),
                e.color,
                width,
            ),
        };
        job.wrap.max_width = width;
        ui.painter().layout_job(job)
    };
    let mut buffer = EditBuffer {
        text: &mut e.buffer,
        edits: &mut e.edits,
        deleted: false,
    };
    let edit = egui::TextEdit::multiline(&mut buffer)
        .id(text_id())
        .frame(egui::Frame::NONE)
        .layouter(&mut layouter)
        .font(FontId::proportional(size))
        .text_color(e.color)
        .desired_width(rect.width())
        .desired_rows(1)
        .margin(egui::Margin::ZERO);
    let shown = ui
        .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| edit.show(ui))
        .inner;
    let out = shown.response.response.clone();
    if !e.focused_once {
        out.request_focus();
        e.focused_once = true;
    }
    // AE puts the insertion point where the text was clicked (otherwise at
    // the end).
    let initial = e.caret_at.take().map(|p| {
        let anchor = shown.galley.cursor_from_pos(p - shown.galley_pos);
        let end = e.drag_end.take().map_or(anchor, |p| {
            shown.galley.cursor_from_pos(p - shown.galley_pos)
        });
        egui::text::CCursorRange::two(anchor, end)
    });
    let cursor = initial.or(shown.cursor_range);
    if initial.is_some() || (cursor.is_none() && out.has_focus()) {
        let mut state = shown.state;
        state.cursor.set_char_range(cursor.or_else(|| {
            Some(egui::text::CCursorRange::one(egui::text::CCursor::new(
                e.buffer.chars().count(),
            )))
        }));
        state.store(ui.ctx(), text_id());
    }
    if (out.has_focus() || initial.is_some())
        && let Some(cursor) = cursor
    {
        let [a, b] = cursor.sorted_cursors();
        e.selection = (a.index != b.index).then_some(a.index.0..b.index.0);
        if let Some(layer) = &e.layer {
            let mut s = session.lock();
            s.text_selection = e
                .selection
                .clone()
                .map(|range| (layer.clone(), s.active_time(), range));
        }
    }
    if e.refocus && !escape && !command_enter && !out.has_focus() {
        // Clicked elsewhere in the viewer: keep typing.
        out.request_focus();
        if !ui.input(|i| i.pointer.any_down()) {
            e.refocus = false;
        }
    } else if escape {
        session.lock().text_selection = None;
        // Esc cancels (AE): new text is not created, edits are dropped (#573).
        state.editing = None;
    } else if command_enter || (out.lost_focus() && !out.has_focus()) {
        let mut e = state.editing.take().expect("editing");
        if command_enter && e.buffer.ends_with('\n') {
            // The Enter of Cmd/Ctrl+Enter is not part of the text.
            let n = e.buffer.chars().count();
            e.buffer.pop();
            e.edits.push(TextReplacement {
                range: n - 1..n,
                text: String::new(),
            });
        }
        if command_enter {
            session.lock().text_selection = None;
            e.selection = None;
        }
        e.commit(cx, session);
    }
}
