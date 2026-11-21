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
    let (_, nodes) = catalog::MENUS.iter().find(|(t, _)| *t == top)?;
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
            for c in catalog::EFFECT_CATEGORIES {
                ui.add_enabled(false, egui::Button::new(tr(c)));
            }
        }
    }
}

fn catalog_item(ui: &mut Ui, cx: &mut PanelContext<'_>, it: &'static catalog::Item) {
    if !it.platform.current() {
        return;
    }
    let enabled = it.status.works();
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

/// AE's layer context menu: entries of the Layer, Effect and Animation menus,
/// `(top-level menu, entry title)`; `("", "")` is a separator. The last group
/// (Invert Selection, Select Children, Rename) only exists here.
pub(crate) const LAYER_MENU: &[(&str, &str)] = &[
    ("Layer", "New"),
    ("Layer", "Layer Settings..."),
    ("Layer", "Open Layer"),
    ("Layer", "Open Layer Source"),
    ("Layer", "Reveal in Explorer"),
    ("", ""),
    ("Layer", "Mask"),
    ("Layer", "Mask and Shape Path"),
    ("Layer", "Quality"),
    ("Layer", "Switches"),
    ("Layer", "Transform"),
    ("Layer", "Time"),
    ("Layer", "Frame Blending"),
    ("Layer", "3D Layer"),
    ("Layer", "Guide Layer"),
    ("Layer", "Environment Layer"),
    ("Layer", "Markers"),
    ("", ""),
    ("Layer", "Blending Mode"),
    ("Layer", "Layer Styles"),
    ("", ""),
    ("Effect", ""),
    ("Animation", "Keyframe Assistant"),
    ("", ""),
    ("Animation", "Track Camera"),
    ("Animation", "Warp Stabilizer VFX"),
    ("Animation", "Track Motion"),
    ("Animation", "Track Mask"),
    ("", ""),
    ("Layer", "Pre-compose..."),
    ("", ""),
    ("Layer", "Reveal"),
    ("Layer", "Create"),
];

/// What the layer menu asks the Timeline to do itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayerMenuOut {
    Rename,
}

/// Right-click on a layer (its name or bar): AE's layer context menu.
pub(crate) fn layer_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    layer: &PrimPath,
) -> Option<LayerMenuOut> {
    let stage = cx.stage;
    for (top, title) in LAYER_MENU {
        match (*top, *title) {
            ("", "") => {
                ui.separator();
            }
            ("Effect", "") => {
                if let Some((_, nodes)) = catalog::MENUS.iter().find(|(t, _)| *t == "Effect") {
                    let cats: &'static [Node] = nodes;
                    common::submenu(ui, tr("Effect"), &mut |ui| {
                        for n in cats.iter().filter(|n| matches!(n, Node::Effects)) {
                            catalog_node(ui, cx, n);
                        }
                    });
                }
            }
            (top, title) => {
                if let Some(n) = catalog_entry(top, title) {
                    catalog_node(ui, cx, n);
                }
            }
        }
    }
    ui.separator();
    let mut out = None;
    if ui.button(tr("Invert Selection")).clicked() {
        invert_layer_selection(stage, session, layer);
        ui.close();
    }
    if ui.button(tr("Select Children")).clicked() {
        select_children(stage, session);
        ui.close();
    }
    let locked = query::is_locked(stage, layer);
    if ui
        .add_enabled(!locked, egui::Button::new(tr("Rename")))
        .clicked()
    {
        out = Some(LayerMenuOut::Rename);
        ui.close();
    }
    out
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
