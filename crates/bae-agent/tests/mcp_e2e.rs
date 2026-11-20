//! End-to-end: MCP clients talk to `BaeMcpServer` over an in-process byte pipe, both
//! through rmcp's client and with hand-written JSON-RPC lines.

use std::sync::Arc;

use bae_agent::{AuditLog, AuditOutcome, BaeMcpServer, McpOptions, Permission, Workspace};
use bae_model::{PrimPath, Project, SublayerId};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientConfig, ContentBlock, Implementation,
};
use rmcp::service::RunningService;
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use serde_json::{Value as Json, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const FLICKS_PER_FRAME_30: i64 = 705_600_000 / 30;

#[derive(Clone)]
struct TestClient(&'static str);

impl ClientHandler for TestClient {
    fn get_info(&self) -> ClientConfig {
        let mut info = ClientConfig::default();
        info.client_info = Implementation::new(self.0, "1.0");
        info
    }
}

async fn connect(
    server: BaeMcpServer,
    client_name: &'static str,
) -> RunningService<RoleClient, TestClient> {
    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        let running = server.serve(server_io).await.expect("server starts");
        let _ = running.waiting().await;
    });
    TestClient(client_name)
        .serve(client_io)
        .await
        .expect("client connects")
}

async fn call(
    client: &RunningService<RoleClient, TestClient>,
    name: &str,
    args: Json,
) -> CallToolResult {
    let mut params = CallToolRequestParams::new(name.to_owned());
    if let Json::Object(map) = args {
        params = params.with_arguments(map);
    }
    client.call_tool(params).await.expect("tools/call")
}

fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn ok_json(result: &CallToolResult) -> Json {
    assert_ne!(
        result.is_error,
        Some(true),
        "tool failed: {}",
        text_of(result)
    );
    result
        .structured_content
        .clone()
        .unwrap_or_else(|| serde_json::from_str(&text_of(result)).expect("JSON result"))
}

fn image_png(result: &CallToolResult) -> (Vec<u8>, Json) {
    assert_ne!(
        result.is_error,
        Some(true),
        "tool failed: {}",
        text_of(result)
    );
    let data = result
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::Image(i) => Some(i.data.clone()),
            _ => None,
        })
        .expect("image content");
    (
        decode_base64(&data),
        serde_json::from_str(&text_of(result)).unwrap(),
    )
}

fn decode_base64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::new();
    for chunk in bytes.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, c)| acc | (val(*c) << (18 - 6 * i)));
        for i in 0..chunk.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
    (w, h)
}

/// The title animation from the guide's example task, as one batch.
fn title_batch() -> Json {
    let title = "/comps/main/layers/title";
    json!({
        "label": "title_in: build comp and animate",
        "commands": [
            {"command": "prim.create", "params": {"path": "/comps", "prim_type": "Folder"}},
            {"command": "prim_create", "params": {"path": "/comps/main", "prim_type": "Comp"}},
            {"command": "attr.set", "params": {"path": "/comps/main", "attr": "duration",
                "value": {"type": "time", "value": 3 * 705_600_000i64}}},
            {"command": "prim.create", "params": {"path": "/comps/main/layers", "prim_type": "Folder"}},
            {"command": "prim.create", "params": {"path": title, "prim_type": "Layer"}},
            {"command": "attr.set", "params": {"path": title, "attr": "kind",
                "value": {"type": "enum", "value": "text"}}},
            {"command": "key.set", "params": {"path": title, "attr": "transform:opacity",
                "time": 0, "keyframe": {"value": {"type": "float", "value": 0.0}}}},
            {"command": "key.set", "params": {"path": title, "attr": "transform:opacity",
                "time": 15 * FLICKS_PER_FRAME_30, "keyframe": {"value": {"type": "float", "value": 100.0}}}},
        ]
    })
}

#[tokio::test]
async fn agent_builds_animates_renders_and_proposes() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = dir.path().join("demo.bae");
    let mut project = Project::new();
    project.save_as(&project_path).unwrap();
    let workspace = Workspace::new(project).shared();
    let audit = Arc::new(AuditLog::new());
    let server =
        BaeMcpServer::new(workspace.clone(), McpOptions::default()).with_audit_log(audit.clone());
    let client = connect(server, "Test Agent").await;

    // Handshake: server info and instructions from the agent guide.
    let info = client.peer_info().expect("server info");
    assert_eq!(info.server_info.as_ref().unwrap().name, "bae");
    assert!(
        info.instructions
            .as_deref()
            .unwrap()
            .contains("stage_describe")
    );

    // Tool list: generated commands plus built-ins, all with API-safe names.
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for want in [
        "stage_describe",
        "prim_get",
        "prim_find",
        "render_frame",
        "render_contact_sheet",
        "command_batch",
        "history_undo",
        "attr_set",
        "key_set",
        "sublayer_merge",
        "proposal_accept",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }
    assert!(names.iter().all(|n| !n.contains('.')));

    let describe = call(&client, "stage_describe", json!({})).await;
    let text = text_of(&describe);
    assert!(text.contains("You: agent test-agent"), "{text}");
    assert!(text.contains("(no prims yet)"), "{text}");

    // Build and animate as one undo step.
    let batch = ok_json(&call(&client, "command_batch", title_batch()).await);
    assert_eq!(batch["sublayers"], json!(["agent/test-agent"]));
    assert_eq!(batch["saved"], true);
    assert_eq!(batch["outputs"][4]["path"], "/comps/main/layers/title");

    let prim = ok_json(
        &call(
            &client,
            "prim_get",
            json!({"path": "/comps/main/layers/title"}),
        )
        .await,
    );
    assert_eq!(
        prim["attributes"]["transform:opacity"]["key_frames"],
        json!([0, 15])
    );
    assert_eq!(
        prim["attributes"]["transform:opacity"]["source"],
        "agent/test-agent"
    );
    assert_eq!(prim["comp"]["duration_frames"], 90);

    let text = text_of(&call(&client, "stage_describe", json!({"detail": "full"})).await);
    assert!(
        text.contains("main (Comp) 1920x1080 30fps 3s (90 frames)"),
        "{text}"
    );
    assert!(
        text.contains("transform:opacity: 2 keys f0..f15 @agent/test-agent"),
        "{text}"
    );

    let found = ok_json(&call(&client, "prim_find", json!({"kind": "text"})).await);
    assert_eq!(found["matches"].as_array().unwrap().len(), 1);

    // Visual checks (placeholder renderer until the compositor lands).
    let (png, meta) = image_png(
        &call(
            &client,
            "render_frame",
            json!({"comp": "/comps/main", "frame": 15}),
        )
        .await,
    );
    assert_eq!(png_size(&png), (960, 540));
    assert_eq!(meta["placeholder"], true);
    let (png, meta) = image_png(
        &call(
            &client,
            "render_contact_sheet",
            json!({"comp": "/comps/main", "start": 0, "end": 89, "step": 15, "columns": 4, "thumb_width": 160}),
        )
        .await,
    );
    assert_eq!(meta["frames"], json!([0, 15, 30, 45, 60, 75, 89]));
    assert_eq!(png_size(&png), (4 * 160 + 5 * 4, 2 * 90 + 3 * 4));

    // The proposal is what the user reviews.
    let diff = ok_json(&call(&client, "sublayer_diff", json!({})).await);
    assert_eq!(diff["sublayer"], "agent/test-agent");
    assert_eq!(diff["prims"][0]["name"], "comps");

    // Saved to disk after every edit: reopening shows the agent's sublayer.
    {
        let reopened = Project::open(&project_path).unwrap();
        let own = SublayerId::new("agent/test-agent").unwrap();
        assert!(reopened.stage.layer_stack().contains(&own));
        assert!(
            reopened
                .stage
                .has_prim(&PrimPath::new("/comps/main/layers/title").unwrap())
        );
    }

    // Own-layer permission: touching other sublayers is refused, nothing changes.
    let denied = call(&client, "sublayer_set_edit_target", json!({"id": "root"})).await;
    assert_eq!(denied.is_error, Some(true));
    assert!(
        text_of(&denied).contains("permission denied"),
        "{}",
        text_of(&denied)
    );
    let denied = call(
        &client,
        "proposal_accept",
        json!({"sublayer": "agent/test-agent"}),
    )
    .await;
    assert_eq!(denied.is_error, Some(true));

    // Bad arguments are reported to the agent, with the schema.
    let bad = call(&client, "render_frame", json!({"frame": 1})).await;
    assert_eq!(bad.is_error, Some(true));
    assert!(text_of(&bad).contains("comp"), "{}", text_of(&bad));

    // Undo/redo the whole batch.
    ok_json(&call(&client, "history_undo", json!({})).await);
    let found = ok_json(&call(&client, "prim_find", json!({"prim_type": "Layer"})).await);
    assert_eq!(found["matches"], json!([]));
    ok_json(&call(&client, "history_redo", json!({})).await);
    assert!(
        workspace
            .lock()
            .project
            .stage
            .has_prim(&PrimPath::new("/comps/main/layers/title").unwrap())
    );

    // Everything is audited, including refusals and errors.
    let entries = audit.entries();
    assert!(entries.iter().any(|e| e.tool == "command.batch"
        && e.outcome.is_ok()
        && e.sublayers == vec![SublayerId::new("agent/test-agent").unwrap()]));
    assert_eq!(
        entries
            .iter()
            .filter(|e| matches!(e.outcome, AuditOutcome::Denied { .. }))
            .count(),
        2
    );
    assert!(
        entries
            .iter()
            .any(|e| e.tool == "render.frame" && matches!(e.outcome, AuditOutcome::Error { .. }))
    );
    assert!(
        entries
            .iter()
            .all(|e| e.author.as_str() == "agent:test-agent")
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn read_only_agent_cannot_modify_anything() {
    let mut project = Project::new();
    let user = bae_model::AuthorId::new("user:felix").unwrap();
    let mut ws = Workspace::new(Project::new());
    ws.registry
        .execute(
            &mut ws.project.stage,
            &user,
            "prim.create",
            json!({"path": "/c", "prim_type": "Comp"}),
        )
        .unwrap();
    std::mem::swap(&mut project, &mut ws.project);
    let workspace = Workspace::new(project).shared();
    let before = bae_model::export_flattened_json(&workspace.lock().project.stage);
    let options = McpOptions {
        agent_name: Some("viewer".into()),
        permission: Permission::ReadOnly,
        ..Default::default()
    };
    let client = connect(BaeMcpServer::new(workspace.clone(), options), "whatever").await;

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"stage_describe") && names.contains(&"render_frame"));
    assert!(!names.contains(&"attr_set") && !names.contains(&"command_batch"));
    assert!(
        tools
            .iter()
            .all(|t| t.annotations.as_ref().and_then(|a| a.read_only_hint) == Some(true))
    );

    let text = text_of(&call(&client, "stage_describe", json!({})).await);
    assert!(
        text.contains("agent viewer") && text.contains("permission read-only"),
        "{text}"
    );

    // Hidden tools are still refused when called directly.
    for (name, args) in [
        (
            "attr_set",
            json!({"path": "/c", "attr": "width", "value": {"type": "int", "value": 10}}),
        ),
        ("prim_delete", json!({"path": "/c"})),
        ("sublayer_create", json!({"id": "agent/viewer"})),
        (
            "command_batch",
            json!({"commands": [{"command": "prim.delete", "params": {"path": "/c"}}]}),
        ),
        ("history_undo", json!({})),
    ] {
        let r = call(&client, name, args).await;
        assert_eq!(r.is_error, Some(true), "{name}");
        assert!(text_of(&r).contains("read-only"), "{name}: {}", text_of(&r));
    }
    let stage = &workspace.lock().project.stage;
    assert_eq!(bae_model::export_flattened_json(stage), before);
    assert_eq!(stage.layer_stack(), &[SublayerId::root()]);
    drop(client);
}

/// A client that speaks raw newline-delimited JSON-RPC, as Claude Code and Codex do
/// over stdio (protocol 2025-06-18 handshake).
#[tokio::test]
async fn hand_written_json_rpc() {
    let workspace = Workspace::new(Project::new()).shared();
    let server = BaeMcpServer::new(workspace, McpOptions::default());
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    let (read, mut write) = tokio::io::split(client_io);
    let mut lines = BufReader::new(read).lines();
    let mut send = async |msg: Json| {
        write
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .unwrap();
    };

    send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "claude-code", "version": "2.0.0"}
        }}),
    )
    .await;
    let init: Json = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("command_batch")
    );

    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await;
    send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await;
    let list: Json = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    let attr_set = tools.iter().find(|t| t["name"] == "attr_set").unwrap();
    assert_eq!(attr_set["inputSchema"]["type"], "object");
    assert!(attr_set["inputSchema"]["properties"]["value"].is_object());
    assert!(
        attr_set["description"]
            .as_str()
            .unwrap()
            .contains("static value")
    );

    send(
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "prim_create", "arguments": {"path": "/comp", "prim_type": "Comp"}
        }}),
    )
    .await;
    let created: Json = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(created["result"]["isError"], false);
    assert_eq!(
        created["result"]["structuredContent"]["output"]["path"],
        "/comp"
    );
    assert_eq!(
        created["result"]["structuredContent"]["sublayers"],
        json!(["agent/claude-code"])
    );

    send(
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
            "name": "no_such_tool", "arguments": {}
        }}),
    )
    .await;
    let unknown: Json = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(unknown["result"]["isError"], true);
}
