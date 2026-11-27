use super::*;
use bae_motion::query;
use serde_json::json;

struct FolderPicker(PathBuf);
impl bae_ui::files::FilePicker for FolderPicker {
    fn open(&self, _: &bae_ui::files::OpenRequest) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_folder(&self, _: &str) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

fn desktop(dir: &Path) -> Ui {
    let mut b = AppBuilder::new();
    b.insert_resource(bae_platform::AppDirs::under_root(dir.join("app")));
    let mut app = crate::assemble_modules(b).build();
    app.insert_resource(bae_ui::files::SharedFilePicker::new(FolderPicker(
        dir.into(),
    )));
    let mut ui = gui_with(Gui::with_app(app));
    assert!(ui.state_mut().menu("motion.file.choose_script_directory"));
    settle(&mut ui);
    ui
}

fn run(h: &mut Ui, source: &str) {
    h.state_mut().dispatch(Invocation::with_params(
        bae_script::RUN_SCRIPT,
        json!({"source":source,"interactive":true}),
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        frame(h);
        if h.state()
            .app
            .resource::<bae_script::ScriptWindows>()
            .unwrap()
            .windows()
            .is_empty()
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "JSX is still waiting for an application operation"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    settle(h);
    if let Some(report) = h
        .state()
        .app
        .resource::<bae_script::ScriptConsole>()
        .unwrap()
        .take()
    {
        assert_eq!(report.error, None);
    }
    let dialogs = h
        .state()
        .app
        .resource::<bae_ui_motion::dialogs::SharedDialogs>()
        .unwrap()
        .lock();
    assert!(dialogs.open.is_none(), "{:?}", dialogs.open);
}

#[test]
fn jsx_folder_settings_and_render_queue_use_application_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = desktop(dir.path());
    run(
        &mut h,
        include_str!("../../../../crates/bae-script/tests/compat/files-settings.jsx"),
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("reports/caption.txt")).unwrap(),
        "Alpha\nBeta"
    );
    run(
        &mut h,
        include_str!("../../../../crates/bae-script/tests/compat/render-queue.jsx"),
    );
    let q = h
        .state()
        .app
        .resource::<bae_ui_motion::SharedRenderQueue>()
        .unwrap()
        .lock();
    assert_eq!(q.items().len(), 1);
    assert!(!q.items()[0].render);
    drop(q);
    let comp = query::comps(h.state().stage())[0].clone();
    h.state().session().lock().open_comp(&comp);
    run(
        &mut h,
        "var id = app.findMenuCommandId('Add to Render Queue'); if (id !== 2161) throw Error('menu lookup: ' + id); app.executeCommand(2161); if (app.project.renderQueue.numItems !== 2) throw Error('menu dispatch');",
    );
    assert_eq!(
        h.state()
            .app
            .resource::<bae_ui_motion::SharedRenderQueue>()
            .unwrap()
            .lock()
            .items()
            .len(),
        2
    );
    drop(h);
    let mut reopened = desktop(dir.path());
    run(
        &mut reopened,
        "if (app.settings.getSetting('Caption tool','Last title') !== 'Beta') throw Error('settings were not saved');",
    );
}

#[test]
fn jsx_render_waits_for_all_output_modules() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut h = desktop(dir.path());
    h.state_mut().app.insert_resource(gpu);
    run(
        &mut h,
        "var comp = app.project.items.addComp('Frame',32,32,1,1,1); app.project.renderQueue.items.add(comp);",
    );
    {
        let mut queue = h
            .state()
            .app
            .resource::<bae_ui_motion::SharedRenderQueue>()
            .unwrap()
            .lock();
        let id = queue.items()[0].id;
        let modules = &mut queue.item_mut(id).unwrap().outputs;
        modules[0].set_format(bae_render::ExportFormat::Png);
        modules.push(modules[0].clone());
    }
    run(
        &mut h,
        r#"
        var queue = app.project.renderQueue, item = queue.item(1);
        if (item.numOutputModules !== 2) throw Error("output count");
        item.outputModule(1).file = new File("first_####.png");
        item.outputModule(2).file = new File("second_####.png");
        queue.render();
        if (queue.rendering || !new File("second_0000.png").exists) throw Error("render incomplete");
    "#,
    );
    for name in ["first_0000.png", "second_0000.png"] {
        let image = std::fs::read(dir.path().join(name)).unwrap();
        assert!(image.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}

#[test]
fn jsx_import_and_save_use_the_active_document() {
    // A generated 2-by-1 opaque white RGBA PNG.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("input.png"),
        [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 244, 34, 127, 138, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 248, 15,
            5, 0, 35, 229, 7, 249, 138, 52, 120, 179, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ],
    )
    .unwrap();
    let mut h = desktop(dir.path());
    run(
        &mut h,
        include_str!("../../../../crates/bae-script/tests/compat/import-and-save.jsx"),
    );
    let file = dir.path().join("demo.baez");
    assert_eq!(h.state().doc.location(), Some(file.as_path()));
    let saved = bae_model::Project::open(file).unwrap();
    assert_eq!(
        query::comp_layers(&saved.stage, &query::comps(&saved.stage)[0]).len(),
        1
    );
}
