//! Keyframe assistant panels of the Window menu.

use bae_model::{PrimPath, Time, Value};
use bae_motion::KeyRef;

pub(crate) mod mask_interpolation;
pub(crate) mod smoother;
pub(crate) mod wiggler;

/// Chinese strings of the keyframe assistants (AE 26's terms).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Wiggler", "摇摆器"),
    ("Smoother", "平滑器"),
    ("Apply To:", "应用到:"),
    ("Temporal Graph", "时间图表"),
    ("Spatial Path", "空间路径"),
    ("Noise Type:", "杂色类型:"),
    ("Jagged", "成锯齿状"),
    ("One Dimension", "一维"),
    ("All Dimensions Independently", "所有尺寸独立"),
    ("All Dimensions the Same", "所有尺寸相同"),
    ("Frequency:", "频率:"),
    ("Magnitude:", "数量级:"),
    ("Tolerance:", "容差:"),
    (
        "Select at least two keyframes of a property.",
        "请选择同一属性的至少两个关键帧。",
    ),
    (
        "Select at least three keyframes of a property.",
        "请选择同一属性的至少三个关键帧。",
    ),
    ("Mask Interpolation", "蒙版插值"),
    ("Keyframe Rate:", "关键帧速率:"),
    ("per second", "每秒"),
    ("Keyframe Fields (doubles rate)", "“关键帧”字段（双重比率）"),
    ("Use “Linear” Vertex Paths", "使用“线性”顶点路径"),
    ("Bending Resistance:", "抗弯强度:"),
    ("Quality:", "品质:"),
    ("Add Mask Path Vertices", "添加蒙版路径顶点"),
    ("Pixels Between Vertices", "顶点之间的像素"),
    ("Total Vertices", "总顶点数"),
    ("Percentage of Outline", "轮廓的百分比"),
    ("Matching Method:", "配合法:"),
    ("Curve", "曲线"),
    ("Polyline", "多角线"),
    ("Use 1:1 Vertex Matches", "使用 1:1 顶点匹配"),
    ("First Vertices Match", "第一顶点匹配"),
    ("Apply", "应用"),
    (
        "Please select at least 2 consecutive mask path keyframes to interpolate between.",
        "请选择至少 2 个连续蒙版路径关键帧以在其间插值。",
    ),
];

/// Registers the assistants' Chinese strings and panels (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "mask_interpolation",
        bae_ui::PanelKind::new("Mask Interpolation", |_| {
            Box::new(mask_interpolation::MaskInterpolationPanel::default())
        }),
    );
    crate::register_panel(
        app,
        "wiggler",
        bae_ui::PanelKind::new("Wiggler", |_| Box::new(wiggler::WigglerPanel::default())),
    );
    crate::register_panel(
        app,
        "smoother",
        bae_ui::PanelKind::new("Smoother", |_| Box::new(smoother::SmootherPanel::default())),
    );
}

/// The selected keyframes by property (two or more each), times sorted.
fn properties(keys: &[KeyRef]) -> Vec<((PrimPath, String), Vec<Time>)> {
    let mut out: Vec<((PrimPath, String), Vec<Time>)> = Vec::new();
    for k in keys {
        match out
            .iter_mut()
            .find(|((p, a), _)| *p == k.path && *a == k.attr)
        {
            Some((_, t)) => t.push(k.time),
            None => out.push(((k.path.clone(), k.attr.clone()), vec![k.time])),
        }
    }
    out.retain(|(_, t)| t.len() >= 2);
    for (_, t) in &mut out {
        t.sort();
    }
    out
}

/// A value's numbers (one per dimension).
fn comps(v: &Value) -> Option<Vec<f64>> {
    Some(match v {
        Value::Float(x) => vec![*x],
        Value::Vec2(a) => a.to_vec(),
        Value::Vec3(a) => a.to_vec(),
        Value::Color(a) => a.to_vec(),
        _ => return None,
    })
}

/// `like` with its numbers replaced.
fn from_comps(like: &Value, c: &[f64]) -> Value {
    match like {
        Value::Float(_) => Value::Float(c[0]),
        Value::Vec2(_) => Value::Vec2([c[0], c[1]]),
        Value::Vec3(_) => Value::Vec3([c[0], c[1], c[2]]),
        Value::Color(_) => Value::Color([c[0], c[1], c[2], c[3]]),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use bae_model::{AuthorId, CommandRegistry, Stage};
    use serde_json::json;

    use super::*;

    /// A solid whose Rotation has keys 0 → 0°, 1 s → 10°, 2 s → 20°, 3 s → 90°.
    fn rotation_keys() -> (Stage, PrimPath) {
        let mut stage = Stage::new();
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let author = AuthorId::new("user:test").unwrap();
        let mut run = |id: &str, p: serde_json::Value| {
            reg.execute(&mut stage, &author, id, p).unwrap().output
        };
        let path = |v: &serde_json::Value| PrimPath::new(v["path"].as_str().unwrap()).unwrap();
        let comp = path(&run(
            "comp.create",
            json!({"width": 640, "height": 360, "fps": 30}),
        ));
        let layer = path(&run("layer.create", json!({"comp": comp, "kind": "solid"})));
        run(
            "attr.set_animated",
            json!({"path": layer, "attr": "transform:rotation", "animated": true, "time": 0}),
        );
        for (s, v) in [(0.0, 0.0), (1.0, 10.0), (2.0, 20.0), (3.0, 90.0)] {
            run(
                "attr.set_at",
                json!({"path": layer, "attr": "transform:rotation", "time": Time::from_seconds(s), "value": {"type": "float", "value": v}}),
            );
        }
        (stage, layer)
    }

    fn keys(layer: &PrimPath, secs: &[f64]) -> Vec<KeyRef> {
        secs.iter()
            .map(|s| KeyRef::new(layer.clone(), "transform:rotation", Time::from_seconds(*s)))
            .collect()
    }

    #[test]
    fn wiggler_adds_bounded_keys_at_the_frequency() {
        let (stage, layer) = rotation_keys();
        let s = wiggler::Settings::default();
        let mut eval = bae_anim::Evaluator::new();
        let edits =
            wiggler::wiggle_edits(&stage, &mut eval, &keys(&layer, &[0.0, 1.0]), &s).unwrap();
        // 5 per second between 0 and 1 s: 0.2, 0.4, 0.6, 0.8.
        assert_eq!(edits.len(), 4);
        for (i, e) in edits.iter().enumerate() {
            let v = e["value"]["value"].as_f64().unwrap();
            let base = 10.0 * 0.2 * (i + 1) as f64;
            assert!((v - base).abs() <= 1.0 + 1e-9, "{v} vs {base}");
        }
        let again =
            wiggler::wiggle_edits(&stage, &mut eval, &keys(&layer, &[0.0, 1.0]), &s).unwrap();
        assert_eq!(edits, again, "deterministic");
        assert!(wiggler::wiggle_edits(&stage, &mut eval, &keys(&layer, &[0.0]), &s).is_err());
    }

    #[test]
    fn smoother_drops_keys_on_the_line_and_keeps_corners() {
        let (stage, layer) = rotation_keys();
        let gone = smoother::smooth_deletions(&stage, &keys(&layer, &[0.0, 1.0, 2.0, 3.0]), 1.0);
        // 10° at 1 s lies on the line from 0 to 20° at 2 s; 20° at 2 s is a corner.
        assert_eq!(gone, keys(&layer, &[1.0]));
    }
}
