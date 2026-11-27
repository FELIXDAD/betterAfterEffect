//! Global-script bytecode for installed ScriptUI panels. A panel stays in
//! ExtendScript's non-strict mode, including when its code runs from the cache.
use rquickjs::{CatchResultExt, Context, Ctx, Runtime, Value, qjs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// The file's modification time and length when it was compiled.
type Stamp = Option<(SystemTime, u64)>;
type Code = Arc<Result<Vec<u8>, String>>;

pub struct PanelProgram {
    path: PathBuf,
    code: Mutex<Option<(Stamp, Code)>>,
    compiling: AtomicBool,
}

fn stamp(path: &std::path::Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

impl PanelProgram {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            code: Mutex::default(),
            compiling: AtomicBool::new(false),
        }
    }

    fn current(&self, stamp: &Stamp) -> bool {
        self.code
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|(s, _)| s == stamp)
    }

    /// Whether the bytecode matches the file now. When it doesn't (not warmed up
    /// yet, or the file was edited since), compiles it on a background thread and
    /// returns false: the panel shows a spinner and asks again next frame.
    pub fn ready(self: &Arc<Self>) -> bool {
        if self.current(&stamp(&self.path)) {
            return true;
        }
        if !self.compiling.swap(true, Ordering::AcqRel) {
            let program = self.clone();
            let spawned = std::thread::Builder::new()
                .name("scriptui-compile".into())
                .spawn(move || {
                    program.prepare();
                    program.compiling.store(false, Ordering::Release);
                });
            if spawned.is_err() {
                self.compiling.store(false, Ordering::Release);
            }
        }
        false
    }

    /// Compiles the file unless the bytecode already matches it (blocking: the
    /// warm-up thread or [`Self::ready`]'s background thread).
    pub fn prepare(&self) {
        let stamp = stamp(&self.path);
        if self.current(&stamp) {
            return;
        }
        let code = Arc::new(self.compile());
        *self.code.lock().unwrap_or_else(|e| e.into_inner()) = Some((stamp, code));
    }

    fn compile(&self) -> Result<Vec<u8>, String> {
        let source = std::fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        let mut source = super::engine::panel_source(&source);
        source.push('\0');
        let rt = Runtime::new().map_err(|e| e.to_string())?;
        rt.set_memory_limit(super::engine::MEMORY_LIMIT);
        rt.set_max_stack_size(super::engine::JS_STACK);
        Context::full(&rt).map_err(|e| e.to_string())?.with(|ctx| {
            let compile = || {
                let raw = ctx.as_raw().as_ptr();
                // SAFETY: the source and filename are terminated and live through
                // JS_Eval. COMPILE_ONLY returns an owned function without running it.
                let function = unsafe {
                    Value::from_raw(
                        ctx.clone(),
                        qjs::JS_Eval(
                            raw,
                            source.as_ptr().cast(),
                            (source.len() - 1) as _,
                            c"script".as_ptr(),
                            (qjs::JS_EVAL_TYPE_GLOBAL | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as i32,
                        ),
                    )
                };
                if function.is_exception() {
                    return Err(rquickjs::Error::Exception);
                }
                let mut len = 0;
                // SAFETY: function belongs to raw; copy the returned allocation
                // before freeing it with the same QuickJS allocator.
                unsafe {
                    let ptr = qjs::JS_WriteObject(
                        raw,
                        &mut len,
                        function.as_raw(),
                        qjs::JS_WRITE_OBJ_BYTECODE as i32,
                    );
                    if ptr.is_null() {
                        return Err(rquickjs::Error::Exception);
                    }
                    let bytes = std::slice::from_raw_parts(ptr, len as usize).to_vec();
                    qjs::js_free(raw, ptr.cast());
                    Ok(bytes)
                }
            };
            compile().catch(&ctx).map_err(|e| e.to_string())
        })
    }

    pub(crate) fn eval<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let compiled = self
            .code
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(_, code)| code.clone());
        let code = match compiled.as_deref() {
            Some(Ok(code)) => code,
            Some(Err(error)) => return Err(rquickjs::Exception::throw_message(ctx, error)),
            None => return Err(rquickjs::Exception::throw_message(ctx, "panel is loading")),
        };
        let raw = ctx.as_raw().as_ptr();
        // SAFETY: prepare() generated the immutable bytecode in this process using
        // the same engine. JS_EvalFunction consumes the function on every return path.
        let result = unsafe {
            let function = qjs::JS_ReadObject(
                raw,
                code.as_ptr(),
                code.len() as _,
                qjs::JS_READ_OBJ_BYTECODE as i32,
            );
            if qjs::JS_IsException(function) {
                return Err(rquickjs::Error::Exception);
            }
            Value::from_raw(ctx.clone(), qjs::JS_EvalFunction(raw, function))
        };
        if result.is_exception() {
            Err(rquickjs::Error::Exception)
        } else {
            Ok(result)
        }
    }
}

pub(crate) enum InteractiveSource {
    Text(String),
    Panel(Arc<PanelProgram>),
}
