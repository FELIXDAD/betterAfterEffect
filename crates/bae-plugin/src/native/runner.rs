//! Out-of-process mode (spec §15): a native library runs in a child process
//! (`bae plugin runner`, or any executable calling [`runner_main`]), so a
//! crash or hang in the plugin cannot take the application down.
//!
//! * Control: length-prefixed JSON messages over the child's stdin/stdout
//!   (one request in flight; responses carry the plugin's log lines). The
//!   runner moves its stdout away first, so `printf` in a plugin goes to
//!   stderr instead of corrupting the channel.
//! * Frames: input and output pixels live in a shared memory file mapped by
//!   both processes (grown on demand, reused across frames).
//! * Crashes and hangs: EOF on the channel, or no reply within the watchdog
//!   time (render 10 s, load 30 s), fails the call; the runner is killed and
//!   restarted lazily on the next call. Three crashes within 60 s disable the
//!   plugin for the session ([`CallError::Crashed`] with `disabled`).

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use memmap2::MmapMut;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::library::Library;
use super::pixels;
use super::{CallError, FrameRequest, LibraryInfo, PreRenderOut, RenderOut, rect};

/// Largest control message accepted (pixels never go through the channel).
const MAX_MESSAGE: usize = 64 << 20;
/// Watchdog for loading a library (spec §15.5: non-render calls 30 s).
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// Watchdog for render calls (spec §15.5: 10 s).
const RENDER_TIMEOUT: Duration = Duration::from_secs(10);
/// Crashes within [`CRASH_WINDOW`] that disable the plugin for the session.
const MAX_CRASHES: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(60);

/// How the host starts a runner process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerCommand {
    /// The executable.
    pub program: PathBuf,
    /// Arguments before nothing else (`["plugin", "runner"]` for `bae`).
    pub args: Vec<OsString>,
}

impl RunnerCommand {
    /// This executable as `bae plugin runner` (the application binary).
    pub fn current_exe() -> io::Result<Self> {
        Ok(Self {
            program: std::env::current_exe()?,
            args: vec!["plugin".into(), "runner".into()],
        })
    }

    /// A dedicated runner executable taking no arguments.
    pub fn program(path: impl Into<PathBuf>) -> Self {
        Self {
            program: path.into(),
            args: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Request {
    Load {
        library: PathBuf,
        plugin_id: String,
        plugin_dir: PathBuf,
        abi: u32,
    },
    PreRender(FrameRequest),
    Render {
        frame: FrameRequest,
        /// A new shared memory file to map (first frame, or after growing).
        shm: Option<ShmSpec>,
        /// Where the output image starts in the shared memory.
        output_offset: usize,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShmSpec {
    path: PathBuf,
    len: usize,
}

#[derive(Debug, Serialize, Deserialize)]
enum Reply {
    Loaded(LibraryInfo),
    PreRendered(PreRenderOut),
    Rendered(RenderOut),
    Done,
    Failed { status: i32, message: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    reply: Reply,
    logs: Vec<(u32, String)>,
}

fn write_message(w: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

fn read_message(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Runner process
// ---------------------------------------------------------------------------

/// The protocol output: the original stdout, after stdout itself was pointed
/// at stderr.
fn protocol_output() -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        // SAFETY: plain descriptor calls; the duplicate is owned by the File.
        unsafe {
            let fd = libc::dup(1);
            if fd < 0 || libc::dup2(2, 1) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(File::from_raw_fd(fd))
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };
        // SAFETY: plain Win32 calls; the stdout handle is owned by the File.
        unsafe {
            let out = GetStdHandle(STD_OUTPUT_HANDLE);
            let err = GetStdHandle(STD_ERROR_HANDLE);
            if out.is_null() || SetStdHandle(STD_OUTPUT_HANDLE, err) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(File::from_raw_handle(out.cast()))
        }
    }
}

/// Main loop of a runner process: serves one native library over
/// stdin/stdout until the host says `Shutdown` or goes away.
pub fn runner_main() -> ExitCode {
    let mut output = match protocol_output() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("bae plugin runner: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut input = io::stdin().lock();
    let mut library: Option<Library> = None;
    let mut shm: Option<MmapMut> = None;
    loop {
        let Ok(bytes) = read_message(&mut input) else {
            return ExitCode::SUCCESS;
        };
        let request: Request = match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("bae plugin runner: bad request: {e}");
                return ExitCode::FAILURE;
            }
        };
        let shutdown = matches!(request, Request::Shutdown);
        let reply = serve(request, &mut library, &mut shm);
        let logs = library.as_ref().map(Library::take_logs).unwrap_or_default();
        if write_message(&mut output, &Response { reply, logs }).is_err() || shutdown {
            drop(library);
            return ExitCode::SUCCESS;
        }
    }
}

fn failed(e: CallError) -> Reply {
    match e {
        CallError::Status { status, message } => Reply::Failed { status, message },
        other => Reply::Failed {
            status: bae_plugin_sdk::abi::BAE_ERR_INTERNAL,
            message: other.to_string(),
        },
    }
}

fn serve(request: Request, library: &mut Option<Library>, shm: &mut Option<MmapMut>) -> Reply {
    let internal = |m: String| Reply::Failed {
        status: bae_plugin_sdk::abi::BAE_ERR_INTERNAL,
        message: m,
    };
    match request {
        Request::Load {
            library: path,
            plugin_id,
            plugin_dir,
            abi,
        } => {
            let caps = bae_plugin_sdk::abi::BAE_HOST_CAP_OUT_OF_PROCESS;
            match Library::load(&path, &plugin_id, &plugin_dir, abi, caps) {
                Ok(lib) => {
                    let info = lib.info().clone();
                    *library = Some(lib);
                    Reply::Loaded(info)
                }
                Err(e) => internal(e),
            }
        }
        Request::PreRender(frame) => match library {
            Some(lib) => lib
                .pre_render(&frame)
                .map_or_else(failed, Reply::PreRendered),
            None => internal("no library loaded".into()),
        },
        Request::Render {
            frame,
            shm: spec,
            output_offset,
        } => {
            if let Some(spec) = spec {
                match map_file(&spec.path, spec.len) {
                    Ok(m) => *shm = Some(m),
                    Err(e) => return internal(format!("cannot map shared memory: {e}")),
                }
            }
            let (Some(lib), Some(mem)) = (library.as_ref(), shm.as_mut()) else {
                return internal("no library or shared memory".into());
            };
            let input_len = pixels::image_len(frame.format, rect(frame.input));
            let output_len = pixels::image_len(frame.format, rect(frame.request));
            if output_offset < input_len || output_offset + output_len > mem.len() {
                return internal("frame does not fit the shared memory".into());
            }
            let (input, rest) = mem.split_at_mut(output_offset);
            lib.render(&frame, &input[..input_len], &mut rest[..output_len])
                .map_or_else(failed, Reply::Rendered)
        }
        Request::Shutdown => {
            *library = None;
            Reply::Done
        }
    }
}

fn map_file(path: &Path, len: usize) -> io::Result<MmapMut> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    if (file.metadata()?.len() as usize) < len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "shared memory file too small",
        ));
    }
    // SAFETY: the file is private to this host/runner pair; both sides only
    // touch disjoint ranges while a request is in flight.
    unsafe { MmapMut::map_mut(&file) }
}

// ---------------------------------------------------------------------------
// Host side
// ---------------------------------------------------------------------------

/// Shared memory owned by the host: a temporary file mapped read/write.
struct Shm {
    path: PathBuf,
    map: MmapMut,
}

impl Shm {
    fn create(len: usize) -> io::Result<Shm> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bae-plugin-{}-{}.shm",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.set_len(len as u64)?;
        // SAFETY: a fresh private file; see `map_file`.
        let map = unsafe { MmapMut::map_mut(&file) }?;
        Ok(Shm { path, map })
    }
}

impl Drop for Shm {
    fn drop(&mut self) {
        // Best effort: on Windows the runner may still map it until it exits.
        let _ = std::fs::remove_file(&self.path);
    }
}

struct Runner {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<io::Result<Vec<u8>>>,
    /// Whether the runner mapped the current `ClientState::shm`.
    shm_mapped: bool,
}

impl Runner {
    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct ClientState {
    runner: Option<Runner>,
    shm: Option<Shm>,
    crashes: Vec<Instant>,
    disabled: bool,
}

/// A native library served by a runner process.
pub struct RunnerClient {
    command: RunnerCommand,
    library: PathBuf,
    plugin_id: String,
    plugin_dir: PathBuf,
    abi: u32,
    state: Mutex<ClientState>,
}

impl std::fmt::Debug for RunnerClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerClient")
            .field("plugin", &self.plugin_id)
            .finish_non_exhaustive()
    }
}

impl RunnerClient {
    /// Starts a runner and loads `library` in it.
    pub fn start(
        command: RunnerCommand,
        library: &Path,
        plugin_id: &str,
        plugin_dir: &Path,
        abi: u32,
    ) -> Result<(RunnerClient, LibraryInfo), CallError> {
        let client = RunnerClient {
            command,
            library: library.to_owned(),
            plugin_id: plugin_id.to_owned(),
            plugin_dir: plugin_dir.to_owned(),
            abi,
            state: Mutex::new(ClientState::default()),
        };
        let info = {
            let mut state = client.state.lock();
            client.spawn(&mut state)?
        };
        Ok((client, info))
    }

    /// Spawns the runner and loads the library; returns what it exported.
    fn spawn(&self, state: &mut ClientState) -> Result<LibraryInfo, CallError> {
        let mut cmd = Command::new(&self.command.program);
        cmd.args(&self.command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| {
            CallError::Failed(format!(
                "cannot start the plugin runner `{}`: {e}",
                self.command.program.display()
            ))
        })?;
        let (Some(stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(CallError::Failed("plugin runner without pipes".into()));
        };
        let (tx, replies) = channel();
        let spawned = std::thread::Builder::new()
            .name(format!("bae-plugin-runner:{}", self.plugin_id))
            .spawn(move || {
                loop {
                    let message = read_message(&mut stdout);
                    let end = message.is_err();
                    if tx.send(message).is_err() || end {
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            let _ = child.kill();
            return Err(CallError::Failed(format!(
                "cannot start the runner reader: {e}"
            )));
        }
        state.runner = Some(Runner {
            child,
            stdin,
            replies,
            shm_mapped: false,
        });
        let request = Request::Load {
            library: self.library.clone(),
            plugin_id: self.plugin_id.clone(),
            plugin_dir: self.plugin_dir.clone(),
            abi: self.abi,
        };
        match self.exchange(state, &request, LOAD_TIMEOUT)? {
            Reply::Loaded(info) => Ok(info),
            Reply::Failed { message, .. } => {
                // A clean load failure is not a crash: stop the runner.
                if let Some(r) = state.runner.take() {
                    r.kill();
                }
                Err(CallError::Failed(message))
            }
            other => Err(CallError::Failed(format!("unexpected reply {other:?}"))),
        }
    }

    /// Sends one request and waits for its reply; on EOF, a broken pipe or a
    /// timeout the runner is killed and the crash is recorded.
    fn exchange(
        &self,
        state: &mut ClientState,
        request: &Request,
        timeout: Duration,
    ) -> Result<Reply, CallError> {
        let runner = state
            .runner
            .as_mut()
            .ok_or_else(|| CallError::Failed("runner not started".into()))?;
        let outcome = match write_message(&mut runner.stdin, request) {
            Err(e) => Err(format!("the plugin process stopped ({e})")),
            Ok(()) => match runner.replies.recv_timeout(timeout) {
                Ok(Ok(bytes)) => serde_json::from_slice::<Response>(&bytes)
                    .map_err(|e| format!("bad reply from the plugin process: {e}")),
                Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => {
                    // The channel closed: the process is exiting (give it a moment).
                    let mut status = None;
                    for _ in 0..50 {
                        status = runner.child.try_wait().ok().flatten();
                        if status.is_some() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(match status {
                        Some(s) => format!("the plugin process crashed ({s})"),
                        None => "the plugin process crashed".to_owned(),
                    })
                }
                Err(RecvTimeoutError::Timeout) => Err(format!(
                    "the plugin did not respond within {} s and was stopped",
                    timeout.as_secs()
                )),
            },
        };
        match outcome {
            Ok(response) => {
                for (level, line) in &response.logs {
                    log_line(&self.plugin_id, *level, line);
                }
                Ok(response.reply)
            }
            Err(message) => {
                if let Some(r) = state.runner.take() {
                    r.kill();
                }
                let now = Instant::now();
                state
                    .crashes
                    .retain(|t| now.duration_since(*t) < CRASH_WINDOW);
                state.crashes.push(now);
                state.disabled = state.crashes.len() >= MAX_CRASHES;
                Err(CallError::Crashed {
                    message,
                    disabled: state.disabled,
                })
            }
        }
    }

    /// Runs `request`, (re)starting the runner first when needed.
    fn call(
        &self,
        state: &mut ClientState,
        request: &Request,
        timeout: Duration,
    ) -> Result<Reply, CallError> {
        if state.disabled {
            return Err(CallError::Disabled);
        }
        if state.runner.is_none() {
            self.spawn(state)?;
        }
        self.exchange(state, request, timeout)
    }

    /// `pre_render` in the runner.
    pub fn pre_render(&self, frame: &FrameRequest) -> Result<PreRenderOut, CallError> {
        let mut state = self.state.lock();
        match self.call(
            &mut state,
            &Request::PreRender(frame.clone()),
            RENDER_TIMEOUT,
        )? {
            Reply::PreRendered(out) => Ok(out),
            Reply::Failed { status, message } => Err(CallError::Status { status, message }),
            other => Err(CallError::Failed(format!("unexpected reply {other:?}"))),
        }
    }

    /// One frame in the runner: `input` covers `frame.input`, `output` covers
    /// `frame.request` (see [`Library::render`]).
    pub fn render(
        &self,
        frame: &FrameRequest,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<RenderOut, CallError> {
        let mut state = self.state.lock();
        if state.disabled {
            return Err(CallError::Disabled);
        }
        if state.runner.is_none() {
            self.spawn(&mut state)?;
        }
        let output_offset = input.len().next_multiple_of(pixels::ROW_ALIGN);
        let len = output_offset + output.len();
        if state.shm.as_ref().is_none_or(|s| s.map.len() < len) {
            // Grow with headroom so small size changes do not remap.
            let shm = Shm::create(len + len / 4)
                .map_err(|e| CallError::Failed(format!("cannot create shared memory: {e}")))?;
            state.shm = Some(shm);
            if let Some(r) = state.runner.as_mut() {
                r.shm_mapped = false;
            }
        }
        let mapped = state.runner.as_ref().is_some_and(|r| r.shm_mapped);
        let shm = state.shm.as_mut().expect("created above");
        shm.map[..input.len()].copy_from_slice(input);
        let spec = (!mapped).then(|| ShmSpec {
            path: shm.path.clone(),
            len: shm.map.len(),
        });
        let request = Request::Render {
            frame: frame.clone(),
            shm: spec,
            output_offset,
        };
        let reply = self.exchange(&mut state, &request, RENDER_TIMEOUT)?;
        if let Some(r) = state.runner.as_mut() {
            r.shm_mapped = true;
        }
        match reply {
            Reply::Rendered(out) => {
                let shm = state.shm.as_ref().expect("created above");
                output.copy_from_slice(&shm.map[output_offset..output_offset + output.len()]);
                Ok(out)
            }
            Reply::Failed { status, message } => Err(CallError::Status { status, message }),
            other => Err(CallError::Failed(format!("unexpected reply {other:?}"))),
        }
    }

    /// Process id of the running runner (tests).
    pub fn pid(&self) -> Option<u32> {
        self.state.lock().runner.as_ref().map(|r| r.child.id())
    }
}

impl Drop for RunnerClient {
    fn drop(&mut self) {
        let state = self.state.get_mut();
        if let Some(mut runner) = state.runner.take() {
            let _ = write_message(&mut runner.stdin, &Request::Shutdown);
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = runner.child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            runner.kill();
        }
    }
}

/// Emits a plugin log line (`bae.core.log`) through `tracing`.
pub(crate) fn log_line(plugin: &str, level: u32, line: &str) {
    use bae_plugin_sdk::abi::{BAE_LOG_ERROR, BAE_LOG_INFO, BAE_LOG_WARN};
    match level {
        BAE_LOG_ERROR => tracing::error!(target: "plugin", "{plugin}: {line}"),
        BAE_LOG_WARN => tracing::warn!(target: "plugin", "{plugin}: {line}"),
        BAE_LOG_INFO => tracing::info!(target: "plugin", "{plugin}: {line}"),
        _ => tracing::debug!(target: "plugin", "{plugin}: {line}"),
    }
}
