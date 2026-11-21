//! A small demo project built with motion commands: a comp with a background solid,
//! an animated star (position, scale, rotation keyframes with Easy Ease), a rounded
//! rectangle with a rotation expression and a text layer. Used by the headless smoke
//! test screenshots (`BAE_SMOKE_DEMO=1`) and by tests.

use bae_app::Resources;
use bae_model::{PrimPath, Time};
use bae_motion::KeyRef;
use bae_motion::query;
use bae_ui::Invocation;
use serde_json::json;

use crate::session::{Reveal, SharedSession};

const COMP: &str = "/comps/Main";
const STAR: &str = "/comps/Main/layers/Star";

/// The commands that build the demo project, in order.
pub fn steps() -> Vec<Invocation> {
    let sec = Time::FLICKS_PER_SECOND;
    let key = |attr: &str, t: i64, v: serde_json::Value| {
        Invocation::with_params(
            "attr.set_at",
            json!({"path": STAR, "attr": attr, "time": t, "value": v}),
        )
    };
    let vec3 = |x: f64, y: f64, z: f64| json!({"type": "vec3", "value": [x, y, z]});
    let float = |v: f64| json!({"type": "float", "value": v});
    let layers = format!("{COMP}/layers");
    let inv = Invocation::with_params;
    vec![
        inv(
            "comp.create",
            json!({"name": "Main", "width": 1920, "height": 1080, "fps": 30, "duration": 5 * sec}),
        ),
        inv(
            "layer.create",
            json!({"comp": COMP, "kind": "solid", "name": "Background", "color": [0.08, 0.1, 0.18, 1.0]}),
        ),
        inv(
            "layer.create",
            json!({"comp": COMP, "kind": "shape", "name": "Panel", "shape": "rect", "color": [0.2, 0.55, 0.95, 1.0], "position": [1420.0, 560.0]}),
        ),
        inv(
            "attr.set",
            json!({"path": format!("{layers}/Panel/contents/Rectangle_1/contents/Rectangle_Path_1"), "attr": "roundness", "value": float(40.0)}),
        ),
        inv(
            "attr.set_expression",
            json!({"path": format!("{layers}/Panel"), "attr": "transform:rotation", "source": "time * 20"}),
        ),
        inv(
            "layer.create",
            json!({"comp": COMP, "kind": "shape", "name": "Star", "shape": "star", "color": [1.0, 0.78, 0.2, 1.0], "position": [420.0, 540.0]}),
        ),
        inv(
            "attr.set_animated",
            json!({"path": STAR, "attr": "transform:position", "animated": true, "time": 0}),
        ),
        key("transform:position", 2 * sec, vec3(960.0, 420.0, 0.0)),
        key("transform:position", 4 * sec, vec3(1500.0, 700.0, 0.0)),
        inv(
            "attr.set_animated",
            json!({"path": STAR, "attr": "transform:scale", "animated": true, "time": 0}),
        ),
        key("transform:scale", 2 * sec, vec3(160.0, 160.0, 100.0)),
        inv(
            "attr.set_animated",
            json!({"path": STAR, "attr": "transform:rotation", "animated": true, "time": 0}),
        ),
        key("transform:rotation", 4 * sec, float(360.0)),
        inv(
            "key.ease",
            json!({"keys": [
                KeyRef::new(PrimPath::new(STAR).expect("valid"), "transform:position", Time(2 * sec)),
                KeyRef::new(PrimPath::new(STAR).expect("valid"), "transform:scale", Time(2 * sec)),
            ]}),
        ),
        inv(
            "layer.create",
            json!({"comp": COMP, "kind": "text", "name": "Title", "text": "Better After Effects", "font_size": 96.0, "position": [520.0, 220.0]}),
        ),
        inv(
            "layer.trim",
            json!({"layers": [format!("{layers}/Title")], "in_point": sec / 2, "out_point": 9 * sec / 2}),
        ),
        inv(
            "comp.set_work_area",
            json!({"comp": COMP, "start": 0, "end": 4 * sec}),
        ),
    ]
}

/// Open the demo comp at 1.5 s with the star selected and its animated properties
/// revealed (as after pressing U).
pub fn open(resources: &Resources, stage: &bae_model::Stage) {
    let Some(session) = resources.get::<SharedSession>() else {
        return;
    };
    let comp = PrimPath::new(COMP).expect("valid");
    let star = PrimPath::new(STAR).expect("valid");
    let Some(info) = query::comp_info(stage, &comp) else {
        return;
    };
    let mut s = session.lock();
    s.open_comp(&comp);
    s.set_time(&info, Time::from_seconds(1.5));
    s.selected_layers = vec![star.clone()];
    s.reveal.insert(star, Reveal::Animated);
}
