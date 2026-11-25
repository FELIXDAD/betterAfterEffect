//! The outline's metrics (AE 26).

use super::*;

#[test]
fn outline_follows_ae_metrics() {
    // Positions measured on AE 26's Timeline: the Shy switch at x 275, the
    // expand arrow at x 76 (left of the label), rows 17 points apart.
    let mut h = H::new();
    let comp = h.comp.clone();
    let solid = h.layer.clone();
    let other = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let mut tl = h.bars();
    let bar = |tl: &TimelinePanel, l: &PrimPath| {
        tl.last_bars
            .iter()
            .find(|(p, _)| p == l)
            .map(|(_, r)| r.center().y)
            .unwrap()
    };
    assert_eq!((bar(&tl, &solid) - bar(&tl, &other)).abs(), 17.0);
    let y = bar(&tl, &solid);
    let r = h.click(&mut tl, pos2(275.0, y), Modifiers::NONE);
    h.apply(&r);
    let shy = query::static_value(&h.stage.prim(&solid).unwrap(), "shy");
    assert_eq!(shy.and_then(|v| v.as_bool()), Some(true));
    h.session().lock().reveal.clear();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let y = bar(&tl, &solid);
    let rows = tl.last_bars.len();
    h.click(&mut tl, pos2(76.0, y), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(
        tl.expanded_layer(&solid),
        "the arrow at x 76 expands the layer"
    );
    assert_eq!(tl.last_bars.len(), rows);
}
