//! The host side of the object model: the prelude (`prelude.js`) calls
//! `__host(method, json)`; [`call`] answers reads from the transaction's stage
//! (so a script sees its own changes) and turns every change into a registered
//! command run inside the same transaction.
//!
//! Times are seconds of comp time, as in AE; keyframe times are converted with
//! `bae_motion::query::{to_key_time, to_comp_time}`.

use bae_model::{PrimPath, PrimType, PrimView, Stage, Time, Transaction, Value};
use bae_motion::query;
use serde_json::{Value as Json, json};

use crate::convert::{from_js, to_js, zero_of};
use crate::{ScriptEnv, ScriptReport};

/// State of one run, reachable from the host function.
pub(crate) struct RunState<'a, 'e, 't> {
    pub tx: &'a mut Transaction<'t>,
    pub env: &'a ScriptEnv<'e>,
    pub access: crate::ProjectAccess,
    /// Selected layers (`layer.selected = …` changes it for the rest of the run).
    pub selected: Vec<PrimPath>,
    /// Version of the project and selection visible to this run.
    pub generation: u32,
    pub playheads: std::collections::BTreeMap<PrimPath, Time>,
    pub evaluator: bae_anim::Evaluator,
    pub output_bytes: usize,
    pub report: ScriptReport,
}

type R = Result<Json, String>;

fn arg<'j>(args: &'j Json, key: &str) -> Result<&'j Json, String> {
    args.get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| format!("missing `{key}`"))
}

fn path(args: &Json, key: &str) -> Result<PrimPath, String> {
    let s = arg(args, key)?
        .as_str()
        .ok_or_else(|| format!("`{key}` is not a path"))?;
    PrimPath::new(s).map_err(|e| e.to_string())
}

fn string(args: &Json, key: &str) -> Result<String, String> {
    arg(args, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("`{key}` is not a string"))
}

fn seconds(args: &Json, key: &str) -> Option<f64> {
    args.get(key).and_then(Json::as_f64)
}

fn secs(t: Time) -> f64 {
    t.as_seconds()
}

fn flicks(seconds: f64) -> Time {
    Time::from_seconds(seconds)
}

fn bool_attr(view: &PrimView<'_>, attr: &str, default: bool) -> bool {
    query::static_value(view, attr)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn prim_ref(view: &PrimView<'_>, attr: &str) -> Json {
    view.value(attr)
        .and_then(Value::as_prim_ref)
        .map_or(Json::Null, |p| json!(p.as_str()))
}

fn view<'s>(stage: &'s Stage, path: &PrimPath) -> Result<PrimView<'s>, String> {
    stage
        .prim(path)
        .ok_or_else(|| format!("{path} does not exist (it may have been deleted)"))
}

/// Project panel item type (`comp`, `folder`, `footage`), `None` for everything
/// else (layers, effects, the folders inside comps and layers).
fn item_type(stage: &Stage, path: &PrimPath) -> Option<&'static str> {
    let ty = item_kind(&stage.prim(path)?)?;
    let mut up = path.parent();
    while let Some(p) = up {
        if stage
            .prim(&p)
            .is_some_and(|a| a.is_a(&PrimType::COMP) || a.is_a(&PrimType::LAYER))
        {
            return None;
        }
        up = p.parent();
    }
    Some(ty)
}

fn item_kind(v: &PrimView<'_>) -> Option<&'static str> {
    if v.is_a(&PrimType::COMP) {
        Some("comp")
    } else if v.is_a(&PrimType::FOLDER) {
        Some("folder")
    } else if v.is_a(&PrimType::ASSET) {
        Some("footage")
    } else {
        None
    }
}

fn items(stage: &Stage) -> Vec<Json> {
    let mut rows = Vec::new();
    let mut stack: Vec<_> = stage
        .children(&PrimPath::root())
        .into_iter()
        .rev()
        .map(|p| (p, None::<PrimPath>))
        .collect();
    while let Some((path, parent)) = stack.pop() {
        let Some(v) = stage.prim(&path) else { continue };
        let kind = item_kind(&v);
        if let Some(kind) = kind {
            rows.push(json!({ "path": path, "type": kind, "name": query::display_name(&v), "parent": parent }));
        }
        if kind == Some("comp") || v.is_a(&PrimType::LAYER) {
            continue;
        }
        let folder = (kind == Some("folder")).then_some(&path);
        stack.extend(
            stage
                .children(&path)
                .into_iter()
                .rev()
                .map(|p| (p, folder.cloned())),
        );
    }
    rows
}

fn parent_folder(stage: &Stage, path: &PrimPath) -> Json {
    path.parent()
        .filter(|p| item_type(stage, p) == Some("folder"))
        .map_or(Json::Null, |p| json!(p.as_str()))
}

fn item(stage: &Stage, path: &PrimPath) -> R {
    let v = view(stage, path)?;
    let ty = item_type(stage, path).ok_or_else(|| format!("{path} is not a project item"))?;
    let mut out = json!({
        "type": ty,
        "name": query::display_name(&v),
        "parent": parent_folder(stage, path),
    });
    match ty {
        "comp" => {
            let c = query::comp_info(stage, path).ok_or("not a comp")?;
            let layers: Vec<_> = query::comp_layers(stage, path).into_iter().map(|p| {
                json!({ "path": p, "kind": query::layer_kind(stage, &p),
                    "name": query::display_name(&stage.prim(&p).expect("comp_layers returned a layer")) })
            }).collect();
            let o = json!({
                "width": c.width,
                "height": c.height,
                "pixelAspect": c.pixel_aspect,
                "frameRate": c.frame_rate.as_f64(),
                "frameDuration": secs(c.frame_duration()),
                "duration": secs(c.duration),
                "workAreaStart": secs(c.work_area.start),
                "workAreaDuration": secs(c.work_area.end - c.work_area.start),
                "displayStartTime": secs(c.display_start),
                "bgColor": [c.bg_color[0], c.bg_color[1], c.bg_color[2]],
                "layers": layers,
            });
            merge(&mut out, o);
        }
        "folder" => {}
        _ => {
            let num = |a: &str| query::static_value(&v, a).and_then(|x| x.as_f64());
            let duration = query::static_value(&v, "duration")
                .and_then(|x| x.as_time())
                .map_or(0.0, secs);
            let rate = match query::static_value(&v, "frame_rate") {
                Some(Value::FrameRate(r)) if r.num > 0 && r.den > 0 => r.as_f64(),
                _ => 0.0,
            };
            let color = match query::static_value(&v, "solid_color") {
                Some(Value::Color(c)) => json!([c[0], c[1], c[2]]),
                _ => Json::Null,
            };
            merge(
                &mut out,
                json!({
                    "width": num("width").unwrap_or(0.0),
                    "height": num("height").unwrap_or(0.0),
                    "pixelAspect": num("pixel_aspect").unwrap_or(1.0),
                    "duration": duration,
                    "frameRate": rate,
                    "kind": query::static_value(&v, "kind").and_then(|k| k.as_str().map(str::to_owned)),
                    "color": color,
                    "file": query::static_value(&v, "file").and_then(|f| f.as_str().map(str::to_owned)),
                }),
            );
        }
    }
    Ok(out)
}

fn merge(out: &mut Json, more: Json) {
    if let (Json::Object(a), Json::Object(b)) = (out, more) {
        a.extend(b);
    }
}

fn layer(stage: &Stage, path: &PrimPath) -> R {
    let v = view(stage, path)?;
    if !v.is_a(&PrimType::LAYER) {
        return Err(format!("{path} is not a layer"));
    }
    let comp = query::owning_comp(stage, path).ok_or("the layer is not in a comp")?;
    let t = query::layer_timing(stage, path);
    let enum_attr =
        |a: &str| query::static_value(&v, a).and_then(|x| x.as_str().map(str::to_owned));
    Ok(json!({
        "name": query::display_name(&v),
        "kind": query::layer_kind(stage, path),
        "comp": comp.as_str(),
        "inPoint": secs(t.in_point),
        "outPoint": secs(t.out_point),
        "startTime": secs(t.start_time),
        "stretch": t.stretch,
        "enabled": bool_attr(&v, "enabled", true),
        "solo": bool_attr(&v, "solo", false),
        "shy": bool_attr(&v, "shy", false),
        "locked": query::is_locked(stage, path),
        "threeD": bool_attr(&v, "three_d", false),
        "adjustment": bool_attr(&v, "adjustment_layer", false),
        "motionBlur": bool_attr(&v, "motion_blur", false),
        "label": query::static_value(&v, "label").and_then(|x| x.as_f64()).unwrap_or(0.0),
        "blendingMode": enum_attr("blend_mode"),
        "quality": enum_attr("quality"),
        "parent": prim_ref(&v, "parent"),
        "source": prim_ref(&v, "source"),
    }))
}

/// Children of `parent/<folder>` of prim type `ty`, in stack order.
/// The AE property indices from the layer down to a mask or Bezier shape path.
fn path_info(stage: &Stage, path: &PrimPath) -> R {
    let prim = view(stage, path)?;
    let mask = prim.is_a(&PrimType::MASK);
    if !mask && !prim.is_a(&PrimType::PATH) {
        return Err(format!("{path} is not a Bezier path"));
    }
    let layer = query::owning_layer(stage, path).ok_or("path has no layer")?;
    let mut indices = Vec::new();
    let mut item = path.clone();
    while item != layer {
        let folder = item.parent().ok_or("path has no property group")?;
        let index = stage
            .children(&folder)
            .iter()
            .position(|p| p == &item)
            .ok_or("missing path item")?;
        indices.push(index + 1);
        item = folder.parent().ok_or("path has no containing layer")?;
    }
    indices.reverse();
    Ok(
        json!({"layer": layer, "mask": mask, "indices": indices, "name": query::display_name(&prim)}),
    )
}

fn children_of_type(stage: &Stage, parent: &PrimPath, folder: &str, ty: &PrimType) -> Vec<Json> {
    let Ok(f) = parent.child(folder) else {
        return Vec::new();
    };
    stage
        .children(&f)
        .into_iter()
        .filter_map(|c| {
            let v = stage.prim(&c)?;
            v.is_a(ty).then(|| {
                json!({
                    "path": c.as_str(),
                    "name": query::display_name(&v),
                    "matchName": v.value("match_name").and_then(|m| m.as_str().map(str::to_owned)),
                })
            })
        })
        .collect()
}

impl RunState<'_, '_, '_> {
    fn stage(&self) -> &Stage {
        self.tx.stage()
    }

    /// Runs a registered command inside the script's transaction.
    pub fn invoke(&mut self, id: &str, params: Json) -> R {
        if self.access != crate::ProjectAccess::Write {
            return Err("this plugin requires project write permission".into());
        }
        if id == crate::RUN_SCRIPT {
            return Err("a script cannot run another script".into());
        }
        let cmd = (self.env.commands)(id).ok_or_else(|| format!("unknown command `{id}`"))?;
        // Script file grants are scoped handles. Raw-path commands cannot exercise
        // one, even when the manifest requests filesystem access.
        if cmd.requires_filesystem() {
            return Err(format!(
                "command `{id}` requires a host-granted file handle"
            ));
        }
        self.generation += 1;
        self.evaluator.clear();
        cmd.run_json(self.tx, params).map_err(|e| e.to_string())
    }

    fn key_time(&self, path: &PrimPath, comp_seconds: f64) -> Time {
        query::to_key_time(self.stage(), path, flicks(comp_seconds))
    }

    fn comp_time(&self, comp: &PrimPath) -> Time {
        self.playheads.get(comp).copied().unwrap_or_else(|| {
            if self.env.active_comp.as_ref() == Some(comp) {
                self.env.time
            } else if let Some(view) = self.env.view {
                view.time(comp)
            } else if self.env.active_comp.is_none() {
                self.env.time
            } else {
                Time::ZERO
            }
        })
    }

    fn now(&self, path: &PrimPath) -> f64 {
        query::owning_comp(self.stage(), path).map_or(0.0, |comp| secs(self.comp_time(&comp)))
    }

    /// The effect parameter definition behind `param:NNNN` on an effect prim.
    fn effect_default(&self, path: &PrimPath, attr: &str) -> Option<Value> {
        let id = bae_model::effect::param_id(attr)?;
        let v = self.stage().prim(path)?;
        let match_name = v.value("match_name")?.as_str()?.to_owned();
        let def = (self.env.effects)(&match_name)?;
        let p = def.param(id)?;
        p.default
            .clone()
            .or_else(|| p.kind.value_type().and_then(zero_of))
    }

    /// The value of `attr` at comp time `t` (keyframes, no expressions).
    fn sample(&self, path: &PrimPath, attr: &str, t: f64) -> Option<Value> {
        let kt = self.key_time(path, t);
        query::sample_attr(self.stage(), path, attr, kt).or_else(|| self.effect_default(path, attr))
    }

    fn keys(&self, path: &PrimPath, attr: &str) -> Vec<Time> {
        query::attr_spec(self.stage(), path, attr)
            .map(|s| s.keyframes.keys().copied().collect())
            .unwrap_or_default()
    }

    fn prop(&self, path: &PrimPath, attr: &str) -> R {
        let stage = self.stage();
        view(stage, path)?;
        let spec = query::attr_spec(stage, path, attr);
        let keys: Vec<Json> = spec
            .map(|s| {
                s.keyframes
                    .iter()
                    .map(|(t, k)| {
                        json!({
                            "time": secs(query::to_comp_time(stage, path, *t)),
                            "value": to_js(&k.value),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let expr = spec.and_then(|s| s.expression.as_ref());
        Ok(json!({
            "keys": keys,
            "expression": expr.map_or("", |e| e.source.as_str()),
            "expressionEnabled": expr.is_some_and(|e| e.enabled),
        }))
    }

    /// `setValue` (`key` false) or `setValueAtTime` (`key` true).
    fn set(&mut self, args: &Json) -> R {
        let path = path(args, "path")?;
        let attr = string(args, "attr")?;
        let key = args.get("key").and_then(Json::as_bool).unwrap_or(false);
        let value = arg(args, "value")?;
        view(self.stage(), &path)?;
        let t = seconds(args, "time").unwrap_or_else(|| self.now(&path));
        let kt = self.key_time(&path, t);
        let animated =
            query::attr_spec(self.stage(), &path, &attr).is_some_and(|s| s.is_animated());
        if !key && animated {
            return Err(
                "setValue cannot be used on a property that has keyframes; use setValueAtTime"
                    .into(),
            );
        }
        if attr == "text:source" && value.is_string() && !key {
            // Characters only, the way the Type tool edits them.
            self.invoke("text.set_text", json!({ "layer": path, "text": value }))?;
        } else {
            let template = self.sample(&path, &attr, t);
            let v = from_js(template.as_ref(), value)?;
            self.invoke(
                "attr.set_at",
                json!({ "path": path, "attr": attr, "time": kt, "value": v }),
            )?;
        }
        if key && !animated {
            // The static value is in place: the stopwatch keys it at `kt`.
            self.invoke(
                "attr.set_animated",
                json!({ "path": path, "attr": attr, "animated": true, "time": kt }),
            )?;
        }
        Ok(Json::Null)
    }

    fn add_key(&mut self, args: &Json) -> R {
        let path = path(args, "path")?;
        let attr = string(args, "attr")?;
        let t = seconds(args, "time").unwrap_or_else(|| self.now(&path));
        let kt = self.key_time(&path, t);
        let animated =
            query::attr_spec(self.stage(), &path, &attr).is_some_and(|s| s.is_animated());
        if animated {
            let v = self
                .sample(&path, &attr, t)
                .ok_or("the property has no value")?;
            self.invoke(
                "attr.set_at",
                json!({ "path": path, "attr": attr, "time": kt, "value": v }),
            )?;
        } else {
            if query::attr_spec(self.stage(), &path, &attr).is_none_or(|s| s.value.is_none()) {
                // Write the default first so the stopwatch has a value to key.
                if let Some(v) = self.sample(&path, &attr, t) {
                    self.invoke(
                        "attr.set_at",
                        json!({ "path": path, "attr": attr, "time": kt, "value": v }),
                    )?;
                }
            }
            self.invoke(
                "attr.set_animated",
                json!({ "path": path, "attr": attr, "animated": true, "time": kt }),
            )?;
        }
        let index = self.keys(&path, &attr).iter().position(|k| *k == kt);
        Ok(json!(index.map_or(0, |i| i + 1)))
    }

    fn key_at(&self, args: &Json) -> Result<(PrimPath, String, Time), String> {
        let path = path(args, "path")?;
        let attr = string(args, "attr")?;
        let index = arg(args, "index")?.as_u64().unwrap_or(0) as usize;
        let keys = self.keys(&path, &attr);
        let t = index
            .checked_sub(1)
            .and_then(|i| keys.get(i))
            .copied()
            .ok_or_else(|| format!("keyframe index {index} is out of range (1–{})", keys.len()))?;
        Ok((path, attr, t))
    }

    fn set_layer(&mut self, args: &Json) -> R {
        let path = path(args, "path")?;
        let field = string(args, "field")?;
        let value = args.get("value").cloned().unwrap_or(Json::Null);
        let switch = |attr: &str| json!({ "layers": [&path], "attr": attr, "value": {"type": "bool", "value": value.as_bool().unwrap_or(false)} });
        match field.as_str() {
            "name" => self.invoke(
                "attr.set",
                json!({ "path": path, "attr": "name", "value": {"type": "string", "value": value.as_str().unwrap_or_default()} }),
            ),
            "enabled" | "solo" | "shy" | "locked" => self.invoke("layer.set_attr", switch(&field)),
            "threeD" => self.invoke("layer.set_attr", switch("three_d")),
            "adjustment" => self.invoke("layer.set_attr", switch("adjustment_layer")),
            "motionBlur" => self.invoke("layer.set_attr", switch("motion_blur")),
            "label" => self.invoke(
                "layer.set_attr",
                json!({ "layers": [&path], "attr": "label", "value": {"type": "int", "value": value.as_f64().unwrap_or(0.0) as i64} }),
            ),
            "blendingMode" => self.invoke(
                "layer.set_attr",
                json!({ "layers": [&path], "attr": "blend_mode", "value": {"type": "enum", "value": value.as_str().unwrap_or("normal")} }),
            ),
            "inPoint" | "outPoint" => {
                let t = flicks(value.as_f64().ok_or("expected seconds")?);
                let key = if field == "inPoint" { "in_point" } else { "out_point" };
                self.invoke("layer.trim", json!({ "layers": [&path], key: t }))
            }
            "startTime" => {
                let old = query::layer_timing(self.stage(), &path).start_time;
                let t = flicks(value.as_f64().ok_or("expected seconds")?);
                self.invoke(
                    "layer.shift_time",
                    json!({ "layers": [&path], "delta": Time(t.0 - old.0) }),
                )
            }
            "parent" | "parentJump" => {
                let time = flicks(self.now(&path));
                self.invoke(
                    "layer.set_parent",
                    json!({ "layers": [&path], "parent": value, "keep_transform": field == "parent", "time": time }),
                )
            }
            "selected" => {
                self.generation += 1;
                self.selected.retain(|l| *l != path);
                if value.as_bool() == Some(true) {
                    self.selected.push(path);
                }
                Ok(Json::Null)
            }
            other => Err(format!("layer.{other} cannot be set from a script")),
        }
    }

    fn add_layer(&mut self, args: &Json) -> R {
        let comp = path(args, "comp")?;
        let mut params = json!({ "comp": comp, "kind": string(args, "kind")? });
        for (from, to) in [
            ("name", "name"),
            ("text", "text"),
            ("pixelAspect", "pixel_aspect"),
            ("source", "source"),
        ] {
            if let Some(v) = args.get(from).filter(|v| !v.is_null()) {
                params[to] = v.clone();
            }
        }
        if let Some(c) = args.get("color").and_then(Json::as_array) {
            let n: Vec<f64> = c.iter().filter_map(Json::as_f64).collect();
            if let [r, g, b, ..] = n.as_slice() {
                params["color"] = json!([r, g, b, n.get(3).copied().unwrap_or(1.0)]);
            }
        }
        if let (Some(w), Some(h)) = (
            args.get("width").and_then(Json::as_f64),
            args.get("height").and_then(Json::as_f64),
        ) {
            params["size"] = json!([w.round().max(1.0) as u32, h.round().max(1.0) as u32]);
        }
        let out = self.invoke("layer.create", params)?;
        let layer = out
            .get("path")
            .and_then(Json::as_str)
            .ok_or("layer.create returned no path")?
            .to_owned();
        if let Some(d) = seconds(args, "duration").filter(|d| *d > 0.0) {
            let lp = PrimPath::new(&layer).map_err(|e| e.to_string())?;
            let start = query::layer_timing(self.stage(), &lp).in_point;
            self.invoke(
                "layer.trim",
                json!({ "layers": [&lp], "out_point": Time(start.0 + flicks(d).0) }),
            )?;
        }
        Ok(json!(layer))
    }

    fn set_comp(&mut self, args: &Json) -> R {
        let comp = path(args, "path")?;
        let field = string(args, "field")?;
        let value = arg(args, "value")?;
        let mut params = json!({ "comp": comp });
        match field.as_str() {
            "name" => params["name"] = value.clone(),
            "width" | "height" => {
                params[field.as_str()] = json!(value.as_f64().unwrap_or(1.0).round() as u32)
            }
            "pixelAspect" => params["pixel_aspect"] = value.clone(),
            "frameRate" => params["fps"] = value.clone(),
            "time" => {
                if self.access != crate::ProjectAccess::Write {
                    return Err("moving the playhead requires project write permission".into());
                }
                let info = query::comp_info(self.stage(), &comp).ok_or("expected composition")?;
                let time = info.playhead_time(flicks(value.as_f64().ok_or("expected seconds")?));
                if let Some(view) = self.env.view {
                    view.set_time(&info, time);
                }
                self.playheads.insert(comp, time);
                return Ok(Json::Null);
            }
            "duration" => {
                params["duration"] = json!(flicks(value.as_f64().ok_or("expected seconds")?))
            }
            "bgColor" => {
                let n: Vec<f64> = value
                    .as_array()
                    .map(|a| a.iter().filter_map(Json::as_f64).collect())
                    .unwrap_or_default();
                let [r, g, b] = [0, 1, 2].map(|i| n.get(i).copied().unwrap_or(0.0));
                params["bg_color"] = json!([r, g, b, 1.0]);
            }
            other => return Err(format!("comp.{other} cannot be set from a script")),
        }
        self.invoke("comp.set_settings", params)
    }

    fn effect(&self, path: &PrimPath) -> R {
        let stage = self.stage();
        let v = view(stage, path)?;
        let match_name = v
            .value("match_name")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_owned();
        let params: Vec<Json> = match (self.env.effects)(&match_name) {
            Some(def) => def
                .params
                .iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "matchName": p.match_name,
                        "attr": bae_model::effect::param_attr(p.id),
                    })
                })
                .collect(),
            None => {
                let mut attrs: Vec<&str> = v
                    .attr_names()
                    .into_iter()
                    .filter(|a| a.starts_with("param:"))
                    .collect();
                attrs.sort_unstable();
                attrs
                    .into_iter()
                    .map(|a| json!({ "name": a, "matchName": Json::Null, "attr": a }))
                    .collect()
            }
        };
        Ok(json!({
            "name": query::display_name(&v),
            "matchName": match_name,
            "enabled": bool_attr(&v, "enabled", true),
            "params": params,
        }))
    }
}

/// Answers one `__host(method, args)` call.
pub(crate) fn call(s: &mut RunState<'_, '_, '_>, method: &str, args: &Json) -> R {
    if s.access == crate::ProjectAccess::None
        && !matches!(method, "log" | "alert" | "undoGroup" | "blendModes")
    {
        return Err("this plugin has no project access".into());
    }
    match method {
        "blendModes" => Ok(json!(bae_model::schema::BLEND_MODES)),
        "context" => Ok(json!({
            "activeComp": s.env.active_comp.as_ref().map(PrimPath::as_str),
            "selected": s.selected.iter().map(PrimPath::as_str).collect::<Vec<_>>(),
            "version": env!("CARGO_PKG_VERSION"),
        })),
        "log" | "alert" => {
            const MAX_ENTRIES: usize = 256;
            const MAX_BYTES: usize = 64 * 1024;
            if s.report.log.len() + s.report.alerts.len() < MAX_ENTRIES && s.output_bytes < MAX_BYTES {
                let text = args.get("text").and_then(Json::as_str).unwrap_or_default();
                let text = &text[..text.floor_char_boundary((MAX_BYTES - s.output_bytes).min(text.len()))];
                s.output_bytes += text.len();
                if method == "alert" {
                    s.report.alerts.push(text.to_owned());
                } else {
                    tracing::info!(target: "script", "{text}");
                    s.report.log.push(text.to_owned());
                }
            }
            Ok(Json::Null)
        }
        "undoGroup" => {
            if s.report.undo_group.is_none() {
                s.report.undo_group = Some(string(args, "name").unwrap_or_default());
            }
            Ok(Json::Null)
        }
        "items" => Ok(Json::Array(items(s.stage()))),
        "item" => item(s.stage(), &path(args, "path")?),
        "pathInfo" => path_info(s.stage(), &path(args, "path")?),
        "copyLayersToComp" => {
            let layers: Vec<PrimPath> = serde_json::from_value(arg(args, "layers")?.clone())
                .map_err(|e| e.to_string())?;
            let comp = path(args, "comp")?;
            if !view(s.stage(), &comp)?.is_a(&PrimType::COMP)
                || layers.iter().any(|p| !query::is_layer(s.stage(), p)) {
                return Err("expected layers and a destination composition".into());
            }
            let copied = bae_motion::layer::copy_layers(s.stage(), &layers);
            let out = s.invoke("layer.paste", json!({"layers": copied, "comp":comp}))?;
            Ok(out["paths"].clone())
        }
        "layer" => layer(s.stage(), &path(args, "path")?),
        "prop" => s.prop(&path(args, "path")?, &string(args, "attr")?),
        "sample" => {
            let p = path(args, "path")?;
            view(s.stage(), &p)?;
            let t = seconds(args, "time").unwrap_or_else(|| s.now(&p));
            let attr = string(args, "attr")?;
            let value = if args["preExpression"].as_bool() == Some(true) {
                s.sample(&p, &attr, t)
            } else {
                s.evaluator.evaluate_attribute(s.tx.stage(), &p, &attr, flicks(t))
                    .ok().or_else(|| s.effect_default(&p, &attr))
            };
            Ok(value.map_or(Json::Null, |v| to_js(&v)))
        }
        "effects" => Ok(Json::Array(children_of_type(
            s.stage(),
            &path(args, "layer")?,
            "effects",
            &PrimType::EFFECT,
        ))),
        "masks" => Ok(Json::Array(children_of_type(
            s.stage(),
            &path(args, "layer")?,
            "masks",
            &PrimType::MASK,
        ))),
        "effect" => s.effect(&path(args, "path")?),
        "set" => s.set(args),
        "addKey" => s.add_key(args),
        "removeKey" => {
            let (p, attr, t) = s.key_at(args)?;
            s.invoke(
                "key.delete",
                json!({ "keys": [{ "path": p, "attr": attr, "time": t }] }),
            )
        }
        "setInterpolation" => {
            let (p, attr, t) = s.key_at(args)?;
            s.invoke(
                "key.set_interpolation",
                json!({
                    "keys": [{ "path": p, "attr": attr, "time": t }],
                    "interp_in": args.get("in"),
                    "interp_out": args.get("out"),
                }),
            )
        }
        "setExpression" => {
            let source = args
                .get("source")
                .and_then(Json::as_str)
                .filter(|s| !s.is_empty());
            s.invoke(
                "attr.set_expression",
                json!({
                    "path": path(args, "path")?,
                    "attr": string(args, "attr")?,
                    "source": source,
                    "enabled": args.get("enabled").and_then(Json::as_bool).unwrap_or(true),
                }),
            )
        }
        "setLayer" => s.set_layer(args),
        "compTime" => Ok(json!(secs(s.comp_time(&path(args, "path")?)))),
        "setComp" => s.set_comp(args),
        "rename" => s.invoke(
            "attr.set",
            json!({ "path": path(args, "path")?, "attr": "name", "value": {"type": "string", "value": string(args, "name")?} }),
        ),
        "addLayer" => s.add_layer(args),
        "addComp" => {
            let mut params = json!({});
            for (from, to) in [("name", "name"), ("pixelAspect", "pixel_aspect"), ("frameRate", "fps"), ("parent", "parent")] {
                if let Some(v) = args.get(from).filter(|v| !v.is_null()) {
                    params[to] = v.clone();
                }
            }
            for key in ["width", "height"] {
                if let Some(v) = args.get(key).and_then(Json::as_f64) {
                    params[key] = json!(v.round().max(1.0) as u32);
                }
            }
            if let Some(d) = seconds(args, "duration") {
                params["duration"] = json!(flicks(d));
            }
            let out = s.invoke("comp.create", params)?;
            Ok(out.get("path").cloned().unwrap_or(Json::Null))
        }
        "addFolder" => {
            let mut params = json!({ "name": args.get("name") });
            if let Some(p) = args.get("parent").filter(|v| !v.is_null()) {
                params["parent"] = p.clone();
            }
            let out = s.invoke("folder.create", params)?;
            Ok(out.get("path").cloned().unwrap_or(Json::Null))
        }
        "deleteItem" => s.invoke("item.delete", json!({ "paths": [path(args, "path")?] })),
        "deleteLayer" => {
            let layer = path(args, "path")?;
            let time = flicks(s.now(&layer));
            s.invoke(
                "layer.delete",
                json!({ "layers": [layer], "time": time }),
            )
        }
        "duplicateLayer" => {
            let out = s.invoke("layer.duplicate", json!({ "layers": [path(args, "path")?] }))?;
            Ok(out["paths"].get(0).cloned().unwrap_or(Json::Null))
        }
        "moveLayer" => s.invoke(
            "layer.reorder",
            json!({ "layers": [path(args, "path")?], "index": arg(args, "index")?.as_u64().unwrap_or(0) }),
        ),
        "applyEffect" => {
            let name = string(args, "matchName")?;
            let def = (s.env.effects)(&name).ok_or_else(|| format!("unknown effect `{name}`"))?;
            let out = s.invoke(
                "effect.apply",
                json!({ "layers": [path(args, "layer")?], "match_name": def.match_name }),
            )?;
            Ok(out["effects"].get(0).cloned().unwrap_or(Json::Null))
        }
        "removeEffect" => s.invoke("effect.remove", json!({ "effects": [path(args, "path")?] })),
        "setEffectEnabled" => s.invoke(
            "effect.set_enabled",
            json!({ "effects": [path(args, "path")?], "enabled": args.get("value").and_then(Json::as_bool).unwrap_or(true) }),
        ),
        "invoke" => {
            let id = string(args, "id")?;
            let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
            s.invoke(&id, params)
        }
        other => Err(format!("unknown host method `{other}`")),
    }
}
