//! Project preferences through the desktop command and file routes.

use super::*;
use bae_motion::query;

#[test]
fn new_projects_use_an_untitled_copy_of_the_template() {
    let mut h = gui();
    let folder = tempfile::tempdir().unwrap();
    for extension in ["bae", "baez"] {
        let template = folder.path().join(format!("Template.{extension}"));
        let copy = folder.path().join(format!("Copy.{extension}"));
        new_comp(&mut h);
        let expected = bae_model::export_flattened_json(h.state().stage());
        h.state_mut().dialogs.save.push_back(Some(template.clone()));
        h.state_mut().dispatch(Invocation::new(ids::SAVE_AS));
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("project.new_project_loads_template", true);
        prefs.set("project.project_template", template.to_str().unwrap());
        h.state().prefs.set(prefs).unwrap();

        h.state_mut().dispatch(Invocation::new(ids::CLOSE_PROJECT));
        assert!(query::comps(h.state().stage()).is_empty());
        h.state_mut().dispatch(Invocation::new(ids::NEW_PROJECT));
        assert!(h.state().doc.location().is_none());
        assert_eq!(
            bae_model::export_flattened_json(h.state().stage()),
            expected
        );
        new_comp(&mut h);
        h.state_mut().dialogs.save.push_back(Some(copy.clone()));
        h.state_mut().dispatch(Invocation::new(ids::SAVE));
        assert_eq!(h.state().doc.location(), Some(copy.as_path()));
        let source = bae_model::Project::open(&template).unwrap();
        assert_eq!(bae_model::export_flattened_json(&source.stage), expected);
        h.state_mut().dispatch(Invocation::new(ids::CLOSE_PROJECT));
    }
}

#[test]
fn folders_use_the_defaults_only_when_created() {
    let mut h = gui();
    let update = |h: &mut Ui, name: &str, label: i64| {
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("project.new_project_solids_folder", name);
        prefs.set("labels.label_default_folder", label);
        h.state().prefs.set(prefs).unwrap();
    };
    update(&mut h, "Color cards", 12);
    new_comp(&mut h);
    new_solid(&mut h);
    update(&mut h, "New cards", 9);
    new_solid(&mut h);
    h.state_mut()
        .dispatch(Invocation::new("motion.file.new_folder"));
    settle(&mut h);
    let stage = h.state().stage();
    let solids = PrimPath::new(query::SOLIDS_FOLDER).unwrap();
    assert_eq!(query::display_name_of(stage, &solids), "Color cards");
    let label = |p: &PrimPath| query::static_value(&stage.prim(p).unwrap(), "label").unwrap();
    assert_eq!(label(&solids), bae_model::Value::Int(12));
    let folder = stage
        .children(&PrimPath::root())
        .into_iter()
        .find(|p| query::display_name_of(stage, p).starts_with("Untitled"))
        .unwrap();
    assert_eq!(label(&folder), bae_model::Value::Int(9));
}

#[test]
fn opening_a_project_can_restore_its_saved_workspace() {
    let mut h = gui();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("Workspace.bae");
    let original = h.state().workspace.active_workspace().to_owned();
    let other = h
        .state()
        .workspace
        .workspaces()
        .into_iter()
        .map(|(id, _)| id)
        .find(|id| *id != original)
        .unwrap();
    new_comp(&mut h);
    h.state_mut().dialogs.save.push_back(Some(file.clone()));
    h.state_mut().dispatch(Invocation::new(ids::SAVE_AS));
    assert_eq!(h.state().doc.location(), Some(file.as_path()));
    h.state_mut().dispatch(Invocation::with_params(
        ids::SWITCH_WORKSPACE,
        serde_json::json!({"id": other}),
    ));
    assert_eq!(h.state().workspace.active_workspace(), other);
    let open = Invocation::with_params(ids::OPEN_PROJECT, serde_json::json!({"path": file}));
    h.state_mut().dispatch(open.clone());
    assert_eq!(h.state().workspace.active_workspace(), other);
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("project.preserve_current_workspace_on_open", false);
    h.state().prefs.set(prefs).unwrap();
    h.state_mut().dispatch(open);
    assert_eq!(h.state().workspace.active_workspace(), original);
}
