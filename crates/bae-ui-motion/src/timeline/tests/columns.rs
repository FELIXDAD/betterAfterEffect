//! Columns.

use super::*;

#[test]
fn saved_pane_switches_survive_column_layout_upgrade() {
    use crate::timeline::Column::{Modes, Parent, Switches};
    let old = json!({"switches": false, "modes": true, "parent": false, "zoom": 0.75});
    let upgraded = TimelinePanel::from_state(Some(&old));
    let visibility = |tl: &TimelinePanel| [Switches, Modes, Parent].map(|c| tl.column_visible(c));
    assert_eq!(visibility(&upgraded), [false, true, false]);
    let saved = upgraded.save_state().unwrap();
    assert_eq!(saved["zoom"], 0.75);
    assert!(saved.get("switches").is_none());
    let mut mixed = json!({"switches": true, "modes": false, "parent": true, "zoom": 0.75});
    mixed["columns"] = saved["columns"].clone();
    let restored = TimelinePanel::from_state(Some(&mixed));
    assert_eq!(visibility(&restored), visibility(&upgraded));
    assert_eq!(restored.save_state(), Some(saved));
}

#[test]
fn resizing_the_outline_preserves_rename_focus_and_only_requests_visible_timings() {
    use crate::timeline::Column;
    let mut h = H::new();
    let layer = h.layer.clone();
    let name = query::display_name_of(&h.stage, &layer);
    let mut state = TimelinePanel::default().save_state().unwrap();
    state["columns"]["layer_names"] = json!(true);
    for column in [Column::Stretch, Column::RenderTime] {
        state["columns"]["shown"][column as usize] = json!(true);
    }
    let mut tl = TimelinePanel::from_state(Some(&state));
    h.painted_text = Some(Vec::new());
    h.session()
        .lock()
        .layer_times
        .insert(layer.clone(), std::time::Duration::from_micros(1250));
    h.frame(&mut tl, vec![], Modifiers::NONE);
    let painted = |h: &H, text: &str| {
        h.painted_text
            .as_ref()
            .unwrap()
            .iter()
            .any(|(s, _)| s == text)
    };
    assert!(painted(&h, &format!("[{name}]")));
    assert!(painted(&h, "100.0%"));
    assert!(painted(&h, &format!("1.25 {}", bae_ui::tr("ms"))));
    assert!(h.session().lock().timing_column_frame.is_some());

    tl.start_rename(layer.clone(), &name);
    h.frame(&mut tl, vec![], Modifiers::NONE);
    let focus = h.ctx.memory(|m| m.focused()).expect("rename field");
    h.width = 540.0;
    h.frame(&mut tl, vec![], Modifiers::NONE);
    assert_eq!(h.ctx.memory(|m| m.focused()), Some(focus));
    assert!(h.session().lock().timing_column_frame.is_none());
    h.width = 1280.0;
    h.frame(&mut tl, vec![Event::Text("Title".into())], Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    h.apply(&r);
    h.frame(&mut tl, vec![], Modifiers::NONE);
    assert!(painted(&h, "Title"));
    assert_eq!(query::display_name_of(&h.stage, &layer), "Title");
}

#[test]
fn column_menu_shows_the_actual_out_frame_and_header_drag_changes_layout() {
    let mut h = H::new();
    let layer = h.layer.clone();
    h.run(
        "layer.trim",
        json!({"layers":[layer],"out_point":Time::from_seconds(3.0)}),
    );
    let mut tl = h.bars();
    h.painted_text = Some(Vec::new());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let locate = |h: &H, label: &str| {
        h.painted_text
            .as_ref()
            .unwrap()
            .iter()
            .rev()
            .find(|(s, _)| s == label)
            .unwrap_or_else(|| panic!("missing {label} in {:?}", h.painted_text))
            .1
            .center()
    };
    let header = locate(&h, bae_ui::tr("Source Name"));
    let right = |down| Event::PointerButton {
        pos: header,
        button: PointerButton::Secondary,
        pressed: down,
        modifiers: Modifiers::NONE,
    };
    h.frame(
        &mut tl,
        vec![Event::PointerMoved(header), right(true)],
        Modifiers::NONE,
    );
    h.frame(&mut tl, vec![right(false)], Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let menu = locate(&h, bae_ui::tr("Columns"));
    h.frame(&mut tl, vec![Event::PointerMoved(menu)], Modifiers::NONE);
    for _ in 0..22 {
        h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    }
    let out = locate(&h, bae_ui::tr("Out"));
    h.click(&mut tl, out, Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.column_visible(crate::timeline::Column::Out));
    let cell = locate(&h, "0:00:02:29");
    let prev = tl.last_axis.unwrap().left;
    let divider = pos2(prev - 2.0, header.y);
    h.drag(&mut tl, divider, divider + vec2(36.0, 0.0), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!((tl.last_axis.unwrap().left - prev - 36.0).abs() < 1.0);
    assert!((locate(&h, "0:00:02:29").x - cell.x).abs() < 1.0);
    // Narrow just the outline, then move its scrollbar: the ruler and bars stay put.
    let boundary = tl.last_axis.unwrap().left;
    let split = pos2(boundary - 1.0, 340.0);
    h.drag(&mut tl, split, split - vec2(190.0, 0.0), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let axis = tl.last_axis.unwrap();
    assert!((axis.left - boundary + 190.0).abs() < 1.0);
    let bar = tl.last_bars.clone();
    let before = locate(&h, bae_ui::tr("Source Name")).x;
    let thumb = pos2(36.0, 796.0);
    h.drag(&mut tl, thumb, thumb + vec2(52.0, 0.0), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(tl.last_axis.unwrap().left, axis.left);
    assert_eq!(tl.last_bars, bar);
    assert!(locate(&h, bae_ui::tr("Source Name")).x < before - 40.0);
    let saved = tl.save_state().unwrap();
    let restored = TimelinePanel::from_state(Some(&saved));
    assert_eq!(restored.save_state(), Some(saved));
}
