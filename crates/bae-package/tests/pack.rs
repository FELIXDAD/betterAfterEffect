//! End-to-end packaging tests. Every fixture is generated here: a project whose
//! assets are scattered over several folders, an image sequence, an unused
//! asset, a missing one, a solid, nested comps and a text layer.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bae_model::asset::hash_file;
use bae_model::{AssetFile, AuthorId, CommandRegistry, PrimPath, Project, Stage};
use bae_package::{
    AssetLocation, AssetLocator, AssetStatus, FoundVia, PackOptions, PackSource, PackageError,
    PackageReader, WarningCode, WriteOptions, collect_files, open_project, pack, scan, unpack,
};
use serde_json::{Value as Json, json};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn who() -> AuthorId {
    AuthorId::new("user:test").unwrap()
}

/// Deterministic pseudo-random bytes (stand-in for media; packaging never decodes).
fn bytes(seed: u32, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn write(path: &Path, data: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    project: Project,
    registry: CommandRegistry,
}

/// The original files, with their content and modification time.
struct Originals(Vec<(PathBuf, Vec<u8>, SystemTime)>);

impl Originals {
    fn capture(paths: &[PathBuf]) -> Self {
        Originals(
            paths
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        fs::read(p).unwrap(),
                        fs::metadata(p).unwrap().modified().unwrap(),
                    )
                })
                .collect(),
        )
    }

    fn assert_unchanged(&self) {
        for (p, data, mtime) in &self.0 {
            assert_eq!(
                &fs::read(p).unwrap(),
                data,
                "{} content changed",
                p.display()
            );
            assert_eq!(
                &fs::metadata(p).unwrap().modified().unwrap(),
                mtime,
                "{} mtime changed",
                p.display()
            );
        }
    }
}

impl Fixture {
    fn run(&mut self, id: &str, params: Json) -> Json {
        self.registry
            .execute(&mut self.project.stage, &who(), id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    fn create(&mut self, path: &str, t: &str) {
        self.run("prim.create", json!({"path": path, "prim_type": t}));
    }

    fn set(&mut self, path: &str, attr: &str, value: Json) {
        self.run(
            "attr.set",
            json!({"path": path, "attr": attr, "value": value}),
        );
    }

    fn asset(&mut self, path: &str, kind: &str, file: Option<&Path>) {
        self.create(path, "Asset");
        self.set(path, "kind", json!({"type": "enum", "value": kind}));
        if let Some(f) = file {
            let base = self.project.base_dir().map(Path::to_owned);
            let file = AssetFile::from_path(f, base.as_deref(), true).unwrap();
            self.run("asset.set_file", json!({"path": path, "file": file}));
        }
    }

    fn layer(&mut self, path: &str, kind: &str, source: Option<(&str, &str)>) {
        self.create(path, "Layer");
        self.set(path, "kind", json!({"type": "enum", "value": kind}));
        if let Some((ty, target)) = source {
            self.set(path, "source", json!({"type": ty, "value": target}));
        }
    }

    /// Asset files: a (next to the project), b (another folder), seq (third
    /// folder, 3 frames + an unrelated file), c (unused), d (missing), a solid.
    fn new() -> (Self, Originals) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let a = root.join("work/footage/a.mov");
        let b = root.join("shared/stills/b.png");
        let seq: Vec<PathBuf> = (1..=3)
            .map(|i| root.join(format!("renders/seq/shot_{i:04}.png")))
            .collect();
        let c = root.join("music/c.wav");
        write(&a, &bytes(1, 64 * 1024));
        write(&b, &bytes(2, 3000));
        for (i, f) in seq.iter().enumerate() {
            write(f, &bytes(10 + i as u32, 2000 + i));
        }
        write(&root.join("renders/seq/notes.txt"), b"not a frame");
        write(&c, &bytes(3, 5000));

        let mut registry = CommandRegistry::with_builtins();
        bae_package::register_commands(&mut registry);
        let mut project = Project::new();
        project.save_as(root.join("work/Film.bae")).unwrap();
        let mut fx = Fixture {
            _tmp: tmp,
            root: root.clone(),
            project,
            registry,
        };

        fx.create("/assets", "Folder");
        fx.asset("/assets/a", "footage", Some(&a));
        fx.asset("/assets/b", "footage", Some(&b));
        fx.asset("/assets/seq", "sequence", Some(&seq[0]));
        fx.set(
            "/assets/seq",
            "sequence:pattern",
            json!({"type": "string", "value": "shot_####.png"}),
        );
        fx.asset("/assets/c", "footage", Some(&c));
        fx.asset("/assets/d", "footage", None);
        let gone = AssetFile {
            relative: Some("../gone/d.mov".into()),
            original: bae_model::asset::to_slash(&root.join("gone/d.mov")),
            size: 1234,
            mtime_ms: None,
            hash: None,
        };
        fx.run("asset.set_file", json!({"path": "/assets/d", "file": gone}));
        fx.asset("/assets/solid", "solid", None);

        fx.create("/comps", "Folder");
        fx.create("/comps/pre", "Comp");
        fx.create("/comps/pre/layers", "Folder");
        fx.layer(
            "/comps/pre/layers/still",
            "footage",
            Some(("asset_ref", "/assets/b")),
        );
        fx.layer(
            "/comps/pre/layers/seq",
            "footage",
            Some(("asset_ref", "/assets/seq")),
        );
        fx.layer("/comps/pre/layers/title", "text", None);
        fx.set(
            "/comps/pre/layers/title",
            "text:source",
            json!({"type": "text_document", "value": {"text": "Hi", "font": "Inter-Regular", "font_size": 72.0}}),
        );
        fx.create("/comps/main", "Comp");
        fx.create("/comps/main/layers", "Folder");
        fx.layer(
            "/comps/main/layers/plate",
            "footage",
            Some(("asset_ref", "/assets/a")),
        );
        fx.layer(
            "/comps/main/layers/bg",
            "solid",
            Some(("prim_ref", "/assets/solid")),
        );
        fx.layer(
            "/comps/main/layers/nest",
            "pre_comp",
            Some(("prim_ref", "/comps/pre")),
        );
        fx.create("/comps/other", "Comp");
        fx.create("/comps/other/layers", "Folder");
        fx.layer(
            "/comps/other/layers/x",
            "footage",
            Some(("asset_ref", "/assets/d")),
        );
        fx.project.save().unwrap();

        let mut files = vec![a, b, c];
        files.extend(seq);
        (fx, Originals::capture(&files))
    }

    fn source(&self) -> PackSource<'_> {
        PackSource::project(&self.project)
    }

    /// Rename every original asset folder: the package must not need them.
    fn move_originals_away(&self) {
        for d in ["work/footage", "shared", "renders", "music"] {
            let from = self.root.join(d);
            let to = self.root.join(format!("{}-moved", d.replace('/', "-")));
            fs::rename(from, to).unwrap();
        }
    }
}

fn status(report: &bae_package::PackageReport, path: &str) -> AssetStatus {
    report.asset(&p(path)).unwrap().status
}

#[test]
fn collects_used_unused_fonts_through_nested_comps() {
    let (fx, _) = Fixture::new();
    let all = bae_package::collect::collect(&fx.project.stage, &Default::default());
    let used: Vec<&str> = all.used().map(|a| a.path.as_str()).collect();
    assert_eq!(
        used,
        [
            "/assets/a",
            "/assets/b",
            "/assets/seq",
            "/assets/d",
            "/assets/solid"
        ]
    );
    let unused: Vec<&str> = all.unused().map(|a| a.path.as_str()).collect();
    assert_eq!(unused, ["/assets/c"]);
    assert_eq!(all.fonts.len(), 1);
    assert_eq!(all.fonts[0].name, "Inter-Regular");
    assert_eq!(all.fonts[0].users, [p("/comps/pre/layers/title")]);

    // Scoped to the main comp: the pre-comp's assets count, `other` does not.
    let main = bae_package::collect::collect(
        &fx.project.stage,
        &bae_package::CollectOptions {
            roots: vec![p("/comps/main")],
        },
    );
    let used: Vec<&str> = main.used().map(|a| a.path.as_str()).collect();
    assert_eq!(
        used,
        ["/assets/a", "/assets/b", "/assets/seq", "/assets/solid"]
    );
    assert_eq!(
        main.asset(&p("/assets/b")).unwrap().users,
        [p("/comps/pre/layers/still")]
    );
}

#[test]
fn baez_opens_complete_after_the_originals_are_moved() {
    let (fx, originals) = Fixture::new();
    let out = fx.root.join("handoff/Film.baez");
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    let report = pack(
        &fx.source(),
        &out,
        &PackOptions::default(),
        WriteOptions::default(),
    )
    .unwrap();

    assert_eq!(status(&report, "/assets/a"), AssetStatus::Packed);
    assert_eq!(status(&report, "/assets/c"), AssetStatus::Packed);
    assert_eq!(status(&report, "/assets/d"), AssetStatus::Missing);
    assert_eq!(status(&report, "/assets/solid"), AssetStatus::NoFile);
    let seq = report.asset(&p("/assets/seq")).unwrap();
    assert_eq!(seq.files, 3, "every frame, not the notes file");
    assert_eq!(
        seq.packaged_as.as_deref(),
        Some("assets/shot/shot_0001.png")
    );
    assert_eq!(
        report
            .asset(&p("/assets/a"))
            .unwrap()
            .packaged_as
            .as_deref(),
        Some("assets/a.mov")
    );
    assert_eq!(report.stats.packed, 4);
    assert_eq!(report.stats.missing, 1);
    assert_eq!(report.stats.files, 6);
    assert_eq!(
        report.stats.asset_bytes,
        64 * 1024 + 3000 + 5000 + 2000 + 2001 + 2002
    );
    assert!(report.stats.output_bytes.unwrap() > report.stats.asset_bytes);
    let codes: Vec<WarningCode> = report.warnings.iter().map(|w| w.code).collect();
    assert!(codes.contains(&WarningCode::MissingAssets));
    assert!(codes.contains(&WarningCode::FontsNotEmbedded));
    assert!(!report.is_self_contained());

    // Packing only read the originals; the open project is unchanged.
    originals.assert_unchanged();
    let a = fx.project.stage.prim(&p("/assets/a")).unwrap();
    assert_eq!(
        AssetFile::from_prim(&a).unwrap().relative.as_deref(),
        Some("../footage/a.mov")
    );

    fx.move_originals_away();
    let opened = open_project(&out).unwrap();
    assert_eq!(opened.missing(), [p("/assets/d")]);
    for (asset, original) in [
        ("/assets/a", &originals.0[0]),
        ("/assets/b", &originals.0[1]),
        ("/assets/c", &originals.0[2]),
        ("/assets/seq", &originals.0[3]),
    ] {
        let AssetLocation::Packaged { entry } = opened.locate(&p(asset)) else {
            panic!("{asset} not in the package");
        };
        let pkg = opened.assets.package().unwrap();
        assert_eq!(pkg.read_entry(&entry).unwrap(), original.1, "{asset}");
    }
    // Media entries are stored: a plain byte range of the .baez.
    let pkg = PackageReader::open(&out).unwrap();
    let entry = pkg.entry("assets/a.mov").unwrap().clone();
    assert!(entry.stored);
    let mut raw = fs::File::open(&out).unwrap();
    raw.seek(SeekFrom::Start(entry.offset + 100)).unwrap();
    let mut buf = [0u8; 16];
    raw.read_exact(&mut buf).unwrap();
    assert_eq!(buf, originals.0[0].1[100..116]);
    let mut reader = pkg.open_entry("assets/a.mov").unwrap();
    reader.seek(SeekFrom::End(-16)).unwrap();
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf, originals.0[0].1[64 * 1024 - 16..]);
    assert_eq!(reader.read(&mut buf).unwrap(), 0);
    // Project data is compressed.
    let mut zip = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
    assert_eq!(
        zip.by_name("project.json").unwrap().compression(),
        zip::CompressionMethod::Deflated
    );
    let manifest = opened.project.manifest.extra.get("package").unwrap();
    assert_eq!(manifest["fonts"], json!(["Inter-Regular"]));

    // Unpacked into a folder, every packed asset resolves by its relative path.
    let dir = fx.root.join("received/Film.bae");
    let unpacked = unpack(&out, &dir).unwrap();
    assert_eq!(unpacked.missing, [p("/assets/d")]);
    let opened = open_project(&dir).unwrap();
    match opened.locate(&p("/assets/seq")) {
        AssetLocation::File { path, via } => {
            assert_eq!(via, FoundVia::Relative);
            assert_eq!(fs::read(path).unwrap(), originals.0[3].1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        fs::read(dir.join("assets/shot/shot_0003.png")).unwrap(),
        originals.0[5].1
    );
}

#[test]
fn collect_files_writes_a_project_folder() {
    let (fx, originals) = Fixture::new();
    let dir = fx.root.join("collected/Film.bae");
    let report = collect_files(&fx.source(), &dir, &PackOptions::default()).unwrap();
    assert_eq!(report.stats.packed, 4);
    originals.assert_unchanged();
    // Copies keep the original modification time (fingerprints match).
    assert_eq!(
        fs::metadata(dir.join("assets/a.mov"))
            .unwrap()
            .modified()
            .unwrap(),
        originals.0[0].2
    );

    fx.move_originals_away();
    let opened = open_project(&dir).unwrap();
    assert_eq!(opened.missing(), [p("/assets/d")]);
    let AssetLocation::File { path, via } = opened.locate(&p("/assets/b")) else {
        panic!("b missing");
    };
    assert_eq!(via, FoundVia::Relative);
    assert_eq!(fs::read(path).unwrap(), originals.0[1].1);

    // The folder must be new or empty.
    let err = collect_files(&fx.source(), &dir, &PackOptions::default()).unwrap_err();
    assert!(matches!(err, PackageError::OutputNotEmpty(_)), "{err}");
}

#[test]
fn scope_unused_and_missing_options() {
    let (fx, _) = Fixture::new();
    let options = PackOptions {
        roots: vec![p("/comps/main")],
        include_unused: false,
        ..Default::default()
    };
    let report = scan(&fx.source(), &options).unwrap();
    assert_eq!(status(&report, "/assets/c"), AssetStatus::Excluded);
    // Missing, but outside the scope: excluded, not reported missing.
    assert_eq!(status(&report, "/assets/d"), AssetStatus::Excluded);
    assert_eq!(report.stats.missing, 0);
    assert_eq!(report.stats.packed, 3);
    assert!(report.stats.output_bytes.is_none());

    let out = fx.root.join("main.baez");
    pack(&fx.source(), &out, &options, WriteOptions::default()).unwrap();
    let pkg = PackageReader::open(&out).unwrap();
    assert!(pkg.contains("assets/a.mov"));
    assert!(!pkg.contains("assets/c.wav"));

    let strict = PackOptions {
        fail_on_missing: true,
        ..Default::default()
    };
    let err = pack(
        &fx.source(),
        &fx.root.join("x.baez"),
        &strict,
        WriteOptions::default(),
    )
    .unwrap_err();
    match err {
        PackageError::MissingAssets(paths) => assert_eq!(paths, [p("/assets/d")]),
        other => panic!("{other}"),
    }
    assert!(!fx.root.join("x.baez").exists());

    // Existing output: refused unless overwriting.
    let err = pack(&fx.source(), &out, &options, WriteOptions::default()).unwrap_err();
    assert!(matches!(err, PackageError::OutputExists(_)), "{err}");
    pack(
        &fx.source(),
        &out,
        &options,
        WriteOptions { overwrite: true },
    )
    .unwrap();
    // Never onto the project or an asset.
    let a = fx.root.join("work/footage/a.mov");
    let err = pack(&fx.source(), &a, &options, WriteOptions { overwrite: true }).unwrap_err();
    assert!(matches!(err, PackageError::OutputIsSource(_)), "{err}");

    // Trimming is reserved: whole files, with a warning.
    let trim = PackOptions {
        trim: Some(Default::default()),
        ..Default::default()
    };
    let report = scan(&fx.source(), &trim).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::TrimNotAvailable)
    );
}

#[test]
fn repacking_an_opened_package_needs_no_originals() {
    let (fx, originals) = Fixture::new();
    let first = fx.root.join("first.baez");
    pack(
        &fx.source(),
        &first,
        &PackOptions::default(),
        WriteOptions::default(),
    )
    .unwrap();
    fx.move_originals_away();

    let opened = open_project(&first).unwrap();
    let second = fx.root.join("out/second.baez");
    fs::create_dir_all(second.parent().unwrap()).unwrap();
    let report = pack(
        &PackSource::project(&opened.project),
        &second,
        &PackOptions::default(),
        WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(report.stats.packed, 4);
    assert!(
        report
            .asset(&p("/assets/a"))
            .unwrap()
            .source
            .as_deref()
            .unwrap()
            .ends_with("first.baez!/assets/a.mov")
    );
    let reopened = open_project(&second).unwrap();
    let AssetLocation::Packaged { entry } = reopened.locate(&p("/assets/seq")) else {
        panic!("seq");
    };
    assert_eq!(
        reopened
            .assets
            .package()
            .unwrap()
            .read_entry(&entry)
            .unwrap(),
        originals.0[3].1
    );
    assert_eq!(
        reopened
            .assets
            .package()
            .unwrap()
            .read_entry("assets/shot/shot_0002.png")
            .unwrap(),
        originals.0[4].1
    );
    // No staging leftovers next to the output.
    let names: Vec<_> = fs::read_dir(second.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["second.baez"]);
}

#[test]
fn relink_finds_the_other_moved_assets() {
    let (mut fx, _) = Fixture::new();
    // The whole tree moves to another "volume".
    let moved = fx.root.join("nas");
    fs::create_dir_all(&moved).unwrap();
    for d in ["shared", "renders", "music"] {
        fs::rename(fx.root.join(d), moved.join(d)).unwrap();
    }
    let locator = AssetLocator::for_project(&fx.project).unwrap();
    let missing = |fx: &Fixture, locator: &AssetLocator| -> Vec<String> {
        bae_package::collect::collect(&fx.project.stage, &Default::default())
            .assets
            .into_iter()
            .filter(|a| {
                a.file
                    .as_ref()
                    .is_some_and(|f| locator.locate(f).is_missing())
            })
            .map(|a| a.path.to_string())
            .collect()
    };
    assert_eq!(
        missing(&fx, &locator),
        ["/assets/b", "/assets/seq", "/assets/c", "/assets/d"]
    );

    let location = fx.project.location().unwrap().to_owned();
    let out = fx.run(
        "asset.relink",
        json!({
            "path": "/assets/b",
            "file": moved.join("shared/stills/b.png"),
            "project": location,
        }),
    );
    let relinked: Vec<&str> = out["relinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(relinked, ["/assets/b", "/assets/seq", "/assets/c"]);
    assert_eq!(
        out["mapping"]["to"],
        bae_model::asset::to_slash(&moved) + "/"
    );
    assert_eq!(out["still_missing"], json!(["/assets/d"]));
    assert_eq!(missing(&fx, &locator), ["/assets/d"]);
    let b = AssetFile::from_prim(&fx.project.stage.prim(&p("/assets/b")).unwrap()).unwrap();
    assert!(b.hash.is_some(), "hash kept when the old reference had one");

    // One undo step.
    fx.project.stage.undo(&who()).unwrap();
    assert_eq!(
        missing(&fx, &locator),
        ["/assets/b", "/assets/seq", "/assets/c", "/assets/d"]
    );
}

#[test]
fn set_files_writes_where_the_winning_opinion_is() {
    let (mut fx, _) = Fixture::new();
    // A stronger sublayer overrides the relative path; the edit target is root.
    fx.run("sublayer.create", json!({"id": "anim/shot010", "index": 0}));
    fx.run("sublayer.set_edit_target", json!({"id": "anim/shot010"}));
    fx.set(
        "/assets/a",
        "file:relative",
        json!({"type": "string", "value": "../somewhere/a.mov"}),
    );
    fx.run("sublayer.set_edit_target", json!({"id": "root"}));

    let mut file = AssetFile::from_prim(&fx.project.stage.prim(&p("/assets/a")).unwrap()).unwrap();
    file.relative = Some("assets/a.mov".into());
    fx.run(
        "asset.set_files",
        json!({"assets": [{"path": "/assets/a", "file": file}]}),
    );
    let a = fx.project.stage.prim(&p("/assets/a")).unwrap();
    assert_eq!(
        a.value("file:relative").and_then(|v| v.as_str()),
        Some("assets/a.mov")
    );
    assert_eq!(
        a.attr_source("file:relative").unwrap().as_str(),
        "anim/shot010"
    );
    // `file:original` stayed where it was authored.
    assert_eq!(a.attr_source("file:original").unwrap().as_str(), "root");
}

#[test]
fn commands_pack_scan_collect_unpack() {
    let (mut fx, originals) = Fixture::new();
    let location = fx.project.location().unwrap().to_owned();
    let out = fx.root.join("cmd.baez");
    let report = fx.run(
        "package.pack",
        json!({"output": out, "project": location, "include_unused": false}),
    );
    assert_eq!(report["kind"], "baez");
    assert_eq!(report["stats"]["packed"], 3);
    assert_eq!(report["stats"]["excluded"], 1);
    let scan = fx.run("package.scan", json!({"project": location}));
    assert_eq!(scan["stats"]["missing"], 1);
    let dir = fx.root.join("cmd.bae");
    let collected = fx.run(
        "package.collect",
        json!({"output": dir, "project": location}),
    );
    assert_eq!(collected["kind"], "folder");
    let unpacked = fx.run(
        "package.unpack",
        json!({"package": out, "output": fx.root.join("cmd-unpacked.bae")}),
    );
    assert_eq!(unpacked["missing"], json!(["/assets/d"]));
    originals.assert_unchanged();

    // Every command is described for agents.
    for id in [
        "package.scan",
        "package.pack",
        "package.collect",
        "package.unpack",
        "asset.relink",
        "asset.set_files",
    ] {
        let info = fx.registry.get(id).unwrap().info();
        assert!(info.params_schema["properties"].is_object(), "{id}");
    }
}

#[test]
fn path_mappings_and_packages_resolve_before_search() {
    let (fx, _) = Fixture::new();
    // A reference written on Windows: Z:/stills/b.png, Z: is the "NAS" here.
    let file = AssetFile {
        relative: None,
        original: "Z:/stills/b.png".into(),
        size: 3000,
        mtime_ms: None,
        hash: Some(hash_file(&fx.root.join("shared/stills/b.png")).unwrap()),
    };
    let mapping = bae_model::PathMapping {
        from: "Z:/".into(),
        to: bae_model::asset::to_slash(&fx.root.join("shared")) + "/",
    };
    let locator = AssetLocator::new(None, vec![mapping]);
    assert!(matches!(
        locator.locate(&file),
        AssetLocation::File {
            via: FoundVia::Mapped,
            ..
        }
    ));
    // Without the mapping, a search directory finds it by name + fingerprint.
    let locator = AssetLocator::new(None, Vec::new()).with_search_dirs([fx.root.clone()]);
    assert!(matches!(
        locator.locate(&file),
        AssetLocation::File {
            via: FoundVia::Searched,
            ..
        }
    ));
    // UNC paths map the same way.
    let unc = bae_model::PathMapping {
        from: "//server/share/".into(),
        to: "/Volumes/share/".into(),
    };
    assert_eq!(
        unc.apply("//server/share/proj/a.mov").unwrap(),
        "/Volumes/share/proj/a.mov"
    );
}

#[test]
fn stage_only_source_works_for_unsaved_projects() {
    let (fx, _) = Fixture::new();
    // Same stage, no location: relative paths cannot resolve, originals can.
    let stage: &Stage = &fx.project.stage;
    let report = scan(&PackSource::stage(stage, None), &PackOptions::default()).unwrap();
    assert_eq!(report.stats.packed, 4);
    assert_eq!(
        report.asset(&p("/assets/a")).unwrap().source.as_deref(),
        Some(bae_model::asset::to_slash(&fx.root.join("work/footage/a.mov")).as_str())
    );
}

#[test]
fn assets_sharing_files_are_packed_once() {
    let (mut fx, originals) = Fixture::new();
    // A second asset on the same movie, and one on another frame of the same sequence.
    let a = fx.root.join("work/footage/a.mov");
    let frame2 = fx.root.join("renders/seq/shot_0002.png");
    fx.asset("/assets/a2", "footage", Some(&a));
    fx.asset("/assets/seq2", "sequence", Some(&frame2));
    let out = fx.root.join("shared.baez");
    let report = pack(
        &fx.source(),
        &out,
        &PackOptions::default(),
        WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(report.stats.packed, 6);
    assert_eq!(report.stats.files, 6, "no file twice");
    assert_eq!(
        report
            .asset(&p("/assets/a2"))
            .unwrap()
            .packaged_as
            .as_deref(),
        Some("assets/a.mov")
    );
    let seq2 = report.asset(&p("/assets/seq2")).unwrap();
    assert_eq!(
        seq2.packaged_as.as_deref(),
        Some("assets/shot/shot_0002.png")
    );
    assert_eq!(seq2.files, 3);

    fx.move_originals_away();
    let opened = open_project(&out).unwrap();
    let AssetLocation::Packaged { entry } = opened.locate(&p("/assets/seq2")) else {
        panic!("seq2");
    };
    assert_eq!(
        opened.assets.package().unwrap().read_entry(&entry).unwrap(),
        originals.0[4].1
    );
}

/// Issue #131: packing keeps the manifest's other fields — the desktop app's
/// saved view (open comps), path mappings — even through the commands, which
/// only see the stage.
#[test]
fn packing_keeps_the_manifest_fields() {
    let (mut fx, _) = Fixture::new();
    let view =
        json!({"comps": [{"path": "/comps/main", "time": 705600000}], "active": "/comps/main"});
    fx.project
        .manifest
        .extra
        .insert("view".into(), view.clone());
    fx.project.manifest.path_mappings = vec![bae_model::PathMapping {
        from: "Z:/".into(),
        to: "/Volumes/NAS/".into(),
    }];
    let location = fx.project.location().unwrap().to_owned();
    fx.project.save().unwrap();
    let manifest_of = |path: &std::path::Path| bae_package::read_manifest(path).unwrap();
    assert_eq!(manifest_of(&location).extra.get("view"), Some(&view));

    // The command (MCP's package_pack) with the project's location.
    let out = fx.root.join("视图 保留.baez");
    fx.run("package.pack", json!({"output": out, "project": location}));
    let packed = manifest_of(&out);
    assert_eq!(
        packed.extra.get("view"),
        Some(&view),
        "view lost by package.pack"
    );
    assert_eq!(packed.path_mappings.len(), 1);
    assert!(packed.extra.contains_key("package"), "package note");
    // The library call with the opened project, and a package packed again.
    let opened = Project::open(&out).unwrap();
    let again = fx.root.join("again.baez");
    bae_package::pack(
        &bae_package::PackSource::project(&opened),
        &again,
        &bae_package::PackOptions::default(),
        bae_package::WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(manifest_of(&again).extra.get("view"), Some(&view));
}
