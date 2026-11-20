//! Crash handling: panic hook, crash reports and emergency saves.
//!
//! [`install`] replaces the panic hook. On a panic (any thread) it:
//!
//! 1. writes a crash report (version, OS, thread, message, location, backtrace,
//!    context such as the GPU name, log file path) to the crash report directory,
//! 2. logs the panic and flushes the session log,
//! 3. runs every registered emergency-save callback (at most once per
//!    [`EMERGENCY_SAVE_INTERVAL`]), most recently registered first,
//! 4. records the report path so the next launch can tell the user
//!    ([`take_previous_crash`]),
//! 5. chains to the previous hook (which prints the usual message to stderr).
//!
//! Release builds use `panic = "unwind"`, so the hook always runs before the
//! process exits.
//!
//! # Emergency-save callbacks
//!
//! Register one per open project with [`register_emergency_save`]. Callbacks run
//! **inside the panic hook on the panicking thread**, so they must:
//!
//! * never panic (a panic inside the hook aborts the process immediately),
//! * never block on locks the crashed thread may hold — use `try_lock` or a
//!   lock-free snapshot of the document,
//! * write a *recovery copy* (e.g. `MyFilm.bae/recovery/…`), never overwrite the
//!   user's project,
//! * be fast (well under a second).

use std::any::Any;
use std::fmt::Write as _;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use crate::time::UtcTime;

/// Minimum time between two emergency-save runs (panics can cascade).
pub const EMERGENCY_SAVE_INTERVAL: Duration = Duration::from_secs(10);

/// Name of the marker file pointing at the latest crash report.
const LAST_CRASH_MARKER: &str = "last-crash.txt";

/// Crash reporter configuration.
#[derive(Debug, Clone)]
pub struct CrashConfig {
    /// Directory for crash reports (created on demand).
    pub reports_dir: PathBuf,
    /// Version line included in reports (see [`crate::version_line`]).
    pub version: String,
    /// Number of reports to keep (older ones are deleted when a new one is written).
    pub keep_reports: usize,
}

impl CrashConfig {
    /// Configuration writing to `reports_dir` with the current version line.
    pub fn new(reports_dir: impl Into<PathBuf>) -> Self {
        Self {
            reports_dir: reports_dir.into(),
            version: crate::version_line(),
            keep_reports: 20,
        }
    }
}

/// Result of one emergency-save callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyOutcome {
    /// Callback name.
    pub name: String,
    /// `Ok(description)` (e.g. the recovery file path) or `Err(message)`.
    pub result: Result<String, String>,
}

type SaveFn = Arc<dyn Fn() -> Result<String, String> + Send + Sync>;

struct SaveEntry {
    id: u64,
    name: String,
    save: SaveFn,
}

#[derive(Default)]
struct State {
    config: Option<CrashConfig>,
    saves: Vec<SaveEntry>,
    context: Vec<(String, String)>,
    last_report: Option<PathBuf>,
    last_save_run: Option<Instant>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static INSTALLED: AtomicBool = AtomicBool::new(false);
static IN_HOOK: AtomicBool = AtomicBool::new(false);
static NEXT_SAVE_ID: AtomicU64 = AtomicU64::new(1);

fn state() -> MutexGuard<'static, State> {
    STATE
        .get_or_init(|| Mutex::new(State::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// Like [`state`] but never blocks: the panicking thread may already hold the lock.
fn try_state() -> Option<MutexGuard<'static, State>> {
    let mutex = STATE.get_or_init(|| Mutex::new(State::default()));
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(poison)) => Some(poison.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

/// Installs the panic hook. Calling it again only updates the configuration.
pub fn install(config: CrashConfig) {
    state().config = Some(config);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // A panic while handling a panic (or two threads panicking at once): do the
        // minimum and let the previous hook print the message.
        if !IN_HOOK.swap(true, Ordering::SeqCst) {
            handle_panic(info);
            IN_HOOK.store(false, Ordering::SeqCst);
        }
        previous(info);
    }));
}

/// Adds a `key: value` line to every future crash report (e.g. `gpu`, `project`).
/// Setting an existing key replaces it.
pub fn set_context(key: impl Into<String>, value: impl Into<String>) {
    let (key, value) = (key.into(), value.into());
    let mut state = state();
    match state.context.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = value,
        None => state.context.push((key, value)),
    }
}

/// Registers an emergency-save callback; it stays registered until the returned
/// guard is dropped. See the module docs for the rules callbacks must follow.
#[must_use = "the callback is unregistered when the guard is dropped"]
pub fn register_emergency_save(
    name: impl Into<String>,
    save: impl Fn() -> Result<String, String> + Send + Sync + 'static,
) -> EmergencySaveGuard {
    let id = NEXT_SAVE_ID.fetch_add(1, Ordering::Relaxed);
    state().saves.push(SaveEntry {
        id,
        name: name.into(),
        save: Arc::new(save),
    });
    EmergencySaveGuard { id }
}

/// Unregisters its emergency-save callback when dropped.
#[derive(Debug)]
pub struct EmergencySaveGuard {
    id: u64,
}

impl Drop for EmergencySaveGuard {
    fn drop(&mut self) {
        state().saves.retain(|entry| entry.id != self.id);
    }
}

/// Runs all emergency-save callbacks now (newest first). Also useful on an OS
/// shutdown request or a fatal GPU error. Respects [`EMERGENCY_SAVE_INTERVAL`]
/// only when called from the panic hook.
pub fn run_emergency_saves() -> Vec<EmergencyOutcome> {
    let saves: Vec<(String, SaveFn)> = state()
        .saves
        .iter()
        .rev()
        .map(|entry| (entry.name.clone(), entry.save.clone()))
        .collect();
    run_saves(saves)
}

fn run_saves(saves: Vec<(String, SaveFn)>) -> Vec<EmergencyOutcome> {
    saves
        .into_iter()
        .map(|(name, save)| {
            // Outside the panic hook a panicking callback is caught; inside the hook
            // the runtime aborts on a nested panic (documented requirement).
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| save()))
                .unwrap_or_else(|payload| Err(format!("panicked: {}", payload_str(&*payload))));
            EmergencyOutcome { name, result }
        })
        .collect()
}

/// Path of the crash report written during this process, if any.
pub fn last_report() -> Option<PathBuf> {
    state().last_report.clone()
}

/// Returns the crash report left by a *previous* run (and clears the marker), so
/// the UI can offer to open it and the recovery files.
pub fn take_previous_crash(reports_dir: &Path) -> Option<PathBuf> {
    let marker = reports_dir.join(LAST_CRASH_MARKER);
    let text = std::fs::read_to_string(&marker).ok()?;
    let _ = std::fs::remove_file(&marker);
    let path = PathBuf::from(text.trim());
    path.exists().then_some(path)
}

fn handle_panic(info: &PanicHookInfo<'_>) {
    let message = payload_str(info.payload()).to_owned();
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".into());
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>").to_owned();

    tracing::error!(target: "bae::crash", thread = %thread_name, %location, "panic: {message}");

    // Snapshot state without blocking (this thread may hold the lock).
    let (config, context, saves) = match try_state() {
        Some(mut state) => {
            let now = Instant::now();
            let run_saves_now = state
                .last_save_run
                .is_none_or(|last| now.duration_since(last) >= EMERGENCY_SAVE_INTERVAL);
            if run_saves_now {
                state.last_save_run = Some(now);
            }
            let saves: Vec<(String, SaveFn)> = if run_saves_now {
                state
                    .saves
                    .iter()
                    .rev()
                    .map(|e| (e.name.clone(), e.save.clone()))
                    .collect()
            } else {
                Vec::new()
            };
            (state.config.clone(), state.context.clone(), saves)
        }
        None => (None, Vec::new(), Vec::new()),
    };

    let outcomes = run_saves(saves);
    crate::logging::flush();

    let Some(config) = config else { return };
    let report = format_report(&ReportInput {
        version: &config.version,
        thread: &thread_name,
        message: &message,
        location: &location,
        context: &context,
        outcomes: &outcomes,
        log_file: crate::logging::current_log_file(),
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
    });
    if let Some(path) = write_report(&config, &report) {
        eprintln!("BAE crashed. Crash report: {}", path.display());
        if let Some(mut state) = try_state() {
            state.last_report = Some(path);
        }
    }
}

struct ReportInput<'a> {
    version: &'a str,
    thread: &'a str,
    message: &'a str,
    location: &'a str,
    context: &'a [(String, String)],
    outcomes: &'a [EmergencyOutcome],
    log_file: Option<PathBuf>,
    backtrace: String,
}

fn format_report(input: &ReportInput<'_>) -> String {
    let mut out = String::with_capacity(4096 + input.backtrace.len());
    let _ = writeln!(out, "BAE crash report");
    let _ = writeln!(out, "time:     {}", UtcTime::now().rfc3339());
    let _ = writeln!(out, "version:  {}", input.version);
    let _ = writeln!(
        out,
        "os:       {} {} (family {})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    );
    let _ = writeln!(out, "pid:      {}", std::process::id());
    let _ = writeln!(out, "thread:   {}", input.thread);
    let _ = writeln!(out, "location: {}", input.location);
    if let Some(log) = &input.log_file {
        let _ = writeln!(out, "log:      {}", log.display());
    }
    for (key, value) in input.context {
        let _ = writeln!(out, "{key}: {value}");
    }
    let _ = writeln!(out, "\nmessage:\n{}\n", input.message);
    if !input.outcomes.is_empty() {
        let _ = writeln!(out, "emergency saves:");
        for outcome in input.outcomes {
            match &outcome.result {
                Ok(detail) => {
                    let _ = writeln!(out, "  ok    {}: {detail}", outcome.name);
                }
                Err(error) => {
                    let _ = writeln!(out, "  FAIL  {}: {error}", outcome.name);
                }
            }
        }
        out.push('\n');
    }
    let _ = writeln!(out, "backtrace:\n{}", input.backtrace);
    out
}

fn write_report(config: &CrashConfig, report: &str) -> Option<PathBuf> {
    let dir = &config.reports_dir;
    std::fs::create_dir_all(dir).ok()?;
    prune_reports(dir, config.keep_reports.saturating_sub(1));
    let path = dir.join(format!(
        "crash-{}-{}.txt",
        UtcTime::now().file_stamp(),
        std::process::id()
    ));
    std::fs::write(&path, report).ok()?;
    let _ = std::fs::write(
        dir.join(LAST_CRASH_MARKER),
        path.to_string_lossy().as_bytes(),
    );
    Some(path)
}

fn prune_reports(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut reports: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    if reports.len() <= keep {
        return;
    }
    reports.sort(); // names start with a sortable UTC stamp
    let excess = reports.len() - keep;
    for path in reports.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

fn payload_str(payload: &(dyn Any + Send)) -> &str {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        s
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s
    } else {
        "<non-string panic payload>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The panic hook is process-global; keep all hook-related assertions in one
    // test so parallel tests do not race on it.
    #[test]
    fn panic_writes_report_and_runs_emergency_saves() {
        let dir = tempfile::tempdir().unwrap();
        install(CrashConfig::new(dir.path()));
        set_context("gpu", "Test Adapter");
        set_context("gpu", "Test Adapter 2");

        let saved = Arc::new(AtomicBool::new(false));
        let saved2 = saved.clone();
        let guard = register_emergency_save("test-project", move || {
            saved2.store(true, Ordering::SeqCst);
            Ok("recovery/test.bae".into())
        });
        let _failing = register_emergency_save("failing", || Err("disk full".into()));

        let result = std::thread::Builder::new()
            .name("crash-test".into())
            .spawn(|| panic!("boom for the crash test"))
            .unwrap()
            .join();
        assert!(result.is_err());
        assert!(saved.load(Ordering::SeqCst), "emergency save ran");

        let report_path = last_report().expect("report written");
        let report = std::fs::read_to_string(&report_path).unwrap();
        assert!(report.contains("boom for the crash test"));
        assert!(report.contains("thread:   crash-test"));
        assert!(report.contains("gpu: Test Adapter 2"));
        assert!(!report.contains("gpu: Test Adapter\n"));
        assert!(report.contains("ok    test-project: recovery/test.bae"));
        assert!(report.contains("FAIL  failing: disk full"));
        assert!(report.contains("backtrace:"));

        // The next launch sees the report exactly once.
        assert_eq!(take_previous_crash(dir.path()), Some(report_path));
        assert_eq!(take_previous_crash(dir.path()), None);

        // Manual run (outside the hook) catches panicking callbacks.
        drop(guard);
        let _panicking = register_emergency_save("panicky", || panic!("save failed"));
        let outcomes = run_emergency_saves();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].name, "panicky");
        assert!(
            outcomes[0]
                .result
                .as_ref()
                .unwrap_err()
                .contains("save failed")
        );
    }

    #[test]
    fn prune_reports_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        for stamp in ["20260101-000000", "20260102-000000", "20260103-000000"] {
            std::fs::write(dir.path().join(format!("crash-{stamp}-1.txt")), "x").unwrap();
        }
        prune_reports(dir.path(), 1);
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, ["crash-20260103-000000-1.txt"]);
    }
}
