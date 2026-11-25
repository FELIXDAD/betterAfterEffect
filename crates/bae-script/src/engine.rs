//! The QuickJS side: one runtime per run (memory and time limits), the AE
//! object model (`prelude.js`) and the single host function `__host`.

use std::cell::Cell;
use std::time::{Duration, Instant};

use rquickjs::context::EvalOptions;
use rquickjs::{CatchResultExt, CaughtError, Context, Ctx, Function, Runtime};
use serde_json::Value as Json;

use crate::host::{self, RunState};

const PRELUDE: &str = include_str!("prelude.js");

/// File name of the user's script in QuickJS stack traces (to find the line).
const SCRIPT_FILE: &str = "script";

/// QuickJS heap per run.
const MEMORY_LIMIT: usize = 128 << 20;
/// Native stack QuickJS may use: deep recursion is a RangeError, never a crash,
/// also on the 1 MiB main thread of Windows.
const JS_STACK: usize = 384 << 10;

thread_local! {
    /// The run in progress on this thread (type-erased `*mut RunState`).
    static CURRENT: Cell<*mut ()> = const { Cell::new(std::ptr::null_mut()) };
}

/// Clears [`CURRENT`] when the run ends, also on panic.
struct Current;

impl Current {
    fn set(state: &mut RunState<'_, '_, '_>) -> Current {
        CURRENT.with(|c| c.set(std::ptr::from_mut(state).cast()));
        Current
    }
}

impl Drop for Current {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(std::ptr::null_mut()));
    }
}

fn with_state<T>(f: impl FnOnce(&mut RunState<'_, '_, '_>) -> T) -> Option<T> {
    let ptr = CURRENT.with(Cell::get);
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` was set by `Current::set` from a `&mut RunState` that outlives
    // the evaluation (the guard is dropped before the state), on this thread.
    // Host calls are not re-entrant (commands never call back into JavaScript),
    // so this is the only live reference while `f` runs; the erased lifetimes
    // are only used inside `f`.
    let state = unsafe { &mut *ptr.cast::<RunState<'_, '_, '_>>() };
    Some(f(state))
}

fn throw<T>(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Result<T> {
    Err(rquickjs::Exception::throw_message(ctx, msg))
}

fn install_host(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let f = Function::new(ctx.clone(), |ctx: Ctx<'_>, method: String, args: String| {
        let args: Json = serde_json::from_str(&args).unwrap_or(Json::Null);
        match with_state(|s| host::call(s, &method, &args)) {
            Some(Ok(out)) => Ok(out.to_string()),
            Some(Err(e)) => throw(&ctx, &e),
            None => throw(&ctx, "no script is running"),
        }
    })?;
    ctx.globals().set("__host", f)
}

/// ExtendScript preprocessor lines (`#target aftereffects`, `#include`,
/// `#targetengine`, `#strict`) are blanked, keeping line numbers.
fn strip_directives(source: &str) -> String {
    source
        .lines()
        .map(|l| {
            if l.trim_start().starts_with('#') {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn describe(e: CaughtError<'_>, timed_out: Option<Duration>) -> String {
    if let Some(limit) = timed_out {
        return format!(
            "the script was stopped after {:.1} seconds",
            limit.as_secs_f64()
        );
    }
    match e {
        CaughtError::Exception(ex) => {
            let msg = ex.message().unwrap_or_default();
            let name = ex
                .get::<_, String>("name")
                .unwrap_or_else(|_| "Error".to_owned());
            let line = ex.stack().and_then(|s| {
                let i = s.find(&format!("{SCRIPT_FILE}:"))?;
                let digits: String = s[i + SCRIPT_FILE.len() + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse::<usize>().ok()
            });
            match line {
                Some(l) => format!("{name} at line {l}: {msg}"),
                None => format!("{name}: {msg}"),
            }
        }
        CaughtError::Value(v) => format!(
            "Error: {}",
            v.as_string()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_else(|| format!("{v:?}"))
        ),
        CaughtError::Error(e) => e.to_string(),
    }
}

/// Evaluates `source` with the object model; returns the completion value as
/// JSON, or the error that stopped it ("TypeError at line 3: …").
pub(crate) fn execute(state: &mut RunState<'_, '_, '_>, source: &str) -> Result<Json, String> {
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.set_memory_limit(MEMORY_LIMIT);
    rt.set_max_stack_size(JS_STACK);
    let limit = state.env.time_limit.unwrap_or(crate::TIME_LIMIT);
    let deadline = Instant::now() + limit;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let ctx = Context::full(&rt).map_err(|e| e.to_string())?;
    let _current = Current::set(state);
    let source = strip_directives(source);
    ctx.with(|ctx| {
        install_host(&ctx).map_err(|e| e.to_string())?;
        let mut opts = EvalOptions::default();
        opts.global = true;
        opts.strict = false;
        opts.filename = Some("bae-prelude".into());
        ctx.eval_with_options::<(), _>(PRELUDE, opts)
            .catch(&ctx)
            .map_err(|e| format!("script prelude: {}", describe(e, None)))?;
        let mut opts = EvalOptions::default();
        opts.global = true;
        opts.strict = false;
        opts.filename = Some(SCRIPT_FILE.into());
        let result = ctx
            .eval_with_options::<rquickjs::Value<'_>, _>(source, opts)
            .catch(&ctx)
            .map_err(|e| describe(e, (Instant::now() >= deadline).then_some(limit)))?;
        let json: Function<'_> = ctx.globals().get("__json").map_err(|e| e.to_string())?;
        let text: String = json.call((result,)).unwrap_or_else(|_| "null".into());
        Ok(serde_json::from_str(&text).unwrap_or(Json::Null))
    })
}
