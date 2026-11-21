//! The Illustrator export script (`bridges/ai/bae-ai-export.jsx`) without
//! Illustrator: `bridges/ai/tests/harness.js` runs it against a small mock of
//! Illustrator's scripting DOM with ES3 builtins only. It needs a JavaScript
//! shell, node (any platform) or macOS's built-in jsc; with neither the test is
//! skipped and says why. `bridges/ai/tests/run.sh` is the same thing from a shell.
//!
//! The run against real Illustrator is `live_illustrator` in `illustrator.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bridge_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bridges")
        .join("ai")
}

const JSC: &str =
    "/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc";

/// The command that runs `harness.js` from `bridges/ai/tests`, or why there is none.
fn js_shell() -> Result<Command, String> {
    let node = std::env::var_os("BAE_NODE").unwrap_or_else(|| "node".into());
    if Command::new(&node).arg("--version").output().is_ok() {
        let mut c = Command::new(node);
        c.args(["node-run.js", "harness.js"]);
        return Ok(c);
    }
    if Path::new(JSC).exists() {
        let mut c = Command::new(JSC);
        c.arg("harness.js");
        return Ok(c);
    }
    Err("no JavaScript shell: install node (or set BAE_NODE), or run on macOS (jsc)".into())
}

#[test]
fn export_script_against_mock_dom() {
    let mut shell = match js_shell() {
        Ok(c) => c,
        Err(why) => {
            eprintln!("skipped: {why}");
            return;
        }
    };
    let out = shell
        .current_dir(bridge_dir().join("tests"))
        .output()
        .expect("run the harness");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // jsc's quit() always exits 0, so the summary line decides.
    let summary = text
        .lines()
        .find(|l| l.ends_with(" failed") && l.contains(" checks, "));
    assert!(
        out.status.success() && summary.is_some_and(|l| l.ends_with(" checks, 0 failed")),
        "{:?} ({}):\n{text}",
        shell.get_program(),
        out.status
    );
    eprintln!("{}", summary.unwrap());
}

/// Without a byte order mark ExtendScript does not promise to read a script as
/// UTF-8 (it may use the system encoding, which differs between machines), so
/// scripts that run in Illustrator stay ASCII and write layer names and CJK
/// text in fixtures as `\u` escapes.
#[test]
fn illustrator_scripts_are_ascii() {
    let dir = bridge_dir();
    let mut checked = 0;
    for sub in [dir.clone(), dir.join("tests")] {
        for entry in std::fs::read_dir(&sub).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "jsx") {
                let bytes = std::fs::read(&path).unwrap();
                if let Some(at) = bytes.iter().position(|b| !b.is_ascii()) {
                    let line = bytes[..at].iter().filter(|b| **b == b'\n').count() + 1;
                    panic!("{}:{line}: non-ASCII byte", path.display());
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 3, "found only {checked} .jsx files");
}
