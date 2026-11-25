//! Smaller panels: Properties (AE 2024+ contextual properties of the selected layer),
//! Effect Controls (F3) and Preview (playback controls).

use bae_model::{PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::actions::{self, ids};
use crate::common::{self, EvalCache};
use crate::props::{self, Node};
use crate::session::SharedSession;

/// One editable property line: stopwatch, label, value.
#[allow(clippy::too_many_arguments)]
fn prop_line(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    eval: &mut EvalCache,
    path: &PrimPath,
    attr: &str,
    label: &str,
    now: Time,
    dims: usize,
) {
    let stage = cx.stage;
    let spec = query::attr_spec(stage, path, attr)
        .cloned()
        .unwrap_or_default();
    let key_now = query::to_key_time(stage, path, now);
    let row = ui.horizontal(|ui| {
        ui.set_min_height(20.0);
        let mut on = spec.is_animated();
        let animatable = stage
            .prim(path)
            .map(|v| bae_model::schema::is_animatable(v.prim_type(), attr))
            .unwrap_or(false);
        if animatable {
            if widgets::stopwatch_for(ui, &mut on, tr(label)).clicked() {
                cx.invoke_with(
                    "attr.set_animated",
                    json!({"path": path, "attr": attr, "animated": !spec.is_animated(), "time": key_now}),
                );
            }
        } else {
            ui.add_space(16.0);
        }
        let (r, _) = ui.allocate_exact_size(vec2(96.0, 18.0), Sense::hover());
        widgets::clipped_text(ui.painter(), pos2(r.left(), r.center().y), label, theme::body(), color::TEXT, r.width());
        let value = session
            .lock()
            .scrub
            .as_ref()
            .filter(|s| s.path == *path && s.attr == attr)
            .map(|s| s.value.clone())
            .or_else(|| common::current_value(eval.get(cx.resources), stage, path, attr, now));
        if let Some(v) = value {
            let id = ui.id().with(("prop", path.as_str(), attr));
            let expression = spec.expression.as_ref().is_some_and(|e| e.enabled);
            let edit = common::value_editor(ui, id, attr, &v, dims, tr(label), expression);
            common::apply_edit(cx, session, path, attr, now, edit);
        }
    });
    // An expression pick whip dragged from the Timeline.
    if let Some(whip) = row.response.dnd_release_payload::<crate::expr::Whip>() {
        crate::expr::link(cx, eval.get(cx.resources), &whip, path, Some(attr), now);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    ui.painter().text(
        pos2(r.left() + 2.0, r.center().y),
        Align2::LEFT_CENTER,
        title,
        theme::body(),
        color::TEXT_STRONG,
    );
    ui.painter().hline(
        r.x_range(),
        r.bottom() - 0.5,
        egui::Stroke::new(1.0, color::DIVIDER),
    );
}

fn selected_layer(stage: &Stage, session: &SharedSession) -> Option<PrimPath> {
    let s = session.lock();
    s.selected_layers
        .first()
        .cloned()
        .filter(|l| stage.has_prim(l))
}

// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct PropertiesPanel {
    eval: EvalCache,
    title: String,
}

impl Panel for PropertiesPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        if self.title.is_empty() {
            TabInfo::new(tr("Properties"))
        } else {
            TabInfo::new(format!("{}: {}", tr("Properties"), self.title))
        }
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let stage = cx.stage;
        let Some(layer) = selected_layer(stage, &session) else {
            self.title.clear();
            common::empty_hint(
                ui,
                tr("No layer selected"),
                tr("Select a layer to see its properties."),
            );
            return;
        };
        self.title = query::display_name_of(stage, &layer);
        let now = session.lock().active_time();
        let dims = if common::is_3d(stage, &layer) { 3 } else { 2 };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                section_title(ui, tr("Layer Transform"));
                if widgets::link(ui, tr("Reset")).clicked() {
                    for attr in props::transform_attrs(stage, &layer) {
                        cx.invoke_with("attr.clear", json!({"path": layer, "attr": attr}));
                    }
                }
            });
            let three_d = common::is_3d(stage, &layer);
            for attr in props::transform_attrs(stage, &layer) {
                let label = common::attr_label(None, attr, three_d);
                prop_line(ui, cx, &session, &mut self.eval, &layer, attr, &label, now, dims);
            }
            // Text.
            if query::layer_kind(stage, &layer).as_deref() == Some("text") {
                section(ui, tr("Text"));
                let doc = common::current_value(self.eval.get(cx.resources), stage, &layer, "text:source", now);
                if let Some(Value::TextDocument(doc)) = doc {
                    let key = ui.id().with(("text_src", layer.as_str()));
                    let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| doc.text.clone());
                    let value = text.clone();
                    let r = ui
                        .add(egui::TextEdit::multiline(&mut text).desired_rows(2).desired_width(f32::INFINITY))
                        .labeled_value(egui::WidgetType::TextEdit, tr("Source Text"), &value);
                    if r.changed() {
                        ui.data_mut(|d| d.insert_temp(key, text.clone()));
                    }
                    let mut d2 = (*doc).clone();
                    let mut commit = false;
                    if r.lost_focus() {
                        ui.data_mut(|d| d.remove::<String>(key));
                        if text != doc.text {
                            d2.text = text;
                            commit = true;
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Font Size")).color(color::TEXT));
                        let mut size = d2.font_size;
                        let px = format!(" {}", tr("px")); // 像素 in Chinese (#197)
                        let r = ui.add(widgets::HotNumber::new(&mut size).decimals(0).suffix(&px).range(1.0..=2000.0).label(tr("Font Size")));
                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                            d2.font_size = size;
                            commit = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Fill Color")).color(color::TEXT));
                        let mut c = d2.fill_color.map(|v| v as f32);
                        if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                            d2.fill_color = c.map(f64::from);
                            commit = true;
                        }
                    });
                    if commit {
                        let key_now = query::to_key_time(stage, &layer, now);
                        cx.invoke_with(
                            "attr.set_at",
                            json!({"path": layer, "attr": "text:source", "time": key_now, "value": Value::TextDocument(Box::new(d2))}),
                        );
                    }
                }
            }
            // Shape contents: fills and strokes of the layer's groups.
            let tree = props::layer_tree(stage, &layer);
            if let Some(Node::Group { children, .. }) = tree.iter().find(|n| matches!(n, Node::Group { id, .. } if id.ends_with("#contents"))) {
                section(ui, tr("Layer Contents"));
                let mut flat = Vec::new();
                collect_props(children, &mut flat);
                for (path, attr, label) in flat.into_iter().filter(|(_, a, _)| a == "color" || a == "size" || a == "width") {
                    let owner = query::display_name_of(stage, &path);
                    prop_line(ui, cx, &session, &mut self.eval, &path, &attr, &format!("{owner} · {label}"), now, 2);
                }
            }
        });
    }
}

fn section_title(ui: &mut Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .color(color::TEXT_STRONG)
            .font(theme::body()),
    );
}

fn collect_props(nodes: &[Node], out: &mut Vec<(PrimPath, String, String)>) {
    for n in nodes {
        match n {
            Node::Group { children, .. } => collect_props(children, out),
            Node::Prop { path, attr, label } => {
                out.push((path.clone(), attr.clone(), label.clone()))
            }
        }
    }
}

// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct EffectControlsPanel {
    eval: EvalCache,
    title: String,
    controls: crate::effects::controls::ControlsState,
}

impl Panel for EffectControlsPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        if self.title.is_empty() {
            TabInfo::new(tr("Effect Controls"))
        } else {
            TabInfo::new(format!("{} {}", tr("Effect Controls"), self.title))
        }
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let stage = cx.stage;
        let Some(layer) = selected_layer(stage, &session) else {
            self.title.clear();
            common::empty_hint(
                ui,
                tr("No layer selected"),
                tr("Select a layer to see its properties."),
            );
            return;
        };
        self.title = query::display_name_of(stage, &layer);
        let now = session.lock().active_time();
        let effects = props::effect_nodes(stage, &layer);
        if effects.is_empty() {
            common::empty_hint(ui, &self.title.clone(), tr("No effects applied"));
            return;
        }
        // Controls from the effect definitions (07's built-ins, plugins).
        if crate::effects::registries(cx.resources).is_some() {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    crate::effects::controls::show(
                        ui,
                        cx,
                        &session,
                        &mut self.eval,
                        &mut self.controls,
                        &layer,
                        now,
                    );
                });
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for e in effects {
                    if let Node::Group {
                        label, children, ..
                    } = e
                    {
                        section(ui, &label);
                        for c in children {
                            if let Node::Prop { path, attr, label } = c {
                                prop_line(
                                    ui,
                                    cx,
                                    &session,
                                    &mut self.eval,
                                    &path,
                                    &attr,
                                    &label,
                                    now,
                                    2,
                                );
                            }
                        }
                    }
                }
            });
    }
}

// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct PreviewPanel;

/// Content size the Preview panel needs (transport row with Loop, timecode).
const PREVIEW_MIN_W: f32 = 210.0;
const PREVIEW_MIN_H: f32 = 56.0;

impl Panel for PreviewPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        // The transport row and the timecode line below it, never cut off.
        TabInfo::new(tr("Preview")).min_size(egui::vec2(PREVIEW_MIN_W, PREVIEW_MIN_H))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let info = common::active_comp(cx.stage, &session);
        let playing = session.lock().is_playing();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let buttons: [(Icon, &str, Invocation); 5] = [
                (
                    Icon::FirstFrame,
                    "First Frame",
                    Invocation::new(ids::GO_START),
                ),
                (
                    Icon::PrevFrame,
                    "Previous Frame",
                    Invocation::with_params(ids::STEP, json!({"frames": -1})),
                ),
                (
                    if playing { Icon::Pause } else { Icon::Play },
                    "Play/Stop",
                    Invocation::new(ids::PLAY),
                ),
                (
                    Icon::NextFrame,
                    "Next Frame",
                    Invocation::with_params(ids::STEP, json!({"frames": 1})),
                ),
                (Icon::LastFrame, "Last Frame", Invocation::new(ids::GO_END)),
            ];
            for (icon, tip, inv) in buttons {
                if icons::button(ui, icon, 24.0, false)
                    .labeled(egui::WidgetType::Button, tr(tip))
                    .on_hover_text(tr(tip))
                    .clicked()
                {
                    actions::handle(&inv, cx, &session);
                }
            }
            ui.add_space(8.0);
            let mut l = session.lock().loop_playback;
            if widgets::checkbox(ui, &mut l, tr("Loop")).changed() {
                session.lock().loop_playback = l;
            }
        });
        if let Some(info) = info {
            let now = session.lock().time(&info.path);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(common::timecode(&info, now)).color(color::ACCENT));
                ui.label(
                    egui::RichText::new(format!(
                        "{} {:.2}",
                        tr("Frame Rate (Auto)"),
                        info.frame_rate.as_f64()
                    ))
                    .font(theme::small())
                    .color(color::TEXT_DIM),
                );
            });
        }
        let _ = Rect::NOTHING;
    }
}
