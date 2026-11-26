//! `text.create` (point and paragraph text in one step) and `text.set_text`
//! (characters replaced, style kept, keyframed when animated).

use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage, TextDocument, Time, Value};
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

    fn comp(&mut self) -> PrimPath {
        p(&self.run("comp.create", json!({"width": 640, "height": 360}))["path"])
    }

    fn doc(&self, layer: &PrimPath) -> TextDocument {
        match query::static_value(&self.stage.prim(layer).unwrap(), "text:source") {
            Some(Value::TextDocument(d)) => *d,
            other => panic!("{other:?}"),
        }
    }

    fn v3(&self, layer: &PrimPath, attr: &str) -> Option<Value> {
        query::static_value(&self.stage.prim(layer).unwrap(), attr)
    }
}

#[test]
fn point_and_paragraph_text_in_one_step() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let out = fx.run(
        "text.create",
        json!({"comp": comp, "text": "Hello\nWorld", "position": [100.0, 200.0]}),
    );
    let layer = p(&out["path"]);
    // Named after its first line.
    assert_eq!(query::display_name_of(&fx.stage, &layer), "Hello");
    let d = fx.doc(&layer);
    assert_eq!(d.text, "Hello\nWorld");
    assert_eq!(d.style.font_size, bae_motion::text::DEFAULT_SIZE);
    assert_eq!(d.box_size, None);
    assert_eq!(
        fx.v3(&layer, "transform:position"),
        Some(Value::Vec3([100.0, 200.0, 0.0]))
    );
    assert_eq!(
        fx.v3(&layer, "transform:anchor_point"),
        Some(Value::Vec3([0.0, 0.0, 0.0]))
    );
    fx.stage.undo(&fx.author).unwrap();
    assert!(
        query::comp_layers(&fx.stage, &comp).is_empty(),
        "one undo step"
    );
    let out = fx.run(
        "text.create",
        json!({"comp": comp, "text": "Box", "position": [50.0, 60.0], "box_size": [200.0, -80.0],
               "font_size": 24.0, "name": "Title", "fill_color": [1.0, 0.0, 0.0, 1.0]}),
    );
    let layer = p(&out["path"]);
    assert_eq!(query::display_name_of(&fx.stage, &layer), "Title");
    let d = fx.doc(&layer);
    assert_eq!(d.box_size, Some([200.0, 80.0]));
    // AE 26: the layer sits at the box's center, the box offset by half.
    assert_eq!(d.box_position, Some([-100.0, -40.0]));
    assert_eq!(
        fx.v3(&layer, "transform:position"),
        Some(Value::Vec3([150.0, 100.0, 0.0]))
    );
    assert_eq!(d.style.font_size, 24.0);
    assert_eq!(d.style.fill_color, [1.0, 0.0, 0.0, 1.0]);
    // Empty text makes an empty text layer (AE keeps those).
    let out = fx.run(
        "text.create",
        json!({"comp": comp, "text": "", "position": [0.0, 0.0]}),
    );
    let empty = p(&out["path"]);
    assert_eq!(
        query::display_name_of(&fx.stage, &empty),
        "<empty text layer>"
    );
    assert_eq!(fx.doc(&empty).text, "");
}

#[test]
fn set_text_keeps_the_style_and_keys_animated_text() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = p(&fx.run(
        "text.create",
        json!({"comp": comp, "text": "One", "position": [0.0, 100.0], "font_size": 30.0}),
    )["path"]);
    let out = fx.run("text.set_text", json!({"layer": layer, "text": "Two"}));
    assert_eq!(out["changed"], true);
    let d = fx.doc(&layer);
    assert_eq!((d.text.as_str(), d.style.font_size), ("Two", 30.0));
    // Unchanged text is a no-op.
    assert_eq!(
        fx.run("text.set_text", json!({"layer": layer, "text": "Two"}))["changed"],
        false
    );
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(fx.doc(&layer).text, "One");
    // Animated Source Text: a keyframe at the time, the earlier one kept.
    fx.run(
        "attr.set_animated",
        json!({"path": layer, "attr": "text:source", "animated": true, "time": 0}),
    );
    let t = Time::from_seconds(1.0);
    fx.run(
        "text.set_text",
        json!({"layer": layer, "text": "Later", "time": t}),
    );
    let spec = query::attr_spec(&fx.stage, &layer, "text:source").unwrap();
    let texts: Vec<String> = spec
        .keyframes
        .values()
        .map(|k| match &k.value {
            Value::TextDocument(d) => d.text.clone(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(texts, ["One", "Later"]);
    // Only text layers.
    let solid = p(&fx.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    assert!(
        fx.try_run("text.set_text", json!({"layer": solid, "text": "x"}))
            .is_err()
    );
}

#[test]
fn text_follows_a_mask_path() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = p(&fx.run(
        "text.create",
        json!({"comp": comp, "text": "Hi", "position": [10.0, 10.0]}),
    )["path"]);
    let mask = fx.run(
        "mask.create",
        json!({"layer": layer, "kind": "ellipse", "position": [0.0, 0.0], "size": [100.0, 100.0]}),
    )["mask"]
        .clone();
    fx.run("text.set_path", json!({"layer": layer, "mask": mask}));
    assert_eq!(fx.v3(&layer, "text:path"), Some(Value::PrimRef(p(&mask))));
    fx.run("text.set_path", json!({"layer": layer, "mask": null}));
    assert_eq!(fx.v3(&layer, "text:path"), None);
    // Only the layer's own masks.
    let other = p(&fx.run(
        "text.create",
        json!({"comp": comp, "text": "x", "position": [0.0, 0.0]}),
    )["path"]);
    assert!(
        fx.try_run("text.set_path", json!({"layer": other, "mask": mask}))
            .is_err()
    );
}
