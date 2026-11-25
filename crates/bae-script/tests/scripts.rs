//! AE-style scripts against a stage, through the same commands the app runs.

use std::sync::Arc;

use bae_app::{Owner, Registry};
use bae_model::{AuthorId, CommandRegistry, EffectDef, PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_script::{RUN_SCRIPT, RunScript, ScriptEnv, ScriptReport};
use serde_json::{Value as Json, json};

const SEC: i64 = Time::FLICKS_PER_SECOND;

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    defs: Arc<Registry<EffectDef>>,
    author: AuthorId,
}

/// Numbers as f64 (JavaScript prints `25.0` as `25`).
fn norm(j: Json) -> Json {
    match j {
        Json::Number(n) => json!(n.as_f64().unwrap()),
        Json::Array(a) => Json::Array(a.into_iter().map(norm).collect()),
        Json::Object(o) => Json::Object(o.into_iter().map(|(k, v)| (k, norm(v))).collect()),
        other => other,
    }
}

impl Fx {
    fn new() -> Self {
        let defs = Arc::new(Registry::<EffectDef>::new());
        let blur = EffectDef {
            match_name: "ADBE Gaussian Blur 2".into(),
            name: "Gaussian Blur".into(),
            category: "Blur & Sharpen".into(),
            plugin: None,
            params: vec![bae_model::EffectParamDef {
                id: 1,
                name: "Blurriness".into(),
                match_name: Some("ADBE Gaussian Blur 2-0001".into()),
                kind: bae_model::ParamKind::Slider {
                    valid_min: 0.0,
                    valid_max: 1000.0,
                    slider_min: 0.0,
                    slider_max: 50.0,
                    precision: 1,
                    integer: false,
                    display: "none".into(),
                },
                default: Some(Value::Float(0.0)),
                flags: Default::default(),
            }],
            thread_safe: true,
            gpu: true,
        };
        defs.register(Owner::Core, "ADBE%20Gaussian%20Blur%202", blur)
            .unwrap();
        let mut reg = bae_script::standard_commands(defs.clone());
        reg.register(RunScript::standalone(defs.clone())).unwrap();
        Fx {
            stage: Stage::new(),
            reg,
            defs,
            author: AuthorId::new("user:test").unwrap(),
        }
    }

    /// Runs a script through `script.run` (one undo step), like the menu does.
    fn script(&mut self, source: &str, extra: Json) -> ScriptReport {
        let mut params = json!({ "source": source, "name": "test.jsx" });
        if let Json::Object(m) = extra {
            params.as_object_mut().unwrap().extend(m);
        }
        let out = self
            .reg
            .execute(&mut self.stage, &self.author, RUN_SCRIPT, params)
            .unwrap_or_else(|e| panic!("script.run: {e}"))
            .output;
        serde_json::from_value(out).unwrap()
    }

    fn ok(&mut self, source: &str) -> Json {
        let r = self.script(source, json!({}));
        assert_eq!(r.error, None, "log: {:?}", r.log);
        norm(r.result)
    }

    fn comps(&self) -> Vec<PrimPath> {
        query::comps(&self.stage)
    }
}

#[test]
fn collection_loops_read_each_snapshot_once_and_refresh_after_edits() {
    let mut fx = Fx::new();
    fx.ok(r#"
        var f = app.project.items.addFolder("Examples");
        var c = f.items.addComp("Loop", 100, 100, 1, 4, 24);
        for (var i = 0; i < 8; i++) c.layers.addNull();
        var p = c.layer(1).position;
        for (var k = 0; k < 8; k++) p.setValueAtTime(k / 4, [k, k]);
        0
    "#);
    let out = fx.ok(r#"
        var calls = {}, host = __host;
        globalThis.__host = function(m, args) {
            calls[m] = (calls[m] || 0) + 1;
            return host(m, args);
        };
        var project = app.project, comp, folder;
        for (var i = 1; i <= project.numItems; i++) {
            var item = project.item(i);
            if (item.name === "Loop") comp = item;
            if (item.name === "Examples") folder = item;
        }
        var ids = [], roots = project.rootFolder.items;
        for (var i = 1; i <= roots.length; i++) ids.push(roots[i].id);
        var children = folder.items, layers = comp.layers, indices = 0;
        for (var i = 1; i <= layers.length; i++) indices += layers[i].index;
        var p = layers[1].position, total = 0;
        for (var k = 1; k <= p.numKeys; k++) total += p.keyTime(k) + p.keyValue(k)[0];
        p.keyValue(1)[0] = 99;
        var initial = [calls.items, calls.item, calls.layer || 0, calls.prop,
                       children.length, children[1].id === comp.id, indices, total, p.keyValue(1)[0]];
        var held = layers[1], added = layers.addText("New");
        var afterAdd = [layers.length, layers[1] instanceof TextLayer, held.index];
        added.remove();
        p.removeKey(1);
        folder.items.addFolder("Nested");
        [initial, afterAdd, layers.length, held.index, p.numKeys, p.keyValue(1)[0], children.length]
    "#);
    assert_eq!(
        out,
        norm(json!([
            [1, 1, 0, 1, 1, true, 36, 35, 0],
            [9, true, 2],
            8,
            1,
            7,
            1,
            2
        ]))
    );
}

#[test]
fn creates_comps_and_layers_like_ae() {
    let mut fx = Fx::new();
    let out = fx.ok(r#"
        #target aftereffects
        app.beginUndoGroup("Make lower third");
        var comp = app.project.items.addComp("Lower Third", 1280, 720, 1, 5, 25);
        var bg = comp.layers.addSolid([0.1, 0.2, 0.8], "Bar", 1280, 120, 1);
        var title = comp.layers.addText("Hello BAE");
        var ctrl = comp.layers.addNull();
        ctrl.name = "Controller";
        title.parent = ctrl;
        app.endUndoGroup();
        [comp.name, comp.width, comp.frameRate, comp.duration, comp.numLayers,
         comp.layer(1).name, comp.layer(3).name, title.parent.name,
         bg instanceof AVLayer, title instanceof TextLayer, comp instanceof CompItem]
    "#);
    assert_eq!(
        out,
        norm(json!([
            "Lower Third",
            1280,
            25.0,
            5.0,
            3,
            "Controller",
            "Bar",
            "Controller",
            true,
            true,
            true
        ]))
    );
    let comp = fx.comps().pop().unwrap();
    let layers = query::comp_layers(&fx.stage, &comp);
    assert_eq!(layers.len(), 3);
    // Text layer kind and source text.
    let title = &layers[1];
    assert_eq!(query::layer_kind(&fx.stage, title).as_deref(), Some("text"));
    // The whole script is one undo step, named by beginUndoGroup… through
    // script.run the step carries the command title.
    assert!(fx.stage.undo(&fx.author).is_some());
    assert!(fx.comps().is_empty(), "one undo removes everything");
    assert!(!fx.stage.can_undo(&fx.author));
}

#[test]
fn keyframes_and_values() {
    let mut fx = Fx::new();
    let out = fx.ok(r#"
        var comp = app.project.items.addComp("C", 1920, 1080, 1, 10, 30);
        var s = comp.layers.addSolid([1, 1, 1], "S", 100, 100, 1);
        var pos = s.property("Transform").property("Position");
        pos.setValue([100, 200]);
        var before = pos.value;
        s.transform.position.setValueAtTime(0, [0, 0]);
        s.position.setValueAtTime(1, [300, 400]);
        var o = s.property("Opacity");
        o.setValue(50);
        var mid = pos.valueAtTime(0.5);
        var k = pos.addKey(2);
        pos.removeKey(1);
        [before, pos.numKeys, pos.keyTime(1), pos.keyValue(1), mid, o.value, k,
         s.property("ADBE Transform Group").property("ADBE Opacity").value]
    "#);
    let a = out.as_array().unwrap();
    assert_eq!(a[0], json!([100.0, 200.0, 0.0]));
    assert_eq!(a[1], json!(2.0));
    assert_eq!(a[2], json!(1.0));
    assert_eq!(a[3], json!([300.0, 400.0, 0.0]));
    let mid = a[4].as_array().unwrap();
    assert!(mid[0].as_f64().unwrap() > 0.0 && mid[0].as_f64().unwrap() < 300.0);
    assert_eq!(a[5], json!(50.0));
    assert_eq!(a[6], json!(3.0), "the new key at 2 s is the third");
    assert_eq!(a[7], json!(50.0));

    let comp = fx.comps().pop().unwrap();
    let layer = query::comp_layers(&fx.stage, &comp).remove(0);
    let keys: Vec<i64> = query::attr_spec(&fx.stage, &layer, "transform:position")
        .unwrap()
        .keyframes
        .keys()
        .map(|t| t.0)
        .collect();
    assert_eq!(keys, vec![SEC, 2 * SEC]);
}

#[test]
fn errors_keep_earlier_changes_and_report_the_line() {
    let mut fx = Fx::new();
    let r = fx.script(
        "var c = app.project.items.addComp('Kept', 640, 360, 1, 2, 24);\n\
         c.layers.addNull();\n\
         undefinedThing.foo();\n\
         c.layers.addNull();",
        json!({}),
    );
    let err = r.error.expect("error");
    assert!(err.contains("line 3"), "{err}");
    let comp = fx.comps().pop().expect("the comp is kept");
    assert_eq!(query::comp_layers(&fx.stage, &comp).len(), 1);
    // setValue on keyframed properties fails like AE.
    let r = fx.script(
        "var l = app.project.item(app.project.numItems).layer(1);\n\
         l.opacity.setValueAtTime(0, 10);\n\
         l.opacity.setValue(20);",
        json!({}),
    );
    assert!(r.error.unwrap().contains("setValueAtTime"));
    // A script cannot run scripts.
    let r = fx.script(
        "bae.commands.invoke('script.run', {source: '1'})",
        json!({}),
    );
    assert!(r.error.unwrap().contains("cannot run another script"));
}

#[test]
fn active_comp_selection_alerts_and_commands() {
    let mut fx = Fx::new();
    fx.ok("var c = app.project.items.addComp('Main', 1920, 1080, 1, 10, 30); c.layers.addShape(); c.layers.addNull();");
    let comp = fx.comps().pop().unwrap();
    let layers = query::comp_layers(&fx.stage, &comp);
    let r = fx.script(
        r#"
        var comp = app.project.activeItem;
        if (!(comp instanceof CompItem)) alert("Select a composition");
        var sel = comp.selectedLayers;
        for (var i = 0; i < sel.length; i++) sel[i].name = "Picked " + (i + 1);
        $.writeln("renamed " + sel.length);
        alert("done");
        bae.commands.invoke("comp.set_settings", { comp: comp.id, width: 1280 });
        [sel.length, comp.width, comp.layers[2].name, comp.layers.length]
        "#,
        json!({ "active_comp": comp, "selected_layers": [layers[1]] }),
    );
    assert_eq!(r.error, None);
    assert_eq!(r.alerts, vec!["done".to_owned()]);
    assert_eq!(r.log, vec!["renamed 1".to_owned()]);
    assert_eq!(norm(r.result), norm(json!([1, 1280, "Picked 1", 2])));
}

#[test]
fn effects_text_and_expressions() {
    let mut fx = Fx::new();
    let out = fx.ok(r#"
        var c = app.project.items.addComp('FX', 640, 360, 1, 4, 24);
        var t = c.layers.addText("one");
        t.property("Source Text").setValue("two");
        var doc = t.text.sourceText.value;
        var blur = t.property("Effects").addProperty("Gaussian Blur");
        blur.property("Blurriness").setValue(12);
        var again = t.effect("Gaussian Blur")("Blurriness").value;
        t.transform.rotation.expression = "time * 90";
        [doc.text, doc instanceof TextDocument, blur.matchName, again,
         t.property("Effects").numProperties, t.rotation.expression, t.rotation.expressionEnabled]
    "#);
    assert_eq!(
        out,
        norm(json!([
            "two",
            true,
            "ADBE Gaussian Blur 2",
            12.0,
            1,
            "time * 90",
            true
        ]))
    );
    let _ = &fx.defs;
}

#[test]
fn run_api_labels_the_step_with_the_undo_group() {
    let mut fx = Fx::new();
    let reg = bae_script::standard_commands(fx.defs.clone());
    let commands = |id: &str| reg.get(id).cloned();
    let effects = |_: &str| None;
    let env = ScriptEnv {
        commands: &commands,
        effects: &effects,
        active_comp: None,
        selected_layers: Vec::new(),
        time: Time::ZERO,
        time_limit: None,
    };
    let (report, _) = bae_script::run(
        &mut fx.stage,
        &fx.author,
        &env,
        "named.jsx",
        "app.beginUndoGroup('Build Rig'); app.project.items.addFolder('Rig'); app.endUndoGroup(); 42",
    )
    .unwrap();
    assert_eq!(report.undo_group.as_deref(), Some("Build Rig"));
    assert_eq!(norm(report.result), json!(42.0));
    assert_eq!(fx.stage.undo_label(&fx.author), Some("Build Rig"));
}

#[test]
fn runaway_scripts_stop_at_the_time_limit() {
    let mut fx = Fx::new();
    let reg = bae_script::standard_commands(fx.defs.clone());
    let commands = |id: &str| reg.get(id).cloned();
    let effects = |_: &str| None;
    let env = ScriptEnv {
        commands: &commands,
        effects: &effects,
        active_comp: None,
        selected_layers: Vec::new(),
        time: Time::ZERO,
        time_limit: Some(std::time::Duration::from_millis(200)),
    };
    let (report, _) = bae_script::run(
        &mut fx.stage,
        &fx.author,
        &env,
        "loop.jsx",
        "app.project.items.addFolder('Before'); while (true) {}",
    )
    .unwrap();
    assert!(report.error.unwrap().contains("stopped after 0.2 seconds"));
    // Changes before the error are kept, as one undo step.
    assert_eq!(
        fx.stage.undo_label(&fx.author),
        Some("Run Script: loop.jsx")
    );
}

fn example(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/examples/scripts")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn example_scripts_run() {
    let mut fx = Fx::new();
    // Without an active comp the selection scripts only alert.
    let r = fx.script(&example("bounce-selected.jsx"), json!({}));
    assert_eq!(r.error, None);
    assert_eq!(r.alerts, vec!["Open a composition first.".to_owned()]);

    let r = fx.script(&example("lower-third.jsx"), json!({}));
    assert_eq!(r.error, None, "{:?}", r.log);
    assert_eq!(r.result, json!("Lower Third"));
    let comp = fx.comps().pop().unwrap();
    let layers = query::comp_layers(&fx.stage, &comp);
    assert_eq!(layers.len(), 3);
    let ctrl = &layers[0];
    let keys = query::attr_spec(&fx.stage, ctrl, "transform:position")
        .unwrap()
        .keyframes
        .clone();
    assert_eq!(keys.len(), 2);
    let last = keys.values().last().unwrap();
    assert_eq!(last.interp_in, bae_model::Interpolation::Bezier);
    assert_eq!(fx.stage.undo_label(&fx.author), Some("Run Script"));

    let ctx =
        json!({ "active_comp": comp, "selected_layers": [layers[1], layers[2]], "time": 1.0 });
    let r = fx.script(&example("bounce-selected.jsx"), ctx.clone());
    assert_eq!(r.error, None);
    let scale = query::attr_spec(&fx.stage, &layers[1], "transform:scale").unwrap();
    assert_eq!(scale.keyframes.len(), 4);
    assert_eq!(
        query::to_comp_time(
            &fx.stage,
            &layers[1],
            *scale.keyframes.keys().next().unwrap()
        ),
        Time(SEC)
    );

    let r = fx.script(&example("rename-layers.jsx"), ctx.clone());
    assert_eq!(r.error, None);
    assert_eq!(r.log, vec!["Renamed 2 layer(s)".to_owned()]);
    let names: Vec<String> = layers
        .iter()
        .map(|l| query::display_name_of(&fx.stage, l))
        .collect();
    assert_eq!(names, ["Controller", "Shot 01", "Shot 02"]);

    let r = fx.script(
        &example("align-to-grid.jsx"),
        json!({ "active_comp": comp }),
    );
    assert_eq!(r.error, None);
    assert_eq!(norm(r.result), json!(1.0), "the title at 180, 915 moves");
}

#[test]
fn layer_attributes_and_order() {
    let mut fx = Fx::new();
    let out = fx.ok(r#"
        var c = app.project.items.addComp('L', 640, 360, 1, 10, 30);
        var a = c.layers.addNull(4);
        a.name = "A";
        var b = c.layers.addShape();
        b.name = "B";
        a.inPoint = 1;
        a.outPoint = 3;
        b.startTime = 2;
        b.enabled = false;
        b.threeDLayer = true;
        var copy = a.duplicate();
        a.moveToBeginning();
        var order1 = [c.layer(1).name, c.layer(2).name, c.layer(3).name];
        a.moveAfter(b);
        var order2 = [c.layer(1).name, c.layer(2).name, c.layer(3).name];
        copy.remove();
        [a.inPoint, a.outPoint, b.startTime, b.inPoint, b.enabled, b.threeDLayer,
         order1, order2, c.numLayers, a.index]
    "#);
    assert_eq!(
        out,
        norm(json!([
            1,
            3,
            2,
            2,
            false,
            true,
            ["A", "B", "A 2"],
            ["B", "A", "A 2"],
            2,
            2
        ]))
    );
}
