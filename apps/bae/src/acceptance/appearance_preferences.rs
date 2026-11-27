use super::*;
use egui_kittest::kittest::Queryable as _;

fn set_pref(h: &mut Ui, key: &str, on: bool) {
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set(key, on);
    h.state().prefs.set(prefs).unwrap();
    settle(h);
}

#[test]
fn time_display_respects_project_units_and_alternate_line() {
    let mut h = gui();
    new_comp(&mut h);
    let comp = h.state().active_comp().unwrap();
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    let start = info.frame_rate.frame_to_time(123);
    h.state_mut().dispatch(Invocation::with_params(
        "comp.set_settings",
        serde_json::json!({"comp":comp, "start_timecode":start}),
    ));
    let fps = format!(" ({:.2} fps)", info.frame_rate.as_f64());
    let tc = bae_ui::widgets::timecode(123, info.frame_rate.as_f64());
    for (mode, primary, alternate) in [
        ("timecode", tc.as_str(), "00123"),
        ("frames", "00123", tc.as_str()),
    ] {
        h.state_mut().dispatch(Invocation::with_params(
            "project.set_settings",
            serde_json::json!({"time_display": mode}),
        ));
        for show in [true, false] {
            set_pref(
                &mut h,
                "appearance.show_both_timecode_and_frames_in_timeline",
                show,
            );
            let header = h.get_by_label("Current Time").rect();
            assert!(
                find_texts(&h, |t| t == primary)
                    .iter()
                    .any(|r| header.contains(r.center()))
            );
            assert_eq!(find_text(&h, &format!("{alternate}{fps}")).is_some(), show);
        }
    }
}

#[test]
fn viewer_tab_swatches_and_hover_tips_follow_appearance() {
    let mut h = gui();
    new_comp(&mut h);
    let comp = h.state().active_comp().unwrap();
    let name = bae_motion::query::display_name_of(h.state().stage(), &comp);
    h.state_mut().dispatch(Invocation::with_params(
        "attr.set",
        serde_json::json!({"path":comp,"attr":"label","value":{"type":"int","value":12}}),
    ));
    for on in [true, false] {
        set_pref(&mut h, "appearance.use_label_color_for_related_tabs", on);
        for title in ["Composition", "Timeline"] {
            let tab = h
                .get_by_role_and_label(egui::accesskit::Role::Button, &format!("{title} {name}"))
                .rect();
            assert_eq!(h.output().shapes.iter().any(|s| {
                matches!(&s.shape, egui::Shape::Rect(r) if tab.contains(r.rect.center()) && r.fill == bae_ui::theme::label_color(12))
            }), on, "{title} swatch");
        }
    }
    let tip = "Enables Motion Blur for all layers with the Motion Blur switch set";
    for on in [false, true] {
        set_pref(&mut h, "appearance.show_tool_tips", on);
        let button = h.get_by_label("Enable Motion Blur").rect();
        send(&mut h, Event::PointerMoved(button.center()));
        for _ in 0..60 {
            frame(&mut h);
        }
        assert_eq!(find_text(&h, tip).is_some(), on, "hover tip");
    }
}
