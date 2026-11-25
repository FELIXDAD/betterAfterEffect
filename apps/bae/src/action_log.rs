//! A read-only record of the actions the app runs, for test tools: with
//! `BAE_ACTION_LOG=<file>` every invocation the shell dispatches (shortcut, menu,
//! panel request) is appended to the file as one JSON line `{"t": <ms since
//! start>, "id": "<action>", "params": {…}}`. The macOS shortcut sweep
//! (`tools/qa/shortcut_sweep_mac.py`) presses real keys and reads it back.

use std::io::Write as _;
use std::time::Instant;

use bae_ui::Invocation;

pub struct ActionLog {
    file: Option<std::fs::File>,
    start: Instant,
}

impl ActionLog {
    /// The log `BAE_ACTION_LOG` names, or none.
    pub fn from_env() -> Self {
        let file = std::env::var_os("BAE_ACTION_LOG").and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| tracing::warn!("BAE_ACTION_LOG {path:?}: {e}"))
                .ok()
        });
        ActionLog {
            file,
            start: Instant::now(),
        }
    }

    /// Appends one dispatched invocation.
    pub fn write(&mut self, inv: &Invocation) {
        let Some(file) = &mut self.file else {
            return;
        };
        let line = serde_json::json!({
            "t": self.start.elapsed().as_millis() as u64,
            "id": inv.id,
            "params": inv.params,
        });
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_one_json_line_per_action() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("actions.jsonl");
        let mut log = ActionLog {
            file: Some(std::fs::File::create(&path).unwrap()),
            start: Instant::now(),
        };
        log.write(&Invocation::new("motion.duplicate"));
        log.write(&Invocation::with_params(
            "motion.new_layer",
            serde_json::json!({"kind": "solid"}),
        ));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["id"], "motion.duplicate");
        assert_eq!(lines[1]["params"]["kind"], "solid");
    }
}
