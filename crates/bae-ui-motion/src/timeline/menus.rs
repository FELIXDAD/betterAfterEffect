//! Timeline context menus.

use super::*;

pub(super) fn new_layer_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
    bae_ui::widgets::menu_button(ui, tr("New"), |ui| {
        let items: [(&str, LayerKind, Option<&str>); 9] = [
            ("Text", LayerKind::Text, None),
            ("Solid", LayerKind::Solid, None),
            ("Light", LayerKind::Light, None),
            ("Camera", LayerKind::Camera, None),
            ("Null Object", LayerKind::Null, None),
            ("Shape Layer", LayerKind::Shape, None),
            ("Rectangle", LayerKind::Shape, Some("rect")),
            ("Ellipse", LayerKind::Shape, Some("ellipse")),
            ("Adjustment Layer", LayerKind::Adjustment, None),
        ];
        for (title, kind, shape) in items {
            if ui.button(tr(title)).clicked() {
                let mut params = json!({"kind": kind});
                if let Some(s) = shape {
                    params["shape"] = json!(s);
                }
                actions::handle(
                    &Invocation::with_params(ids::NEW_LAYER, params),
                    cx,
                    session,
                );
                ui.close();
            }
        }
    });
}

pub(crate) fn keyframe_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
    bae_ui::widgets::menu_button(ui, tr("Keyframe Assistant"), |ui| {
        for (title, id, params) in [
            ("Easy Ease", ids::EASE, json!({"mode": "easy_ease"})),
            ("Easy Ease In", ids::EASE, json!({"mode": "ease_in"})),
            ("Easy Ease Out", ids::EASE, json!({"mode": "ease_out"})),
            ("Time-Reverse Keyframes", ids::REVERSE_KEYS, json!({})),
        ] {
            if ui.button(tr(title)).clicked() {
                actions::handle(&Invocation::with_params(id, params), cx, session);
                ui.close();
            }
        }
    });
    if ui.button(tr("Toggle Hold Keyframe")).clicked() {
        actions::handle(&Invocation::new(ids::TOGGLE_HOLD), cx, session);
        ui.close();
    }
}
