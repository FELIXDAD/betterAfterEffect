use bae_model::{
    PrimPath, Stage, Value,
    effect::{param_attr, param_id},
    layer_styles::{self as styles, Control},
};
use bae_ui::{
    ActionHandler, Invocation, PanelContext, i18n,
    menu::{MenuItemSpec, MenuPos},
    tr,
};
use egui::{Id, Ui};
use serde_json::{Value as Json, json};

use crate::{
    common::{self, Edit},
    menus::WORKSPACE,
    props::Node,
};

const ACTION: &str = "motion.layer.styles";

#[derive(Clone, Copy)]
struct Item {
    title: &'static str,
    operation: &'static str,
    style: &'static str,
}

impl Item {
    fn params(self) -> Json {
        if self.operation == "add" {
            json!({"operation":"add","style":self.style})
        } else {
            json!({"operation":self.operation})
        }
    }
    fn available(self, stage: &Stage, layers: &[PrimPath]) -> bool {
        !layers.is_empty()
            && layers.iter().all(|l| {
                let kind = bae_motion::query::layer_kind(stage, l);
                kind.is_some()
                    && !matches!(kind.as_deref(), Some("camera" | "light" | "null"))
                    && bae_motion::query::locked_layer_of(stage, l).is_none()
            })
            && match self.operation {
                "remove_all" => layers
                    .iter()
                    .any(|l| styles::effective_styles(stage, l).is_some()),
                "convert" => layers.iter().any(|l| {
                    styles::source_styles(stage, l).is_some()
                        && l.child(styles::PRIM).is_ok_and(|p| !stage.has_prim(&p))
                }),
                _ => true,
            }
    }
}

fn items() -> impl Iterator<Item = Item> {
    [
        ("Convert to Editable Styles", "convert"),
        ("Show All", "show_all"),
        ("Remove All", "remove_all"),
    ]
    .map(|(title, operation)| Item {
        title,
        operation,
        style: "",
    })
    .into_iter()
    .chain(styles::GROUPS.iter().map(|g| Item {
        title: g.name,
        operation: "add",
        style: g.token,
    }))
}

pub(super) fn menu(levels: &[MenuPos]) -> Vec<(String, MenuItemSpec)> {
    items()
        .enumerate()
        .map(|(i, item)| {
            let mut pos = levels.to_vec();
            pos.push(MenuPos::new(
                if item.operation == "add" { 1 } else { 0 },
                i as i32,
            ));
            let id = format!("menu.layer.styles.{}{}", item.operation, item.style);
            (
                id,
                MenuItemSpec::new(&["Layer", "Layer Styles"], item.title, ACTION)
                    .workspace(WORKSPACE)
                    .at(pos)
                    .params(item.params())
                    .enabled_when(move |cx| {
                        item.available(
                            cx.stage,
                            &common::session(cx.resources).lock().selected_layers,
                        )
                    }),
            )
        })
        .collect()
}

pub(super) fn context_menu(ui: &mut Ui, cx: &mut PanelContext<'_>) {
    let layers = common::session(cx.resources).lock().selected_layers.clone();
    for item in items().filter(|it| it.style != "blendingOptions") {
        if item.style == "dropShadow" {
            ui.separator();
        }
        if ui
            .add_enabled(
                item.available(cx.stage, &layers),
                egui::Button::new(tr(item.title)),
            )
            .clicked()
        {
            cx.invoke_with(ACTION, item.params());
            ui.close();
        }
    }
}

pub(super) struct Actions;
impl ActionHandler for Actions {
    fn actions(&self) -> Vec<String> {
        vec![ACTION.into()]
    }
    fn workspace(&self) -> Option<&str> {
        Some(WORKSPACE)
    }
    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        if inv.id != ACTION {
            return false;
        }
        let session = common::session(cx.resources);
        let layers = session.lock().selected_layers.clone();
        let Some(item) = items().find(|it| it.params() == inv.params) else {
            return true;
        };
        if !item.available(cx.stage, &layers) {
            return true;
        }
        let mut params = item.params();
        params["layers"] = json!(layers);
        cx.invoke_with("layer.styles", params);
        if item.operation != "remove_all" {
            let mut s = session.lock();
            for l in layers {
                s.reveal.remove(&l);
                s.reveal_style_groups
                    .extend([l.to_string(), format!("{l}#styles")]);
                if !item.style.is_empty() {
                    s.reveal_style_groups
                        .push(format!("{l}#styles#{}", item.style));
                }
            }
        }
        true
    }
}

pub(super) fn node(stage: &Stage, layer: &PrimPath) -> Option<Node> {
    let path = layer.child(styles::PRIM).ok()?;
    let view = stage.prim(&path)?;
    let mut groups = Vec::new();
    for g in styles::GROUPS {
        let enabled = param_attr(g.enabled);
        if view.attr(&enabled).is_none() {
            continue;
        }
        let mut children = Vec::new();
        if g.enabled != 0 {
            children.push(Node::Prop {
                path: path.clone(),
                attr: enabled,
                label: tr("Enabled").into(),
            });
        }
        let props = g.fields.iter().map(|f| Node::Prop {
            path: path.clone(),
            attr: param_attr(f.id),
            label: if i18n::lang() == i18n::Lang::Zh {
                f.zh
            } else {
                f.name
            }
            .into(),
        });
        if g.enabled == 0 {
            let mut props = props;
            children.extend(props.by_ref().take(2));
            children.push(Node::Group {
                id: format!("{layer}#styles#advanced"),
                label: tr("Advanced Blending").into(),
                children: props.collect(),
            });
        } else {
            children.extend(props);
        }
        groups.push(Node::Group {
            id: format!("{layer}#styles#{}", g.token),
            label: tr(g.name).into(),
            children,
        });
    }
    Some(Node::Group {
        id: format!("{layer}#styles"),
        label: tr("Layer Styles").into(),
        children: groups,
    })
}

pub(super) fn edit(
    ui: &mut Ui,
    id: Id,
    path: &PrimPath,
    attr: &str,
    value: &Value,
) -> Option<Edit> {
    if path.name() != styles::PRIM {
        return None;
    }
    let f = styles::field_by_id(param_id(attr)?)?;
    match (f.kind, value) {
        (Control::Menu(_, names), Value::Int(n)) => {
            let mut current = (*n - 1).clamp(0, names.len() as i64 - 1) as usize;
            let before = current;
            let labels: Vec<_> = names.iter().map(|n| tr(n)).collect();
            bae_ui::widgets::dropdown(ui, id.with("style_choice"), &mut current, &labels, 140.);
            Some(if before == current {
                Edit::None
            } else {
                Edit::Commit(Value::Int(current as i64 + 1))
            })
        }
        (Control::Number(_, min, max), Value::Float(v)) => {
            let mut n = *v;
            let label = if i18n::lang() == i18n::Lang::Zh {
                f.zh
            } else {
                f.name
            };
            let r = ui
                .push_id(id, |ui| {
                    ui.add(
                        bae_ui::widgets::HotNumber::new(&mut n)
                            .label(label)
                            .speed(0.5)
                            .decimals(1)
                            .range(min..=max),
                    )
                })
                .inner;
            Some(if r.drag_stopped() {
                Edit::Commit(Value::Float(n))
            } else if r.changed() {
                if r.dragged() {
                    Edit::Live(Value::Float(n))
                } else {
                    Edit::Commit(Value::Float(n))
                }
            } else {
                Edit::None
            })
        }
        (Control::Gradient, Value::FloatList(stops)) => Some(gradient(ui, stops)),
        _ => None,
    }
}

fn gradient(ui: &mut Ui, stops: &[f64]) -> Edit {
    let mut points = stops.to_vec();
    let mut edit = Edit::None;
    ui.menu_button(tr("Edit Gradient"), |ui| {
        let mut changed = false;
        let mut dragging = false;
        let mut ended = false;
        let n = points.len() / 5;
        let mut remove = None;
        for (i, stop) in points.as_chunks_mut::<5>().0.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::DragValue::new(&mut stop[0])
                        .range(0.0..=1.0)
                        .speed(0.005),
                );
                changed |= r.changed();
                dragging |= r.dragged();
                ended |= r.drag_stopped();
                let mut rgba = [
                    stop[1] as f32,
                    stop[2] as f32,
                    stop[3] as f32,
                    stop[4] as f32,
                ];
                if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                    stop[1..].copy_from_slice(&rgba.map(f64::from));
                    changed = true;
                }
                if ui.add_enabled(n > 2, egui::Button::new("−")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            points.drain(i * 5..i * 5 + 5);
            changed = true;
        }
        if ui.button(tr("Add Color Stop")).clicked() {
            points.extend_from_slice(&[0.5, 0.5, 0.5, 0.5, 1.]);
            changed = true;
        }
        if ended || changed {
            edit = if dragging {
                Edit::Live(Value::FloatList(points.clone()))
            } else {
                Edit::Commit(Value::FloatList(points.clone()))
            };
        }
    });
    edit
}

pub(super) fn register_i18n() {
    let mut pairs: Vec<_> = styles::GROUPS.iter().map(|g| (g.name, g.zh)).collect();
    pairs.extend([
        ("Layer Styles", "图层样式"),
        ("Show All", "全部显示"),
        ("Remove All", "全部移除"),
        ("Convert to Editable Styles", "转换为可编辑样式"),
        ("Advanced Blending", "高级混合"),
        ("Edit Gradient", "编辑渐变"),
        ("Add Color Stop", "添加色标"),
        ("Single Color", "单色"),
        ("Gradient", "渐变"),
        ("Softer", "柔和"),
        ("Precise", "精确"),
        ("Edge", "边缘"),
        ("Center", "居中"),
        ("Outer Bevel", "外斜面"),
        ("Inner Bevel", "内斜面"),
        ("Emboss", "浮雕"),
        ("Pillow Emboss", "枕状浮雕"),
        ("Stroke Emboss", "描边浮雕"),
        ("Smooth", "平滑"),
        ("Chisel Hard", "雕刻清晰"),
        ("Chisel Soft", "雕刻柔和"),
        ("Up", "向上"),
        ("Down", "向下"),
        ("Linear", "线性"),
        ("Radial", "径向"),
        ("Reflected", "对称的"),
        ("Diamond", "菱形"),
        ("Outside", "外部"),
        ("Inside", "内部"),
    ]);
    i18n::register(&pairs);
}
