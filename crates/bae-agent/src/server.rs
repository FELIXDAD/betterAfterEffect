//! The MCP server: one connection = one agent session. The JSON-RPC layer is in
//! [`crate::protocol`].

use std::sync::Arc;
use std::time::Instant;

use bae_model::{Change, PrimPath, StageEvent, SublayerId, Time};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};

use crate::audit::{AuditEntry, AuditLog, AuditOutcome, clip_params, unix_ms_now};
use crate::describe::{
    self, AgentContext, DescribeContext, DescribeParams, FindParams, PrimGetParams,
};
use crate::identity::{AgentIdentity, Permission};
use crate::render::{self, FrameRenderer, FrameRequest, PlaceholderRenderer, RgbaImage};
use crate::session::{AgentError, AgentSession, Step};
use crate::tools::{self, ToolEntry, ToolKind};
use crate::workspace::SharedWorkspace;

/// Operating rules from the agent guide (workstream 18), English (default).
pub const GUIDE_INSTRUCTIONS_EN: &str =
    include_str!("../../../docs/agent-guide/mcp-instructions/en.md");
/// The same rules in Chinese.
pub const GUIDE_INSTRUCTIONS_ZH: &str =
    include_str!("../../../docs/agent-guide/mcp-instructions/zh.md");

/// Server-specific notes appended to the guide text: the actual tool naming and the
/// guide tools this server does not offer yet.
const SERVER_NOTE_EN: &str = "\n\nServer notes (bae mcp)\n- Tool names use _ instead of . (stage.describe -> stage_describe, attr.set -> attr_set).\n- Instead of transaction.begin / transaction.commit, send the steps of one change in a single command_batch call: one undo step, all or nothing.\n- Not available yet: attribute.evaluate (read keyframes with prim_get), proposal.describe and comment.add (summarize your proposal in your reply instead).\n- schema_get lists attribute names, value types and defaults; sublayer_diff shows your whole proposal.\n- Times: wherever a command takes a time, you may pass a frame number {\"frame\": 12} or \"12f\", or timecode \"0:00:00:12\"; integers are flicks. Frames count at the comp's frame rate, in the parameter's own time base (layer time for keyframes on layers, the same frames prim_get reports).\n- Commands with a project parameter (package_*, asset_relink) get the open project filled in.\n- Named versions: history_checkpoint saves one, history_list / history_diff compare, stage_describe and prim_get take a checkpoint to look at an old version, history_revert goes back (with own-layer permission, give sublayer agent/<you>).\n";
const SERVER_NOTE_ZH: &str = "\n\n本服务器说明（bae mcp）\n- 工具名用 _ 代替 .（stage.describe -> stage_describe，attr.set -> attr_set）。\n- 不用 transaction.begin / transaction.commit：一个修改的所有步骤放进一次 command_batch 调用，是一个撤销单元，要么全部生效，要么全部不生效。\n- 暂未提供：attribute.evaluate（用 prim_get 读关键帧）、proposal.describe 和 comment.add（在回复里写提案说明）。\n- schema_get 列出属性名、值类型和默认值；sublayer_diff 显示你的完整提案。\n- 时间参数可以直接写帧号 {\"frame\": 12} 或 \"12f\"，或者时码 \"0:00:00:12\"；整数仍是 flicks。帧号按合成帧速率计算，时间基准跟参数本身一致（图层上的关键帧用图层时间，和 prim_get 返回的帧号一致）。\n- 带 project 参数的命令（package_*、asset_relink）由服务器自动填入当前工程。\n- 命名版本：history_checkpoint 保存一个版本，history_list / history_diff 列出和比较，stage_describe 和 prim_get 传 checkpoint 可以查看旧版本，history_revert 回到某个版本（只写自己层的权限下请指定 sublayer agent/<你>）。\n";

/// Language of the built-in instructions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InstructionsLang {
    #[default]
    En,
    Zh,
}

impl std::str::FromStr for InstructionsLang {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "en" | "english" => Ok(InstructionsLang::En),
            "zh" | "zh-cn" | "chinese" => Ok(InstructionsLang::Zh),
            _ => Err(format!("unknown instructions language {s:?} (en or zh)")),
        }
    }
}

/// The built-in `instructions` text: the guide's rules plus server notes.
pub fn instructions(lang: InstructionsLang) -> String {
    match lang {
        InstructionsLang::En => format!("{}{SERVER_NOTE_EN}", GUIDE_INSTRUCTIONS_EN.trim_end()),
        InstructionsLang::Zh => format!("{}{SERVER_NOTE_ZH}", GUIDE_INSTRUCTIONS_ZH.trim_end()),
    }
}

/// Server settings.
#[derive(Clone, Debug)]
pub struct McpOptions {
    /// Agent name (`agent/<name>` sublayer, `agent:<name>` author). `None` derives it
    /// from the MCP client's name (e.g. "claude-code").
    pub agent_name: Option<String>,
    pub permission: Permission,
    /// Save the project after every successful edit (only if it has a location).
    pub autosave: bool,
    /// Language of the built-in instructions.
    pub lang: InstructionsLang,
    /// Replace the built-in instructions entirely.
    pub instructions: Option<String>,
}

impl Default for McpOptions {
    fn default() -> Self {
        McpOptions {
            agent_name: None,
            permission: Permission::OwnLayer,
            autosave: true,
            lang: InstructionsLang::En,
            instructions: None,
        }
    }
}

const MAX_RENDER_SIDE: u32 = 2048;
const MAX_SHEET_FRAMES: usize = 48;
const MAX_CHANGED_PATHS: usize = 50;

/// The BAE MCP server. Cheap to clone; clones share the session.
#[derive(Clone)]
pub struct BaeMcpServer {
    workspace: SharedWorkspace,
    session: Arc<Mutex<Option<AgentSession>>>,
    options: Arc<McpOptions>,
    audit: Arc<AuditLog>,
    renderer: Arc<dyn FrameRenderer>,
}

impl std::fmt::Debug for BaeMcpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaeMcpServer")
            .field("options", &self.options)
            .field("session", &self.session.lock())
            .finish_non_exhaustive()
    }
}

/// One content block of a tool result.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    /// Base64-encoded image data.
    Image {
        data: String,
        mime_type: String,
    },
}

/// The result of a tool call (MCP `CallToolResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub content: Vec<Content>,
    /// Machine-readable result (also serialized into the text content).
    pub structured: Option<Json>,
    /// The call failed; the text says why (the agent reads it).
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        ToolResult {
            content: vec![Content::Text(text.into())],
            structured: None,
            is_error: false,
        }
    }

    pub fn json(value: Json) -> Self {
        ToolResult {
            content: vec![Content::Text(value.to_string())],
            structured: Some(value),
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        ToolResult {
            is_error: true,
            ..ToolResult::text(message)
        }
    }

    /// All text content, joined by newlines.
    pub fn joined_text(&self) -> String {
        let parts: Vec<&str> = self
            .content
            .iter()
            .filter_map(|c| match c {
                Content::Text(t) => Some(t.as_str()),
                Content::Image { .. } => None,
            })
            .collect();
        parts.join("\n")
    }

    /// MCP JSON shape: `{content, structuredContent?, isError}`.
    pub fn to_json(&self) -> Json {
        let content: Vec<Json> = self
            .content
            .iter()
            .map(|c| match c {
                Content::Text(t) => json!({ "type": "text", "text": t }),
                Content::Image { data, mime_type } => {
                    json!({ "type": "image", "data": data, "mimeType": mime_type })
                }
            })
            .collect();
        let mut out = json!({ "content": content, "isError": self.is_error });
        if let Some(s) = &self.structured {
            out["structuredContent"] = s.clone();
        }
        out
    }
}

/// Successful tool result before conversion to MCP content.
enum Reply {
    Text(String),
    Json(Json),
    Image { png: Vec<u8>, meta: Json },
}

struct Done {
    reply: Reply,
    event: Option<StageEvent>,
}

impl From<Reply> for Done {
    fn from(reply: Reply) -> Self {
        Done { reply, event: None }
    }
}

fn parse<P: DeserializeOwned>(tool: &ToolEntry, args: &Json) -> Result<P, AgentError> {
    serde_json::from_value(args.clone()).map_err(|e| {
        AgentError::Invalid(format!(
            "invalid arguments for {}: {e}. Expected schema: {}",
            tool.name,
            Json::Object((*tool.input_schema).clone())
        ))
    })
}

fn changed_paths(event: &StageEvent) -> Vec<&PrimPath> {
    let mut paths: Vec<&PrimPath> = Vec::new();
    for c in &event.changes {
        let p = match c {
            Change::PrimMoved { to, .. } => to,
            other => other.path(),
        };
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths
}

impl BaeMcpServer {
    /// A server over `workspace` with an in-memory audit log and the placeholder
    /// renderer.
    pub fn new(workspace: SharedWorkspace, options: McpOptions) -> Self {
        BaeMcpServer {
            workspace,
            session: Arc::new(Mutex::new(None)),
            options: Arc::new(options),
            audit: Arc::new(AuditLog::new()),
            renderer: Arc::new(PlaceholderRenderer),
        }
    }

    pub fn with_audit_log(mut self, audit: Arc<AuditLog>) -> Self {
        self.audit = audit;
        self
    }

    pub fn with_renderer(mut self, renderer: Arc<dyn FrameRenderer>) -> Self {
        self.renderer = renderer;
        self
    }

    pub fn workspace(&self) -> &SharedWorkspace {
        &self.workspace
    }

    pub fn audit_log(&self) -> &Arc<AuditLog> {
        &self.audit
    }

    pub fn options(&self) -> &McpOptions {
        &self.options
    }

    /// Bind the session to an agent (on `initialize`; later calls keep the first).
    /// `client_name` is used when no agent name was configured.
    pub fn bind_agent(&self, client_name: Option<&str>) -> Result<AgentIdentity, AgentError> {
        let mut slot = self.session.lock();
        if let Some(s) = slot.as_ref() {
            return Ok(s.identity().clone());
        }
        let identity = match &self.options.agent_name {
            Some(n) => AgentIdentity::new(n).map_err(|e| AgentError::Invalid(e.to_string()))?,
            None => AgentIdentity::from_client_name(client_name.unwrap_or("agent")),
        };
        tracing::info!(
            "agent {} connected ({} permission)",
            identity.name(),
            self.options.permission
        );
        *slot = Some(AgentSession::new(identity.clone(), self.options.permission));
        Ok(identity)
    }

    /// The tools this session offers.
    pub fn tool_entries(&self) -> Vec<ToolEntry> {
        let permission = self
            .session
            .lock()
            .as_ref()
            .map_or(self.options.permission, AgentSession::permission);
        let ws = self.workspace.lock();
        let mut entries = tools::catalog(&ws.registry, permission);
        if ws.project.location().is_some() {
            for t in &mut entries {
                if t.kind == ToolKind::Command
                    && tools::takes_project(&Json::Object((*t.input_schema).clone()))
                {
                    let mut schema = (*t.input_schema).clone();
                    tools::note_project_default(&mut schema);
                    t.input_schema = Arc::new(schema);
                }
            }
        }
        entries
    }

    /// The `instructions` text sent on `initialize`.
    pub fn instructions(&self) -> String {
        self.options
            .instructions
            .clone()
            .unwrap_or_else(|| instructions(self.options.lang))
    }

    /// Handle one tool call (transport independent). Every call is audited.
    pub fn call(&self, name: &str, args: Json) -> ToolResult {
        let started = Instant::now();
        let unix_ms = unix_ms_now();
        if self.session.lock().is_none() {
            let _ = self.bind_agent(None);
        }
        let args = if args.is_null() {
            Json::Object(Default::default())
        } else {
            args
        };
        let tool = tools::resolve(&self.workspace.lock().registry, name);
        let result = match &tool {
            Some(t) => self.dispatch(t, &args),
            None => Err(AgentError::Invalid(format!(
                "unknown tool {name}; call tools/list for the available tools"
            ))),
        };

        let (author, agent) = {
            let s = self.session.lock();
            let s = s.as_ref().expect("bound above");
            (
                s.identity().author().clone(),
                s.identity().name().to_owned(),
            )
        };
        let (outcome, sublayers, changes) = match &result {
            Ok(done) => (
                AuditOutcome::Ok,
                done.event
                    .as_ref()
                    .map(|e| e.sublayers.clone())
                    .unwrap_or_default(),
                done.event.as_ref().map_or(0, |e| e.changes.len()),
            ),
            Err(e @ AgentError::Denied(_)) => (
                AuditOutcome::Denied {
                    message: e.to_string(),
                },
                vec![],
                0,
            ),
            Err(e) => (
                AuditOutcome::Error {
                    message: e.to_string(),
                },
                vec![],
                0,
            ),
        };
        self.audit.record(AuditEntry {
            seq: 0,
            unix_ms,
            agent,
            author,
            tool: tool
                .as_ref()
                .map_or_else(|| name.to_owned(), |t| t.id.clone()),
            params: clip_params(&args),
            outcome,
            sublayers,
            changes,
            duration_us: started.elapsed().as_micros() as u64,
        });

        match result {
            Ok(Done { reply, .. }) => match reply {
                Reply::Text(text) => ToolResult::text(text),
                Reply::Json(j) => ToolResult::json(j),
                Reply::Image { png, meta } => ToolResult {
                    content: vec![
                        Content::Image {
                            data: render::base64(&png),
                            mime_type: "image/png".into(),
                        },
                        Content::Text(meta.to_string()),
                    ],
                    structured: None,
                    is_error: false,
                },
            },
            Err(e) => ToolResult::error(e.to_string()),
        }
    }

    fn dispatch(&self, tool: &ToolEntry, args: &Json) -> Result<Done, AgentError> {
        if tool.kind == ToolKind::Command {
            let step = Step {
                command: tool.id.clone(),
                params: args.clone(),
            };
            return self.run_steps(None, vec![step]);
        }
        match tool.id.as_str() {
            tools::STAGE_DESCRIBE => {
                let p: DescribeParams = parse(tool, args)?;
                let session = self.session.lock();
                let ws = self.workspace.lock();
                let ctx = DescribeContext {
                    location: ws.project.location().map(|l| l.display().to_string()),
                    agent: session.as_ref().map(AgentContext::of),
                };
                let Some(id) = &p.checkpoint else {
                    return Ok(
                        Reply::Text(describe::describe_stage(&ws.project.stage, &p, &ctx)?).into(),
                    );
                };
                let (cp, view) = checkpoint_view(&ws, id)?;
                let mut text = format!(
                    "Named version {:?} ({}), saved by {} — read-only view; edits always go to the current project.\n",
                    cp.name, cp.id.0, cp.author
                );
                if !view.missing().is_empty() {
                    text.push_str(&format!(
                        "Sublayers missing from this project now (not shown): {:?}\n",
                        view.missing()
                    ));
                }
                text.push_str(&describe::describe_stage(view.stage(), &p, &ctx)?);
                Ok(Reply::Text(text).into())
            }
            tools::PRIM_GET => {
                let p: PrimGetParams = parse(tool, args)?;
                let ws = self.workspace.lock();
                match &p.checkpoint {
                    None => Ok(Reply::Json(describe::prim_json(&ws.project.stage, &p)?).into()),
                    Some(id) => {
                        let (cp, view) = checkpoint_view(&ws, id)?;
                        let mut j = describe::prim_json(view.stage(), &p)?;
                        j["checkpoint"] = json!({ "id": cp.id.0, "name": cp.name });
                        Ok(Reply::Json(j).into())
                    }
                }
            }
            tools::HISTORY_LIST => {
                let ws = self.workspace.lock();
                let list: Vec<Json> = ws
                    .history
                    .checkpoints()
                    .iter()
                    .map(checkpoint_json)
                    .collect();
                Ok(Reply::Json(json!({ "checkpoints": list })).into())
            }
            tools::HISTORY_DIFF => {
                let p: tools::HistoryDiffParams = parse(tool, args)?;
                let ws = self.workspace.lock();
                let from = bae_sync::CheckpointId(p.from);
                let to = p.to.map(bae_sync::CheckpointId);
                let diffs = ws.history.diff(&ws.project.stage, &from, to.as_ref())?;
                let limit = p.limit.unwrap_or(200).clamp(1, 5000);
                let listed: Vec<Json> = diffs.iter().take(limit).map(prim_diff_json).collect();
                Ok(Reply::Json(json!({
                    "changes": listed,
                    "total": diffs.len(),
                    "truncated": diffs.len() > limit,
                }))
                .into())
            }
            tools::HISTORY_CHECKPOINT => {
                let p: tools::CheckpointParams = parse(tool, args)?;
                let session = self.session.lock();
                let session = session.as_ref().expect("bound");
                if !session.permission().can_write() {
                    return Err(AgentError::Denied(format!(
                        "agent {} is read-only; it cannot save named versions",
                        session.identity().name()
                    )));
                }
                let mut ws = self.workspace.lock();
                let ws = &mut *ws;
                let cp = ws.history.create_checkpoint(
                    &ws.project.stage,
                    session.identity().author(),
                    &p.name,
                    &p.note,
                )?;
                let saved = match ws.save_history() {
                    Ok(true) => json!(true),
                    Ok(false) => json!("in memory only (the project is not a saved .bae folder)"),
                    Err(e) => json!(format!("save failed: {e}")),
                };
                Ok(
                    Reply::Json(json!({ "checkpoint": checkpoint_json(&cp), "saved": saved }))
                        .into(),
                )
            }
            tools::HISTORY_REVERT => {
                let p: tools::HistoryRevertParams = parse(tool, args)?;
                let (name, version) = {
                    let ws = self.workspace.lock();
                    let cp = ws
                        .history
                        .checkpoint(&bae_sync::CheckpointId(p.checkpoint.clone()))
                        .ok_or_else(|| {
                            AgentError::Invalid(format!(
                                "no named version {}; see history_list",
                                p.checkpoint
                            ))
                        })?;
                    (cp.name, cp.version.to_json())
                };
                let mut params = json!({ "version": version });
                if let Some(s) = p.sublayer {
                    params["sublayer"] = json!(s);
                }
                if let Some(prim) = p.prim {
                    params["prim"] = json!(prim);
                }
                self.run_steps(
                    Some(&format!("revert to {name}")),
                    vec![Step {
                        command: "version.revert".into(),
                        params,
                    }],
                )
            }
            tools::PRIM_FIND => {
                let p: FindParams = parse(tool, args)?;
                let ws = self.workspace.lock();
                Ok(Reply::Json(describe::find_prims(&ws.project.stage, &p)?).into())
            }
            tools::SUBLAYER_LIST => {
                let session = self.session.lock();
                let ws = self.workspace.lock();
                Ok(
                    Reply::Json(describe::sublayer_list(&ws.project.stage, session.as_ref()))
                        .into(),
                )
            }
            tools::SUBLAYER_DIFF => {
                let p: tools::SublayerDiffParams = parse(tool, args)?;
                let session = self.session.lock();
                let ws = self.workspace.lock();
                let id: SublayerId = match p.sublayer {
                    Some(id) => id,
                    None => session
                        .as_ref()
                        .map(|s| s.edit_target().clone())
                        .unwrap_or_else(SublayerId::root),
                };
                if ws.project.stage.sublayer(&id).is_none() {
                    return Ok(Reply::Json(json!({
                        "sublayer": id,
                        "prims": [],
                        "note": "this sublayer does not exist yet (your proposal layer is created on your first edit)",
                    }))
                    .into());
                }
                Ok(Reply::Json(bae_model::export_sublayer_json(&ws.project.stage, &id)?).into())
            }
            tools::SCHEMA_GET => {
                let p: tools::SchemaGetParams = parse(tool, args)?;
                let all = bae_model::schema::to_json();
                let Some(want) = p.prim_type else {
                    return Ok(Reply::Json(all).into());
                };
                let found = all["prim_types"]
                    .as_array()
                    .and_then(|v| {
                        v.iter().find(|t| {
                            t["prim_type"]
                                .as_str()
                                .is_some_and(|n| n.eq_ignore_ascii_case(&want))
                        })
                    })
                    .cloned();
                match found {
                    Some(t) => Ok(Reply::Json(t).into()),
                    None => Err(AgentError::Invalid(format!(
                        "no schema for prim type {want}; omit prim_type to list all"
                    ))),
                }
            }
            tools::RENDER_FRAME => {
                let p: tools::RenderFrameParams = parse(tool, args)?;
                self.render_frame(&p).map(Done::from)
            }
            tools::RENDER_CONTACT_SHEET => {
                let p: tools::ContactSheetParams = parse(tool, args)?;
                self.render_contact_sheet(&p).map(Done::from)
            }
            tools::COMMAND_BATCH => {
                let p: tools::BatchParams = parse(tool, args)?;
                let steps = {
                    let ws = self.workspace.lock();
                    p.commands
                        .into_iter()
                        .map(|s| {
                            let command = tools::resolve_command(&ws.registry, &s.command)
                                .ok_or_else(|| {
                                    AgentError::Invalid(format!(
                                        "unknown command {} in batch (use a command ID like attr.set)",
                                        s.command
                                    ))
                                })?;
                            Ok(Step {
                                command,
                                params: s.params,
                            })
                        })
                        .collect::<Result<Vec<_>, AgentError>>()?
                };
                self.run_steps(p.label.as_deref(), steps)
            }
            tools::HISTORY_UNDO | tools::HISTORY_REDO => {
                let undo = tool.id == tools::HISTORY_UNDO;
                let mut session = self.session.lock();
                let session = session.as_mut().expect("bound");
                let mut ws = self.workspace.lock();
                let event = if undo {
                    session.undo(&mut ws)?
                } else {
                    session.redo(&mut ws)?
                };
                let reply = match &event {
                    Some(e) => json!({
                        "ok": true,
                        "sublayers": e.sublayers,
                        "changed": changed_paths(e).into_iter().take(MAX_CHANGED_PATHS).collect::<Vec<_>>(),
                        "saved": self.autosave(&mut ws),
                    }),
                    None => {
                        json!({ "ok": false, "note": format!("nothing to {}", if undo { "undo" } else { "redo" }) })
                    }
                };
                Ok(Done {
                    reply: Reply::Json(reply),
                    event,
                })
            }
            other => Err(AgentError::Invalid(format!(
                "tool {other} is not implemented"
            ))),
        }
    }

    fn run_steps(&self, label: Option<&str>, steps: Vec<Step>) -> Result<Done, AgentError> {
        let mut session = self.session.lock();
        let session = session.as_mut().expect("bound");
        let mut ws = self.workspace.lock();
        let outcome = session.execute(&mut ws, label, &steps)?;
        let changed: Vec<&PrimPath> = changed_paths(&outcome.event);
        let truncated = changed.len() > MAX_CHANGED_PATHS;
        let mut reply = json!({
            "ok": true,
            "sublayers": outcome.event.sublayers,
            "changed": changed.into_iter().take(MAX_CHANGED_PATHS).collect::<Vec<_>>(),
        });
        if truncated {
            reply["changed_truncated"] = json!(true);
        }
        if steps.len() == 1 {
            let out = outcome.outputs.into_iter().next().unwrap_or(Json::Null);
            if !out.is_null() {
                reply["output"] = out;
            }
        } else {
            reply["outputs"] = json!(outcome.outputs);
        }
        if let Some(saved) = self.autosave(&mut ws) {
            reply["saved"] = saved;
        }
        Ok(Done {
            reply: Reply::Json(reply),
            event: Some(outcome.event),
        })
    }

    /// Save after an edit if configured; returns a status for the reply.
    fn autosave(&self, ws: &mut crate::Workspace) -> Option<Json> {
        if !self.options.autosave || ws.project.location().is_none() {
            return None;
        }
        Some(match ws.project.save() {
            Ok(()) => json!(true),
            Err(e) => {
                tracing::warn!("autosave failed: {e}");
                json!(format!("save failed: {e}"))
            }
        })
    }

    fn render_meta(&self, comp: &PrimPath, extra: Json) -> Json {
        let mut meta = json!({ "comp": comp });
        if let (Json::Object(m), Json::Object(e)) = (&mut meta, extra) {
            m.extend(e);
        }
        if self.renderer.is_placeholder() {
            meta["placeholder"] = json!(true);
            meta["note"] = json!(
                "Placeholder image: the compositor is not connected yet, so this only shows the frame number and timing, not the comp's content."
            );
        }
        meta
    }

    fn render_frame(&self, p: &tools::RenderFrameParams) -> Result<Reply, AgentError> {
        let ws = self.workspace.lock();
        let stage = &ws.project.stage;
        let settings = describe::comp_settings(stage, &p.comp)
            .ok_or_else(|| AgentError::Invalid(format!("{} is not a Comp", p.comp)))?;
        let scale = p.scale.unwrap_or(0.5).clamp(0.05, 1.0);
        let (w, h) = fit(settings.width, settings.height, scale, MAX_RENDER_SIDE);
        let req = FrameRequest {
            comp: p.comp.clone(),
            frame: p.frame,
            time: settings.frame_rate.frame_to_time(p.frame),
            width: w,
            height: h,
            frame_count: settings.frame_count(),
        };
        let img = self
            .renderer
            .render(stage, &req)
            .map_err(|e| AgentError::Invalid(e.to_string()))?;
        let png = img
            .to_png()
            .map_err(|e| AgentError::Invalid(e.to_string()))?;
        let meta = self.render_meta(
            &p.comp,
            json!({
                "frame": p.frame,
                "time_seconds": req.time.as_seconds(),
                "width": w,
                "height": h,
                "comp_frames": settings.frame_count(),
            }),
        );
        Ok(Reply::Image { png, meta })
    }

    fn render_contact_sheet(&self, p: &tools::ContactSheetParams) -> Result<Reply, AgentError> {
        let ws = self.workspace.lock();
        let stage = &ws.project.stage;
        let settings = describe::comp_settings(stage, &p.comp)
            .ok_or_else(|| AgentError::Invalid(format!("{} is not a Comp", p.comp)))?;
        let last = settings.frame_count() - 1;
        let frames: Vec<i64> = match &p.frames {
            Some(f) if !f.is_empty() => f.clone(),
            _ => {
                let start = p.start.unwrap_or(0).clamp(0, last);
                let end = p.end.unwrap_or(last).clamp(start, last);
                let step = p.step.unwrap_or_else(|| ((end - start) / 11).max(1)).max(1);
                let mut v: Vec<i64> = (start..=end).step_by(step as usize).collect();
                if v.last() != Some(&end) && v.len() < MAX_SHEET_FRAMES {
                    v.push(end);
                }
                v
            }
        };
        if frames.len() > MAX_SHEET_FRAMES {
            return Err(AgentError::Invalid(format!(
                "{} frames requested; a contact sheet shows at most {MAX_SHEET_FRAMES} (raise step)",
                frames.len()
            )));
        }
        let thumb_w = p.thumb_width.unwrap_or(320).clamp(32, 1024);
        let scale = (f64::from(thumb_w) / f64::from(settings.width)).min(1.0);
        let (w, h) = fit(settings.width, settings.height, scale, 1024);
        let mut tiles: Vec<(i64, RgbaImage)> = Vec::with_capacity(frames.len());
        for &frame in &frames {
            let req = FrameRequest {
                comp: p.comp.clone(),
                frame,
                time: settings.frame_rate.frame_to_time(frame),
                width: w,
                height: h,
                frame_count: settings.frame_count(),
            };
            let img = self
                .renderer
                .render(stage, &req)
                .map_err(|e| AgentError::Invalid(e.to_string()))?;
            tiles.push((frame, img));
        }
        let columns = p.columns.unwrap_or(4);
        let sheet = render::contact_sheet(&tiles, columns);
        let png = sheet
            .to_png()
            .map_err(|e| AgentError::Invalid(e.to_string()))?;
        let meta = self.render_meta(
            &p.comp,
            json!({
                "frames": frames,
                "columns": columns.clamp(1, frames.len().max(1) as u32),
                "thumb_width": w,
                "thumb_height": h,
                "fps": settings.frame_rate.as_f64(),
                "seconds_per_frame": Time::from_flicks(settings.frame_rate.frame_duration().flicks()).as_seconds(),
            }),
        );
        Ok(Reply::Image { png, meta })
    }
}

fn checkpoint_view(
    ws: &crate::Workspace,
    id: &str,
) -> Result<(bae_sync::Checkpoint, bae_sync::VersionView), AgentError> {
    let id = bae_sync::CheckpointId(id.to_owned());
    let cp = ws.history.checkpoint(&id).ok_or_else(|| {
        AgentError::Invalid(format!("no named version {}; see history_list", id.0))
    })?;
    let view = ws.history.view(&ws.project.stage, &id)?;
    Ok((cp, view))
}

fn checkpoint_json(cp: &bae_sync::Checkpoint) -> Json {
    json!({
        "id": cp.id.0,
        "name": cp.name,
        "note": cp.note,
        "author": cp.author,
        "created_ms": cp.created_ms,
        "layer_stack": cp.layer_stack,
        "version": cp.version.to_json(),
    })
}

fn prim_diff_json(d: &bae_sync::PrimDiff) -> Json {
    let (change, from) = match &d.change {
        bae_sync::PrimChange::Added => ("added", None),
        bae_sync::PrimChange::Removed => ("removed", None),
        bae_sync::PrimChange::Modified => ("modified", None),
        bae_sync::PrimChange::Moved { from } => ("moved", Some(from)),
    };
    let mut j = json!({
        "sublayer": d.sublayer,
        "path": d.path,
        "change": change,
        "attrs": d.attrs,
        "fields": d.fields,
        "authors": d.authors,
        "last_changed_ms": d.last_changed,
    });
    if let Some(from) = from {
        j["from"] = json!(from);
    }
    j
}

/// Scale `w`×`h` by `scale`, capping the longer side at `max_side`.
fn fit(w: u32, h: u32, scale: f64, max_side: u32) -> (u32, u32) {
    let mut s = scale;
    let longest = f64::from(w.max(h)) * s;
    if longest > f64::from(max_side) {
        s *= f64::from(max_side) / longest;
    }
    (
        ((f64::from(w) * s).round() as u32).max(1),
        ((f64::from(h) * s).round() as u32).max(1),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Serve `server` over stdin/stdout until the client closes stdin (`bae mcp`).
/// Blocks the calling thread; requests are handled one at a time.
pub fn run_stdio(server: BaeMcpServer) -> Result<(), ServeError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    crate::protocol::serve_io(&server, stdin.lock(), stdout.lock())?;
    tracing::info!("MCP session ended (stdin closed)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;

    /// Guide tool names this server does not offer (yet); see the server notes.
    const PENDING: &[&str] = &[
        "transaction.begin",
        "transaction.commit",
        "transaction.abort",
        "attribute.evaluate",
        "proposal.describe",
        "comment.add",
        "selection.get",
    ];

    const NAMESPACES: &[&str] = &[
        "stage",
        "prim",
        "attr",
        "key",
        "render",
        "attribute",
        "transaction",
        "proposal",
        "comment",
        "sublayer",
        "history",
        "command",
        "opinion",
        "schema",
        "selection",
    ];

    /// Dotted tool names (`ns.verb`) mentioned in a text.
    fn mentioned_tools(text: &str) -> Vec<String> {
        let mut out: Vec<String> = text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .map(|w| w.trim_matches('.'))
            .filter(|w| {
                w.split_once('.').is_some_and(|(ns, verb)| {
                    NAMESPACES.contains(&ns) && !verb.is_empty() && !verb.contains('.')
                })
            })
            .map(str::to_owned)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    #[test]
    fn guide_instructions_fit_and_name_real_tools() {
        let ws = Workspace::new(bae_model::Project::new());
        let catalog = tools::catalog(&ws.registry, Permission::AnyLayer);
        for (lang, text) in [("en", GUIDE_INSTRUCTIONS_EN), ("zh", GUIDE_INSTRUCTIONS_ZH)] {
            assert!(text.chars().count() <= 3000, "{lang}: over 3000 characters");
            let names = mentioned_tools(text);
            assert!(
                names.contains(&"stage.describe".to_owned()),
                "{lang}: {names:?}"
            );
            for name in names {
                let offered = catalog.iter().any(|t| t.id == name);
                assert!(
                    offered || PENDING.contains(&name.as_str()),
                    "{lang} instructions mention {name}, which the server does not offer \
                     (add it, or list it in the server notes and PENDING)"
                );
            }
        }
        for lang in [InstructionsLang::En, InstructionsLang::Zh] {
            assert!(instructions(lang).contains("command_batch"));
        }
        assert_eq!("ZH".parse::<InstructionsLang>(), Ok(InstructionsLang::Zh));
    }

    #[test]
    fn fit_caps_the_longer_side() {
        assert_eq!(fit(1920, 1080, 0.5, 2048), (960, 540));
        assert_eq!(fit(8000, 1000, 1.0, 2048), (2048, 256));
        assert_eq!(fit(10, 10, 0.01, 2048), (1, 1));
    }
}
