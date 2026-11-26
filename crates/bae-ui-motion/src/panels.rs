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

mod audio;

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
            .and_then(|s| s.value_for(path, attr))
            .cloned()
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
}

impl Panel for PropertiesPanel {
    /// "Properties: <layer>" for the layer selected now, even while the panel
    /// is not drawn (a collapsed or background tab, #465).
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        match selected_layer(cx.stage, &common::session(cx.resources)) {
            Some(layer) => TabInfo::new(format!(
                "{}: {}",
                tr("Properties"),
                query::display_name_of(cx.stage, &layer)
            )),
            None => TabInfo::new(tr("Properties")),
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
            common::empty_hint(
                ui,
                tr("No layer selected"),
                tr("Select a layer to see its properties."),
            );
            return;
        };
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
                    // Paragraph alignment.
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Paragraph")).color(color::TEXT));
                        for (icon, value, name) in [
                            (Icon::TextAlignLeft, "left", "Left Align Text"),
                            (Icon::TextAlignCenter, "center", "Center Text"),
                            (Icon::TextAlignRight, "right", "Right Align Text"),
                        ] {
                            let on = d2.justification == value;
                            let r = icons::button(ui, icon, 20.0, on)
                                .labeled_toggle(egui::WidgetType::Button, tr(name), on)
                                .on_hover_text(tr(name));
                            if r.clicked() && !on {
                                d2.justification = value.to_owned();
                                commit = true;
                            }
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
            // Effects: each effect's controls, in stacking order (AE).
            let effects = props::effect_nodes(stage, &layer);
            if !effects.is_empty() {
                section(ui, tr("Effects"));
                for effect in &effects {
                    let Node::Group { label, children, .. } = effect else {
                        continue;
                    };
                    ui.label(egui::RichText::new(label).color(color::TEXT_STRONG));
                    let mut flat = Vec::new();
                    collect_props(children, &mut flat);
                    for (path, attr, label) in flat {
                        prop_line(ui, cx, &session, &mut self.eval, &path, &attr, &label, now, dims);
                    }
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

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        ui.checkbox(
            &mut self.controls.show_presets,
            tr("Show Animation Presets"),
        );
        ui.separator();
        if ui.button(tr("Manage Effects...")).clicked() {
            crate::effects::manager::open(cx);
            ui.close();
        }
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({"show_presets":self.controls.show_presets}))
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
/// Height the Preview options need under the transport row.
const PREVIEW_OPTIONS_H: f32 = 240.0;

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
        // The shell ticks the clock; this keeps the audio mix current.
        if let Some(info) = &info {
            session.lock().update_preview(cx, info);
        }
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
        // The options below the transport row show when the panel has room for
        // them (AE's Default workspace shows the transport row only).
        if ui.available_height() >= PREVIEW_OPTIONS_H {
            preview_options(ui, &session, info.as_ref());
        }
        let _ = Rect::NOTHING;
    }
}

/// AE's Preview panel options under the transport row: Shortcut, Include,
/// Cache Before Playback, Range, Play From, Frame Rate, Skip, Resolution,
/// Full Screen and what Spacebar does on stop. Range, Play From, Frame Rate,
/// Skip, Resolution and Move Time To Preview Time drive playback; the others
/// show AE's values, greyed.
fn preview_options(ui: &mut Ui, session: &SharedSession, info: Option<&query::CompInfo>) {
    use crate::session::PreviewRange;
    const RATES: [f64; 12] = [
        8.0, 10.0, 12.0, 15.0, 23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0,
    ];
    let mut o = session.lock().preview.clone();
    let before = o.clone();
    let lw = 82.0;
    ui.add_space(4.0);
    ui.add_enabled_ui(false, |ui| {
        bae_ui::dialog::row(ui, lw, tr("Shortcut"), |ui| {
            let mut i = 0;
            widgets::dropdown(ui, "preview-shortcut", &mut i, &[tr("Spacebar")], 120.0);
        });
        bae_ui::dialog::row(ui, lw, tr("Include:"), |ui| {
            for (icon, tip) in [
                (Icon::Eye, "Include Video"),
                (Icon::Speaker, "Include Audio"),
            ] {
                icons::button(ui, icon, 18.0, true).on_hover_text(tr(tip));
            }
        });
        let mut cache = false;
        widgets::checkbox(ui, &mut cache, tr("Cache Before Playback"));
    });
    bae_ui::dialog::row(ui, lw, tr("Range"), |ui| {
        let names = [
            tr("Work Area"),
            tr("Work Area Extended By Current Time"),
            tr("Entire Duration"),
        ];
        let ranges = [
            PreviewRange::WorkArea,
            PreviewRange::WorkAreaExtended,
            PreviewRange::EntireDuration,
        ];
        let mut i = ranges.iter().position(|r| *r == o.range).unwrap_or(1);
        if widgets::dropdown(ui, "preview-range", &mut i, &names, 180.0).changed() {
            o.range = ranges[i];
        }
    });
    bae_ui::dialog::row(ui, lw, tr("Play From"), |ui| {
        let names = [tr("Start of Range"), tr("Current Time")];
        let mut i = usize::from(o.from_current);
        if widgets::dropdown(ui, "preview-from", &mut i, &names, 180.0).changed() {
            o.from_current = i == 1;
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(tr("Frame Rate"));
            let auto = info.map_or(30.0, |i| i.frame_rate.as_f64());
            let mut names = vec![format!("({auto:.2})")];
            names.extend(RATES.iter().map(|r| format!("{r}")));
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let mut i = o
                .fps
                .and_then(|f| RATES.iter().position(|r| (r - f).abs() < 1e-3))
                .map_or(0, |p| p + 1);
            if widgets::dropdown(ui, "preview-fps", &mut i, &refs, 70.0).changed() {
                o.fps = (i > 0).then(|| RATES[i - 1]);
            }
        });
        ui.vertical(|ui| {
            ui.label(tr("Skip"));
            let mut skip = f64::from(o.skip);
            if ui
                .add(
                    widgets::HotNumber::new(&mut skip)
                        .label(tr("Skip"))
                        .range(0.0..=99.0)
                        .decimals(0),
                )
                .changed()
            {
                o.skip = skip as u32;
            }
        });
        ui.vertical(|ui| {
            ui.label(tr("Resolution"));
            let names = [
                tr("Auto"),
                tr("Full"),
                tr("Half"),
                tr("Third"),
                tr("Quarter"),
            ];
            let mut i = o.resolution.map_or(0, |r| r as usize);
            if widgets::dropdown(ui, "preview-res", &mut i, &names, 90.0).changed() {
                o.resolution = (i > 0).then_some(i as u32);
            }
        });
    });
    ui.add_enabled_ui(false, |ui| {
        let mut full = false;
        widgets::checkbox(ui, &mut full, tr("Full Screen"));
    });
    ui.add_space(2.0);
    ui.label(tr("On (Spacebar) Stop:"));
    ui.indent("preview-stop", |ui| {
        ui.add_enabled_ui(false, |ui| {
            let mut cached = true;
            widgets::checkbox(ui, &mut cached, tr("If caching, play cached frames"));
        });
        widgets::checkbox(ui, &mut o.move_time, tr("Move time to preview time"));
    });
    if o != before {
        session.lock().preview = o;
    }
}

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    // Preview panel (AE 26's Time_Palette strings).
    ("Spacebar", "空格键"),
    ("Include:", "包含:"),
    ("Include Video", "在预览中播放视频"),
    ("Include Audio", "在预览中播放音频"),
    ("Cache Before Playback", "在回放前缓存"),
    (
        "Work Area Extended By Current Time",
        "工作区域按当前时间延伸",
    ),
    ("Entire Duration", "整个持续时间"),
    ("Play From", "播放自"),
    ("Start of Range", "范围开头"),
    ("Skip", "跳过"),
    ("Full Screen", "全屏"),
    ("On (Spacebar) Stop:", "点击 (空格键) 停止："),
    ("If caching, play cached frames", "如果缓存，则播放缓存的帧"),
    ("Move time to preview time", "将时间移到预览时间"),
    // Properties
    ("Transform", "变换"),
    ("Layer Transform", "图层变换"),
    ("Layer Contents", "图层内容"),
    ("Contents", "内容"),
    ("Masks", "蒙版"),
    ("Effects", "效果"),
    ("Anchor Point", "锚点"),
    ("Position", "位置"),
    ("Scale", "缩放"),
    ("Rotation", "旋转"),
    ("X Rotation", "X 轴旋转"),
    ("Y Rotation", "Y 轴旋转"),
    ("Z Rotation", "Z 轴旋转"),
    ("Orientation", "方向"),
    ("Opacity", "不透明度"),
    ("Point of Interest", "目标点"),
    ("Source Text", "源文本"),
    ("Path Options", "路径选项"),
    ("Reverse Path", "反转路径"),
    ("Perpendicular To Path", "垂直于路径"),
    ("Force Alignment", "强制对齐"),
    ("First Margin", "首字边距"),
    ("Last Margin", "末字边距"),
    ("Roundness", "圆度"),
    ("Points", "点"),
    ("Inner Radius", "内径"),
    ("Outer Radius", "外径"),
    ("Inner Roundness", "内圆度"),
    ("Outer Roundness", "外圆度"),
    ("Fill", "填充"),
    ("Stroke", "描边"),
    ("Color", "颜色"),
    ("Stroke Width", "描边宽度"),
    ("Skew", "倾斜"),
    ("Skew Axis", "倾斜轴"),
    ("Path", "路径"),
    ("Mask Path", "蒙版路径"),
    ("Mask Feather", "蒙版羽化"),
    ("Mask Opacity", "蒙版不透明度"),
    ("Mask Expansion", "蒙版扩展"),
    ("Reset", "重置"),
    ("Param", "参数"),
    ("Expression", "表达式"),
    ("Font Size", "字体大小"),
    ("Fill Color", "填充颜色"),
    ("Left Align Text", "左对齐文本"),
    ("Center Text", "居中对齐文本"),
    ("Right Align Text", "右对齐文本"),
    ("Zoom", "缩放"),
    ("Intensity", "强度"),
    ("No layer selected", "未选择图层"),
    ("No effects applied", "未应用效果"),
    (
        "Select a layer to see its properties.",
        "选择一个图层以查看其属性。",
    ),
    ("Master Volume", "主音量"),
    ("Left output level", "左输出电平"),
    ("Right output level", "右输出电平"),
    // Preview panel
    ("First Frame", "第一帧"),
    ("Previous Frame", "上一帧"),
    ("Play/Stop", "播放/停止"),
    ("Next Frame", "下一帧"),
    ("Last Frame", "最后一帧"),
    ("Loop", "循环"),
    ("Frame Rate (Auto)", "帧速率（自动）"),
    ("Playing", "正在播放"),
    ("Go to Start", "转到开头"),
    ("Go to End", "转到结尾"),
    ("Go to Layer In Point", "转到图层入点"),
    ("Go to Layer Out Point", "转到图层出点"),
];

/// Registers this module's Chinese strings and panels (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "audio",
        bae_ui::PanelKind::new("Audio", |_| Box::new(audio::AudioPanel::default())),
    );
    crate::register_panel(
        app,
        "properties",
        bae_ui::PanelKind::new("Properties", |_| Box::new(PropertiesPanel::default())),
    );
    crate::register_panel(
        app,
        "effect_controls",
        bae_ui::PanelKind::new("Effect Controls", |spawn| {
            let mut panel = EffectControlsPanel::default();
            panel.controls.show_presets = spawn
                .state
                .and_then(|v| v["show_presets"].as_bool())
                .unwrap_or(false);
            Box::new(panel)
        }),
    );
    crate::register_panel(
        app,
        "preview",
        bae_ui::PanelKind::new("Preview", |_| Box::new(PreviewPanel)),
    );
}
