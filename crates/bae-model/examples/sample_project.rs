//! Write a small sample project: `cargo run -p bae-model --example sample_project -- Out.bae`
//! (use a `.baez` extension for the single-file container).

use bae_model::{AuthorId, CommandRegistry, Project, Stage};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Sample.bae".into());
    let reg = CommandRegistry::with_builtins();
    let who = AuthorId::new("user:sample")?;
    let mut stage = Stage::new();
    let run = |stage: &mut Stage, id: &str, params: serde_json::Value| {
        reg.execute(stage, &who, id, params).map(|_| ())
    };
    for (path, t) in [
        ("/comps", "Folder"),
        ("/comps/main", "Comp"),
        ("/comps/main/title", "Layer"),
        ("/comps/main/bg", "Layer"),
    ] {
        run(
            &mut stage,
            "prim.create",
            json!({"path": path, "prim_type": t}),
        )?;
    }
    run(
        &mut stage,
        "attr.set",
        json!({"path": "/comps/main", "attr": "width", "value": {"type": "int", "value": 1920}}),
    )?;
    run(
        &mut stage,
        "attr.set",
        json!({"path": "/comps/main", "attr": "height", "value": {"type": "int", "value": 1080}}),
    )?;
    run(&mut stage, "sublayer.create", json!({"id": "anim/main"}))?;
    run(
        &mut stage,
        "sublayer.set_edit_target",
        json!({"id": "anim/main"}),
    )?;
    for (t, v) in [(0, 0.0), (705_600_000, 100.0)] {
        run(
            &mut stage,
            "key.set",
            json!({"path": "/comps/main/title", "attr": "opacity", "time": t,
                   "keyframe": {"value": {"type": "float", "value": v}, "interp_in": "bezier", "interp_out": "bezier"}}),
        )?;
    }
    Project::from_stage(stage).save_as(&out)?;
    println!("wrote {out}");
    Ok(())
}
