//! The Effect Controls panel (F3) content: every effect of the selected layer
//! with AE's header (twirl, fx switch, name, Reset, About) and its parameters
//! drawn from the effect definition — sliders with AE's slider range and
//! precision, angles (revolutions + degrees), checkboxes, colours, popups,
//! points, layer pickers, parameter groups. Edits run the same commands as the
//! timeline (`attr.set_at`, `attr.set_animated`; one undo step per drag).

use std::collections::HashSet;

use bae_model::effect::{ParamKind, param_attr};
use bae_model::{EffectDef, EffectParamDef, PrimPath, Time, Value};
use bae_motion::query;
use bae_ui::theme::{self, color};
use bae_ui::widgets::{self, HotNumber};
use bae_ui::{PanelContext, tr};
use egui::{Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::common::{self, Edit, EvalCache};
use crate::session::SharedSession;

/// Panel-local view state (twirled effects and parameters).
#[derive(Default)]
pub(crate) struct ControlsState {
    /// Collapsed effects (AE shows new effects expanded).
    collapsed: HashSet<String>,
    pub(crate) show_presets: bool,
    /// Parameters whose slider / dial row is shown.
    open_params: HashSet<String>,
    /// Collapsed parameter groups.
    closed_groups: HashSet<String>,
}

const LABEL_WIDTH: f32 = 150.0;

/// Draws the effects of `layer` at comp time `now`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    eval: &mut EvalCache,
    state: &mut ControlsState,
    layer: &PrimPath,
    now: Time,
) -> bool {
    let effects = bae_motion::effect::effects_of(cx.stage, layer);
    if effects.is_empty() {
        return false;
    }
    let registries = super::registries(cx.resources);
    for (index, effect) in effects.iter().enumerate() {
        let match_name = cx
            .stage
            .prim(effect)
            .and_then(|v| query::static_value(&v, "match_name"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let def = registries.as_ref().and_then(|r| r.def(&match_name));
        header(
            ui,
            cx,
            state,
            effect,
            index,
            def.as_deref(),
            &match_name,
            now,
        );
        if state.collapsed.contains(effect.as_str()) {
            continue;
        }
        if state.show_presets {
            widgets::menu_button(ui, tr("Animation Presets"), |ui| {
                let indexed = cx
                    .resources
                    .get::<std::sync::Arc<crate::presets::PresetIndex>>()
                    .map(|index| {
                        index.refresh(false);
                        index.entries()
                    })
                    .unwrap_or_default();
                let mut available = false;
                for preset in indexed.iter() {
                    let [saved] = preset.effects.as_ref() else {
                        continue;
                    };
                    if saved.match_name != match_name {
                        continue;
                    }
                    available = true;
                    if ui.button(&preset.name).clicked() {
                        let time = query::to_key_time(cx.stage, effect, now);
                        let edits: Vec<_> = saved.values.iter().map(|(attr,value)| {
                            json!({"path":effect,"time":time,"attr":attr,"value":value})
                        }).collect();
                        if !edits.is_empty() {
                            cx.invoke_with("attr.set_many", json!({"edits":edits}));
                        }
                        ui.close();
                    }
                }
                if !available {
                    ui.add_enabled(false, egui::Button::new(tr("None")));
                }
            });
        }
        match def {
            Some(def) => params(ui, cx, session, eval, state, effect, &def, now),
            None => {
                // Missing effect (plugin not installed): values stay untouched.
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.colored_label(color::TEXT_DISABLED, tr("Missing effect"));
                });
            }
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn header(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    state: &mut ControlsState,
    effect: &PrimPath,
    index: usize,
    def: Option<&EffectDef>,
    match_name: &str,
    now: Time,
) {
    let stage = cx.stage;
    let enabled = stage
        .prim(effect)
        .and_then(|v| query::static_value(&v, "enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let name = query::display_name_of(stage, effect);
    let session = common::session(cx.resources);
    let selected = session.lock().selected_effects.contains(effect);
    ui.add_space(2.0);
    // Selected effects are highlighted like AE's (their points show in the viewer).
    let highlight = ui.painter().add(egui::Shape::Noop);
    let row = ui.horizontal(|ui| {
        ui.set_min_height(20.0);
        let mut open = !state.collapsed.contains(effect.as_str());
        if widgets::disclosure(ui, &mut open).clicked() {
            if open {
                state.collapsed.remove(effect.as_str());
            } else {
                state.collapsed.insert(effect.as_str().to_owned());
            }
        }
        let mut on = enabled;
        let fx = widgets::checkbox(ui, &mut on, "");
        let (icon, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        bae_ui::icons::paint(ui.painter(), icon, bae_ui::icons::Icon::Fx, color::ICON);
        if fx.clicked() {
            cx.invoke_with(
                "effect.set_enabled",
                json!({"effects": [effect], "enabled": !enabled}),
            );
        }
        let title = ui.add(
            egui::Label::new(
                egui::RichText::new(&name)
                    .strong()
                    .color(color::TEXT_STRONG),
            )
            .sense(Sense::click_and_drag()),
        );
        if title.clicked() {
            let toggle = ui.input(|i| i.modifiers.shift || i.modifiers.command);
            let mut s = session.lock();
            if !toggle {
                s.selected_effects = vec![effect.clone()];
            } else if let Some(i) = s.selected_effects.iter().position(|e| e == effect) {
                s.selected_effects.remove(i);
            } else {
                s.selected_effects.push(effect.clone());
            }
        }
        // Drag an effect header onto another one to reorder (AE).
        title.dnd_set_drag_payload(EffectMove(effect.clone()));
        if let Some(moved) = title.dnd_release_payload::<EffectMove>()
            && moved.0 != *effect
        {
            cx.invoke_with(
                "effect.reorder",
                json!({"effects": [moved.0], "index": index}),
            );
        }
        title.context_menu(|ui| {
            if ui.button(tr("Reset")).clicked() {
                reset(cx, effect, now);
                ui.close();
            }
            if ui.button(tr("Duplicate")).clicked() {
                cx.invoke_with("effect.duplicate", json!({"effects": [effect]}));
                ui.close();
            }
            if ui.button(tr("Remove Effect")).clicked() {
                cx.invoke_with("effect.remove", json!({"effects": [effect]}));
                ui.close();
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let about = ui.add(egui::Label::new(tr("About...")).sense(Sense::click()));
            egui::Popup::menu(&about).show(|ui| {
                let shown = super::effect_name(match_name, def);
                ui.label(egui::RichText::new(shown).strong());
                ui.label(match_name);
                let source = match def.and_then(|d| d.plugin.as_ref()) {
                    Some(p) => format!("{} {}", p.id, p.version),
                    None => tr("BAE built-in effect").to_owned(),
                };
                ui.label(source);
            });
            if def.is_some()
                && ui
                    .add(egui::Label::new(tr("Reset")).sense(Sense::click()))
                    .clicked()
            {
                reset(cx, effect, now);
            }
        });
    });
    let r = row.response.rect;
    if selected {
        let full = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), r.y_range());
        ui.painter().set(
            highlight,
            egui::Shape::rect_filled(full, 0.0, color::ROW_SELECTED),
        );
    }
    ui.painter().hline(
        r.x_range(),
        r.bottom() + 1.0,
        egui::Stroke::new(1.0, color::DIVIDER),
    );
}

fn reset(cx: &mut PanelContext<'_>, effect: &PrimPath, now: Time) {
    let key = query::to_key_time(cx.stage, effect, now);
    cx.invoke_with("effect.reset", json!({"effects": [effect], "time": key}));
}

/// Dragged effect header (reordering in Effect Controls).
#[derive(Clone, Debug)]
struct EffectMove(PrimPath);

#[allow(clippy::too_many_arguments)]
fn params(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    eval: &mut EvalCache,
    state: &mut ControlsState,
    effect: &PrimPath,
    def: &EffectDef,
    now: Time,
) {
    let mut depth = 1usize;
    // Depth at which a closed group hides its contents.
    let mut hidden_from: Option<usize> = None;
    for param in &def.params {
        match param.kind {
            ParamKind::GroupEnd => {
                depth = depth.saturating_sub(1).max(1);
                if hidden_from.is_some_and(|d| d > depth) {
                    hidden_from = None;
                }
                continue;
            }
            ParamKind::GroupStart => {
                if hidden_from.is_none() && !param.flags.hidden {
                    let key = format!("{effect}#{}", param.id);
                    let mut open = !state.closed_groups.contains(&key);
                    ui.horizontal(|ui| {
                        ui.add_space(indent(depth));
                        if widgets::disclosure(ui, &mut open).clicked() {
                            if open {
                                state.closed_groups.remove(&key);
                            } else {
                                state.closed_groups.insert(key.clone());
                            }
                        }
                        ui.label(super::param_label(&def.match_name, param));
                    });
                    if !open {
                        hidden_from = Some(depth + 1);
                    }
                }
                depth += 1;
                continue;
            }
            _ => {}
        }
        if param.flags.hidden || hidden_from.is_some_and(|d| depth >= d) {
            continue;
        }
        if def.match_name == "ADBE Param EQ" && param.kind == ParamKind::Display {
            ui.label(super::param_label(&def.match_name, param));
            let rate = session
                .lock()
                .output
                .as_ref()
                .map_or(48_000, |output| output.sample_rate());
            super::eq_response::show(ui, rate, |id| {
                common::current_value(
                    eval.get(cx.resources),
                    cx.stage,
                    effect,
                    &param_attr(id),
                    now,
                )
                .or_else(|| def.param(id)?.default.clone())
                .and_then(|v| {
                    v.as_f64()
                        .or_else(|| v.as_bool().map(|b| f64::from(u8::from(b))))
                })
                .unwrap_or_default()
            });
            continue;
        }
        param_row(ui, cx, session, eval, state, effect, def, param, depth, now);
    }
}

fn indent(depth: usize) -> f32 {
    8.0 + 14.0 * depth as f32
}

#[allow(clippy::too_many_arguments)]
fn param_row(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    eval: &mut EvalCache,
    state: &mut ControlsState,
    effect: &PrimPath,
    def: &EffectDef,
    param: &EffectParamDef,
    depth: usize,
    now: Time,
) {
    let stage = cx.stage;
    let attr = param_attr(param.id);
    let label = super::param_label(&def.match_name, param);
    let spec = query::attr_spec(stage, effect, &attr)
        .cloned()
        .unwrap_or_default();
    let value = session
        .lock()
        .scrub
        .as_ref()
        .and_then(|s| s.value_for(effect, &attr))
        .cloned()
        .or_else(|| common::current_value(eval.get(cx.resources), stage, effect, &attr, now))
        .or_else(|| {
            let layer = super::layer_of(effect)?;
            bae_motion::effect::instance_default(stage, &layer, param)
        });
    let key = format!("{effect}#{}", param.id);
    let twirl = matches!(param.kind, ParamKind::Slider { .. } | ParamKind::Angle);
    let mut edit = Edit::None;
    let expression = spec.expression.as_ref().is_some_and(|e| e.enabled);
    let row = ui.horizontal(|ui| {
        ui.set_min_height(20.0);
        ui.add_space(indent(depth) - 16.0);
        if twirl {
            let mut open = state.open_params.contains(&key);
            if widgets::disclosure(ui, &mut open).clicked() {
                if open {
                    state.open_params.insert(key.clone());
                } else {
                    state.open_params.remove(&key);
                }
            }
        } else {
            ui.add_space(16.0);
        }
        // AE's layer and mask popups have no stopwatch.
        let animatable = param.kind.value_type().is_some()
            && !param.flags.cannot_animate
            && !matches!(param.kind, ParamKind::Layer | ParamKind::Path);
        if animatable {
            let mut on = spec.is_animated();
            if widgets::stopwatch_for(ui, &mut on, &label).clicked() {
                let t = query::to_key_time(stage, effect, now);
                cx.invoke_with(
                    "attr.set_animated",
                    json!({"path": effect, "attr": attr, "animated": !spec.is_animated(), "time": t}),
                );
            }
        } else {
            ui.add_space(18.0);
        }
        let (r, _) = ui.allocate_exact_size(vec2(LABEL_WIDTH - indent(depth), 18.0), Sense::hover());
        widgets::clipped_text(
            ui.painter(),
            pos2(r.left(), r.center().y),
            &label,
            theme::body(),
            color::TEXT,
            r.width(),
        );
        if def.match_name.starts_with("ADBE OCIO ") {
            if param.id==1 && matches!(param.kind,ParamKind::Button){
                let is_cdl=def.match_name=="ADBE OCIO CDL Transform";
                let data_id=if is_cdl{6}else{5};
                let stored=common::current_value(eval.get(cx.resources),stage,effect,&param_attr(data_id),now);
                let filename=stored.as_ref().and_then(|v|match v{Value::Bytes(data)=>data.split(|b|*b==0).next().and_then(|s|std::str::from_utf8(s).ok()),_=>None});
                if ui.button(filename.unwrap_or(tr("Open..."))).clicked(){
                    cx.invoke_with("motion.effect.open_color_file",json!({"path":effect,"time":query::to_key_time(stage,effect,now),"cdl":is_cdl}));
                }
                return;
            }
            if param.name=="Reset Controls" {
                if ui.button(tr("Reset")).clicked(){reset(cx,effect,now);}
                return;
            }
            if param.name=="Information"{
                ui.label(if super::chinese(){"OCIO 配置"}else{"OCIO config"}).on_hover_text(std::env::var("OCIO").unwrap_or_else(|_|"BAE".to_owned()));
                return;
            }
        }
        let disabled = param.flags.disabled;
        if disabled && matches!(param.kind, ParamKind::Layer | ParamKind::Path) {
            // Layer / mask inputs the renderer does not feed yet.
            ui.colored_label(color::TEXT_DISABLED, tr("Not available in BAE yet"));
        } else {
            ui.add_enabled_ui(!disabled, |ui| {
                if def.match_name=="ADBE MESH WARP" && param.id==4 {
                    let mut number=|id:u32|common::current_value(eval.get(cx.resources),stage,effect,&param_attr(id),now)
                        .or_else(||def.param(id).and_then(|p|p.default.clone())).and_then(|v|v.as_f64()).unwrap_or(0.);
                    edit=super::warp_grid::edit(ui,value.as_ref(),[number(2) as usize,number(1) as usize]);
                    return;
                }
                edit = value_control(
                    ui,
                    cx,
                    effect,
                    def,
                    param,
                    value.as_ref(),
                    &label,
                    expression,
                );
            });
        }
    });
    // An expression pick whip dragged from the Timeline.
    if let Some(whip) = row.response.dnd_release_payload::<crate::expr::Whip>() {
        crate::expr::link(cx, eval.get(cx.resources), &whip, effect, Some(&attr), now);
    }
    if twirl && state.open_params.contains(&key) {
        ui.horizontal(|ui| {
            ui.add_space(LABEL_WIDTH + 36.0);
            if let Some(e) = expanded_control(ui, param, value.as_ref()) {
                edit = e;
            }
        });
    }
    common::apply_edit(cx, session, effect, &attr, now, edit);
}

/// The value column of one parameter (red when an expression drives it).
#[allow(clippy::too_many_arguments)]
fn value_control(
    ui: &mut Ui,
    cx: &PanelContext<'_>,
    effect: &PrimPath,
    def: &EffectDef,
    param: &EffectParamDef,
    value: Option<&Value>,
    label: &str,
    expression: bool,
) -> Edit {
    let id = ui.id().with(("fx-param", effect.as_str(), param.id));
    match &param.kind {
        ParamKind::Slider {
            valid_min,
            valid_max,
            precision,
            integer,
            display,
            ..
        } => {
            let model = value.and_then(Value::as_f64).unwrap_or(0.0);
            let suffix = match display.as_str() {
                "percent" => "%",
                "degrees" => "°",
                _ => "",
            };
            let decimals = if *integer { 0 } else { usize::from(*precision) };
            let to_value = |v: f64| {
                if *integer {
                    Value::Int(v.round() as i64)
                } else {
                    Value::Float(v)
                }
            };
            match hot_number(
                ui,
                id,
                model,
                decimals,
                *valid_min..=*valid_max,
                suffix,
                label,
                expression,
            ) {
                Num::None => Edit::None,
                Num::Live(v) => Edit::Live(to_value(v)),
                Num::Commit(v) => Edit::Commit(to_value(v)),
            }
        }
        // AE angles: revolutions + degrees (0x+30.0°), like Rotation.
        ParamKind::Angle => {
            let v = value.cloned().unwrap_or(Value::Float(0.0));
            common::value_editor(ui, id, "rotation", &v, 1, label, expression)
        }
        ParamKind::Checkbox => {
            let on = value.and_then(Value::as_bool).unwrap_or(false);
            let mut now_on = on;
            if widgets::checkbox(ui, &mut now_on, label).changed() {
                Edit::Commit(Value::Bool(now_on))
            } else {
                Edit::None
            }
        }
        ParamKind::Popup { .. } => {
            let options = super::option_labels(&def.match_name, param);
            let raw = popup_index(value);
            let current = if def.match_name.starts_with("ADBE OCIO ") && raw > options.len() as i64
            {
                1
            } else {
                raw.clamp(1, options.len().max(1) as i64)
            };
            let shown = options
                .get(current as usize - 1)
                .cloned()
                .unwrap_or_default();
            let mut chosen = None;
            egui::ComboBox::from_id_salt(id)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (i, o) in options.iter().enumerate() {
                        let n = i as i64 + 1;
                        // AE's "(-" entries are separators (their index still counts).
                        if o.starts_with("(-") {
                            ui.separator();
                            continue;
                        }
                        if ui.selectable_label(n == current, o).clicked() {
                            chosen = Some(n);
                        }
                    }
                });
            match chosen {
                Some(n) if n != current => Edit::Commit(Value::Int(n)),
                _ => Edit::None,
            }
        }
        ParamKind::Color | ParamKind::Point | ParamKind::Point3d => {
            let fallback = match param.kind {
                ParamKind::Color => Value::Color([1.0, 1.0, 1.0, 1.0]),
                ParamKind::Point => Value::Vec2([0.0, 0.0]),
                _ => Value::Vec3([0.0, 0.0, 0.0]),
            };
            let v = value.cloned().unwrap_or(fallback);
            let dims = if matches!(param.kind, ParamKind::Point3d) {
                3
            } else {
                2
            };
            common::value_editor(ui, id, "param", &v, dims, label, expression)
        }
        ParamKind::Layer => layer_picker(ui, cx, effect, id, value),
        ParamKind::Path => mask_picker(ui, cx, effect, id, value),
        ParamKind::Button => {
            let caption = if label.is_empty() {
                tr(&param.name)
            } else {
                label
            };
            ui.add_enabled(false, egui::Button::new(caption));
            Edit::None
        }
        ParamKind::ArbitraryData | ParamKind::CurvePoints => {
            ui.colored_label(color::TEXT_DISABLED, "—");
            Edit::None
        }
        ParamKind::GroupStart | ParamKind::GroupEnd | ParamKind::Display => Edit::None,
    }
}

/// The twirled-open row: a slider over AE's slider range, or an angle dial.
fn expanded_control(ui: &mut Ui, param: &EffectParamDef, value: Option<&Value>) -> Option<Edit> {
    match &param.kind {
        ParamKind::Slider {
            slider_min,
            slider_max,
            integer,
            ..
        } => {
            let mut v = value.and_then(Value::as_f64).unwrap_or(0.0);
            let resp = ui.add(
                egui::Slider::new(&mut v, *slider_min..=*slider_max)
                    .show_value(false)
                    .clamping(egui::SliderClamping::Never),
            );
            let to_value = |v: f64| {
                if *integer {
                    Value::Int(v.round() as i64)
                } else {
                    Value::Float(v)
                }
            };
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                Some(Edit::Commit(to_value(v)))
            } else if resp.dragged() {
                Some(Edit::Live(to_value(v)))
            } else {
                None
            }
        }
        ParamKind::Angle => {
            let v = value.and_then(Value::as_f64).unwrap_or(0.0);
            angle_dial(ui, v)
        }
        _ => None,
    }
}

/// AE's angle dial: 0° up, clockwise; dragging keeps the revolutions.
fn angle_dial(ui: &mut Ui, v: f64) -> Option<Edit> {
    let size = 44.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let c = rect.center();
    let radius = size / 2.0 - 3.0;
    let p = ui.painter();
    p.circle_stroke(c, radius, egui::Stroke::new(1.5, color::TEXT));
    let a = (v.to_radians() - std::f64::consts::FRAC_PI_2) as f32;
    p.line_segment(
        [c, c + vec2(a.cos(), a.sin()) * radius],
        egui::Stroke::new(2.0, color::ACCENT),
    );
    let pointer = resp.interact_pointer_pos()?;
    let d = pointer - c;
    let deg = (f64::from(d.y).atan2(f64::from(d.x)).to_degrees() + 90.0).rem_euclid(360.0);
    let revs = (v / 360.0).floor();
    let value = Value::Float(revs * 360.0 + deg);
    if resp.drag_stopped() || resp.clicked() {
        Some(Edit::Commit(value))
    } else if resp.dragged() {
        Some(Edit::Live(value))
    } else {
        None
    }
}

fn popup_index(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(i)) => *i,
        Some(Value::Float(f)) => f.round() as i64,
        _ => 1,
    }
}

/// A Layer parameter: "None" or a layer of the same comp (AE).
fn layer_picker(
    ui: &mut Ui,
    cx: &PanelContext<'_>,
    effect: &PrimPath,
    id: egui::Id,
    value: Option<&Value>,
) -> Edit {
    let layers = super::layer_of(effect)
        .and_then(|l| query::owning_comp(cx.stage, &l))
        .map(|comp| query::comp_layers(cx.stage, &comp))
        .unwrap_or_default();
    let name = |p: &PrimPath| query::display_name_of(cx.stage, p);
    let items: Vec<_> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.clone(), format!("{}. {}", i + 1, name(l))))
        .collect();
    prim_picker(ui, cx, id, value, &items)
}

/// A Path parameter: "None" or a mask of the effect's layer (AE).
fn mask_picker(
    ui: &mut Ui,
    cx: &PanelContext<'_>,
    effect: &PrimPath,
    id: egui::Id,
    value: Option<&Value>,
) -> Edit {
    let items: Vec<_> = super::layer_masks(cx.stage, effect)
        .into_iter()
        .map(|m| {
            let name = query::display_name_of(cx.stage, &m);
            (m, name)
        })
        .collect();
    prim_picker(ui, cx, id, value, &items)
}

/// "None" or one of `items` (prim, menu text). A reference to a prim that no
/// longer exists shows as "None"; choosing "None" writes an empty string.
fn prim_picker(
    ui: &mut Ui,
    cx: &PanelContext<'_>,
    id: egui::Id,
    value: Option<&Value>,
    items: &[(PrimPath, String)],
) -> Edit {
    let current = match value {
        Some(Value::PrimRef(p)) if cx.stage.prim(p).is_some() => Some(p),
        _ => None,
    };
    let shown = current.map_or_else(
        || tr("None").to_owned(),
        |p| query::display_name_of(cx.stage, p),
    );
    let mut out = Edit::None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), tr("None")).clicked() {
                out = Edit::Commit(Value::String(String::new()));
            }
            for (p, text) in items {
                if ui.selectable_label(current == Some(p), text).clicked() {
                    out = Edit::Commit(Value::PrimRef(p.clone()));
                }
            }
        });
    out
}

enum Num {
    None,
    Live(f64),
    Commit(f64),
}

/// A hot number clamped to the valid range; one command per drag.
#[allow(clippy::too_many_arguments)]
fn hot_number(
    ui: &mut Ui,
    id: egui::Id,
    model: f64,
    decimals: usize,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    name: &str,
    expression: bool,
) -> Num {
    let key = id.with("scrub");
    let mut v = ui.data(|d| d.get_temp::<f64>(key)).unwrap_or(model);
    let speed = ((range.end() - range.start()) / 1000.0).clamp(0.01, 1.0);
    let resp = ui
        .push_id(id, |ui| {
            ui.add(
                HotNumber::new(&mut v)
                    .tint(if expression {
                        common::EXPRESSION_VALUE
                    } else {
                        color::ACCENT
                    })
                    .decimals(decimals)
                    .speed(speed)
                    .range(range)
                    .suffix(suffix)
                    .label(name),
            )
        })
        .inner;
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<f64>(key));
        return Num::Commit(v);
    }
    if resp.dragged() {
        ui.data_mut(|d| d.insert_temp(key, v));
        return Num::Live(v);
    }
    if resp.changed() {
        return Num::Commit(v);
    }
    Num::None
}
