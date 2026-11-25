//! End to end with real Illustrator output: documents built by
//! `bridges/ai/tests/make-fixtures.jsx` in Illustrator 30, exported by
//! `bridges/ai/bae-ai-export.jsx` (regenerate with `bridges/ai/tests/regen.sh`),
//! read here, planned, executed and evaluated into shape layers.
//!
//! Expected values come from the fixture script (artboard space, y down), and
//! for gradients from Illustrator's own SVG export of the same document.
//!
//! The checked-in data in `tests/data/illustrator` is tested everywhere (CI on
//! macOS and Windows has no Illustrator). `live_illustrator` drives a real
//! Illustrator instead and is skipped unless `BAE_TEST_ILLUSTRATOR=1` (macOS).

use std::path::{Path, PathBuf};
use std::process::Command;

use bae_anim::evaluate_comp;
use bae_interop_ai::{
    AiDocument, AiItem, AiLayer, AiPath, AiText, BridgeOptions, ImportOptions, plan_import,
    read_bridge_with, read_document,
};
use bae_model::{AuthorId, BezierPath, CommandRegistry, LayerKind, PrimPath, Stage, Time, Value};
use bae_vector::{
    BlendMode, Brush, FillRule, GradientKind, LineCap, LineJoin, PaintKind, VectorScene,
    kurbo::Point, shape_layer_scene,
};

const FIXTURES: [&str; 5] = ["basic", "paths", "paint", "structure", "text"];

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The checked-in Illustrator 30 output.
fn checked_in() -> PathBuf {
    crate_dir().join("tests").join("data").join("illustrator")
}

fn read(dir: &Path, name: &str) -> AiDocument {
    read_document(&dir.join(format!("{name}.bae-ai.json")))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn layer<'d>(doc: &'d AiDocument, artboard: usize, name: &str) -> &'d AiLayer {
    doc.artboards[artboard]
        .layers
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no layer {name} on artboard {artboard}"))
}

fn named<'d>(items: &'d [AiItem], name: &str) -> &'d AiItem {
    fn find<'d>(items: &'d [AiItem], name: &str) -> Option<&'d AiItem> {
        for item in items {
            let n = match item {
                AiItem::Group(g) => g.name.as_deref(),
                AiItem::Path(p) => p.name.as_deref(),
                AiItem::Text(t) => t.name.as_deref(),
            };
            if n == Some(name) {
                return Some(item);
            }
            if let AiItem::Group(g) = item
                && let Some(found) = find(&g.items, name)
            {
                return Some(found);
            }
        }
        None
    }
    find(items, name).unwrap_or_else(|| panic!("no item {name}"))
}

fn path<'d>(items: &'d [AiItem], name: &str) -> &'d AiPath {
    match named(items, name) {
        AiItem::Path(p) => p,
        other => panic!("{name} is not a path: {other:?}"),
    }
}

fn text<'d>(items: &'d [AiItem], name: &str) -> &'d AiText {
    match named(items, name) {
        AiItem::Text(t) => t,
        other => panic!("{name} is not text: {other:?}"),
    }
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn close2(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    close(a[0], b[0], tol) && close(a[1], b[1], tol)
}

/// Bounding box of a path's vertices.
fn vertex_box(p: &BezierPath) -> [f64; 4] {
    let mut r = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for v in &p.vertices {
        r = [
            r[0].min(v[0]),
            r[1].min(v[1]),
            r[2].max(v[0]),
            r[3].max(v[1]),
        ];
    }
    r
}

fn assert_box(p: &BezierPath, expected: [f64; 4], what: &str) {
    let b = vertex_box(p);
    for k in 0..4 {
        assert!(
            close(b[k], expected[k], 1e-3),
            "{what}: box {b:?}, expected {expected:?}"
        );
    }
}

fn solid(b: &Brush) -> [f32; 4] {
    match b {
        Brush::Solid(c) => *c,
        other => panic!("not solid: {other:?}"),
    }
}

/// Imports the document into a fresh stage; returns the stage and the plan's comps.
fn import(doc: &AiDocument) -> (Stage, Vec<PrimPath>, bae_interop_ai::ImportReport) {
    let mut stage = Stage::new();
    let plan = plan_import(doc, &stage, &ImportOptions::default()).unwrap();
    plan.batch
        .execute(
            &CommandRegistry::with_builtins(),
            &mut stage,
            &AuthorId::system(),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", doc.name));
    (stage, plan.report.comps.clone(), plan.report)
}

#[test]
fn every_fixture_reads_and_imports() {
    check_every_fixture(&checked_in());
}

#[test]
fn basic_document() {
    check_basic(&checked_in());
}

#[test]
fn paths_document() {
    check_paths(&checked_in());
}

#[test]
fn paint_document() {
    check_paint(&checked_in());
}

#[test]
fn structure_document() {
    check_structure(&checked_in());
}

#[test]
fn text_document() {
    check_text(&checked_in());
}

/// File > Import and agents: `ai.import` with a bridge file is one undo step.
#[test]
fn ai_import_command_reads_bridge_files() {
    let mut registry = CommandRegistry::with_builtins();
    bae_interop_ai::register_commands(&mut registry).unwrap();
    let mut stage = Stage::new();
    let author = AuthorId::system();
    let file = checked_in().join("basic.bae-ai.json");
    let out = registry
        .execute(
            &mut stage,
            &author,
            "ai.import",
            serde_json::json!({ "file": file.to_string_lossy() }),
        )
        .unwrap();
    let report = &out.output;
    assert_eq!(report["source"], "bridge", "{report}");
    assert_eq!(
        report["comps"].as_array().map(Vec::len),
        Some(2),
        "{report}"
    );
    assert_eq!(report["text_layers"], 1, "{report}");
    let comp = PrimPath::new(report["comps"][0].as_str().unwrap()).unwrap();
    assert!(stage.prim(&comp).is_some());
    stage.undo(&author).expect("one undo step");
    assert!(stage.prim(&comp).is_none());
}

fn check_every_fixture(dir: &Path) {
    for name in FIXTURES {
        let doc = read(dir, name);
        assert_eq!(doc.name, name);
        assert!(!doc.artboards.is_empty(), "{name}");
        let (stage, comps, report) = import(&doc);
        assert_eq!(comps.len(), doc.artboards.len(), "{name}");
        // Every shape layer draws something.
        for comp in &comps {
            let evaluated = evaluate_comp(&stage, comp, Time::ZERO).unwrap();
            for l in evaluated
                .layers
                .iter()
                .filter(|l| l.kind == LayerKind::Shape)
            {
                let mut warnings = Vec::new();
                let scene = shape_layer_scene(l, &mut warnings);
                assert!(
                    !scene.items.is_empty(),
                    "{name}: empty shape layer {}",
                    l.name
                );
            }
        }
        assert!(report.shape_layers + report.text_layers > 0, "{name}");
    }
}

fn check_basic(dir: &Path) {
    let doc = read(dir, "basic");
    assert_eq!(doc.artboards.len(), 2);
    let poster = &doc.artboards[0];
    assert_eq!(poster.name, "Poster");
    assert_eq!(poster.size, [400.0, 300.0]);
    let badge = &doc.artboards[1];
    assert_eq!(badge.name, "Badge");
    assert_eq!(badge.size, [200.0, 100.0]);
    assert_eq!(badge.origin[0] - poster.origin[0], 450.0);

    let names: Vec<&str> = poster.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Text", "图层 1", "Masked", "Hidden"]);
    let hidden = layer(&doc, 0, "Hidden");
    assert!(!hidden.visible && hidden.locked);
    // The badge ellipse belongs to the second artboard only.
    assert_eq!(badge.layers.len(), 1);
    assert_eq!(badge.layers[0].name, "图层 1");
    let AiItem::Path(ellipse) = &badge.layers[0].items[0] else {
        panic!()
    };
    assert_box(&ellipse.subpaths[0], [20.0, 10.0, 180.0, 90.0], "badge");

    let shapes = &layer(&doc, 0, "图层 1").items;
    let AiItem::Group(logo) = named(shapes, "Logo") else {
        panic!()
    };
    assert!(close(logo.opacity, 0.9, 1e-6));
    let frame = path(shapes, "Frame");
    assert_eq!(frame.subpaths.len(), 2);
    // Sub-paths in Illustrator's order: the hole was drawn last, so it is on top.
    assert_box(&frame.subpaths[0], [60.0, 50.0, 140.0, 110.0], "hole");
    assert_box(&frame.subpaths[1], [20.0, 20.0, 180.0, 140.0], "outer");
    // Stroke above fill, as in Illustrator's Appearance panel.
    let PaintKind::Stroke(s) = &frame.paints[0].kind else {
        panic!()
    };
    assert_eq!(
        (s.width, s.cap, s.join),
        (4.0, LineCap::Round, LineJoin::Bevel)
    );
    assert_eq!(s.dashes, [6.0, 3.0]);
    assert_eq!(frame.paints[1].kind, PaintKind::Fill(FillRule::EvenOdd));
    assert_eq!(solid(&frame.paints[1].brush), [0.8, 0.102, 0.102, 1.0]);

    // The circle: three-stop linear gradient, 50 % multiply.
    let circle = logo
        .items
        .iter()
        .find_map(|i| match i {
            AiItem::Path(p) if p.blend == BlendMode::Multiply => Some(p),
            _ => None,
        })
        .unwrap();
    assert!(close(circle.opacity, 0.5, 1e-6));
    let Brush::Gradient(g) = &circle.paints[0].brush else {
        panic!()
    };
    assert_eq!(g.kind, GradientKind::Linear);
    assert_eq!(g.stops.len(), 3);
    assert!(close(g.stops[1].offset as f64, 0.4, 1e-6));
    // Default gradient across the circle's bounds: (250, 80) → (350, 80).
    assert!(
        close2([g.start.x, g.start.y], [250.0, 80.0], 1.0),
        "{:?}",
        g.start
    );
    assert!(
        close2([g.end.x, g.end.y], [350.0, 80.0], 1.0),
        "{:?}",
        g.end
    );

    // Clip group: mask rect (0, 150, 200, 150).
    let masked = &layer(&doc, 0, "Masked").items;
    let AiItem::Group(clip) = &masked[0] else {
        panic!()
    };
    let mask = clip.clip.as_ref().expect("clip path");
    assert_box(&mask[0], [0.0, 150.0, 200.0, 300.0], "mask");

    let AiItem::Text(hello) = &layer(&doc, 0, "Text").items[0] else {
        panic!()
    };
    assert_eq!(hello.text, "Hello");
    assert_eq!(hello.font, "MyriadPro-Regular");
    assert_eq!(hello.size, 36.0);
    assert!(
        close2(hello.position, [20.0, 280.0], 1e-3),
        "{:?}",
        hello.position
    );
    assert_eq!(hello.fill, Some([0.2, 0.2, 0.2, 1.0]));

    // Into the stage: comps per artboard, hidden layer switched off.
    let (stage, comps, report) = import(&doc);
    assert_eq!(comps.len(), 2);
    assert_eq!(report.text_layers, 1);
    let evaluated = evaluate_comp(&stage, &comps[0], Time::ZERO).unwrap();
    let kinds: Vec<(&str, LayerKind)> = evaluated
        .layers
        .iter()
        .map(|l| (&*l.name, l.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Hello", LayerKind::Text),
            ("图层 1", LayerKind::Shape),
            ("Masked", LayerKind::Shape),
            ("Hidden", LayerKind::Shape),
        ]
    );
    let hidden = stage
        .prim(&PrimPath::new(&format!("{}/layers/Hidden", comps[0])).unwrap())
        .unwrap();
    assert_eq!(hidden.value("enabled").cloned(), Some(Value::Bool(false)));

    // The shape layer draws the document's own shape tree.
    let shape = evaluated
        .layers
        .iter()
        .find(|l| &*l.name == "图层 1")
        .unwrap();
    let mut warnings = Vec::new();
    let scene = shape_layer_scene(shape, &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    let expected = VectorScene::from_group(&layer(&doc, 0, "图层 1").to_shape_group());
    assert_eq!(scene.items.len(), expected.items.len());
}

fn check_paths(dir: &Path) {
    let doc = read(dir, "paths");
    let items = &layer(&doc, 0, "Paths").items;
    let names: Vec<&str> = items
        .iter()
        .map(|i| match i {
            AiItem::Path(p) => p.name.as_deref().unwrap_or(""),
            _ => "?",
        })
        .collect();
    assert_eq!(
        names,
        [
            "Wave", "Zigzag", "Dashed", "Rings", "Donut", "Triangle", "Rounded"
        ]
    );

    let wave = path(items, "Wave");
    assert!(!wave.subpaths[0].closed);
    assert_eq!(
        wave.subpaths[0].vertices,
        [[270.0, 260.0], [330.0, 260.0], [390.0, 260.0]]
    );
    assert_eq!(wave.subpaths[0].in_tangents[1], [-20.0, -20.0]);
    assert_eq!(wave.subpaths[0].out_tangents[1], [20.0, 20.0]);
    assert_eq!(wave.paints.len(), 1, "stroke only");

    let zig = path(items, "Zigzag");
    let PaintKind::Stroke(s) = &zig.paints[0].kind else {
        panic!()
    };
    assert_eq!(
        (s.width, s.miter_limit, s.cap, s.join),
        (6.0, 2.0, LineCap::Projecting, LineJoin::Miter)
    );

    let dashed = path(items, "Dashed");
    let PaintKind::Stroke(s) = &dashed.paints[0].kind else {
        panic!()
    };
    assert_eq!(
        (s.dashes.as_slice(), s.dash_offset),
        (&[10.0, 4.0, 2.0, 4.0][..], 5.0)
    );

    assert_eq!(
        path(items, "Donut").paints[0].kind,
        PaintKind::Fill(FillRule::EvenOdd)
    );
    assert_eq!(
        path(items, "Rings").paints[0].kind,
        PaintKind::Fill(FillRule::NonZero)
    );
    assert_eq!(
        path(items, "Triangle").subpaths[0].vertices,
        [[40.0, 20.0], [100.0, 120.0], [20.0, 120.0]]
    );
    let rounded = path(items, "Rounded");
    assert_eq!(rounded.subpaths[0].vertices.len(), 8);
    assert_box(&rounded.subpaths[0], [240.0, 20.0, 380.0, 100.0], "rounded");
}

fn check_paint(dir: &Path) {
    let doc = read(dir, "paint");
    let items = &layer(&doc, 0, "Paint").items;
    // CMYK, grey and spot colours arrive as RGB converted by Illustrator.
    let cmyk = solid(&path(items, "Cmyk").paints[0].brush);
    assert!(
        cmyk[0] > 0.9 && cmyk[1] > 0.4 && cmyk[1] < 0.6 && cmyk[2] < 0.2,
        "{cmyk:?}"
    );
    assert_eq!(
        solid(&path(items, "Gray").paints[0].brush),
        [0.7, 0.7, 0.7, 1.0]
    );
    let spot = solid(&path(items, "Spot").paints[0].brush);
    assert!(spot[0] > 0.95 && spot[2] < spot[1], "{spot:?}");

    let screen = path(items, "Screen");
    assert_eq!(screen.blend, BlendMode::Screen);
    assert!(close(screen.opacity, 0.7, 1e-6));
    assert_eq!(path(items, "Overlay").blend, BlendMode::Overlay);

    // Rotated linear gradient: Illustrator's SVG export puts it at
    // (22.68, 194.64) → (177.32, 105.36). The scripting DOM rounds the gradient
    // origin down to whole points, hence the tolerance.
    let Brush::Gradient(g) = &path(items, "LinRot").paints[0].brush else {
        panic!()
    };
    assert!(
        close2([g.start.x, g.start.y], [22.68, 194.64], 1.5),
        "{:?}",
        g.start
    );
    assert!(
        close2([g.end.x, g.end.y], [177.32, 105.36], 1.5),
        "{:?}",
        g.end
    );
    // Opacity stop and the moved midpoint (an extra stop at 30 %).
    let offsets: Vec<f32> = g.stops.iter().map(|s| s.offset).collect();
    assert_eq!(offsets, [0.0, 0.3, 1.0]);
    assert_eq!(g.stops[0].color[3], 0.5);
    assert!(close(g.stops[1].color[3] as f64, 0.75, 1e-6));

    // Elliptical radial gradient: centre and radius as in the SVG export, with a warning.
    let Brush::Gradient(g) = &path(items, "Ellipse").paints[0].brush else {
        panic!()
    };
    assert!(
        matches!(g.kind, GradientKind::Radial { highlight_length, .. } if highlight_length == 0.0)
    );
    assert!(close2([g.start.x, g.start.y], [300.0, 150.0], 1e-3));
    assert!(close((g.end - g.start).hypot(), 90.55, 0.01));
    assert!(
        doc.warnings.iter().any(|w| w.contains("elliptical")),
        "{:?}",
        doc.warnings
    );

    // Gradient on a stroke.
    let gs = path(items, "GradStroke");
    assert!(matches!(&gs.paints[0].kind, PaintKind::Stroke(s) if s.width == 12.0));
    let Brush::Gradient(g) = &gs.paints[0].brush else {
        panic!()
    };
    assert_eq!(
        (g.start, g.end),
        (Point::new(14.0, 280.0), Point::new(146.0, 280.0))
    );
}

fn check_structure(dir: &Path) {
    let doc = read(dir, "structure");
    let [a, b] = [&doc.artboards[0], &doc.artboards[1]];
    let names = |ab: &bae_interop_ai::Artboard| -> Vec<String> {
        ab.layers.iter().map(|l| l.name.clone()).collect()
    };
    // Empty layers on an artboard are left out; "Guides" only holds a guide and
    // an item far outside both artboards.
    assert_eq!(names(a), ["Top", "Nested", "Template"]);
    assert_eq!(names(b), ["Nested", "Symbols"]);
    assert!(
        doc.warnings.iter().any(|w| w.contains("1 item(s) outside")),
        "{:?}",
        doc.warnings
    );

    // Sublayers interleave with items in Layers panel order; the empty one is gone.
    let top: Vec<String> = layer(&doc, 0, "Top")
        .items
        .iter()
        .map(|i| match i {
            AiItem::Group(g) => format!("group {}", g.name.as_deref().unwrap_or("")),
            AiItem::Path(p) => p.name.clone().unwrap_or_default(),
            AiItem::Text(_) => "text".into(),
        })
        .collect();
    assert_eq!(top, ["ItemB", "group Sub1", "ItemA"]);

    // Nested groups and a nested clip group, placed by its clip path.
    let nested = &layer(&doc, 0, "Nested").items;
    let AiItem::Group(inner) = named(nested, "Inner") else {
        panic!()
    };
    let AiItem::Group(clip) = &inner.items[0] else {
        panic!()
    };
    assert_box(
        &clip.clip.as_ref().unwrap()[0],
        [150.0, 40.0, 250.0, 140.0],
        "nested clip",
    );
    // The bridge rect spans both artboards and appears on each, in its space.
    assert_box(
        &path(nested, "Bridge").subpaths[0],
        [250.0, 160.0, 470.0, 190.0],
        "bridge on A",
    );
    let on_b = &layer(&doc, 1, "Nested").items;
    assert_eq!(on_b.len(), 1);
    assert_box(
        &path(on_b, "Bridge").subpaths[0],
        [-150.0, 160.0, 70.0, 190.0],
        "bridge on B",
    );

    // The symbol instance is its expanded artwork: a 10-point star.
    let AiItem::Group(star) = named(&layer(&doc, 1, "Symbols").items, "StarInstance") else {
        panic!()
    };
    let AiItem::Group(art) = &star.items[0] else {
        panic!()
    };
    let AiItem::Path(p) = &art.items[0] else {
        panic!()
    };
    assert_eq!(p.subpaths[0].vertices.len(), 10);
    assert_eq!(p.paints.len(), 2);

    // The non-printing template layer comes in hidden and locked.
    let tmpl = layer(&doc, 0, "Template");
    assert!(!tmpl.visible && tmpl.locked);
}

fn check_text(dir: &Path) {
    let doc = read(dir, "text");
    let items = &layer(&doc, 0, "Text").items;
    let mixed = text(items, "Mixed");
    assert_eq!(mixed.text, "Big small\rcentred line");
    assert_eq!(mixed.size, 30.0);
    assert!(close2(mixed.position, [20.0, 50.0], 1e-3));

    let rotated = text(items, "Rotated");
    assert!(close(rotated.rotation, -30.0, 1e-3), "{}", rotated.rotation);
    assert!(
        close2(rotated.position, [200.0, 120.0], 1e-3),
        "{:?}",
        rotated.position
    );
    assert!(close(rotated.scale[0], 100.0, 0.01));

    let zh = text(items, "Chinese");
    assert_eq!(zh.text, "你好，世界");
    assert_eq!(zh.font, "SourceHanSansSC-Regular");
    assert_eq!(zh.stroke, Some(([0.0, 0.0, 1.0, 1.0], 1.0)));

    // Area and path text: point text at the frame, with warnings.
    let area = text(items, "Area");
    assert!(close(area.position[0], 220.0, 1e-3));
    assert!(doc.warnings.iter().any(|w| w.contains("area text")));
    assert!(doc.warnings.iter().any(|w| w.contains("several styles")));

    let (_, _, report) = import(&doc);
    assert_eq!(report.text_layers, 5);

    // As outlines: groups of glyph shapes, no text layers.
    let bytes = std::fs::read(dir.join("text.bae-ai.json")).unwrap();
    let outlined = read_bridge_with(
        "text",
        &bytes,
        &BridgeOptions {
            text_outlines: true,
        },
    )
    .unwrap();
    let AiItem::Group(g) = named(&layer(&outlined, 0, "Text").items, "Chinese") else {
        panic!("outlines")
    };
    assert_eq!(g.items.len(), 5, "five glyphs");
    let (_, _, report) = import(&outlined);
    assert_eq!(report.text_layers, 0);
    assert!(report.paths > 50);
}

// ---------------------------------------------------------------- real Illustrator

/// Runs `tools/ai/aictl`; returns its exit code and the result JSON it prints.
fn aictl(args: &[&str]) -> (i32, serde_json::Value) {
    let tool = crate_dir().join("../../tools/ai/aictl");
    let out = Command::new(&tool)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", tool.display()));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str(l).ok())
        .unwrap_or_else(|| {
            panic!(
                "aictl {args:?}: no result JSON ({})\n{stdout}\n{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            )
        });
    (out.status.code().unwrap_or(-1), json)
}

/// Real Illustrator, through `tools/ai/aictl`: the script channel's contract
/// (results, script errors), the fixture documents rebuilt by `regen.sh` into
/// a scratch folder and put through the same checks as the checked-in data,
/// and last a dialog opened by a script, which aictl recovers from by ending
/// Illustrator. Takes a minute or two.
///
/// `BAE_TEST_ILLUSTRATOR=1 cargo test -p bae-interop-ai --test illustrator live_illustrator -- --nocapture`
#[test]
fn live_illustrator() {
    if std::env::var_os("BAE_TEST_ILLUSTRATOR").is_none_or(|v| v != "1") {
        eprintln!(
            "skipped: needs Adobe Illustrator; set BAE_TEST_ILLUSTRATOR=1 on a Mac with Illustrator to run it"
        );
        return;
    }
    if !cfg!(target_os = "macos") {
        panic!("BAE_TEST_ILLUSTRATOR=1, but tools/ai/aictl only drives Illustrator on macOS");
    }

    // The script channel.
    let (code, r) = aictl(&["eval", "app.version"]);
    assert_eq!((code, &r["ok"]), (0, &true.into()), "{r}");
    let version = r["value"].as_str().unwrap_or_default().to_owned();
    assert!(version.starts_with(|c: char| c.is_ascii_digit()), "{r}");
    eprintln!("Illustrator {version}");
    let (code, r) = aictl(&["eval", "throw new Error('bae-live-test')"]);
    assert_eq!((code, &r["ok"]), (1, &false.into()), "{r}");
    assert!(
        r["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("bae-live-test")),
        "{r}"
    );

    // The fixtures, rebuilt and exported by the current scripts.
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("illustrator-live");
    let _ = std::fs::remove_dir_all(&out);
    let regen = crate_dir().join("../../bridges/ai/tests/regen.sh");
    let status = Command::new("sh")
        .arg(&regen)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run regen.sh");
    assert!(status.success(), "regen.sh: {status}");
    for name in FIXTURES {
        let ai = std::fs::read(out.join(format!("{name}.ai"))).unwrap();
        assert!(ai.starts_with(b"%PDF-"), "{name}.ai is not PDF compatible");
    }
    check_every_fixture(&out);
    check_basic(&out);
    check_paths(&out);
    check_paint(&out);
    check_structure(&out);
    check_text(&out);

    // Not an error (colour conversion follows Illustrator's colour settings),
    // but a sign that the checked-in data needs `regen.sh`.
    for name in FIXTURES {
        let json = |dir: &Path| -> serde_json::Value {
            let file = dir.join(format!("{name}.bae-ai.json"));
            let mut v: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            v.as_object_mut().unwrap().remove("host");
            v
        };
        if json(&out) != json(&checked_in()) {
            eprintln!(
                "note: {name}.bae-ai.json from Illustrator {version} differs from the checked-in copy (bridges/ai/tests/regen.sh refreshes it)"
            );
        }
    }

    // Last, because it ends Illustrator: a dialog the script opened is detected
    // and recovered from (the next aictl call starts Illustrator again).
    let (code, r) = aictl(&["eval", "alert('bae-live-test')", "--timeout", "120"]);
    assert_eq!((code, &r["error"]["recovered"]), (3, &true.into()), "{r}");
}
