//! Save and open: Save As `.bae` / `.baez`, Cmd+S, the unsaved mark, the
//! unsaved-changes question on close / quit / new / open (Save, Don't Save,
//! Cancel), round trips compared through `export_flattened_json`, paths with
//! spaces and Chinese, read-only folders, overwriting, broken files.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use bae_model::Project;
use egui::{Key, Modifiers, pos2};
use serde_json::{Value as Json, json};

use super::*;

/// The flattened export of the open project (what `bae export-json` prints).
fn export(h: &Ui) -> Json {
    bae_model::export_flattened_json(h.state().stage())
}

fn export_file(path: &Path) -> Json {
    let project = Project::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    bae_model::export_flattened_json(&project.stage)
}

/// A comp with a solid, a rectangle and a text layer, a renamed layer, three
/// position keyframes (one hold) and a scale keyframe, made through the UI.
fn animated_project(h: &mut Ui) {
    comp_with(
        h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.rectangle",
            "motion.layer.new.text",
        ],
    );
    // Rename the text layer (selected, on top) through the timeline.
    let head = find_text(h, "Layer Name").expect("timeline");
    let name = h.state().layer_names()[0].clone();
    let row = find_texts(h, |t| t == name)
        .into_iter()
        .find(|r| r.top() > head.bottom() && r.left() < head.left() + 400.0)
        .expect("text layer row");
    double_click_at(h, row.center());
    press(h, Modifiers::COMMAND, Key::A);
    ime_compose(h, &["biao", "biao ti"], "标题");
    send(h, Event::Text(" 01".into()));
    key(h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().layer_names()[0], "标题 01");

    // Position: Option+Shift+P at 0, 10 and 20 frames; scale at 5 frames.
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    key(h, alt_shift, Key::P);
    for _ in 0..10 {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    settle(h);
    key(h, alt_shift, Key::P);
    for _ in 0..10 {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    settle(h);
    key(h, alt_shift, Key::P);
    key(h, Modifiers::NONE, Key::Home);
    for _ in 0..5 {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    settle(h);
    key(h, alt_shift, Key::S);
    // Make the 10-frame position key a hold key, as the keyframe context menu
    // does (`key.set_interpolation`).
    let layer = h.state().layers()[0].clone();
    let times: Vec<_> =
        bae_motion::query::attr_spec(h.state().stage(), &layer, "transform:position")
            .expect("position animated")
            .keyframes
            .keys()
            .copied()
            .collect();
    assert_eq!(times.len(), 3, "position keyframes {times:?}");
    h.state_mut().invoke(Invocation::with_params(
        "key.set_interpolation",
        json!({"keys": [bae_motion::KeyRef::new(layer.clone(), "transform:position", times[1])],
               "interp_out": "hold"}),
    ));
    settle(h);
    assert!(
        h.state()
            .outcomes
            .iter()
            .all(|(_, o)| !o.starts_with("failed")),
        "{:?}",
        h.state().outcomes
    );
}

fn save_as(h: &mut Ui, path: PathBuf) {
    h.state_mut().dialogs.save.push_back(Some(path));
    key(h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
}

fn title(h: &Ui) -> String {
    h.state().doc.title()
}

fn no_surprises(h: &Ui) {
    let d = &h.state().dialogs;
    assert!(
        d.unexpected.is_empty(),
        "unexpected dialogs: {:?}",
        d.unexpected
    );
    assert!(d.errors.is_empty(), "errors: {:?}", d.errors);
}

#[test]
fn save_as_bae_and_baez_round_trip_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let mut h = gui();
    animated_project(&mut h);
    let before = export(&h);
    assert!(title(&h).ends_with(" *"), "no unsaved mark: {}", title(&h));

    // Save As without an extension: a `.bae` folder is written.
    save_as(&mut h, dir.join("动画 工程 v1.2"));
    let bae = dir.join("动画 工程 v1.2.bae");
    no_surprises(&h);
    assert!(
        bae.join("project.json").is_file(),
        "{} not written",
        bae.display()
    );
    assert_eq!(h.state().doc.location(), Some(bae.as_path()));
    assert_eq!(title(&h), "BAE - 动画 工程 v1.2.bae", "mark not cleared");
    assert_eq!(h.state().dialogs.suggested, ["Untitled Project.bae"]);
    assert_eq!(export_file(&bae), before, ".bae round trip differs");

    // Reopen through File > Open: identical, clean.
    h.state_mut().dialogs.open.push_back(Some(bae.clone()));
    key(&mut h, Modifiers::COMMAND, Key::O);
    no_surprises(&h);
    assert_eq!(export(&h), before);
    assert!(!h.state().doc.is_dirty());

    // Save As a `.baez` package next to it.
    save_as(&mut h, dir.join("打包 文件.baez"));
    let baez = dir.join("打包 文件.baez");
    no_surprises(&h);
    assert!(baez.is_file());
    assert_eq!(export_file(&baez), before, ".baez round trip differs");

    // Reopened projects show no comp (the session belongs to the old project);
    // open it from the Project panel (double-click), as in AE.
    assert!(h.state().active_comp().is_none());
    let comp = find_text(&h, "Comp 1").expect("comp in the Project panel");
    double_click_at(&mut h, comp.center());
    assert!(
        h.state().active_comp().is_some(),
        "double-click did not open the comp"
    );
    // An edit marks the project; Cmd+S writes the package in place, silently.
    key(&mut h, Modifiers::COMMAND, Key::Y);
    assert!(title(&h).ends_with(" *"));
    let suggested = h.state().dialogs.suggested.len();
    key(&mut h, Modifiers::COMMAND, Key::S);
    no_surprises(&h);
    assert_eq!(
        h.state().dialogs.suggested.len(),
        suggested,
        "Cmd+S showed a dialog"
    );
    assert!(!title(&h).ends_with(" *"));
    let after = export(&h);
    assert_ne!(after, before);
    assert_eq!(export_file(&baez), after, "Cmd+S did not write the edit");
    // The `.bae` saved before is untouched.
    assert_eq!(export_file(&bae), before);
}

#[test]
fn cmd_s_while_renaming_saves_the_typed_name() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("typing.bae");
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    save_as(&mut h, path.clone());
    no_surprises(&h);

    // Rename field open, a new name typed, Cmd+S before Enter.
    let head = find_text(&h, "Layer Name").unwrap();
    let row = find_texts(&h, |t| t == "Solid 1")
        .into_iter()
        .find(|r| r.top() > head.bottom())
        .unwrap();
    double_click_at(&mut h, row.center());
    press(&mut h, Modifiers::COMMAND, Key::A);
    send(&mut h, Event::Text("Typed".into()));
    press(&mut h, Modifiers::COMMAND, Key::S);
    settle(&mut h);
    no_surprises(&h);
    assert_eq!(h.state().layer_names(), ["Typed"]);
    assert!(
        !h.state().doc.is_dirty(),
        "the save ran before the rename committed"
    );
    let mut check = Document::new(None);
    check.open(&path).unwrap();
    let layer = h.state().layers()[0].clone();
    assert_eq!(
        bae_motion::query::display_name_of(check.stage(), &layer),
        "Typed"
    );
}

/// How the project goes away.
#[derive(Clone, Copy, Debug)]
enum Away {
    Close,
    Quit,
    New,
    Open,
}

fn go_away(h: &mut Ui, away: Away, other: &Path) {
    match away {
        Away::Close => {
            assert!(h.state_mut().menu("shell.file.close_project"));
            settle(h);
        }
        Away::Quit => {
            // Cmd+Q (native menu item) and the window's close button both queue Quit.
            h.state_mut().invoke(Invocation::new(ids::QUIT));
            settle(h);
        }
        Away::New => key(h, Modifiers::COMMAND | Modifiers::ALT, Key::N),
        Away::Open => {
            h.state_mut().dialogs.open.push_back(Some(other.to_owned()));
            key(h, Modifiers::COMMAND, Key::O);
        }
    }
}

#[test]
fn unsaved_changes_question_on_close_quit_new_and_open() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    // The project opened instead (File > Open).
    let other = dir.join("其他 工程.bae");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.null"]);
        save_as(&mut h, other.clone());
        no_surprises(&h);
    }
    let other_export = export_file(&other);

    for away in [Away::Close, Away::Quit, Away::New, Away::Open] {
        for choice in [
            UnsavedChoice::Cancel,
            UnsavedChoice::DontSave,
            UnsavedChoice::Save,
        ] {
            let case = format!("{away:?} / {choice:?}");
            let path = dir.join(format!("{away:?} {choice:?} 工程.bae"));
            let mut h = gui();
            comp_with(&mut h, &["motion.layer.new.solid"]);
            save_as(&mut h, path.clone());
            let saved = export(&h);
            // An unsaved edit.
            key(&mut h, Modifiers::COMMAND, Key::Y);
            let edited = export(&h);
            assert!(h.state().doc.is_dirty(), "{case}");

            h.state_mut().dialogs.answers.push_back(choice);
            go_away(&mut h, away, &other);
            let g = h.state();
            assert_eq!(
                g.dialogs.asked,
                [path.file_name().unwrap().to_string_lossy().into_owned()],
                "{case}: question"
            );
            assert!(
                g.dialogs.unexpected.is_empty(),
                "{case}: {:?}",
                g.dialogs.unexpected
            );
            assert!(
                g.dialogs.errors.is_empty(),
                "{case}: {:?}",
                g.dialogs.errors
            );
            let on_disk = export_file(&path);
            match choice {
                UnsavedChoice::Cancel => {
                    assert_eq!(export(&h), edited, "{case}: project changed");
                    assert!(g.doc.is_dirty(), "{case}");
                    assert!(!g.quit, "{case}: quit anyway");
                    assert_eq!(on_disk, saved, "{case}: file written");
                }
                UnsavedChoice::DontSave | UnsavedChoice::Save => {
                    let want_disk = if choice == UnsavedChoice::Save {
                        &edited
                    } else {
                        &saved
                    };
                    assert_eq!(&on_disk, want_disk, "{case}: file on disk");
                    match away {
                        Away::Quit => assert!(g.quit, "{case}: did not quit"),
                        Away::Open => assert_eq!(export(&h), other_export, "{case}"),
                        Away::Close | Away::New => {
                            assert!(g.doc.location().is_none(), "{case}");
                            assert!(g.layers().is_empty() && g.active_comp().is_none(), "{case}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn untitled_project_save_at_the_question_asks_where() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    // Close → Save → the Save As dialog is cancelled: nothing closes.
    h.state_mut().dialogs.answers.push_back(UnsavedChoice::Save);
    h.state_mut().dialogs.save.push_back(None);
    go_away(&mut h, Away::Close, tmp.path());
    assert_eq!(h.state().layer_names(), ["Solid 1"]);
    assert!(h.state().doc.is_dirty());
    // Quit → Save → a path: written, then the app quits.
    let path = tmp.path().join("退出 前 保存");
    h.state_mut().dialogs.answers.push_back(UnsavedChoice::Save);
    h.state_mut().dialogs.save.push_back(Some(path.clone()));
    go_away(&mut h, Away::Quit, tmp.path());
    assert!(h.state().quit);
    assert!(
        tmp.path()
            .join("退出 前 保存.bae")
            .join("project.json")
            .is_file()
    );
    // A clean project closes without a question.
    let mut h = gui();
    go_away(&mut h, Away::Close, tmp.path());
    go_away(&mut h, Away::Quit, tmp.path());
    assert!(h.state().dialogs.asked.is_empty());
    assert!(h.state().quit);
}

#[test]
fn read_only_folder_reports_and_keeps_the_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("只读 目录");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let edited = export(&h);

    for name in ["工程.bae", "工程.baez"] {
        save_as(&mut h, locked.join(name));
        let g = h.state();
        assert_eq!(g.dialogs.errors.len(), 1, "{name}: {:?}", g.dialogs.errors);
        let message = &g.dialogs.errors[0];
        assert!(message.contains("Could not save the project."), "{message}");
        assert!(
            message.contains("只读 目录"),
            "message does not name the folder: {message}"
        );
        assert!(g.doc.is_dirty() && g.doc.location().is_none());
        assert_eq!(export(&h), edited);
        h.state_mut().dialogs.errors.clear();
    }
    // Quitting with Save into the read-only folder keeps the app open.
    h.state_mut().dialogs.answers.push_back(UnsavedChoice::Save);
    h.state_mut()
        .dialogs
        .save
        .push_back(Some(locked.join("q.bae")));
    go_away(&mut h, Away::Quit, tmp.path());
    assert!(!h.state().quit, "quit after a failed save: changes lost");
    assert_eq!(export(&h), edited);
    // Nothing was left behind in the folder.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(fs::read_dir(&locked).unwrap().count(), 0);

    // A saved project whose folder became read-only: Cmd+S reports, stays dirty.
    let path = tmp.path().join("later locked.bae");
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    save_as(&mut h, path.clone());
    key(&mut h, Modifiers::COMMAND, Key::Y);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o555)).unwrap();
    fs::set_permissions(path.join("project.json"), fs::Permissions::from_mode(0o444)).unwrap();
    key(&mut h, Modifiers::COMMAND, Key::S);
    let errors = h.state().dialogs.errors.clone();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(h.state().doc.is_dirty());
}

#[test]
fn save_as_over_existing_projects_replaces_them() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let bae = dir.join("已有 工程.bae");
    let baez = dir.join("已有 工程.baez");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.null", "motion.layer.new.null"]);
        save_as(&mut h, bae.clone());
        save_as(&mut h, baez.clone());
        no_surprises(&h);
    }
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mine = export(&h);
    // The OS save panel asks "Replace?"; answering Replace returns the path.
    save_as(&mut h, bae.clone());
    save_as(&mut h, baez.clone());
    no_surprises(&h);
    assert_eq!(export_file(&bae), mine, ".bae not replaced");
    assert_eq!(export_file(&baez), mine, ".baez not replaced");
    // No stray staging folders or temporary files next to them.
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names
            .iter()
            .all(|n| n == "已有 工程.bae" || n == "已有 工程.baez"),
        "leftovers: {names:?}"
    );
}

#[test]
fn broken_files_are_reported_without_touching_the_open_project() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    // A valid package to damage.
    let good = dir.join("好的.baez");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.solid"]);
        save_as(&mut h, good.clone());
        no_surprises(&h);
    }
    let bytes = fs::read(&good).unwrap();
    let mut cases: Vec<(&str, PathBuf)> = Vec::new();
    let mut add = |label: &'static str, path: PathBuf| cases.push((label, path));

    let p = dir.join("随机 字节.baez");
    fs::write(
        &p,
        (0..4096u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 24) as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    add("random bytes .baez", p);
    let p = dir.join("截断.baez");
    fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    add("truncated .baez", p);
    let p = dir.join("空文件.baez");
    fs::write(&p, b"").unwrap();
    add("empty .baez", p);
    let p = dir.join("坏 json.bae");
    fs::create_dir(&p).unwrap();
    fs::write(
        p.join("project.json"),
        b"{\"format\": \"bae.project\", \"vers",
    )
    .unwrap();
    add("truncated project.json", p);
    let p = dir.join("错误 结构.bae");
    fs::create_dir(&p).unwrap();
    fs::write(p.join("project.json"), b"{\"hello\": [1, 2, 3]}").unwrap();
    add("wrong JSON schema", p);
    let p = dir.join("未来 版本.bae");
    fs::create_dir(&p).unwrap();
    fs::write(
        p.join("project.json"),
        b"{\"format\": \"bae.project\", \"format_version\": 999, \"sublayers\": []}",
    )
    .unwrap();
    add("future format version", p);
    let p = dir.join("空目录.bae");
    fs::create_dir(&p).unwrap();
    add("empty .bae folder", p);
    add("missing path", dir.join("不存在.bae"));
    let p = dir.join("二进制 project.bae");
    fs::create_dir(&p).unwrap();
    fs::write(p.join("project.json"), [0xffu8, 0xfe, 0x00, 0x01, 0x80]).unwrap();
    add("binary project.json", p);

    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mine = export(&h);
    let mut report = Vec::new();
    for (label, path) in &cases {
        h.state_mut().dialogs.open.push_back(Some(path.clone()));
        key(&mut h, Modifiers::COMMAND, Key::O);
        let g = h.state();
        assert_eq!(g.dialogs.errors.len(), 1, "{label}: {:?}", g.dialogs.errors);
        let message = g.dialogs.errors[0].clone();
        assert!(
            message.contains("Could not open the project."),
            "{label}: {message}"
        );
        let file = path.file_name().unwrap().to_string_lossy();
        assert!(
            message.contains(&*file),
            "{label}: message does not name the file: {message}"
        );
        assert!(
            g.dialogs.asked.is_empty(),
            "{label}: asked before reading the file"
        );
        assert_eq!(export(&h), mine, "{label}: open project changed");
        assert!(g.doc.is_dirty());
        report.push(format!("{label}: {}", message.replace('\n', " ")));
        h.state_mut().dialogs.errors.clear();
    }
    println!("ERROR MESSAGES\n{}", report.join("\n"));
    // The app still works.
    key(&mut h, Modifiers::COMMAND, Key::Y);
    assert_eq!(h.state().layers().len(), 2);
    let _ = pos2(0.0, 0.0);
}

#[test]
#[ignore = "issue #113"]
fn reopening_shows_the_comp_that_was_open() {
    // AE reopens the compositions whose Timeline panels were open when the
    // project was saved; BAE opens the project with an empty Timeline and
    // viewer, which looks like the content was lost.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("reopen.bae");
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    save_as(&mut h, path.clone());
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::N);
    h.state_mut().dialogs.open.push_back(Some(path));
    key(&mut h, Modifiers::COMMAND, Key::O);
    assert!(
        h.state().active_comp().is_some(),
        "no comp open after reopening"
    );
    assert!(find_text(&h, "Solid 1").is_some());
}
