//! Multi-process tests: the test binary re-executes itself as a client.
//!
//! No extra binaries are shipped for this. A test file has one `#[test]` entry
//! that only acts when [`ROLE_VAR`] is set ([`role`]); in a normal test run it
//! returns at once. [`Proc::spawn`] starts `current_exe() --exact <entry>
//! --nocapture --test-threads=1` with that variable, so the child runs nothing
//! but the entry. Parent and child talk in lines: the parent writes commands to
//! the child's stdin, the child answers on stdout with lines marked by
//! [`PREFIX`] (libtest's own output around them is skipped). A child exits when
//! its stdin closes, so a parent that dies does not leave clients behind.
//!
//! Processes are killed with [`Child::kill`]: `TerminateProcess` on Windows,
//! `SIGKILL` on Unix; neither runs any cleanup in the child.
//!
//! Shared by `crates/bae-sync/tests/multiprocess_dir.rs` and
//! `services/bae-hub/tests/multiprocess.rs` (included there by path).
#![allow(dead_code)]

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use bae_model::SublayerId;
use bae_model::loro::VersionVector;
use bae_sync::DocSet;

/// Set in children: which client to run.
pub const ROLE_VAR: &str = "BAE_MP_ROLE";
/// Protocol lines on a child's stdout start with this.
pub const PREFIX: &str = "@mp ";

/// The role of this process when it was started by [`Proc::spawn`]; `None` in
/// the normal test run.
pub fn role() -> Option<String> {
    std::env::var(ROLE_VAR).ok()
}

/// A required environment variable of a child.
pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// A child process running one client.
pub struct Proc {
    pub name: String,
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    /// Protocol lines read while waiting for another kind.
    pending: VecDeque<String>,
}

impl Proc {
    /// Start this test binary again, running only the test `entry`, as `role`
    /// with `vars` set.
    pub fn spawn(entry: &str, name: &str, role: &str, vars: &[(&str, String)]) -> Proc {
        let exe = std::env::current_exe().expect("current test binary");
        let mut cmd = Command::new(exe);
        cmd.args(["--exact", entry, "--nocapture", "--test-threads=1"])
            .env(ROLE_VAR, role)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (k, v) in vars {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap_or_else(|e| panic!("start {name}: {e}"));
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("mp-{name}-out"))
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    // Not always at the start: libtest prints `test <name> ... `
                    // without a line break before running the test.
                    if let Some(at) = line.find(PREFIX)
                        && tx.send(line[at + PREFIX.len()..].to_owned()).is_err()
                    {
                        break;
                    }
                }
            })
            .expect("spawn reader thread");
        Proc {
            name: name.to_owned(),
            stdin: child.stdin.take(),
            child,
            lines,
            pending: VecDeque::new(),
        }
    }

    /// Send one command line.
    pub fn send(&mut self, cmd: &str) {
        let stdin = self.stdin.as_mut().expect("child running");
        writeln!(stdin, "{cmd}")
            .and_then(|()| stdin.flush())
            .unwrap_or_else(|e| panic!("{}: cannot send {cmd:?}: {e}", self.name));
    }

    /// The next answer of `kind` (its text after the kind), skipping (and
    /// keeping) answers of other kinds. Panics after `timeout` or when the
    /// child exits.
    pub fn wait_for(&mut self, kind: &str, timeout: Duration) -> String {
        if let Some(i) = self.pending.iter().position(|l| is_kind(l, kind)) {
            let line = self.pending.remove(i).expect("present");
            return rest_of(&line, kind);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) if is_kind(&line, kind) => return rest_of(&line, kind),
                Ok(line) => self.pending.push_back(line),
                Err(RecvTimeoutError::Timeout) => {
                    panic!("{}: no {kind:?} answer within {timeout:?}", self.name)
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let status = self.child.wait();
                    panic!(
                        "{}: exited ({status:?}) before answering {kind:?}",
                        self.name
                    )
                }
            }
        }
    }

    /// Send `cmd` and wait for the answer of `kind`.
    pub fn ask(&mut self, cmd: &str, kind: &str, timeout: Duration) -> String {
        self.send(cmd);
        self.wait_for(kind, timeout)
    }

    /// Kill the process (no cleanup runs in it) without waiting for it to be
    /// gone: what a crash looks like to everyone else.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        self.stdin = None;
    }

    /// Kill the process and wait until it is gone.
    pub fn kill_and_wait(&mut self) {
        self.kill();
        let _ = self.child.wait();
    }

    /// Wait for a process that was told to quit; panics if it does not exit
    /// within `timeout` or exits with a failure.
    pub fn wait_exit(&mut self, timeout: Duration) {
        self.stdin = None;
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    assert!(status.success(), "{} exited with {status}", self.name);
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                other => panic!("{}: did not exit within {timeout:?}: {other:?}", self.name),
            }
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}

fn is_kind(line: &str, kind: &str) -> bool {
    line == kind || line.strip_prefix(kind).is_some_and(|r| r.starts_with(' '))
}

fn rest_of(line: &str, kind: &str) -> String {
    line[kind.len()..].trim_start().to_owned()
}

/// `key=value` field of an answer (values have no spaces).
pub fn field<'a>(answer: &'a str, key: &str) -> &'a str {
    answer
        .split(' ')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no {key}= in {answer:?}"))
}

pub fn field_u64(answer: &str, key: &str) -> u64 {
    field(answer, key)
        .parse()
        .unwrap_or_else(|_| panic!("{key} is not a number in {answer:?}"))
}

// ---- in the child ----

/// What a child's main loop waits for.
pub enum Input {
    /// A command from the parent.
    Line(String),
    /// The transport has something (its `on_receive`).
    Wake,
    /// The parent closed stdin (it quit or died): exit.
    Eof,
}

/// Read the parent's commands on a thread; the sender is for wake-ups.
pub fn input() -> (Sender<Input>, Receiver<Input>) {
    let (tx, rx) = mpsc::channel();
    let lines = tx.clone();
    std::thread::Builder::new()
        .name("mp-stdin".into())
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if lines.send(Input::Line(line)).is_err() {
                    return;
                }
            }
            let _ = lines.send(Input::Eof);
        })
        .expect("spawn stdin thread");
    (tx, rx)
}

/// Answer the parent.
pub fn reply(kind: &str, text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{PREFIX}{kind} {text}");
    let _ = out.flush();
}

// ---- data passed between processes ----

/// Lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Short hash of a fingerprint (fingerprints are large).
pub fn digest(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..16].to_owned()
}

/// The version of every document, as one field value: `doc:hex,doc:hex`.
pub fn versions(docs: &dyn DocSet) -> String {
    let mut parts: Vec<String> = docs
        .doc_keys()
        .iter()
        .filter_map(|k| {
            Some(format!(
                "{}:{}",
                k.as_str(),
                hex(&docs.version(k)?.encode())
            ))
        })
        .collect();
    parts.sort();
    parts.join(",")
}

/// Parse [`versions`].
pub fn parse_versions(s: &str) -> BTreeMap<String, VersionVector> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (doc, vv) = p.rsplit_once(':').expect("doc:hex");
            (
                doc.to_owned(),
                VersionVector::decode(&unhex(vv)).expect("version vector"),
            )
        })
        .collect()
}

/// `have` contains everything in `want`, document by document.
pub fn includes(
    have: &BTreeMap<String, VersionVector>,
    want: &BTreeMap<String, VersionVector>,
) -> bool {
    want.iter()
        .all(|(doc, w)| w.is_empty() || have.get(doc).is_some_and(|h| h.includes_vv(w)))
}

/// Same versions (version vectors have no canonical encoding, so compare both
/// ways).
pub fn same_versions(a: &str, b: &str) -> bool {
    let (a, b) = (parse_versions(a), parse_versions(b));
    includes(&a, &b) && includes(&b, &a)
}

/// Write sublayer snapshots to `path` (temporary file + rename): the base
/// project the parent hands its children, or a client's saved state.
pub fn write_snapshots(path: &Path, layers: &[(SublayerId, Vec<u8>)]) {
    let mut out = Vec::new();
    for (id, bytes) in layers {
        for part in [id.as_str().as_bytes(), bytes.as_slice()] {
            out.extend_from_slice(&(part.len() as u64).to_le_bytes());
            out.extend_from_slice(part);
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &out).expect("write snapshots");
    std::fs::rename(&tmp, path).expect("rename snapshots");
}

/// Read [`write_snapshots`].
pub fn read_snapshots(path: &Path) -> Vec<(SublayerId, Vec<u8>)> {
    fn part(bytes: &[u8], at: &mut usize) -> Vec<u8> {
        let len = u64::from_le_bytes(bytes[*at..*at + 8].try_into().expect("8 bytes")) as usize;
        *at += 8 + len;
        bytes[*at - len..*at].to_vec()
    }
    let bytes = std::fs::read(path).expect("read snapshots");
    let mut at = 0;
    let mut layers = Vec::new();
    while at < bytes.len() {
        let id = String::from_utf8(part(&bytes, &mut at)).expect("utf-8 id");
        let data = part(&bytes, &mut at);
        layers.push((SublayerId::new(&id).expect("sublayer id"), data));
    }
    layers
}

/// Poll `f` until it holds; panics with `what` after `timeout`.
pub fn wait_until(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !f() {
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
