//! The MCP tool catalog.
//!
//! **Naming.** Every tool has a canonical dotted ID (`stage.describe`, `attr.set`);
//! the MCP tool name replaces `.` with `_` (`stage_describe`, `attr_set`), because
//! the Claude and OpenAI tool-use APIs only accept `[a-zA-Z0-9_-]` in tool names and
//! some MCP clients rewrite or reject dotted names. The audit log records the
//! canonical ID.
//!
//! **Sources.** Built-in agent tools (queries, renders, batch, history) are defined
//! here; every command in the [`CommandRegistry`] becomes a tool automatically, with
//! its title, description and parameter schema. A command whose tool name collides
//! with a built-in tool is skipped.

use std::sync::Arc;

use bae_model::{CommandInfo, CommandRegistry, PrimPath, SublayerId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value as Json;

use crate::describe::{DescribeParams, FindParams, PrimGetParams};
use crate::identity::Permission;

pub const STAGE_DESCRIBE: &str = "stage.describe";
pub const PRIM_GET: &str = "prim.get";
pub const PRIM_FIND: &str = "prim.find";
pub const SUBLAYER_LIST: &str = "sublayer.list";
pub const SUBLAYER_DIFF: &str = "sublayer.diff";
pub const SCHEMA_GET: &str = "schema.get";
pub const RENDER_FRAME: &str = "render.frame";
pub const RENDER_CONTACT_SHEET: &str = "render.contact_sheet";
pub const COMMAND_BATCH: &str = "command.batch";
pub const HISTORY_UNDO: &str = "history.undo";
pub const HISTORY_REDO: &str = "history.redo";

/// MCP tool name for a canonical ID: `.` → `_`.
pub fn tool_name(id: &str) -> String {
    id.replace('.', "_")
}

/// Where a tool comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    /// Defined by this crate.
    Builtin,
    /// Generated from a registry command.
    Command,
}

/// One tool as offered over MCP.
#[derive(Clone, Debug)]
pub struct ToolEntry {
    /// MCP tool name (`attr_set`).
    pub name: String,
    /// Canonical ID (`attr.set`).
    pub id: String,
    pub title: String,
    pub description: String,
    /// JSON Schema of the arguments object (`"type": "object"`).
    pub input_schema: Arc<serde_json::Map<String, Json>>,
    /// Does not modify the project.
    pub read_only: bool,
    pub kind: ToolKind,
}

// ---------------------------------------------------------------------------
// Parameters of the built-in tools (the doc comments become schema descriptions).

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct NoParams {}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct SublayerDiffParams {
    /// Sublayer to show (default: your edit target, normally agent/<you>).
    #[serde(default)]
    pub sublayer: Option<SublayerId>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct SchemaGetParams {
    /// Prim type, e.g. Comp, Layer, Mask, Effect. Omitted lists every built-in type.
    #[serde(default)]
    pub prim_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct RenderFrameParams {
    /// Comp to render, e.g. /comps/main.
    pub comp: PrimPath,
    /// Comp frame number, from 0 (default 0).
    #[serde(default)]
    pub frame: i64,
    /// Output scale relative to the comp size: 1, 0.5 (default) or 0.25. The longer
    /// side is capped at 2048 pixels.
    #[serde(default)]
    pub scale: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ContactSheetParams {
    /// Comp to render, e.g. /comps/main.
    pub comp: PrimPath,
    /// Explicit comp frame numbers. If omitted, frames go from `start` to `end` every
    /// `step`.
    #[serde(default)]
    pub frames: Option<Vec<i64>>,
    /// First frame (default 0).
    #[serde(default)]
    pub start: Option<i64>,
    /// Last frame, inclusive (default: the comp's last frame).
    #[serde(default)]
    pub end: Option<i64>,
    /// Frame step (default: chosen so that about 12 frames are shown).
    #[serde(default)]
    pub step: Option<i64>,
    /// Grid columns (default 4).
    #[serde(default)]
    pub columns: Option<u32>,
    /// Width of each thumbnail in pixels (default 320).
    #[serde(default)]
    pub thumb_width: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct BatchStep {
    /// Command ID (attr.set) or its tool name (attr_set).
    pub command: String,
    /// The command's parameters, exactly as for its own tool.
    #[serde(default = "empty_object")]
    pub params: Json,
}

fn empty_object() -> Json {
    Json::Object(Default::default())
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct BatchParams {
    /// Commands to run in order as one undo step. If any fails, none is applied.
    pub commands: Vec<BatchStep>,
    /// Undo/history label (default: the first command's title).
    #[serde(default)]
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------

/// Turn a schemars schema into an MCP input schema: an object schema without the
/// `$schema` and `title` noise.
pub fn input_schema(schema: &Json) -> Arc<serde_json::Map<String, Json>> {
    let mut map = match schema {
        Json::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };
    map.remove("$schema");
    map.remove("title");
    if map.get("type").and_then(Json::as_str) != Some("object") {
        // Unit / non-object params: no arguments.
        map = serde_json::Map::new();
        map.insert("type".into(), "object".into());
        map.insert("properties".into(), Json::Object(Default::default()));
    }
    Arc::new(map)
}

fn schema_of<T: JsonSchema>() -> Arc<serde_json::Map<String, Json>> {
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Json::Null);
    input_schema(&schema)
}

fn builtin(
    id: &str,
    title: &str,
    description: &str,
    read_only: bool,
    input_schema: Arc<serde_json::Map<String, Json>>,
) -> ToolEntry {
    ToolEntry {
        name: tool_name(id),
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        input_schema,
        read_only,
        kind: ToolKind::Builtin,
    }
}

/// The built-in agent tools.
pub fn builtin_tools() -> Vec<ToolEntry> {
    vec![
        builtin(
            STAGE_DESCRIBE,
            "Describe Project",
            "Compact text summary of the project: your identity, permission and edit target, the sublayer stack, and the prim tree (comps with size/fps/duration, layers with kind, authored attributes with keyframe counts and frame ranges, LOCKED marks). Start here; narrow with root, detail and max_chars.",
            true,
            schema_of::<DescribeParams>(),
        ),
        builtin(
            PRIM_GET,
            "Get Prim",
            "Read one prim as JSON: type, which sublayers have opinions about it, every authored attribute (static value, keyframes with key_frames in frames, expression) with the sublayer the value comes from and the weaker ones it overrides, references, children, and the enclosing comp's size and frame rate. include_defaults also lists unauthored schema attributes.",
            true,
            schema_of::<PrimGetParams>(),
        ),
        builtin(
            PRIM_FIND,
            "Find Prims",
            "Find prims by type, layer kind and/or name glob (* and ?; matches the path segment or the display name), optionally below a prim.",
            true,
            schema_of::<FindParams>(),
        ),
        builtin(
            SUBLAYER_LIST,
            "List Sublayers",
            "The sublayer stack (strongest first) with spec counts, which ones are yours / writable / proposals, your edit target and permission.",
            true,
            schema_of::<NoParams>(),
        ),
        builtin(
            SUBLAYER_DIFF,
            "Show Sublayer Opinions",
            "Everything one sublayer says (its own opinions, not the composed result) as JSON. Defaults to your proposal layer: this is exactly what the user will review.",
            true,
            schema_of::<SublayerDiffParams>(),
        ),
        builtin(
            SCHEMA_GET,
            "Get Prim Schema",
            "Attribute names, value types, defaults, animatable flags and enum options of built-in prim types (Comp, Layer, Mask, Effect, shape items …). Use it before attr_set / key_set to get names and types right.",
            true,
            schema_of::<SchemaGetParams>(),
        ),
        builtin(
            RENDER_FRAME,
            "Render Frame",
            "Render one comp frame and return it as a PNG image. Costs render budget: prefer scale 0.5 or 0.25.",
            true,
            schema_of::<RenderFrameParams>(),
        ),
        builtin(
            RENDER_CONTACT_SHEET,
            "Render Contact Sheet",
            "Render several comp frames as labelled thumbnails in one PNG grid, to check timing and motion at a glance. Give frames, or start/end/step.",
            true,
            schema_of::<ContactSheetParams>(),
        ),
        builtin(
            COMMAND_BATCH,
            "Run Commands",
            "Run several commands as ONE undo step (one history entry, one proposal change). All or nothing: if any command fails or is not permitted, nothing is applied. Prefer this for multi-step edits.",
            false,
            schema_of::<BatchParams>(),
        ),
        builtin(
            HISTORY_UNDO,
            "Undo",
            "Undo your own most recent step (other collaborators' changes are never undone).",
            false,
            schema_of::<NoParams>(),
        ),
        builtin(
            HISTORY_REDO,
            "Redo",
            "Redo your own most recently undone step.",
            false,
            schema_of::<NoParams>(),
        ),
    ]
}

fn command_entry(info: &CommandInfo) -> ToolEntry {
    let mut description = info.description.clone();
    description
        .push_str(" Writes to your edit target sublayer (agent/<you>) unless stated otherwise.");
    ToolEntry {
        name: tool_name(&info.id),
        id: info.id.clone(),
        title: info.title.clone(),
        description,
        input_schema: {
            let mut schema = (*input_schema(&info.params_schema)).clone();
            crate::timeargs::rewrite_schema(&mut schema);
            Arc::new(schema)
        },
        read_only: false,
        kind: ToolKind::Command,
    }
}

/// Does a command take an optional top-level `project` parameter (the open
/// project's location, e.g. the `package.*` commands)? The server fills it in.
pub fn takes_project(schema: &Json) -> bool {
    schema.pointer("/properties/project").is_some()
        && !schema
            .get("required")
            .and_then(Json::as_array)
            .is_some_and(|r| r.iter().any(|x| x == "project"))
}

/// Tell the agent that `project` defaults to the open project.
pub fn note_project_default(schema: &mut serde_json::Map<String, Json>) {
    if let Some(Json::Object(p)) = schema
        .get_mut("properties")
        .and_then(|p| p.get_mut("project"))
    {
        p.insert(
            "description".into(),
            "Project location (.bae folder or .baez file). Omit it: the server fills in the open project.".into(),
        );
    }
}

/// Every tool offered to an agent with `permission`: built-ins first, then the
/// registry's commands. Read-only agents only see read-only tools.
pub fn catalog(registry: &CommandRegistry, permission: Permission) -> Vec<ToolEntry> {
    let mut tools = builtin_tools();
    for info in registry.infos() {
        let entry = command_entry(info);
        if tools.iter().any(|t| t.name == entry.name) {
            tracing::warn!(
                "command {} skipped: tool name {} is already taken",
                info.id,
                entry.name
            );
            continue;
        }
        tools.push(entry);
    }
    if !permission.can_write() {
        tools.retain(|t| t.read_only);
    }
    tools
}

/// Find a tool by MCP name or canonical ID.
pub fn resolve(registry: &CommandRegistry, name: &str) -> Option<ToolEntry> {
    catalog(registry, Permission::AnyLayer)
        .into_iter()
        .find(|t| t.name == name || t.id == name)
}

/// Command ID for a batch step given as an ID or a tool name.
pub fn resolve_command(registry: &CommandRegistry, name: &str) -> Option<String> {
    if registry.get(name).is_some() {
        return Some(name.to_owned());
    }
    registry
        .infos()
        .find(|i| tool_name(&i.id) == name)
        .map(|i| i.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CommandRegistry {
        let mut r = CommandRegistry::with_builtins();
        crate::register_commands(&mut r);
        r
    }

    #[test]
    fn names_are_api_safe_and_unique() {
        let tools = catalog(&registry(), Permission::AnyLayer);
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        for n in &names {
            assert!(
                n.len() <= 64
                    && n.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "{n}"
            );
        }
        let len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), len);
        for t in &tools {
            assert_eq!(t.input_schema["type"], "object", "{}", t.id);
            assert!(!t.input_schema.contains_key("$schema"));
        }
        assert!(names.contains(&"attr_set") && names.contains(&"proposal_accept"));
    }

    #[test]
    fn read_only_sees_only_queries() {
        let tools = catalog(&registry(), Permission::ReadOnly);
        assert!(tools.iter().all(|t| t.read_only));
        assert!(tools.iter().any(|t| t.id == RENDER_FRAME));
        assert!(
            !tools
                .iter()
                .any(|t| t.id == "attr.set" || t.id == COMMAND_BATCH)
        );
    }

    #[test]
    fn resolves_names_and_ids() {
        let r = registry();
        assert_eq!(resolve(&r, "attr_set").unwrap().id, "attr.set");
        assert_eq!(
            resolve(&r, "stage.describe").unwrap().name,
            "stage_describe"
        );
        assert_eq!(
            resolve_command(&r, "sublayer_set_edit_target").unwrap(),
            "sublayer.set_edit_target"
        );
        assert!(resolve_command(&r, "stage_describe").is_none());
    }
}
