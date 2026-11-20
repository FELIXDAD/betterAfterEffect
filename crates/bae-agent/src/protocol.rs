//! MCP over JSON-RPC 2.0, without an async runtime.
//!
//! `bae mcp` only needs the tools part of MCP: `initialize`, `ping`, `tools/list`
//! and `tools/call` (plus ignoring notifications). Implementing that directly keeps
//! the release app ~1.4 MiB smaller than linking rmcp's service layer and Tokio
//! (measured; see the 17 PR). rmcp's client is still used in the tests to check
//! interoperability with the official SDK.
//!
//! [`BaeMcpServer::handle_message`] is transport independent: [`serve_io`] drives
//! it over newline-delimited stdio; the Streamable HTTP transport will reuse it.

use std::io::{self, BufRead, Write};

use serde_json::{Map, Value as Json, json};

use crate::server::BaeMcpServer;

/// Protocol revisions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC error codes.
pub mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
}

/// Pick the protocol revision for a client's requested one: the same if supported,
/// otherwise our newest (the client then decides whether to continue).
pub fn negotiate(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| PROTOCOL_VERSIONS.iter().find(|v| **v == r).copied())
        .unwrap_or(PROTOCOL_VERSIONS[0])
}

fn error(id: Json, code: i64, message: impl Into<String>) -> Json {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

impl BaeMcpServer {
    /// Handle one serialized JSON-RPC message (or batch); returns the serialized
    /// response, if any (notifications and client responses get none).
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let reply = match serde_json::from_str::<Json>(line) {
            Ok(msg) => self.handle_message(msg)?,
            Err(e) => error(Json::Null, code::PARSE_ERROR, format!("parse error: {e}")),
        };
        Some(reply.to_string())
    }

    /// Handle one JSON-RPC message or batch.
    pub fn handle_message(&self, msg: Json) -> Option<Json> {
        match msg {
            Json::Array(batch) => {
                if batch.is_empty() {
                    return Some(error(Json::Null, code::INVALID_REQUEST, "empty batch"));
                }
                let replies: Vec<Json> = batch
                    .into_iter()
                    .filter_map(|m| self.handle_single(m))
                    .collect();
                (!replies.is_empty()).then_some(Json::Array(replies))
            }
            other => self.handle_single(other),
        }
    }

    fn handle_single(&self, msg: Json) -> Option<Json> {
        let Json::Object(mut obj) = msg else {
            return Some(error(
                Json::Null,
                code::INVALID_REQUEST,
                "not a JSON-RPC object",
            ));
        };
        let id = obj.remove("id");
        let params = obj.remove("params").unwrap_or(Json::Null);
        let Some(method) = obj.get("method").and_then(Json::as_str) else {
            // A response to a request we never send, or garbage.
            if obj.contains_key("result") || obj.contains_key("error") {
                return None;
            }
            return Some(error(
                id.unwrap_or(Json::Null),
                code::INVALID_REQUEST,
                "missing method",
            ));
        };
        let Some(id) = id else {
            // Notifications (initialized, cancelled, progress …): nothing to do.
            return None;
        };
        Some(match self.request(method, params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((c, m)) => error(id, c, m),
        })
    }

    fn request(&self, method: &str, params: Json) -> Result<Json, (i64, String)> {
        match method {
            "initialize" => {
                let requested = params.get("protocolVersion").and_then(Json::as_str);
                let client = params.pointer("/clientInfo/name").and_then(Json::as_str);
                self.bind_agent(client)
                    .map_err(|e| (code::INVALID_PARAMS, e.to_string()))?;
                Ok(json!({
                    "protocolVersion": negotiate(requested),
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "bae",
                        "title": "BAE",
                        "version": env!("CARGO_PKG_VERSION"),
                        "description": "BAE motion graphics: read, edit (as a proposal) and render projects",
                    },
                    "instructions": self.instructions(),
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.tools_json() })),
            "tools/call" => {
                let name = params.get("name").and_then(Json::as_str).ok_or((
                    code::INVALID_PARAMS,
                    "tools/call needs a tool name".to_owned(),
                ))?;
                let args = params.get("arguments").cloned().unwrap_or(Json::Null);
                Ok(self.call(name, args).to_json())
            }
            other => Err((code::METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }

    fn tools_json(&self) -> Vec<Json> {
        self.tool_entries()
            .into_iter()
            .map(|t| {
                let mut annotations = Map::new();
                annotations.insert("readOnlyHint".into(), t.read_only.into());
                if !t.read_only {
                    annotations.insert("openWorldHint".into(), false.into());
                }
                json!({
                    "name": t.name,
                    "title": t.title,
                    "description": t.description,
                    "inputSchema": Json::Object((*t.input_schema).clone()),
                    "annotations": annotations,
                })
            })
            .collect()
    }
}

/// Serve newline-delimited JSON-RPC from `input` to `output` until end of input.
pub fn serve_io(
    server: &BaeMcpServer,
    input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            output.write_all(reply.as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{McpOptions, Workspace};

    fn server() -> BaeMcpServer {
        BaeMcpServer::new(
            Workspace::new(bae_model::Project::new()).shared(),
            McpOptions::default(),
        )
    }

    fn exchange(input: &str) -> Vec<Json> {
        let mut out = Vec::new();
        serve_io(&server(), input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn handshake_and_errors() {
        let replies = exchange(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01","capabilities":{},"clientInfo":{"name":"Some Client","version":"1"}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","id":"p","method":"ping"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#,
            "\n",
            "{not json\n",
            r#"[{"jsonrpc":"2.0","id":4,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":4}}]"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":9,"result":{}}"#,
            "\n",
        ));
        assert_eq!(replies.len(), 6, "{replies:?}");
        let init = &replies[0]["result"];
        assert_eq!(init["protocolVersion"], PROTOCOL_VERSIONS[0]);
        assert_eq!(init["serverInfo"]["name"], "bae");
        assert!(init["capabilities"]["tools"].is_object());
        assert_eq!(
            replies[1],
            json!({"jsonrpc": "2.0", "id": "p", "result": {}})
        );
        assert_eq!(replies[2]["error"]["code"], code::METHOD_NOT_FOUND);
        assert_eq!(replies[3]["error"]["code"], code::PARSE_ERROR);
        assert_eq!(replies[3]["id"], Json::Null);
        assert_eq!(
            replies[4],
            json!([{"jsonrpc": "2.0", "id": 4, "result": {}}])
        );
        assert_eq!(replies[5]["error"]["code"], code::INVALID_PARAMS);
    }

    #[test]
    fn negotiates_known_versions() {
        for v in PROTOCOL_VERSIONS {
            assert_eq!(negotiate(Some(v)), *v);
        }
        assert_eq!(negotiate(None), PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn tools_list_and_call() {
        let s = server();
        let list = s
            .handle_message(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
            .unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        let describe = tools
            .iter()
            .find(|t| t["name"] == "stage_describe")
            .unwrap();
        assert_eq!(describe["annotations"]["readOnlyHint"], true);
        assert_eq!(describe["inputSchema"]["type"], "object");
        let call = s
            .handle_message(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "prim_create", "arguments": {"path": "/c", "prim_type": "Comp"}}}))
            .unwrap();
        let result = &call["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["output"]["path"], "/c");
        assert_eq!(result["content"][0]["type"], "text");
    }
}
