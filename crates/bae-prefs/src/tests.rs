use super::*;
use serde_json::json;

#[test]
fn pages_in_ae_order() {
    let titles: Vec<_> = PAGES.iter().map(|p| p.title).collect();
    assert_eq!(
        titles,
        [
            "General",
            "Previews",
            "Display",
            "Import",
            "Output",
            "Grids & Guides",
            "Labels",
            "Media & Disk Cache",
            "Video Preview",
            "Appearance",
            "New Project",
            "Auto-Save",
            "Memory & Performance",
            "Audio Hardware",
            "Audio Output Mapping",
            "Type",
            "Scripting & Expressions",
            "3D",
            "Notifications",
        ]
    );
}

#[test]
fn keys_unique_and_defaults_valid() {
    let mut seen = std::collections::HashSet::new();
    let defaults = Preferences::default();
    for p in schema::all() {
        assert!(seen.insert(p.key), "duplicate key {}", p.key);
        let page = p.key.split('.').next().unwrap();
        assert!(!page.is_empty());
        let v = defaults.get(p.key).unwrap();
        assert_eq!(
            validate(p, v).as_ref(),
            Some(v),
            "default of {} is invalid",
            p.key
        );
        if let Some(When::On(k) | When::Is(k, _)) = p.enabled {
            assert!(
                schema::find(k).is_some(),
                "{} depends on unknown {k}",
                p.key
            );
        }
        if let Some(When::Is(k, option)) = p.enabled {
            let Kind::Choice { options, .. } = schema::find(k).unwrap().kind else {
                panic!("{k} is not a choice");
            };
            assert!(options.contains(&option), "{option} not an option of {k}");
        }
    }
}

#[test]
fn ae_defaults() {
    let p = Preferences::default();
    assert_eq!(p.levels_of_undo(), 99);
    assert_eq!(p.int(keys::PATH_POINT_SIZE), 5);
    assert!(p.bool(keys::CREATE_LAYERS_AT_START));
    assert!(!p.bool(keys::DEFAULT_SPATIAL_LINEAR));
    assert_eq!(p.float(keys::GRID_SPACING), 80.0);
    assert_eq!(p.int(keys::GRID_SUBDIVISIONS), 4);
    assert_eq!(p.color(keys::GRID_COLOR), [0, 0x80, 0]);
    assert_eq!(p.float(keys::ACTION_SAFE), 10.0);
    assert_eq!(p.float(keys::TITLE_SAFE), 20.0);
    assert_eq!(p.float(keys::CC_ACTION_SAFE), 32.5);
    assert_eq!(p.float(keys::CC_TITLE_SAFE), 40.0);
    assert_eq!(p.auto_save_minutes(), Some(5));
    assert_eq!(p.int(keys::AUTO_SAVE_MAX_VERSIONS), 3);
    assert_eq!(p.float(keys::SEQUENCE_FPS), 30.0);
    assert_eq!(p.int("labels.default_comp"), 15);
    assert_eq!(p.label_name(2), "Yellow");
    assert_eq!(p.label_color(1), [0xa7, 0x41, 0x3d]);
    assert_eq!(p.label_color(0), [0x3a, 0x3a, 0x3a]);
}

#[test]
fn lenient_reading() {
    let data = json!({
        "general.levels_of_undo": 500,      // clamped to 99
        "general.show_tool_tips": "yes",    // wrong type: default
        "grids.grid_style": "Zigzag",       // unknown option: default
        "grids.grid_color": "#FF0000",      // normalized
        "labels.name_1": "Hot",
        "future.setting": [1, 2],           // kept
    });
    let (p, fixed) = Preferences::from_json(data.as_object().unwrap());
    assert_eq!(p.levels_of_undo(), 99);
    assert!(p.bool("general.show_tool_tips"));
    assert_eq!(p.str(keys::GRID_STYLE), "Lines");
    assert_eq!(p.color(keys::GRID_COLOR), [255, 0, 0]);
    assert_eq!(p.label_name(1), "Hot");
    for k in [
        "general.levels_of_undo",
        "general.show_tool_tips",
        "grids.grid_style",
    ] {
        assert!(fixed.iter().any(|f| f == k), "{k} not reported");
    }
    let out = p.to_json();
    assert_eq!(out["future.setting"], json!([1, 2]));
    assert_eq!(out["labels.name_1"], json!("Hot"));
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
    p.reset(keys::LEVELS_OF_UNDO);
    assert_eq!(p.levels_of_undo(), 99);
}

#[test]
fn enable_conditions() {
    let mut p = Preferences::default();
    let interval = schema::find(keys::AUTO_SAVE_INTERVAL).unwrap();
    assert!(p.enabled(interval));
    p.set(keys::AUTO_SAVE, false);
    assert!(!p.enabled(interval));
    assert_eq!(p.auto_save_minutes(), None);
    let folder = schema::find(keys::AUTO_SAVE_FOLDER).unwrap();
    assert!(!p.enabled(folder));
    p.set(keys::AUTO_SAVE_LOCATION, "Custom Location");
    assert!(p.enabled(folder));
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
    let zh: std::collections::HashMap<_, _> = i18n::ZH.iter().copied().collect();
    let proper = ["Source Code Pro", "Menlo", "Consolas", "Courier New"];
    let mut missing = Vec::new();
    let mut check = |s: &str| {
        if s.chars().any(char::is_alphabetic) && !zh.contains_key(s) && !proper.contains(&s) {
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
                        Kind::Choice { options, .. } => options.iter().for_each(|o| check(o)),
                        Kind::Slider { left, right, .. } => {
                            check(left);
                            check(right);
                        }
                        Kind::Path { button } => check(button),
                        _ => {}
                    }
                    if let DefaultValue::Str(s) = p.default
                        && matches!(p.kind, Kind::Text)
                        && p.key.starts_with("new_project")
                    {
                        check(s);
                    }
                }
                Item::Row(list) => items(list, check),
                Item::Group(title, list) => {
                    check(title);
                    items(list, check);
                }
                Item::Button(_, text) | Item::Note(text) => check(text),
                Item::Only(_, item) => items(std::slice::from_ref(*item), check),
                Item::Gap => {}
            }
        }
    }
    for page in PAGES {
        check(page.title);
        items(page.items, &mut check);
    }
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
        r#"{"kind":"bae-preferences","version":1,"data":{"general.levels_of_undo":"x","auto_save.interval":30}}"#,
    )
    .unwrap();
    let (odd, _) = PrefsStore::open(&path);
    assert_eq!(odd.current().levels_of_undo(), 99);
    assert_eq!(odd.current().auto_save_minutes(), Some(30));
}
