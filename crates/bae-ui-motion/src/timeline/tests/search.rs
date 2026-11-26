//! The search field: filtering layers and properties, Edit > Find.

use super::*;

#[test]
fn search_field_filters_layers_and_properties() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let b = path(
        &h.run(
            "layer.create",
            json!({"comp": comp, "kind": "solid", "name": "Backdrop"}),
        )["path"],
    );
    let mut tl = h.bars();
    h.session().lock().reveal.clear();
    let rows = |h: &H, tl: &mut TimelinePanel| {
        let s = h.session();
        let s = s.lock();
        tl.search_rows(&h.stage, &h.comp, &s)
    };
    let a = query::display_name_of(&h.stage, &h.layer);
    // A property name: every layer, opened down to it.
    tl.search = "opac".into();
    assert_eq!(
        rows(&h, &mut tl),
        [
            "Backdrop",
            "  Transform",
            "    transform:opacity",
            a.as_str(),
            "  Transform",
            "    transform:opacity",
        ]
    );
    // A layer name: that layer only.
    tl.search = "BACK".into();
    assert_eq!(rows(&h, &mut tl), ["Backdrop"]);
    assert_eq!(query::display_name_of(&h.stage, &b), "Backdrop");
    // Nothing matches: no rows; cleared: all layers again.
    tl.search = "zzz".into();
    assert!(rows(&h, &mut tl).is_empty());
    tl.search.clear();
    assert_eq!(rows(&h, &mut tl).len(), 2);
    // Edit ▸ Find is the timeline's.
    let r = h.action(&mut tl, Invocation::new("motion.file.find"));
    assert!(r.invocations.is_empty());
}
