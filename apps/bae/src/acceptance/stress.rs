//! Stability: a deep undo stack, the same command many times in one frame,
//! many layers in the timeline.

use std::time::Instant;

use egui::{Key, Modifiers};

use super::*;

#[test]
fn deep_undo_stack_round_trips() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let start = bae_model::export_flattened_json(h.state().stage());
    // 1 200 edits: renames (each one undo step) queued as the timeline does.
    // The stage keeps 1 000 undo steps (`UNDO_LIMIT`; AE allows at most 99).
    let n = 1200;
    let t = Instant::now();
    for i in 0..n {
        h.state_mut().invoke(Invocation::with_params(
            "attr.set",
            serde_json::json!({"path": layer, "attr": "name",
                               "value": {"type": "string", "value": format!("名字 {i}")}}),
        ));
        if i % 100 == 0 {
            frame(&mut h);
        }
    }
    settle(&mut h);
    let edit_time = t.elapsed();
    assert_eq!(h.state().name(&layer), format!("名字 {}", n - 1));
    let end = bae_model::export_flattened_json(h.state().stage());
    // Cmd+Z until nothing is left: exactly 1 000 steps back.
    let t = Instant::now();
    for _ in 0..n {
        press(&mut h, Modifiers::COMMAND, Key::Z);
    }
    settle(&mut h);
    let undo_time = t.elapsed();
    assert_eq!(h.state().name(&layer), format!("名字 {}", n - 1 - 1000));
    assert_ne!(bae_model::export_flattened_json(h.state().stage()), start);
    let t = Instant::now();
    for _ in 0..1000 {
        press(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    }
    settle(&mut h);
    let redo_time = t.elapsed();
    assert_eq!(bae_model::export_flattened_json(h.state().stage()), end);
    println!("deep undo: {n} edits {edit_time:?}, undo {undo_time:?}, redo {redo_time:?}");
}

#[test]
fn rapid_repeated_commands() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    // 100 Cmd+Y presses in a row: 100 solids, unique names.
    for _ in 0..100 {
        press(&mut h, Modifiers::COMMAND, Key::Y);
    }
    settle(&mut h);
    let names = h.state().layer_names();
    assert_eq!(names.len(), 100, "{names:?}");
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 100, "duplicate layer names");
    // Duplicate 50 times, split 20 times, select all, delete.
    for _ in 0..50 {
        press(&mut h, Modifiers::COMMAND, Key::D);
    }
    settle(&mut h);
    assert_eq!(h.state().layers().len(), 150);
    for _ in 0..20 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
        press(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    }
    settle(&mut h);
    let after_split = h.state().layers().len();
    assert!(after_split > 150, "{after_split}");
    key(&mut h, Modifiers::COMMAND, Key::A);
    // The timeline must be focused for Delete to remove layers.
    let head = find_text(&h, "Layer Name").unwrap();
    click_at(
        &mut h,
        egui::pos2(head.left() + 800.0, head.bottom() + 60.0),
    );
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::NONE, Key::Delete);
    assert!(
        h.state().layers().is_empty(),
        "{} left",
        h.state().layers().len()
    );
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(h.state().layers().len(), after_split);
    // Repeated splits after the first fail while #108 keeps the first half
    // selected ("no layer is active at the split time"); nothing else may fail.
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(id, o)| o.starts_with("failed") && id != "layer.split")
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn frame_time_with_many_layers() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    for _ in 0..200 {
        press(&mut h, Modifiers::COMMAND, Key::Y);
    }
    settle(&mut h);
    assert_eq!(h.state().layers().len(), 200);
    key(&mut h, Modifiers::COMMAND, Key::A);
    let t = Instant::now();
    let frames = 30;
    for _ in 0..frames {
        frame(&mut h);
    }
    let per = t.elapsed() / frames;
    println!("UI frame with 200 layers (debug build, no GPU): {per:?}");
}

#[test]
fn key_presses_piled_up_in_one_frame() {
    // A slow frame (heavy comp, debug build) collects several key events; each
    // still runs once, against the project as the previous one left it.
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    let mut events = Vec::new();
    for _ in 0..5 {
        events.extend(key_events(Modifiers::COMMAND, Key::Y));
    }
    send_batch(&mut h, events);
    settle(&mut h);
    let names = h.state().layer_names();
    assert_eq!(names.len(), 5, "{names:?}");
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    println!("five Cmd+Y in one frame: {names:?}");
    assert_eq!(
        unique.len(),
        5,
        "same-frame presses made layers with the same name: {names:?}"
    );
}
