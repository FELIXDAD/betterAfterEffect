//! Default names in a Chinese UI (#152): new folders, comps, layers and shape
//! contents are named in Chinese when they are created (AE zh_CN); paths stay
//! English, and names already in the project never change with the language.
//! Its own test binary: the naming language is process-wide.

use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage};
use bae_motion::query;
use serde_json::{Value as Json, json};

fn run(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: Json) -> Json {
    reg.execute(stage, &AuthorId::new("user:test").unwrap(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .output
}

fn name(stage: &Stage, path: &str) -> String {
    query::display_name_of(stage, &PrimPath::new(path).unwrap())
}

#[test]
fn new_items_are_named_in_the_ui_language_and_keep_their_names() {
    let mut reg = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut reg);
    let mut stage = Stage::new();
    bae_motion::names::set_chinese(true);

    let comp = run(
        &reg,
        &mut stage,
        "comp.create",
        json!({"width": 640, "height": 360, "fps": 30}),
    );
    let comp = comp["path"].as_str().unwrap().to_owned();
    assert!(comp.starts_with("/comps/"), "{comp}");
    assert_eq!(name(&stage, &comp), "合成 1");
    assert_eq!(name(&stage, "/comps"), "合成");
    let shape = run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "shape", "shape": "rect"}),
    );
    let shape = shape["path"].as_str().unwrap().to_owned();
    assert_eq!(name(&stage, &shape), "形状图层 1");
    // AE zh_CN: 矩形 1 / 矩形路径 1 / 填充 1 (docs/reference/ae/layers/shape.json);
    // the paths keep their English segments.
    let group = format!("{shape}/contents/Rectangle_1");
    assert_eq!(name(&stage, &group), "矩形 1");
    assert_eq!(
        name(&stage, &format!("{group}/contents/Rectangle_Path_1")),
        "矩形路径 1"
    );
    assert_eq!(name(&stage, &format!("{group}/contents/Fill_1")), "填充 1");
    let ellipse = run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "shape", "shape": "ellipse"}),
    );
    let ellipse = ellipse["path"].as_str().unwrap().to_owned();
    assert_eq!(name(&stage, &ellipse), "形状图层 2");
    assert_eq!(
        name(
            &stage,
            &format!("{ellipse}/contents/Ellipse_1/contents/Ellipse_Path_1")
        ),
        "椭圆路径 1"
    );
    let solid = run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "solid"}),
    );
    assert_eq!(name(&stage, solid["path"].as_str().unwrap()), "纯色 1");
    assert_eq!(name(&stage, "/assets"), "素材");
    assert_eq!(name(&stage, "/assets/solids"), "纯色");
    let null = run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "null"}),
    );
    assert_eq!(name(&stage, null["path"].as_str().unwrap()), "空 1");
    let folder = run(&reg, &mut stage, "folder.create", json!({}));
    assert_eq!(name(&stage, folder["path"].as_str().unwrap()), "未命名 1");

    // Switching the language renames nothing; new items follow the new one.
    bae_motion::names::set_chinese(false);
    assert_eq!(name(&stage, "/comps"), "合成");
    assert_eq!(name(&stage, &group), "矩形 1");
    let comp2 = run(
        &reg,
        &mut stage,
        "comp.create",
        json!({"width": 640, "height": 360, "fps": 30}),
    );
    assert_eq!(name(&stage, comp2["path"].as_str().unwrap()), "Comp 1");
    let shape2 = run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "shape", "shape": "rect"}),
    );
    let shape2 = shape2["path"].as_str().unwrap().to_owned();
    assert_eq!(name(&stage, &shape2), "Shape Layer 1");
    assert_eq!(
        name(
            &stage,
            &format!("{shape2}/contents/Rectangle_1/contents/Fill_1")
        ),
        "Fill 1"
    );
}
