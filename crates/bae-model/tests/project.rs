use std::fs;
use std::path::Path;

use bae_model::{
    AssetFile, AssetResolver, AuthorId, BaezAsset, CommandRegistry, PrimPath, Project,
    ProjectError, Resolution, Stage, SublayerId, Value, asset::infer_mapping,
    export_flattened_json, export_sublayer_json, extract_baez,
};
use serde_json::{Value as Json, json};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn run(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: Json) {
    reg.execute(stage, &AuthorId::new("user:a").unwrap(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"));
}

fn sample(reg: &CommandRegistry) -> Stage {
    let mut s = Stage::new();
    for (path, t) in [
        ("/comps", "Folder"),
        ("/comps/main", "Comp"),
        ("/comps/main/title", "Layer"),
    ] {
        run(
            reg,
            &mut s,
            "prim.create",
            json!({"path": path, "prim_type": t}),
        );
    }
    run(
        reg,
        &mut s,
        "sublayer.create",
        json!({"id": "anim/shot010"}),
    );
    run(
        reg,
        &mut s,
        "sublayer.set_edit_target",
        json!({"id": "anim/shot010"}),
    );
    run(
        reg,
        &mut s,
        "key.set",
        json!({"path": "/comps/main/title", "attr": "opacity", "time": 0, "keyframe": {"value": {"type": "float", "value": 0.0}}}),
    );
    s
}

fn flat(stage: &Stage) -> Json {
    serde_json::to_value(stage.flatten()).unwrap()
}

#[test]
fn directory_package_roundtrip() {
    let reg = CommandRegistry::with_builtins();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    let mut project = Project::from_stage(sample(&reg));
    project.save_as(&dir).unwrap();
    for f in [
        "project.json",
        "sublayers/root.loro",
        "sublayers/anim/shot010.loro",
    ] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    let manifest: Json =
        serde_json::from_slice(&fs::read(dir.join("project.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], "bae.project");
    assert_eq!(manifest["layer_stack"], json!(["anim/shot010", "root"]));

    let mut reopened = Project::open(&dir).unwrap();
    assert_eq!(reopened.stage.layer_stack(), project.stage.layer_stack());
    assert_eq!(flat(&reopened.stage), flat(&project.stage));

    // Edit, save in place, drop a sublayer: its stale snapshot is removed.
    run(
        &reg,
        &mut reopened.stage,
        "attr.set",
        json!({"path": "/comps/main", "attr": "width", "value": {"type": "int", "value": 1920}}),
    );
    reopened
        .stage
        .remove_sublayer(&SublayerId::new("anim/shot010").unwrap())
        .unwrap();
    reopened.save().unwrap();
    assert!(!dir.join("sublayers/anim/shot010.loro").exists());
    let again = Project::open(&dir).unwrap();
    assert_eq!(
        again.stage.prim(&p("/comps/main")).unwrap().value("width"),
        Some(&Value::Int(1920))
    );
    assert_eq!(again.stage.layer_stack(), &[SublayerId::root()]);

    assert!(matches!(
        Project::open(tmp.path()),
        Err(ProjectError::NotAProject(_))
    ));
}

#[test]
fn baez_roundtrip_with_embedded_assets() {
    let reg = CommandRegistry::with_builtins();
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("clip.bin");
    let bytes: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
    fs::write(&media, &bytes).unwrap();

    let mut project = Project::from_stage(sample(&reg));
    let baez = tmp.path().join("Handoff.baez");
    project
        .write_baez(
            &baez,
            &[BaezAsset {
                name: "assets/footage/clip.bin".into(),
                source: media.clone(),
            }],
        )
        .unwrap();

    let opened = Project::open(&baez).unwrap();
    assert_eq!(flat(&opened.stage), flat(&project.stage));
    assert_eq!(opened.base_dir(), Some(tmp.path()));

    let out = tmp.path().join("Unpacked.bae");
    extract_baez(&baez, &out).unwrap();
    assert_eq!(
        fs::read(out.join("assets/footage/clip.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        flat(&Project::open(&out).unwrap().stage),
        flat(&project.stage)
    );

    let bad = project.write_baez(
        tmp.path().join("bad.baez"),
        &[BaezAsset {
            name: "assets/../escape".into(),
            source: media,
        }],
    );
    assert!(matches!(bad, Err(ProjectError::Zip(_))));
}

#[test]
fn cross_project_references_load_on_open() {
    let reg = CommandRegistry::with_builtins();
    let tmp = tempfile::tempdir().unwrap();

    let mut lib = Stage::new();
    run(
        &reg,
        &mut lib,
        "prim.create",
        json!({"path": "/chars", "prim_type": "Folder"}),
    );
    run(
        &reg,
        &mut lib,
        "prim.create",
        json!({"path": "/chars/robot", "prim_type": "Comp"}),
    );
    run(
        &reg,
        &mut lib,
        "prim.create",
        json!({"path": "/chars/robot/head", "prim_type": "Layer"}),
    );
    // The library points back at the shot project: a cycle the loader must cut.
    run(
        &reg,
        &mut lib,
        "prim.create",
        json!({"path": "/back", "prim_type": "Layer"}),
    );
    run(
        &reg,
        &mut lib,
        "prim.set_references",
        json!({"path": "/back", "references": [{"project": "Shot.bae", "prim": "/shots"}]}),
    );
    Project::from_stage(lib)
        .save_as(tmp.path().join("Library.bae"))
        .unwrap();

    let mut shot = Stage::new();
    run(
        &reg,
        &mut shot,
        "prim.create",
        json!({"path": "/shots", "prim_type": "Folder"}),
    );
    run(
        &reg,
        &mut shot,
        "prim.create",
        json!({"path": "/shots/robot", "prim_type": "Layer"}),
    );
    run(
        &reg,
        &mut shot,
        "prim.set_references",
        json!({"path": "/shots/robot", "references": [{"project": "../Library.bae", "prim": "/chars/robot"}]}),
    );
    // "../" is relative to Shot.bae itself (the project's base directory).
    let mut project = Project::from_stage(shot);
    project.save_as(tmp.path().join("Shot.bae")).unwrap();

    let opened = Project::open(tmp.path().join("Shot.bae")).unwrap();
    assert!(opened.stage.has_prim(&p("/shots/robot/head")));
    assert!(opened.stage.unresolved_references().is_empty());

    let no_refs = Project::open_with(
        tmp.path().join("Shot.bae"),
        &bae_model::OpenOptions {
            load_references: false,
        },
    )
    .unwrap();
    assert!(!no_refs.stage.has_prim(&p("/shots/robot/head")));
    assert_eq!(no_refs.stage.unresolved_references().len(), 1);
}

#[test]
fn export_json_is_readable() {
    let reg = CommandRegistry::with_builtins();
    let stage = sample(&reg);
    let layer = export_sublayer_json(&stage, &SublayerId::new("anim/shot010").unwrap()).unwrap();
    assert_eq!(layer["sublayer"], "anim/shot010");
    assert_eq!(layer["prims"][0]["name"], "comps");
    assert_eq!(layer["prims"][0]["specifier"], "over");
    let title = &layer["prims"][0]["children"][0]["children"][0];
    assert_eq!(
        title["attrs"]["opacity"]["keyframes"]["0"]["value"]["type"],
        "float"
    );

    let all = export_flattened_json(&stage);
    assert_eq!(all["flattened"], true);
    assert_eq!(all["prims"][0]["children"][0]["prim_type"], "Comp");
    assert_eq!(
        bae_model::export_prim_json(&stage, &p("/comps/main/title")).unwrap()["prims"][0]["name"],
        "title"
    );
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn assets_resolve_relink_and_map() {
    let reg = CommandRegistry::with_builtins();
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let base = root.join("Film.bae");
    fs::create_dir_all(&base).unwrap();
    write(&root.join("footage/a.mov"), b"aaaa");
    write(&root.join("footage/b.mov"), b"bbbbbb");

    let a = AssetFile::from_path(&root.join("footage/a.mov"), Some(&base), true).unwrap();
    assert_eq!(a.relative.as_deref(), Some("../footage/a.mov"));
    assert_eq!(a.size, 4);
    assert!(a.hash.is_some());
    let b = AssetFile::from_path(&root.join("footage/b.mov"), Some(&base), false).unwrap();

    // Store on Asset prims through the command and read back.
    let mut stage = Stage::new();
    run(
        &reg,
        &mut stage,
        "prim.create",
        json!({"path": "/assets", "prim_type": "Folder"}),
    );
    run(
        &reg,
        &mut stage,
        "prim.create",
        json!({"path": "/assets/a", "prim_type": "Asset"}),
    );
    run(
        &reg,
        &mut stage,
        "asset.set_file",
        json!({"path": "/assets/a", "file": a}),
    );
    assert_eq!(
        AssetFile::from_prim(&stage.prim(&p("/assets/a")).unwrap()).unwrap(),
        a
    );

    let resolver = AssetResolver::new(Some(base.clone()), Vec::new());
    assert!(matches!(resolver.resolve(&a), Resolution::Relative(_)));

    // The footage folder moves away: relative and original both fail.
    fs::rename(root.join("footage"), root.join("moved")).unwrap();
    assert!(resolver.resolve(&a).is_missing());

    // A search directory finds it by name + fingerprint (hash checked).
    let mut searching = resolver.clone();
    searching.search_dirs.push(root.clone());
    let found = searching.resolve(&a);
    assert_eq!(found, Resolution::Searched(root.join("moved/a.mov")));

    // Relinking one asset gives a mapping that finds the other.
    let mapping = infer_mapping(&a.original, found.path().unwrap()).unwrap();
    let mapped = AssetResolver::new(None, vec![mapping]);
    assert_eq!(
        mapped.resolve(&b),
        Resolution::Mapped(root.join("moved/b.mov"))
    );

    // A same-named file with the wrong content is not accepted.
    write(&root.join("decoy/a.mov"), b"xxxx");
    fs::remove_file(root.join("moved/a.mov")).unwrap();
    assert!(searching.resolve(&a).is_missing());
}
