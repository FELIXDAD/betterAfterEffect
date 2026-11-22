//! `tracing` initialization: stderr + per-session log file.
//!
//! * Filter directives come from (highest priority first) [`LogOptions::filter`]
//!   (`--log`), `BAE_LOG`, `RUST_LOG`, then [`DEFAULT_FILTER`]. The syntax is
//!   `tracing_subscriber::filter::Targets`: `info,bae_gpu=debug,wgpu_core=warn`.
//!   (We deliberately avoid `EnvFilter`, which pulls in a regex engine.)
//! * Each run writes `bae-<UTC timestamp>-<pid>.log` in the log directory. Writing
//!   happens on a background thread so a slow disk never stalls the UI or render
//!   threads; the panic hook calls [`flush`] so the last lines before a crash land
//!   on disk. A session file that exceeds [`LogOptions::max_file_bytes`] continues
//!   in `….1.log`, `….2.log`. Old files beyond [`LogOptions::keep_files`] are
//!   deleted at startup.
//! * Records from the `log` crate (wgpu, winit, egui) are forwarded to `tracing`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

use poison_free::Mutex;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::time::UtcTime;

/// Filter used when nothing else is configured. wgpu/naga are noisy at `info`.
pub const DEFAULT_FILTER: &str =
    "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,egui_wgpu=warn,winit=warn";

/// Environment variable with filter directives (takes precedence over `RUST_LOG`).
pub const LOG_ENV: &str = "BAE_LOG";

/// Prefix of session log file names.
pub const FILE_PREFIX: &str = "bae-";

/// Logging options.
#[derive(Debug, Clone)]
pub struct LogOptions {
    /// Explicit filter directives (e.g. from `--log`); overrides the environment.
    pub filter: Option<String>,
    /// Write human-readable logs to stderr.
    pub stderr: bool,
    /// Colorize stderr; `None` = only when stderr is a terminal.
    pub stderr_ansi: Option<bool>,
    /// Directory for the session log file; `None` disables file logging.
    pub file_dir: Option<PathBuf>,
    /// Number of session log files to keep (older ones are deleted at startup).
    pub keep_files: usize,
    /// Size after which the session continues in a new numbered file.
    pub max_file_bytes: u64,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            filter: None,
            stderr: true,
            stderr_ansi: None,
            file_dir: None,
            keep_files: 20,
            max_file_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Errors during logging setup.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// Filter directives could not be parsed.
    #[error("invalid log filter {filter:?}: {message}")]
    InvalidFilter {
        /// The offending directives.
        filter: String,
        /// Parser message.
        message: String,
    },
    /// The log directory or file could not be created.
    #[error("cannot open log file in {dir}: {source}")]
    File {
        /// Log directory.
        dir: PathBuf,
        /// Underlying error.
        source: io::Error,
    },
    /// A global subscriber was already installed.
    #[error("logging is already initialized")]
    AlreadyInitialized,
}

/// Keeps logging alive; dropping it flushes and stops the file writer thread.
#[must_use = "dropping the guard stops file logging"]
#[derive(Debug)]
pub struct LogGuard {
    file: Option<PathBuf>,
}

impl LogGuard {
    /// The current session's log file, if file logging is enabled.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        shutdown();
    }
}

/// Resolves the effective filter string (see module docs).
pub fn effective_filter(explicit: Option<&str>) -> String {
    explicit
        .map(str::to_owned)
        .or_else(|| std::env::var(LOG_ENV).ok().filter(|v| !v.trim().is_empty()))
        .or_else(|| {
            std::env::var("RUST_LOG")
                .ok()
                .filter(|v| !v.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_FILTER.to_owned())
}

/// Parses filter directives.
pub fn parse_filter(filter: &str) -> Result<Targets, LogError> {
    Targets::from_str(filter).map_err(|err| LogError::InvalidFilter {
        filter: filter.to_owned(),
        message: err.to_string(),
    })
}

/// Installs the global `tracing` subscriber.
pub fn init(options: &LogOptions) -> Result<LogGuard, LogError> {
    let filter_str = effective_filter(options.filter.as_deref());
    let targets = parse_filter(&filter_str)?;

    let file_sink = match &options.file_dir {
        Some(dir) => Some(FileSink::start(dir, options)?),
        None => None,
    };
    let file_path = file_sink.as_ref().map(|sink| sink.first_path.clone());

    let stderr_layer = options.stderr.then(|| {
        let ansi = options
            .stderr_ansi
            .unwrap_or_else(|| io::stderr().is_terminal());
        tracing_subscriber::fmt::layer()
            .with_writer(io::stderr)
            .with_ansi(ansi)
            .with_target(true)
    });
    let file_layer = file_sink.map(|sink| {
        let writer = SinkWriter(sink.install());
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_thread_names(true)
            .with_target(true)
    });

    tracing_subscriber::registry()
        .with(targets)
        .with(stderr_layer)
        .with(file_layer)
        .try_init()
        .map_err(|_| LogError::AlreadyInitialized)?;

    tracing::debug!(filter = %filter_str, file = ?file_path, "logging initialized");
    Ok(LogGuard { file: file_path })
}

/// Path of the current session log file (if file logging is active).
pub fn current_log_file() -> Option<PathBuf> {
    SINK.get()
        .and_then(|sink| sink.lock().as_ref().map(|s| s.current_path()))
}

/// Blocks (up to one second) until all queued log lines are written to disk.
/// Safe to call from a panic hook.
pub fn flush() {
    let sender = SINK
        .get()
        .and_then(|sink| sink.lock().as_ref().map(|s| s.sender.clone()));
    if let Some(sender) = sender {
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        if sender.send(Message::Flush(done_tx)).is_ok() {
            let _ = done_rx.recv_timeout(Duration::from_secs(1));
        }
    }
}

/// Removes session log files beyond the newest `keep` (by modification time).
pub fn prune_logs(dir: &Path, keep: usize) -> io::Result<usize> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(FILE_PREFIX) && name.ends_with(".log")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect();
    if files.len() <= keep {
        return Ok(0);
    }
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified)); // newest first
    let mut removed = 0;
    for (_, path) in files.into_iter().skip(keep) {
        if fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn shutdown() {
    let state = SINK.get().and_then(|sink| sink.lock().take());
    if let Some(state) = state {
        let _ = state.sender.send(Message::Shutdown);
        if let Some(handle) = state.thread {
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Background file writer
// ---------------------------------------------------------------------------

/// A std mutex that ignores poisoning: logging must keep working after a panic.
mod poison_free {
    pub(crate) struct Mutex<T>(std::sync::Mutex<T>);
    impl<T> Mutex<T> {
        pub(crate) const fn new(value: T) -> Self {
            Self(std::sync::Mutex::new(value))
        }
        pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            // A poisoned log mutex must never take the app down.
            self.0.lock().unwrap_or_else(|poison| poison.into_inner())
        }
    }
}

enum Message {
    Line(Vec<u8>),
    Flush(SyncSender<()>),
    Shutdown,
}

struct SinkState {
    sender: SyncSender<Message>,
    thread: Option<JoinHandle<()>>,
    path_info: std::sync::Arc<Mutex<PathBuf>>,
}

impl SinkState {
    fn current_path(&self) -> PathBuf {
        self.path_info.lock().clone()
    }
}

static SINK: OnceLock<Mutex<Option<SinkState>>> = OnceLock::new();
static DROPPED_LINES: AtomicU64 = AtomicU64::new(0);

/// Queue capacity in lines; when full, lines are dropped (counted and reported)
/// rather than blocking the logging thread.
const QUEUE_CAPACITY: usize = 16 * 1024;

struct FileSink {
    first_path: PathBuf,
    state: SinkState,
}

impl FileSink {
    fn start(dir: &Path, options: &LogOptions) -> Result<Self, LogError> {
        let to_err = |source| LogError::File {
            dir: dir.to_owned(),
            source,
        };
        fs::create_dir_all(dir).map_err(to_err)?;
        // Keep `keep_files - 1` old files plus the one we are about to create.
        let _ = prune_logs(dir, options.keep_files.saturating_sub(1));

        let stem = format!(
            "{FILE_PREFIX}{}-{}",
            UtcTime::now().file_stamp(),
            std::process::id()
        );
        let first_path = dir.join(format!("{stem}.log"));
        let file = open_log(&first_path).map_err(to_err)?;
        let path_info = std::sync::Arc::new(Mutex::new(first_path.clone()));

        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let worker = Worker {
            dir: dir.to_owned(),
            stem,
            part: 0,
            writer: BufWriter::with_capacity(64 * 1024, file),
            written: 0,
            max_bytes: options.max_file_bytes.max(64 * 1024),
            path_info: path_info.clone(),
        };
        let thread = std::thread::Builder::new()
            .name("bae-log-writer".into())
            .spawn(move || worker.run(receiver))
            .map_err(to_err)?;
        Ok(Self {
            first_path,
            state: SinkState {
                sender,
                thread: Some(thread),
                path_info,
            },
        })
    }

    /// Publishes the sink globally (for flush/shutdown) and returns its sender.
    fn install(self) -> SyncSender<Message> {
        let sender = self.state.sender.clone();
        let slot = SINK.get_or_init(|| Mutex::new(None));
        *slot.lock() = Some(self.state);
        sender
    }
}

fn open_log(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

struct Worker {
    dir: PathBuf,
    stem: String,
    part: u32,
    writer: BufWriter<File>,
    written: u64,
    max_bytes: u64,
    path_info: std::sync::Arc<Mutex<PathBuf>>,
}

impl Worker {
    fn run(mut self, receiver: Receiver<Message>) {
        loop {
            // Block for the next message, but flush whenever the queue drains so
            // lines reach the disk promptly without a syscall per line.
            let message = match receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => {
                    let _ = self.writer.flush();
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            };
            match message {
                Message::Line(line) => {
                    self.write_line(&line);
                    while let Ok(next) = receiver.try_recv() {
                        match next {
                            Message::Line(line) => self.write_line(&line),
                            Message::Flush(done) => {
                                self.flush();
                                let _ = done.try_send(());
                            }
                            Message::Shutdown => {
                                self.flush();
                                return;
                            }
                        }
                    }
                    let _ = self.writer.flush();
                }
                Message::Flush(done) => {
                    self.flush();
                    let _ = done.try_send(());
                }
                Message::Shutdown => break,
            }
        }
        self.flush();
    }

    fn flush(&mut self) {
        let _ = self.writer.flush();
        let _ = self.writer.get_ref().sync_data();
    }

    fn write_line(&mut self, line: &[u8]) {
        let dropped = DROPPED_LINES.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            let note = format!("[bae-log] {dropped} log lines dropped (queue full)\n");
            self.raw_write(note.as_bytes());
        }
        if self.written + line.len() as u64 > self.max_bytes && self.written > 0 {
            self.rotate();
        }
        self.raw_write(line);
    }

    fn raw_write(&mut self, bytes: &[u8]) {
        if self.writer.write_all(bytes).is_ok() {
            self.written += bytes.len() as u64;
        }
    }

    fn rotate(&mut self) {
        self.part += 1;
        let path = self.dir.join(format!("{}.{}.log", self.stem, self.part));
        if let Ok(file) = open_log(&path) {
            let _ = self.writer.flush();
            self.writer = BufWriter::with_capacity(64 * 1024, file);
            self.written = 0;
            *self.path_info.lock() = path;
        }
    }
}

/// `MakeWriter` that buffers one formatted event and enqueues it on drop.
#[derive(Clone)]
struct SinkWriter(SyncSender<Message>);

struct LineBuffer {
    sender: SyncSender<Message>,
    buf: Vec<u8>,
}

impl Write for LineBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LineBuffer {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let line = std::mem::take(&mut self.buf);
        match self.sender.try_send(Message::Line(line)) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(_)) => {
                DROPPED_LINES.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl<'a> MakeWriter<'a> for SinkWriter {
    type Writer = LineBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        LineBuffer {
            sender: self.0.clone(),
            buf: Vec::with_capacity(256),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_precedence_and_parsing() {
        assert_eq!(effective_filter(Some("debug")), "debug");
        assert!(parse_filter(DEFAULT_FILTER).is_ok());
        assert!(parse_filter("info,bae_gpu=trace").is_ok());
        assert!(matches!(
            parse_filter("bae_gpu=loud=1"),
            Err(LogError::InvalidFilter { .. })
        ));
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            let path = dir.path().join(format!("{FILE_PREFIX}{i}.log"));
            fs::write(&path, b"x").unwrap();
            let time = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1000 + i);
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        }
        fs::write(dir.path().join("other.txt"), b"keep").unwrap();
        assert_eq!(prune_logs(dir.path(), 2).unwrap(), 3);
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["bae-3.log", "bae-4.log", "other.txt"]);
    }

    #[test]
    fn worker_writes_rotates_and_flushes() {
        let dir = tempfile::tempdir().unwrap();
        let options = LogOptions {
            max_file_bytes: 64 * 1024,
            ..LogOptions::default()
        };
        let sink = FileSink::start(dir.path(), &options).unwrap();
        let first = sink.first_path.clone();
        let sender = sink.state.sender.clone();
        let writer = SinkWriter(sender.clone());
        let line = vec![b'a'; 1023];
        for _ in 0..100 {
            let mut w = writer.make_writer();
            w.write_all(&line).unwrap();
            w.write_all(b"\n").unwrap();
        }
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        sender.send(Message::Flush(done_tx)).unwrap();
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let rotated = sink.state.current_path();
        assert_ne!(rotated, first, "100 KiB with a 64 KiB limit must rotate");
        let total: u64 = [&first, &rotated]
            .iter()
            .map(|p| fs::metadata(p).unwrap().len())
            .sum();
        assert_eq!(total, 100 * 1024);
        sender.send(Message::Shutdown).unwrap();
        let mut state = sink.state;
        state.thread.take().unwrap().join().unwrap();
    }
}
