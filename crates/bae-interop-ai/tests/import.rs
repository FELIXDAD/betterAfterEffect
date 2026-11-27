//! End to end: an Illustrator document built in code → command batch → stage
//! → evaluated by bae-anim → drawn by bae-vector, compared with the document's
//! own shape tree.

use bae_anim::evaluate_comp;
use bae_interop_ai::{
    AiDocument, AiGroup, AiItem, AiLayer, AiPath, AiText, Artboard, CommandBatch, ImportOptions,
    TextMode, plan_import,
};
use bae_model::{
    AuthorId, BezierPath, CommandRegistry, LayerKind, PrimPath, PrimType, Stage, Time, Value,
};
use bae_vector::kurbo::{Affine, BezPath, PathEl, Point};
use bae_vector::{
    BlendMode, Brush, Composite, DrawItem, FillRule, Gradient, GradientKind, GradientStop, LineCap,
    LineJoin, Paint, PaintKind, StrokeStyle, VectorScene, shape_layer_scene,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> BezierPath {
    BezierPath {
        vertices: vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        in_tangents: vec![[0.0; 2]; 4],
        out_tangents: vec![[0.0; 2]; 4],
        closed: true,
        feather: Vec::new(),
    }
}

/// A circle as four cubic segments (Illustrator's ellipse tool).
fn circle(cx: f64, cy: f64, r: f64) -> BezierPath {
    let k = r * 0.552_284_749_8;
    BezierPath {
        vertices: vec![[cx, cy - r], [cx + r, cy], [cx, cy + r], [cx - r, cy]],
        in_tangents: vec![[-k, 0.0], [0.0, -k], [k, 0.0], [0.0, k]],
        out_tangents: vec![[k, 0.0], [0.0, k], [-k, 0.0], [0.0, -k]],
        closed: true,
        feather: Vec::new(),
    }
}

fn fill(color: [f32; 4], rule: FillRule) -> Paint {
    Paint {
        kind: PaintKind::Fill(rule),
        brush: Brush::Solid(color),
        opacity: 1.0,
        blend: BlendMode::Normal,
        composite: Composite::Below,
    }
}

fn stroke(color: [f32; 4], width: f64) -> Paint {
    Paint {
        kind: PaintKind::Stroke(StrokeStyle {
            width,
            cap: LineCap::Round,
            join: LineJoin::Bevel,
            miter_limit: 10.0,
            dashes: vec![6.0, 3.0],
            dash_offset: 1.0,
        }),
        brush: Brush::Solid(color),
        opacity: 0.8,
        blend: BlendMode::Normal,
        composite: Composite::Below,
    }
}

fn gradient(kind: GradientKind, start: Point, end: Point) -> Brush {
    Brush::Gradient(Gradient {
        kind,
        start,
        end,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: [1.0, 0.5, 0.0, 1.0],
            },
            GradientStop {
                offset: 0.4,
                color: [0.0, 0.25, 1.0, 0.5],
            },
            GradientStop {
                offset: 1.0,
                color: [0.0, 0.0, 0.0, 1.0],
            },
        ],
    })
}

/// 400 × 300 artboard: a text layer, a vector layer (group with a compound
/// path, a gradient circle, a radial-gradient path with text inside the
/// group), a layer with a clip group, a hidden layer.
fn document() -> AiDocument {
    let compound = AiPath {
        name: Some("Frame".into()),
        ..AiPath::new(vec![
            rect(20.0, 20.0, 160.0, 120.0),
            rect(60.0, 50.0, 80.0, 60.0),
        ])
    }
    .with_paint(stroke([0.0, 0.0, 1.0, 1.0], 4.0))
    .with_paint(fill([0.8, 0.1, 0.1, 1.0], FillRule::EvenOdd));
    let gradient_circle = AiPath {
        opacity: 0.5,
        blend: BlendMode::Multiply,
        ..AiPath::new(vec![circle(300.0, 80.0, 50.0)])
    }
    .with_paint(Paint {
        brush: gradient(
            GradientKind::Linear,
            Point::new(250.0, 80.0),
            Point::new(350.0, 80.0),
        ),
        ..fill([0.0; 4], FillRule::NonZero)
    });
    let radial = AiPath::new(vec![rect(220.0, 180.0, 120.0, 90.0)]).with_paint(Paint {
        brush: gradient(
            GradientKind::Radial {
                highlight_length: 25.0,
                highlight_angle: 30.0,
            },
            Point::new(280.0, 225.0),
            Point::new(340.0, 225.0),
        ),
        ..fill([0.0; 4], FillRule::NonZero)
    });
    let mut logo = AiGroup::new(Some("Logo".into()));
    logo.opacity = 0.9;
    let mut inner_text = AiText::new("Inside", "MyriadPro-Regular", 12.0);
    inner_text.position = [230.0, 200.0];
    logo.items = vec![
        AiItem::Path(compound),
        AiItem::Text(inner_text),
        AiItem::Path(gradient_circle),
        AiItem::Path(radial),
    ];
    let mut shapes = AiLayer::new("图层 1");
    shapes.items = vec![AiItem::Group(logo)];

    let mut title = AiText::new("Hello\rWorld", "MyriadPro-Bold", 36.0);
    title.position = [20.0, 280.0];
    title.fill = Some([0.2, 0.2, 0.2, 1.0]);
    title.rotation = -5.0;
    let mut text_layer = AiLayer::new("Text");
    text_layer.items = vec![AiItem::Text(title)];

    let mut clip = AiGroup::new(None);
    clip.clip = Some(vec![rect(0.0, 150.0, 200.0, 150.0)]);
    clip.items = vec![AiItem::Path(
        AiPath::new(vec![circle(100.0, 225.0, 90.0)])
            .with_paint(fill([0.1, 0.7, 0.2, 1.0], FillRule::NonZero)),
    )];
    let mut masked = AiLayer::new("Masked");
    masked.items = vec![AiItem::Group(clip)];

    let mut hidden = AiLayer::new("Hidden");
    hidden.visible = false;
    hidden.locked = true;
    hidden.items = vec![AiItem::Path(
        AiPath::new(vec![rect(0.0, 0.0, 10.0, 10.0)]).with_paint(fill([1.0; 4], FillRule::NonZero)),
    )];

    let mut artboard = Artboard::new("Artboard 1", [400.0, 300.0]);
    artboard.layers = vec![text_layer, shapes, masked, hidden];
    AiDocument {
        name: "poster".into(),
        artboards: vec![artboard],
        ..AiDocument::default()
    }
}

fn run(stage: &mut Stage, batch: &CommandBatch) {
    batch
        .execute(
            &CommandRegistry::with_builtins(),
            stage,
            &AuthorId::system(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

/// Display-list item with the transform applied to geometry and gradient
/// points, for comparing scenes built in different spaces.
#[derive(Debug)]
enum Flat {
    Paint {
        stroke: Option<StrokeStyle>,
        rule: Option<FillRule>,
        points: Vec<Point>,
        brush: Brush,
        opacity: f32,
    },
    Layer {
        opacity: f32,
        blend: BlendMode,
        items: Vec<Flat>,
    },
}

fn points(path: &BezPath, t: Affine) -> Vec<Point> {
    let mut out = Vec::new();
    for el in (t * path).elements() {
        match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => out.push(p),
            PathEl::QuadTo(a, b) => out.extend([a, b]),
            PathEl::CurveTo(a, b, c) => out.extend([a, b, c]),
            PathEl::ClosePath => {}
        }
    }
    out
}

fn brush(b: &Brush, t: Affine) -> Brush {
    match b {
        Brush::Gradient(g) => Brush::Gradient(Gradient {
            start: t * g.start,
            end: t * g.end,
            ..g.clone()
        }),
        solid => solid.clone(),
    }
}

fn flatten(items: &[DrawItem]) -> Vec<Flat> {
    items
        .iter()
        .map(|i| match i {
            DrawItem::Fill(f) => Flat::Paint {
                stroke: None,
                rule: Some(f.rule),
                points: points(&f.path, f.transform),
                brush: brush(&f.brush, f.transform),
                opacity: f.opacity,
            },
            DrawItem::Stroke(s) => Flat::Paint {
                stroke: Some(s.style.clone()),
                rule: None,
                points: points(&s.path, s.transform),
                brush: brush(&s.brush, s.transform),
                opacity: s.opacity,
            },
            DrawItem::Layer(l) => Flat::Layer {
                opacity: l.opacity,
                blend: l.blend,
                items: flatten(&l.items),
            },
        })
        .collect()
}

fn assert_same(actual: &[Flat], expected: &[Flat], at: &str) {
    assert_eq!(actual.len(), expected.len(), "{at}: item count");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        let at = format!("{at}[{i}]");
        match (a, e) {
            (
                Flat::Paint {
                    stroke: sa,
                    rule: ra,
                    points: pa,
                    brush: ba,
                    opacity: oa,
                },
                Flat::Paint {
                    stroke: se,
                    rule: re,
                    points: pe,
                    brush: be,
                    opacity: oe,
                },
            ) => {
                assert_eq!(sa, se, "{at}: stroke");
                assert_eq!(ra, re, "{at}: rule");
                assert!((oa - oe).abs() < 1e-6, "{at}: opacity {oa} vs {oe}");
                assert_eq!(pa.len(), pe.len(), "{at}: points");
                for (x, y) in pa.iter().zip(pe) {
                    assert!(x.distance(*y) < 1e-9, "{at}: {x:?} vs {y:?}");
                }
                match (ba, be) {
                    (Brush::Gradient(ga), Brush::Gradient(ge)) => {
                        assert_eq!(ga.kind, ge.kind, "{at}: gradient kind");
                        assert!(ga.start.distance(ge.start) < 1e-9, "{at}: start");
                        assert!(ga.end.distance(ge.end) < 1e-9, "{at}: end");
                        assert_eq!(ga.stops, ge.stops, "{at}: stops");
                    }
                    (a, e) => assert_eq!(a, e, "{at}: brush"),
                }
            }
            (
                Flat::Layer {
                    opacity: oa,
                    blend: ba,
                    items: ia,
                },
                Flat::Layer {
                    opacity: oe,
                    blend: be,
                    items: ie,
                },
            ) => {
                assert!((oa - oe).abs() < 1e-6, "{at}: layer opacity");
                assert_eq!(ba, be, "{at}: blend");
                assert_same(ia, ie, &at);
            }
            _ => panic!("{at}: {a:?} vs {e:?}"),
        }
    }
}

#[test]
fn artboard_becomes_a_comp_with_shape_and_text_layers() {
    let doc = document();
    let mut stage = Stage::new();
    let plan = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
    // Only model built-ins: the batch runs on a bare registry.
    assert!(
        plan.batch
            .commands
            .iter()
            .all(|s| s.command == "prim.create" || s.command == "attr.set")
    );
    run(&mut stage, &plan.batch);

    let comp = p("/comps/poster");
    assert_eq!(plan.report.comps, vec![comp.clone()]);
    assert_eq!(plan.report.artboards, vec![p("/artboards/Artboard_1")]);
    assert_eq!(plan.report.shape_layers, 3);
    assert_eq!(plan.report.text_layers, 2);
    assert_eq!(plan.report.paths, 5);

    let view = stage.prim(&comp).unwrap();
    assert_eq!(view.value("width").cloned(), Some(Value::Int(400)));
    assert_eq!(view.value("height").cloned(), Some(Value::Int(300)));
    assert_eq!(
        view.value("ai:artboard").cloned(),
        Some(Value::PrimRef(p("/artboards/Artboard_1")))
    );
    let art = stage.prim(&p("/artboards/Artboard_1")).unwrap();
    assert_eq!(art.prim_type(), Some(&PrimType::ARTBOARD));
    assert_eq!(
        art.value("size").cloned(),
        Some(Value::Vec2([400.0, 300.0]))
    );

    let evaluated = evaluate_comp(&stage, &comp, Time::ZERO).unwrap();
    let names: Vec<(&str, LayerKind)> = evaluated
        .layers
        .iter()
        .map(|l| (&*l.name, l.kind))
        .collect();
    // Layers panel order is kept; the text inside the group sits above it.
    assert_eq!(
        names,
        vec![
            ("Hello", LayerKind::Text),
            ("Inside", LayerKind::Text),
            ("图层 1", LayerKind::Shape),
            ("Masked", LayerKind::Shape),
            ("Hidden", LayerKind::Shape),
        ]
    );
    assert!(
        plan.report
            .warnings
            .iter()
            .any(|w| w.contains("text inside groups")),
        "{:?}",
        plan.report.warnings
    );

    // The shape layer draws exactly the document's own shape tree.
    let layer = &evaluated.layers[2];
    let mut warnings = Vec::new();
    let mut scene = shape_layer_scene(layer, &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    // Layer → comp: anchor (0, 0), position at the comp centre.
    scene.transform(Affine::translate((200.0, 150.0)));
    let expected = VectorScene::from_group(&doc.artboards[0].layers[1].to_shape_group());
    assert!(!expected.is_empty());
    assert_same(&flatten(&scene.items), &flatten(&expected.items), "layer 1");

    // Hidden and locked layers are imported switched off.
    let hidden = stage.prim(&p("/comps/poster/layers/Hidden")).unwrap();
    assert_eq!(hidden.value("enabled").cloned(), Some(Value::Bool(false)));
    assert_eq!(hidden.value("locked").cloned(), Some(Value::Bool(true)));

    // Text: one style per text, AE line breaks, baseline origin as position.
    let text = stage.prim(&p("/comps/poster/layers/Hello")).unwrap();
    let Some(Value::TextDocument(td)) = text.value("text:source").cloned() else {
        panic!("no text document")
    };
    assert_eq!(td.text, "Hello\rWorld");
    assert_eq!(td.style.font, "MyriadPro-Bold");
    assert_eq!(td.style.font_size, 36.0);
    assert_eq!(td.style.fill_color, [0.2, 0.2, 0.2, 1.0]);
    assert_eq!(
        text.value("transform:position").cloned(),
        Some(Value::Vec3([20.0, 280.0, 0.0]))
    );
    assert_eq!(
        text.value("transform:rotation").cloned(),
        Some(Value::Float(-5.0))
    );
}

#[test]
fn clip_group_becomes_a_layer_mask() {
    let doc = document();
    let mut stage = Stage::new();
    let plan = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
    run(&mut stage, &plan.batch);
    let mask = stage
        .prim(&p("/comps/poster/layers/Masked/masks/Mask_1"))
        .unwrap();
    assert_eq!(mask.prim_type(), Some(&PrimType::MASK));
    assert_eq!(mask.value("mode").cloned(), Some(Value::Enum("add".into())));
    // Layer space: the artboard rectangle (0, 150)–(200, 300) around the centre.
    let Some(Value::Bezier(b)) = mask.value("path").cloned() else {
        panic!("no mask path")
    };
    assert_eq!(b.vertices[0], [-200.0, 0.0]);
    assert_eq!(b.vertices[2], [0.0, 150.0]);
}

#[test]
fn import_is_one_undo_step_and_names_do_not_collide() {
    let doc = document();
    let mut stage = Stage::new();
    let author = AuthorId::system();
    let registry = CommandRegistry::with_builtins();
    let first = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
    first.batch.execute(&registry, &mut stage, &author).unwrap();
    let second = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
    second
        .batch
        .execute(&registry, &mut stage, &author)
        .unwrap();
    assert_eq!(second.report.comps, vec![p("/comps/poster_2")]);
    assert_eq!(second.report.artboards, vec![p("/artboards/Artboard_1_2")]);
    assert!(stage.has_prim(&p("/comps/poster_2/layers/Masked")));

    assert_eq!(stage.undo_label(&author), Some("Import poster"));
    stage.undo(&author).unwrap();
    assert!(!stage.has_prim(&p("/comps/poster_2")));
    assert!(stage.has_prim(&p("/comps/poster")));
    stage.undo(&author).unwrap();
    assert!(!stage.has_prim(&p("/comps")));
}

#[test]
fn failing_batch_applies_nothing() {
    let doc = document();
    let mut stage = Stage::new();
    let mut plan = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
    plan.batch.push(
        "attr.set",
        serde_json::json!({"path": "/nowhere", "attr": "x", "value": {"type": "int", "value": 1}}),
    );
    let err = plan
        .batch
        .execute(
            &CommandRegistry::with_builtins(),
            &mut stage,
            &AuthorId::system(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("attr.set"), "{err}");
    assert!(!stage.has_prim(&p("/comps")));
}

#[test]
fn batch_json_matches_the_agent_batch_tool() {
    let plan = plan_import(&document(), &Stage::new(), &ImportOptions::default()).unwrap();
    let json = serde_json::to_value(&plan.batch).unwrap();
    assert_eq!(json["label"], "Import poster");
    assert_eq!(json["commands"][0]["command"], "prim.create");
    assert_eq!(json["commands"][0]["params"]["path"], "/comps");
    let back: CommandBatch = serde_json::from_value(json).unwrap();
    assert_eq!(back, plan.batch);
}

#[test]
fn options_select_artboards_and_skip_text_and_hidden() {
    let mut doc = document();
    let mut second = Artboard::new("Artboard 2", [100.5, 50.0]);
    second.origin = [500.0, 0.0];
    doc.artboards.push(second);
    let options = ImportOptions {
        artboards: vec![1, 0],
        text: TextMode::Skip,
        skip_hidden: true,
        artboard_prims: false,
        fps: 24.0,
        ..ImportOptions::default()
    };
    let mut stage = Stage::new();
    let plan = plan_import(&doc, &stage, &options).unwrap();
    run(&mut stage, &plan.batch);
    assert_eq!(
        plan.report.comps,
        vec![p("/comps/poster_Artboard_2"), p("/comps/poster_Artboard_1")]
    );
    assert!(plan.report.artboards.is_empty());
    assert_eq!(plan.report.text_layers, 0);
    assert_eq!(plan.report.shape_layers, 2);
    let small = stage.prim(&p("/comps/poster_Artboard_2")).unwrap();
    assert_eq!(
        small.value("width").cloned(),
        Some(Value::Int(101)),
        "rounded up"
    );
    assert_eq!(
        small.value("frame_rate").cloned(),
        Some(Value::FrameRate(bae_model::FrameRate::FPS_24))
    );
    assert!(!stage.has_prim(&p("/comps/poster_Artboard_1/layers/Hidden")));

    let err = plan_import(
        &doc,
        &stage,
        &ImportOptions {
            artboards: vec![7],
            ..ImportOptions::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains('7'));
}

#[test]
fn ai_import_command_reports_unsupported_files() {
    let mut registry = CommandRegistry::with_builtins();
    bae_interop_ai::register_commands(&mut registry).unwrap();
    let info = registry.get("ai.import").unwrap().info().clone();
    assert!(info.params_schema["properties"]["file"].is_object());
    assert!(info.params_schema["properties"]["text"].is_object());

    let dir = std::env::temp_dir().join(format!("bae-interop-ai-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let legacy = dir.join("old.ai");
    std::fs::write(
        &legacy,
        b"%!PS-Adobe-3.0\n%%Creator: Adobe Illustrator(R) 8.0\n",
    )
    .unwrap();
    let mut stage = Stage::new();
    let err = registry
        .execute(
            &mut stage,
            &AuthorId::system(),
            "ai.import",
            serde_json::json!({ "file": legacy.to_string_lossy() }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("PDF Compatible"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
