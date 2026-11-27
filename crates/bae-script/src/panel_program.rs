//! Global-script bytecode for installed ScriptUI panels. A panel stays in
//! ExtendScript's non-strict mode, including when its code runs from the cache.
use rquickjs::{CatchResultExt, Context, Ctx, Runtime, Value, qjs};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub struct PanelProgram {
    path: PathBuf,
    code: OnceLock<Result<Vec<u8>, String>>,
}

impl PanelProgram {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            code: OnceLock::new(),
        }
    }

    pub fn ready(&self) -> bool {
        self.code.get().is_some()
    }

    pub fn prepare(&self) {
        self.code.get_or_init(|| {
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
        });
    }

    pub(crate) fn eval<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let code = self
            .code
            .get()
            .and_then(|c| c.as_ref().ok())
            .ok_or_else(|| {
                let error = self
                    .code
                    .get()
                    .and_then(|c| c.as_ref().err())
                    .map(String::as_str);
                rquickjs::Exception::throw_message(ctx, error.unwrap_or("panel is loading"))
            })?;
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
