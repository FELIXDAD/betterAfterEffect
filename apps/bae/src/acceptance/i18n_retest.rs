//! #152 retest (macOS, dev 8fb98fe), written apart from `i18n_scan`:
//!
//! - new folders, comps, layers and shape contents are named in the UI
//!   language they are created in, Chinese with `BAE_LANG=zh` and English with
//!   `BAE_LANG=en`, while their prim paths keep the same segments;
//! - the names are project data: a project saved in one language and opened in
//!   the other keeps every name, and new items follow the new language;
//! - the chrome #152 lists (shortcut scopes, the window close button, the GPU
//!   kind) and what #153 added (frame time, About) is translated, in the
//!   accessibility tree and in what is painted;
//! - painted text, including rows the panels paint themselves without an
//!   accessibility node, has no English words in Chinese and no Chinese in
//!   English.

use std::collections::BTreeMap;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{AccessKitNode, NodeT as _, Queryable as _};

use super::*;

/// `Layer > New` items made in the first comp, in order.
const NEW_LAYERS: &[&str] = &[
    "motion.layer.new.rectangle",
    "motion.layer.new.ellipse",
    "motion.layer.new.shape",
    "motion.layer.new.solid",
    "motion.layer.new.null",
    "motion.layer.new.text",
    "motion.layer.new.adjustment",
    "motion.layer.new.camera",
    "motion.layer.new.light",
];

/// Latin words of a Chinese UI with an open issue: let through here only, so the
/// rest of the scan keeps running. Remove an entry when its issue is fixed.
const KNOWN_ISSUES: &[(&str, &str)] = &[
    // Character panel "72 px", Graph Editor "px" / "px/秒"; AE zh_CN: 像素.
    ("px", "#197"),
];

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

/// Latin words (two letters or more) of `text` that a Chinese UI should not
/// show; command ids (`viewer.fit`) are identifiers and pass.
fn latin_words(text: &str, extra_ok: &[String]) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_')
        .filter(|w| !(w.contains('.') && w.chars().any(|c| c.is_ascii_lowercase())))
        .flat_map(|w| w.split(['.', '_']))
        .filter(|w| w.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 2)
        // The suite's one list of what AE's own Chinese UI keeps in Latin letters.
        .filter(|w| !super::i18n_scan::ALLOWED.contains(w) && !extra_ok.iter().any(|o| o == w))
        .filter(|w| !KNOWN_ISSUES.iter().any(|(k, _)| k == w))
        .map(str::to_owned)
        .collect()
}

/// Labels and values of the accessibility tree; values of text fields are the
/// user's and are left out.
fn a11y_texts(h: &Ui) -> Vec<String> {
    fn walk(n: AccessKitNode<'_>, out: &mut Vec<String>) {
        let typed = matches!(
            n.role(),
            Role::TextInput | Role::MultilineTextInput | Role::SearchInput
        );
        out.extend(n.label());
        if !typed {
            out.extend(n.value());
            for c in n.children() {
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(h.root().accesskit_node(), &mut out);
    out
}

/// Painted and accessible texts of the last frame that break the language:
/// Latin words in Chinese, Chinese characters in English.
fn wrong_language(h: &Ui, zh: bool, extra_ok: &[String]) -> Vec<String> {
    let mut texts = painted_texts(h);
    texts.extend(a11y_texts(h));
    texts.sort();
    texts.dedup();
    texts
        .into_iter()
        // File paths (the render queue's output) are the file system's.
        .filter(|t| !(t.starts_with('/') || t.starts_with('~') || t.get(1..3) == Some(":\\")))
        .filter_map(|t| {
            if zh {
                let words = latin_words(&t, extra_ok);
                (!words.is_empty()).then(|| format!("“{t}”: {}", words.join(", ")))
            } else {
                // The language picker names Chinese in Chinese.
                (t.chars().any(is_cjk) && t != "简体中文").then(|| format!("“{t}”"))
            }
        })
        .collect()
}

/// Opens every closed timeline row: layer twirls (`图层 1“名称” · 展开`) and
/// property groups (`图层 1“名称” · 内容`, shape groups, `变换` …). Both are
/// toggle buttons labelled with the layer's context, ` · ` and one segment
/// naming what they open (keyframes add more segments; switches are check
/// boxes). Property rows look the same (a selectable label: toggled when
/// selected), so every row is clicked once (by its accesskit id: the contents
/// rows of a layer and of its shape group share a label): a group stays open,
/// a property is only selected. One row per frame: opening a row moves the rows below it.
fn expand_timeline(h: &mut Ui) {
    let mut clicked = std::collections::BTreeSet::new();
    for _ in 0..400 {
        let closed = h
            .query_all_by_role(Role::Button)
            .map(|n| n.accesskit_node())
            .find(|n| {
                n.toggled() == Some(egui::accesskit::Toggled::False)
                    && !clicked.contains(&n.id())
                    && n.label().is_some_and(|l| {
                        l.split_once("” · ")
                            .is_some_and(|(_, what)| !what.contains(" · "))
                    })
            })
            .map(|n| (n.id(), n.raw_bounds()));
        let Some((id, Some(r))) = closed else {
            return;
        };
        clicked.insert(id);
        let r = Rect::from_min_max(
            Pos2::new(r.x0 as f32, r.y0 as f32),
            Pos2::new(r.x1 as f32, r.y1 as f32),
        );
        click_at(h, r.center());
        settle(h);
    }
    panic!("timeline rows still closed after 400 clicks");
}

/// Display names of every prim under `root` (itself included), by path.
fn names_under(stage: &Stage, root: &str) -> BTreeMap<String, String> {
    stage
        .traverse()
        .into_iter()
        .filter(|p| p.as_str() == root || p.as_str().starts_with(&format!("{root}/")))
        .filter(|p| {
            stage
                .prim(p)
                .and_then(|v| bae_motion::query::static_value(&v, "name"))
                .is_some()
                || bae_motion::query::is_layer(stage, p)
                || bae_motion::query::comp_info(stage, p).is_some()
        })
        .map(|p| {
            let name = bae_motion::query::display_name_of(stage, &p);
            (p.as_str().to_owned(), name)
        })
        .collect()
}

/// Every named prim of the project, by path.
fn all_names(stage: &Stage) -> BTreeMap<String, String> {
    names_under(stage, "")
}

/// What a session in one language makes: two comps (Cmd+N), every
/// `Layer > New` item in the first, by menu id.
struct Made {
    comps: Vec<PrimPath>,
    layers: Vec<(&'static str, PrimPath)>,
}

fn make_everything(h: &mut Ui) -> Made {
    comp_with(h, NEW_LAYERS);
    let first = h.state().active_comp().unwrap();
    // Layer > New puts the new layer on top: the last made is first.
    let mut layers: Vec<PrimPath> = h.state().layers();
    layers.reverse();
    assert_eq!(
        layers.len(),
        NEW_LAYERS.len(),
        "{:?}",
        h.state().layer_names()
    );
    let layers = NEW_LAYERS.iter().copied().zip(layers).collect();
    let (m, k) = cmd(Key::N);
    key(h, m, k);
    let second = h.state().active_comp().unwrap();
    assert_ne!(first, second, "second Cmd+N made no comp");
    Made {
        comps: vec![first, second],
        layers,
    }
}

impl Made {
    fn layer(&self, id: &str) -> &PrimPath {
        &self.layers.iter().find(|(i, _)| *i == id).unwrap().1
    }
}

/// Shape contents under `layer`: `(path relative to the layer, name)`.
fn contents(stage: &Stage, layer: &PrimPath) -> Vec<(String, String)> {
    let root = format!("{}/contents", layer.as_str());
    names_under(stage, &root)
        .into_iter()
        .filter(|(p, _)| p != &root)
        .map(|(p, n)| (p[layer.as_str().len() + 1..].to_owned(), n))
        .collect()
}

/// A GUI with the status bar showing the GPU like the window does.
fn gui_with_status() -> (Ui, Vec<String>) {
    let mut g = Gui::new();
    let mut extra_ok = Vec::new();
    if let Some(gpu) = gpu() {
        // The adapter's own name ("Apple M4") is a proper noun.
        extra_ok.extend(
            gpu.adapter_info()
                .name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .map(str::to_owned),
        );
        g.set_gpu_status(crate::ui::gpu_summary(&gpu));
    }
    // Tall, so that the expanded timeline rows of two shape layers fit.
    (gui_sized(g, egui::vec2(1600.0, 2600.0)), extra_ok)
}

/// In the active (second, empty) comp: a rectangle and an ellipse layer, numbered
/// per comp like AE, then every timeline row expanded.
fn two_shapes_expanded(h: &mut Ui) -> Vec<String> {
    for id in ["motion.layer.new.rectangle", "motion.layer.new.ellipse"] {
        assert!(h.state_mut().menu(id), "{id} disabled");
        settle(h);
    }
    let names = h.state().layer_names();
    expand_timeline(h);
    names
}

/// Opens Keyboard Shortcuts and returns the accessible texts with a panel scope
/// (`id · <panel>`).
fn shortcut_scopes(h: &mut Ui) -> Vec<String> {
    h.state_mut()
        .invoke(Invocation::new(ids::KEYBOARD_SHORTCUTS));
    settle(h);
    let mut scopes: Vec<String> = a11y_texts(h)
        .into_iter()
        .chain(painted_texts(h))
        .filter_map(|t| {
            let (id, scope) = t.split_once(" · ")?;
            // Rows of the dialog: `<command id> · <panel>`.
            let is_id = id.contains('.')
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_');
            is_id.then(|| scope.to_owned())
        })
        .collect();
    scopes.sort();
    scopes.dedup();
    scopes
}

const TEST_ZH: &str = "acceptance::i18n_retest::chinese_ui_names_new_items_in_chinese";
const TEST_EN: &str = "acceptance::i18n_retest::english_ui_names_new_items_in_english";

#[test]
fn chinese_ui_names_new_items_in_chinese() {
    if !in_child(TEST_ZH, &[("BAE_LANG", "zh")]) {
        return;
    }
    crate::i18n::init_from_env();
    let (mut h, extra_ok) = gui_with_status();
    let mut problems = Vec::new();
    let check = |h: &Ui, step: &str, problems: &mut Vec<String>| {
        for p in wrong_language(h, true, &extra_ok) {
            problems.push(format!("{step}: {p}"));
        }
    };
    settle(&mut h);
    assert_eq!(h.state().doc.title(), "BAE - 未命名项目");
    check(&h, "start", &mut problems);

    let made = make_everything(&mut h);
    let stage = h.state().stage();
    let names: Vec<String> = made.comps.iter().map(|c| h.state().name(c)).collect();
    assert_eq!(names, ["合成 1", "合成 2"]);
    for (path, zh) in [
        ("/comps", "合成"),
        ("/assets", "素材"),
        ("/assets/solids", "纯色"),
    ] {
        let p = PrimPath::new(path).unwrap();
        assert!(stage.has_prim(&p), "{path} missing");
        assert_eq!(bae_motion::query::display_name_of(stage, &p), zh, "{path}");
    }
    let layer_names: Vec<(&str, String)> = made
        .layers
        .iter()
        .map(|(id, p)| (*id, h.state().name(p)))
        .collect();
    println!("zh layers: {layer_names:?}");
    for (id, name) in &layer_names {
        assert!(
            !name.chars().any(|c| c.is_ascii_alphabetic()) && name.chars().any(is_cjk),
            "{id} made “{name}”"
        );
    }
    let by_id = |id: &str| {
        layer_names
            .iter()
            .find(|(i, _)| *i == id)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(by_id("motion.layer.new.rectangle"), "形状图层 1");
    assert_eq!(by_id("motion.layer.new.ellipse"), "形状图层 2");
    assert_eq!(by_id("motion.layer.new.shape"), "形状图层 3");
    assert_eq!(by_id("motion.layer.new.null"), "空 1");
    assert_eq!(by_id("motion.layer.new.adjustment"), "调整图层 1");
    assert_eq!(by_id("motion.layer.new.camera"), "摄像机 1");
    assert_eq!(by_id("motion.layer.new.light"), "灯光 1");
    let rect = contents(stage, made.layer("motion.layer.new.rectangle"));
    let ellipse = contents(stage, made.layer("motion.layer.new.ellipse"));
    println!("zh rectangle contents: {rect:?}\nzh ellipse contents: {ellipse:?}");
    let rect_names: Vec<&str> = rect.iter().map(|(_, n)| n.as_str()).collect();
    let ellipse_names: Vec<&str> = ellipse.iter().map(|(_, n)| n.as_str()).collect();
    for want in ["矩形 1", "矩形路径 1", "填充 1"] {
        assert!(rect_names.contains(&want), "{want} not in {rect:?}");
    }
    for want in ["椭圆 1", "椭圆路径 1", "填充 1"] {
        assert!(ellipse_names.contains(&want), "{want} not in {ellipse:?}");
    }
    // Contents paths keep the English segments in every language.
    for (path, name) in rect.iter().chain(&ellipse) {
        assert!(path.is_ascii(), "path {path} of “{name}” is not English");
        assert!(name.chars().any(is_cjk), "“{name}” ({path}) is not Chinese");
    }

    // Layers are numbered per comp, and the timeline paints the same names.
    assert_eq!(two_shapes_expanded(&mut h), ["形状图层 2", "形状图层 1"]);
    for want in [
        "形状图层 1",
        "矩形 1",
        "矩形路径 1",
        "填充 1",
        "椭圆 1",
        "椭圆路径 1",
    ] {
        assert!(
            find_text(&h, want).is_some(),
            "timeline does not paint “{want}”: {:?}",
            painted_texts(&h)
        );
    }
    check(&h, "comp 2 expanded", &mut problems);
    // Comp 1, every kind of layer, expanded.
    h.state().session().lock().active_comp = Some(made.comps[0].clone());
    settle(&mut h);
    expand_timeline(&mut h);
    check(&h, "comp 1 expanded", &mut problems);

    // Status bar: GPU kind and frame time.
    if let Some(gpu) = gpu() {
        let kind = crate::ui::workspace_gpu_kind(gpu.adapter_info().device_type);
        let zh_kind = bae_ui::tr(kind);
        assert!(
            zh_kind.chars().all(|c| !c.is_ascii_alphabetic()),
            "{kind}: {zh_kind}"
        );
        assert!(
            find_texts(&h, |t| t.contains(zh_kind)).len() == 1,
            "status bar lacks “{zh_kind}”: {:?}",
            painted_texts(&h)
        );
    }
    assert!(
        !find_texts(&h, |t| t.starts_with("帧 ") && t.ends_with(" 毫秒")).is_empty(),
        "no “帧 … 毫秒”: {:?}",
        painted_texts(&h)
    );

    key(&mut h, Modifiers::COMMAND, Key::M);
    settle(&mut h);
    check(&h, "render queue", &mut problems);

    let scopes = shortcut_scopes(&mut h);
    println!("zh shortcut scopes: {scopes:?}");
    assert!(scopes.iter().any(|s| s == "合成"), "{scopes:?}");
    assert!(scopes.iter().any(|s| s == "时间轴"), "{scopes:?}");
    assert!(
        scopes.iter().all(|s| s.chars().any(is_cjk)),
        "untranslated scopes: {scopes:?}"
    );
    let texts = a11y_texts(&h);
    assert!(
        texts.iter().any(|t| t == "关闭窗口"),
        "no 关闭窗口: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.eq_ignore_ascii_case("Close window")),
        "{texts:?}"
    );
    check(&h, "keyboard shortcuts", &mut problems);
    key(&mut h, Modifiers::NONE, Key::Escape);

    h.state_mut().invoke(Invocation::new(ids::ABOUT));
    settle(&mut h);
    let texts = a11y_texts(&h);
    let build = if cfg!(debug_assertions) {
        "调试版"
    } else {
        "发布版"
    };
    assert!(texts.iter().any(|t| t.contains(build)), "{texts:?}");
    assert!(texts.iter().any(|t| t == "许可证：Apache-2.0"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "关闭窗口"), "About: {texts:?}");
    check(&h, "about", &mut problems);

    // The suggested file name of Save As.
    h.state_mut().dialogs.save.push_back(None);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    assert_eq!(h.state().dialogs.suggested, ["未命名项目.bae"]);

    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn english_ui_names_new_items_in_english() {
    if !in_child(TEST_EN, &[("BAE_LANG", "en")]) {
        return;
    }
    crate::i18n::init_from_env();
    let (mut h, extra_ok) = gui_with_status();
    let mut problems = Vec::new();
    let check = |h: &Ui, step: &str, problems: &mut Vec<String>| {
        for p in wrong_language(h, false, &extra_ok) {
            problems.push(format!("{step}: {p}"));
        }
    };
    settle(&mut h);
    check(&h, "start", &mut problems);
    let made = make_everything(&mut h);
    let stage = h.state().stage();
    let names: Vec<String> = made.comps.iter().map(|c| h.state().name(c)).collect();
    assert_eq!(names, ["Comp 1", "Comp 2"]);
    for path in ["/comps", "/assets", "/assets/solids"] {
        let p = PrimPath::new(path).unwrap();
        let name = bae_motion::query::display_name_of(stage, &p);
        assert!(name.is_ascii(), "{path} is named “{name}”");
    }
    let layer_names: Vec<(&str, String)> = made
        .layers
        .iter()
        .map(|(id, p)| (*id, h.state().name(p)))
        .collect();
    println!("en layers: {layer_names:?}");
    for (id, name) in &layer_names {
        assert!(name.is_ascii(), "{id} made “{name}”");
    }
    let by_id = |id: &str| {
        layer_names
            .iter()
            .find(|(i, _)| *i == id)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(by_id("motion.layer.new.rectangle"), "Shape Layer 1");
    assert_eq!(by_id("motion.layer.new.ellipse"), "Shape Layer 2");
    let rect = contents(stage, made.layer("motion.layer.new.rectangle"));
    let ellipse = contents(stage, made.layer("motion.layer.new.ellipse"));
    let rect_names: Vec<&str> = rect.iter().map(|(_, n)| n.as_str()).collect();
    let ellipse_names: Vec<&str> = ellipse.iter().map(|(_, n)| n.as_str()).collect();
    for want in ["Rectangle 1", "Rectangle Path 1", "Fill 1"] {
        assert!(rect_names.contains(&want), "{want} not in {rect:?}");
    }
    for want in ["Ellipse 1", "Ellipse Path 1", "Fill 1"] {
        assert!(ellipse_names.contains(&want), "{want} not in {ellipse:?}");
    }

    assert_eq!(
        two_shapes_expanded(&mut h),
        ["Shape Layer 2", "Shape Layer 1"]
    );
    for want in [
        "Shape Layer 1",
        "Rectangle 1",
        "Rectangle Path 1",
        "Fill 1",
        "Ellipse Path 1",
    ] {
        assert!(find_text(&h, want).is_some(), "timeline lacks “{want}”");
    }
    check(&h, "comp 2 expanded", &mut problems);
    h.state().session().lock().active_comp = Some(made.comps[0].clone());
    settle(&mut h);
    expand_timeline(&mut h);
    if let Some(gpu) = gpu() {
        let kind = crate::ui::workspace_gpu_kind(gpu.adapter_info().device_type);
        assert_eq!(
            find_texts(&h, |t| t.contains(kind)).len(),
            1,
            "status bar lacks “{kind}”: {:?}",
            painted_texts(&h)
        );
    }
    assert!(
        !find_texts(&h, |t| t.starts_with("frame ") && t.ends_with(" ms")).is_empty(),
        "{:?}",
        painted_texts(&h)
    );
    check(&h, "comp 1 expanded", &mut problems);
    key(&mut h, Modifiers::COMMAND, Key::M);
    settle(&mut h);
    check(&h, "render queue", &mut problems);
    let scopes = shortcut_scopes(&mut h);
    println!("en shortcut scopes: {scopes:?}");
    assert!(scopes.iter().any(|s| s == "Composition"), "{scopes:?}");
    assert!(scopes.iter().any(|s| s == "Timeline"), "{scopes:?}");
    let texts = a11y_texts(&h);
    assert!(texts.iter().any(|t| t == "Close Window"), "{texts:?}");
    check(&h, "keyboard shortcuts", &mut problems);
    key(&mut h, Modifiers::NONE, Key::Escape);
    h.state_mut().invoke(Invocation::new(ids::ABOUT));
    settle(&mut h);
    let texts = a11y_texts(&h);
    assert!(
        texts.iter().any(|t| t == "License: Apache-2.0"),
        "{texts:?}"
    );
    check(&h, "about", &mut problems);
    h.state_mut().dialogs.save.push_back(None);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    assert_eq!(h.state().dialogs.suggested, ["Untitled Project.bae"]);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

const TEST_SWITCH: &str = "acceptance::i18n_retest::names_stay_when_the_language_changes";

fn read_names(path: &Path) -> BTreeMap<String, String> {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write_names(path: &Path, names: &BTreeMap<String, String>) {
    std::fs::write(path, serde_json::to_string_pretty(names).unwrap()).unwrap();
}

/// Opens `project` through File > Open in a fresh GUI.
fn open(project: &Path) -> Ui {
    let mut h = gui();
    settle(&mut h);
    h.state_mut()
        .dialogs
        .open
        .push_back(Some(project.to_owned()));
    key(&mut h, Modifiers::COMMAND, Key::O);
    let d = &h.state().dialogs;
    assert!(
        d.errors.is_empty() && d.unexpected.is_empty(),
        "{:?} {:?}",
        d.errors,
        d.unexpected
    );
    assert_eq!(h.state().doc.location(), Some(project));
    h
}

/// zh session makes and saves a project; an en session opens it (nothing
/// renamed), adds a comp and a shape layer (English) and saves; a zh session
/// opens it again (nothing renamed either way) and numbers the next comp after
/// the Chinese ones.
#[test]
fn names_stay_when_the_language_changes() {
    let Some(phase) = std::env::var("BAE_RETEST_PHASE").ok() else {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tricky_dir(tmp.path());
        for (lang, phase) in [("zh", "1"), ("en", "2"), ("zh", "3")] {
            // `in_child` returns in the parent; each child runs one phase.
            assert!(!in_child(
                TEST_SWITCH,
                &[
                    ("BAE_LANG", lang),
                    ("BAE_RETEST_PHASE", phase),
                    ("BAE_RETEST_DIR", dir.to_str().unwrap()),
                ],
            ));
        }
        return;
    };
    crate::i18n::init_from_env();
    let dir = PathBuf::from(std::env::var("BAE_RETEST_DIR").unwrap());
    let project = dir.join("双语 工程.bae");
    match phase.as_str() {
        "1" => {
            let mut h = gui();
            comp_with(
                &mut h,
                &["motion.layer.new.rectangle", "motion.layer.new.solid"],
            );
            h.state_mut().dialogs.save.push_back(Some(project.clone()));
            key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
            assert!(!h.state().doc.is_dirty());
            let names = all_names(h.state().stage());
            assert!(names.values().any(|n| n == "合成 1"), "{names:?}");
            assert!(names.values().any(|n| n == "矩形路径 1"), "{names:?}");
            write_names(&dir.join("names-1.json"), &names);
        }
        "2" => {
            let mut h = open(&project);
            let before = read_names(&dir.join("names-1.json"));
            assert_eq!(
                all_names(h.state().stage()),
                before,
                "renamed on open in English"
            );
            comp_with(&mut h, &["motion.layer.new.rectangle"]);
            let comp = h.state().active_comp().unwrap();
            assert_eq!(h.state().name(&comp), "Comp 1");
            assert_eq!(h.state().layer_names(), ["Shape Layer 1"]);
            key(&mut h, Modifiers::COMMAND, Key::S);
            assert!(!h.state().doc.is_dirty());
            let names = all_names(h.state().stage());
            for (path, name) in &before {
                assert_eq!(names.get(path), Some(name), "{path} renamed by new items");
            }
            assert!(names.values().any(|n| n == "Rectangle Path 1"), "{names:?}");
            write_names(&dir.join("names-2.json"), &names);
        }
        "3" => {
            let mut h = open(&project);
            let before = read_names(&dir.join("names-2.json"));
            assert_eq!(
                all_names(h.state().stage()),
                before,
                "renamed on open in Chinese"
            );
            let (m, k) = cmd(Key::N);
            key(&mut h, m, k);
            let comp = h.state().active_comp().unwrap();
            assert_eq!(h.state().name(&comp), "合成 2", "AE numbers comps per name");
        }
        other => panic!("phase {other}"),
    }
}
