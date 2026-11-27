//! Read-only views of a stage written for agents: `stage.describe` (a compact text
//! summary), `prim.get`, `prim.find` and `sublayer.list` (JSON).

use std::fmt::Write as _;

use bae_model::{
    AttrSpec, FrameRate, PrimPath, PrimType, PrimView, Stage, SublayerId, Time, Value, schema,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::session::AgentSession;

// ---------------------------------------------------------------------------
// Comp settings

/// The settings of a `Comp` prim (authored values, else schema defaults).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub duration: Time,
}

impl CompSettings {
    pub fn frame_count(&self) -> i64 {
        self.frame_rate.time_to_frame(self.duration).max(1)
    }
}

/// A prim's static value for `attr`, falling back to its schema default.
pub fn value_or_default(view: &PrimView<'_>, attr: &str) -> Option<Value> {
    view.value(attr).cloned().or_else(|| {
        let ty = view.prim_type()?;
        schema::attr_def(ty, attr)?.default.to_value()
    })
}

/// Settings of the comp at `path`; `None` if it is not a Comp.
pub fn comp_settings(stage: &Stage, path: &PrimPath) -> Option<CompSettings> {
    let view = stage.prim(path)?;
    if !view.is_a(&PrimType::COMP) {
        return None;
    }
    let dim = |attr: &str, fallback: u32| match value_or_default(&view, attr) {
        Some(Value::Int(v)) => u32::try_from(v).unwrap_or(fallback).max(1),
        Some(Value::Float(v)) if v >= 1.0 => v as u32,
        _ => fallback,
    };
    let frame_rate = match value_or_default(&view, "frame_rate") {
        Some(Value::FrameRate(r)) if r.num > 0 && r.den > 0 => r,
        _ => FrameRate::FPS_30,
    };
    let duration = match value_or_default(&view, "duration") {
        Some(Value::Time(t)) if t > Time::ZERO => t,
        _ => Time::from_seconds(10.0),
    };
    Some(CompSettings {
        width: dim("width", 1920),
        height: dim("height", 1080),
        frame_rate,
        duration,
    })
}

/// The nearest `Comp` at or above `path`.
pub fn enclosing_comp(stage: &Stage, path: &PrimPath) -> Option<PrimPath> {
    std::iter::once(path.clone())
        .chain(path.ancestors())
        .find(|p| stage.prim(p).is_some_and(|v| v.is_a(&PrimType::COMP)))
}

fn fps_label(r: FrameRate) -> String {
    let fps = r.as_f64();
    if r.den == 1 {
        format!("{fps}fps")
    } else {
        format!("{fps:.3}fps")
    }
}

// ---------------------------------------------------------------------------
// stage.describe

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    /// Prim tree with types, layer kinds and comp settings.
    Brief,
    /// Also each prim's authored attributes (static values, keyframe counts and ranges,
    /// expressions).
    #[default]
    Normal,
    /// Also the sublayer every attribute value comes from.
    Full,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct DescribeParams {
    /// Describe a named version or automatic snapshot (ID from history_list) instead of the current project.
    #[serde(default)]
    pub checkpoint: Option<String>,
    /// Prim to summarize (default "/", the whole project), e.g. /comps/main.
    #[serde(default)]
    pub root: Option<PrimPath>,
    /// brief | normal (default) | full.
    #[serde(default)]
    pub detail: Detail,
    /// Maximum length of the summary in characters (default 8000, at most 100000).
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// Who is asking, for the header of the summary.
#[derive(Clone, Debug, Default)]
pub struct DescribeContext {
    /// Where the project lives, if saved.
    pub location: Option<String>,
    /// The asking agent (identity, permission, edit target).
    pub agent: Option<AgentContext>,
}

#[derive(Clone, Debug)]
pub struct AgentContext {
    pub name: String,
    pub author: String,
    pub permission: String,
    pub edit_target: SublayerId,
}

impl AgentContext {
    pub fn of(session: &AgentSession) -> Self {
        AgentContext {
            name: session.identity().name().to_owned(),
            author: session.identity().author().to_string(),
            permission: session.permission().to_string(),
            edit_target: session.edit_target().clone(),
        }
    }
}

const DEFAULT_MAX_CHARS: usize = 8000;
const MAX_MAX_CHARS: usize = 100_000;

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn nums(v: &[f64]) -> String {
    let parts: Vec<String> = v.iter().map(|x| num(*x)).collect();
    format!("[{}]", parts.join(","))
}

/// A short human-readable rendering of a value (for summaries, not round-tripping).
pub fn format_value(v: &Value, rate: Option<FrameRate>) -> String {
    match v {
        Value::Float(x) => num(*x),
        Value::Int(x) => x.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => format!("{:?}", short(s, 40)),
        Value::Vec2(a) => nums(a),
        Value::Vec3(a) => nums(a),
        Value::Color(a) => format!("rgba{}", nums(a)),
        Value::Enum(s) => s.clone(),
        Value::AssetRef(p) | Value::PrimRef(p) => format!("->{p}"),
        Value::Time(t) => match rate {
            Some(r) => format!("f{}", r.time_to_frame(*t)),
            None => format!("{}s", num(t.as_seconds())),
        },
        Value::FrameRate(r) => fps_label(*r),
        Value::TextDocument(doc) => format!("{:?}", short(&doc.text, 40)),
        Value::Bezier(b) => format!("path({} pts)", b.vertices.len()),
        Value::Bytes(b) => format!("bytes({})", b.len()),
        Value::FloatList(l) => format!("list({})", l.len()),
    }
}

fn frame_of(t: Time, rate: Option<FrameRate>) -> String {
    match rate {
        Some(r) => format!("f{}", r.time_to_frame(t)),
        None => format!("{}s", num(t.as_seconds())),
    }
}

fn attr_summary(spec: &AttrSpec, rate: Option<FrameRate>) -> String {
    let mut out = String::new();
    if spec.is_animated() {
        let first = spec.keyframes.keys().next().copied().unwrap_or(Time::ZERO);
        let last = spec
            .keyframes
            .keys()
            .next_back()
            .copied()
            .unwrap_or(Time::ZERO);
        let n = spec.keyframes.len();
        let _ = write!(
            out,
            "{n} key{} {}..{}",
            if n == 1 { "" } else { "s" },
            frame_of(first, rate),
            frame_of(last, rate)
        );
    } else if let Some(v) = &spec.value {
        out.push_str(&format_value(v, rate));
    }
    if let Some(e) = &spec.expression {
        if !out.is_empty() {
            out.push(' ');
        }
        let state = if e.enabled { "" } else { " (disabled)" };
        let _ = write!(out, "expr{state} {:?}", short(&e.source, 40));
    }
    out
}

/// One line describing a prim (without indentation).
fn prim_line(
    stage: &Stage,
    view: &PrimView<'_>,
    detail: Detail,
    rate: Option<FrameRate>,
) -> String {
    let mut line = String::new();
    let ty = view.prim_type().map_or("?", |t| t.as_str());
    let _ = write!(line, "{} ({ty}", view.name());
    if view.is_a(&PrimType::LAYER)
        && let Some(Value::Enum(kind)) = view.value("kind")
    {
        let _ = write!(line, " {kind}");
    }
    line.push(')');
    if let Some(Value::String(name)) = view.value("name")
        && name != view.name()
    {
        let _ = write!(line, " {:?}", short(name, 40));
    }
    if let Some(c) = comp_settings(stage, view.path()) {
        let _ = write!(
            line,
            " {}x{} {} {}s ({} frames)",
            c.width,
            c.height,
            fps_label(c.frame_rate),
            num(c.duration.as_seconds()),
            c.frame_count()
        );
    }
    if view.value("locked") == Some(&Value::Bool(true)) {
        line.push_str(" LOCKED");
    }
    if !view.references().is_empty() {
        let refs: Vec<String> = view
            .references()
            .iter()
            .map(|r| match &r.project {
                Some(p) => format!("{p}:{}", r.prim),
                None => r.prim.to_string(),
            })
            .collect();
        let _ = write!(line, " refs[{}]", refs.join(", "));
    }
    if detail == Detail::Brief {
        return line;
    }
    let skip = ["name", "kind"];
    let attrs: Vec<String> = view
        .attr_names()
        .into_iter()
        .filter(|a| !skip.contains(a))
        .filter_map(|a| {
            let spec = view.attr(a)?;
            let mut s = format!("{a}: {}", attr_summary(spec, rate));
            if detail == Detail::Full
                && let Some(src) = view.attr_source(a)
            {
                let _ = write!(s, " @{src}");
            }
            Some(s)
        })
        .collect();
    if !attrs.is_empty() {
        let _ = write!(line, " | {}", attrs.join("; "));
    }
    line
}

/// The `stage.describe` summary.
pub fn describe_stage(
    stage: &Stage,
    params: &DescribeParams,
    ctx: &DescribeContext,
) -> Result<String, bae_model::ModelError> {
    let root = params.root.clone().unwrap_or_else(PrimPath::root);
    if !root.is_root() {
        stage.require_prim(&root)?;
    }
    let max = params
        .max_chars
        .unwrap_or(DEFAULT_MAX_CHARS)
        .clamp(200, MAX_MAX_CHARS);

    let mut out = String::new();
    let _ = writeln!(
        out,
        "BAE project: {}",
        ctx.location.as_deref().unwrap_or("(unsaved)")
    );
    if let Some(a) = &ctx.agent {
        let exists = stage.sublayer(&a.edit_target).is_some();
        let _ = writeln!(
            out,
            "You: agent {} (author {}), permission {}; edits go to sublayer {}{}",
            a.name,
            a.author,
            a.permission,
            a.edit_target,
            if exists {
                ""
            } else {
                " (created on first edit)"
            }
        );
    }
    let stack: Vec<String> = stage
        .sublayers()
        .map(|l| format!("{} [{} specs]", l.id(), l.len()))
        .collect();
    let _ = writeln!(out, "Layer stack (strongest first): {}", stack.join(", "));
    let _ = writeln!(
        out,
        "Times: comps in comp frames; layer keyframes in layer frames (f = frame)."
    );
    out.push('\n');

    // Depth-first over the composed stage.
    let mut lines_total = 0usize;
    let mut omitted = 0usize;
    let mut stack: Vec<(PrimPath, usize)> = if root.is_root() {
        stage
            .children(&root)
            .into_iter()
            .rev()
            .map(|p| (p, 0))
            .collect()
    } else {
        vec![(root.clone(), 0)]
    };
    while let Some((path, depth)) = stack.pop() {
        let Some(view) = stage.prim(&path) else {
            continue;
        };
        lines_total += 1;
        let rate = enclosing_comp(stage, &path)
            .and_then(|c| comp_settings(stage, &c).map(|s| s.frame_rate));
        let line = format!(
            "{}{}\n",
            "  ".repeat(depth),
            prim_line(stage, &view, params.detail, rate)
        );
        if omitted > 0 || out.len() + line.len() > max {
            omitted += 1;
        } else {
            out.push_str(&line);
        }
        for child in view.children().into_iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    if lines_total == 0 {
        out.push_str("(no prims yet)\n");
    }
    if omitted > 0 {
        let _ = writeln!(
            out,
            "… {omitted} more prims not shown. Narrow with root=<path>, detail=brief, or read one prim with prim_get."
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// prim.get

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct PrimGetParams {
    /// Prim path, e.g. /comps/main/layers/title.
    pub path: PrimPath,
    /// Read the prim at a named version or automatic snapshot (ID from history_list) instead of now.
    #[serde(default)]
    pub checkpoint: Option<String>,
    /// Also list schema attributes that are not authored, with their defaults.
    #[serde(default)]
    pub include_defaults: bool,
}

/// The `prim.get` result: type, references, every authored attribute with the
/// sublayer it comes from (and which sublayers it overrides), and children.
pub fn prim_json(stage: &Stage, params: &PrimGetParams) -> Result<Json, bae_model::ModelError> {
    let view = stage.require_prim(&params.path)?;
    let comp = enclosing_comp(stage, &params.path);
    let settings = comp.as_ref().and_then(|c| comp_settings(stage, c));
    let rate = settings.map(|s| s.frame_rate);

    let mut attrs = serde_json::Map::new();
    for name in view.attr_names() {
        let Some(spec) = view.attr(name) else {
            continue;
        };
        let mut entry = serde_json::to_value(spec).unwrap_or(Json::Null);
        if let Json::Object(obj) = &mut entry {
            if let Some(src) = view.attr_source(name) {
                obj.insert("source".into(), json!(src));
            }
            let weaker: Vec<&SublayerId> = view
                .opinions()
                .iter()
                .filter(|o| o.spec.attrs.contains_key(name))
                .map(|o| o.sublayer())
                .skip(1)
                .collect();
            if !weaker.is_empty() {
                obj.insert("overrides".into(), json!(weaker));
            }
            if let (Some(r), true) = (rate, spec.is_animated()) {
                let frames: Vec<i64> = spec.keyframes.keys().map(|t| r.time_to_frame(*t)).collect();
                obj.insert("key_frames".into(), json!(frames));
            }
            if let Some(ty) = view.prim_type()
                && let Some(def) = schema::attr_def(ty, name)
            {
                obj.insert("value_type".into(), json!(def.value_type));
            }
        }
        attrs.insert(name.to_owned(), entry);
    }

    let opinions: Vec<Json> = view
        .opinions()
        .iter()
        .map(|o| {
            let mut j = json!({
                "sublayer": o.sublayer(),
                "specifier": o.spec.specifier,
            });
            if let Some(p) = o.project {
                j["project"] = json!(p);
            }
            if o.path != params.path {
                j["via_reference"] = json!(o.path);
            }
            j
        })
        .collect();

    let mut out = json!({
        "path": params.path,
        "type": view.prim_type(),
        "opinions": opinions,
        "attributes": attrs,
        "children": view.children().iter().map(|p| p.name().to_owned()).collect::<Vec<_>>(),
    });
    if !view.references().is_empty() {
        out["references"] = json!(view.references());
    }
    if let (Some(c), Some(s)) = (&comp, settings) {
        out["comp"] = json!({
            "path": c,
            "width": s.width,
            "height": s.height,
            "frame_rate": s.frame_rate,
            "fps": s.frame_rate.as_f64(),
            "duration_frames": s.frame_count(),
        });
    }
    if params.include_defaults
        && let Some(ty) = view.prim_type()
    {
        let kind = match view.value("kind") {
            Some(Value::Enum(k)) => Some(k.clone()),
            _ => schema::attr_def(ty, "kind")
                .and_then(|d| d.default.to_value())
                .and_then(|v| v.as_str().map(str::to_owned)),
        };
        let defaults: serde_json::Map<String, Json> = schema::attr_defs(ty)
            .into_iter()
            .filter(|d| !attrs.contains_key(d.name))
            .filter(|d| d.kinds.is_empty() || kind.as_deref().is_some_and(|k| d.kinds.contains(&k)))
            .map(|d| {
                (
                    d.name.to_owned(),
                    json!({
                        "value_type": d.value_type,
                        "default": d.default.to_value(),
                        "animatable": d.animatable,
                    }),
                )
            })
            .collect();
        out["defaults"] = Json::Object(defaults);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// prim.find

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct FindParams {
    /// Prim type, e.g. Comp, Layer, Effect, Mask, ShapeGroup.
    #[serde(default)]
    pub prim_type: Option<String>,
    /// Layer kind (with prim_type Layer or alone), e.g. text, shape, solid, null.
    #[serde(default)]
    pub kind: Option<String>,
    /// Glob on the prim name or its display name (`name` attribute); `*` and `?`.
    #[serde(default)]
    pub name: Option<String>,
    /// Search below this prim (default "/").
    #[serde(default)]
    pub under: Option<PrimPath>,
    /// Maximum number of results (default 100, at most 1000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Case-insensitive glob with `*` and `?`.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

pub fn find_prims(stage: &Stage, params: &FindParams) -> Result<Json, bae_model::ModelError> {
    let under = params.under.clone().unwrap_or_else(PrimPath::root);
    if !under.is_root() {
        stage.require_prim(&under)?;
    }
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let mut matches = Vec::new();
    let mut truncated = false;
    let mut stack: Vec<PrimPath> = stage.children(&under).into_iter().rev().collect();
    while let Some(path) = stack.pop() {
        let Some(view) = stage.prim(&path) else {
            continue;
        };
        stack.extend(view.children().into_iter().rev());
        let ty = view.prim_type();
        if let Some(want) = &params.prim_type
            && !ty.is_some_and(|t| t.as_str().eq_ignore_ascii_case(want))
        {
            continue;
        }
        let kind = match view.value("kind") {
            Some(Value::Enum(k)) => Some(k.as_str()),
            _ => None,
        };
        if let Some(want) = &params.kind
            && kind != Some(want.as_str())
        {
            continue;
        }
        let display = view.value("name").and_then(Value::as_str);
        if let Some(pat) = &params.name
            && !glob_match(pat, view.name())
            && !display.is_some_and(|d| glob_match(pat, d))
        {
            continue;
        }
        if matches.len() == limit {
            truncated = true;
            break;
        }
        let mut m = json!({ "path": path, "type": ty });
        if let Some(k) = kind {
            m["kind"] = json!(k);
        }
        if let Some(d) = display {
            m["name"] = json!(d);
        }
        matches.push(m);
    }
    Ok(json!({ "matches": matches, "truncated": truncated }))
}

// ---------------------------------------------------------------------------
// sublayer.list

pub fn sublayer_list(stage: &Stage, session: Option<&AgentSession>) -> Json {
    let layers: Vec<Json> = stage
        .sublayers()
        .map(|l| {
            let mut j = json!({
                "id": l.id(),
                "specs": l.len(),
                "proposal": crate::proposal::is_proposal_layer(l.id()),
            });
            if let Some(s) = session {
                j["yours"] = json!(s.identity().owns(l.id()));
                j["writable"] = json!(s.may_write(l.id()));
                j["edit_target"] = json!(s.edit_target() == l.id());
            }
            j
        })
        .collect();
    let mut out = json!({ "layer_stack": layers });
    if let Some(s) = session {
        out["you"] = json!({
            "agent": s.identity().name(),
            "author": s.identity().author(),
            "permission": s.permission(),
            "proposal_layer": s.identity().proposal_layer(),
            "edit_target": s.edit_target(),
            "edit_target_exists": stage.sublayer(s.edit_target()).is_some(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use bae_model::{AuthorId, CommandRegistry};

    use super::*;

    fn stage() -> Stage {
        let mut stage = Stage::new();
        let reg = CommandRegistry::with_builtins();
        let user = AuthorId::new("user:felix").unwrap();
        let mut run = |id: &str, p: Json| {
            reg.execute(&mut stage, &user, id, p).unwrap();
        };
        run(
            "prim.create",
            json!({"path": "/comps", "prim_type": "Folder"}),
        );
        run(
            "prim.create",
            json!({"path": "/comps/main", "prim_type": "Comp"}),
        );
        run(
            "attr.set",
            json!({"path": "/comps/main", "attr": "frame_rate", "value": {"type": "frame_rate", "value": {"num": 25, "den": 1}}}),
        );
        run(
            "prim.create",
            json!({"path": "/comps/main/layers", "prim_type": "Folder"}),
        );
        run(
            "prim.create",
            json!({"path": "/comps/main/layers/title", "prim_type": "Layer"}),
        );
        run(
            "attr.set",
            json!({"path": "/comps/main/layers/title", "attr": "kind", "value": {"type": "enum", "value": "text"}}),
        );
        run(
            "attr.set",
            json!({"path": "/comps/main/layers/title", "attr": "name", "value": {"type": "string", "value": "Main Title"}}),
        );
        for (frame, v) in [(0, 0.0), (12, 100.0)] {
            run(
                "key.set",
                json!({
                    "path": "/comps/main/layers/title",
                    "attr": "transform:opacity",
                    "time": frame * Time::FLICKS_PER_SECOND / 25,
                    "keyframe": {"value": {"type": "float", "value": v}},
                }),
            );
        }
        stage
    }

    #[test]
    fn describe_summarizes_tree_and_animation() {
        let stage = stage();
        let text = describe_stage(
            &stage,
            &DescribeParams::default(),
            &DescribeContext::default(),
        )
        .unwrap();
        assert!(
            text.contains("main (Comp) 1920x1080 25fps 10s (250 frames)"),
            "{text}"
        );
        assert!(
            text.contains("    title (Layer text) \"Main Title\""),
            "{text}"
        );
        assert!(text.contains("transform:opacity: 2 keys f0..f12"), "{text}");
        let brief = describe_stage(
            &stage,
            &DescribeParams {
                detail: Detail::Brief,
                ..Default::default()
            },
            &DescribeContext::default(),
        )
        .unwrap();
        assert!(!brief.contains("transform:opacity"));
        let tiny = describe_stage(
            &stage,
            &DescribeParams {
                max_chars: Some(200),
                ..Default::default()
            },
            &DescribeContext::default(),
        )
        .unwrap();
        assert!(tiny.contains("more prims not shown"), "{tiny}");
    }

    #[test]
    fn prim_get_reports_sources_and_frames() {
        let stage = stage();
        let j = prim_json(
            &stage,
            &PrimGetParams {
                path: PrimPath::new("/comps/main/layers/title").unwrap(),
                checkpoint: None,
                include_defaults: true,
            },
        )
        .unwrap();
        let op = &j["attributes"]["transform:opacity"];
        assert_eq!(op["source"], "root");
        assert_eq!(op["key_frames"], json!([0, 12]));
        assert_eq!(j["comp"]["fps"], 25.0);
        assert!(j["defaults"]["transform:position"].is_object());
        // Text-only attributes appear for a text layer, camera ones do not.
        assert!(j["defaults"].get("camera:zoom").is_none());
    }

    #[test]
    fn find_by_type_kind_and_glob() {
        let stage = stage();
        let r = find_prims(
            &stage,
            &FindParams {
                prim_type: Some("layer".into()),
                name: Some("main*".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r["matches"][0]["path"], "/comps/main/layers/title");
        let r = find_prims(
            &stage,
            &FindParams {
                kind: Some("shape".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r["matches"], json!([]));
        assert!(glob_match("t?tle", "Title"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("a*c", "abd"));
    }
}
