use super::*;
use bae_model::{PrimType, Time};
use bae_motion::query;
use egui_kittest::kittest::Queryable as _;
use serde_json::json;

#[test]
fn recent_script_menu_disables_missing_files_and_runs_installed_ids() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = bae_platform::AppDirs::under_root(dir.path());
    let scripts = dirs.data.join("Scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let installed = scripts.join("Caption.jsx");
    std::fs::write(
        &installed,
        include_str!("../../../../crates/bae-script/tests/compat/editor-title.jsx"),
    )
    .unwrap();
    let missing = scripts.join("Removed.jsx");
    let config = dirs.config_file("recent-scripts.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(config, serde_json::to_vec(&[&missing, &installed]).unwrap()).unwrap();
    let mut builder = AppBuilder::new();
    builder.insert_resource(dirs);
    let mut h = gui_with(Gui::with_app(crate::assemble_modules(builder).build()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.state().menu_item("motion.file.recent_script.0").enabled {
        assert!(
            std::time::Instant::now() < deadline,
            "missing script remained enabled"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        h.state()
            .menu_item("motion.file.recent_script.0")
            .invocation
            .params["path"],
        json!(missing)
    );
    assert!(!h.state_mut().menu("motion.file.recent_script.0"));
    assert!(h.state_mut().menu("motion.file.recent_script.1"));
    settle(&mut h);
    assert!(h.state_mut().menu("motion.file.script.Caption.jsx"));
    settle(&mut h);
    assert_eq!(query::comps(h.state().stage()).len(), 2);
    assert!(!h.state().menu_item("motion.file.recent_script.1").enabled);
}

#[test]
fn script_editor_runs_typed_jsx_through_the_file_menu() {
    let mut h = gui();
    assert!(h.state_mut().menu("motion.file.script_editor"));
    settle(&mut h);
    let source = h.get_by_label(bae_ui::tr("Script Source")).rect().center();
    click_at(&mut h, source);
    type_text(
        &mut h,
        include_str!("../../../../crates/bae-script/tests/compat/editor-title.jsx"),
    );
    let run = h.get_by_label(bae_ui::tr("Run Script")).rect().center();
    click_at(&mut h, run);
    settle(&mut h);
    let stage = h.state().stage();
    let comp = query::comps(stage)
        .into_iter()
        .find(|p| query::display_name_of(stage, p) == "Caption")
        .unwrap();
    assert_eq!(query::comp_layers(stage, &comp).len(), 1);
}

#[test]
fn scriptui_dialog_resumes_the_original_script_after_click() {
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(bae_script::RUN_SCRIPT, json!({
        "source": include_str!("../../../../crates/bae-script/tests/compat/scriptui-dialog.jsx"),
        "interactive": true
    })));
    settle(&mut h);
    let button = h.get_by_label("Create caption").rect().center();
    click_at(&mut h, button);
    settle(&mut h);
    let stage = h.state().stage();
    let comp = query::comps(stage)
        .into_iter()
        .find(|p| query::display_name_of(stage, p) == "Caption 1")
        .unwrap_or_else(|| {
            panic!(
                "script actions: {:?}, outcomes: {:?}, dialog: {:?}",
                h.state().dispatched,
                h.state().outcomes,
                h.state()
                    .app
                    .resource::<bae_ui_motion::dialogs::SharedDialogs>()
                    .unwrap()
                    .lock()
                    .open
            )
        });
    assert_eq!(query::comp_layers(stage, &comp).len(), 1);
}

fn panel_gui() -> Ui {
    let app = crate::app_builder().build();
    assert!(
        app.get_registry::<dyn bae_gpu::WarmUp>()
            .unwrap()
            .get("bae.script.builtin_panels")
            .is_some()
    );
    crate::shell::builtin_scripts::load(app.resource::<bae_plugin::PluginRuntime>().unwrap())
        .unwrap();
    gui_with(Gui::with_app(app))
}

#[test]
fn scriptui_panels_folder_opens_a_docked_script_with_live_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = bae_platform::AppDirs::under_root(dir.path());
    let folder = dirs.data.join("Scripts/ScriptUI Panels");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("Count comps.jsx"),
        include_str!("../../../../crates/bae-script/tests/compat/scriptui-panel.jsx"),
    )
    .unwrap();
    let mut builder = AppBuilder::new();
    builder.insert_resource(dirs);
    let mut h = gui_with(Gui::with_app(crate::assemble_modules(builder).build()));
    let id = h
        .state()
        .menu_items()
        .into_iter()
        .find(|i| i.title == "Count comps.jsx")
        .unwrap()
        .id;
    assert!(h.state_mut().menu(&id));
    settle(&mut h);
    for _ in 0..2 {
        let button = h.get_by_label("Add numbered comp").rect().center();
        click_at(&mut h, button);
        settle(&mut h);
    }
    assert!(
        query::comps(h.state().stage())
            .iter()
            .any(|p| query::display_name_of(h.state().stage(), p) == "Pass 2")
    );
}

fn click_button(h: &mut Ui, title: &str) {
    let center = h
        .query_by(|n| {
            matches!(
                n.author_id(),
                Some("panel:create_nulls_from_paths" | "panel:vr_comp_editor")
            )
        })
        .expect("script panel is open")
        .query_by(|n| {
            n.role() == egui::accesskit::Role::Button
                && n.toggled().is_none()
                && n.label().as_deref() == Some(bae_ui::tr(title))
        })
        .unwrap_or_else(|| panic!("button {title} missing"))
        .rect()
        .center();
    click_at(h, center);
    settle(h);
}

#[test]
fn path_null_buttons_bind_evaluated_vertices_and_undo_in_one_step() {
    let mut h = panel_gui();
    let run =
        |h: &mut Ui, id: &str, args| h.state_mut().dispatch(Invocation::with_params(id, args));
    run(
        &mut h,
        "comp.create",
        json!({"name":"Paths","width":640,"height":360}),
    );
    let comp = query::comps(h.state().stage())[0].clone();
    h.state().session().lock().open_comp(&comp);
    run(&mut h, "layer.create", json!({"comp":comp,"kind":"solid"}));
    let solid = h.state().layers()[0].clone();
    run(
        &mut h,
        "mask.create",
        json!({"layer":solid,"kind":"rect","position":[200,150],"size":[200,100]}),
    );
    let mask = h.state().stage().children(&solid.child("masks").unwrap())[0].clone();
    run(
        &mut h,
        "shape.add",
        json!({"comp":comp,"kind":"rect","bezier":true,"size":[90,50]}),
    );
    let shape = h
        .state()
        .layers()
        .into_iter()
        .find(|p| p != &solid)
        .unwrap();
    let group = h
        .state()
        .stage()
        .children(&shape.child("contents").unwrap())[0]
        .clone();
    let path = h
        .state()
        .stage()
        .children(&group.child("contents").unwrap())
        .into_iter()
        .find(|p| h.state().stage().prim(p).unwrap().is_a(&PrimType::PATH))
        .unwrap();
    assert!(
        h.state_mut()
            .menu("menu.window.create_nulls_from_paths_jsx")
    );
    settle(&mut h);
    for (title, count) in [
        ("Points Follow Nulls", 8),
        ("Nulls Follow Points", 8),
        ("Trace Path", 2),
    ] {
        h.state().session().lock().selected_props =
            vec![(mask.clone(), "path".into()), (path.clone(), "path".into())];
        click_button(&mut h, title);
        assert_eq!(
            h.state().layers().len(),
            2 + count,
            "{title}: {:?}",
            h.state().outcomes
        );
        let eval = |h: &Ui, t| {
            bae_anim::Evaluator::new()
                .evaluate_comp(h.state().stage(), &comp, Time::from_seconds(t))
                .unwrap()
        };
        let frame = eval(&h, 0.0);
        assert!(frame.errors.is_empty(), "{title}: {:?}", frame.errors);
        let vertex = frame.layer(&solid).unwrap().masks[0].shape.vertices[0];
        let position = frame
            .layer(&solid)
            .unwrap()
            .world_matrix
            .transform_point3(bae_anim::glam::DVec3::new(vertex[0], vertex[1], 0.0));
        let node = frame
            .layers
            .iter()
            .find(|l| {
                l.name.starts_with(&*frame.layer(&solid).unwrap().name) && l.name.ends_with("[0]")
            })
            .or_else(|| {
                frame.layers.iter().find(|l| {
                    l.name
                        .starts_with(&format!("Trace {}:", frame.layer(&solid).unwrap().name))
                })
            })
            .unwrap();
        assert!(
            (node.transform.position.truncate() - position.truncate()).length() < 0.1,
            "{title}: {:?} vs {position:?}",
            node.transform.position
        );
        if title == "Trace Path" {
            let later = eval(&h, 0.25);
            assert!(later.errors.is_empty(), "{:?}", later.errors);
            assert_ne!(
                later.layer(&node.path).unwrap().transform.position,
                node.transform.position
            );
        } else if title == "Points Follow Nulls" {
            run(
                &mut h,
                "attr.set",
                json!({"path":node.path,"attr":"transform:position","value":{"type":"vec3","value":[300,140,0]}}),
            );
            let moved = eval(&h, 0.0);
            assert_ne!(
                moved.layer(&solid).unwrap().masks[0].shape.vertices[0],
                vertex,
                "position {:?}, vertices {:?}, errors {:?}, last command {:?}",
                moved.layer(&node.path).unwrap().transform.position,
                moved.layer(&solid).unwrap().masks[0].shape.vertices,
                moved.errors,
                h.state().outcomes.last()
            );
            h.state_mut().dispatch(Invocation::new(ids::UNDO));
        }
        h.state_mut().dispatch(Invocation::new(ids::UNDO));
        settle(&mut h);
        assert_eq!(h.state().layers().len(), 2);
        assert!(
            query::attr_spec(h.state().stage(), &mask, "path")
                .unwrap()
                .expression
                .is_none()
        );
    }
    assert!(
        h.state_mut()
            .menu("menu.window.create_nulls_from_paths_jsx")
    );
    settle(&mut h);
    assert!(h.query_by_label(bae_ui::tr("Trace Path")).is_none());
}

fn open_vr(h: &mut Ui) {
    assert!(h.state_mut().menu("menu.window.vr_comp_editor_jsx"));
    settle(h);
}
fn vr_role(h: &Ui, role: &str) -> Vec<PrimPath> {
    query::comps(h.state().stage())
        .into_iter()
        .filter(|p| {
            query::static_value(&h.state().stage().prim(p).unwrap(), "vr:role")
                .is_some_and(|v| v.as_str() == Some(role))
        })
        .collect()
}

#[test]
fn vr_editor_creates_edits_outputs_and_six_cubemap_views() {
    let mut h = panel_gui();
    h.state_mut().dispatch(Invocation::with_params(
        "comp.create",
        json!({"name":"Panorama","width":2048,"height":1024}),
    ));
    let master = query::comps(h.state().stage())[0].clone();
    h.state().session().lock().close_comp(&master);
    open_vr(&mut h);
    click_button(&mut h, "Add 2D Edit");
    click_button(&mut h, "Add 2D Edit");
    assert_eq!(vr_role(&h, "edit2d").len(), 1, "{:?}", h.state().outcomes);
    let edit = vr_role(&h, "edit2d")[0].clone();
    let output = vr_role(&h, "output")[0].clone();
    assert_eq!(h.state().active_comp(), Some(edit.clone()));
    assert_eq!(query::comp_layers(h.state().stage(), &output).len(), 2);
    click_button(&mut h, "Properties");
    click_button(&mut h, "Save");
    click_button(&mut h, "Open Output/Render");
    assert_eq!(h.state().active_comp(), Some(output.clone()));
    click_button(&mut h, "Output Settings");
    click_button(&mut h, "Save");
    click_button(&mut h, "Extract Cubemap");
    click_button(&mut h, "Extract Cubemap");
    let faces = vr_role(&h, "face");
    assert_eq!(faces.len(), 6, "{:?}", h.state().outcomes);
    for face in faces {
        let name = query::static_value(&h.state().stage().prim(&face).unwrap(), "vr:face").unwrap();
        click_button(&mut h, &format!("{} View", name.as_str().unwrap()));
        assert_eq!(h.state().active_comp(), Some(face.clone()));
        let view = bae_anim::Evaluator::new()
            .evaluate_comp(h.state().stage(), &face, Time::ZERO)
            .unwrap();
        assert!(view.errors.is_empty(), "{face}: {:?}", view.errors);
        assert!(view.camera.is_some());
    }
    h.state_mut().dispatch(Invocation::new(ids::UNDO));
    settle(&mut h);
    assert!(vr_role(&h, "face").is_empty());
    assert!(h.state().stage().has_prim(&output));
    h.state().session().lock().open_comp(&edit);
    settle(&mut h);
    click_button(&mut h, "Properties");
    click_button(&mut h, "Delete");
    assert!(
        !h.state().stage().has_prim(&edit),
        "{:?}",
        h.state().outcomes
    );
    h.state_mut().dispatch(Invocation::new(ids::UNDO));
    assert!(h.state().stage().has_prim(&edit));
    open_vr(&mut h);
    assert!(h.query_by_label(bae_ui::tr("Add 2D Edit")).is_none());
    open_vr(&mut h);
    assert!(h.query_by_label(bae_ui::tr("Add 2D Edit")).is_some());
}

#[test]
fn vr_master_and_3d_edit_refresh_the_six_camera_scenes() {
    let mut h = panel_gui();
    open_vr(&mut h);
    click_button(&mut h, "Create VR Master");
    click_button(&mut h, "Create VR Master");
    let master = vr_role(&h, "master3d");
    assert_eq!(master.len(), 1, "{:?}", h.state().outcomes);
    assert_eq!(h.state().active_comp(), Some(master[0].clone()));
    click_button(&mut h, "Add 3D Edit");
    click_button(&mut h, "Add 3D Edit");
    let edits = vr_role(&h, "edit3d");
    assert_eq!(edits.len(), 1, "{:?}", h.state().outcomes);
    assert_eq!(h.state().active_comp(), Some(edits[0].clone()));
    h.state_mut().dispatch(Invocation::with_params(
        "layer.create",
        json!({"comp":edits[0],"kind":"solid","name":"Title"}),
    ));
    click_button(&mut h, "Open Output/Render");
    let copies: Vec<_> = vr_role(&h, "face")
        .into_iter()
        .filter(|p| {
            query::comp_layers(h.state().stage(), p)
                .iter()
                .any(|l| h.state().name(l) == "Title")
        })
        .collect();
    assert_eq!(copies.len(), 6, "{:?}", h.state().outcomes);
    let mut directions = Vec::new();
    for face in copies {
        let view = bae_anim::Evaluator::new()
            .evaluate_comp(h.state().stage(), &face, Time::ZERO)
            .unwrap();
        assert!(view.errors.is_empty(), "{face}: {:?}", view.errors);
        let camera = view.layers[view.camera.unwrap()].camera.as_ref().unwrap();
        let forward = camera
            .world_matrix
            .transform_vector3(bae_anim::glam::DVec3::Z)
            .round()
            .as_ivec3();
        assert!(
            !directions.contains(&forward),
            "{face}: repeated camera direction {forward:?}"
        );
        directions.push(forward);
    }
}
