//! The context menus against AE 26's (`docs/reference/ae/mac/ui/context-menus.json`):
//! same entries, order, separators and submenus; every path resolves to a catalog
//! entry; every title has Chinese.

use serde_json::Value;

use super::*;

const AE: &str = include_str!("../../../../docs/reference/ae/mac/ui/context-menus.json");

/// One menu line.
#[derive(Debug)]
enum Line {
    Sep,
    /// Title and, for a submenu, its lines.
    Item(String, Option<Vec<Line>>),
}

fn item(t: &str) -> Line {
    Line::Item(t.to_owned(), None)
}

/// Our catalog title as AE shows it on macOS, and AE's run-time titles as the
/// catalog names them.
fn same_title(ae: &str, ours: &str) -> bool {
    let ae = match ae {
        "Reveal in Finder" => "Reveal in Explorer",
        "Active Camera (Camera 1)" => "Active Camera (Default)",
        "Reset Camera 1" => "Reset 3D View",
        "Camera 1 Settings..." => "Camera Settings...",
        // Layer ▸ Transform on a camera.
        "Point of Interest..." => "Anchor Point...",
        t => t,
    };
    ae == ours
}

fn node_lines(nodes: &'static [Node], out: &mut Vec<Line>) {
    // AE sorts the marked items of a group by title (Keyframe Assistant).
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Item(it) = &nodes[i]
            && it.sorted
        {
            let mut run = Vec::new();
            while let Some(Node::Item(it)) = nodes
                .get(i)
                .filter(|n| matches!(n, Node::Item(it) if it.sorted))
            {
                run.push(it.title);
                i += 1;
            }
            run.sort_unstable();
            out.extend(run.into_iter().map(item));
            continue;
        }
        match &nodes[i] {
            Node::Item(it) if it.platform.current() && it.status != catalog::Status::Dynamic => {
                out.push(item(it.title))
            }
            Node::Item(_) => {}
            Node::Separator => out.push(Line::Sep),
            Node::Submenu(s) => {
                let mut sub = Vec::new();
                node_lines(s.children, &mut sub);
                out.push(Line::Item(s.title.to_owned(), Some(sub)));
            }
            Node::Effects => out.push(item("<effects>")),
        }
        i += 1;
    }
}

fn lines(entries: &'static [Entry]) -> Vec<Line> {
    let mut out = Vec::new();
    for e in entries {
        match *e {
            Menu(path) => {
                let n = find(path).unwrap_or_else(|| panic!("no catalog entry {path}"));
                node_lines(std::slice::from_ref(n), &mut out);
            }
            Items(path) => {
                let c = children(path);
                assert!(!c.is_empty(), "no catalog submenu {path}");
                node_lines(c, &mut out);
            }
            Sep => out.push(Line::Sep),
            Sub(title, list) => out.push(Line::Item(title.into(), Some(lines(list)))),
            As(title, path) => {
                assert!(
                    matches!(find(path), Some(Node::Item(_))),
                    "no catalog item {path}"
                );
                out.push(item(title));
            }
            Own(title) | Off(title) => out.push(item(title)),
        }
    }
    out
}

fn compare(ae: &[Value], ours: &[Line], path: &str, errors: &mut Vec<String>) {
    let mut o = ours.iter();
    let mut a = ae.iter();
    loop {
        match (a.next(), o.next()) {
            (None, None) => return,
            (Some(x), None) => {
                return errors.push(format!("{path}: AE has more, from {}", x["title"]));
            }
            (None, Some(l)) => return errors.push(format!("{path}: extra {l:?}")),
            (Some(x), Some(Line::Sep)) if x["type"] == "separator" => {}
            // The effect categories: the rest of AE's list.
            (Some(_), Some(Line::Item(t, _))) if t == "<effects>" => return,
            (Some(x), Some(Line::Item(t, sub))) if x["type"] != "separator" => {
                let at = x["title"].as_str().unwrap_or_default();
                if !same_title(at, t) {
                    return errors.push(format!("{path}: AE {at:?} vs ours {t:?}"));
                }
                match (x["items"].as_array(), sub) {
                    // Filled when it opens (Layer Styles) or at run time (recent
                    // footage).
                    (Some(_), Some(s)) if s.is_empty() => {}
                    (Some(items), Some(_)) if items.is_empty() => {}
                    (Some(items), Some(s)) => compare(items, s, &format!("{path} ▸ {t}"), errors),
                    (None, None) => {}
                    _ => errors.push(format!("{path}: {t} submenu on one side only")),
                }
            }
            (Some(x), Some(l)) => {
                return errors.push(format!("{path}: AE {} vs ours {l:?}", x["title"]));
            }
        }
    }
}

fn check(name: &str, menu: &'static [Entry]) {
    let json: Value = serde_json::from_str(AE).unwrap();
    let ae = json["menus"][name]
        .as_array()
        .unwrap_or_else(|| panic!("no AE menu {name}"));
    let mut errors = Vec::new();
    compare(ae, &lines(menu), name, &mut errors);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn menus_match_ae() {
    check("viewer.blank", VIEWER_BLANK);
    check("viewer.layer.solid", VIEWER_LAYER);
    check("timeline.blank", TIMELINE_BLANK);
    // The Timeline's layer menu is the same table (AE 26 on the Mac).
    check("timeline.layer.camera", VIEWER_LAYER);
    check("timeline.header", TIMELINE_HEADER);
    check("project.blank", PROJECT_BLANK);
    check("project.header", PROJECT_HEADER);
    for kind in ["comp", "footage", "folder"] {
        check(&format!("project.item.{kind}"), PROJECT_ITEM);
    }
}

#[test]
fn every_title_has_chinese() {
    bae_ui::i18n::register(&crate::i18n::all());
    bae_ui::i18n::register(crate::tools::ZH);
    bae_ui::i18n::register(crate::catalog::menu_zh());
    let mut missing = Vec::new();
    fn walk(lines: &[Line], missing: &mut Vec<String>) {
        for l in lines {
            let Line::Item(t, sub) = l else { continue };
            if t != "<effects>" && !bae_ui::i18n::has_translation(t) {
                missing.push(t.clone());
            }
            if let Some(s) = sub {
                walk(s, missing);
            }
        }
    }
    for menu in [
        VIEWER_BLANK,
        VIEWER_LAYER,
        TIMELINE_HEADER,
        PROJECT_BLANK,
        PROJECT_ITEM,
        PROJECT_HEADER,
    ] {
        walk(&lines(menu), &mut missing);
    }
    missing.sort();
    missing.dedup();
    assert!(missing.is_empty(), "no Chinese for {missing:?}");
}
