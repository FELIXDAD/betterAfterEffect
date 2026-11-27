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
            Only(_, entry) => out.extend(lines(std::slice::from_ref(entry))),
            // AE's macOS title (the reference is the Mac menu).
            RevealProject => out.push(item("Reveal Project in Finder")),
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
    let ae: Value = serde_json::from_str(AE).unwrap();
    let ours: Vec<_> = crate::timeline::Column::ALL
        .iter()
        .map(|c| c.title())
        .collect();
    let entries = ae["menus"]["timeline.header"].as_array().unwrap();
    let cols = entries.iter().find(|i| i["title"] == "Columns").unwrap()["items"]
        .as_array()
        .unwrap();
    assert_eq!(
        ours,
        cols.iter()
            .map(|i| i["title"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    check("project.blank", PROJECT_BLANK);
    let columns = ae["menus"]["project.header"][3]["items"]
        .as_array()
        .unwrap();
    assert_eq!(columns.len(), crate::project::COLUMNS.len());
    for (reference, col) in columns.iter().zip(crate::project::COLUMNS) {
        assert_eq!(reference["title"], col.title);
        assert_eq!(reference["enabled"].as_bool().unwrap_or(true), !col.fixed);
        assert_eq!(reference["checked"].as_bool().unwrap_or(false), col.visible);
    }
    assert!(crate::project::ZH.contains(&("Thumbnail Transparency Grid", "缩览图透明网格")));
    assert!(crate::project::ZH.contains(&("Columns", "列数")));
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
    for menu in [VIEWER_BLANK, VIEWER_LAYER, PROJECT_BLANK, PROJECT_ITEM] {
        walk(&lines(menu), &mut missing);
    }
    for c in crate::timeline::Column::ALL {
        walk(&[item(c.title())], &mut missing);
    }
    missing.sort();
    missing.dedup();
    assert!(missing.is_empty(), "no Chinese for {missing:?}");
}

/// Our own items (Rename …) are greyed exactly where AE 26 greys them (#599).
#[test]
fn own_items_are_greyed_like_ae() {
    let json: Value = serde_json::from_str(AE).unwrap();
    for (name, menu) in [
        ("viewer.blank", VIEWER_BLANK),
        ("timeline.blank", TIMELINE_BLANK),
    ] {
        let ae = json["menus"][name].as_array().unwrap();
        for e in menu {
            let (title, enabled) = match *e {
                Own(t) => (t, true),
                Off(t) => (t, false),
                _ => continue,
            };
            let item = ae
                .iter()
                .find(|i| i["title"] == title)
                .unwrap_or_else(|| panic!("{name}: no AE item {title}"));
            let ae_enabled = item["enabled"].as_bool() != Some(false);
            assert_eq!(enabled, ae_enabled, "{name}: {title}");
        }
    }
}

/// #600: the Project item menu greys by item kind as AE does — the comp-only
/// and not-for-comps entries match `project.item.comp` and `.folder`.
#[test]
fn project_item_menu_greys_by_kind() {
    let json: Value = serde_json::from_str(AE).unwrap();
    for (name, kind) in [("comp", COMP), ("folder", FOLDER)] {
        let ae = json["menus"][format!("project.item.{name}")]
            .as_array()
            .unwrap()
            .clone();
        let mut i = 0;
        for e in PROJECT_ITEM {
            let titles = lines(std::slice::from_ref(e));
            let Some(Line::Item(title, _)) = titles.first() else {
                continue;
            };
            while ae[i]["title"]
                .as_str()
                .is_some_and(|t| !same_title(t, title))
            {
                i += 1;
            }
            if let Only(kinds, _) = e {
                let ours = kinds & kind != 0;
                let theirs = ae[i]["enabled"].as_bool().unwrap_or(true);
                // AE's capture greys file items on a footage-less project:
                // only check what the comp / folder rows show enabled or not
                // for comp-only and not-for-comp entries.
                if *kinds == COMP || *kinds == FOOTAGE | FOLDER | SOLID {
                    assert_eq!(ours, theirs, "{name}: {title}");
                }
            }
            i += 1;
        }
    }
}
