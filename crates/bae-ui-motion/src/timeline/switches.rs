//! Paint the starting switch's value down its column. One release is one edit.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) struct SwitchInput {
    press: bool,
    release: bool,
    pos: Option<Pos2>,
    cancel: bool,
}

impl SwitchInput {
    pub(super) fn read(i: &egui::InputState) -> Self {
        Self {
            press: i.pointer.primary_pressed(),
            release: i.pointer.primary_released(),
            pos: i.pointer.interact_pos(),
            cancel: i.key_pressed(egui::Key::Escape) || !i.focused,
        }
    }
}

pub(super) struct SwitchDrag {
    attr: &'static str,
    value: bool,
    previous: Pos2,
    changed: BTreeSet<PrimPath>,
}

fn set_switch(cx: &mut PanelContext<'_>, paths: impl serde::Serialize, attr: &str, on: bool) {
    let value = if attr == "quality" {
        bae_model::Value::Enum(if on { "best" } else { "draft" }.into())
    } else {
        bae_model::Value::Bool(on)
    };
    cx.invoke_with(
        "layer.set_attr",
        json!({"layers": paths, "attr": attr, "value": value}),
    );
}

/// Whether `layer` has the `attr` switch and can change it.
fn switch_applies(stage: &Stage, layer: &PrimPath, attr: &str) -> bool {
    if attr != "locked" && query::is_locked(stage, layer) {
        return false;
    }
    match attr {
        "audio_enabled" => matches!(
            query::layer_kind(stage, layer).as_deref(),
            Some("footage" | "audio" | "pre_comp")
        ),
        "effects_enabled" => !bae_motion::effect::effects_of(stage, layer).is_empty(),
        _ => true,
    }
}

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn switch_cell(
        &mut self,
        ui: &Ui,
        input: SwitchInput,
        cx: &mut PanelContext<'_>,
        rect: Rect,
        id: Id,
        layer: &PrimPath,
        label: &str,
        attr: &'static str,
        icon: Icon,
        mut on: bool,
        locked: bool,
    ) {
        let resp = ui.interact(rect, id, Sense::click_and_drag());
        if !locked || attr == "locked" {
            if input.press
                && resp.is_pointer_button_down_on()
                && let Some(pos) = input.pos
            {
                self.switch_drag = Some(SwitchDrag {
                    attr,
                    value: !on,
                    previous: pos,
                    changed: BTreeSet::new(),
                });
            }
            if let Some(drag) = &mut self.switch_drag
                && drag.attr == attr
            {
                if let Some(pos) = input.pos {
                    let cell = rect.intersect(ui.clip_rect());
                    // Cover intermediate rows even if two pointer samples are several rows apart.
                    let traversed = cell.contains(pos)
                        || (cell.x_range().contains(pos.x)
                            && cell.x_range().contains(drag.previous.x)
                            && cell.bottom() >= pos.y.min(drag.previous.y)
                            && cell.top() <= pos.y.max(drag.previous.y));
                    if traversed && on != drag.value {
                        drag.changed.insert(layer.clone());
                    }
                }
                if drag.changed.contains(layer) {
                    on = drag.value;
                }
            } else if resp.clicked() && !input.release {
                // Activation through the keyboard or accessibility tree has no pointer stroke.
                set_switch(cx, [layer], attr, !on);
            }
        }
        let resp = resp.labeled_toggle(
            egui::WidgetType::Checkbox,
            &bae_ui::a11y::context(&[label, tr(rows::switch_name(icon))]),
            on,
        );
        if matches!(icon, Icon::Shy) {
            icons::paint(
                ui.painter(),
                Rect::from_center_size(rect.center(), vec2(13.0, 13.0)),
                if on { Icon::ShyOn } else { Icon::Shy },
                if resp.hovered() {
                    color::TEXT_STRONG
                } else {
                    color::ICON
                },
            );
        } else {
            widgets::paint_switch(
                ui.painter(),
                rect.shrink2(vec2(1.0, 2.0)),
                on,
                icon,
                resp.hovered(),
            );
        }
    }

    pub(super) fn finish_switch_drag(&mut self, input: SwitchInput, cx: &mut PanelContext<'_>) {
        let Some(drag) = &mut self.switch_drag else {
            return;
        };
        if input.cancel {
            self.switch_drag = None;
        } else if input.release {
            let mut layers = std::mem::take(&mut drag.changed);
            // AE: clicking the switch of one of several selected layers sets it
            // on all of them (a sweep sets only the rows it crosses).
            if layers.len() == 1 {
                let session = common::session(cx.resources);
                let s = session.lock();
                if s.selected_layers.len() > 1
                    && layers
                        .first()
                        .is_some_and(|l| s.selected_layers.contains(l))
                {
                    layers.extend(
                        s.selected_layers
                            .iter()
                            .filter(|l| switch_applies(cx.stage, l, drag.attr))
                            .cloned(),
                    );
                }
            }
            if !layers.is_empty() {
                set_switch(cx, &layers, drag.attr, drag.value);
            }
            self.switch_drag = None;
        } else if let Some(pos) = input.pos {
            drag.previous = pos;
        }
    }
}
