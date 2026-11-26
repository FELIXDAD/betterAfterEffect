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

/// A property name's right-click menu (AE 26: Anchor Point and Position show
/// the same 15 entries): title (`None`: a separator) and whether it is enabled
/// for `attr` of `path` at keyframe time `key_now`.
pub(crate) fn property_menu_entries(
    stage: &Stage,
    path: &PrimPath,
    attr: &str,
    key_now: Time,
) -> Vec<(Option<&'static str>, bool)> {
    let spec = query::attr_spec(stage, path, attr);
    let keys: Vec<Time> = spec
        .map(|s| s.keyframes.keys().copied().collect())
        .unwrap_or_default();
    let on_layer = query::is_layer(stage, path);
    let animatable = stage
        .prim(path)
        .is_some_and(|v| bae_model::schema::is_animatable(v.prim_type(), attr));
    let edit_value = on_layer && crate::dialogs::TransformKind::of_attr(attr).is_some();
    let separable = on_layer
        && matches!(
            attr,
            "transform:position"
                | "transform:position_x"
                | "transform:position_y"
                | "transform:position_z"
                | "transform:anchor_point"
        );
    let effect = crate::effects::layer_of(path).is_some();
    vec![
        (Some("Edit Value..."), edit_value),
        // Essential Graphics properties (not built): greyed, as AE greys it here.
        (Some("Edit Property..."), false),
        (Some("Separate Dimensions"), separable),
        (None, true),
        (Some("Add Keyframe"), animatable),
        (Some("Remove All Keyframes"), !keys.is_empty()),
        (Some("Previous Keyframe"), keys.iter().any(|t| *t < key_now)),
        (Some("Next Keyframe"), keys.iter().any(|t| *t > key_now)),
        (None, true),
        (Some("Reset"), animatable),
        (Some("Edit Expression"), animatable),
        (Some("Copy Expression Only"), true),
        (Some("Reveal in Effect Controls"), effect),
        (Some("Keyframe Assistant"), true),
        // The Essential Graphics panel is not built yet.
        (Some("Add Property to Essential Graphics"), false),
    ]
}

/// A property name's right-click menu ([`property_menu_entries`]) for `attr` of
/// `path` at comp time `now`.
pub(crate) fn property_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    path: &PrimPath,
    attr: &str,
    now: Time,
) {
    let stage = cx.stage;
    let key_now = query::to_key_time(stage, path, now);
    for (title, enabled) in property_menu_entries(stage, path, attr, key_now) {
        let Some(title) = title else {
            ui.separator();
            continue;
        };
        if title == "Keyframe Assistant" {
            common::submenu(ui, tr(title), &mut |ui| {
                keyframe_assistant_menu(ui, cx, session);
            });
            continue;
        }
        if !ui
            .add_enabled(enabled, egui::Button::new(tr(title)))
            .clicked()
        {
            continue;
        }
        ui.close();
        let spec = query::attr_spec(stage, path, attr)
            .cloned()
            .unwrap_or_default();
        match title {
            "Edit Value..." => {
                if let (Some(kind), Some(info)) = (
                    crate::dialogs::TransformKind::of_attr(attr),
                    common::active_comp(stage, session),
                ) {
                    let layers = [path.clone()];
                    crate::dialogs::transform_value(cx, &info.path, &layers, now, kind, Some(attr));
                }
            }
            "Separate Dimensions" => {
                let separated = bae_motion::dimensions::is_separated(stage, path);
                cx.invoke_with(
                    "transform.separate_dimensions",
                    json!({"layers": [path], "separated": !separated}),
                );
            }
            "Add Keyframe" => {
                if spec.is_animated() {
                    let value = common::current_value(
                        &mut bae_anim::Evaluator::new(),
                        stage,
                        path,
                        attr,
                        now,
                    );
                    if let Some(value) = value {
                        cx.invoke_with(
                            "attr.set_at",
                            json!({"path": path, "attr": attr, "time": key_now, "value": value}),
                        );
                    }
                } else {
                    cx.invoke_with(
                        "attr.set_animated",
                        json!({"path": path, "attr": attr, "animated": true, "time": key_now}),
                    );
                }
            }
            "Remove All Keyframes" => {
                cx.invoke_with(
                    "attr.set_animated",
                    json!({"path": path, "attr": attr, "animated": false, "time": key_now}),
                );
            }
            "Previous Keyframe" | "Next Keyframe" => {
                let target = if title == "Previous Keyframe" {
                    spec.keyframes
                        .keys()
                        .copied()
                        .filter(|t| *t < key_now)
                        .max()
                } else {
                    spec.keyframes.keys().copied().find(|t| *t > key_now)
                };
                if let (Some(t), Some(info)) = (target, common::active_comp(stage, session)) {
                    session
                        .lock()
                        .set_time(&info, query::to_comp_time(stage, path, t));
                }
            }
            "Reset" => {
                let default = stage
                    .prim(path)
                    .and_then(|v| v.prim_type().cloned())
                    .and_then(|t| bae_model::schema::attr_def(&t, attr))
                    .and_then(|d| d.default.to_value());
                if let Some(value) = default {
                    cx.invoke_with(
                        "attr.set_at",
                        json!({"path": path, "attr": attr, "time": key_now, "value": value}),
                    );
                }
            }
            "Edit Expression" => {
                session.lock().focus_expression =
                    Some((path.clone(), attr.to_owned(), std::time::Instant::now()));
                if spec.expression.is_none() {
                    cx.invoke_with(
                        "attr.set_expression",
                        json!({"path": path, "attr": attr,
                               "source": crate::expr::default_expression(stage, path, attr)}),
                    );
                }
            }
            "Copy Expression Only" => {
                session.lock().expression_clipboard = spec.expression.map(|e| e.source);
            }
            "Reveal in Effect Controls" => {
                if let Some(layer) = crate::effects::layer_of(path) {
                    let mut s = session.lock();
                    s.selected_layers = vec![layer];
                    s.selected_effects = vec![path.clone()];
                }
                cx.open_panel("effect_controls");
            }
            _ => {}
        }
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
    // Edit Value… (AE-ANI-073): the property's value dialog, for this
    // keyframe (transform properties; the others' dialog is not built yet).
    let kind = crate::dialogs::TransformKind::of_attr(&clicked.attr)
        .filter(|_| query::is_layer(stage, &clicked.path));
    if ui
        .add_enabled(kind.is_some(), egui::Button::new(tr("Edit Value...")))
        .clicked()
    {
        if let (Some(kind), Some(info)) = (kind, common::active_comp(stage, session)) {
            let t = query::to_comp_time(stage, &clicked.path, clicked.time);
            let layers = [clicked.path.clone()];
            crate::dialogs::transform_value(cx, &info.path, &layers, t, kind, Some(&clicked.attr));
        }
        ui.close();
    }
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

/// An animator's Add menu (AE): Property ▸ (the Animate Text properties; those
/// not rendered yet are greyed), Selector ▸ Range (Wiggly and Expression greyed).
pub(crate) fn animator_add_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, animator: &PrimPath) {
    let add = |ui: &mut Ui, cx: &mut PanelContext<'_>, title: &str, property: &str, on: bool| {
        if ui.add_enabled(on, egui::Button::new(tr(title))).clicked() {
            cx.invoke_with(
                "text.animate",
                json!({"animator": animator, "property": property}),
            );
            ui.close();
        }
    };
    common::submenu(ui, tr("Property"), &mut |ui| {
        for (title, property, on) in [
            ("Anchor Point", "anchor_point", false),
            ("Position", "position", true),
            ("Scale", "scale", true),
            ("Skew", "skew", false),
            ("Rotation", "rotation", true),
            ("Opacity", "opacity", true),
        ] {
            add(ui, cx, title, property, on);
        }
        ui.separator();
        add(ui, cx, "Fill Color", "fill_rgb", true);
        add(ui, cx, "Stroke Color", "stroke_rgb", false);
        add(ui, cx, "Stroke Width", "stroke_width", false);
        ui.separator();
        add(ui, cx, "Tracking", "tracking", true);
        add(ui, cx, "Line Spacing", "line_spacing", false);
        ui.separator();
        add(ui, cx, "Blur", "blur", false);
    });
    common::submenu(ui, tr("Selector"), &mut |ui| {
        if ui.button(tr("Range")).clicked() {
            cx.invoke_with("text.add_selector", json!({ "animator": animator }));
            ui.close();
        }
        ui.add_enabled(false, egui::Button::new(tr("Wiggly")));
        ui.add_enabled(false, egui::Button::new(tr("Expression")));
    });
}
