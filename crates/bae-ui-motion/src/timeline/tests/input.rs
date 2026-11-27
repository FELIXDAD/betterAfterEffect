//! Mouse input over the timeline (0.2.0): Alt + wheel zooms time around the
//! pointer, a middle-button drag pans, marquees select layers and keyframes
//! with AE's Shift (add) and Cmd/Ctrl (toggle).

use super::*;

fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Modifiers::NONE
    }
}

fn wheel(lines: f32, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, lines),
        phase: egui::TouchPhase::Move,
        modifiers,
    }
}

fn middle(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Middle,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn alt_wheel_zooms_time_around_the_pointer_and_a_middle_drag_pans() {
    let mut h = H::new();
    let mut tl = h.bars();
    let span = |tl: &TimelinePanel| tl.last_axis.unwrap().span;
    let full = span(&tl);
    let y = bar_rect(&tl, &h.layer.clone()).center().y;
    let at = pos2(x_of(&tl, 5.0), y);
    // A plain wheel turn does not zoom.
    h.frame(&mut tl, vec![Event::PointerMoved(at)], Modifiers::NONE);
    h.frame(&mut tl, vec![wheel(2.0, Modifiers::NONE)], Modifiers::NONE);
    assert_eq!(span(&tl), full);
    // Alt + wheel up zooms in, keeping 5 s under the pointer.
    h.frame(&mut tl, vec![wheel(3.0, alt())], alt());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(span(&tl) < full, "{:?} !< {full:?}", span(&tl));
    assert!((x_of(&tl, 5.0) - at.x).abs() < 1.5, "anchor moved");
    // Down zooms back out.
    h.frame(&mut tl, vec![wheel(-30.0, alt())], alt());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(span(&tl), full);
    // Zoomed in, a middle drag to the right shows earlier time.
    h.frame(&mut tl, vec![wheel(6.0, alt())], alt());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let start = tl.last_axis.unwrap().start;
    let to = at + vec2(120.0, 0.0);
    h.frame(&mut tl, vec![middle(at, true)], Modifiers::NONE);
    h.frame(&mut tl, vec![Event::PointerMoved(to)], Modifiers::NONE);
    h.frame(&mut tl, vec![middle(to, false)], Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(
        tl.last_axis.unwrap().start < start,
        "{:?} !< {start:?}",
        tl.last_axis.unwrap().start
    );
    // And up and down through many layers.
    let comp = h.comp.clone();
    for _ in 0..60 {
        h.run("layer.create", json!({"comp": comp, "kind": "solid"}));
    }
    for _ in 0..30 {
        h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    }
    let before = tl.rows_offset;
    let from = pos2(at.x, 400.0);
    let up = from - vec2(0.0, 150.0);
    h.frame(&mut tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
    h.frame(&mut tl, vec![middle(from, true)], Modifiers::NONE);
    h.frame(&mut tl, vec![Event::PointerMoved(up)], Modifiers::NONE);
    h.frame(&mut tl, vec![middle(up, false)], Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(
        (tl.rows_offset - before - 150.0).abs() < 2.0,
        "rows from {before} to {}",
        tl.rows_offset
    );
}

#[test]
fn marquees_select_layers_from_the_empty_outline_and_keys_in_the_graph() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let first = h.layer.clone();
    let second = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let third = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let (t0, t1) = (1.0, 3.0);
    h.keys(POS, &[(t0, vec3(960.0, 540.0)), (t1, vec3(1500.0, 540.0))]);
    let mut tl = h.bars();
    h.session().lock().selected_layers.clear();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let y = |tl: &TimelinePanel, l: &PrimPath| bar_rect(tl, l).center().y;
    let bottom = [&first, &second, &third]
        .iter()
        .map(|l| bar_rect(&tl, l).bottom())
        .fold(0.0_f32, f32::max);
    // From the empty outline below the layers up to the middle one: the
    // bottom two layers (the newest is on top).
    let below = pos2(60.0, bottom + 60.0);
    let sel = |h: &H| {
        let mut s = h.session().lock().selected_layers.clone();
        s.sort();
        s
    };
    let sorted = |mut v: Vec<PrimPath>| {
        v.sort();
        v
    };
    let lowest = |tl: &TimelinePanel| {
        let mut rows = [&first, &second, &third].map(|l| (y(tl, l), (*l).clone()));
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        rows
    };
    let rows = lowest(&tl);
    let r = h.drag(&mut tl, below, pos2(60.0, rows[1].0), Modifiers::NONE);
    h.apply(&r);
    assert_eq!(sel(&h), sorted(vec![rows[1].1.clone(), rows[2].1.clone()]));
    // Cmd/Ctrl toggles: the bottom one goes, the rest stays.
    let r = h.drag(&mut tl, below, pos2(60.0, rows[2].0), cmd());
    h.apply(&r);
    assert_eq!(sel(&h), vec![rows[1].1.clone()]);
    // Shift adds: all three.
    let r = h.drag(
        &mut tl,
        below,
        pos2(60.0, rows[0].0),
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
    h.apply(&r);
    assert_eq!(
        sel(&h),
        sorted(vec![first.clone(), second.clone(), third.clone()])
    );
    // Keys in the time graph: replace, Shift adds, Cmd/Ctrl toggles.
    h.session().lock().selected_layers = vec![first.clone()];
    h.session()
        .lock()
        .reveal
        .insert(first.clone(), crate::session::Reveal::Animated);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let keys = [h.key(POS, t0), h.key(POS, t1)];
    let around = |tl: &TimelinePanel, k: &KeyRef| {
        let p = bar_key(tl, k);
        (p - vec2(10.0, 6.0), p + vec2(10.0, 6.0))
    };
    let keys_sel = |h: &H| h.session().lock().selected_keys.clone();
    let (a, b) = around(&tl, &keys[0]);
    let r = h.drag(&mut tl, a, b, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(keys_sel(&h), vec![keys[0].clone()]);
    let (a, b) = around(&tl, &keys[1]);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let r = h.drag(&mut tl, a, b, shift);
    h.apply(&r);
    assert_eq!(keys_sel(&h), vec![keys[0].clone(), keys[1].clone()]);
    let (a, b) = around(&tl, &keys[0]);
    let r = h.drag(&mut tl, a, b, cmd());
    h.apply(&r);
    assert_eq!(keys_sel(&h), vec![keys[1].clone()]);
}
