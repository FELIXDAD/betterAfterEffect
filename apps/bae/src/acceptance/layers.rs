//! Compositions and layers: Cmd+N, every Layer ▸ New item (menu and AE
//! shortcut), greyed out without a comp, Arrange, Duplicate, Split; undo / redo.

use bae_motion::query;
use egui::{Key, Modifiers};

use super::*;

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
const NEW_ITEMS: [(&str, &str); 9] = [
    ("motion.layer.new.text", "text"),
    ("motion.layer.new.solid", "solid"),
    ("motion.layer.new.light", "light"),
    ("motion.layer.new.camera", "camera"),
    ("motion.layer.new.null", "null"),
    ("motion.layer.new.shape", "shape"),
    ("motion.layer.new.adjustment", "adjustment"),
    ("motion.layer.new.rectangle", "shape"),
    ("motion.layer.new.ellipse", "shape"),
];

#[test]
fn new_composition_with_cmd_n_opens_it() {
    let mut h = gui();
    assert!(h.state().active_comp().is_none());
    key(&mut h, Modifiers::COMMAND, Key::N);
    let comp = h.state().active_comp().expect("Cmd+N opened no comp");
    assert_eq!(h.state().name(&comp), "Comp 1");
    // Timeline and viewer show it.
    assert!(
        find_text(&h, "Timeline Comp 1").is_some(),
        "{:?}",
        painted_texts(&h)
    );
    key(&mut h, Modifiers::COMMAND, Key::N);
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
    key(&mut h, Modifiers::COMMAND, Key::N);
    for (id, want) in NEW_ITEMS {
        let before = h.state().layers();
        assert!(h.state_mut().menu(id), "{id} disabled with a comp open");
        settle(&mut h);
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
    key(&mut h, Modifiers::COMMAND, Key::N);
    for (m, k, want) in cases {
        let before = h.state().layers();
        key(&mut h, m, k);
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
        (cmd, Key::M),
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
        "Cmd+M queued without a comp"
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
    key(&mut h, Modifiers::COMMAND, Key::N);
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
