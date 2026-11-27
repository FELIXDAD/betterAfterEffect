//! Motion dialogs driven like a user: Composition Settings (Cmd/Ctrl+N,
//! Cmd/Ctrl+K), Solid Settings (Cmd/Ctrl+Y), Camera and Light Settings, and
//! Layer Settings (Cmd/Ctrl+Shift+Y). Controls are found by their accesskit
//! labels; values are typed.

use egui_kittest::kittest::Queryable as _;

use super::*;

/// The control labeled `label`; when panels behind the dialog use the same
/// label (the timeline's Zoom), the one nearest the window center, where
/// dialogs open.
fn dialog_control(h: &Ui, label: &str) -> egui::Rect {
    let center = egui::pos2(SIZE[0] / 2.0, SIZE[1] / 2.0);
    h.query_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| {
            let (da, db) = (a.center().distance(center), b.center().distance(center));
            da.total_cmp(&db)
        })
        .unwrap_or_else(|| panic!("no control labeled {label:?}"))
}

/// Click a hot number / field by its label, select its text and type `text`;
/// Tab commits it (the dialog stays open).
fn type_into(h: &mut Ui, label: &str, text: &str) {
    let rect = dialog_control(h, label);
    click_at(h, rect.center());
    key(h, Modifiers::COMMAND, Key::A);
    type_text(h, text);
    key(h, Modifiers::NONE, Key::Tab);
}

fn click_label(h: &mut Ui, label: &str) {
    let rect = h.get_by_label(label).rect();
    click_at(h, rect.center());
}

#[test]
fn new_composition_asks_for_settings_first() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    assert!(dialog_open(&h), "Cmd+N opened no dialog");
    assert!(find_text(&h, "Composition Settings").is_some());
    assert!(h.state().active_comp().is_none(), "created before OK");
    // The name is selected: typing replaces it.
    type_text(&mut h, "Main");
    type_into(&mut h, "Width", "1280");
    type_into(&mut h, "Height", "720");
    // OK and Cancel sit where the platform puts them.
    let ok = h.get_by_label("OK").rect();
    let cancel = h.get_by_label("Cancel").rect();
    if cfg!(target_os = "macos") {
        assert!(ok.center().x > cancel.center().x, "macOS: Cancel, OK");
    } else {
        assert!(ok.center().x < cancel.center().x, "Windows: OK, Cancel");
    }
    click_label(&mut h, "OK");
    assert!(!dialog_open(&h));
    let comp = h.state().active_comp().expect("OK opened the comp");
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!(info.name, "Main");
    assert_eq!((info.width, info.height), (1280, 720));
    // One undo step removes it again.
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("New Composition"));
}

#[test]
fn cancel_escape_and_blocked_shortcuts() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    click_label(&mut h, "Cancel");
    assert!(!dialog_open(&h));
    assert!(bae_motion::query::comps(h.state().stage()).is_empty());
    // While it is open, shortcuts and menu commands do not reach the project.
    key(&mut h, Modifiers::COMMAND, Key::N);
    assert!(h.state().workspace.modal_open(h.state().app.resources()));
    key(&mut h, Modifiers::COMMAND, Key::Y);
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::N);
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!dialog_open(&h));
    assert!(bae_motion::query::comps(h.state().stage()).is_empty());
    assert!(!h.state().doc.is_dirty());
    // Shortcuts work again afterwards.
    new_comp(&mut h);
    assert!(h.state().active_comp().is_some());
}

#[test]
fn composition_settings_edits_the_open_comp_in_one_undo_step() {
    let mut h = gui();
    // Composition Settings needs a comp.
    assert!(
        !h.state()
            .menu_item("motion.comp.composition_settings")
            .enabled
    );
    key(&mut h, Modifiers::COMMAND, Key::K);
    assert!(!dialog_open(&h));
    comp_with(&mut h, &["motion.layer.new.null"]);
    let comp = h.state().active_comp().unwrap();
    let null = h.state().layers()[0].clone();
    let pos = |h: &Ui| {
        bae_motion::query::static_value(
            &h.state().stage().prim(&null).unwrap(),
            "transform:position",
        )
    };
    let before = pos(&h);
    assert!(
        h.state()
            .menu_item("motion.comp.composition_settings")
            .enabled
    );
    key(&mut h, Modifiers::COMMAND, Key::K);
    assert!(dialog_open(&h));
    type_into(&mut h, "Width", "1000");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(dialog_open(&h), "Tab opened the height field");
    click_label(&mut h, "OK");
    assert!(!dialog_open(&h));
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!((info.width, info.height), (1000, 1080));
    // The layer kept its place in the frame: centered anchor, half the change.
    assert_eq!(pos(&h), Some(bae_model::Value::Vec3([500.0, 540.0, 0.0])));
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("Composition Settings"));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!((info.width, info.height), (1920, 1080));
    assert_eq!(pos(&h), before);
    // The menu item opens the same dialog.
    assert!(h.state_mut().menu("motion.comp.composition_settings"));
    settle(&mut h);
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Escape);
}

#[test]
fn composition_settings_in_chinese() {
    if !in_child(
        "acceptance::dialogs::composition_settings_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    for text in [
        "合成设置",
        "合成名称:",
        "基本",
        "高级",
        "3D 渲染器",
        "宽度:",
        "帧速率:",
        "背景颜色:",
    ] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    assert!(h.query_by_label("确定").is_some());
    assert!(h.query_by_label("取消").is_some());
    click_label(&mut h, "高级");
    for text in ["锚点:", "运动模糊", "快门角度:"] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    key(&mut h, Modifiers::NONE, Key::Enter);
    let comp = h.state().active_comp().expect("comp created");
    assert_eq!(h.state().name(&comp), "合成 1");
}

#[test]
fn color_picker_from_the_background_swatch() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    click_label(&mut h, "Background Color");
    // The picker is a second modal dialog over Composition Settings.
    assert!(h.query_by_label("Hex Color").is_some(), "picker not open");
    assert!(h.query_by_label("Hue").is_some());
    type_into(&mut h, "Hex Color", "FF8000");
    // Enter closes only the picker (OK); Composition Settings stays open.
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(h.query_by_label("Hex Color").is_none(), "picker still open");
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    let comp = h.state().active_comp().expect("comp created");
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!(info.bg_color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
    // Esc in the picker cancels it and keeps the dialog's color.
    key(&mut h, Modifiers::COMMAND, Key::K);
    click_label(&mut h, "Background Color");
    type_into(&mut h, "Hex Color", "00FF00");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(h.query_by_label("Hex Color").is_none());
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Enter);
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!(info.bg_color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
}

/// The open Solid Settings dialog's values (name, width, height).
fn solid_dialog(h: &Ui) -> Option<(String, u32, u32)> {
    let dialogs = h.state().app.resource::<bae_ui_motion::SharedDialogs>()?;
    let d = dialogs.lock();
    match d.open.as_ref()? {
        bae_ui_motion::dialogs::Dialog::SolidSettings(s) => {
            Some((s.values.name.clone(), s.values.width, s.values.height))
        }
        _ => None,
    }
}

fn solid_of(h: &Ui, layer: &bae_model::PrimPath) -> bae_motion::SolidInfo {
    let stage = h.state().stage();
    let item = bae_motion::solid::layer_solid(stage, layer).expect("a solid layer");
    bae_motion::solid::solid_info(stage, &item).unwrap()
}

#[test]
fn new_solid_asks_for_solid_settings_first() {
    let mut h = gui();
    new_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::Y);
    assert!(dialog_open(&h), "Cmd+Y opened no dialog");
    assert!(find_text(&h, "Solid Settings").is_some());
    assert!(h.state().layers().is_empty(), "created before OK");
    // AE's defaults: comp size, named after the color.
    assert_eq!(
        solid_dialog(&h),
        Some(("Dark Gray Solid 1".to_owned(), 1920, 1080))
    );
    // The name is selected: typing replaces it.
    type_text(&mut h, "Backdrop");
    type_into(&mut h, "Width", "640");
    type_into(&mut h, "Height", "200");
    assert_eq!(solid_dialog(&h), Some(("Backdrop".to_owned(), 640, 200)));
    // Make Comp Size: back to the comp's size.
    click_label(&mut h, "Make Comp Size");
    assert_eq!(solid_dialog(&h), Some(("Backdrop".to_owned(), 1920, 1080)));
    type_into(&mut h, "Height", "300");
    click_label(&mut h, "OK");
    assert!(!dialog_open(&h));
    let layers = h.state().layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(h.state().name(&layers[0]), "Backdrop");
    let solid = solid_of(&h, &layers[0]);
    assert_eq!((solid.width, solid.height), (1920, 300));
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("New Solid"));
    // Cancel creates nothing.
    key(&mut h, Modifiers::COMMAND, Key::Y);
    click_label(&mut h, "Cancel");
    assert!(!dialog_open(&h));
    assert_eq!(h.state().layers().len(), 1);
}

#[test]
fn solid_color_names_the_new_solid() {
    let mut h = gui();
    new_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::Y);
    click_label(&mut h, "Solid Color");
    type_into(&mut h, "Hex Color", "FF0000");
    key(&mut h, Modifiers::NONE, Key::Enter);
    // Picker closed with OK: the default name follows the color.
    assert_eq!(solid_dialog(&h).unwrap().0, "Red Solid 1");
    key(&mut h, Modifiers::NONE, Key::Enter);
    let layer = h.state().layers()[0].clone();
    assert_eq!(solid_of(&h, &layer).color, [1.0, 0.0, 0.0, 1.0]);
    // The next New Solid starts from the last color.
    key(&mut h, Modifiers::COMMAND, Key::Y);
    assert_eq!(solid_dialog(&h).unwrap().0, "Red Solid 2");
    key(&mut h, Modifiers::NONE, Key::Escape);
}

#[test]
fn layer_settings_edits_the_selected_solid_in_one_undo_step() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.null"]);
    // A null has no Solid Settings: the item is greyed, the shortcut idle.
    assert!(!h.state().menu_item("motion.layer.layer_settings").enabled);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Y);
    assert!(!dialog_open(&h));
    new_solid(&mut h);
    let layer = h.state().layers()[0].clone();
    assert!(h.state().menu_item("motion.layer.layer_settings").enabled);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Y);
    assert!(dialog_open(&h), "Cmd+Shift+Y opened no Solid Settings");
    assert!(find_text(&h, "This change will affect 1 layer.").is_some());
    type_text(&mut h, "Sky");
    type_into(&mut h, "Width", "500");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    assert_eq!(h.state().name(&layer), "Sky");
    let solid = solid_of(&h, &layer);
    assert_eq!((solid.name.as_str(), solid.width), ("Sky", 500));
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("Solid Settings"));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    let solid = solid_of(&h, &layer);
    assert_eq!(
        (solid.name.as_str(), solid.width),
        ("Dark Gray Solid 1", 1920)
    );
    assert_eq!(h.state().name(&layer), "Dark Gray Solid 1");
    // The menu item opens the same dialog.
    assert!(h.state_mut().menu("motion.layer.layer_settings"));
    settle(&mut h);
    assert!(solid_dialog(&h).is_some());
    key(&mut h, Modifiers::NONE, Key::Escape);
}

#[test]
fn solid_settings_in_chinese() {
    if !in_child(
        "acceptance::dialogs::solid_settings_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    new_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::Y);
    for text in [
        "纯色设置",
        "名称:",
        "大小",
        "宽度:",
        "单位:",
        "像素长宽比:",
        "颜色",
    ] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    assert!(h.query_by_label("制作合成大小").is_some());
    key(&mut h, Modifiers::NONE, Key::Enter);
    let layer = h.state().layers()[0].clone();
    assert_eq!(h.state().name(&layer), "深灰色 纯色 1");
}

fn attr(h: &Ui, layer: &bae_model::PrimPath, name: &str) -> Option<bae_model::Value> {
    bae_motion::query::static_value(&h.state().stage().prim(layer).unwrap(), name)
}

#[test]
fn new_camera_and_light_ask_for_their_settings() {
    let mut h = gui();
    new_comp(&mut h);
    let cmd = Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT;
    // Camera: Cmd+Alt+Shift+C opens Camera Settings; OK creates it.
    key(&mut h, cmd, Key::C);
    assert!(dialog_open(&h), "no Camera Settings");
    assert!(find_text(&h, "Camera Settings").is_some());
    assert!(h.state().layers().is_empty(), "created before OK");
    // Lengths are in millimeters by default, at 72 ppi (AE).
    type_into(&mut h, "Zoom", "254");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    let cam = h.state().layers()[0].clone();
    assert_eq!(h.state().name(&cam), "Camera 1");
    match attr(&h, &cam, "camera:zoom") {
        Some(bae_model::Value::Float(z)) => assert!((z - 720.0).abs() < 1e-6, "{z}"),
        other => panic!("{other:?}"),
    }
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("New Camera"));
    // Light: Cmd+Alt+Shift+L opens Light Settings.
    key(&mut h, cmd, Key::L);
    assert!(find_text(&h, "Light Settings").is_some());
    type_into(&mut h, "Intensity", "50");
    key(&mut h, Modifiers::NONE, Key::Enter);
    let light = h.state().layers()[0].clone();
    assert_eq!(h.state().name(&light), "Spot Light 1");
    assert_eq!(
        attr(&h, &light, "light:intensity"),
        Some(bae_model::Value::Float(50.0))
    );
    // Layer Settings (Cmd+Shift+Y) on the new light: one undo step.
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Y);
    assert!(find_text(&h, "Light Settings").is_some());
    type_into(&mut h, "Intensity", "75");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(
        attr(&h, &light, "light:intensity"),
        Some(bae_model::Value::Float(75.0))
    );
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("Light Settings"));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        attr(&h, &light, "light:intensity"),
        Some(bae_model::Value::Float(50.0))
    );
    // Esc creates nothing.
    key(&mut h, cmd, Key::C);
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(h.state().layers().len(), 2);
}

#[test]
fn camera_and_light_settings_in_chinese() {
    if !in_child(
        "acceptance::dialogs::camera_and_light_settings_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    new_comp(&mut h);
    let cmd = Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT;
    key(&mut h, cmd, Key::C);
    for text in [
        "摄像机设置",
        "类型:",
        "预设:",
        "缩放:",
        "视角:",
        "启用景深",
        "胶片大小:",
        "焦距:",
        "合成大小",
        "量度胶片大小:",
    ] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&h.state().layers()[0]), "摄像机 1");
    key(&mut h, cmd, Key::L);
    for text in [
        "灯光设置",
        "灯光类型:",
        "强度:",
        "锥形角度:",
        "衰减:",
        "投影",
    ] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&h.state().layers()[0]), "聚光 1");
}

#[test]
fn precompose_moves_the_selected_layers_in_one_undo_step() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.null", "motion.layer.new.solid"]);
    let comp = h.state().active_comp().unwrap();
    // Nothing selected: the item is greyed and the shortcut idle.
    h.state().session().lock().selected_layers.clear();
    settle(&mut h);
    assert!(!h.state().menu_item("motion.layer.pre_compose").enabled);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::C);
    assert!(!dialog_open(&h));
    // Both layers: Move all attributes.
    let layers = h.state().layers();
    h.state().session().lock().selected_layers = layers.clone();
    settle(&mut h);
    assert!(h.state().menu_item("motion.layer.pre_compose").enabled);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::C);
    assert!(dialog_open(&h), "Cmd+Shift+C opened no Pre-compose");
    assert!(find_text(&h, "Pre-compose").is_some());
    type_text(&mut h, "Group");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    let now = h.state().layers();
    assert_eq!(now.len(), 1);
    assert_eq!(h.state().name(&now[0]), "Group");
    assert_eq!(h.state().active_comp(), Some(comp.clone()));
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("Pre-compose"));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(h.state().layers(), layers);
}

/// An open panel that answers once.
struct PickOnce(std::sync::Mutex<Vec<std::path::PathBuf>>);

impl bae_ui::files::FilePicker for PickOnce {
    fn open(&self, _: &bae_ui::files::OpenRequest) -> Vec<std::path::PathBuf> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

#[test]
fn import_with_cmd_i_and_by_dropping_files_on_the_project_panel() {
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15,
        0, 2, 135, 1, 128, 235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("logo.png"), PNG).unwrap();
    for n in 1..=2 {
        std::fs::write(dir.path().join(format!("clip_{n:04}.png")), PNG).unwrap();
    }
    let mut h = gui();
    let picker = PickOnce(std::sync::Mutex::new(vec![dir.path().join("logo.png")]));
    h.state_mut()
        .app
        .insert_resource(bae_ui::files::SharedFilePicker::new(picker));
    let assets = bae_model::PrimPath::new(bae_motion::query::ASSETS_FOLDER).unwrap();
    let names = |h: &Ui| -> Vec<String> {
        let stage = h.state().stage();
        stage
            .children(&assets)
            .iter()
            .map(|c| bae_motion::query::display_name_of(stage, c))
            .collect()
    };
    // Cmd+I: the open panel, then one import step.
    key(&mut h, Modifiers::COMMAND, Key::I);
    assert_eq!(names(&h), ["logo.png"]);
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("Import"));
    // A frame of a numbered sequence dropped on the Project panel.
    let over_project =
        find_text(&h, "Name").expect("Project panel").center() + egui::vec2(0.0, 80.0);
    send(&mut h, Event::PointerMoved(over_project));
    h.input_mut()
        .dropped_files
        .push(std::sync::Arc::new(DroppedFile(
            dir.path().join("clip_0001.png"),
        )));
    settle(&mut h);
    assert_eq!(names(&h), ["logo.png", "clip_[0001-0002].png"]);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(names(&h), ["logo.png"]);
}
