//! `path.set_vertices`, `path.insert_vertex` and `path.delete_vertex` on masks and
//! shape paths, static and animated: vertex counts stay equal in every keyframe.

use bae_model::{AuthorId, BezierPath, CommandRegistry, PrimPath, Stage, Time, Value};
use bae_motion::query;
use serde_json::{Value as Json, json};

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
}

fn p(v: &Json) -> PrimPath {
    PrimPath::new(v.as_str().unwrap()).unwrap()
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

    /// A solid with a 200×100 rectangle mask at (200, 150).
    fn mask(&mut self) -> PrimPath {
        let comp = p(&self.run("comp.create", json!({"width": 640, "height": 360}))["path"]);
        let solid = p(&self.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
        p(&self.run(
            "mask.create",
            json!({"layer": solid, "kind": "rect", "position": [200.0, 150.0], "size": [200.0, 100.0]}),
        )["mask"])
    }

    fn path(&self, prim: &PrimPath) -> BezierPath {
        match query::static_value(&self.stage.prim(prim).unwrap(), "path") {
            Some(Value::Bezier(b)) => b,
            other => panic!("{other:?}"),
        }
    }

    fn keys(&self, prim: &PrimPath) -> Vec<BezierPath> {
        query::attr_spec(&self.stage, prim, "path")
            .unwrap()
            .keyframes
            .values()
            .map(|k| match &k.value {
                Value::Bezier(b) => b.clone(),
                other => panic!("{other:?}"),
            })
            .collect()
    }
}

fn second() -> Time {
    Time::from_seconds(1.0)
}

#[test]
fn set_vertices_writes_static_values_and_keyframes() {
    let mut fx = Fx::new();
    let mask = fx.mask();
    let tri = json!({"path": mask, "vertices": [[0.0, 0.0], [10.0, 0.0], [5.0, 8.0]],
                     "in_tangents": [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
                     "out_tangents": [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]], "closed": true});
    fx.run("path.set_vertices", tri.clone());
    assert_eq!(fx.path(&mask).vertices.len(), 3);
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(fx.path(&mask).vertices.len(), 4);
    // Animated: a keyframe at the time.
    fx.run(
        "attr.set_animated",
        json!({"path": mask, "attr": "path", "animated": true, "time": 0}),
    );
    let mut at = tri;
    at["time"] = json!(second());
    fx.run("path.set_vertices", at);
    let keys = fx.keys(&mask);
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[1].vertices.len(), 3);
    // Bad input and non-path prims are refused.
    let bad = json!({"path": mask, "vertices": [[0.0, 0.0]], "in_tangents": [], "out_tangents": [], "closed": false});
    assert!(fx.try_run("path.set_vertices", bad).is_err());
    let layer = mask.parent().unwrap().parent().unwrap();
    let not_path = json!({"path": layer, "vertices": [[0.0, 0.0]], "in_tangents": [[0.0, 0.0]],
                          "out_tangents": [[0.0, 0.0]], "closed": false});
    assert!(fx.try_run("path.set_vertices", not_path).is_err());
}

#[test]
fn insert_and_delete_change_every_keyframe() {
    let mut fx = Fx::new();
    let mask = fx.mask();
    // Static: insert on the top edge (closing segment 3 → 0), then delete it.
    let out = fx.run(
        "path.insert_vertex",
        json!({"path": mask, "segment": 3, "t": 0.5}),
    );
    assert_eq!(out["index"], 4);
    let b = fx.path(&mask);
    assert_eq!(b.vertices.len(), 5);
    assert_eq!(b.vertices[4], [200.0, 100.0]);
    fx.run("path.delete_vertex", json!({"path": mask, "index": 4}));
    assert_eq!(fx.path(&mask).vertices.len(), 4);
    // Animated: both keyframes get the vertex.
    fx.run(
        "attr.set_animated",
        json!({"path": mask, "attr": "path", "animated": true, "time": 0}),
    );
    let mut moved = fx.path(&mask);
    for v in &mut moved.vertices {
        v[0] += 50.0;
    }
    let mut params = serde_json::to_value(&moved).unwrap();
    params["path"] = json!(mask);
    params["time"] = json!(second());
    fx.run("path.set_vertices", params);
    fx.run(
        "path.insert_vertex",
        json!({"path": mask, "segment": 0, "t": 0.25}),
    );
    let keys = fx.keys(&mask);
    assert!(keys.iter().all(|k| k.vertices.len() == 5), "{keys:?}");
    // A straight segment with no handles: t = 0.25 is at 3t² − 2t³ of the way.
    assert_eq!(keys[0].vertices[1], [300.0, 115.625]);
    assert_eq!(keys[1].vertices[1], [350.0, 115.625]);
    fx.run("path.delete_vertex", json!({"path": mask, "index": 0}));
    assert!(fx.keys(&mask).iter().all(|k| k.vertices.len() == 4));
    // One undo step each.
    fx.stage.undo(&fx.author).unwrap();
    assert!(fx.keys(&mask).iter().all(|k| k.vertices.len() == 5));
    // Out of range, and the last vertex, are refused.
    assert!(
        fx.try_run("path.insert_vertex", json!({"path": mask, "segment": 9}))
            .is_err()
    );
    assert!(
        fx.try_run("path.delete_vertex", json!({"path": mask, "index": 9}))
            .is_err()
    );
}

#[test]
fn shape_paths_are_edited_too() {
    let mut fx = Fx::new();
    let comp = p(&fx.run("comp.create", json!({"width": 640, "height": 360}))["path"]);
    let path = json!({"vertices": [[0.0, 0.0], [100.0, 0.0]], "in_tangents": [[0.0, 0.0], [0.0, 0.0]],
                      "out_tangents": [[0.0, 0.0], [0.0, 0.0]], "closed": false});
    let out = fx.run(
        "shape.add",
        json!({"comp": comp, "kind": "path", "path": path}),
    );
    let group = p(&out["group"]);
    let contents = group.child("contents").unwrap();
    let prim = fx.stage.children(&contents)[0].clone();
    fx.run("path.insert_vertex", json!({"path": prim, "segment": 0}));
    assert_eq!(
        fx.path(&prim).vertices,
        [[0.0, 0.0], [50.0, 0.0], [100.0, 0.0]]
    );
    // An open path has no closing segment.
    assert!(
        fx.try_run("path.insert_vertex", json!({"path": prim, "segment": 2}))
            .is_err()
    );
}
