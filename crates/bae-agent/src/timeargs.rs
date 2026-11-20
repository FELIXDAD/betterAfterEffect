//! Frame numbers and timecode for `Time` parameters.
//!
//! Commands take times in flicks (`bae_model::Time`). Agents think in frames, so the
//! MCP layer also accepts, wherever a command's schema has a `Time`:
//!
//! - `{"frame": 12}`, `"12f"` or `"f12"`: a frame number;
//! - `"0:00:00:12"` (`H:MM:SS:FF`, `MM:SS:FF` or `SS:FF`): timecode, non-drop; `;`
//!   before the frames means drop-frame (29.97 / 59.94).
//!
//! Frames count at the frame rate of the comp the command works in (a top-level
//! `frame_rate` / `fps` argument or its schema default, e.g. `comp.create`; else the
//! comp around the first prim path in the arguments), in the parameter's own time
//! base: comp time for comp and
//! layer timing, layer time for keyframes on layers (the same frames `prim_get`
//! reports in `key_frames`). Integers are still flicks.
//!
//! Conversion runs inside the transaction, so a comp created by an earlier command of
//! the same batch provides the frame rate for later ones.

use bae_model::{FrameRate, PrimPath, Stage, Time};
use serde_json::{Map, Value as Json, json};

use crate::describe::{comp_settings, enclosing_comp};

/// Name schemars gives `bae_model::Time` in `$defs`.
const TIME_DEF: &str = "Time";
const MAX_DEPTH: usize = 64;

/// Does this command schema have `Time` parameters?
pub fn has_time(schema: &Json) -> bool {
    schema
        .pointer(&format!("/$defs/{TIME_DEF}"))
        .is_some_and(|d| d.get("type").and_then(Json::as_str) == Some("integer"))
}

/// The schema agents see for `Time` parameters.
pub fn agent_time_schema() -> Json {
    json!({
        "description": "Time: an integer number of flicks (705600000 per second), or a frame number {\"frame\": 12} / \"12f\", or timecode \"0:00:00:12\" (\";\" before the frames = drop-frame). Frames count at the frame rate of the comp the command works in, in the parameter's own time base (comp time; layer time for keyframes on layers, as prim_get's key_frames).",
        "anyOf": [
            { "type": "integer" },
            {
                "type": "object",
                "properties": { "frame": { "type": "integer" } },
                "required": ["frame"],
                "additionalProperties": false
            },
            { "type": "string" }
        ]
    })
}

/// Replace the `Time` definition in a tool's input schema with
/// [`agent_time_schema`].
pub fn rewrite_schema(schema: &mut Map<String, Json>) {
    if let Some(Json::Object(defs)) = schema.get_mut("$defs")
        && defs
            .get(TIME_DEF)
            .is_some_and(|d| d.get("type").and_then(Json::as_str) == Some("integer"))
    {
        defs.insert(TIME_DEF.into(), agent_time_schema());
    }
}

/// A time value given in frames, before the frame rate is known.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FrameForm {
    Frames(i64),
    /// Timecode fields; `drop` for `;` before the frames.
    Timecode {
        hours: i64,
        minutes: i64,
        seconds: i64,
        frames: i64,
        drop: bool,
    },
}

fn parse_frame_form(v: &Json) -> Result<Option<FrameForm>, String> {
    match v {
        Json::Number(_) => Ok(None),
        Json::Object(o) => match (o.len(), o.get("frame").and_then(Json::as_i64)) {
            (1, Some(f)) => Ok(Some(FrameForm::Frames(f))),
            _ => Err(format!(
                "{v} is not a time (flicks, {{\"frame\": n}}, \"12f\" or timecode)"
            )),
        },
        Json::String(s) => parse_time_string(s).map(Some),
        _ => Ok(None),
    }
}

fn parse_time_string(s: &str) -> Result<FrameForm, String> {
    let t = s.trim();
    let bad = || {
        format!(
            "{s:?} is not a time: use flicks, {{\"frame\": n}}, \"12f\" or timecode \"0:00:00:12\""
        )
    };
    if let Some(n) = t.strip_suffix('f').or_else(|| t.strip_prefix('f')) {
        return n
            .trim()
            .parse::<i64>()
            .map(FrameForm::Frames)
            .map_err(|_| bad());
    }
    let drop = t.contains(';');
    let parts: Vec<&str> = t.split([':', ';']).collect();
    if !(2..=4).contains(&parts.len()) || t.matches(';').count() > 1 {
        return Err(bad());
    }
    if drop && !t.rsplit_once(';').is_some_and(|(_, f)| !f.contains(':')) {
        return Err(bad());
    }
    let nums: Vec<i64> = parts
        .iter()
        .map(|p| p.trim().parse::<i64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    let mut f = [0i64; 4];
    f[4 - nums.len()..].copy_from_slice(&nums);
    if f[1..].iter().any(|x| *x < 0) || f[1] > 59 || f[2] > 59 {
        return Err(bad());
    }
    Ok(FrameForm::Timecode {
        hours: f[0],
        minutes: f[1],
        seconds: f[2],
        frames: f[3],
        drop,
    })
}

fn to_time(form: FrameForm, rate: FrameRate) -> Result<Time, String> {
    let frame = match form {
        FrameForm::Frames(n) => n,
        FrameForm::Timecode {
            hours,
            minutes,
            seconds,
            frames,
            drop,
        } => {
            let nominal = rate.as_f64().round() as i64;
            if frames >= nominal {
                return Err(format!(
                    "timecode frame field {frames} is not below {nominal} fps"
                ));
            }
            let total_minutes = hours * 60 + minutes;
            let mut n = (total_minutes * 60 + seconds) * nominal + frames;
            if drop {
                // SMPTE drop-frame: skip frame numbers 0 and 1 (0-3 at 59.94) at the
                // start of every minute except every tenth.
                let per = match nominal {
                    30 => 2,
                    60 => 4,
                    _ => {
                        return Err(format!(
                            "drop-frame timecode needs 29.97 or 59.94 fps, not {}",
                            rate.as_f64()
                        ));
                    }
                };
                n -= per * (total_minutes - total_minutes / 10);
            }
            n
        }
    };
    Ok(rate.frame_to_time(frame))
}

/// Visit every value at a `Time` position of `schema` in `value`.
fn walk(
    node: &Json,
    defs: &Map<String, Json>,
    value: &mut Json,
    depth: usize,
    visit: &mut dyn FnMut(&mut Json) -> Result<(), String>,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    if let Some(r) = node.get("$ref").and_then(Json::as_str) {
        let name = r.strip_prefix("#/$defs/").unwrap_or(r);
        if name == TIME_DEF {
            return visit(value);
        }
        if let Some(def) = defs.get(name) {
            walk(def, defs, value, depth + 1, visit)?;
        }
    }
    if let Some(Json::Array(branches)) = node.get("allOf") {
        for b in branches {
            walk(b, defs, value, depth + 1, visit)?;
        }
    }
    // Only the alternatives that fit the value: a tagged `Value` holding a string
    // must not be read as its `time` variant.
    for key in ["anyOf", "oneOf"] {
        if let Some(Json::Array(branches)) = node.get(key) {
            for b in branches {
                if branch_matches(b, defs, value, depth + 1) {
                    walk(b, defs, value, depth + 1, visit)?;
                }
            }
        }
    }
    match value {
        Json::Object(obj) => {
            if let Some(Json::Object(props)) = node.get("properties") {
                for (k, sub) in props {
                    if let Some(v) = obj.get_mut(k) {
                        walk(sub, defs, v, depth + 1, visit)?;
                    }
                }
            }
        }
        Json::Array(items) => {
            if let Some(item) = node.get("items") {
                for v in items.iter_mut() {
                    walk(item, defs, v, depth + 1, visit)?;
                }
            }
            if let Some(Json::Array(prefix)) = node.get("prefixItems") {
                for (sub, v) in prefix.iter().zip(items.iter_mut()) {
                    walk(sub, defs, v, depth + 1, visit)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn json_type_matches(ty: &str, v: &Json) -> bool {
    match ty {
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "string" => v.is_string(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        _ => true,
    }
}

/// Could `value` be an instance of the alternative `branch`? Checks `type`,
/// `const` (including tag properties) and `required`; a reference to `Time` always
/// fits (frame forms are not integers yet).
fn branch_matches(branch: &Json, defs: &Map<String, Json>, value: &Json, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    if let Some(r) = branch.get("$ref").and_then(Json::as_str) {
        let name = r.strip_prefix("#/$defs/").unwrap_or(r);
        if name == TIME_DEF {
            return true;
        }
        if let Some(def) = defs.get(name)
            && !branch_matches(def, defs, value, depth + 1)
        {
            return false;
        }
    }
    if let Some(c) = branch.get("const")
        && c != value
    {
        return false;
    }
    match branch.get("type") {
        Some(Json::String(t)) if !json_type_matches(t, value) => return false,
        Some(Json::Array(ts))
            if !ts
                .iter()
                .filter_map(Json::as_str)
                .any(|t| json_type_matches(t, value)) =>
        {
            return false;
        }
        _ => {}
    }
    if let Json::Object(obj) = value {
        if let Some(Json::Object(props)) = branch.get("properties") {
            for (k, sub) in props {
                if let Some(c) = sub.get("const")
                    && obj.get(k) != Some(c)
                {
                    return false;
                }
            }
        }
        if let Some(Json::Array(req)) = branch.get("required")
            && !req
                .iter()
                .filter_map(Json::as_str)
                .all(|k| obj.contains_key(k))
        {
            return false;
        }
    }
    true
}

fn frame_rate_param(v: &Json) -> Option<FrameRate> {
    let v = match v.get("type").and_then(Json::as_str) {
        Some("frame_rate") => v.get("value")?,
        _ => v,
    };
    if let Some(fps) = v.as_f64() {
        return (fps > 0.0).then(|| FrameRate::from_fps(fps));
    }
    let num = u32::try_from(v.get("num")?.as_u64()?).ok()?;
    let den = u32::try_from(v.get("den")?.as_u64()?).ok()?;
    (num > 0 && den > 0).then(|| FrameRate::new(num, den))
}

fn collect_paths<'a>(v: &'a Json, out: &mut Vec<&'a str>) {
    match v {
        Json::String(s) if s.starts_with('/') => out.push(s),
        Json::Array(a) => a.iter().for_each(|x| collect_paths(x, out)),
        Json::Object(o) => o.values().for_each(|x| collect_paths(x, out)),
        _ => {}
    }
}

/// The frame rate frames are counted at for a command call: a `frame_rate` / `fps`
/// argument, else that parameter's schema default (a command creating a comp),
/// else the comp around the first prim path in the arguments.
fn frame_rate_for(stage: &Stage, schema: &Json, params: &Json) -> Result<FrameRate, String> {
    for key in ["frame_rate", "fps"] {
        if let Some(r) = params.get(key).and_then(frame_rate_param) {
            return Ok(r);
        }
    }
    for key in ["frame_rate", "fps"] {
        if let Some(r) = schema
            .pointer(&format!("/properties/{key}/default"))
            .and_then(frame_rate_param)
        {
            return Ok(r);
        }
    }
    let mut paths = Vec::new();
    collect_paths(params, &mut paths);
    for p in paths {
        let Ok(path) = PrimPath::new(p) else { continue };
        if let Some(settings) = enclosing_comp(stage, &path).and_then(|c| comp_settings(stage, &c))
        {
            return Ok(settings.frame_rate);
        }
    }
    Err(
        "frame numbers need a comp for the frame rate, but no parameter is a prim inside a comp \
         (and there is no frame_rate / fps parameter); pass times in flicks instead"
            .into(),
    )
}

/// Convert frame numbers and timecode at `Time` positions of `schema` into flicks.
/// Parameters without frame-form times are returned unchanged.
pub fn convert(stage: &Stage, schema: &Json, mut params: Json) -> Result<Json, String> {
    if !has_time(schema) {
        return Ok(params);
    }
    let empty = Map::new();
    let defs = schema
        .get("$defs")
        .and_then(Json::as_object)
        .unwrap_or(&empty);
    let mut needed = false;
    walk(schema, defs, &mut params, 0, &mut |v| {
        needed |= parse_frame_form(v)?.is_some();
        Ok(())
    })?;
    if !needed {
        return Ok(params);
    }
    let rate = frame_rate_for(stage, schema, &params)?;
    walk(schema, defs, &mut params, 0, &mut |v| {
        if let Some(form) = parse_frame_form(v)? {
            *v = json!(to_time(form, rate)?.flicks());
        }
        Ok(())
    })?;
    Ok(params)
}

#[cfg(test)]
mod tests {
    use bae_model::{AuthorId, CommandRegistry};

    use super::*;

    fn stage_with_comp(fps: (u32, u32)) -> (Stage, CommandRegistry) {
        let mut stage = Stage::new();
        let reg = CommandRegistry::with_builtins();
        let who = AuthorId::system();
        for (p, t) in [
            ("/c", "Comp"),
            ("/c/layers", "Folder"),
            ("/c/layers/l", "Layer"),
        ] {
            reg.execute(
                &mut stage,
                &who,
                "prim.create",
                json!({"path": p, "prim_type": t}),
            )
            .unwrap();
        }
        reg.execute(
            &mut stage,
            &who,
            "attr.set",
            json!({"path": "/c", "attr": "frame_rate",
                   "value": {"type": "frame_rate", "value": {"num": fps.0, "den": fps.1}}}),
        )
        .unwrap();
        (stage, reg)
    }

    fn schema(reg: &CommandRegistry, id: &str) -> Json {
        reg.get(id).unwrap().info().params_schema.clone()
    }

    #[test]
    fn frames_and_timecode_become_flicks() {
        let (stage, reg) = stage_with_comp((25, 1));
        let s = schema(&reg, "key.set");
        assert!(has_time(&s));
        let per_frame = Time::FLICKS_PER_SECOND / 25;
        for (t, frames) in [
            (json!({"frame": 12}), 12),
            (json!("12f"), 12),
            (json!("f12"), 12),
            (json!("0:00:01:05"), 30),
            (json!("01:05"), 30),
            (json!(7 * per_frame), 7),
        ] {
            let p = json!({"path": "/c/layers/l", "attr": "transform:opacity", "time": t,
                           "keyframe": {"value": {"type": "float", "value": 1.0}}});
            let out = convert(&stage, &s, p).unwrap();
            assert_eq!(out["time"], json!(frames * per_frame), "{t}");
        }
        // Converted params run as normal.
        let mut stage = stage;
        let p = convert(
            &stage,
            &s,
            json!({"path": "/c/layers/l", "attr": "transform:opacity", "time": "10f",
                   "keyframe": {"value": {"type": "float", "value": 1.0}}}),
        )
        .unwrap();
        reg.execute(&mut stage, &AuthorId::system(), "key.set", p)
            .unwrap();
    }

    #[test]
    fn drop_frame_timecode() {
        let (stage, reg) = stage_with_comp((30_000, 1001));
        let s = schema(&reg, "key.remove");
        let rate = FrameRate::new(30_000, 1001);
        // 00:01:00;02 is frame 1800 in drop-frame (frames ;00 and ;01 do not exist).
        let p = json!({"path": "/c", "attr": "x", "time": "00:01:00;02"});
        assert_eq!(
            convert(&stage, &s, p).unwrap()["time"],
            json!(rate.frame_to_time(1800).flicks())
        );
        // 00:10:00;00 is frame 17982.
        let p = json!({"path": "/c", "attr": "x", "time": "00:10:00;00"});
        assert_eq!(
            convert(&stage, &s, p).unwrap()["time"],
            json!(rate.frame_to_time(17_982).flicks())
        );
    }

    #[test]
    fn errors_and_untouched_params() {
        let (stage, reg) = stage_with_comp((30, 1));
        let s = schema(&reg, "key.remove");
        let no_comp = json!({"path": "/nowhere", "attr": "x", "time": {"frame": 3}});
        assert!(
            convert(&stage, &s, no_comp)
                .unwrap_err()
                .contains("frame rate")
        );
        for bad in [
            json!("soon"),
            json!({"frame": 1, "x": 2}),
            json!("1:2:3:4:5"),
            json!("0:00:00:30"),
        ] {
            let p = json!({"path": "/c", "attr": "x", "time": bad});
            assert!(convert(&stage, &s, p).is_err(), "{bad}");
        }
        // A string Value that looks like a frame stays a string; a time Value converts.
        let set = schema(&reg, "attr.set");
        let p = json!({"path": "/c", "attr": "name", "value": {"type": "string", "value": "12f"}});
        assert_eq!(convert(&stage, &set, p.clone()).unwrap(), p);
        let p = json!({"path": "/c/layers/l", "attr": "in_point", "value": {"type": "time", "value": "12f"}});
        assert_eq!(
            convert(&stage, &set, p).unwrap()["value"]["value"],
            json!(12 * Time::FLICKS_PER_SECOND / 30)
        );
        // No Time anywhere in the schema: untouched.
        let p = json!({"path": "/c", "prim_type": "12f"});
        assert_eq!(
            convert(&stage, &schema(&reg, "prim.create"), p.clone()).unwrap(),
            p
        );
        // An explicit frame_rate parameter wins.
        let p = json!({"path": "/x", "attr": "x", "time": {"frame": 1}, "frame_rate": {"num": 10, "den": 1}});
        assert_eq!(
            convert(&stage, &s, p).unwrap()["time"],
            json!(Time::FLICKS_PER_SECOND / 10)
        );
    }

    #[test]
    fn rate_from_fps_argument_or_its_default() {
        let stage = Stage::new();
        let schema = json!({
            "$defs": {"Time": {"type": "integer"}},
            "properties": {
                "path": {"type": "string"},
                "fps": {"type": "number", "default": 24.0},
                "duration": {"$ref": "#/$defs/Time"}
            }
        });
        let sec = Time::FLICKS_PER_SECOND;
        let p = json!({"path": "/new", "duration": "48f"});
        assert_eq!(
            convert(&stage, &schema, p).unwrap()["duration"],
            json!(2 * sec)
        );
        let p = json!({"path": "/new", "fps": 25, "duration": "50f"});
        assert_eq!(
            convert(&stage, &schema, p).unwrap()["duration"],
            json!(2 * sec)
        );
        let p = json!({"path": "/new", "fps": 29.97, "duration": "0:00:01:00"});
        assert_eq!(
            convert(&stage, &schema, p).unwrap()["duration"],
            json!(FrameRate::new(30_000, 1001).frame_to_time(30).flicks())
        );
    }

    #[test]
    fn schema_rewrite() {
        let (_, reg) = stage_with_comp((30, 1));
        let mut s = schema(&reg, "key.set").as_object().unwrap().clone();
        rewrite_schema(&mut s);
        assert!(s["$defs"]["Time"]["anyOf"].is_array());
        assert!(!has_time(&Json::Object(s)));
    }
}
