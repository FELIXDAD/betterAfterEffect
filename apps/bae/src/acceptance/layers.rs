//! Compositions and layers: Cmd+N, every Layer ▸ New item (menu and AE
//! shortcut), greyed out without a comp, Arrange, Duplicate, Split; undo / redo.

use bae_motion::query;
use egui::{Key, Modifiers};

use super::*;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

fn kind(h: &Ui, layer: &PrimPath) -> String {
    query::layer_kind(h.state().stage(), layer).unwrap_or_default()
}

fn undo(h: &mut Ui) {
    key(h, Modifiers::COMMAND, Key::Z);
}

fn redo(h: &mut Ui) {
    key(h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
}

/// Layer ▸ New items: registry id, expected layer kind.
const NEW_ITEMS: [(&str, &str); 7] = [
    ("motion.layer.new.text", "text"),
    ("motion.layer.new.solid", "solid"),
    ("motion.layer.new.light", "light"),
    ("motion.layer.new.camera", "camera"),
    ("motion.layer.new.null", "null"),
    ("motion.layer.new.shape", "shape"),
    ("motion.layer.new.adjustment", "adjustment"),
];

#[test]
fn new_composition_with_cmd_n_opens_it() {
    let mut h = gui();
    assert!(h.state().active_comp().is_none());
    new_comp(&mut h);
    let comp = h.state().active_comp().expect("Cmd+N opened no comp");
    assert_eq!(h.state().name(&comp), "Comp 1");
    // Timeline and viewer show it.
    assert!(
        find_text(&h, "Timeline Comp 1").is_some(),
        "{:?}",
        painted_texts(&h)
    );
    new_comp(&mut h);
    let second = h.state().active_comp().unwrap();
    assert_eq!(h.state().name(&second), "Comp 2");
    // Undo removes the second comp; the timeline falls back without crashing.
    undo(&mut h);
    assert!(h.state().stage().prim(&second).is_none());
    settle(&mut h);
    redo(&mut h);
    assert!(h.state().stage().prim(&second).is_some());
}

#[test]
fn every_layer_new_item_from_the_menu() {
    let mut h = gui();
    new_comp(&mut h);
    for (id, want) in NEW_ITEMS {
        let before = h.state().layers();
        assert!(h.state_mut().menu(id), "{id} disabled with a comp open");
        settle(&mut h);
        confirm_dialog(&mut h);
        let after = h.state().layers();
        assert_eq!(
            after.len(),
            before.len() + 1,
            "{id} created no layer: {:?}",
            h.state().outcomes
        );
        let new = after.iter().find(|l| !before.contains(l)).unwrap().clone();
        assert_eq!(kind(&h, &new), want, "{id}");
        assert_eq!(
            h.state().selected_layers(),
            std::slice::from_ref(&new),
            "{id}: new layer not selected"
        );
        // AE puts a new layer above the selected one (on top here).
        assert_eq!(after[0], new, "{id}: not on top");
    }
    // Rectangle and ellipse carry their shape.
    let names = h.state().layer_names();
    println!("layer names: {names:?}");
    // Undo all nine, redo all nine.
    for _ in 0..NEW_ITEMS.len() {
        undo(&mut h);
    }
    assert!(
        h.state().layers().is_empty(),
        "{:?}",
        h.state().layer_names()
    );
    for _ in 0..NEW_ITEMS.len() {
        redo(&mut h);
    }
    assert_eq!(h.state().layer_names(), names);
}

#[test]
fn layer_shortcuts_follow_ae() {
    let cmd = Modifiers::COMMAND;
    let cases = [
        (cmd, Key::Y, "solid"),
        (cmd | Modifiers::ALT, Key::Y, "adjustment"),
        (cmd | Modifiers::ALT | Modifiers::SHIFT, Key::Y, "null"),
        (cmd | Modifiers::ALT | Modifiers::SHIFT, Key::T, "text"),
        (cmd | Modifiers::ALT | Modifiers::SHIFT, Key::C, "camera"),
        (cmd | Modifiers::ALT | Modifiers::SHIFT, Key::L, "light"),
    ];
    let mut h = gui();
    new_comp(&mut h);
    for (m, k, want) in cases {
        let before = h.state().layers();
        key(&mut h, m, k);
        // Cmd+Y asks for Solid Settings first.
        confirm_dialog(&mut h);
        let after = h.state().layers();
        assert_eq!(after.len(), before.len() + 1, "{m:?}+{k:?} ({want})");
        assert_eq!(kind(&h, &after[0]), want, "{m:?}+{k:?}");
    }
}

#[test]
fn layer_items_are_greyed_out_and_shortcuts_do_nothing_without_a_comp() {
    let mut h = gui();
    for (id, _) in NEW_ITEMS {
        assert!(
            !h.state().menu_item(id).enabled,
            "{id} enabled without a comp"
        );
    }
    for id in [
        "motion.composition.trim",
        "motion.composition.render_queue",
        "motion.layer.arrange.front",
        "motion.edit.duplicate",
        "motion.edit.split",
        "motion.animation.ease",
        "motion.animation.hold",
    ] {
        assert!(
            !h.state().menu_item(id).enabled,
            "{id} enabled without a comp"
        );
    }
    assert!(h.state().menu_item("motion.composition.new").enabled);
    // The shortcuts of the greyed-out items do nothing (and do not fail).
    let cmd = Modifiers::COMMAND;
    for (m, k) in [
        (cmd, Key::Y),
        (cmd | Modifiers::ALT, Key::Y),
        (cmd | Modifiers::ALT | Modifiers::SHIFT, Key::T),
        (cmd, Key::D),
        (cmd | Modifiers::SHIFT, Key::D),
        (cmd | Modifiers::CTRL, Key::M),
        (Modifiers::NONE, Key::F9),
    ] {
        key(&mut h, m, k);
    }
    assert!(
        h.state().stage().prim(&"/comps".parse().unwrap()).is_none()
            || h.state().layers().is_empty()
    );
    assert!(
        !h.state().doc.is_dirty(),
        "a greyed-out shortcut changed the project"
    );
    assert!(
        h.state().render_queue().lock().items().is_empty(),
        "Ctrl+Cmd+M queued without a comp"
    );
    assert!(
        h.state()
            .outcomes
            .iter()
            .all(|(_, o)| !o.starts_with("failed")),
        "{:?}",
        h.state().outcomes
    );

    // After closing the last comp's project (File ▸ Close Project) they grey out again.
    new_comp(&mut h);
    assert!(h.state().menu_item("motion.layer.new.solid").enabled);
    h.state_mut()
        .dialogs
        .answers
        .push_back(UnsavedChoice::DontSave);
    assert!(h.state_mut().menu("shell.file.close_project"));
    settle(&mut h);
    assert!(!h.state().menu_item("motion.layer.new.solid").enabled);
}

#[test]
fn arrange_duplicate_and_split() {
    let mut h = gui();
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.text",
        ],
    );
    let [top, mid, bottom]: [PrimPath; 3] = h.state().layers().try_into().unwrap();
    let cmd = Modifiers::COMMAND;
    let select = |h: &mut Ui, l: &PrimPath| {
        h.state().session().lock().selected_layers = vec![l.clone()];
        settle(h);
    };
    assert!(
        !h.state().menu_item("motion.layer.arrange.front").enabled
            || !h.state().selected_layers().is_empty()
    );

    // Bring to Front (Cmd+Shift+]) — bottom goes on top.
    select(&mut h, &bottom);
    assert!(h.state().menu_item("motion.layer.arrange.front").enabled);
    key(&mut h, cmd | Modifiers::SHIFT, Key::CloseBracket);
    assert_eq!(
        h.state().layers(),
        [bottom.clone(), top.clone(), mid.clone()]
    );
    // Send Backward (Cmd+[) — one down.
    key(&mut h, cmd, Key::OpenBracket);
    assert_eq!(
        h.state().layers(),
        [top.clone(), bottom.clone(), mid.clone()]
    );
    // Send to Back (Cmd+Shift+[).
    key(&mut h, cmd | Modifiers::SHIFT, Key::OpenBracket);
    assert_eq!(
        h.state().layers(),
        [top.clone(), mid.clone(), bottom.clone()]
    );
    // Bring Forward (Cmd+]).
    key(&mut h, cmd, Key::CloseBracket);
    assert_eq!(
        h.state().layers(),
        [top.clone(), bottom.clone(), mid.clone()]
    );
    // Send to Back on the bottom layer, Bring to Front on the top one: no change.
    select(&mut h, &mid);
    key(&mut h, cmd | Modifiers::SHIFT, Key::OpenBracket);
    select(&mut h, &top);
    key(&mut h, cmd | Modifiers::SHIFT, Key::CloseBracket);
    assert_eq!(
        h.state().layers(),
        [top.clone(), bottom.clone(), mid.clone()]
    );
    // The same through the menu.
    select(&mut h, &mid);
    assert!(h.state_mut().menu("motion.layer.arrange.front"));
    settle(&mut h);
    assert_eq!(h.state().layers()[0], mid);

    // Duplicate (Cmd+D): a copy right above the original.
    select(&mut h, &bottom);
    let before: Vec<PrimPath> = h.state().layers();
    key(&mut h, cmd, Key::D);
    let layers = h.state().layers();
    assert_eq!(layers.len(), before.len() + 1);
    let copy: Vec<PrimPath> = layers
        .iter()
        .filter(|l| !before.contains(l))
        .cloned()
        .collect();
    assert_eq!(copy.len(), 1);
    let i = layers.iter().position(|l| *l == copy[0]).unwrap();
    assert_eq!(
        layers[i + 1],
        bottom,
        "duplicate not right above the original"
    );
    assert_eq!(kind(&h, &copy[0]), kind(&h, &bottom));
    select(&mut h, &copy[0]);

    // Split (Cmd+Shift+D) at 2 s: two layers meeting at 2 s.
    let solid = copy[0].clone();
    for _ in 0..60 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    settle(&mut h);
    let t = h.state().now();
    let before = h.state().layers().len();
    key(&mut h, cmd | Modifiers::SHIFT, Key::D);
    assert_eq!(h.state().layers().len(), before + 1, "split made no layer");
    let a = query::layer_timing(h.state().stage(), &solid);
    let new: Vec<_> = h
        .state()
        .layers()
        .into_iter()
        .filter(|l| query::layer_timing(h.state().stage(), l).in_point == t)
        .collect();
    assert_eq!(new.len(), 1, "no layer starts at the split time");
    assert_eq!(a.out_point, t, "original does not end at the split time");

    // Seven undo steps (the two no-op arranges made none) restore the start.
    for _ in 0..7 {
        undo(&mut h);
    }
    assert_eq!(
        h.state().layers(),
        [top, mid, bottom],
        "{:?}",
        h.state().layer_names()
    );
}

#[test]
fn duplicate_and_split_need_a_selection() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    assert!(h.state().selected_layers().is_empty());
    for id in [
        "motion.edit.duplicate",
        "motion.edit.split",
        "motion.layer.arrange.back",
    ] {
        assert!(
            !h.state().menu_item(id).enabled,
            "{id} enabled with nothing selected"
        );
    }
    key(&mut h, Modifiers::COMMAND, Key::D);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    assert_eq!(h.state().layers().len(), 1);
}

#[test]
fn duplicate_and_split_select_the_new_layer() {
    // AE: Edit > Duplicate and Split Layer select the new layer, so the next
    // edit (or the next split further on) applies to it.
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let original = h.state().layers()[0].clone();
    key(&mut h, Modifiers::COMMAND, Key::D);
    let copy = h.state().layers()[0].clone();
    assert_ne!(copy, original);
    assert_eq!(
        h.state().selected_layers(),
        std::slice::from_ref(&copy),
        "duplicate: the original is still selected"
    );

    // Split twice at 1 s and 2 s: the second split cuts the part after 1 s.
    h.state().session().lock().selected_layers = vec![copy.clone()];
    for _ in 0..2 {
        for _ in 0..30 {
            press(&mut h, Modifiers::NONE, Key::PageDown);
        }
        key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    }
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(_, o)| o.starts_with("failed"))
        .collect();
    assert!(failed.is_empty(), "second split: {failed:?}");
    assert_eq!(h.state().layers().len(), 4, "{:?}", h.state().layer_names());
}

// ---- Retest of #108 / #109 / #114 (dev 763eaad): edge cases ----

#[test]
fn split_four_times_in_a_row_cuts_the_newest_part_each_time() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mut cuts = Vec::new();
    for _ in 0..4 {
        for _ in 0..30 {
            press(&mut h, Modifiers::NONE, Key::PageDown);
        }
        settle(&mut h);
        cuts.push(h.state().now());
        key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
        let selected = h.state().selected_layers();
        assert_eq!(
            selected.len(),
            1,
            "after split {}: {selected:?}",
            cuts.len()
        );
        let t = query::layer_timing(h.state().stage(), &selected[0]);
        assert_eq!(
            t.in_point,
            *cuts.last().unwrap(),
            "the selected part does not start at the cut"
        );
    }
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(_, o)| o.starts_with("failed"))
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
    // Five contiguous segments: 0–1 s, 1–2 s, 2–3 s, 3–4 s, 4 s–end.
    let mut spans: Vec<(bae_model::Time, bae_model::Time)> = h
        .state()
        .layers()
        .iter()
        .map(|l| {
            let t = query::layer_timing(h.state().stage(), l);
            (t.in_point, t.out_point)
        })
        .collect();
    spans.sort();
    assert_eq!(spans.len(), 5, "{spans:?}");
    for w in spans.windows(2) {
        assert_eq!(w[0].1, w[1].0, "gap or overlap between segments: {spans:?}");
    }
    assert_eq!(spans[0].0, bae_model::Time::ZERO);
    // Four undos give back one layer, four redos the five segments.
    for _ in 0..4 {
        undo(&mut h);
    }
    assert_eq!(h.state().layers().len(), 1);
    for _ in 0..4 {
        redo(&mut h);
    }
    assert_eq!(h.state().layers().len(), 5);
}

#[test]
fn duplicate_several_layers_selects_all_copies_and_survives_undo() {
    let mut h = gui();
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.text",
        ],
    );
    let originals = h.state().layers();
    h.state().session().lock().selected_layers = vec![originals[0].clone(), originals[2].clone()];
    key(&mut h, Modifiers::COMMAND, Key::D);
    let after = h.state().layers();
    assert_eq!(after.len(), 5);
    let copies: Vec<PrimPath> = after
        .iter()
        .filter(|l| !originals.contains(l))
        .cloned()
        .collect();
    let mut selected = h.state().selected_layers();
    selected.sort();
    let mut want = copies.clone();
    want.sort();
    assert_eq!(selected, want, "the two copies are not the selection");
    // Cmd+D again duplicates the copies.
    key(&mut h, Modifiers::COMMAND, Key::D);
    assert_eq!(h.state().layers().len(), 7);
    // Undo twice: the selection no longer points at removed layers, and
    // Cmd+D then does nothing wrong.
    undo(&mut h);
    undo(&mut h);
    assert_eq!(h.state().layers(), originals);
    settle(&mut h);
    let stale: Vec<_> = h
        .state()
        .selected_layers()
        .into_iter()
        .filter(|l| h.state().stage().prim(l).is_none())
        .collect();
    assert!(
        stale.is_empty(),
        "selection keeps removed layers: {stale:?}"
    );
    key(&mut h, Modifiers::COMMAND, Key::D);
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(_, o)| o.starts_with("failed"))
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn cmd_shift_x_trims_the_comp_to_the_work_area() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let comp = h.state().active_comp().unwrap();
    let duration = |h: &Ui| query::comp_info(h.state().stage(), &comp).unwrap().duration;
    let before = duration(&h);
    // Work area 1 s – 3 s (B at 30 frames, N at 89 frames: N is inclusive).
    for _ in 0..30 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    key(&mut h, Modifiers::NONE, Key::B);
    for _ in 0..59 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    key(&mut h, Modifiers::NONE, Key::N);
    assert_eq!(
        h.state()
            .menu_item("motion.composition.trim")
            .chord
            .as_deref(),
        Some("Cmd+Shift+X")
    );
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::X);
    let sec = bae_model::Time(bae_model::Time::FLICKS_PER_SECOND);
    assert_eq!(
        duration(&h),
        bae_model::Time(sec.0 * 2),
        "comp not trimmed to the 2 s work area"
    );
    undo(&mut h);
    assert_eq!(duration(&h), before);
}

// ---- Retest of #124 (dev c20770a): locked layers, edge cases ----

fn lock(h: &mut Ui, layer: &PrimPath) {
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        serde_json::json!({"path": layer, "attr": "locked", "value": {"type": "bool", "value": true}}),
    ));
    settle(h);
}

fn is_locked(h: &Ui, l: &PrimPath) -> bool {
    bae_motion::query::is_locked(h.state().stage(), l)
}

/// The timeline row of layer `name`: its painted name.
fn row_of(h: &Ui, name: &str) -> egui::Rect {
    let head = find_text(h, "Layer Name").expect("timeline");
    find_texts(h, |t| t == name)
        .into_iter()
        .find(|r| r.top() > head.bottom() && r.left() < head.left() + 400.0)
        .unwrap_or_else(|| panic!("no row {name:?}"))
}

/// Clicks the Lock switch on `name`'s timeline row (accesskit name
/// "Layer 1 “name” · Lock", issue #128).
fn click_lock_switch(h: &mut Ui, name: &str) {
    let row = row_of(h, name);
    let suffix = format!("“{name}” · Lock");
    let switch = h
        .root()
        .children_recursive()
        .filter(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Layer ") && l.ends_with(&suffix))
        })
        .map(|n| n.rect())
        .find(|r| (r.center().y - row.center().y).abs() < 6.0)
        .unwrap_or_else(|| panic!("no Lock switch on {name}'s row"));
    click_at(h, switch.center());
}

fn position_keys(h: &Ui, l: &PrimPath) -> usize {
    bae_motion::query::attr_spec(h.state().stage(), l, "transform:position")
        .map_or(0, |a| a.keyframes.len())
}

#[test]
fn lock_switch_with_undo_and_redo() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let l = h.state().layers()[0].clone();
    // A keyframe before locking.
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(position_keys(&h, &l), 1);
    click_lock_switch(&mut h, "Dark Gray Solid 1");
    assert!(is_locked(&h, &l), "the Lock switch did not lock");
    // Locked: Option+Shift+P does nothing, even with the layer forced into the selection.
    h.state().session().lock().selected_layers = vec![l.clone()];
    for _ in 0..5 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(
        position_keys(&h, &l),
        1,
        "a keyframe was added to a locked layer"
    );
    // Undo unlocks (the lock was the last step); the layer is editable again.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(!is_locked(&h, &l), "undo did not unlock");
    h.state().session().lock().selected_layers = vec![l.clone()];
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(position_keys(&h, &l), 2, "unlocked layer not editable");
    // Undo that keyframe and the first one: undo goes through the history even
    // across a lock; redo the lock again.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(position_keys(&h, &l), 1);
    click_lock_switch(&mut h, "Dark Gray Solid 1");
    assert!(is_locked(&h, &l));
    key(&mut h, Modifiers::COMMAND, Key::Z); // unlock
    key(&mut h, Modifiers::COMMAND, Key::Z); // first keyframe
    assert_eq!(position_keys(&h, &l), 0);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert!(
        is_locked(&h, &l) && position_keys(&h, &l) == 1,
        "redo did not restore key + lock"
    );
    // The switch unlocks it again.
    click_lock_switch(&mut h, "Dark Gray Solid 1");
    assert!(!is_locked(&h, &l), "the Lock switch did not unlock");
}

fn set_parent(h: &mut Ui, layer: &PrimPath, parent: Option<&PrimPath>) -> String {
    let n = h.state().outcomes.len();
    h.state_mut().invoke(Invocation::with_params(
        "layer.set_parent",
        serde_json::json!({"layers": [layer], "parent": parent}),
    ));
    settle(h);
    h.state().outcomes[n..]
        .iter()
        .map(|(_, o)| o.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parent_of(h: &Ui, l: &PrimPath) -> Option<String> {
    h.state()
        .stage()
        .prim(l)
        .and_then(|v| bae_motion::query::static_value(&v, "parent"))
        .map(|v| format!("{v:?}"))
}

#[test]
fn locked_layers_and_parenting() {
    let mut h = gui();
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.solid",
        ],
    );
    let [a, p, c]: [PrimPath; 3] = h.state().layers().try_into().unwrap();
    // c is locked: it cannot be given a parent (pick whip on it edits it) …
    lock(&mut h, &c);
    let out = set_parent(&mut h, &c, Some(&p));
    assert!(
        out.starts_with("failed") && out.contains("locked"),
        "parenting a locked layer: {out}"
    );
    // … but another layer may use it as its parent (that edits the other layer).
    let out = set_parent(&mut h, &a, Some(&c));
    assert_eq!(out, "changed", "parenting to a locked layer: {out}");
    // Unlocked parent of a locked child: moving the parent is allowed.
    key(&mut h, Modifiers::COMMAND, Key::Z); // a's parent
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        serde_json::json!({"path": c, "attr": "locked", "value": {"type": "bool", "value": false}}),
    ));
    settle(&mut h);
    assert_eq!(set_parent(&mut h, &c, Some(&p)), "changed");
    lock(&mut h, &c);
    let world = |h: &Ui| {
        let comp = h.state().active_comp().unwrap();
        bae_anim::evaluate_comp(h.state().stage(), &comp, bae_model::Time::ZERO)
            .unwrap()
            .layer(&c)
            .map(|l| l.world_matrix)
            .unwrap()
    };
    // Moving the unlocked parent moves the locked child with it (AE).
    let before = world(&h);
    h.state_mut().invoke(Invocation::with_params(
        "attr.set_at",
        serde_json::json!({"path": p, "attr": "transform:position", "time": 0,
                           "value": {"type": "vec3", "value": [100.0, 100.0, 0.0]}}),
    ));
    settle(&mut h);
    assert_eq!(
        h.state().outcomes.last().map(|(_, o)| o.as_str()),
        Some("changed")
    );
    assert!(
        !before.abs_diff_eq(world(&h), 1e-6),
        "the locked child did not follow its parent"
    );
    assert!(is_locked(&h, &c));
}

#[test]
fn locked_layers_copy_paste_and_duplicate() {
    let mut h = gui();
    comp_with(
        &mut h,
        &["motion.layer.new.solid", "motion.layer.new.solid"],
    );
    let [top, bottom]: [PrimPath; 2] = h.state().layers().try_into().unwrap();
    // Keyframes on the bottom layer, then lock it.
    h.state().session().lock().selected_layers = vec![bottom.clone()];
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    for _ in 0..10 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    lock(&mut h, &bottom);
    let keys: Vec<bae_motion::KeyRef> =
        bae_motion::query::attr_spec(h.state().stage(), &bottom, "transform:position")
            .unwrap()
            .keyframes
            .keys()
            .map(|t| bae_motion::KeyRef::new(bottom.clone(), "transform:position", *t))
            .collect();
    // Copy from the locked layer (reading is fine), paste on the unlocked one.
    // Keyframe copy / paste is a Timeline action: focus it (a click on the
    // empty graph also clears the selection, so select afterwards).
    let head = find_text(&h, "Layer Name").unwrap();
    click_at(
        &mut h,
        egui::pos2(head.left() + 800.0, head.bottom() + 150.0),
    );
    // egui-winit turns Cmd+C / Cmd+V into Copy / Paste events.
    h.state().session().lock().selected_keys = keys.clone();
    send(&mut h, Event::Copy);
    settle(&mut h);
    key(&mut h, Modifiers::NONE, Key::Home);
    {
        let s = h.state().session();
        let mut s = s.lock();
        s.selected_keys.clear();
        s.selected_layers = vec![top.clone()];
    }
    send(&mut h, Event::Paste(String::new()));
    settle(&mut h);
    assert_eq!(
        position_keys(&h, &top),
        2,
        "paste from a locked layer's keys"
    );
    // Paste into the locked layer: nothing changes.
    h.state().session().lock().selected_layers = vec![bottom.clone()];
    for _ in 0..30 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    send(&mut h, Event::Paste(String::new()));
    settle(&mut h);
    assert_eq!(position_keys(&h, &bottom), 2, "pasted into a locked layer");
    // Duplicate / split / arrange with only the locked layer selected: nothing.
    let n = h.state().layers().len();
    for (m, k) in [
        (Modifiers::COMMAND, Key::D),
        (Modifiers::COMMAND | Modifiers::SHIFT, Key::D),
        (Modifiers::COMMAND | Modifiers::SHIFT, Key::CloseBracket),
    ] {
        h.state().session().lock().selected_layers = vec![bottom.clone()];
        key(&mut h, m, k);
    }
    assert_eq!(
        h.state().layers().len(),
        n,
        "duplicate / split acted on the locked layer"
    );
    assert_eq!(
        h.state().layers()[1],
        bottom,
        "arrange moved the locked layer"
    );
    // Easy Ease on the locked layer's keys: refused by the command layer.
    h.state().session().lock().selected_keys = keys;
    key(&mut h, Modifiers::NONE, Key::F9);
    let eased = h
        .state()
        .outcomes
        .iter()
        .rev()
        .find(|(id, _)| id == "key.ease")
        .map(|(_, o)| o.clone());
    assert!(
        eased.as_deref().is_none_or(|o| o.starts_with("failed")),
        "eased a locked layer's keys: {eased:?}"
    );
}

#[test]
fn viewer_clicks_go_through_a_locked_layer() {
    let mut h = gui();
    comp_with(
        &mut h,
        &["motion.layer.new.solid", "motion.layer.new.solid"],
    );
    let [top, bottom]: [PrimPath; 2] = h.state().layers().try_into().unwrap();
    let canvas = h
        .root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer");
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    click_at(&mut h, canvas.center());
    assert_eq!(
        h.state().selected_layers(),
        std::slice::from_ref(&top),
        "unlocked: the top layer is picked"
    );
    lock(&mut h, &top);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    pause(&mut h);
    click_at(&mut h, canvas.center());
    assert_eq!(
        h.state().selected_layers(),
        [bottom],
        "the click did not go through the locked layer"
    );
}

#[test]
fn locking_a_selected_layer_deselects_it() {
    // AE: a locked layer cannot be selected, so locking a selected one
    // deselects it (BAE keeps it selected and highlighted; actions skip it).
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let l = h.state().layers()[0].clone();
    assert_eq!(h.state().selected_layers(), std::slice::from_ref(&l));
    click_lock_switch(&mut h, "Dark Gray Solid 1");
    assert!(is_locked(&h, &l));
    assert!(
        !h.state().selected_layers().contains(&l),
        "the locked layer stays selected"
    );
}

#[test]
fn deleting_the_parent_of_a_locked_layer_is_not_silent() {
    // Deleting an unlocked parent whose child is locked fails with “… is
    // locked; unlock it first” in the log only: Delete looks like it does
    // nothing. AE deletes the parent and the child keeps its place; at least
    // the user must be told.
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.null", "motion.layer.new.solid"]);
    let [child, parent]: [PrimPath; 2] = h.state().layers().try_into().unwrap();
    assert_eq!(set_parent(&mut h, &child, Some(&parent)), "changed");
    lock(&mut h, &child);
    let head = find_text(&h, "Layer Name").unwrap();
    click_at(
        &mut h,
        egui::pos2(head.left() + 800.0, head.bottom() + 150.0),
    );
    h.state().session().lock().selected_layers = vec![parent.clone()];
    key(&mut h, Modifiers::NONE, Key::Delete);
    assert!(
        h.state().stage().prim(&parent).is_none(),
        "{:?}",
        h.state().outcomes.last()
    );
    assert_eq!(
        parent_of(&h, &child),
        None,
        "dangling parent on the locked child"
    );
}

// ---- Retest of #124 (dev 2c94655): refusals in the app ----

/// A panel asks to edit a locked layer (as a value scrub or keyframe click
/// would): refused, explained in the status bar, no undo step, not dirty; the
/// next change clears the notice.
#[test]
fn a_refused_edit_is_explained_in_the_status_bar() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.null"]);
    let solid = h.state().layers()[1].clone();
    lock(&mut h, &solid);
    let undo_before = h.state().doc.undo_state();
    let export_before = bae_model::export_flattened_json(h.state().stage());
    h.state_mut().invoke(Invocation::with_params(
        "attr.set_at",
        serde_json::json!({"path": solid, "attr": "transform:position", "time": 0,
                           "value": {"type": "vec3", "value": [1.0, 2.0, 0.0]}}),
    ));
    settle(&mut h);
    let notice = "Layer “Dark Gray Solid 1” is locked. Unlock it first.";
    assert!(
        find_text(&h, notice).is_some(),
        "no status bar notice: {:?}",
        painted_texts(&h)
    );
    assert_eq!(
        h.state().doc.undo_state(),
        undo_before,
        "a refused edit became an undo step"
    );
    assert_eq!(
        bae_model::export_flattened_json(h.state().stage()),
        export_before
    );
    // A successful change clears it.
    new_solid(&mut h);
    assert!(
        find_text(&h, notice).is_none(),
        "the notice stayed after a change"
    );
}

#[test]
fn the_lock_notice_is_chinese_in_the_chinese_ui() {
    if !in_child(
        "acceptance::layers::the_lock_notice_is_chinese_in_the_chinese_ui",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let solid = h.state().layers()[0].clone();
    lock(&mut h, &solid);
    h.state_mut().invoke(Invocation::with_params(
        "layer.delete",
        serde_json::json!({"layers": [solid]}),
    ));
    settle(&mut h);
    let name = h.state().name(&solid);
    let notice = format!("图层“{name}”已锁定，请先解锁。");
    assert!(
        find_text(&h, &notice).is_some(),
        "no Chinese notice {notice:?}: {:?}",
        painted_texts(&h)
    );
    assert!(h.state().stage().prim(&solid).is_some());
}

#[test]
fn undoing_a_parent_delete_reparents_the_locked_child() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.null", "motion.layer.new.solid"]);
    let [child, parent]: [PrimPath; 2] = h.state().layers().try_into().unwrap();
    assert_eq!(set_parent(&mut h, &child, Some(&parent)), "changed");
    lock(&mut h, &child);
    let parent_before = parent_of(&h, &child);
    assert!(parent_before.is_some());
    let head = find_text(&h, "Layer Name").unwrap();
    click_at(
        &mut h,
        egui::pos2(head.left() + 800.0, head.bottom() + 150.0),
    );
    h.state().session().lock().selected_layers = vec![parent.clone()];
    key(&mut h, Modifiers::NONE, Key::Delete);
    assert!(h.state().stage().prim(&parent).is_none());
    assert_eq!(parent_of(&h, &child), None);
    // One undo brings the parent back and the locked child parented again.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(
        h.state().stage().prim(&parent).is_some(),
        "undo did not restore the parent"
    );
    assert_eq!(
        parent_of(&h, &child),
        parent_before,
        "undo did not re-parent the locked child"
    );
    assert!(is_locked(&h, &child));
    // Redo deletes it again.
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert!(h.state().stage().prim(&parent).is_none());
    assert_eq!(parent_of(&h, &child), None);
}
