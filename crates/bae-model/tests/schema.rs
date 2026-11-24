use bae_model::{
    AuthorId, CommandError, CommandRegistry, EffectDef, EffectRegistry, EffectStatus, ModelError,
    PrimPath, Project, Stage, TextDocument, Time, Value,
};
use serde_json::{Value as Json, json};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn exec(
    reg: &CommandRegistry,
    stage: &mut Stage,
    id: &str,
    params: Json,
) -> Result<(), CommandError> {
    reg.execute(stage, &AuthorId::new("agent:test").unwrap(), id, params)
        .map(|_| ())
}

fn base(reg: &CommandRegistry) -> Stage {
    let mut s = Stage::new();
    for (path, t) in [
        ("/comps", "Folder"),
        ("/comps/main", "Comp"),
        ("/comps/main/layers", "Folder"),
        ("/comps/main/layers/title", "Layer"),
        ("/comps/main/layers/title/glow", "Effect"),
    ] {
        exec(
            reg,
            &mut s,
            "prim.create",
            json!({"path": path, "prim_type": t}),
        )
        .unwrap();
    }
    s
}

#[test]
fn commands_enforce_the_schema() {
    let reg = CommandRegistry::with_builtins();
    let mut s = base(&reg);
    let title = "/comps/main/layers/title";
    let set = |s: &mut Stage, attr: &str, v: Json| {
        exec(
            &reg,
            s,
            "attr.set",
            json!({"path": title, "attr": attr, "value": v}),
        )
    };

    set(
        &mut s,
        "transform:opacity",
        json!({"type": "float", "value": 50.0}),
    )
    .unwrap();
    set(
        &mut s,
        "transform:opacity",
        json!({"type": "int", "value": 50}),
    )
    .unwrap();
    let err = set(
        &mut s,
        "transform:opacity",
        json!({"type": "string", "value": "50"}),
    )
    .unwrap_err();
    assert!(
        matches!(err, CommandError::Model(ModelError::TypeMismatch { .. })),
        "{err}"
    );
    set(
        &mut s,
        "blend_mode",
        json!({"type": "enum", "value": "multiply"}),
    )
    .unwrap();
    let err = set(
        &mut s,
        "blend_mode",
        json!({"type": "enum", "value": "mutliply"}),
    )
    .unwrap_err();
    assert!(err.to_string().contains("one of normal"), "{err}");
    // Custom and AE-only attributes are free-form.
    set(
        &mut s,
        "ae:ADBE_Layer_Overrides",
        json!({"type": "int", "value": 3}),
    )
    .unwrap();

    let err = exec(
        &reg,
        &mut s,
        "key.set",
        json!({"path": title, "attr": "blend_mode", "time": 0, "keyframe": {"value": {"type": "enum", "value": "add"}}}),
    )
    .unwrap_err();
    assert!(
        matches!(err, CommandError::Model(ModelError::NotAnimatable { .. })),
        "{err}"
    );
    let err = exec(
        &reg,
        &mut s,
        "attr.set_expression",
        json!({"path": title, "attr": "three_d", "source": "true"}),
    )
    .unwrap_err();
    assert!(
        matches!(err, CommandError::Model(ModelError::NotAnimatable { .. })),
        "{err}"
    );

    let schema = bae_model::schema::to_json();
    let layer = schema["prim_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["prim_type"] == "Layer")
        .unwrap();
    assert_eq!(layer["attrs"][0]["name"], "transform:anchor_point");
}

#[test]
fn missing_plugin_effects_keep_all_data_and_show_as_placeholders() {
    let reg = CommandRegistry::with_builtins();
    let mut s = base(&reg);
    let fx = "/comps/main/layers/title/glow";
    for (attr, v) in [
        (
            "match_name",
            json!({"type": "string", "value": "com.example.superglow"}),
        ),
        (
            "plugin:id",
            json!({"type": "string", "value": "com.example"}),
        ),
        (
            "plugin:version",
            json!({"type": "string", "value": "2.1.0"}),
        ),
        ("param:0001", json!({"type": "float", "value": 42.5})),
        (
            "param:0007",
            json!({"type": "color", "value": [1.0, 0.5, 0.0, 1.0]}),
        ),
        ("state", json!({"type": "bytes", "value": "AAECAwQ="})),
    ] {
        exec(
            &reg,
            &mut s,
            "attr.set",
            json!({"path": fx, "attr": attr, "value": v}),
        )
        .unwrap();
    }
    exec(
        &reg,
        &mut s,
        "key.set",
        json!({"path": fx, "attr": "param:0001", "time": 705600000, "keyframe": {"value": {"type": "float", "value": 80.0}}}),
    )
    .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    Project::from_stage(s).save_as(&dir).unwrap();
    let opened = Project::open(&dir).unwrap();
    let prim = opened.stage.prim(&p(fx)).unwrap();

    let mut effects = EffectRegistry::new();
    match effects.status(&prim).unwrap() {
        EffectStatus::MissingPlugin { match_name, plugin } => {
            assert_eq!(match_name, "com.example.superglow");
            let plugin = plugin.unwrap();
            assert_eq!(
                (plugin.id.as_str(), plugin.version.as_str()),
                ("com.example", "2.1.0")
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(prim.value("param:0001"), Some(&Value::Float(42.5)));
    assert_eq!(prim.attr("param:0001").unwrap().keyframes.len(), 1);
    assert_eq!(
        prim.value("state"),
        Some(&Value::Bytes(vec![0, 1, 2, 3, 4]))
    );

    // The plugin comes back: the same data is a known effect again.
    effects.register(EffectDef {
        match_name: "com.example.superglow".into(),
        name: "Super Glow".into(),
        category: "Stylize".into(),
        plugin: None,
        params: Vec::new(),
        thread_safe: true,
        gpu: true,
    });
    assert!(matches!(
        effects.status(&prim),
        Some(EffectStatus::Known(_))
    ));
}

#[test]
fn text_documents_animate_and_round_trip() {
    let reg = CommandRegistry::with_builtins();
    let mut s = base(&reg);
    let title = "/comps/main/layers/title";
    exec(
        &reg,
        &mut s,
        "attr.set",
        json!({"path": title, "attr": "kind", "value": {"type": "enum", "value": "text"}}),
    )
    .unwrap();
    let mut doc = TextDocument::new("你好 BAE", "PingFangSC-Regular", 96.0);
    doc.justification = "center".into();
    let mut later = doc.clone();
    later.text = "Hello".into();
    for (t, d) in [(0i64, &doc), (705_600_000, &later)] {
        exec(
            &reg,
            &mut s,
            "key.set",
            json!({"path": title, "attr": "text:source", "time": t,
                   "keyframe": {"value": {"type": "text_document", "value": d}, "interp_out": "hold"}}),
        )
        .unwrap();
    }
    exec(
        &reg,
        &mut s,
        "attr.set",
        json!({"path": title, "attr": "in_point", "value": {"type": "time", "value": 352800000}}),
    )
    .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("Text.baez");
    Project::from_stage(s).save_as(&file).unwrap();
    let opened = Project::open(&file).unwrap();
    let prim = opened.stage.prim(&p(title)).unwrap();
    let keys = &prim.attr("text:source").unwrap().keyframes;
    assert_eq!(keys[&Time(0)].value, Value::TextDocument(Box::new(doc)));
    assert_eq!(
        keys[&Time(705_600_000)].value,
        Value::TextDocument(Box::new(later))
    );
    assert_eq!(
        prim.value("in_point").and_then(Value::as_time),
        Some(Time(352_800_000))
    );
}

#[test]
fn creating_a_text_animator_is_one_undoable_command() {
    let reg = CommandRegistry::with_builtins();
    let mut s = base(&reg);
    let layer = "/comps/main/layers/title";
    let params = json!({"layer": layer, "name": "rise"});
    assert!(exec(&reg, &mut s, "text.create_animator", params.clone()).is_err());
    exec(
        &reg,
        &mut s,
        "attr.set",
        json!({"path": layer, "attr":"kind", "value":Value::Enum("text".into())}),
    )
    .unwrap();
    exec(&reg, &mut s, "text.create_animator", params.clone()).unwrap();
    let folder = p(layer).child("animators").unwrap();
    let group = folder.child("rise").unwrap();
    let selector = group.child("range").unwrap();
    assert!(
        s.require_prim(&selector)
            .unwrap()
            .is_a(&bae_model::PrimType::TEXT_SELECTOR)
    );
    assert!(exec(&reg, &mut s, "text.create_animator", params).is_err());
    let author = AuthorId::new("agent:test").unwrap();
    s.undo(&author).unwrap();
    assert!(!s.has_prim(&folder));
    s.redo(&author).unwrap();
    assert!(s.has_prim(&selector));
    let bad = json!({"path": group,"attr": "animator:position","value":Value::Float(10.0)});
    assert!(exec(&reg, &mut s, "attr.set", bad).is_err());
}
