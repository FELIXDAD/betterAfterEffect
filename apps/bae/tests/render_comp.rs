//! Exercise composition selection through the executable, even without a GPU.

use std::path::Path;
use std::process::Command;

use bae_model::{AuthorId, CommandRegistry, Project};
use serde_json::json;

fn save_project(path: &Path, names: &[&str]) -> Vec<String> {
    let mut project = Project::new();
    let mut commands = CommandRegistry::with_builtins();
    commands.register(bae_motion::comp::CreateComp).unwrap();
    let author = AuthorId::new("user:render-test").unwrap();
    let paths = names
        .iter()
        .map(|name| {
            let result = commands
                .execute(
                    &mut project.stage,
                    &author,
                    "comp.create",
                    json!({"name": name}),
                )
                .unwrap();
            result.output["path"].as_str().unwrap().to_owned()
        })
        .collect();
    project.save_as(path).unwrap();
    paths
}

fn render_error(path: &Path, comp: &str) -> String {
    // Invalid frame range stops a resolved request before GPU initialization.
    let out = Command::new(env!("CARGO_BIN_EXE_bae"))
        .env("BAE_LOG", "warn")
        .arg("render")
        .arg(path)
        .args(["--comp", comp, "--frames", "invalid"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    String::from_utf8(out.stderr).unwrap()
}

#[test]
fn accepts_display_names_and_internal_selectors() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project.bae");
    let names = ["Opening Shot", "片头✨", "主 合成"];
    let paths = save_project(&project, &names);
    for name in names.into_iter().chain(
        paths
            .iter()
            .flat_map(|path| [path.as_str(), path.strip_prefix("/comps/").unwrap()]),
    ) {
        let error = render_error(&project, name);
        assert!(error.contains("bad --frames"), "{name:?}: {error}");
    }
}

#[test]
fn ambiguous_name_lists_each_matching_path() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project.bae");
    let paths = save_project(&project, &["Intro", "Intro", "Credits"]);

    let error = render_error(&project, "Intro");
    assert!(error.contains("ambiguous"), "{error}");
    for path in &paths[..2] {
        assert!(error.contains(path), "{error}");
        let by_path = render_error(&project, path);
        assert!(by_path.contains("bad --frames"), "{by_path}");
    }
    assert!(!error.contains(&paths[2]), "{error}");
}

#[test]
fn missing_name_lists_project_compositions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project.bae");
    let names = ["主 合成", "End Credits"];
    let paths = save_project(&project, &names);

    let error = render_error(&project, "Missing Comp");
    assert!(error.contains("\"Missing Comp\" not found"), "{error}");
    for (name, path) in names.into_iter().zip(paths) {
        assert!(error.contains(&format!("{name:?} ({path})")), "{error}");
    }
}
