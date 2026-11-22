use super::*;
use serde_json::json;

#[test]
fn pages_in_ae_order() {
    let titles: Vec<_> = PAGES.iter().map(|p| p.title).collect();
    assert_eq!(
        titles,
        [
            "Startup & Repair",
            "Project",
            "Composition",
            "Previews",
            "Appearance",
            "Grids & Guides",
            "Labels",
            "Type",
            "Import",
            "Export",
            "Audio",
            "Disk",
            "Memory & CPU",
            "Video",
            "3D",
            "Scripting & Expressions",
        ]
    );
}

#[test]
fn keys_unique_and_defaults_valid() {
    let mut seen = std::collections::HashSet::new();
    let defaults = Preferences::default();
    // 214 controls in R1's data: 205 stored values (label entries hold a color and
    // a name; buttons, notes, the device table and the preview hold none) + 1 hidden.
    assert_eq!(schema::all().len(), 206);
    for p in schema::all() {
        assert!(seen.insert(p.key), "duplicate key {}", p.key);
        let v = defaults.get(p.key).unwrap();
        assert_eq!(
            validate(p, v).as_ref(),
            Some(v),
            "default of {} is invalid",
            p.key
        );
        if let Some(w) = &p.enabled {
            check_when(p.key, w);
        }
    }
}

fn check_when(key: &str, w: &When) {
    match w {
        When::On(k) | When::Off(k) => {
            assert_eq!(
                schema::find(k).map(|p| p.kind),
                Some(Kind::Bool),
                "{key}: {k}"
            );
        }
        When::Is(k, value) => {
            let Some(Kind::Choice { options, .. }) = schema::find(k).map(|p| p.kind) else {
                panic!("{key}: {k} is not a choice");
            };
            assert!(options.iter().any(|o| o.value == *value), "{key}: {value}");
        }
        When::All(all) => all.iter().for_each(|w| check_when(key, w)),
    }
}

/// AE 26 defaults as R1 read them (docs/reference/ae/ui/preferences-dialog.json).
#[test]
fn ae_defaults() {
    let p = Preferences::default();
    assert_eq!(
        p.levels_of_undo(),
        1000,
        "BAE default; AE 26 has no UI for it"
    );
    assert_eq!(p.int(keys::PATH_POINT_SIZE), 5);
    assert!(p.bool(keys::CREATE_LAYERS_AT_START));
    assert!(p.bool(keys::DEFAULT_SPATIAL_LINEAR));
    assert!(p.bool(keys::SPLIT_LAYERS_ABOVE));
    assert!(!p.bool(keys::CENTER_SHAPE_ANCHOR));
    assert_eq!(p.str(keys::COLOR_THEME), "darkest");
    assert_eq!(p.int(keys::GRID_SPACING), 80);
    assert_eq!(p.int(keys::GRID_SUBDIVISIONS), 4);
    assert_eq!(p.color(keys::GRID_COLOR), [0, 0x80, 0]);
    assert_eq!(p.str(keys::GRID_STYLE), "lines");
    assert_eq!(p.color(keys::GUIDE_COLOR), [0x20, 0xe0, 0xff]);
    assert_eq!(p.float(keys::ACTION_SAFE), 10.0);
    assert_eq!(p.float(keys::TITLE_SAFE), 20.0);
    assert_eq!(p.float(keys::CC_ACTION_SAFE), 32.5);
    assert_eq!(p.float(keys::CC_TITLE_SAFE), 40.0);
    assert_eq!(p.auto_save_minutes(), Some(5));
    assert_eq!(p.int(keys::AUTO_SAVE_MAX_VERSIONS), 3);
    assert_eq!(p.str(keys::AUTO_SAVE_LOCATION), "next_to_project");
    assert_eq!(p.float(keys::SEQUENCE_FPS), 30.0);
    assert_eq!(p.str(keys::STILL_DURATION), "0:00:01:00");
    assert_eq!(p.int(keys::EXPR_FONT_SIZE), 12);
    assert_eq!(p.int("labels.label_default_comp"), 15);
    assert_eq!(p.label_name(2), "Yellow");
    assert_eq!(p.label_color(1), [0xb5, 0x38, 0x38]);
    assert!(p.label_color_is_default(1));
}

#[test]
fn lenient_reading() {
    let data = json!({
        "appearance.path_point_and_handle_size": 500, // clamped to 15
        "appearance.show_tool_tips": "yes",          // wrong type: default
        "grids_guides.grid_style": "zigzag",         // unknown option: default
        "grids_guides.grid_color": "#FF0000",        // normalized
        "import.still_footage_duration_timecode": "1:99:00:00", // bad timecode
        "labels.label_name_1": "Hot",
        "future.setting": [1, 2],                   // kept
    });
    let (p, fixed) = Preferences::from_json(data.as_object().unwrap());
    assert_eq!(p.int(keys::PATH_POINT_SIZE), 15);
    assert!(p.bool("appearance.show_tool_tips"));
    assert_eq!(p.str(keys::GRID_STYLE), "lines");
    assert_eq!(p.color(keys::GRID_COLOR), [255, 0, 0]);
    assert_eq!(p.str(keys::STILL_DURATION), "0:00:01:00");
    assert_eq!(p.label_name(1), "Hot");
    for k in [
        "appearance.path_point_and_handle_size",
        "appearance.show_tool_tips",
        "grids_guides.grid_style",
        "import.still_footage_duration_timecode",
    ] {
        assert!(fixed.iter().any(|f| f == k), "{k} not reported");
    }
    let out = p.to_json();
    assert_eq!(out["future.setting"], json!([1, 2]));
    assert_eq!(out["labels.label_name_1"], json!("Hot"));
}

#[test]
fn set_validates() {
    let mut p = Preferences::default();
    assert!(p.set(keys::LEVELS_OF_UNDO, 10));
    assert!(!p.set(keys::LEVELS_OF_UNDO, 10));
    assert!(p.set(keys::LEVELS_OF_UNDO, 0));
    assert_eq!(p.levels_of_undo(), 1);
    assert!(!p.set("nope", 1));
    assert!(!p.set(keys::GRID_COLOR, "red"));
    assert!(!p.set(keys::GRID_STYLE, "Lines"), "values are option ids");
    assert!(p.set(keys::GRID_STYLE, "dots"));
    p.reset(keys::LEVELS_OF_UNDO);
    assert_eq!(p.levels_of_undo(), 1000);
}

#[test]
fn enable_conditions() {
    let mut p = Preferences::default();
    let interval = schema::find(keys::AUTO_SAVE_INTERVAL).unwrap();
    assert!(p.enabled(interval));
    p.set(keys::AUTO_SAVE, false);
    assert!(!p.enabled(interval));
    assert_eq!(p.auto_save_minutes(), None);
    // All: line-number color needs syntax highlighting and line numbers.
    let line_color = schema::find("scripting_expressions.expr_color_line_numbers").unwrap();
    assert!(p.enabled(line_color));
    p.set("scripting_expressions.expr_show_line_numbers", false);
    assert!(!p.enabled(line_color));
}

#[test]
fn store_notifies() {
    use std::sync::atomic::AtomicUsize;
    let store = PrefsStore::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    store.subscribe(move |p| {
        assert_eq!(p.levels_of_undo(), 7);
        c.fetch_add(1, Ordering::SeqCst);
    });
    let mut p = (*store.current()).clone();
    store.set(p.clone()).unwrap();
    assert_eq!(store.generation(), 0, "unchanged values do nothing");
    p.set(keys::LEVELS_OF_UNDO, 7);
    store.set(p).unwrap();
    assert_eq!(store.generation(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.current().levels_of_undo(), 7);
}

#[test]
fn every_string_translated() {
    // Kept in English in AE's Chinese UI (R1 recorded them so).
    let proper = crate::pages::SAME_IN_ZH;
    let mut missing = Vec::new();
    let mut check = |s: &str| {
        if s.chars().any(char::is_alphabetic) && i18n::zh(s).is_none() && !proper.contains(&s) {
            missing.push(s.to_owned());
        }
    };
    for name in LABEL_NAMES {
        check(name);
    }
    fn items(list: &'static [Item], check: &mut dyn FnMut(&str)) {
        for item in list {
            match item {
                Item::P(p) => {
                    check(p.label);
                    check(p.unit);
                    match p.kind {
                        Kind::Choice { options, .. } => options.iter().for_each(|o| check(o.label)),
                        Kind::Slider { left, right, .. } => {
                            check(left);
                            check(right);
                        }
                        _ => {}
                    }
                }
                Item::Row(list) => items(list, check),
                Item::Cols(cols) => cols.iter().for_each(|c| items(c, check)),
                Item::Group(title, list) => {
                    check(title);
                    items(list, check);
                }
                Item::Button(_, text) | Item::Info(text, _) => check(text),
                Item::Table(text, cols) => {
                    check(text);
                    cols.iter().for_each(|c| check(c));
                }
                Item::Only(_, item) | Item::When(_, item) => {
                    items(std::slice::from_ref(*item), check)
                }
                Item::Preview(_) => {}
            }
        }
    }
    for page in PAGES {
        check(page.title);
        items(page.items, &mut check);
    }
    for s in ["Preferences", "OK", "Cancel", "Previous", "Next"] {
        check(s);
    }
    missing.sort();
    missing.dedup();
    assert!(missing.is_empty(), "no Chinese for: {missing:?}");
}

#[cfg(feature = "file")]
#[test]
fn file_roundtrip_and_corrupt_fallback() {
    use bae_platform::settings::LoadStatus;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let (store, status) = PrefsStore::open(&path);
    assert_eq!(status, LoadStatus::Missing);
    assert_eq!(*store.current(), Preferences::default());
    let mut p = (*store.current()).clone();
    p.set(keys::LEVELS_OF_UNDO, 12);
    p.set(keys::GRID_COLOR, "#123456");
    store.set(p.clone()).unwrap();
    // "Restart": read the file again.
    let (again, status) = PrefsStore::open(&path);
    assert_eq!(status, LoadStatus::Loaded);
    assert_eq!(*again.current(), p);
    // Cancel = never calling set: nothing written.
    let before = std::fs::read(&path).unwrap();
    let _ = PrefsStore::open(&path);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // Corrupt file: defaults, no panic.
    std::fs::write(&path, "{ not json").unwrap();
    let (broken, status) = PrefsStore::open(&path);
    assert!(matches!(status, LoadStatus::Corrupt { .. }));
    assert_eq!(*broken.current(), Preferences::default());
    // Old/odd data in a valid envelope: per-key fallback.
    std::fs::write(
        &path,
        r#"{"kind":"bae-preferences","version":1,"data":{"general.levels_of_undo":"x","project.auto_save_interval_minutes":30}}"#,
    )
    .unwrap();
    let (odd, _) = PrefsStore::open(&path);
    assert_eq!(odd.current().levels_of_undo(), 1000);
    assert_eq!(odd.current().auto_save_minutes(), Some(30));
}
