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

    // The comp that was open when it was saved comes back (issue #113); a
    // double-click in the Project panel still opens it, as in AE.
    assert!(
        h.state().active_comp().is_some(),
        "the comp that was open is shown again"
    );
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

/// Projects opened from Finder (double-click, Dock drop: `application:openURLs:`, see
/// `shell::open_documents`) go through `file.open_project` like File > Open: the
/// unsaved-changes question first, `.bae` and `.baez` alike, the comp that was open when
/// the project was saved comes back (#113's restore), only the first of several paths,
/// and a file that is not a project is reported without touching the open one (#115).
#[test]
fn finder_open_goes_through_the_open_flow() {
    use crate::shell::open_files_invocation;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let folder = dir.join("Finder 打开.bae");
    let packed = dir.join("Finder 打开.baez");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.null"]);
        save_as(&mut h, folder.clone());
        save_as(&mut h, packed.clone());
        no_surprises(&h);
    }
    let opened_export = export_file(&folder);
    assert!(open_files_invocation(&[]).is_none());

    for (i, (target, choice)) in [
        (&folder, UnsavedChoice::Cancel),
        (&folder, UnsavedChoice::DontSave),
        (&packed, UnsavedChoice::DontSave),
    ]
    .into_iter()
    .enumerate()
    {
        let case = format!("{} / {choice:?}", target.display());
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.solid"]);
        // A new name per case: saving over an existing project asks first (#126).
        let mine = dir.join(format!("当前 {choice:?} {i}.bae"));
        save_as(&mut h, mine.clone());
        key(&mut h, Modifiers::COMMAND, Key::Y); // an unsaved edit
        let edited = export(&h);
        assert!(h.state().doc.is_dirty(), "{case}");

        h.state_mut().dialogs.answers.push_back(choice);
        // Finder sent two; BAE opens one project at a time.
        let inv = open_files_invocation(&[target.clone(), dir.join("第二个.bae")]).unwrap();
        h.state_mut().invoke(inv);
        settle(&mut h);

        let g = h.state();
        assert_eq!(
            g.dialogs.asked,
            [mine.file_name().unwrap().to_string_lossy().into_owned()],
            "{case}: question"
        );
        no_surprises(&h);
        match choice {
            UnsavedChoice::Cancel => {
                assert_eq!(export(&h), edited, "{case}");
                assert!(h.state().doc.is_dirty(), "{case}");
            }
            _ => {
                assert_eq!(export(&h), opened_export, "{case}");
                assert_eq!(h.state().doc.location(), Some(target.as_path()), "{case}");
                assert!(!h.state().doc.is_dirty(), "{case}");
                assert!(
                    h.state().active_comp().is_some(),
                    "{case}: the saved comp was not reopened"
                );
                assert!(
                    find_text(&h, "Null 1").is_some(),
                    "{case}: its layer is not shown"
                );
            }
        }
    }

    // Not a project: reported, the open project stays.
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let before = export(&h);
    let notes = dir.join("说明.txt");
    fs::write(&notes, "not a project").unwrap();
    h.state_mut()
        .invoke(open_files_invocation(std::slice::from_ref(&notes)).unwrap());
    settle(&mut h);
    assert_eq!(
        h.state().dialogs.errors.len(),
        1,
        "{:?}",
        h.state().dialogs.errors
    );
    assert_eq!(export(&h), before);
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
        // Issue #111: our own words, not the system's "Permission denied".
        assert!(
            message.contains("You don't have permission to save in"),
            "{message}"
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
    // A .bae is a folder on Windows (and in a bare binary on macOS): BAE asks
    // "Replace?" itself (issue #126). For the .baez file the OS save panel asks.
    h.state_mut().dialogs.replace.push_back(true);
    save_as(&mut h, bae.clone());
    save_as(&mut h, baez.clone());
    no_surprises(&h);
    assert_eq!(h.state().dialogs.asked_replace, ["已有 工程.bae"]);
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

/// Issue #126: Windows save dialogs walk into an existing `.bae` folder; a path
/// inside a project (or the project folder itself) means replacing that
/// project, after asking — never a project nested inside another.
#[test]
fn save_as_into_an_existing_project_folder_replaces_it_after_asking() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let existing = dir.join("e_ow2.bae");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.null"]);
        save_as(&mut h, existing.clone());
        no_surprises(&h);
    }
    let old = export_file(&existing);
    let mut h = gui();
    comp_with(
        &mut h,
        &["motion.layer.new.solid", "motion.layer.new.solid"],
    );
    let mine = export(&h);

    // The dialog walked into the folder and the user clicked Save again:
    // e_ow2.bae/e_ow2.bae. Cancel at BAE's question: nothing changes.
    let nested = existing.join("e_ow2.bae");
    h.state_mut().dialogs.replace.push_back(false);
    save_as(&mut h, nested.clone());
    assert_eq!(h.state().dialogs.asked_replace, ["e_ow2.bae"]);
    assert_eq!(export_file(&existing), old, "cancel changed the project");
    assert!(!nested.exists(), "a project was nested inside another");
    assert!(h.state().doc.is_dirty() && h.state().doc.location().is_none());

    // The same, answering Replace: the outer project is replaced.
    h.state_mut().dialogs.replace.push_back(true);
    save_as(&mut h, nested.clone());
    no_surprises(&h);
    assert_eq!(export_file(&existing), mine, "not replaced");
    assert!(!nested.exists(), "a project was nested inside another");
    assert_eq!(h.state().doc.location(), Some(existing.as_path()));
    assert!(!h.state().doc.is_dirty());
    // Any other name inside it means the same project.
    key(&mut h, Modifiers::COMMAND, Key::Y);
    let edited = export(&h);
    save_as(&mut h, existing.join("sublayers").join("别的 名字.bae"));
    no_surprises(&h);
    assert_eq!(
        h.state().dialogs.asked_replace.len(),
        2,
        "saving into the open project's own folder asks nothing"
    );
    assert_eq!(export_file(&existing), edited);
    assert!(!existing.join("sublayers").join("别的 名字.bae").exists());

    // A folder with a project's name that is not a project is refused.
    let foreign = dir.join("别人的.bae");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("notes.txt"), "keep me").unwrap();
    save_as(&mut h, foreign.clone());
    let errors = std::mem::take(&mut h.state_mut().dialogs.errors);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("别人的.bae"), "{}", errors[0]);
    assert_eq!(
        fs::read_to_string(foreign.join("notes.txt")).unwrap(),
        "keep me"
    );
    assert!(!foreign.join("project.json").exists());

    // No staging or backup folders are left next to the project.
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !n.starts_with('.')),
        "leftovers: {names:?}"
    );
}

/// Issue #126: replacing goes through a temporary folder; when it fails the
/// existing project is left exactly as it was.
#[cfg(unix)]
#[test]
fn a_failed_replace_leaves_the_existing_project() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("只读 上级");
    fs::create_dir(&dir).unwrap();
    let existing = dir.join("原来的.bae");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.null"]);
        save_as(&mut h, existing.clone());
        no_surprises(&h);
    }
    let old = export_file(&existing);
    // The parent folder becomes read-only: no temporary folder can be made.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    h.state_mut().dialogs.replace.push_back(true);
    save_as(&mut h, existing.clone());
    let errors = std::mem::take(&mut h.state_mut().dialogs.errors);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    if std::env::var("USER").is_ok_and(|u| u == "root") {
        return;
    }
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("Could not save the project."),
        "{}",
        errors[0]
    );
    assert_eq!(export_file(&existing), old, "the existing project changed");
    assert!(h.state().doc.is_dirty() && h.state().doc.location().is_none());
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
        // Issue #111: say what is wrong, not only that it failed.
        let why = match *label {
            "missing path" => "does not exist",
            "empty .bae folder" => "has no project.json",
            l if l.ends_with(".baez") => "is damaged or is not a .baez project package",
            _ => "(project.json) in",
        };
        assert!(message.contains(why), "{label}: {message}");
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

// ---- Retest of #113 / #111 (dev d23a328): edge cases ----

/// Two comps open (tabs), different times, the second active; saved.
fn two_comps_saved(
    h: &mut Ui,
    path: &Path,
) -> (PrimPath, PrimPath, bae_model::Time, bae_model::Time) {
    comp_with(h, &["motion.layer.new.solid"]);
    let first = h.state().active_comp().unwrap();
    for _ in 0..15 {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    settle(h);
    let t1 = h.state().now();
    comp_with(h, &["motion.layer.new.null"]);
    let second = h.state().active_comp().unwrap();
    for _ in 0..45 {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    settle(h);
    let t2 = h.state().now();
    save_as(h, path.to_owned());
    no_surprises(h);
    (first, second, t1, t2)
}

fn reopen(h: &mut Ui, path: &Path) {
    key(h, Modifiers::COMMAND | Modifiers::ALT, Key::N);
    h.state_mut().dialogs.open.push_back(Some(path.to_owned()));
    key(h, Modifiers::COMMAND, Key::O);
    no_surprises(h);
}

#[test]
fn reopening_restores_every_open_comp_its_time_and_the_active_one() {
    let tmp = tempfile::tempdir().unwrap();
    for name in ["两个 合成.bae", "两个 合成.baez"] {
        let path = tricky_dir(tmp.path()).join(name);
        let mut h = gui();
        let (first, second, t1, t2) = two_comps_saved(&mut h, &path);
        reopen(&mut h, &path);
        let s = h.state().session();
        let s = s.lock();
        assert_eq!(
            s.open_comps,
            [first.clone(), second.clone()],
            "{name}: tabs"
        );
        assert_eq!(s.active_comp.as_ref(), Some(&second), "{name}: active comp");
        assert_eq!(s.time(&first), t1, "{name}: time of the first comp");
        assert_eq!(s.time(&second), t2, "{name}: time of the active comp");
        drop(s);
        // The timeline shows the active comp's layers.
        assert!(
            find_text(&h, "Null 1").is_some(),
            "{name}: {:?}",
            painted_texts(&h)
        );
    }
}

#[test]
fn view_changes_do_not_mark_the_project_changed_or_add_undo_steps() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("视图.bae");
    let mut h = gui();
    two_comps_saved(&mut h, &path);
    reopen(&mut h, &path);
    assert!(!h.state().doc.is_dirty());
    let undo_before = h.state().doc.undo_state();
    // Move the time, play, switch comps (double-click in the Project panel).
    for _ in 0..10 {
        press(&mut h, Modifiers::NONE, Key::PageDown);
    }
    key(&mut h, Modifiers::NONE, Key::Home);
    let comp = find_text(&h, "Comp 1").expect("Comp 1 in the Project panel");
    double_click_at(&mut h, comp.center());
    assert!(
        !h.state().doc.is_dirty(),
        "view changes marked the project as changed"
    );
    assert_eq!(
        h.state().doc.undo_state(),
        undo_before,
        "view changes became undo steps"
    );
    // Closing asks nothing (no data changed).
    assert!(h.state_mut().menu("shell.file.close_project"));
    settle(&mut h);
    assert!(
        h.state().dialogs.asked.is_empty(),
        "{:?}",
        h.state().dialogs.asked
    );
}

/// Replace the `view` record in a saved `.bae`'s project.json.
fn set_view(path: &Path, view: Json) {
    let manifest = path.join("project.json");
    let mut json: Json = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    json["view"] = view;
    fs::write(&manifest, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
}

#[test]
fn a_broken_or_stale_view_record_falls_back_to_the_first_comp() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("视图 记录.bae");
    let mut h = gui();
    let (first, _second, _, _) = two_comps_saved(&mut h, &path);
    let cases = [
        (
            "comps that no longer exist",
            json!({"comps": [{"path": "/comps/Gone", "time": 0}], "active": "/comps/Gone"}),
        ),
        ("not an object", json!(42)),
        ("wrong field types", json!({"comps": "nope", "active": 7})),
        (
            "time far past the end",
            json!({"comps": [{"path": first, "time": 9_000_000_000_000_000i64}], "active": first}),
        ),
        (
            "negative time",
            json!({"comps": [{"path": first, "time": -705_600_000}], "active": first}),
        ),
        ("removed (old project)", Json::Null),
    ];
    for (label, view) in cases {
        if view.is_null() {
            let manifest = path.join("project.json");
            let mut json: Json = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
            json.as_object_mut().unwrap().remove("view");
            fs::write(&manifest, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
        } else {
            set_view(&path, view);
        }
        // The previous case left an edit (Cmd+Y below): discard it.
        if h.state().doc.is_dirty() {
            h.state_mut()
                .dialogs
                .answers
                .push_back(UnsavedChoice::DontSave);
        }
        reopen(&mut h, &path);
        let active = h.state().active_comp();
        assert_eq!(
            active.as_ref(),
            Some(&first),
            "{label}: expected the first comp"
        );
        let info = bae_motion::query::comp_info(h.state().stage(), &first).unwrap();
        let now = h.state().now();
        assert!(
            now >= bae_model::Time::ZERO && now <= info.duration,
            "{label}: time {now:?} outside the comp"
        );
        // The project still works.
        key(&mut h, Modifiers::COMMAND, Key::Y);
        assert!(h.state().doc.is_dirty(), "{label}");
    }
}

#[test]
fn broken_files_are_explained_in_chinese_too() {
    if !in_child(
        "acceptance::files_flow::broken_files_are_explained_in_chinese_too",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let good = dir.join("好的.bae");
    {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.solid"]);
        save_as(&mut h, good.clone());
    }
    let copy = |name: &str| {
        let p = dir.join(name);
        let status = std::process::Command::new("cp")
            .arg("-R")
            .arg(&good)
            .arg(&p)
            .status()
            .unwrap();
        assert!(status.success());
        p
    };
    let mut cases: Vec<(&str, PathBuf, &str)> = Vec::new();
    cases.push(("missing", dir.join("不存在.bae"), "不存在"));
    let p = dir.join("空目录.bae");
    fs::create_dir(&p).unwrap();
    cases.push(("empty folder", p, "没有 project.json"));
    let p = dir.join("坏包.baez");
    fs::write(&p, b"PK\x03\x04 nope").unwrap();
    cases.push(("damaged baez", p, "已损坏，或者不是 .baez 项目包"));
    let p = copy("新版本.bae");
    let manifest = p.join("project.json");
    let mut json: Json = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    json["version"] = json!(999);
    fs::write(&manifest, serde_json::to_vec(&json).unwrap()).unwrap();
    cases.push(("newer version", p, "更新版本的 BAE 保存的"));
    let p = copy("缺层.bae");
    fs::remove_dir_all(p.join("sublayers")).unwrap();
    cases.push(("missing sublayer data", p, "数据缺失"));
    let p = copy("坏层.bae");
    for e in fs::read_dir(p.join("sublayers")).unwrap() {
        fs::write(e.unwrap().path(), b"\x00\x01garbage").unwrap();
    }
    cases.push(("damaged sublayer data", p, "已损坏"));
    let p = copy("无权限.bae");
    fs::set_permissions(p.join("project.json"), fs::Permissions::from_mode(0o000)).unwrap();
    // Issue #111: permission problems in our own words, not the system's.
    cases.push(("unreadable project.json", p, "没有权限打开"));

    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mut report = Vec::new();
    for (label, path, why) in &cases {
        h.state_mut().dialogs.open.push_back(Some(path.clone()));
        key(&mut h, Modifiers::COMMAND, Key::O);
        let errors = std::mem::take(&mut h.state_mut().dialogs.errors);
        assert_eq!(errors.len(), 1, "{label}: {errors:?}");
        let m = &errors[0];
        report.push(format!("{label}: {}", m.replace('\n', " ⏎ ")));
        assert!(m.starts_with("无法打开项目。"), "{label}: {m}");
        assert!(
            m.contains(&*path.file_name().unwrap().to_string_lossy()),
            "{label}: no file name: {m}"
        );
        assert!(m.contains(why), "{label}: no reason {why:?}: {m}");
        // A missing file needs no technical detail; the others add one.
        if *label != "missing" {
            assert!(m.contains("详细信息："), "{label}: {m}");
        }
    }
    let _ = fs::set_permissions(
        dir.join("无权限.bae/project.json"),
        fs::Permissions::from_mode(0o644),
    );
    println!("ZH ERRORS\n{}", report.join("\n"));
}

// ---- Retest of #131 / #111 (dev 277f11a) ----

#[test]
fn a_packed_project_reopens_with_its_comps() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let bae = dir.join("打包 前.bae");
    let mut h = gui();
    let (first, second, t1, t2) = two_comps_saved(&mut h, &bae);
    // Pack it with `bae pack`'s own function, then open the package.
    let baez = dir.join("打包 后.baez");
    crate::pack_cli::pack(
        &bae,
        &baez,
        &bae_package::PackOptions::default(),
        bae_package::WriteOptions::default(),
    )
    .unwrap();
    reopen(&mut h, &baez);
    let s = h.state().session();
    let s = s.lock();
    assert_eq!(s.open_comps, [first.clone(), second.clone()]);
    assert_eq!(s.active_comp.as_ref(), Some(&second));
    assert_eq!((s.time(&first), s.time(&second)), (t1, t2));
}

#[test]
fn save_errors_are_explained_in_chinese_too() {
    if !in_child(
        "acceptance::files_flow::save_errors_are_explained_in_chinese_too",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("只读 目录");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mut report = Vec::new();
    for (label, path, want) in [
        (
            "read-only folder",
            locked.join("工程.bae"),
            "没有权限保存到",
        ),
        (
            "missing folder",
            tmp.path().join("不存在 的 目录").join("工程.baez"),
            "不存在",
        ),
    ] {
        save_as(&mut h, path.clone());
        let errors = std::mem::take(&mut h.state_mut().dialogs.errors);
        assert_eq!(errors.len(), 1, "{label}: {errors:?}");
        let m = &errors[0];
        report.push(format!("{label}: {}", m.replace('\n', " ⏎ ")));
        assert!(m.starts_with("无法保存项目。"), "{label}: {m}");
        assert!(m.contains(want), "{label}: no {want:?}: {m}");
        // The sentence for the user is ours; the system's text may follow
        // only on the technical “详细信息：” line.
        let sentence: String = m
            .lines()
            .take_while(|l| !l.starts_with("详细信息"))
            .collect();
        assert!(
            !sentence.contains("Permission denied"),
            "{label}: system English: {m}"
        );
        assert!(h.state().doc.is_dirty());
    }
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    println!("ZH SAVE ERRORS\n{}", report.join("\n"));
}

#[test]
#[ignore = "manual (macOS): creates and ejects a 4 MB RAM disk"]
fn saving_to_a_full_disk_says_so() {
    let dev = std::process::Command::new("hdiutil")
        .args(["attach", "-nomount", "ram://8192"])
        .output()
        .unwrap();
    let dev = String::from_utf8_lossy(&dev.stdout).trim().to_owned();
    assert!(dev.starts_with("/dev/disk"), "{dev}");
    let ok = std::process::Command::new("diskutil")
        .args(["erasevolume", "HFS+", "BAESAVEFULL", &dev])
        .output()
        .unwrap();
    assert!(ok.status.success());
    let volume = PathBuf::from("/Volumes/BAESAVEFULL");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Fill the volume to the last byte (big blocks, then smaller ones).
        for (i, bs) in ["64k", "4k", "512", "1"].iter().enumerate() {
            let _ = std::process::Command::new("dd")
                .args([
                    "if=/dev/zero",
                    &format!("of={}/fill{i}", volume.display()),
                    &format!("bs={bs}"),
                ])
                .output();
        }
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
        let edited = bae_model::export_flattened_json(h.state().stage());
        save_as(&mut h, volume.join("满 盘.baez"));
        let errors = h.state().dialogs.errors.clone();
        println!("FULL DISK SAVE: {errors:?}");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("There is not enough space to save"),
            "{}",
            errors[0]
        );
        assert!(h.state().doc.is_dirty() && h.state().doc.location().is_none());
        assert_eq!(bae_model::export_flattened_json(h.state().stage()), edited);
        // Nothing half-written is left as the project file.
        assert!(
            !volume.join("满 盘.baez").exists(),
            "a partial package was left behind"
        );
    }));
    let _ = std::process::Command::new("hdiutil")
        .args(["detach", &dev])
        .output();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

// ---- Retest of #126 (dev 5d82c2a): Save As onto an existing project ----

/// A saved project with one null layer; returns its export.
fn existing_project(path: &Path) -> Json {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.null"]);
    save_as(&mut h, path.to_owned());
    no_surprises(&h);
    export_file(path)
}

fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn a_replace_that_fails_during_the_swap_leaves_the_existing_project() {
    // The existing project folder cannot be moved or removed (macOS immutable
    // flag): the temporary copy is written, the swap fails.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let existing = dir.join("锁住 的.bae");
    let old = existing_project(&existing);
    let before = entries(&dir);
    let chflags = |flag: &str| {
        let ok = std::process::Command::new("chflags")
            .args([flag])
            .arg(&existing)
            .status()
            .unwrap();
        assert!(ok.success(), "chflags {flag}");
    };
    chflags("uchg");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.solid"]);
        let mine = export(&h);
        h.state_mut().dialogs.replace.push_back(true);
        save_as(&mut h, existing.clone());
        let errors = h.state().dialogs.errors.clone();
        println!("SWAP FAILURE: {errors:?}");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("锁住 的.bae"), "{}", errors[0]);
        assert!(h.state().doc.is_dirty() && h.state().doc.location().is_none());
        assert_eq!(export(&h), mine, "the open project changed");
    }));
    chflags("nouchg");
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
    assert_eq!(export_file(&existing), old, "the existing project changed");
    assert_eq!(entries(&dir), before, "temporary folders were left behind");
}

#[test]
fn save_as_onto_the_open_projects_own_folder_saves_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tricky_dir(tmp.path()).join("自己.bae");
    existing_project(&path);
    let mut h = gui();
    h.state_mut().dialogs.open.push_back(Some(path.clone()));
    key(&mut h, Modifiers::COMMAND, Key::O);
    key(&mut h, Modifiers::COMMAND, Key::Y);
    let edited = export(&h);
    save_as(&mut h, path.clone());
    no_surprises(&h);
    assert!(
        h.state().dialogs.asked_replace.is_empty(),
        "asked to replace the project itself"
    );
    assert_eq!(export_file(&path), edited);
    assert_eq!(h.state().doc.location(), Some(path.as_path()));
    assert!(!h.state().doc.is_dirty());
    assert!(!path.join("自己.bae").exists());
}

#[test]
fn a_baez_name_or_other_case_inside_an_existing_project_never_nests() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let existing = dir.join("Case 工程.bae");
    existing_project(&existing);
    for (label, picked) in [
        ("a .baez name inside", existing.join("新的 包.baez")),
        (
            "a deeper path inside",
            existing.join("sublayers").join("x.bae"),
        ),
        (
            "the same name in other letter case",
            dir.join("case 工程.BAE"),
        ),
    ] {
        let mut h = gui();
        comp_with(&mut h, &["motion.layer.new.solid"]);
        let mine = export(&h);
        h.state_mut().dialogs.replace.push_back(true);
        save_as(&mut h, picked.clone());
        no_surprises(&h);
        assert_eq!(
            h.state().dialogs.asked_replace.len(),
            1,
            "{label}: not asked"
        );
        assert_eq!(export_file(&existing), mine, "{label}: not replaced");
        assert!(
            !picked.exists() || picked.parent() == existing.parent(),
            "{label}: nested {picked:?}"
        );
        let nested: Vec<_> = walk_names(&existing)
            .into_iter()
            .filter(|n| n.ends_with(".bae") || n.ends_with(".baez"))
            .collect();
        assert!(
            nested.is_empty(),
            "{label}: project nested inside: {nested:?}"
        );
        // One project folder only (case-insensitive file system).
        let projects: Vec<String> = entries(&dir)
            .into_iter()
            .filter(|n| n.to_lowercase().ends_with(".bae"))
            .collect();
        assert_eq!(projects.len(), 1, "{label}: {projects:?}");
    }
}

fn walk_names(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            out.push(e.file_name().to_string_lossy().into_owned());
            if e.path().is_dir() {
                out.extend(walk_names(&e.path()));
            }
        }
    }
    out
}

#[test]
fn declining_the_replace_while_quitting_keeps_the_app_open() {
    let tmp = tempfile::tempdir().unwrap();
    let existing = tricky_dir(tmp.path()).join("退出 时.bae");
    let old = existing_project(&existing);
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mine = export(&h);
    // Quit → Save → Save As picks the existing project → Don't replace.
    h.state_mut().dialogs.answers.push_back(UnsavedChoice::Save);
    h.state_mut().dialogs.save.push_back(Some(existing.clone()));
    h.state_mut().dialogs.replace.push_back(false);
    h.state_mut().invoke(Invocation::new(ids::QUIT));
    settle(&mut h);
    no_surprises(&h);
    assert!(!h.state().quit, "quit although the save was cancelled");
    assert_eq!(export(&h), mine);
    assert!(h.state().doc.is_dirty());
    assert_eq!(export_file(&existing), old);
}

/// Dialogs like BAE.app's on macOS: the save panel itself asked "Replace?".
struct NativeLike {
    path: PathBuf,
    asked: Vec<String>,
    errors: Vec<String>,
}

impl FileDialogs for NativeLike {
    fn pick_project(&mut self) -> Option<PathBuf> {
        None
    }
    fn pick_save_path(&mut self, _: &str, _: Option<&Path>) -> Option<PathBuf> {
        Some(self.path.clone())
    }
    fn ask_unsaved(&mut self, _: &str) -> UnsavedChoice {
        UnsavedChoice::Cancel
    }
    fn confirm_replace(&mut self, name: &str) -> bool {
        self.asked.push(name.to_owned());
        false
    }
    fn save_dialog_confirms_replace(&self) -> bool {
        true
    }
    fn error(&mut self, message: &str) {
        self.errors.push(message.to_owned());
    }
}

#[test]
fn the_mac_save_panels_replace_question_is_not_asked_twice_but_nested_paths_still_are() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    let existing = dir.join("面板 问过.bae");
    existing_project(&existing);
    let mut doc = Document::new(None);
    doc.run(&Invocation::with_params(
        "prim.create",
        json!({"path": "/Mine", "prim_type": "Comp"}),
    ));
    // Picked the project itself: the panel already asked; BAE replaces.
    let mut d = NativeLike {
        path: existing.clone(),
        asked: vec![],
        errors: vec![],
    };
    assert_eq!(files::save_as(&mut doc, &mut d), files::FileOutcome::Saved);
    assert!(d.asked.is_empty(), "asked again after the panel");
    // A path inside it (the panel did not know it is a project): BAE asks;
    // declined, nothing changes.
    let mut doc = Document::new(None);
    doc.run(&Invocation::with_params(
        "prim.create",
        json!({"path": "/Other", "prim_type": "Comp"}),
    ));
    let before = export_file(&existing);
    let mut d = NativeLike {
        path: existing.join("inner.bae"),
        asked: vec![],
        errors: vec![],
    };
    assert_eq!(
        files::save_as(&mut doc, &mut d),
        files::FileOutcome::Cancelled
    );
    assert_eq!(d.asked, ["面板 问过.bae"]);
    assert_eq!(export_file(&existing), before);
}

#[test]
fn replace_texts_are_translated() {
    for en in [
        "“{}” already exists. Do you want to replace it?",
        "The existing project is replaced. This cannot be undone.",
        "Replace",
        "“{file}” is a folder but not a BAE project. Choose another name.",
    ] {
        crate::i18n::register_tables();
        assert!(bae_ui::i18n::has_translation(en), "no Chinese for {en:?}");
    }
}
