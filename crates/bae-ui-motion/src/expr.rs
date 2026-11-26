//! Expression text the UI writes, as AE does: property references (Add
//! Expression, the expression pick whip) and the pick whip's dimension rules.

use std::ops::Range;

use bae_anim::Evaluator;
use bae_model::{PrimPath, PrimType, Stage, Time, Value};
use bae_motion::query;
use bae_ui::PanelContext;
use serde_json::json;

use crate::common;

/// An expression field's text and its selection (chars).
pub(crate) type Selection = (String, Range<usize>);

/// An expression pick whip being dragged. Also the egui drag-and-drop payload, so
/// the Effect Controls take the drop too.
#[derive(Clone, Debug)]
pub(crate) struct Whip {
    pub path: PrimPath,
    pub attr: String,
    /// The text being edited and its selection (chars) when the field was active:
    /// the reference replaces the selection. Otherwise it replaces the expression.
    pub insert: Option<Selection>,
}

fn js(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// A layer's own property (`transform.position`), an effect parameter
/// (`effect("Blur")(1)`) or a mask property (`mask("Mask 1").maskPath`).
fn property_ref(stage: &Stage, layer: &PrimPath, path: &PrimPath, attr: &str) -> Option<String> {
    if path == layer {
        let name = match attr {
            "transform:anchor_point" => "transform.anchorPoint",
            "transform:position" => "transform.position",
            "transform:position_x" => "transform.xPosition",
            "transform:position_y" => "transform.yPosition",
            "transform:position_z" => "transform.zPosition",
            "transform:scale" => "transform.scale",
            "transform:rotation" if common::is_3d(stage, layer) => "transform.zRotation",
            "transform:rotation" => "transform.rotation",
            "transform:rotation_x" => "transform.xRotation",
            "transform:rotation_y" => "transform.yRotation",
            "transform:orientation" => "transform.orientation",
            "transform:opacity" => "transform.opacity",
            "transform:point_of_interest" => "transform.pointOfInterest",
            "time_remap" => "timeRemap",
            _ => return None,
        };
        return Some(name.to_owned());
    }
    let prim = stage.prim(path)?;
    let name = js(&query::display_name_of(stage, path));
    if prim.is_a(&PrimType::EFFECT) {
        let index: u32 = attr.strip_prefix("param:")?.parse().ok()?;
        return Some(format!("effect({name})({index})"));
    }
    if prim.is_a(&PrimType::MASK) {
        let prop = match attr {
            "path" => "maskPath",
            "feather" => "maskFeather",
            "opacity" => "maskOpacity",
            "expansion" => "maskExpansion",
            _ => return None,
        };
        return Some(format!("mask({name}).{prop}"));
    }
    None
}

/// How an expression on a property of `from` (a layer) refers to `target`'s
/// layer: `thisComp.layer("Name")`, or `comp("Comp").layer("Name")` in another
/// comp; `None` for `from` itself.
fn layer_ref(stage: &Stage, from: &PrimPath, target: &PrimPath) -> Option<String> {
    if target == from {
        return None;
    }
    let name = js(&query::display_name_of(stage, target));
    let comp = query::owning_comp(stage, target);
    Some(if comp == query::owning_comp(stage, from) {
        format!("thisComp.layer({name})")
    } else {
        let comp = comp.map_or_else(String::new, |c| query::display_name_of(stage, &c));
        format!("comp({}).layer({name})", js(&comp))
    })
}

/// The reference to `attr` of `path` (a property; a layer when `attr` is
/// `None`) from an expression on a property of layer `from`.
pub(crate) fn reference(
    stage: &Stage,
    from: &PrimPath,
    path: &PrimPath,
    attr: Option<&str>,
) -> Option<String> {
    let layer = query::owning_layer(stage, path)?;
    let Some(attr) = attr else {
        // An effect or a mask as a whole: effect("Blur"), mask("Mask 1").
        let whole = stage.prim(path).and_then(|v| {
            let name = js(&query::display_name_of(stage, path));
            if v.is_a(&PrimType::EFFECT) {
                Some(format!("effect({name})"))
            } else if v.is_a(&PrimType::MASK) {
                Some(format!("mask({name})"))
            } else {
                None
            }
        });
        let layer_part = layer_ref(stage, from, &layer);
        return Some(match (layer_part, whole) {
            (Some(l), Some(w)) => format!("{l}.{w}"),
            (None, Some(w)) => w,
            (Some(l), None) => l,
            (None, None) => "thisLayer".into(),
        });
    };
    let prop = property_ref(stage, &layer, path, attr)?;
    Some(match layer_ref(stage, from, &layer) {
        Some(l) => format!("{l}.{prop}"),
        None => prop,
    })
}

/// AE's expression for a new expression (Alt+click on the stopwatch, Animation
/// > Add Expression): the property's own reference.
pub(crate) fn default_expression(stage: &Stage, path: &PrimPath, attr: &str) -> String {
    query::owning_layer(stage, path)
        .and_then(|l| reference(stage, &l, path, Some(attr)))
        .unwrap_or_else(|| "value".into())
}

/// Number of values of a property (1 for a number, 2 for a point of a 2D layer).
pub(crate) fn dims(stage: &Stage, path: &PrimPath, value: &Value) -> usize {
    match value {
        Value::Vec2(_) => 2,
        Value::Vec3(_) if query::is_layer(stage, path) && !common::is_3d(stage, path) => 2,
        Value::Vec3(_) => 3,
        Value::Color(_) => 4,
        _ => 1,
    }
}

/// The pick whip's expression for a property of `owner` values linked to a
/// reference of `target` values: the reference, its first value
/// (`ref[0]`), or the values spread through `temp` (`[temp, temp]`,
/// `[temp[0], temp[1], temp[1]]`).
pub(crate) fn linked(reference: &str, owner: usize, target: usize) -> String {
    if owner == target || target == 0 {
        return reference.to_owned();
    }
    if owner == 1 {
        return format!("{reference}[0]");
    }
    let parts: Vec<String> = (0..owner)
        .map(|i| {
            if target == 1 {
                "temp".to_owned()
            } else {
                format!("temp[{}]", i.min(target - 1))
            }
        })
        .collect();
    format!("temp = {reference};\n[{}]", parts.join(", "))
}

/// `text` with the chars `range` replaced by `with`.
pub(crate) fn splice(text: &str, range: &Range<usize>, with: &str) -> String {
    let byte = |c: usize| text.char_indices().nth(c).map_or(text.len(), |(b, _)| b);
    let (a, b) = (
        byte(range.start.min(range.end)),
        byte(range.end.max(range.start)),
    );
    format!("{}{with}{}", &text[..a], &text[b..])
}

/// Drop the pick whip on `target` (a property, or a layer when `attr` is
/// `None`): the reference replaces the expression or the selection being edited.
pub(crate) fn link(
    cx: &mut PanelContext<'_>,
    eval: &mut Evaluator,
    whip: &Whip,
    target: &PrimPath,
    attr: Option<&str>,
    now: Time,
) {
    let stage = cx.stage;
    if *target == whip.path && attr == Some(whip.attr.as_str()) {
        return;
    }
    let Some(from) = query::owning_layer(stage, &whip.path) else {
        return;
    };
    let Some(reference) = reference(stage, &from, target, attr) else {
        return;
    };
    let mut value_dims = |path: &PrimPath, attr: &str| {
        common::current_value(eval, stage, path, attr, now).map_or(0, |v| dims(stage, path, &v))
    };
    let owner = value_dims(&whip.path, &whip.attr);
    let text = match attr {
        Some(a) => linked(&reference, owner, value_dims(target, a)),
        None => reference,
    };
    let source = match &whip.insert {
        Some((buf, sel)) => splice(buf, sel, &text),
        None => text,
    };
    let enabled = query::attr_spec(stage, &whip.path, &whip.attr)
        .and_then(|s| s.expression.as_ref())
        .is_none_or(|e| e.enabled);
    cx.invoke_with(
        "attr.set_expression",
        json!({"path": whip.path, "attr": whip.attr, "source": source, "enabled": enabled}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_follows_the_pick_whip_dimension_rules() {
        let r = "thisComp.layer(\"A\").transform.position";
        assert_eq!(linked(r, 2, 2), r);
        assert_eq!(linked(r, 1, 2), format!("{r}[0]"));
        assert_eq!(linked("x", 2, 1), "temp = x;\n[temp, temp]");
        assert_eq!(linked("x", 3, 2), "temp = x;\n[temp[0], temp[1], temp[1]]");
        assert_eq!(linked("x", 2, 3), "temp = x;\n[temp[0], temp[1]]");
    }

    #[test]
    fn splice_replaces_the_selection() {
        assert_eq!(splice("a + b", &(4..5), "c"), "a + c");
        assert_eq!(splice("a + ", &(4..4), "時間"), "a + 時間");
        assert_eq!(splice("時間 b", &Range { start: 4, end: 3 }, "x"), "時間 x");
    }
}
