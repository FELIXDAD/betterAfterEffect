//! egui presentation of live ExtendScript windows and docked panels.
use bae_app::{AppBuilder, Resources};
use bae_script::{RUN_SCRIPT, ScriptWindow, ScriptWindows};
use bae_ui::{Panel, PanelContext, PanelKind};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

pub(crate) fn install(app: &mut AppBuilder, scripts_dir: Option<&Path>) {
    let windows = ScriptWindows::default();
    app.insert_resource(windows.clone());
    let dialogs: Arc<dyn bae_ui::DialogHost> = Arc::new(Host(windows.clone()));
    let _ = app.register_arc("bae.scriptui", dialogs);
    if let Some(dir) = scripts_dir {
        for path in super::scripts::installed_scripts(&dir.join("ScriptUI Panels")) {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let key = format!(
                "scriptui.{}",
                name.bytes().map(|v| format!("{v:02x}")).collect::<String>()
            );
            let panel = key.clone();
            let windows = windows.clone();
            let item =
                bae_ui::MenuItemSpec::new(&["Window"], &name, bae_ui::action::ids::OPEN_PANEL)
                    .params(json!({"id": key}))
                    .workspace(crate::menus::WORKSPACE);
            let _ = app.register(format!("menu.window.{key}"), item);
            let _ = app.register(
                key,
                PanelKind::new(name, move |_| {
                    Box::new(Dock {
                        windows: windows.clone(),
                        panel: panel.clone(),
                        path: path.clone(),
                        started: false,
                    })
                }),
            );
        }
    }
}

struct Host(ScriptWindows);

impl bae_ui::DialogHost for Host {
    fn is_open(&self, _: &Resources) -> bool {
        self.0.windows().iter().any(|w| w.tree["type"] == "dialog")
    }
    fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>) {
        for window in self.0.windows() {
            let root = window.tree.as_ref();
            let title = root["text"].as_str().unwrap_or_default();
            let id = egui::Id::new(("scriptui", window.id));
            match root["type"].as_str() {
                Some("dialog") => {
                    let result =
                        bae_ui::dialog::window(ctx, id, title, |ui| draw(ui, cx, &window, root));
                    if result.is_none() && bae_ui::dialog::closed_by_title_bar(ctx, id) {
                        send(cx, &window, json!({"close": true}));
                    }
                }
                Some("palette" | "window") => {
                    let mut open = true;
                    egui::Window::new(title)
                        .id(id)
                        .open(&mut open)
                        .show(ctx, |ui| draw(ui, cx, &window, root));
                    if !open {
                        send(cx, &window, json!({"close": true}));
                    }
                }
                _ => {}
            }
        }
    }
}

struct Dock {
    windows: ScriptWindows,
    panel: String,
    path: std::path::PathBuf,
    started: bool,
}

impl Panel for Dock {
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        if !self.started {
            self.started = true;
            match std::fs::read_to_string(&self.path) {
                Ok(source) => {
                    cx.invoke_with(RUN_SCRIPT, json!({"source":source, "interactive":true,
                        "name":self.path.file_name().unwrap_or_default().to_string_lossy(), "panel":self.panel}));
                    cx.invoke(super::scripts::REPORT);
                }
                Err(e) => super::dialogs::alert(cx, cx.kind_title, e.to_string()),
            }
        }
        for w in self.windows.windows() {
            if w.panel.as_ref() == Some(&self.panel) && w.tree["type"] == "panel" {
                draw(ui, cx, &w, &w.tree);
            }
        }
    }
    fn on_close(&mut self, _: &mut PanelContext<'_>) {
        for w in self.windows.windows() {
            if w.panel.as_ref() == Some(&self.panel) {
                self.windows.dismiss(w.id);
            }
        }
    }
}

fn send(cx: &mut PanelContext<'_>, w: &ScriptWindow, event: Value) {
    cx.invoke_with(
        RUN_SCRIPT,
        json!({"window":w.id,"event": event,"name": w.tree["text"]}),
    );
    cx.invoke(super::scripts::REPORT);
}

fn draw(ui: &mut egui::Ui, cx: &mut PanelContext<'_>, window: &ScriptWindow, node: &Value) {
    let id = node["id"].as_u64().unwrap_or_default();
    ui.push_id((window.id, id), |ui| {
        ui.add_enabled_ui(node["enabled"] == true, |ui| {
            let text = node["text"].as_str().unwrap_or_default();
            let event = |cx: &mut PanelContext<'_>, kind: &str, value: Value| {
                send(cx, window, json!({"id":id, "kind":kind, "value":value}));
            };
            match node["type"].as_str().unwrap_or_default() {
                "dialog" | "palette" | "window" | "panel" | "group" => {
                    let m =
                        |i: usize| node["margins"][i].as_f64().unwrap_or(0.).clamp(0., 127.) as i8;
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: m(0),
                            top: m(1),
                            right: m(2),
                            bottom: m(3),
                        })
                        .show(ui, |ui| {
                            if node["type"] == "panel" && !text.is_empty() {
                                ui.label(text);
                            }
                            let row = node["orientation"] == "row";
                            let align = &node["alignChildren"];
                            let align = align
                                .as_str()
                                .or_else(|| align[usize::from(row)].as_str())
                                .unwrap_or("left");
                            let cross = match align {
                                "center" => egui::Align::Center,
                                "right" | "bottom" => egui::Align::Max,
                                _ => egui::Align::Min,
                            };
                            let layout = if row {
                                egui::Layout::left_to_right(cross)
                            } else {
                                egui::Layout::top_down(cross)
                            };
                            let spacing = node["spacing"].as_f64().unwrap_or(6.).max(0.) as f32;
                            ui.spacing_mut().item_spacing = egui::Vec2::splat(spacing);
                            let width = if !row && align == "fill" {
                                ui.available_width().min(480.)
                            } else {
                                0.
                            };
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 0.),
                                layout.with_cross_justify(align == "fill"),
                                |ui| {
                                    if let Some(children) = node["children"].as_array() {
                                        for child in children {
                                            draw(ui, cx, window, child);
                                        }
                                    }
                                },
                            );
                        });
                }
                "button" => {
                    if ui.button(text).clicked() {
                        event(cx, "onClick", Value::Null);
                    }
                }
                "statictext" => {
                    ui.label(text);
                }
                "edittext" => {
                    let mut value = text.to_owned();
                    if ui.text_edit_singleline(&mut value).changed() {
                        event(cx, "onChange", json!(value));
                    }
                }
                "checkbox" => {
                    let mut value = node["value"] == true;
                    if ui.checkbox(&mut value, text).changed() {
                        event(cx, "onClick", json!(value));
                    }
                }
                "radiobutton" => {
                    if ui.radio(node["value"] == true, text).clicked() {
                        event(cx, "onClick", json!(true));
                    }
                }
                "dropdownlist" | "listbox" => {
                    let Some(items) = node["items"].as_array() else {
                        return;
                    };
                    let selected = node["selection"].as_u64().map(|i| i as usize);
                    let mut choices = |ui: &mut egui::Ui| {
                        for (i, item) in items.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    selected == Some(i),
                                    item.as_str().unwrap_or_default(),
                                )
                                .clicked()
                            {
                                event(cx, "onChange", json!(i));
                            }
                        }
                    };
                    if node["type"] == "listbox" {
                        choices(ui);
                    } else {
                        let value = selected
                            .and_then(|i| items.get(i))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("choices")
                            .selected_text(value)
                            .show_ui(ui, choices);
                    }
                }
                "slider" | "progressbar" => {
                    let min = node["min"].as_f64().unwrap_or(0.);
                    let max = node["max"].as_f64().unwrap_or(100.).max(min);
                    let mut value = node["value"].as_f64().unwrap_or(min);
                    if node["type"] == "slider" {
                        let response = ui.add(egui::Slider::new(&mut value, min..=max));
                        if response.drag_stopped() || (response.changed() && !response.dragged()) {
                            event(cx, "onChange", json!(value));
                        } else if response.changed() {
                            event(cx, "onChanging", json!(value));
                        }
                    } else {
                        ui.add(egui::ProgressBar::new(
                            ((value - min) / (max - min).max(f64::EPSILON)).clamp(0., 1.) as f32,
                        ));
                    }
                }
                _ => {}
            }
        });
    });
}
