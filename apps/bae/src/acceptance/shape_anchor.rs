//! Shape creation and transform pivots through the Windows and macOS workbench.

use super::*;
use bae_motion::query;
use egui::vec2;
use egui_kittest::kittest::Queryable as _;

/// The Composition viewer's image area.
fn canvas(h: &Ui) -> Rect {
    h.root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer")
}

/// Issues #572, #577 and #581: draw two rectangles, then use the real Y, V and W
/// tools. The preference determines the pivot when the first rectangle is drawn.
#[test]
fn shape_anchor_preference_sets_the_pivot_for_all_three_tools() {
    use bae_anim::glam::DVec3;

    for centered in [false, true] {
        let mut h = gui();
        comp_with(&mut h, &[]);
        let comp = h.state().active_comp().unwrap();
        let none = Modifiers::NONE;
        assert!(
            !h.state()
                .prefs
                .current()
                .bool(bae_prefs::keys::CENTER_SHAPE_ANCHOR)
        );
        if centered {
            h.state_mut().dispatch(Invocation::with_params(
                bae_ui::settings::ACTION,
                serde_json::json!({"page": "composition"}),
            ));
            settle(&mut h);
            h.get_by_label("Center Anchor Point in New Shape Layers")
                .click();
            settle(&mut h);
            h.get_by_label("OK").click();
            settle(&mut h);
        }

        let c = canvas(&h).center();
        let first = Rect::from_center_size(c - vec2(62.0, 38.0), vec2(72.0, 50.0));
        let second = Rect::from_center_size(c + vec2(64.0, 45.0), vec2(60.0, 44.0));
        key(&mut h, none, Key::Q);
        drag(&mut h, first.min, first.max, none);
        let layer = h.state().layers()[0].clone();
        let evaluated = |h: &Ui| {
            bae_anim::evaluate_comp(h.state().stage(), &comp, bae_model::Time::ZERO).unwrap()
        };
        let initial = evaluated(&h);
        let before = initial.layer(&layer).unwrap().transform;
        assert_eq!(before.anchor_point, DVec3::ZERO);
        let info = query::comp_info(h.state().stage(), &comp).unwrap();
        let comp_center = DVec3::new(
            f64::from(info.width) / 2.0,
            f64::from(info.height) / 2.0,
            0.0,
        );
        assert_eq!(before.position == comp_center, !centered);
        let bounds = bae_vector::shape_layer_scene(initial.layer(&layer).unwrap(), &mut Vec::new())
            .bounds(false)
            .unwrap();
        let zoom = f64::from(first.width()) / bounds.width();
        let pivot = first.center()
            - vec2(
                (bounds.center().x * zoom) as f32,
                (bounds.center().y * zoom) as f32,
            );
        if centered {
            assert!(pivot.distance(first.center()) < 0.1);
        }
        drag(&mut h, second.min, second.max, none);
        assert_eq!(h.state().layers().as_slice(), std::slice::from_ref(&layer));
        let read = |h: &Ui| evaluated(h).layer(&layer).unwrap().transform;
        assert_eq!(read(&h), before);

        // Y: both properties move by the drag distance. Every content point stays still.
        key(&mut h, none, Key::Y);
        drag(&mut h, pivot, pivot + vec2(26.0, 13.0), none);
        let moved = read(&h);
        let expected = DVec3::new(26.0 / zoom, 13.0 / zoom, 0.0);
        assert!(
            (moved.anchor_point - before.anchor_point - expected).length() < 0.1,
            "{moved:?}"
        );
        assert!((moved.position - before.position - expected).length() < 0.1);
        assert!(moved.matrix(false).abs_diff_eq(before.matrix(false), 0.001));
        key(&mut h, Modifiers::COMMAND, Key::Z);

        // V: the corner scales both axes, the right midpoint only X. Each axis
        // uses the distance from the chosen pivot to the grabbed handle.
        key(&mut h, none, Key::V);
        let union = first.union(second);
        for (handle, delta) in [
            (union.max, vec2(24.0, 18.0)),
            (union.right_center(), vec2(24.0, 0.0)),
        ] {
            drag(&mut h, handle, handle + delta, none);
            let scaled = read(&h);
            let span = handle - pivot;
            let sx = 100.0 * (1.0 + f64::from(delta.x / span.x));
            let sy = if delta.y == 0.0 {
                100.0
            } else {
                100.0 * (1.0 + f64::from(delta.y / span.y))
            };
            assert!(
                (scaled.scale.x - sx).abs() < 0.2 && (scaled.scale.y - sy).abs() < 0.2,
                "{scaled:?} expected {sx}, {sy}"
            );
            assert_eq!(scaled.position, before.position);
            assert_eq!(scaled.anchor_point, before.anchor_point);
            key(&mut h, Modifiers::COMMAND, Key::Z);
        }

        // W: rotate -35 degrees around the same point, without changing Position.
        key(&mut h, none, Key::W);
        let from = second.center();
        let to = pivot + egui::emath::Rot2::from_angle((-35_f32).to_radians()) * (from - pivot);
        drag(&mut h, from, to, none);
        let rotated = read(&h);
        assert!((rotated.rotation.z + 35.0).abs() < 0.1, "{rotated:?}");
        assert_eq!(rotated.position, before.position);
        assert_eq!(rotated.anchor_point, before.anchor_point);
    }
}

#[test]
fn shape_anchor_preference_follows_the_first_pen_path() {
    let mut h = gui();
    comp_with(&mut h, &[]);
    let mut p = (*h.state().prefs.current()).clone();
    p.set(bae_prefs::keys::CENTER_SHAPE_ANCHOR, true);
    h.state().prefs.set(p).unwrap();
    let comp = h.state().active_comp().unwrap();
    let c = canvas(&h).center();
    let a = c - vec2(94.0, 38.0);
    let b = c - vec2(47.0, 72.0);
    let d = c + vec2(22.0, 19.0);
    key(&mut h, Modifiers::NONE, Key::G);
    for vertex in [a, b, d, a] {
        click_at(&mut h, vertex);
    }
    let e = bae_anim::evaluate_comp(h.state().stage(), &comp, bae_model::Time::ZERO).unwrap();
    let layer = &e.layers[0];
    let bounds = bae_vector::shape_layer_scene(layer, &mut Vec::new())
        .bounds(false)
        .unwrap();
    let center = bounds.center();
    let t = layer.transform;
    assert!(
        (t.anchor_point.x - center.x).abs() < 0.01 && (t.anchor_point.y - center.y).abs() < 0.01
    );
    assert!((t.position.x - center.x - f64::from(e.width) / 2.0).abs() < 0.01);
    assert!((t.position.y - center.y - f64::from(e.height) / 2.0).abs() < 0.01);
}
