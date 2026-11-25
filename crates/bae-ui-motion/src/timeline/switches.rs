//! Paint the starting switch's value down its column. One release is one edit.
use super::*;
use bae_anim::FrameBlending;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) struct SwitchInput {
    press: bool,
    release: bool,
    pos: Option<Pos2>,
    cancel: bool,
    pixel_motion: bool,
}

impl SwitchInput {
    pub(super) fn read(i: &egui::InputState, pixel_motion: bool) -> Self {
        Self {
            pixel_motion,
            press: i.pointer.primary_pressed(),
            release: i.pointer.primary_released(),
            pos: i.pointer.interact_pos(),
            cancel: i.key_pressed(egui::Key::Escape) || !i.focused,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum SwitchValue {
    Toggle(bool),
    Frames(FrameBlending),
}

impl From<bool> for SwitchValue {
    fn from(on: bool) -> Self {
        Self::Toggle(on)
    }
}

impl SwitchValue {
    fn next(self, pixel_motion: bool) -> Self {
        match self {
            Self::Toggle(on) => Self::Toggle(!on),
            Self::Frames(mode) => Self::Frames(match mode {
                FrameBlending::Off => FrameBlending::FrameMix,
                FrameBlending::FrameMix if pixel_motion => FrameBlending::PixelMotion,
                _ => FrameBlending::Off,
            }),
        }
    }
}

pub(super) struct SwitchDrag {
    attr: &'static str,
    value: SwitchValue,
    previous: Pos2,
    changed: BTreeSet<PrimPath>,
}

fn set_switch(
    cx: &mut PanelContext<'_>,
    paths: impl serde::Serialize,
    attr: &str,
    value: SwitchValue,
) {
    use bae_model::Value;
    let value = match value {
        SwitchValue::Toggle(on) if attr == "quality" => {
            Value::Enum(if on { "best" } else { "draft" }.into())
        }
        SwitchValue::Toggle(on) => Value::Bool(on),
        SwitchValue::Frames(mode) => Value::Enum(
            match mode {
                FrameBlending::Off => "off",
                FrameBlending::FrameMix => "frame_mix",
                FrameBlending::PixelMotion => "pixel_motion",
            }
            .into(),
        ),
    };
    cx.invoke_with(
        "layer.set_attr",
        json!({"layers": paths, "attr": attr, "value": value}),
    );
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
        mut value: SwitchValue,
        locked: bool,
    ) -> egui::Response {
        let resp = ui.interact(rect, id, Sense::click_and_drag());
        if !locked || bae_model::editable_on_locked_layer(attr) {
            if input.press
                && resp.is_pointer_button_down_on()
                && let Some(pos) = input.pos
            {
                self.switch_drag = Some(SwitchDrag {
                    attr,
                    value: value.next(input.pixel_motion),
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
                    if traversed && value != drag.value {
                        drag.changed.insert(layer.clone());
                    }
                }
                if drag.changed.contains(layer) {
                    value = drag.value;
                }
            } else if resp.clicked() && !input.release {
                // Activation through the keyboard or accessibility tree has no pointer stroke.
                set_switch(cx, [layer], attr, value.next(input.pixel_motion));
            }
        }
        let name = tr(rows::switch_name(icon));
        let (on, mode) = match value {
            SwitchValue::Toggle(on) => (on, None),
            SwitchValue::Frames(FrameBlending::Off) => (false, Some(tr("Off"))),
            SwitchValue::Frames(FrameBlending::FrameMix) => (true, Some(tr("Frame Mix"))),
            SwitchValue::Frames(FrameBlending::PixelMotion) => (true, Some(tr("Pixel Motion"))),
        };
        let resp = if let Some(mode) = mode {
            resp.labeled_toggle(
                egui::WidgetType::Button,
                &bae_ui::a11y::context(&[label, name, mode]),
                on,
            )
            .on_hover_text(mode)
        } else {
            resp.labeled_toggle(
                egui::WidgetType::Checkbox,
                &bae_ui::a11y::context(&[label, name]),
                on,
            )
        };
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
        } else if value == SwitchValue::Frames(FrameBlending::PixelMotion) {
            let p = ui.painter();
            let rect = rect.shrink2(vec2(1.0, 2.0));
            p.rect_filled(rect.shrink(0.5), 1.0, color::SWITCH_BG);
            let stroke = Stroke::new(
                1.2,
                if resp.hovered() {
                    color::TEXT_STRONG
                } else {
                    color::ICON
                },
            );
            for dy in [-2.0, 2.0] {
                p.arrow(
                    rect.center() + vec2(-4.0, dy + 1.0),
                    vec2(8.0, -2.0),
                    stroke,
                );
            }
        } else {
            widgets::paint_switch(
                ui.painter(),
                rect.shrink2(vec2(1.0, 2.0)),
                on,
                icon,
                resp.hovered(),
            );
        }
        resp
    }

    pub(super) fn finish_switch_drag(&mut self, input: SwitchInput, cx: &mut PanelContext<'_>) {
        let Some(drag) = &mut self.switch_drag else {
            return;
        };
        if input.cancel {
            self.switch_drag = None;
        } else if input.release {
            if !drag.changed.is_empty() {
                set_switch(cx, &drag.changed, drag.attr, drag.value);
            }
            self.switch_drag = None;
        } else if let Some(pos) = input.pos {
            drag.previous = pos;
        }
    }
}
