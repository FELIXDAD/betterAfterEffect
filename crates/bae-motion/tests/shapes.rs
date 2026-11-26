//! `shape.add` and `mask.create` through the command registry: the structure AE's
//! shape and Pen tools create, numbering, fill / stroke, undo, and where the
//! result lands in the comp.

use bae_anim::glam::DVec3;
use bae_model::{AuthorId, BezierPath, CommandRegistry, PrimPath, Stage, Time, Value};
use bae_motion::query;
use serde_json::{Value as Json, json};

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

impl Fx {
    fn new() -> Self {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        Fx {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
        }
    }

    fn run(&mut self, id: &str, params: Json) -> Json {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    fn try_run(&mut self, id: &str, params: Json) -> Result<Json, String> {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .map(|o| o.output)
            .map_err(|e| e.to_string())
    }

    fn comp(&mut self) -> PrimPath {
        let out = self.run(
            "comp.create",
            json!({"width": 1920, "height": 1080, "fps": 30}),
        );
        p(out["path"].as_str().unwrap())
    }

    fn value(&self, path: &PrimPath, attr: &str) -> Option<Value> {
        query::static_value(&self.stage.prim(path).unwrap(), attr)
    }

    fn names(&self, parent: &PrimPath) -> Vec<String> {
        self.stage
            .children(parent)
            .iter()
            .map(|c| query::display_name_of(&self.stage, c))
            .collect()
    }

    fn contents(&self, group: &PrimPath) -> PrimPath {
        group.child("contents").unwrap()
    }

    /// Comp-space point of a layer-space point at time 0.
    fn to_comp(&self, comp: &PrimPath, layer: &PrimPath, x: f64, y: f64) -> DVec3 {
        let e = bae_anim::evaluate_comp(&self.stage, comp, Time::ZERO).unwrap();
        e.layer(layer)
            .unwrap()
            .world_matrix
            .transform_point3(DVec3::new(x, y, 0.0))
    }
}

fn fill() -> Json {
    json!({"color": [1.0, 0.0, 0.0, 1.0]})
}

fn stroke() -> Json {
    json!({"color": [1.0, 1.0, 1.0, 1.0], "width": 2.0})
}

#[test]
fn rectangle_into_a_new_shape_layer() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    // A 200×100 rectangle centered at comp (1060, 440): 100 right of and 100
    // above the comp center.
    let out = fx.run(
        "shape.add",
        json!({"comp": comp, "kind": "rect", "position": [100.0, -100.0],
               "size": [200.0, 100.0], "fill": fill(), "stroke": stroke(),
               "center_anchor": true}),
    );
    let layer = p(out["layer"].as_str().unwrap());
    let group = p(out["group"].as_str().unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &layer), "Shape Layer 1");
    assert_eq!(
        query::layer_kind(&fx.stage, &layer).as_deref(),
        Some("shape")
    );
    assert_eq!(fx.names(&layer.child("contents").unwrap()), ["Rectangle 1"]);
    assert_eq!(
        fx.names(&fx.contents(&group)),
        ["Rectangle Path 1", "Stroke 1", "Fill 1"]
    );
    let rect = fx.contents(&group).child("Rectangle_Path_1").unwrap();
    assert_eq!(fx.value(&rect, "size"), Some(Value::Vec2([200.0, 100.0])));
    assert_eq!(
        fx.value(&rect, "position"),
        Some(Value::Vec2([0.0, 0.0])),
        "path at the group origin"
    );
    assert_eq!(
        fx.value(&group, "transform:position"),
        Some(Value::Vec2([100.0, -100.0]))
    );
    // Anchor at the shape center (AE preference on): the layer's anchor and
    // position follow the drawn center, the shape stays where it was drawn.
    assert_eq!(
        fx.value(&layer, "transform:anchor_point"),
        Some(Value::Vec3([100.0, -100.0, 0.0]))
    );
    assert_eq!(
        fx.value(&layer, "transform:position"),
        Some(Value::Vec3([1060.0, 440.0, 0.0]))
    );
    let c = fx.to_comp(&comp, &layer, 100.0, -100.0);
    assert!((c - DVec3::new(1060.0, 440.0, 0.0)).length() < 1e-9, "{c}");
    let f = fx.contents(&group).child("Fill_1").unwrap();
    assert_eq!(
        fx.value(&f, "color"),
        Some(Value::Color([1.0, 0.0, 0.0, 1.0]))
    );
    let s = fx.contents(&group).child("Stroke_1").unwrap();
    assert_eq!(fx.value(&s, "width"), Some(Value::Float(2.0)));
    // One undo step removes the layer and everything in it.
    fx.stage.undo(&fx.author).unwrap();
    assert!(!fx.stage.has_prim(&layer));
}

#[test]
fn anchor_stays_at_the_comp_center_by_default() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let out = fx.run(
        "shape.add",
        json!({"comp": comp, "kind": "ellipse", "position": [-300.0, 50.0],
               "size": [80.0, 80.0], "fill": fill()}),
    );
    let layer = p(out["layer"].as_str().unwrap());
    assert_eq!(
        fx.value(&layer, "transform:anchor_point"),
        Some(Value::Vec3([0.0; 3]))
    );
    assert_eq!(
        fx.value(&layer, "transform:position"),
        Some(Value::Vec3([960.0, 540.0, 0.0]))
    );
    let c = fx.to_comp(&comp, &layer, -300.0, 50.0);
    assert!((c - DVec3::new(660.0, 590.0, 0.0)).length() < 1e-9, "{c}");
}

#[test]
fn shapes_added_to_a_layer_are_numbered_and_go_on_top() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = p(fx.run(
        "shape.add",
        json!({"comp": comp, "kind": "rect", "size": [10.0, 10.0], "fill": fill()}),
    )["layer"]
        .as_str()
        .unwrap());
    for (kind, extra) in [
        ("rect", json!({"size": [20.0, 20.0], "roundness": 4.0})),
        ("ellipse", json!({"size": [30.0, 20.0]})),
        ("polygon", json!({"outer_radius": 50.0, "points": 6.0})),
        ("star", json!({"outer_radius": 40.0, "rotation": 15.0})),
    ] {
        let mut params = json!({"layer": layer, "kind": kind, "fill": fill()});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        fx.run("shape.add", params);
    }
    assert_eq!(
        fx.names(&layer.child("contents").unwrap()),
        [
            "Polystar 2",
            "Polystar 1",
            "Ellipse 1",
            "Rectangle 2",
            "Rectangle 1"
        ]
    );
    let contents = layer.child("contents").unwrap();
    let rounded = contents
        .child("Rectangle_2")
        .unwrap()
        .child("contents")
        .unwrap()
        .child("Rectangle_Path_1")
        .unwrap();
    assert_eq!(fx.value(&rounded, "roundness"), Some(Value::Float(4.0)));
    let polygon = contents
        .child("Polystar_1")
        .unwrap()
        .child("contents")
        .unwrap()
        .child("Polystar_Path_1")
        .unwrap();
    assert_eq!(
        fx.value(&polygon, "star_type"),
        Some(Value::Enum("polygon".into()))
    );
    assert_eq!(fx.value(&polygon, "points"), Some(Value::Float(6.0)));
    let star = contents
        .child("Polystar_2")
        .unwrap()
        .child("contents")
        .unwrap()
        .child("Polystar_Path_1")
        .unwrap();
    assert_eq!(fx.value(&star, "inner_radius"), Some(Value::Float(20.0)));
    assert_eq!(fx.value(&star, "rotation"), Some(Value::Float(15.0)));
    // Each add is one undo step.
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(fx.names(&contents).len(), 4);
}

#[test]
fn translated_names_no_fill_and_bezier_paths() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let out = fx.run(
        "shape.add",
        json!({"comp": comp, "name": "形状图层 1", "kind": "ellipse", "size": [100.0, 50.0],
               "bezier": true, "stroke": stroke(),
               "names": {"group": "椭圆", "path": "路径", "fill": "填充", "stroke": "描边"}}),
    );
    let layer = p(out["layer"].as_str().unwrap());
    let group = p(out["group"].as_str().unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &layer), "形状图层 1");
    assert_eq!(query::display_name_of(&fx.stage, &group), "椭圆 1");
    assert_eq!(fx.names(&fx.contents(&group)), ["路径 1", "描边 1"]);
    let path = fx.stage.children(&fx.contents(&group))[0].clone();
    let Some(Value::Bezier(b)) = fx.value(&path, "path") else {
        panic!("a Bezier path");
    };
    assert_eq!(b.vertices.len(), 4);
    assert!(b.closed);
    // AE's ellipse starts at the top vertex.
    assert_eq!(b.vertices[0], [0.0, -25.0]);
}

#[test]
fn pen_paths_keep_layer_coordinates() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let path = BezierPath {
        vertices: vec![[-100.0, 0.0], [0.0, -80.0], [120.0, 10.0]],
        in_tangents: vec![[0.0, 0.0], [-30.0, 0.0], [0.0, 0.0]],
        out_tangents: vec![[0.0, 0.0], [30.0, 0.0], [0.0, 0.0]],
        closed: false,
        feather: Vec::new(),
    };
    let out = fx.run(
        "shape.add",
        json!({"comp": comp, "kind": "path", "path": path, "stroke": stroke()}),
    );
    let layer = p(out["layer"].as_str().unwrap());
    let group = p(out["group"].as_str().unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &group), "Shape 1");
    assert_eq!(fx.names(&fx.contents(&group)), ["Path 1", "Stroke 1"]);
    // Pen shapes: layer anchor stays at the origin, group transform identity.
    assert_eq!(
        fx.value(&layer, "transform:anchor_point"),
        Some(Value::Vec3([0.0; 3]))
    );
    assert_eq!(
        fx.value(&group, "transform:position"),
        Some(Value::Vec2([0.0; 2]))
    );
    let c = fx.to_comp(&comp, &layer, -100.0, 0.0);
    assert!((c - DVec3::new(860.0, 540.0, 0.0)).length() < 1e-9);
    // Bad paths are refused before anything is created.
    let bad = json!({"comp": comp, "kind": "path",
                     "path": {"vertices": [[0.0, 0.0]], "in_tangents": [], "out_tangents": [], "closed": false}});
    assert!(fx.try_run("shape.add", bad).is_err());
    assert_eq!(query::comp_layers(&fx.stage, &comp).len(), 1);
}

#[test]
fn shape_add_refuses_other_layers_and_locked_layers() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = p(
        fx.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]
            .as_str()
            .unwrap(),
    );
    let err = fx
        .try_run("shape.add", json!({"layer": solid, "kind": "rect"}))
        .unwrap_err();
    assert!(err.contains("not a shape layer"), "{err}");
    let shape = p(
        fx.run("layer.create", json!({"comp": comp, "kind": "shape"}))["path"]
            .as_str()
            .unwrap(),
    );
    fx.run(
        "attr.set",
        json!({"path": shape, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    assert!(
        fx.try_run("shape.add", json!({"layer": shape, "kind": "rect"}))
            .is_err()
    );
    assert!(fx.try_run("shape.add", json!({"kind": "rect"})).is_err());
}

#[test]
fn masks_are_named_colored_and_in_layer_space() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = p(
        fx.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]
            .as_str()
            .unwrap(),
    );
    let m1 = p(fx.run(
        "mask.create",
        json!({"layer": solid, "kind": "rect", "position": [500.0, 300.0], "size": [200.0, 100.0]}),
    )["mask"]
        .as_str()
        .unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &m1), "Mask 1");
    assert_eq!(fx.value(&m1, "mode"), Some(Value::Enum("add".into())));
    let Some(Value::Bezier(b)) = fx.value(&m1, "path") else {
        panic!("mask path");
    };
    // Rectangle in layer space, AE's vertex order (top right first, clockwise).
    assert_eq!(
        b.vertices,
        [
            [600.0, 250.0],
            [600.0, 350.0],
            [400.0, 350.0],
            [400.0, 250.0]
        ]
    );
    assert!(b.closed);
    let m2 = p(fx.run(
        "mask.create",
        json!({"layer": solid, "kind": "ellipse", "position": [100.0, 100.0], "size": [50.0, 50.0],
               "name": "蒙版 2"}),
    )["mask"]
        .as_str()
        .unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &m2), "蒙版 2");
    // Mask colors cycle through the label colors in AE 26's order: Sea Foam
    // (#B3C7B3), Orange (#E8920D) …
    assert_eq!(
        fx.value(&m1, "color"),
        Some(Value::Color(bae_motion::shape::mask_color(0, true)))
    );
    let [r, g, b, _] = bae_motion::shape::mask_color(0, true);
    assert_eq!(
        [
            (r * 255.0).round(),
            (g * 255.0).round(),
            (b * 255.0).round()
        ],
        [179.0, 199.0, 179.0]
    );
    let [r, g, b, _] = bae_motion::shape::mask_color(1, true);
    assert_eq!(
        [
            (r * 255.0).round(),
            (g * 255.0).round(),
            (b * 255.0).round()
        ],
        [232.0, 146.0, 13.0]
    );
    assert_ne!(fx.value(&m1, "color"), fx.value(&m2, "color"));
    let m3 = p(fx.run(
        "mask.create",
        json!({"layer": solid, "kind": "star", "position": [0.0, 0.0], "outer_radius": 30.0,
               "cycle_colors": false}),
    )["mask"]
        .as_str()
        .unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &m3), "Mask 3");
    assert_eq!(fx.value(&m3, "color"), fx.value(&m1, "color"), "no cycling");
    // Undo removes one mask per step.
    fx.stage.undo(&fx.author).unwrap();
    assert!(!fx.stage.has_prim(&m3));
    assert!(fx.stage.has_prim(&m2));
    // Masks show up in the evaluated layer (bae-anim's CompiledMask).
    let e = bae_anim::evaluate_comp(&fx.stage, &comp, Time::ZERO).unwrap();
    assert_eq!(e.layer(&solid).unwrap().masks.len(), 2);
}

#[test]
fn masks_only_on_layers_that_take_them() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    for kind in ["null", "camera", "light"] {
        let l = p(
            fx.run("layer.create", json!({"comp": comp, "kind": kind}))["path"]
                .as_str()
                .unwrap(),
        );
        assert!(
            fx.try_run(
                "mask.create",
                json!({"layer": l, "kind": "rect", "size": [10.0, 10.0]})
            )
            .is_err(),
            "{kind}"
        );
    }
    let shape = p(
        fx.run("layer.create", json!({"comp": comp, "kind": "shape"}))["path"]
            .as_str()
            .unwrap(),
    );
    fx.run(
        "mask.create",
        json!({"layer": shape, "kind": "rect", "size": [10.0, 10.0]}),
    );
}

/// AE's shape Add menu (`shape.add_item`): at the end of the layer's
/// contents, inside a selected group, or right after a selected item; one
/// undo step.
#[test]
fn add_menu_items_go_after_the_selection() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let out = fx.run(
        "shape.add",
        json!({"comp": comp, "kind": "rect", "position": [0.0, 0.0],
               "size": [200.0, 100.0], "fill": fill(), "stroke": stroke()}),
    );
    let layer = p(out["layer"].as_str().unwrap());
    let group = p(out["group"].as_str().unwrap());
    let contents = layer.child("contents").unwrap();
    // The layer: Trim Paths 1 after the group.
    let out = fx.run(
        "shape.add_item",
        json!({"targets": [layer], "item": "trim"}),
    );
    let trim = p(out["items"][0].as_str().unwrap());
    assert_eq!(fx.names(&contents), ["Rectangle 1", "Trim Paths 1"]);
    assert_eq!(
        fx.value(&trim, "operator"),
        Some(Value::Enum("trim".into()))
    );
    // The group: a Repeater at the end of its contents.
    fx.run(
        "shape.add_item",
        json!({"targets": [group], "item": "repeater"}),
    );
    assert_eq!(
        fx.names(&fx.contents(&group)),
        ["Rectangle Path 1", "Stroke 1", "Fill 1", "Repeater 1"]
    );
    // An item: a second Fill right after the path, with the given color.
    let path = fx.stage.children(&fx.contents(&group))[0].clone();
    let out = fx.run(
        "shape.add_item",
        json!({"targets": [path], "item": "fill", "color": [0.0, 0.0, 1.0, 1.0]}),
    );
    let fill2 = p(out["items"][0].as_str().unwrap());
    assert_eq!(
        fx.names(&fx.contents(&group)),
        [
            "Rectangle Path 1",
            "Fill 2",
            "Stroke 1",
            "Fill 1",
            "Repeater 1"
        ]
    );
    assert_eq!(
        fx.value(&fill2, "color"),
        Some(Value::Color([0.0, 0.0, 1.0, 1.0]))
    );
    // One undo step removes the last item.
    fx.stage.undo(&fx.author).unwrap();
    assert!(fx.stage.prim(&fill2).is_none());
    assert!(
        fx.try_run(
            "shape.add_item",
            json!({"targets": [layer], "item": "spline"})
        )
        .is_err()
    );
}
