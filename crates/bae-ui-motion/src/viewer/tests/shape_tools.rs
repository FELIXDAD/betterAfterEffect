//! Shape tools: rectangle, rounded rectangle, ellipse, polygon, star, Bezier options, double-click, masks.

use super::*;

#[test]
fn rectangle_without_a_selection_makes_a_selected_shape_layer() {
    let mut h = H::new();
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "shape.add");
    assert_eq!(params["name"], "Shape Layer 1");
    h.apply(&r);
    let layers = query::comp_layers(&h.stage, &h.comp);
    assert_eq!(layers.len(), 1);
    let layer = layers[0].clone();
    assert_eq!(h.name(&layer), "Shape Layer 1");
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    // Rectangle 1: Rectangle Path 1, Stroke 1, Fill 1; the group sits at the drag
    // center relative to the comp center (the layer's origin).
    let group = h.contents(&layer)[0].clone();
    assert_eq!(h.name(&group), "Rectangle 1");
    let items: Vec<String> = h.contents(&group).iter().map(|p| h.name(p)).collect();
    assert_eq!(items, ["Rectangle Path 1", "Stroke 1", "Fill 1"]);
    assert!(close(
        vec2v(h.value(&group, "transform:position")),
        [-120.0, -80.0]
    ));
    let rect = h.contents(&group)[0].clone();
    assert!(close(vec2v(h.value(&rect, "size")), [200.0, 100.0]));
    // Fill and stroke from the tool options.
    let fill = h.contents(&group)[2].clone();
    assert_eq!(
        h.value(&fill, "color"),
        Some(Value::Color([1.0, 0.0, 0.0, 1.0]))
    );
    let stroke = h.contents(&group)[1].clone();
    assert_eq!(float(h.value(&stroke, "width")), 2.0);
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    assert!(query::comp_layers(&h.stage, &h.comp).is_empty());
}

#[test]
fn every_shape_tool_on_a_selected_shape_layer_adds_a_group() {
    let mut h = H::new();
    let shape = h.layer("shape");
    h.select(&[&shape]);
    let tools = [
        (Tool::Rectangle, "Rectangle 1"),
        (Tool::RoundedRectangle, "Rectangle 2"),
        (Tool::Ellipse, "Ellipse 1"),
        (Tool::Polygon, "Polystar 1"),
        (Tool::Star, "Polystar 2"),
    ];
    for (tool, name) in tools {
        let r = h.draw(tool, (200.0, 100.0), (260.0, 150.0));
        assert_eq!(one(&r, "shape.add")["layer"], json!(shape), "{tool:?}");
        h.apply(&r);
        let top = h.contents(&shape)[0].clone();
        assert_eq!(h.name(&top), name, "{tool:?}: new groups go on top");
    }
    assert_eq!(
        query::comp_layers(&h.stage, &h.comp),
        std::slice::from_ref(&shape)
    );
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&shape)
    );
    let rounded = h.contents(&shape)[3].clone();
    let path = h.contents(&rounded)[0].clone();
    assert_eq!(float(h.value(&path, "roundness")), 10.0, "tool roundness");
    // Undo takes back one group.
    h.stage.undo(&h.author).unwrap();
    assert_eq!(h.contents(&shape).len(), 4);
}

#[test]
fn shape_tools_on_other_layers_or_with_tool_creates_mask_make_masks() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "mask.create");
    assert_eq!(params["layer"], json!(solid));
    assert!(params.get("name").is_none(), "the command names it");
    h.apply(&r);
    let masks = h.children(&solid.child("masks").unwrap());
    assert_eq!(masks.len(), 1);
    assert_eq!(h.name(&masks[0]), "Mask 1");
    assert_eq!(h.value(&masks[0], "mode"), Some(Value::Enum("add".into())));
    // Layer space of a comp-sized solid is comp space.
    let Some(Value::Bezier(b)) = h.value(&masks[0], "path") else {
        panic!("mask path");
    };
    let (lo, hi) = bounds(&b);
    assert!(
        close(lo, [100.0, 50.0]) && close(hi, [300.0, 150.0]),
        "{lo:?} {hi:?}"
    );
    // The next one is Mask 2; every shape tool draws masks.
    for tool in [Tool::Ellipse, Tool::Star] {
        let r = h.draw(tool, (320.0, 180.0), (360.0, 220.0));
        one(&r, "mask.create");
        h.apply(&r);
    }
    let names: Vec<String> = h
        .children(&solid.child("masks").unwrap())
        .iter()
        .map(|m| h.name(m))
        .collect();
    assert_eq!(names, ["Mask 1", "Mask 2", "Mask 3"]);
    h.stage.undo(&h.author).unwrap();
    assert_eq!(h.children(&solid.child("masks").unwrap()).len(), 2);

    // A shape layer with Tool Creates Mask.
    let shape = h.layer("shape");
    h.select(&[&shape]);
    h.tools().lock().options.creates_mask = true;
    let r = h.draw(Tool::Ellipse, (100.0, 50.0), (300.0, 150.0));
    assert_eq!(one(&r, "mask.create")["layer"], json!(shape));
    h.apply(&r);
    assert!(h.contents(&shape).is_empty(), "no group");

    // A null cannot take a mask: a new shape layer instead.
    h.tools().lock().options.creates_mask = false;
    let null = h.layer("null");
    h.select(&[&null]);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "shape.add");
    assert!(params.get("layer").is_none() && params["comp"] == json!(h.comp));

    // Several layers selected: a new shape layer above the topmost (AE 26).
    h.select(&[&solid, &shape]);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "shape.add");
    assert!(params.get("layer").is_none(), "{params}");
    let order = query::comp_layers(&h.stage, &h.comp);
    let top = order
        .iter()
        .position(|l| *l == solid || *l == shape)
        .unwrap();
    assert_eq!(params["index"], top);
}

#[test]
fn shift_constrains_and_command_draws_from_the_center() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let mask_bounds = |h: &mut H, r: &PanelRequests| {
        h.apply(r);
        let masks = h.children(&solid.child("masks").unwrap());
        let Some(Value::Bezier(b)) = h.value(masks.last().unwrap(), "path") else {
            panic!("mask path");
        };
        bounds(&b)
    };
    // Shift: a square, the larger side wins.
    h.tools().lock().set(Tool::Rectangle);
    h.drag_to((100.0, 100.0), (300.0, 160.0), Modifiers::SHIFT);
    let r = h.release((300.0, 160.0), Modifiers::SHIFT);
    let (lo, hi) = mask_bounds(&mut h, &r);
    assert!(
        close(lo, [100.0, 100.0]) && close(hi, [300.0, 300.0]),
        "{lo:?} {hi:?}"
    );
    // Cmd/Ctrl: from the center outward.
    h.drag_to((200.0, 100.0), (260.0, 140.0), Modifiers::COMMAND);
    let r = h.release((260.0, 140.0), Modifiers::COMMAND);
    let (lo, hi) = mask_bounds(&mut h, &r);
    assert!(
        close(lo, [140.0, 60.0]) && close(hi, [260.0, 140.0]),
        "{lo:?} {hi:?}"
    );
    // Polygon with Shift: no rotation.
    h.tools().lock().set(Tool::Polygon);
    h.select(&[]);
    h.drag_to((320.0, 180.0), (400.0, 200.0), Modifiers::SHIFT);
    let r = h.release((400.0, 200.0), Modifiers::SHIFT);
    let p = one(&r, "shape.add");
    assert_eq!(p["rotation"], 0.0);
}

#[test]
fn space_moves_the_shape_and_esc_cancels() {
    let mut h = H::new();
    h.tools().lock().set(Tool::Ellipse);
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    // Space held: Space and Page Up go to the drag, not playback / frame steps.
    assert!(h.action("motion.play"));
    assert!(!h.session().lock().is_playing());
    let space = |pressed| Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let a = h.screen(300.0, 150.0);
    h.move_to(a, Modifiers::NONE, vec![space(true)]);
    for i in 1..=5 {
        let p = a + vec2(10.0, 4.0) * i as f32;
        h.move_to(p, Modifiers::NONE, Vec::new());
    }
    h.move_to(a + vec2(50.0, 20.0), Modifiers::NONE, vec![space(false)]);
    let r = h.release((350.0, 170.0), Modifiers::NONE);
    let p = one(&r, "shape.add");
    // Same size, moved by (50, 20): center (250, 120) - comp center (320, 180).
    assert!(close(
        [
            p["size"][0].as_f64().unwrap(),
            p["size"][1].as_f64().unwrap()
        ],
        [200.0, 100.0]
    ));
    assert!(close(
        [
            p["position"][0].as_f64().unwrap(),
            p["position"][1].as_f64().unwrap()
        ],
        [-70.0, -60.0]
    ));
    // Esc cancels a drag.
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    h.key(Key::Escape);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    // After the drag, actions reach the viewer's normal handling again.
    assert!(h.viewer.shape.is_none());
}

#[test]
fn arrows_change_roundness_points_and_roundness_of_polystars() {
    let mut h = H::new();
    // Rounded rectangle: Up / Down step the roundness, Left = 0, Right = max.
    h.tools().lock().set(Tool::RoundedRectangle);
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    for _ in 0..3 {
        h.key(Key::ArrowUp);
    }
    h.key(Key::ArrowDown);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["roundness"], 12.0);
    // The tool remembers it.
    assert_eq!(h.tools().lock().options.roundness, 12.0);
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    h.key(Key::ArrowRight);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert_eq!(
        one(&r, "shape.add")["roundness"],
        50.0,
        "half the short side"
    );
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    h.key(Key::ArrowLeft);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["roundness"], 0.0);

    // Star: drag distance = outer radius, inner = half; a point at the cursor;
    // Up adds a point, Right raises the outer roundness, Page Up the inner one.
    h.tools().lock().set(Tool::Star);
    h.drag_to((320.0, 180.0), (320.0, 80.0), Modifiers::NONE);
    h.key(Key::ArrowUp);
    h.key(Key::ArrowRight);
    h.key(Key::PageUp);
    let r = h.release((320.0, 80.0), Modifiers::NONE);
    let p = one(&r, "shape.add");
    assert_eq!(p["kind"], "star");
    assert!((p["outer_radius"].as_f64().unwrap() - 100.0).abs() < 0.01);
    assert!((p["inner_radius"].as_f64().unwrap() - 50.0).abs() < 0.01);
    assert!(p["rotation"].as_f64().unwrap().abs() < 0.01, "pointing up");
    assert_eq!(p["points"], 6.0);
    assert_eq!(p["outer_roundness"], 10.0);
    assert_eq!(p["inner_roundness"], 10.0);
    assert_eq!(h.tools().lock().options.points, 6.0);
    h.apply(&r);
    // Polygon: Down removes a point (never below 3).
    h.tools().lock().set(Tool::Polygon);
    h.drag_to((320.0, 180.0), (400.0, 180.0), Modifiers::NONE);
    for _ in 0..5 {
        h.key(Key::ArrowDown);
    }
    let r = h.release((400.0, 180.0), Modifiers::NONE);
    let p = one(&r, "shape.add");
    assert_eq!(p["kind"], "polygon");
    assert_eq!(p["points"], 3.0);
    assert!((p["rotation"].as_f64().unwrap() - 90.0).abs() < 0.01, "{p}");
}

#[test]
fn bezier_path_option_and_alt_make_paths() {
    let mut h = H::new();
    h.tools().lock().options.bezier_path = true;
    let r = h.draw(Tool::Ellipse, (100.0, 50.0), (300.0, 150.0));
    assert_eq!(one(&r, "shape.add")["bezier"], true);
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let group = h.contents(&layer)[0].clone();
    let path = h.contents(&group)[0].clone();
    assert_eq!(h.name(&path), "Path 1");
    assert!(matches!(h.value(&path, "path"), Some(Value::Bezier(_))));
    // Alt/Option held when the drag starts does the same.
    h.tools().lock().options.bezier_path = false;
    h.tools().lock().set(Tool::Rectangle);
    let a = h.screen(100.0, 50.0);
    h.move_to(a, Modifiers::ALT, Vec::new());
    h.pointer_button(a, true, Modifiers::ALT);
    for i in 1..=6 {
        h.move_to(a + vec2(20.0, 10.0) * i as f32, Modifiers::ALT, Vec::new());
    }
    let r = h.release((220.0, 110.0), Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["bezier"], true);
}

#[test]
fn double_clicking_a_shape_tool_fills_the_comp_or_the_layer() {
    let mut h = H::new();
    let fill = |h: &mut H, tool: Tool, shift: bool| {
        h.tools().lock().fill_request = Some(FillRequest { tool, shift });
        h.frame(Vec::new(), Modifiers::NONE)
    };
    // No selection: a comp-sized rectangle in a new shape layer.
    let r = fill(&mut h, Tool::Rectangle, false);
    let p = one(&r, "shape.add");
    assert_eq!(p["size"], json!([640.0, 360.0]));
    assert_eq!(p["position"], json!([0.0, 0.0]));
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    // That layer selected, Shift: the largest circle that fits, added to it.
    let r = fill(&mut h, Tool::Ellipse, true);
    let p = one(&r, "shape.add");
    assert_eq!(p["layer"], json!(layer));
    assert_eq!(p["size"], json!([360.0, 360.0]));
    // A solid: a mask the size of the layer.
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let r = fill(&mut h, Tool::Rectangle, false);
    let p = one(&r, "mask.create");
    assert_eq!(p["size"], json!([640.0, 360.0]));
    assert_eq!(p["position"], json!([320.0, 180.0]));
    // It also resets the tool's remembered roundness.
    h.tools().lock().options.roundness = 33.0;
    let r = fill(&mut h, Tool::RoundedRectangle, false);
    assert_eq!(one(&r, "mask.create")["roundness"], 10.0);
    assert_eq!(h.tools().lock().options.roundness, 10.0);
}

/// #400: Cmd/Ctrl+Shift+A and F2 deselect (bound in the motion keymap); then
/// a shape tool makes a new shape layer instead of a mask on the image.
#[test]
fn deselect_all_then_shape_tools_make_a_new_layer() {
    let b = crate::actions::bindings().bindings;
    let deselect: Vec<String> = b
        .iter()
        .filter(|b| b.action == crate::actions::ids::DESELECT_ALL)
        .map(|b| b.chord.to_string())
        .collect();
    assert!(deselect.iter().any(|c| c == "F2"), "{deselect:?}");
    assert!(
        deselect.iter().any(|c| c.ends_with("Shift+A")),
        "{deselect:?}"
    );
    let mut h = H::new();
    let image = h.solid([640.0, 360.0], [320.0, 180.0]);
    h.select(&[&image]);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (200.0, 150.0));
    assert!(
        r.invocations.iter().any(|i| i.id == "mask.create"),
        "a mask on the image"
    );
    assert!(h.action(crate::actions::ids::DESELECT_ALL));
    assert!(h.session().lock().selected_layers.is_empty());
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (200.0, 150.0));
    let add = one(&r, "shape.add");
    assert!(
        add.get("layer").is_none() && add.get("comp").is_some(),
        "a new shape layer: {add}"
    );
}

/// #733: a shape tool selects the group it draws (AE), so the box shown is the
/// group's and its corner scales the group around the group's anchor (the
/// rectangle's center), not the layer around the comp center.
#[test]
fn a_drawn_shape_s_group_box_scales_around_the_group_anchor() {
    let mut h = H::new();
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    h.apply(&r);
    h.frame(Vec::new(), Modifiers::NONE);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let group = h.contents(&layer)[0].clone();
    assert_eq!(
        h.session().lock().selected_groups,
        [format!("{layer}#{group}")]
    );
    h.tools().lock().set(Tool::Selection);
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(
        h.viewer.select.group.as_ref(),
        Some(&group),
        "the group's box"
    );
    // Bottom-right corner (300, 150) out by (50, 25): the half size 100 × 50
    // around the center (200, 100) grows by half.
    let r = h.drag((300.0, 150.0), (350.0, 175.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close(
        vec2v(h.value(&group, "transform:scale")),
        [150.0, 150.0]
    ));
    assert!(close(
        vec2v(h.value(&group, "transform:position")),
        [-120.0, -80.0]
    ));
    assert!(close3(
        h.v3(&layer, "transform:scale"),
        [100.0, 100.0, 100.0]
    ));
    // A click away from the box deselects the group in both panels.
    h.click(600.0, 300.0, Modifiers::NONE);
    assert!(h.viewer.select.group.is_none());
    assert!(h.session().lock().selected_groups.is_empty());
    // Drawing into the selected shape layer selects the new group.
    h.select(&[&layer]);
    let r = h.draw(Tool::Ellipse, (400.0, 200.0), (500.0, 300.0));
    h.apply(&r);
    h.frame(Vec::new(), Modifiers::NONE);
    let ellipse = h.contents(&layer)[0].clone();
    assert_ne!(ellipse, group);
    assert_eq!(
        h.session().lock().selected_groups,
        [format!("{layer}#{ellipse}")]
    );
}
