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

/// Keyframe Assistant ▸ (the implemented assistants).
pub(crate) fn keyframe_assistant_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
) {
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
}

/// The layer menu's keyframe items: Keyframe Assistant ▸ and Toggle Hold Keyframe.
pub(crate) fn keyframe_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
    keyframe_assistant_menu(ui, cx, session);
    if ui.button(tr("Toggle Hold Keyframe")).clicked() {
        actions::handle(&Invocation::new(ids::TOGGLE_HOLD), cx, session);
        ui.close();
    }
}

/// Which keyframes "Select … Keyframes" picks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Related {
    /// Same value.
    Equal,
    /// Before the clicked keyframe.
    Previous,
    /// After the clicked keyframe.
    Following,
}

/// Select Equal / Previous / Following Keyframes (AE-ANI-029): keyframes of the
/// clicked keyframe's property, the clicked one included.
pub(crate) fn select_related(
    stage: &Stage,
    session: &SharedSession,
    clicked: &KeyRef,
    related: Related,
) {
    let Some(spec) = query::attr_spec(stage, &clicked.path, &clicked.attr) else {
        return;
    };
    let Some(value) = spec.keyframes.get(&clicked.time).map(|k| &k.value) else {
        return;
    };
    session.lock().selected_keys = spec
        .keyframes
        .iter()
        .filter(|(t, k)| match related {
            Related::Equal => k.value == *value,
            Related::Previous => **t <= clicked.time,
            Related::Following => **t >= clicked.time,
        })
        .map(|(t, _)| KeyRef::new(clicked.path.clone(), clicked.attr.as_str(), *t))
        .collect();
}

/// Whether Rove Across Time applies to `keys` (spatial keyframes that are not the
/// first or last of their property, dimensions not separated), and whether they
/// all rove now.
pub(crate) fn rove_state(stage: &Stage, keys: &[KeyRef]) -> (bool, bool) {
    if keys.is_empty() {
        return (false, false);
    }
    let mut all = true;
    for k in keys {
        let Some(spec) = query::attr_spec(stage, &k.path, &k.attr) else {
            return (false, false);
        };
        let first = spec.keyframes.keys().next() == Some(&k.time);
        let last = spec.keyframes.keys().next_back() == Some(&k.time);
        let Some(kf) = spec.keyframes.get(&k.time) else {
            return (false, false);
        };
        let spatial = bae_anim::schema::curve_kind(&k.attr, spec) == bae_anim::CurveKind::Spatial;
        if first || last || !spatial {
            return (false, false);
        }
        all &= kf.flags.roving;
    }
    (true, all)
}

/// Right-click on a keyframe (AE's keyframe context menu; the layer-bar keyframes,
/// the Graph Editor's keyframes and its Edit Selected Keyframes button). `clicked`
/// is the keyframe the Select … Keyframes items start from.
pub(crate) fn key_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    clicked: &KeyRef,
) {
    let stage = cx.stage;
    // Edit Value… (AE-ANI-073): the value dialog is not built yet.
    ui.add_enabled(false, egui::Button::new(tr("Edit Value...")));
    for (title, related) in [
        ("Select Equal Keyframes", Related::Equal),
        ("Select Previous Keyframes", Related::Previous),
        ("Select Following Keyframes", Related::Following),
    ] {
        if ui.button(tr(title)).clicked() {
            select_related(stage, session, clicked, related);
            ui.close();
        }
    }
    if ui.button(tr("Toggle Hold Keyframe")).clicked() {
        actions::handle(&Invocation::new(ids::TOGGLE_HOLD), cx, session);
        ui.close();
    }
    if ui.button(tr("Keyframe Interpolation...")).clicked() {
        actions::handle(&Invocation::new(ids::KEY_INTERPOLATION), cx, session);
        ui.close();
    }
    let keys = session.lock().selected_keys.clone();
    let (can_rove, roving) = rove_state(stage, &keys);
    let mut on = roving;
    let resp = ui
        .add_enabled_ui(can_rove, |ui| {
            widgets::checkbox(ui, &mut on, tr("Rove Across Time"))
        })
        .inner;
    if resp.clicked() && can_rove {
        let edits: Vec<bae_motion::KeyEdit> = keys
            .iter()
            .map(|k| {
                let mut e = bae_motion::KeyEdit::new(k.clone());
                e.roving = Some(!roving);
                e
            })
            .collect();
        cx.invoke_with("key.edit", json!({ "edits": edits }));
        ui.close();
    }
    if ui.button(tr("Keyframe Velocity...")).clicked() {
        actions::handle(&Invocation::new(ids::KEY_VELOCITY), cx, session);
        ui.close();
    }
    keyframe_assistant_menu(ui, cx, session);
}
