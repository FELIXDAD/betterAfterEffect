//! Model values ⇄ the JSON the script prelude exchanges with the host, in AE
//! scripting units: numbers, arrays (`[x, y, z]`, `[r, g, b, a]`), seconds for
//! times, layer/item paths for references, `{text, fontSize, …}` for a
//! `TextDocument` and `{vertices, inTangents, outTangents, closed}` for a Shape.

use bae_model::{BezierPath, PrimPath, Time, Value, ValueType};
use serde_json::{Value as Json, json};

/// A model value as the script sees it.
pub fn to_js(v: &Value) -> Json {
    match v {
        Value::Float(f) => json!(f),
        Value::Int(i) => json!(i),
        Value::Bool(b) => json!(b),
        Value::String(s) | Value::Enum(s) => json!(s),
        Value::Vec2(a) => json!(a),
        Value::Vec3(a) => json!(a),
        Value::Color(c) => json!(c),
        Value::FloatList(l) => json!(l),
        Value::Time(t) => json!(t.as_seconds()),
        Value::FrameRate(r) => json!(r.as_f64()),
        Value::AssetRef(p) | Value::PrimRef(p) => json!(p.as_str()),
        Value::Bezier(b) => json!({
            "vertices": b.vertices,
            "inTangents": b.in_tangents,
            "outTangents": b.out_tangents,
            "closed": b.closed,
        }),
        Value::TextDocument(d) => json!({
            "text": d.text,
            "font": d.font,
            "fontSize": d.font_size,
            "fillColor": &d.fill_color[..3],
            "strokeColor": d.stroke_color.map(|c| [c[0], c[1], c[2]]),
            "strokeWidth": d.stroke_width,
            "applyFill": true,
            "applyStroke": d.stroke_color.is_some(),
        }),
        Value::Bytes(_) => Json::Null,
    }
}

/// A zero value of `ty`, used as the template for effect parameters that have
/// no default.
pub fn zero_of(ty: ValueType) -> Option<Value> {
    Some(match ty {
        ValueType::Float => Value::Float(0.0),
        ValueType::Int => Value::Int(0),
        ValueType::Bool => Value::Bool(false),
        ValueType::String => Value::String(String::new()),
        ValueType::Vec2 => Value::Vec2([0.0; 2]),
        ValueType::Vec3 => Value::Vec3([0.0; 3]),
        ValueType::Color => Value::Color([0.0, 0.0, 0.0, 1.0]),
        ValueType::FloatList => Value::FloatList(Vec::new()),
        ValueType::Time => Value::Time(Time::ZERO),
        _ => return None,
    })
}

fn numbers(js: &Json) -> Option<Vec<f64>> {
    match js {
        Json::Array(a) => a.iter().map(Json::as_f64).collect(),
        Json::Number(n) => n.as_f64().map(|f| vec![f]),
        _ => None,
    }
}

fn number(js: &Json) -> Option<f64> {
    match js {
        Json::Bool(b) => Some(f64::from(u8::from(*b))),
        Json::Array(a) if a.len() == 1 => a[0].as_f64(),
        _ => js.as_f64(),
    }
}

fn expected(what: &str, js: &Json) -> String {
    format!("expected {what}, got {js}")
}

/// A script value converted to the type of `template` (the property's current
/// value). Without a template the JSON shape decides.
pub fn from_js(template: Option<&Value>, js: &Json) -> Result<Value, String> {
    let nums = || numbers(js).ok_or_else(|| expected("an array of numbers", js));
    let num = || number(js).ok_or_else(|| expected("a number", js));
    let text = || match js {
        Json::String(s) => Ok(s.clone()),
        Json::Number(n) => Ok(n.to_string()),
        _ => Err(expected("a string", js)),
    };
    Ok(match template {
        Some(Value::Float(_)) => Value::Float(num()?),
        Some(Value::Int(_)) => Value::Int(num()?.round() as i64),
        Some(Value::Bool(_)) => Value::Bool(match js {
            Json::Bool(b) => *b,
            _ => num()? != 0.0,
        }),
        Some(Value::String(_)) => Value::String(text()?),
        Some(Value::Enum(_)) => Value::Enum(text()?),
        Some(Value::Time(_)) => Value::Time(Time::from_seconds(num()?)),
        Some(Value::Vec2(_)) => match nums()?.as_slice() {
            [x, y, ..] => Value::Vec2([*x, *y]),
            _ => return Err(expected("[x, y]", js)),
        },
        Some(Value::Vec3(old)) => match nums()?.as_slice() {
            [x, y] => Value::Vec3([*x, *y, old[2]]),
            [x, y, z, ..] => Value::Vec3([*x, *y, *z]),
            _ => return Err(expected("[x, y, z]", js)),
        },
        Some(Value::Color(old)) => match nums()?.as_slice() {
            [r, g, b] => Value::Color([*r, *g, *b, old[3]]),
            [r, g, b, a, ..] => Value::Color([*r, *g, *b, *a]),
            _ => return Err(expected("[r, g, b, a]", js)),
        },
        Some(Value::FloatList(_)) => Value::FloatList(nums()?),
        Some(Value::PrimRef(_)) => Value::PrimRef(path_of(js)?),
        Some(Value::AssetRef(_)) => Value::AssetRef(path_of(js)?),
        Some(Value::TextDocument(doc)) => {
            let mut doc = doc.clone();
            match js {
                Json::String(s) => doc.text = s.clone(),
                Json::Object(o) => {
                    if let Some(t) = o.get("text").and_then(Json::as_str) {
                        doc.text = t.to_owned();
                    }
                    if let Some(f) = o.get("font").and_then(Json::as_str) {
                        doc.font = f.to_owned();
                    }
                    if let Some(s) = o.get("fontSize").and_then(Json::as_f64) {
                        doc.font_size = s;
                    }
                    if let Some([r, g, b, ..]) = o.get("fillColor").and_then(numbers).as_deref() {
                        doc.fill_color = [*r, *g, *b, doc.fill_color[3]];
                    }
                    if let Some([r, g, b, ..]) = o.get("strokeColor").and_then(numbers).as_deref() {
                        doc.stroke_color = Some([*r, *g, *b, 1.0]);
                    }
                    if let Some(w) = o.get("strokeWidth").and_then(Json::as_f64) {
                        doc.stroke_width = w;
                    }
                    if o.get("applyStroke") == Some(&Json::Bool(false)) {
                        doc.stroke_color = None;
                    }
                }
                _ => return Err(expected("a string or a TextDocument", js)),
            }
            Value::TextDocument(doc)
        }
        Some(Value::Bezier(_)) => {
            let pts = |k: &str| -> Result<Vec<[f64; 2]>, String> {
                let a = js
                    .get(k)
                    .and_then(Json::as_array)
                    .ok_or_else(|| expected("a Shape", js))?;
                a.iter()
                    .map(|p| match numbers(p).as_deref() {
                        Some([x, y, ..]) => Ok([*x, *y]),
                        _ => Err(expected("[x, y]", p)),
                    })
                    .collect()
            };
            let vertices = pts("vertices")?;
            let zeros = || vec![[0.0; 2]; vertices.len()];
            Value::Bezier(BezierPath {
                in_tangents: pts("inTangents").unwrap_or_else(|_| zeros()),
                out_tangents: pts("outTangents").unwrap_or_else(|_| zeros()),
                closed: js.get("closed").and_then(Json::as_bool).unwrap_or(true),
                vertices,
                feather: Vec::new(),
            })
        }
        Some(Value::FrameRate(_) | Value::Bytes(_)) => {
            return Err("this property cannot be set from a script".into());
        }
        None => match js {
            Json::Bool(b) => Value::Bool(*b),
            Json::String(s) => Value::String(s.clone()),
            Json::Number(_) => Value::Float(num()?),
            Json::Array(_) => match nums()?.as_slice() {
                [x, y] => Value::Vec2([*x, *y]),
                [x, y, z] => Value::Vec3([*x, *y, *z]),
                [r, g, b, a] => Value::Color([*r, *g, *b, *a]),
                _ => Value::FloatList(nums()?),
            },
            _ => return Err(expected("a value", js)),
        },
    })
}

fn path_of(js: &Json) -> Result<PrimPath, String> {
    let s = js.as_str().ok_or_else(|| expected("a layer", js))?;
    PrimPath::new(s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_in_ae_units() {
        let v = from_js(Some(&Value::Vec3([0.0, 0.0, 5.0])), &json!([10, 20])).unwrap();
        assert_eq!(v, Value::Vec3([10.0, 20.0, 5.0]));
        let c = from_js(Some(&Value::Color([0.0, 0.0, 0.0, 0.5])), &json!([1, 0, 0])).unwrap();
        assert_eq!(c, Value::Color([1.0, 0.0, 0.0, 0.5]));
        assert_eq!(
            from_js(Some(&Value::Int(0)), &json!(2.6)).unwrap(),
            Value::Int(3)
        );
        assert_eq!(to_js(&Value::Time(Time::from_seconds(1.5))), json!(1.5));
        assert!(from_js(Some(&Value::Vec2([0.0; 2])), &json!("x")).is_err());
    }
}
