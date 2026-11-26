//! The Motion workspace's toolbar (the `ae` keymap's [`Toolbar`]): the Home button,
//! one button per tool group with AE's flyouts, and the active tool's options
//! (AE-UI-100).
//!
//! - Click a button: its tool (the group's last used member). Alt/Option-click: the
//!   next member. Press and hold, right-click or click the corner triangle: the
//!   flyout listing the group's tools with their shortcut.
//! - Double-click: Selection resets Scale of the selected layers to 100%, Rotation
//!   resets Rotation to 0°, Hand centers the composition, Zoom goes to 100%, a shape
//!   tool adds a shape (or mask) the size of the layer or comp.
//! - Holding a tool's shortcut switches only while the key is down
//!   ([`ToolState::spring`]).

use std::sync::Mutex;
use std::time::Duration;

use bae_model::{PrimPath, Value};
use bae_motion::query;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{Keymap, PanelContext, Toolbar, tr, widgets};
use egui::{Align2, Color32, Id, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{
    AxisMode, FillRequest, GizmoMode, Group, Paint, PaintKind, SharedTools, Tool, ToolOptions,
    ViewRequest, ids,
};
use crate::common;

/// Press-and-hold time that opens a group's flyout.
const FLYOUT_HOLD: f64 = 0.35;
const BUTTON: f32 = 24.0;

/// The shortcut action shown for a group (tooltips and flyouts).
fn group_action(group: Group) -> Option<&'static str> {
    Some(match group {
        Group::Selection => ids::SELECTION,
        Group::Hand => ids::HAND,
        Group::Zoom => ids::ZOOM,
        Group::Orbit => ids::CAMERA_ORBIT,
        Group::Pan => ids::CAMERA_PAN,
        Group::Dolly => ids::CAMERA_DOLLY,
        Group::Rotation => ids::ROTATION,
        Group::PanBehind => ids::PAN_BEHIND,
        Group::Shape => ids::SHAPE,
        Group::Mesh => ids::PARAMETRIC_MESH,
        Group::Pen => ids::PEN,
        Group::Type => ids::TYPE,
        Group::Brush | Group::CloneStamp | Group::Eraser => ids::PAINT,
        Group::Roto => ids::ROTO_BRUSH,
        Group::Puppet => ids::PUPPET,
    })
}

fn shortcut_text(keymap: &Keymap, group: Group) -> Option<String> {
    let action = group_action(group)?;
    keymap.chords_for(action).next().map(|c| c.display())
}

#[derive(Default)]
struct ViewState {
    /// Group whose flyout is open.
    flyout: Option<Group>,
    /// It was opened by pressing and holding the button: the release is not a
    /// click that closes it.
    by_hold: bool,
}

/// The AE toolbar. Holds the [`SharedTools`] resource it was registered with.
pub struct AeToolbar {
    tools: SharedTools,
    view: Mutex<ViewState>,
}

impl AeToolbar {
    pub fn new(tools: SharedTools) -> Self {
        AeToolbar {
            tools,
            view: Mutex::new(ViewState::default()),
        }
    }

    fn view(&self) -> std::sync::MutexGuard<'_, ViewState> {
        self.view.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Toolbar for AeToolbar {
    fn ui(&self, ui: &mut Ui, cx: &mut PanelContext<'_>, keymap: &Keymap) {
        // Spring-loaded tools: back to the previous tool when the key is released.
        let pointer_pressed = ui.input(|i| i.pointer.any_pressed());
        let pending = self.tools.lock().spring(
            |action| {
                keymap
                    .chords_for(action)
                    .any(|c| ui.input(|i| i.key_down(c.key)))
            },
            pointer_pressed,
        );
        if pending {
            // Look at the key again soon even without input events.
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        ui.spacing_mut().item_spacing.x = 2.0;
        // Home (AE's Home screen): not available yet.
        ui.add_enabled_ui(false, |ui| {
            icons::button(ui, Icon::Home, BUTTON, false)
                .labeled(egui::WidgetType::Button, tr("Home"))
                .on_disabled_hover_text(tr("Home"));
        });
        ui.add_space(6.0);
        for group in Group::ALL {
            self.group_button(ui, cx, keymap, group);
        }
        ui.add_space(10.0);
        let sep = ui.cursor().left();
        ui.painter().vline(
            sep - 5.0,
            ui.max_rect().shrink2(vec2(0.0, 6.0)).y_range(),
            Stroke::new(1.0, color::DIVIDER),
        );
        self.options(ui, cx);
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(self.tools.lock().save())
    }

    fn load_state(&self, state: &serde_json::Value) {
        self.tools.lock().load(state);
    }
}

impl AeToolbar {
    fn group_button(&self, ui: &mut Ui, cx: &mut PanelContext<'_>, keymap: &Keymap, group: Group) {
        let (shown, selected) = {
            let t = self.tools.lock();
            (t.shown(group), t.current().group() == group)
        };
        let resp = icons::button(ui, shown.icon(), BUTTON, selected).labeled_toggle(
            egui::WidgetType::RadioButton,
            tr(shown.name()),
            selected,
        );
        let several = group.members().len() > 1;
        if several {
            // Flyout marker in the lower right corner.
            let r = resp.rect;
            let c = if selected {
                Color32::WHITE
            } else {
                color::ICON_DIM
            };
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    pos2(r.right() - 1.5, r.bottom() - 5.5),
                    pos2(r.right() - 1.5, r.bottom() - 1.5),
                    pos2(r.right() - 5.5, r.bottom() - 1.5),
                ],
                c,
                Stroke::NONE,
            ));
        }
        let tip = match shortcut_text(keymap, group) {
            Some(k) => format!("{} ({k})", tr(shown.name())),
            None => tr(shown.name()).to_owned(),
        };
        let resp = resp.on_hover_text(tip);

        let flyout_id = Id::new(("ae_tool_flyout", group as u8));
        let mut open_flyout = false;
        if several {
            let held = resp.is_pointer_button_down_on()
                && ui.input(|i| {
                    i.pointer
                        .press_start_time()
                        .is_some_and(|t| i.time - t >= FLYOUT_HOLD)
                });
            if resp.is_pointer_button_down_on() && !held {
                ui.ctx()
                    .request_repaint_after(Duration::from_secs_f64(FLYOUT_HOLD));
            }
            let on_triangle = resp.clicked()
                && resp.interact_pointer_pos().is_some_and(|p| {
                    p.x > resp.rect.right() - 7.0 && p.y > resp.rect.bottom() - 7.0
                });
            open_flyout = held || resp.secondary_clicked() || on_triangle;
        }
        if open_flyout {
            let mut v = self.view();
            v.flyout = Some(group);
            v.by_hold = resp.is_pointer_button_down_on();
            egui::Popup::open_id(ui.ctx(), flyout_id);
        }
        let flyout_open = egui::Popup::is_id_open(ui.ctx(), flyout_id);
        if resp.clicked() && !open_flyout {
            if flyout_open {
                // A click on the button closes its open flyout, except the
                // release of the press that opened it.
                if !std::mem::take(&mut self.view().by_hold) {
                    egui::Popup::close_id(ui.ctx(), flyout_id);
                }
            } else {
                let alt = ui.input(|i| i.modifiers.alt);
                let mut t = self.tools.lock();
                let tool = if alt && several {
                    t.next_in_group(group, true)
                } else {
                    shown
                };
                t.set(tool);
            }
        }
        if resp.double_clicked() {
            let modifiers = ui.input(|i| i.modifiers);
            self.double_click(cx, shown, modifiers);
        }
        if several && egui::Popup::is_id_open(ui.ctx(), flyout_id) {
            let mut picked = None;
            let shown_popup = egui::Popup::from_response(&resp)
                .id(flyout_id)
                .kind(egui::PopupKind::Menu)
                .open_memory(None)
                // Closed below: the release that ends a press-and-hold must not
                // close the flyout it opened.
                .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                .gap(2.0)
                .show(|ui| {
                    ui.set_min_width(220.0);
                    let key = shortcut_text(keymap, group).unwrap_or_default();
                    for &tool in group.members() {
                        let r = flyout_row(ui, tool, tool == shown, &key);
                        // Click a row, or press the button, move here and release.
                        let released_here = r.contains_pointer()
                            && ui.input(|i| i.pointer.primary_released());
                        if r.clicked() || released_here {
                            picked = Some(tool);
                        }
                    }
                });
            let popup_rect = shown_popup.map(|r| r.response.rect);
            let (clicked_at, escape) = ui.input(|i| {
                (
                    i.pointer
                        .any_click()
                        .then(|| i.pointer.interact_pos())
                        .flatten(),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            let outside = clicked_at.is_some_and(|p| {
                !resp.rect.contains(p) && !popup_rect.is_some_and(|r| r.contains(p))
            });
            if let Some(tool) = picked {
                self.tools.lock().set(tool);
            }
            if picked.is_some() || outside || escape {
                egui::Popup::close_id(ui.ctx(), flyout_id);
                self.view().flyout = None;
            }
        }
    }

    /// Double-click on a tool button (AE-UI-306).
    fn double_click(&self, cx: &mut PanelContext<'_>, tool: Tool, modifiers: egui::Modifiers) {
        match tool {
            Tool::Selection => reset_selected(cx, "transform:scale"),
            Tool::Rotation => reset_selected(cx, "transform:rotation"),
            Tool::Hand => self.tools.lock().view_request = Some(ViewRequest::Center),
            Tool::Zoom => self.tools.lock().view_request = Some(ViewRequest::Actual),
            // Cmd/Ctrl double-click: center the anchor points in the layers'
            // content (AE tutorials T05@05:31).
            Tool::PanBehind if modifiers.command => {
                self.tools.lock().center_anchor_request = true;
            }
            t if t.is_shape() => {
                self.tools.lock().fill_request = Some(FillRequest {
                    tool: t,
                    shift: modifiers.shift,
                });
            }
            _ => {}
        }
    }

    fn options(&self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let tool = self.tools.lock().current();
        let mut o = self.tools.lock().options.clone();
        let before = o.clone();
        ui.spacing_mut().item_spacing.x = 6.0;
        let session = common::session(cx.resources);
        let selected = session.lock().selected_layers.clone();
        let one_shape_layer = selected.len() == 1
            && query::layer_kind(cx.stage, &selected[0]).as_deref() == Some("shape");
        match tool.group() {
            Group::Selection
            | Group::Rotation
            | Group::PanBehind
            | Group::Orbit
            | Group::Pan
            | Group::Dolly => {
                axis_modes(ui, &mut o);
                if tool == Tool::Selection {
                    let items = [tr("Position"), tr("Scale"), tr("Rotation")];
                    let mut i = o.gizmo as usize;
                    if widgets::dropdown(ui, "gizmo", &mut i, &items, 74.0)
                        .on_hover_text(tr("Toggle 3D Gizmo Mode"))
                        .changed()
                    {
                        o.gizmo = [GizmoMode::Position, GizmoMode::Scale, GizmoMode::Rotation][i];
                    }
                }
                if tool == Tool::Rotation {
                    let items = [tr("Orientation"), tr("Rotation")];
                    let mut i = usize::from(!o.rotation_sets_orientation);
                    ui.label(small(tr("Set:")));
                    if widgets::dropdown(ui, "rotation_sets", &mut i, &items, 90.0).changed() {
                        o.rotation_sets_orientation = i == 0;
                    }
                }
                if !tool.is_camera() {
                    snapping(ui, &mut o);
                }
            }
            Group::Shape | Group::Pen => {
                if one_shape_layer {
                    creates_toggle(ui, &mut o);
                }
                // Fill and stroke apply to shapes, not masks.
                let shape_target = selected.is_empty() || (one_shape_layer && !o.creates_mask);
                ui.add_enabled_ui(shape_target, |ui| {
                    paint_controls(ui, &mut o);
                });
                add_menu(ui, cx, &o);
                if tool.is_pen() {
                    widgets::checkbox(ui, &mut o.roto_bezier, tr("RotoBezier"));
                } else {
                    widgets::checkbox(ui, &mut o.bezier_path, tr("Bezier Path"));
                }
                snapping(ui, &mut o);
            }
            Group::Type | Group::Brush | Group::CloneStamp | Group::Eraser => {
                widgets::checkbox(ui, &mut o.auto_open_panels, tr("Auto-Open Panels"));
            }
            Group::Puppet => {
                // Mesh: Show draws the selected layers' meshes; Expansion and
                // Density are the new Puppet effect's.
                ui.label(small(tr("Mesh:")));
                widgets::checkbox(ui, &mut o.puppet_show_mesh, tr("Show"));
                for (value, label, range) in [
                    (&mut o.puppet_expansion, "Expansion", 0.0..=100.0),
                    (&mut o.puppet_density, "Density", 1.0..=100.0),
                ] {
                    ui.label(small(tr(label)));
                    ui.add(
                        egui::DragValue::new(value)
                            .range(range)
                            .speed(0.25)
                            .max_decimals(0),
                    )
                    .labeled(egui::WidgetType::DragValue, tr(label))
                    .on_hover_text(tr(label));
                }
                ui.add_enabled_ui(false, |ui| widgets::link(ui, tr("Record Options...")));
            }
            Group::Hand | Group::Zoom | Group::Mesh | Group::Roto => {}
        }
        if o != before {
            self.tools.lock().options = o;
        }
    }
}

/// AE's Add menu of the shape and pen tools: `(item id, English title)`;
/// `None` is a separator.
const ADD_MENU: &[Option<(&str, &str)>] = &[
    Some(("group", "Group (empty)")),
    Some(("rect", "Rectangle")),
    Some(("ellipse", "Ellipse")),
    Some(("polystar", "Polystar")),
    Some(("path", "Path")),
    None,
    Some(("fill", "Fill")),
    Some(("stroke", "Stroke")),
    Some(("gradient_fill", "Gradient Fill")),
    Some(("gradient_stroke", "Gradient Stroke")),
    None,
    Some(("merge", "Merge Paths")),
    Some(("offset", "Offset Paths")),
    Some(("pucker_bloat", "Pucker & Bloat")),
    Some(("repeater", "Repeater")),
    Some(("round_corners", "Round Corners")),
    Some(("trim", "Trim Paths")),
    Some(("twist", "Twist")),
    Some(("wiggle_paths", "Wiggle Paths")),
    Some(("wiggle_transform", "Wiggle Transform")),
    Some(("zig_zag", "Zig Zag")),
];

/// Where Add puts an item: the shape groups or items selected in the
/// Timeline, else the selected shape layers.
fn add_targets(stage: &bae_model::Stage, session: &crate::session::SharedSession) -> Vec<PrimPath> {
    let s = session.lock();
    let in_shape = |p: &PrimPath| {
        query::owning_layer(stage, p)
            .is_some_and(|l| query::layer_kind(stage, &l).as_deref() == Some("shape"))
    };
    let rows: Vec<PrimPath> = s
        .selected_groups
        .iter()
        .filter_map(|id| crate::session::group_prim(id))
        .filter(|p| in_shape(p) && query::layer_kind(stage, p).is_none())
        .collect();
    if !rows.is_empty() {
        return rows;
    }
    s.selected_layers
        .iter()
        .filter(|l| query::layer_kind(stage, l).as_deref() == Some("shape"))
        .cloned()
        .collect()
}

/// The Add link and its menu (AE): one `shape.add_item` per pick.
fn add_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, o: &ToolOptions) {
    let session = common::session(cx.resources);
    let targets = add_targets(cx.stage, &session);
    let mut picked = None;
    ui.add_enabled_ui(!targets.is_empty(), |ui| {
        let resp = widgets::link(ui, tr("Add")).on_disabled_hover_text(tr("Add"));
        common::popup_menu(&resp, &mut |ui| {
            for entry in ADD_MENU {
                match entry {
                    None => {
                        ui.separator();
                    }
                    Some((id, title)) => {
                        if ui.button(tr(title)).clicked() {
                            picked = Some(*id);
                            ui.close();
                        }
                    }
                }
            }
        });
    });
    let Some(item) = picked else {
        return;
    };
    let mut params = json!({"targets": targets, "item": item});
    match item {
        "fill" => params["color"] = json!(o.fill.color),
        "stroke" => {
            params["color"] = json!(o.stroke.color);
            params["width"] = json!(o.stroke_width);
        }
        _ => {}
    }
    cx.invoke_with("shape.add_item", params);
}

fn small(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .font(theme::body())
        .color(color::TEXT)
}

fn flyout_row(ui: &mut Ui, tool: Tool, current: bool, key: &str) -> Response {
    let w = ui.available_width().max(200.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
    let resp = resp.labeled_toggle(egui::WidgetType::RadioButton, tr(tool.name()), current);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 2.0, color::ACCENT_DIM);
    }
    if current {
        p.rect_filled(
            Rect::from_min_size(rect.min + vec2(3.0, 9.0), vec2(4.0, 4.0)),
            1.0,
            color::TEXT_STRONG,
        );
    }
    icons::paint(
        p,
        Rect::from_min_size(rect.min + vec2(12.0, 3.0), vec2(16.0, 16.0)),
        tool.icon(),
        color::ICON,
    );
    p.text(
        rect.left_center() + vec2(36.0, 0.0),
        Align2::LEFT_CENTER,
        tr(tool.name()),
        theme::body(),
        color::TEXT_STRONG,
    );
    p.text(
        rect.right_center() - vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        key,
        theme::body(),
        color::TEXT_DIM,
    );
    resp
}

fn axis_modes(ui: &mut Ui, o: &mut ToolOptions) {
    for (mode, icon, name) in [
        (AxisMode::Local, Icon::AxisLocal, "Local Axis Mode"),
        (AxisMode::World, Icon::AxisWorld, "World Axis Mode"),
        (AxisMode::View, Icon::AxisView, "View Axis Mode"),
    ] {
        let on = o.axis_mode == mode;
        if icons::button(ui, icon, 20.0, on)
            .labeled_toggle(egui::WidgetType::RadioButton, tr(name), on)
            .on_hover_text(tr(name))
            .clicked()
        {
            o.axis_mode = mode;
        }
    }
}

/// Snapping (AE-UI-357): layers snap while dragged in the viewer (Cmd/Ctrl
/// inverts it while held). The two options next to it (extend layer edges,
/// snap to features of collapsed and text layers) are not available yet.
fn snapping(ui: &mut Ui, o: &mut ToolOptions) {
    widgets::checkbox(ui, &mut o.snapping, tr("Snapping"));
    // AE's two snapping options, usable while Snapping is on.
    ui.add_enabled_ui(o.snapping, |ui| {
        for (flag, icon, name) in [
            (
                &mut o.snap_edges,
                Icon::SnapEdges,
                "Snap to edges extended beyond layer bounds",
            ),
            (
                &mut o.snap_features,
                Icon::SnapFeatures,
                "Snap to features inside collapsed compositions and text layers",
            ),
        ] {
            let on = *flag;
            if icons::button(ui, icon, 20.0, on)
                .labeled_toggle(egui::WidgetType::Checkbox, tr(name), on)
                .on_hover_text(tr(name))
                .clicked()
            {
                *flag = !on;
            }
        }
    });
}

fn creates_toggle(ui: &mut Ui, o: &mut ToolOptions) {
    for (mask, icon, name) in [
        (false, Icon::Star, "Tool Creates Shape"),
        (true, Icon::MaskVisibility, "Tool Creates Mask"),
    ] {
        let on = o.creates_mask == mask;
        if icons::button(ui, icon, 20.0, on)
            .labeled_toggle(egui::WidgetType::RadioButton, tr(name), on)
            .on_hover_text(tr(name))
            .clicked()
        {
            o.creates_mask = mask;
        }
    }
}

fn color32(c: [f64; 4]) -> Color32 {
    let u = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(u(c[0]), u(c[1]), u(c[2]))
}

/// Fill: [swatch] Stroke: [swatch] [width] px.
fn paint_controls(ui: &mut Ui, o: &mut ToolOptions) {
    paint_control(
        ui,
        "fill",
        "Fill:",
        "Fill Options",
        "Fill Color",
        &mut o.fill,
        false,
    );
    paint_control(
        ui,
        "stroke",
        "Stroke:",
        "Stroke Options",
        "Stroke Color",
        &mut o.stroke,
        true,
    );
    let w = egui::DragValue::new(&mut o.stroke_width)
        .range(0.0..=1000.0)
        .speed(0.25)
        .max_decimals(1)
        .suffix(format!(" {}", tr("px")));
    ui.add_enabled(o.stroke.kind != PaintKind::None, w)
        .labeled(egui::WidgetType::DragValue, tr("Stroke Width"))
        .on_hover_text(tr("Stroke Width"));
}

fn paint_control(
    ui: &mut Ui,
    id: &str,
    label: &str,
    options: &str,
    color_name: &str,
    paint: &mut Paint,
    stroke: bool,
) {
    // The label opens Fill / Stroke Options (type, blend mode, opacity).
    let lr = widgets::link(ui, tr(label)).on_hover_text(tr(options));
    common::popup_menu(&lr, &mut |ui| {
        ui.set_min_width(180.0);
        ui.label(egui::RichText::new(tr(options)).strong());
        for (kind, name, available) in [
            (PaintKind::None, "None", true),
            (PaintKind::Solid, "Solid Color", true),
            (PaintKind::LinearGradient, "Linear Gradient", false),
            (PaintKind::RadialGradient, "Radial Gradient", false),
        ] {
            ui.add_enabled_ui(available, |ui| {
                if ui.radio(paint.kind == kind, tr(name)).clicked() {
                    paint.kind = kind;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(tr("Opacity"));
            ui.add(
                egui::DragValue::new(&mut paint.opacity)
                    .range(0.0..=100.0)
                    .suffix("%"),
            );
        });
        ui.add_enabled_ui(false, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Blend Mode"));
                ui.label(tr("Normal"));
            });
        });
    });
    // The swatch: Alt/Option-click cycles the type, click picks the color.
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 16.0), Sense::click());
    let hex = {
        let c = color32(paint.color);
        format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
    };
    let resp = resp
        .labeled_value(egui::WidgetType::ColorButton, tr(color_name), &hex)
        .on_hover_text(tr(color_name));
    paint_swatch(ui, rect, paint, stroke, resp.hovered());
    let alt = ui.input(|i| i.modifiers.alt);
    if resp.clicked() && alt {
        paint.kind = match paint.kind {
            PaintKind::None => PaintKind::Solid,
            _ => PaintKind::None,
        };
        return;
    }
    let picker = egui::Popup::from_response(&resp)
        .id(Id::new(("paint_color", id)))
        .open_memory((resp.clicked() && !alt).then_some(egui::SetOpenCommand::Toggle))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    picker.show(|ui| {
        let mut c = color32(paint.color);
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            paint.color = [
                f64::from(c.r()) / 255.0,
                f64::from(c.g()) / 255.0,
                f64::from(c.b()) / 255.0,
                1.0,
            ];
            if paint.kind == PaintKind::None {
                paint.kind = PaintKind::Solid;
            }
        }
    });
}

fn paint_swatch(ui: &Ui, rect: Rect, paint: &Paint, stroke: bool, hovered: bool) {
    let p = ui.painter();
    let c = color32(paint.color);
    match paint.kind {
        PaintKind::None => {
            p.rect_filled(rect, 1.0, Color32::WHITE);
            p.line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(1.5, Color32::from_rgb(220, 30, 30)),
            );
        }
        _ if stroke => {
            p.rect_filled(rect, 1.0, c);
            p.rect_filled(rect.shrink(4.0), 0.0, color::PANEL);
        }
        _ => {
            p.rect_filled(rect, 1.0, c);
        }
    }
    let border = if hovered {
        color::TEXT_STRONG
    } else {
        color::FIELD_BORDER
    };
    p.rect_stroke(
        rect,
        1.0,
        Stroke::new(1.0, border),
        egui::StrokeKind::Outside,
    );
}

/// Double-clicking Selection / Rotation: reset Scale / Rotation of the selected
/// layers at the current time (one undo step).
fn reset_selected(cx: &mut PanelContext<'_>, attr: &str) {
    let session = common::session(cx.resources);
    let (layers, time) = {
        let s = session.lock();
        (s.selected_layers.clone(), s.active_time())
    };
    let edits: Vec<serde_json::Value> = layers
        .iter()
        .filter(|l| cx.stage.has_prim(l))
        .map(|l| {
            let value = if attr == "transform:scale" {
                Value::Vec3([100.0, 100.0, 100.0])
            } else {
                Value::Float(0.0)
            };
            json!({
                "path": l,
                "attr": attr,
                "time": query::to_key_time(cx.stage, l, time),
                "value": value,
            })
        })
        .collect();
    if !edits.is_empty() {
        cx.invoke_with("attr.set_many", json!({ "edits": edits }));
    }
}
