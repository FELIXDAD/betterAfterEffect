//! Timeline context menus.

use crate::catalog::{self, Node};

use super::*;

/// Keyframe Assistant ▸, as in Animation ▸ Keyframe Assistant (the command catalog).
pub(crate) fn keyframe_assistant_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    _session: &SharedSession,
) {
    if let Some(n) = catalog_entry("Animation", "Keyframe Assistant") {
        catalog_node(ui, cx, n);
    }
}

// ---------------------------------------------------------------------------
// Menus from the command catalog (`crate::catalog`): the same entries, action ids
// and greying as the menu bar.

/// The entry titled `title` directly in the top-level menu `top`.
pub(crate) fn catalog_entry(top: &str, title: &str) -> Option<&'static Node> {
    let (_, nodes) = catalog::menus().iter().find(|(t, _)| *t == top)?;
    nodes.iter().find(|n| match n {
        Node::Item(it) => it.title == title,
        Node::Submenu(s) => s.title == title,
        _ => false,
    })
}

/// AE's title of a catalog item on this platform.
fn menu_title(title: &'static str) -> &'static str {
    match title {
        "Reveal in Explorer" if cfg!(target_os = "macos") => "Reveal in Finder",
        t => t,
    }
}

/// Catalog entries as menu entries. AE sorts the marked items of a group by their
/// localized title.
pub(crate) fn catalog_nodes(ui: &mut Ui, cx: &mut PanelContext<'_>, nodes: &'static [Node]) {
    let mut i = 0;
    while i < nodes.len() {
        if matches!(&nodes[i], Node::Item(it) if it.sorted) {
            let mut run: Vec<&'static catalog::Item> = Vec::new();
            while let Some(Node::Item(it)) = nodes
                .get(i)
                .filter(|n| matches!(n, Node::Item(it) if it.sorted))
            {
                run.push(it);
                i += 1;
            }
            run.sort_by(|a, b| tr(a.title).cmp(tr(b.title)));
            for it in run {
                catalog_item(ui, cx, it);
            }
            continue;
        }
        catalog_node(ui, cx, &nodes[i]);
        i += 1;
    }
}

/// One catalog entry: an item runs its action (greyed until its command works), a
/// submenu opens.
pub(crate) fn catalog_node(ui: &mut Ui, cx: &mut PanelContext<'_>, node: &'static Node) {
    match node {
        Node::Item(it) => catalog_item(ui, cx, it),
        Node::Separator => {
            ui.separator();
        }
        Node::Submenu(s) if s.platform.current() => {
            if s.children.is_empty() {
                ui.add_enabled(false, egui::Button::new(tr(s.title)));
            } else {
                common::submenu(ui, tr(s.title), &mut |ui| catalog_nodes(ui, cx, s.children));
            }
        }
        Node::Submenu(_) => {}
        // The effect list comes from the effect registry (not wired to menus yet).
        Node::Effects => {
            for c in catalog::effect_categories() {
                ui.add_enabled(false, egui::Button::new(tr(c)));
            }
        }
    }
}

fn catalog_item(ui: &mut Ui, cx: &mut PanelContext<'_>, it: &'static catalog::Item) {
    // Run-time lists (recent files …) are filled elsewhere.
    if !it.platform.current() || it.status == catalog::Status::Dynamic {
        return;
    }
    let enabled = crate::context_menu::runs(cx, it);
    if ui
        .add_enabled(enabled, egui::Button::new(tr(menu_title(it.title))))
        .clicked()
    {
        let params = if it.params.is_empty() {
            json!({})
        } else {
            serde_json::from_str(it.params).unwrap_or_else(|_| json!({}))
        };
        cx.invoke_with(it.action, params);
        ui.close();
    }
}

/// What the layer menu asks the Timeline to do itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayerMenuOut {
    Rename,
}

/// Right-click on a layer (its name or bar): AE's layer context menu, the same
/// table as the Composition panel's (`context_menu::VIEWER_LAYER`).
pub(crate) fn layer_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    layer: &PrimPath,
) -> Option<LayerMenuOut> {
    let locked: &[&str] = if query::is_locked(cx.stage, layer) {
        &["Rename"]
    } else {
        &[]
    };
    match crate::context_menu::show_with(ui, cx, crate::context_menu::VIEWER_LAYER, locked) {
        Some("Rename") => return Some(LayerMenuOut::Rename),
        Some("Invert Selection") => invert_layer_selection(cx.stage, session, layer),
        Some("Select Children") => select_children(cx.stage, session),
        _ => {}
    }
    None
}

/// Invert Selection: the comp's other unlocked layers become the selection.
pub(crate) fn invert_layer_selection(stage: &Stage, session: &SharedSession, layer: &PrimPath) {
    let Some(comp) = query::owning_comp(stage, layer) else {
        return;
    };
    let mut s = session.lock();
    let selected = s.selected_layers.clone();
    s.selected_layers = query::comp_layers(stage, &comp)
        .into_iter()
        .filter(|l| !selected.contains(l) && !query::is_locked(stage, l))
        .collect();
    s.selected_keys.clear();
}

/// Select Children: adds every layer parented (directly or further down) to a
/// selected layer.
pub(crate) fn select_children(stage: &Stage, session: &SharedSession) {
    let mut s = session.lock();
    let Some(comp) = s
        .selected_layers
        .first()
        .and_then(|l| query::owning_comp(stage, l))
    else {
        return;
    };
    let layers = query::comp_layers(stage, &comp);
    let parent_of = |l: &PrimPath| {
        stage
            .prim(l)
            .and_then(|v| v.value("parent").and_then(|v| v.as_prim_ref()).cloned())
    };
    loop {
        let more: Vec<PrimPath> = layers
            .iter()
            .filter(|l| {
                !s.selected_layers.contains(l)
                    && !query::is_locked(stage, l)
                    && parent_of(l).is_some_and(|p| s.selected_layers.contains(&p))
            })
            .cloned()
            .collect();
        if more.is_empty() {
            break;
        }
        s.selected_layers.extend(more);
    }
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

/// Right-click on a keyframe: AE 26's keyframe context menu (the layer-bar
/// keyframes, the Graph Editor's keyframes and its Edit Selected Keyframes button).
/// `clicked` is the keyframe the value, Go To and Select … items start from.
pub(crate) fn key_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    clicked: &KeyRef,
) {
    let stage = cx.stage;
    let spec = query::attr_spec(stage, &clicked.path, &clicked.attr);
    let kf = spec.and_then(|s| s.keyframes.get(&clicked.time));
    // The keyframe's value (greyed, as in AE).
    let value = kf
        .and_then(|k| crate::graph::comps(&k.value))
        .map(|c| {
            c.iter()
                .map(|v| crate::graph::fmt_num(*v))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    ui.add_enabled(false, egui::Button::new(value));
    // Edit Value… (AE-ANI-073): the value dialog is not built yet.
    ui.add_enabled(false, egui::Button::new(tr("Edit Value...")));
    if ui.button(tr("Go To Keyframe Time")).clicked() {
        if let Some(info) = common::active_comp(stage, session) {
            let t = query::to_comp_time(stage, &clicked.path, clicked.time);
            session.lock().set_time(&info, t);
        }
        ui.close();
    }
    ui.separator();
    let current = kf.map_or(0, |k| usize::from(k.label));
    common::submenu(ui, tr("Label"), &mut |ui| {
        if ui.button(tr("Select Label Group")).clicked() {
            select_label_group(stage, session, clicked);
            ui.close();
        }
        ui.separator();
        for (i, (name, _)) in theme::LABELS.iter().enumerate() {
            let mut on = i == current;
            if widgets::checkbox(ui, &mut on, tr(name)).clicked() {
                let keys = session.lock().selected_keys.clone();
                let edits: Vec<bae_motion::KeyEdit> = keys
                    .iter()
                    .map(|k| {
                        let mut e = bae_motion::KeyEdit::new(k.clone());
                        e.label = Some(i as u8);
                        e
                    })
                    .collect();
                cx.invoke_with("key.edit", json!({ "edits": edits }));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Edit Label Colors...")).clicked() {
            cx.invoke_with("app.settings", json!({"page": "labels"}));
            ui.close();
        }
    });
    if let Some(n) = catalog_entry("Edit", "Select Keyframe Label Group") {
        catalog_node(ui, cx, n);
    }
    ui.separator();
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
    ui.separator();
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
    ui.separator();
    keyframe_assistant_menu(ui, cx, session);
}

/// Label ▸ Select Label Group: the keyframes of the clicked keyframe's layer with
/// its label.
pub(crate) fn select_label_group(stage: &Stage, session: &SharedSession, clicked: &KeyRef) {
    let Some(label) = query::attr_spec(stage, &clicked.path, &clicked.attr)
        .and_then(|s| s.keyframes.get(&clicked.time))
        .map(|k| k.label)
    else {
        return;
    };
    let Some(layer) = query::owning_layer(stage, &clicked.path) else {
        return;
    };
    session.lock().selected_keys = keys_with_labels(stage, &[layer], &[label]);
}

/// Every keyframe of `layers` (and the prims below them) whose label is in `labels`.
pub(crate) fn keys_with_labels(stage: &Stage, layers: &[PrimPath], labels: &[u8]) -> Vec<KeyRef> {
    let mut out = Vec::new();
    let mut stack: Vec<PrimPath> = layers.to_vec();
    while let Some(p) = stack.pop() {
        if let Some(v) = stage.prim(&p) {
            for name in v.attr_names() {
                if let Some(spec) = v.attr(name) {
                    for (t, k) in &spec.keyframes {
                        if labels.contains(&k.label) {
                            out.push(KeyRef::new(p.clone(), name, *t));
                        }
                    }
                }
            }
        }
        stack.extend(stage.children(&p));
    }
    out
}
