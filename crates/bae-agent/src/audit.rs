//! Audit log: one entry per agent tool call (ARCHITECTURE §10 "所有操作都有审计记录").
//!
//! Entries are kept in memory (the newest [`AuditLog::DEFAULT_CAPACITY`]) for the
//! Agent panel and can also be appended to a JSON Lines file.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use bae_model::{AuthorId, SublayerId};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// How a call ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuditOutcome {
    Ok,
    /// The call failed (bad parameters, model error); nothing was changed.
    Error {
        message: String,
    },
    /// Refused by the agent's permission level; nothing was changed.
    Denied {
        message: String,
    },
}

impl AuditOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, AuditOutcome::Ok)
    }
}

/// One tool call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Increasing within one log.
    pub seq: u64,
    /// Wall-clock time the call started, milliseconds since the Unix epoch.
    pub unix_ms: u64,
    /// Agent name (e.g. `claude-code`).
    pub agent: String,
    pub author: AuthorId,
    /// Canonical tool ID (`stage.describe`, `attr.set`, `command.batch` …).
    pub tool: String,
    /// Arguments as received; very large ones are replaced by a size note.
    pub params: Json,
    #[serde(flatten)]
    pub outcome: AuditOutcome,
    /// Sublayers the call wrote.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sublayers: Vec<SublayerId>,
    /// Number of changes in the resulting stage event.
    #[serde(default)]
    pub changes: usize,
    pub duration_us: u64,
}

/// Parameters larger than this (serialized) are not stored verbatim.
const MAX_PARAMS_BYTES: usize = 16 * 1024;

/// Shrink oversized parameters to a note, keeping small ones verbatim.
pub fn clip_params(params: &Json) -> Json {
    let size = serde_json::to_string(params).map_or(0, |s| s.len());
    if size <= MAX_PARAMS_BYTES {
        params.clone()
    } else {
        serde_json::json!({ "omitted": true, "bytes": size })
    }
}

pub fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

struct Inner {
    entries: VecDeque<AuditEntry>,
    next_seq: u64,
    sink: Option<File>,
}

/// Thread-safe audit log.
pub struct AuditLog {
    inner: Mutex<Inner>,
    capacity: usize,
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("AuditLog")
            .field("entries", &inner.entries.len())
            .field("file", &inner.sink.is_some())
            .finish()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLog {
    pub const DEFAULT_CAPACITY: usize = 10_000;

    /// In-memory only.
    pub fn new() -> Self {
        AuditLog {
            inner: Mutex::new(Inner {
                entries: VecDeque::new(),
                next_seq: 1,
                sink: None,
            }),
            capacity: Self::DEFAULT_CAPACITY,
        }
    }

    /// Also append every entry to `path` as JSON Lines (the file is created if
    /// missing, parent directories included).
    pub fn with_file(path: &Path) -> io::Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let log = Self::new();
        log.inner.lock().sink = Some(file);
        Ok(log)
    }

    /// Append an entry; `seq` is assigned here. Returns the stored entry.
    pub fn record(&self, mut entry: AuditEntry) -> AuditEntry {
        let mut inner = self.inner.lock();
        entry.seq = inner.next_seq;
        inner.next_seq += 1;
        if let Some(file) = inner.sink.as_mut() {
            let line = serde_json::to_string(&entry).unwrap_or_default();
            if let Err(e) = writeln!(file, "{line}").and_then(|()| file.flush()) {
                tracing::warn!("audit log write failed: {e}");
            }
        }
        if inner.entries.len() == self.capacity {
            inner.entries.pop_front();
        }
        inner.entries.push_back(entry.clone());
        entry
    }

    /// Entries in order, oldest first.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.inner.lock().entries.iter().cloned().collect()
    }

    /// Entries with `seq > after` (for incremental polling by the UI).
    pub fn entries_since(&self, after: u64) -> Vec<AuditEntry> {
        self.inner
            .lock()
            .entries
            .iter()
            .filter(|e| e.seq > after)
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tool: &str) -> AuditEntry {
        AuditEntry {
            seq: 0,
            unix_ms: unix_ms_now(),
            agent: "t".into(),
            author: AuthorId::new("agent:t").unwrap(),
            tool: tool.into(),
            params: serde_json::json!({}),
            outcome: AuditOutcome::Ok,
            sublayers: vec![],
            changes: 0,
            duration_us: 1,
        }
    }

    #[test]
    fn records_and_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs/audit.jsonl");
        let log = AuditLog::with_file(&path).unwrap();
        log.record(entry("stage.describe"));
        let mut denied = entry("attr.set");
        denied.outcome = AuditOutcome::Denied {
            message: "no".into(),
        };
        log.record(denied);
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries_since(1)[0].tool, "attr.set");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<AuditEntry> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].seq, 2);
        assert!(matches!(lines[1].outcome, AuditOutcome::Denied { .. }));
        assert!(text.contains("\"status\":\"denied\""));
    }

    #[test]
    fn clips_large_params() {
        let big = serde_json::json!({ "s": "x".repeat(MAX_PARAMS_BYTES) });
        assert_eq!(clip_params(&big)["omitted"], true);
        let small = serde_json::json!({ "a": 1 });
        assert_eq!(clip_params(&small), small);
    }
}
