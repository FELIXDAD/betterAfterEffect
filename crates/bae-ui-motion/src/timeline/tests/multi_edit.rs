//! Editing several selected layers at once: values and switches.

use super::*;

#[test]
fn editing_one_of_several_selected_layers_edits_them_all_in_one_undo_step() {
    use crate::common::Edit;
    const OP: &str = "transform:opacity";
    let mut h = H::new();
    let comp = h.comp.clone();
    let a = h.layer.clone();
    let b = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let c = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    h.run(
        "attr.set",
        json!({"path": c, "attr": OP, "value": {"type": "float", "value": 50.0}}),
    );
    let opacity = |h: &H| -> Vec<f64> {
        [&a, &b, &c]
            .iter()
            .map(|l| {
                query::sample_attr(&h.stage, l, OP, Time::ZERO)
                    .and_then(|v| v.as_f64())
                    .unwrap()
            })
            .collect()
    };
    h.session().lock().selected_layers = vec![a.clone(), b.clone(), c.clone()];

    // A drag moves the others by as much (100 → 80: c 50 → 30), previewed live.
    h.edit(&a, OP, Edit::Live(Value::Float(80.0)));
    let scrub = h.session().lock().scrub.clone().unwrap();
    assert_eq!(scrub.value_for(&c, OP), Some(&Value::Float(30.0)));
    let r = h.edit(&a, OP, Edit::Commit(Value::Float(80.0)));
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "attr.set_many");
    h.apply(&r);
    assert_eq!(opacity(&h), [80.0, 80.0, 30.0]);
    h.undo();
    assert_eq!(opacity(&h), [100.0, 100.0, 50.0]);

    // A typed value sets them all; a locked layer keeps its value.
    h.run(
        "layer.set_attr",
        json!({"layers": [b], "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    h.apply(&h.edit(&a, OP, Edit::Commit(Value::Float(40.0))));
    assert_eq!(opacity(&h), [40.0, 100.0, 40.0]);

    // Only the edited layer when it is not among the selected ones.
    h.session().lock().selected_layers = vec![b.clone(), c.clone()];
    let r = h.edit(&a, OP, Edit::Commit(Value::Float(10.0)));
    assert_eq!(r.invocations[0].id, "attr.set_at");
    h.apply(&r);
    assert_eq!(opacity(&h), [10.0, 100.0, 40.0]);

    // Properties selected across layers: Cmd-clicked Opacity of a and c.
    {
        let s = h.session();
        let mut s = s.lock();
        s.selected_layers = vec![a.clone()];
        s.selected_props = vec![(a.clone(), OP.into()), (c.clone(), OP.into())];
    }
    h.apply(&h.edit(&a, OP, Edit::Commit(Value::Float(70.0))));
    assert_eq!(opacity(&h), [70.0, 100.0, 70.0]);
}

#[test]
fn clicking_a_switch_of_one_selected_layer_sets_all_selected() {
    let mut h = H::new();
    for _ in 0..2 {
        h.run("layer.create", json!({"comp": h.comp, "kind": "solid"}));
    }
    let mut tl = h.bars();
    let mut rows = tl.last_bars.clone();
    rows.sort_by(|a, b| a.1.center().y.total_cmp(&b.1.center().y));
    let layers: Vec<PrimPath> = rows.iter().map(|(p, _)| p.clone()).collect();
    let enabled = |h: &H| -> Vec<bool> {
        layers
            .iter()
            .map(|l| {
                query::static_value(&h.stage.prim(l).unwrap(), "enabled") == Some(Value::Bool(true))
            })
            .collect()
    };
    assert_eq!(enabled(&h), [true; 3]);
    // The top two selected: clicking the first one's eye turns both off.
    h.session().lock().selected_layers = vec![layers[0].clone(), layers[1].clone()];
    let r = h.click(&mut tl, pos2(12.0, rows[0].1.center().y), Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one undo step");
    h.apply(&r);
    assert_eq!(enabled(&h), [false, false, true]);
    // An unselected layer's switch changes only that layer.
    let r = h.click(&mut tl, pos2(12.0, rows[2].1.center().y), Modifiers::NONE);
    h.apply(&r);
    assert_eq!(enabled(&h), [false, false, false]);
}

/// #593: a value drag is evaluated live (children through parenting follow),
/// commits as one command, and stays shown until that command has run.
#[test]
fn value_drags_are_evaluated_live_and_commit_once() {
    use crate::common::{Edit, EvalCache};
    let mut h = H::new();
    let comp = h.comp.clone();
    let parent = h.layer.clone();
    let child = path(&h.run("layer.create", json!({"comp": comp, "kind": "null"}))["path"]);
    h.run(
        "attr.set",
        json!({"path": child, "attr": "parent", "value": {"type": "prim_ref", "value": parent}}),
    );
    let mut eval = EvalCache::default();
    let child_x = |h: &H, eval: &mut EvalCache| {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("composition");
        let cx = PanelContext::new(
            &id,
            "composition",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        let ec = crate::viewer::live_evaluator(eval, &cx, &h.session())
            .evaluate_comp(&h.stage, &comp, Time::ZERO)
            .unwrap();
        ec.layer(&child).unwrap().world_matrix.w_axis.x
    };
    let before = child_x(&h, &mut eval);
    let start = match query::attr_spec(&h.stage, &parent, POS).and_then(|s| s.value.clone()) {
        Some(Value::Vec3(v)) => v,
        _ => [0.0; 3],
    };
    // Dragging: no command, the child already moves with the parent.
    let to = Value::Vec3([start[0] + 200.0, start[1], start[2]]);
    let r = h.edit(&parent, POS, Edit::Live(to.clone()));
    assert!(r.invocations.is_empty(), "no command while dragging");
    assert!(
        (child_x(&h, &mut eval) - (before + 200.0)).abs() < 1e-6,
        "live"
    );
    // Release: one command; until it runs, the value stays shown.
    let r = h.edit(&parent, POS, Edit::Commit(to));
    assert_eq!(r.invocations.len(), 1);
    assert!(
        (child_x(&h, &mut eval) - (before + 200.0)).abs() < 1e-6,
        "no flash back"
    );
    h.apply(&r);
    h.session().lock().sync(&h.stage);
    assert!(
        h.session().lock().scrub.is_none(),
        "done once the command ran"
    );
    assert!(
        (child_x(&h, &mut eval) - (before + 200.0)).abs() < 1e-6,
        "committed"
    );
    h.undo();
    h.session().lock().sync(&h.stage);
    assert!(
        (child_x(&h, &mut eval) - before).abs() < 1e-6,
        "one undo step"
    );
}
