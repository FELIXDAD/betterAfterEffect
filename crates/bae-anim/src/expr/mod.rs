//! AE expressions on QuickJS (rquickjs).
//!
//! - One QuickJS runtime per thread, created on first use; the AE library
//!   (`prelude.js`) is loaded once. Compiled expressions are cached by source text.
//! - Sources are rewritten first ([`rewrite`]): vector operators, completion value,
//!   AE names (`value`, `position`, `wiggle`, …) as function-local variables.
//! - An evaluation [`Scope`] (thread-local, one per top-level `evaluate_comp`) holds
//!   the compiled comps expressions can reach, the per-call memo (property × time),
//!   the stack used to break cycles, collected errors and cross-layer dependencies.
//!   Host functions called from JavaScript read it; they never hold a borrow while
//!   calling back into JavaScript.

pub(crate) mod fast;
mod path;
pub(crate) mod rewrite;

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bae_model::{PrimPath, Time, Value};
use rquickjs::context::EvalOptions;
use rquickjs::{
    Array, CatchResultExt, CaughtError, Context, Ctx, Function, IntoJs, Object, Runtime,
};

use crate::eval::{CompiledComp, Cx, ExprProp, num};
use crate::evaluated::AttrError;
use crate::schema::transform as tr;

const PRELUDE: &str = include_str!("prelude.js");

/// Script name used in QuickJS stack traces (to find the failing line).
const EXPR_FILE: &str = "expression";

struct Engine {
    _rt: Runtime,
    ctx: Context,
    /// Source → index into the JS array `__fns`, or the compile error.
    compiled: HashMap<Arc<str>, Result<u32, String>>,
    /// The same by the source's address (compiled comps keep their sources
    /// across frames), holding the source so the address stays its own.
    by_ptr: rustc_hash::FxHashMap<usize, (Arc<str>, Result<u32, String>)>,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

impl Engine {
    fn new() -> Engine {
        let rt = Runtime::new().expect("QuickJS runtime");
        // Expressions are small; a runaway loop should not take the app down.
        rt.set_memory_limit(256 << 20);
        // Native stack QuickJS may use from the entry point ([`with_js`] resets it),
        // shared by the nested expression evaluations (at most MAX_DEPTH) and user
        // recursion. Small enough to leave room on 1 MiB thread stacks (Windows main
        // thread), so deep recursion is a RangeError, never a crash.
        rt.set_max_stack_size(JS_STACK);
        let ctx = Context::full(&rt).expect("QuickJS context");
        ctx.with(|ctx| {
            install_host(&ctx).expect("host functions");
            ctx.eval::<(), _>(PRELUDE)
                .catch(&ctx)
                .unwrap_or_else(|e| panic!("expression prelude: {e}"));
            ctx.globals()
                .set("__fns", Array::new(ctx.clone()).expect("array"))
                .expect("__fns");
        });
        Engine {
            _rt: rt,
            ctx,
            compiled: HashMap::new(),
            by_ptr: rustc_hash::FxHashMap::default(),
        }
    }
}

/// Engines made ahead of use by [`prewarm`] (creating one takes a few ms: the
/// runtime, the context and the prelude).
static PREWARMED: std::sync::Mutex<Vec<Engine>> = std::sync::Mutex::new(Vec::new());

/// Make an expression engine now, on this (background) thread, for the first
/// thread that evaluates expressions to take over instead of creating one
/// while the user waits (the app's warm-up runs this at startup).
pub fn prewarm() {
    let engine = Engine::new();
    PREWARMED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(engine);
}

fn take_or_new() -> Engine {
    let ready = PREWARMED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pop();
    ready.unwrap_or_else(Engine::new)
}

/// QuickJS stack budget, see [`Engine::new`].
const JS_STACK: usize = 384 << 10;

/// Run `f` inside this thread's JavaScript context. Only called from Rust (never from
/// a host function), so it is the outermost entry into JavaScript.
pub(crate) fn with_js<R>(f: impl FnOnce(&Ctx<'_>) -> R) -> R {
    let ctx = ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.get_or_insert_with(take_or_new).ctx.clone()
    });
    ctx.with(|ctx| {
        // QuickJS measures its stack limit from the stack top recorded when the
        // runtime was created; measure it from here instead, so the budget does not
        // depend on where the runtime happened to be created.
        // SAFETY: `ctx` is a live context of this thread's runtime, so the runtime
        // pointer is valid; JS_UpdateStackTop only reads the current stack pointer.
        unsafe {
            rquickjs::qjs::JS_UpdateStackTop(rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr()));
        }
        f(&ctx)
    })
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// An evaluation on the stack: comp id, layer, property, comp time.
type Key = (usize, usize, PrimPath, Arc<str>, i64);

/// Memo key: comp id, layer, the compiled property (its address: the scope's
/// comps outlive the scope) and comp time. Integers only, so a lookup costs a
/// few multiplies instead of hashing paths (1000-layer comps look up every
/// property twice per frame). The stack still compares paths, so a second
/// compiled copy of a property only costs a second memo entry.
type MemoKey = (usize, usize, usize, i64);

fn memo_key(cx: &Cx<'_, '_>, e: &ExprProp) -> MemoKey {
    (cx.comp, cx.layer, std::ptr::from_ref(e) as usize, cx.ct.0)
}

type Memo = rustc_hash::FxHashMap<MemoKey, Value>;

thread_local! {
    /// The last scope's memo, emptied: the next frame reuses its table.
    static MEMO_POOL: RefCell<Memo> = RefCell::new(Memo::default());
}

/// State of one top-level evaluation.
pub(crate) struct Scope {
    comps: Vec<Arc<CompiledComp>>,
    by_path: HashMap<PrimPath, usize>,
    by_name: HashMap<String, usize>,
    memo: Memo,
    stack: Vec<Key>,
    /// Errors by comp id, deduplicated by (prim, attr).
    errors: Vec<(usize, AttrError)>,
    /// (comp id, layer) → layers it read through expressions.
    deps: BTreeSet<((usize, usize), (usize, usize))>,
    /// Set when an evaluation hit [`MAX_DEPTH`]: everything above it on the stack is
    /// incomplete (not memoised, errors not recorded) and is retried by the caller's
    /// warm-up passes ([`warm_up`]).
    deferred: bool,
}

/// Nested expression evaluations allowed on the native stack. Deeper dependency
/// chains are resolved by repeated passes that memoise them from the far end, so the
/// stack use is bounded whatever the chain length.
const MAX_DEPTH: usize = 4;

impl Scope {
    pub(crate) fn new(comps: Vec<Arc<CompiledComp>>) -> Scope {
        let by_path = comps
            .iter()
            .enumerate()
            .map(|(i, c)| (c.path.clone(), i))
            .collect();
        let mut by_name = HashMap::new();
        for (i, c) in comps.iter().enumerate() {
            by_name.entry(c.name.clone()).or_insert(i);
        }
        Scope {
            comps,
            by_path,
            by_name,
            memo: MEMO_POOL.with(|p| std::mem::take(&mut *p.borrow_mut())),
            stack: Vec::new(),
            errors: Vec::new(),
            deps: BTreeSet::new(),
            deferred: false,
        }
    }

    pub(crate) fn comp_id(&self, path: &PrimPath) -> Option<usize> {
        self.by_path.get(path).copied()
    }
}

/// What a finished scope reports back.
pub(crate) struct ScopeResult {
    pub errors: Vec<(usize, AttrError)>,
    pub deps: BTreeSet<((usize, usize), (usize, usize))>,
    pub comps: Vec<Arc<CompiledComp>>,
}

/// Install `scope` for the duration of `f` (no-op nesting: an active scope is reused).
pub(crate) fn with_scope<R>(scope: Scope, f: impl FnOnce() -> R) -> (R, Option<ScopeResult>) {
    let installed = SCOPE.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_some() {
            false
        } else {
            *s = Some(scope);
            true
        }
    });
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            if self.0 {
                SCOPE.with(|s| s.borrow_mut().take());
            }
        }
    }
    let reset = Reset(installed);
    let r = f();
    let result = installed
        .then(|| SCOPE.with(|s| s.borrow_mut().take()))
        .flatten()
        .map(|mut s| {
            let mut memo = std::mem::take(&mut s.memo);
            memo.clear();
            MEMO_POOL.with(|p| *p.borrow_mut() = memo);
            ScopeResult {
                errors: s.errors,
                deps: s.deps,
                comps: s.comps,
            }
        });
    drop(reset);
    (r, result)
}

/// Comp id of `path` in the active scope.
pub(crate) fn scope_comp_id(path: &PrimPath) -> Option<usize> {
    SCOPE.with(|s| s.borrow().as_ref().and_then(|s| s.comp_id(path)))
}

fn scope_comp(c: usize) -> Option<Arc<CompiledComp>> {
    SCOPE.with(|s| s.borrow().as_ref().and_then(|s| s.comps.get(c).cloned()))
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Evaluate the expression of `e` at `cx`. Errors are recorded in the scope and the
/// pre-expression value is returned, as AE does.
pub(crate) fn evaluate(ctx: &Ctx<'_>, cx: &Cx<'_, '_>, e: &ExprProp) -> Value {
    let pre = e.base.sample(cx);
    let key = memo_key(cx, e);
    let state = SCOPE.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        if let Some(v) = s.memo.get(&key) {
            return Some(Some(v.clone()));
        }
        // On the stack at any time: the property reading itself (`value`,
        // `valueAtTime`, `velocity`) or a cycle → pre-expression value, as in AE.
        s.stack
            .iter()
            .any(|k| k.0 == cx.comp && k.1 == cx.layer && k.2 == e.prim && *k.3 == *e.attr)
            .then_some(None)
    });
    match state {
        Some(Some(v)) => return v,
        Some(None) => return pre,
        None if SCOPE.with(|s| s.borrow().is_none()) => return pre,
        None => {}
    }
    // Common shapes run in Rust: they read only their own property, so they
    // need no stack entry or dependency; anything unusual falls back to JS.
    if let Some(f) = e.fast.as_ref().filter(|_| fast_enabled())
        && let Some(value) = eval_fast(f, cx, e, &pre)
    {
        let value = clamped(e, value);
        #[cfg(test)]
        FAST_HITS.with(|h| h.set(h.get() + 1));
        SCOPE.with(|s| {
            if let Some(s) = s.borrow_mut().as_mut() {
                s.memo.insert(key, value.clone());
            }
        });
        return value;
    }
    let too_deep = SCOPE.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut().expect("scope");
        if s.stack.len() >= MAX_DEPTH {
            s.deferred = true;
            true
        } else {
            false
        }
    });
    if too_deep {
        return pre;
    }
    let func = match compile(ctx, &e.source) {
        Ok(f) => f,
        Err(msg) => {
            record_error(cx, e, msg);
            return pre;
        }
    };
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.stack
                .push((cx.comp, cx.layer, e.prim.clone(), e.attr.clone(), cx.ct.0));
        }
    });
    let result = call(ctx, &func, cx, e);
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.stack.pop();
        }
    });
    if is_deferred() {
        // A dependency was too deep to finish here; a later warm-up pass retries.
        return pre;
    }
    let value = match result.and_then(|js| to_value(js, &pre)) {
        Ok(v) => clamped(e, v),
        Err(msg) => {
            record_error(cx, e, msg);
            pre
        }
    };
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.memo.insert(key, value.clone());
        }
    });
    value
}

/// `v` clamped to the property's range (Opacity), as after interpolation.
fn clamped(e: &ExprProp, v: Value) -> Value {
    match (v, e.clamp) {
        (Value::Float(v), Some((lo, hi))) => Value::Float(v.clamp(lo, hi)),
        (v, _) => v,
    }
}

#[cfg(test)]
thread_local! {
    /// Tests compare the fast path with JavaScript by turning it off.
    pub(crate) static NO_FAST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Evaluations the fast path finished.
    pub(crate) static FAST_HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn fast_enabled() -> bool {
    #[cfg(test)]
    return !NO_FAST.with(std::cell::Cell::get);
    #[cfg(not(test))]
    true
}

/// What the fast path reads: the property's own pre-expression value, keys
/// and seed, exactly as the JS host functions give them to `value`,
/// `valueAtTime`, `velocityAtTime` and the keyframe list.
struct FastEnv<'a, 'b, 'js> {
    cx: &'a Cx<'b, 'js>,
    e: &'a ExprProp,
    layer: &'a crate::eval::CompiledLayer,
    /// AE's 1/1024-frame time grid.
    tick: f64,
    /// A 2D layer's Position, Anchor Point or Scale: two components in JS.
    flat: bool,
    seed: u32,
}

impl FastEnv<'_, '_, '_> {
    fn sample(&self, ct: Time) -> Vec<f64> {
        let cx = Cx {
            ct,
            lt: self.layer.timing.layer_time(ct),
            ..*self.cx
        };
        let mut out = components(&self.e.base.sample(&cx));
        if self.flat {
            out.truncate(2);
        }
        out
    }

    fn grid(&self, t: f64) -> f64 {
        (t / self.tick).round() * self.tick
    }
}

impl fast::Env for FastEnv<'_, '_, '_> {
    fn now(&self) -> f64 {
        self.cx.ct.as_seconds()
    }

    fn pre(&self, t: f64) -> Vec<f64> {
        self.sample(Time::from_seconds(self.grid(t)))
    }

    fn keys(&self) -> Vec<(f64, Vec<f64>)> {
        self.e.base.curve().map_or_else(Vec::new, |c| {
            c.keys()
                .map(|(t, v)| (self.layer.timing.comp_time(t).as_seconds(), components(v)))
                .collect()
        })
    }

    fn pre_velocity(&self, t: f64) -> Vec<f64> {
        let q = self.grid(t);
        let (t0, t1) = (Time::from_seconds(q), Time::from_seconds(q + self.tick));
        let (a, b) = (self.sample(t0), self.sample(t1));
        let dt = (t1 - t0).as_seconds();
        a.iter().zip(&b).map(|(a, b)| (b - a) / dt).collect()
    }

    fn seed(&self) -> u32 {
        self.seed
    }
}

/// `e` through the fast path, or `None` when JS has to evaluate it.
fn eval_fast(f: &fast::Fast, cx: &Cx<'_, '_>, e: &ExprProp, pre: &Value) -> Option<Value> {
    let comp = scope_comp(cx.comp)?;
    let layer = comp.layers.get(cx.layer)?;
    let env = FastEnv {
        cx,
        e,
        layer,
        tick: comp.frame_rate.frame_duration().as_seconds() / 1024.0,
        flat: !layer.switches.three_d
            && e.prim == layer.path
            && matches!(&*e.attr, tr::POSITION | tr::ANCHOR_POINT | tr::SCALE),
        seed: seed_of(e, cx.comp, cx.layer),
    };
    let comps = f.eval(&env)?;
    // Errors (non-finite results) are reported by the JS path.
    if comps.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let pick = |i: usize, fallback: f64| comps.get(i).copied().unwrap_or(fallback);
    Some(match pre {
        Value::Float(_) => Value::Float(pick(0, 0.0)),
        Value::Int(_) => Value::Int(pick(0, 0.0).round() as i64),
        Value::Bool(_) => Value::Bool(pick(0, 0.0) != 0.0),
        Value::Time(_) => Value::Time(Time::from_seconds(pick(0, 0.0))),
        Value::Vec2(t) => Value::Vec2([pick(0, t[0]), pick(1, t[1])]),
        Value::Vec3(t) => Value::Vec3([pick(0, t[0]), pick(1, t[1]), pick(2, t[2])]),
        Value::Color(t) => {
            Value::Color([pick(0, t[0]), pick(1, t[1]), pick(2, t[2]), pick(3, t[3])])
        }
        Value::FloatList(_) => Value::FloatList(comps),
        _ => return None,
    })
}

fn is_deferred() -> bool {
    SCOPE.with(|s| s.borrow().as_ref().is_some_and(|s| s.deferred))
}

/// Memoise every expression of a comp before the real pass, so long dependency
/// chains never recurse deeper than [`MAX_DEPTH`]: each pass completes the
/// evaluations whose remaining chain fits, and the next pass builds on them.
/// `eval_one(i)` evaluates property `i`; when a pass makes no progress (a cycle longer
/// than the depth limit), `cut(i)` memoises property `i`'s pre-expression value,
/// which is how cycles are cut anyway.
pub(crate) fn warm_up(count: usize, mut eval_one: impl FnMut(usize), mut cut: impl FnMut(usize)) {
    let mut pending: Vec<usize> = (0..count).collect();
    for pass in 0..(4 * count + 8) {
        let mut still = Vec::new();
        // Alternate directions: a chain running either way through the layer order
        // completes within two passes.
        let order: Vec<usize> = if pass % 2 == 0 {
            pending.clone()
        } else {
            pending.iter().rev().copied().collect()
        };
        for &i in &order {
            set_deferred(false);
            eval_one(i);
            if is_deferred() {
                still.push(i);
            }
        }
        if still.is_empty() {
            break;
        }
        if still.len() == pending.len() && pass % 2 == 1 {
            cut(still.remove(0));
        }
        still.sort_unstable();
        pending = still;
    }
    set_deferred(false);
}

fn set_deferred(v: bool) {
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.deferred = v;
        }
    });
}

/// Memoise `e`'s pre-expression value at `cx` (cuts a dependency cycle there).
pub(crate) fn memoize_pre(cx: &Cx<'_, '_>, e: &ExprProp) {
    let key = memo_key(cx, e);
    let pre = e.base.sample(cx);
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.memo.insert(key, pre);
        }
    });
}

fn record_error(cx: &Cx<'_, '_>, e: &ExprProp, message: String) {
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            let exists = s
                .errors
                .iter()
                .any(|(c, err)| *c == cx.comp && err.prim == e.prim && *err.attr == *e.attr);
            if !exists {
                s.errors.push((
                    cx.comp,
                    AttrError {
                        prim: e.prim.clone(),
                        attr: e.attr.to_string(),
                        message,
                    },
                ));
            }
        }
    });
}

/// Compiled wrapper function for `source` (cached per thread).
fn compile<'js>(ctx: &Ctx<'js>, source: &Arc<str>) -> Result<Function<'js>, String> {
    let ptr = Arc::as_ptr(source).cast::<u8>() as usize;
    let cached = ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        let e = e.as_mut()?;
        if let Some((_, r)) = e.by_ptr.get(&ptr) {
            return Some(r.clone());
        }
        let r = e.compiled.get(source).cloned()?;
        e.by_ptr.insert(ptr, (source.clone(), r.clone()));
        Some(r)
    });
    let fns: Array<'js> = ctx.globals().get("__fns").map_err(|e| e.to_string())?;
    let id = match cached {
        Some(r) => r?,
        None => {
            let r = compile_new(ctx, source).and_then(|f| {
                let id = fns.len() as u32;
                fns.set(id as usize, f).map_err(|e| e.to_string())?;
                Ok(id)
            });
            ENGINE.with(|e| {
                if let Some(e) = e.borrow_mut().as_mut() {
                    e.compiled.insert(source.clone(), r.clone());
                }
            });
            r?
        }
    };
    fns.get(id as usize).map_err(|e| e.to_string())
}

/// AE names that become function-local variables when an expression uses them.
const LOCALS: &[(&str, &str)] = &[
    ("value", "thisProperty.__pre(time)"),
    ("index", "thisLayer.index"),
    ("name", "thisLayer.name"),
    ("inPoint", "thisLayer.inPoint"),
    ("outPoint", "thisLayer.outPoint"),
    ("startTime", "thisLayer.startTime"),
    ("hasParent", "thisLayer.hasParent"),
    ("parent", "thisLayer.parent"),
    ("enabled", "thisLayer.enabled"),
    ("active", "thisLayer.active"),
    ("hasVideo", "thisLayer.hasVideo"),
    ("hasAudio", "thisLayer.hasAudio"),
    ("width", "thisLayer.width"),
    ("height", "thisLayer.height"),
    ("source", "thisLayer.source"),
    ("transform", "thisLayer.transform"),
    ("content", "thisLayer.content.bind(thisLayer)"),
    ("points", "thisProperty.points.bind(thisProperty)"),
    ("inTangents", "thisProperty.inTangents.bind(thisProperty)"),
    ("outTangents", "thisProperty.outTangents.bind(thisProperty)"),
    ("isClosed", "thisProperty.isClosed.bind(thisProperty)"),
    ("pointOnPath", "thisProperty.pointOnPath.bind(thisProperty)"),
    (
        "tangentOnPath",
        "thisProperty.tangentOnPath.bind(thisProperty)",
    ),
    (
        "normalOnPath",
        "thisProperty.normalOnPath.bind(thisProperty)",
    ),
    ("createPath", "thisProperty.createPath.bind(thisProperty)"),
    ("anchorPoint", "thisLayer.anchorPoint"),
    ("position", "thisLayer.position"),
    ("scale", "thisLayer.scale"),
    ("rotation", "thisLayer.rotation"),
    ("xRotation", "thisLayer.xRotation"),
    ("yRotation", "thisLayer.yRotation"),
    ("zRotation", "thisLayer.zRotation"),
    ("orientation", "thisLayer.orientation"),
    ("opacity", "thisLayer.opacity"),
    ("pointOfInterest", "thisLayer.pointOfInterest"),
    ("timeRemap", "thisLayer.timeRemap"),
    ("effect", "thisLayer.effect.bind(thisLayer)"),
    ("mask", "thisLayer.mask.bind(thisLayer)"),
    ("toComp", "thisLayer.toComp.bind(thisLayer)"),
    ("fromComp", "thisLayer.fromComp.bind(thisLayer)"),
    ("toWorld", "thisLayer.toWorld.bind(thisLayer)"),
    ("fromWorld", "thisLayer.fromWorld.bind(thisLayer)"),
    ("toCompVec", "thisLayer.toCompVec.bind(thisLayer)"),
    ("fromCompVec", "thisLayer.fromCompVec.bind(thisLayer)"),
    ("toWorldVec", "thisLayer.toWorldVec.bind(thisLayer)"),
    ("fromWorldVec", "thisLayer.fromWorldVec.bind(thisLayer)"),
    ("sourceTime", "thisLayer.sourceTime.bind(thisLayer)"),
    ("velocity", "thisProperty.velocity"),
    ("speed", "thisProperty.speed"),
    ("numKeys", "thisProperty.numKeys"),
    ("propertyIndex", "thisProperty.propertyIndex"),
    ("valueAtTime", "thisProperty.valueAtTime.bind(thisProperty)"),
    (
        "velocityAtTime",
        "thisProperty.velocityAtTime.bind(thisProperty)",
    ),
    ("speedAtTime", "thisProperty.speedAtTime.bind(thisProperty)"),
    ("key", "thisProperty.key.bind(thisProperty)"),
    ("nearestKey", "thisProperty.nearestKey.bind(thisProperty)"),
    ("wiggle", "thisProperty.wiggle.bind(thisProperty)"),
    (
        "temporalWiggle",
        "thisProperty.temporalWiggle.bind(thisProperty)",
    ),
    ("smooth", "thisProperty.smooth.bind(thisProperty)"),
    ("loopIn", "thisProperty.loopIn.bind(thisProperty)"),
    ("loopOut", "thisProperty.loopOut.bind(thisProperty)"),
    (
        "loopInDuration",
        "thisProperty.loopInDuration.bind(thisProperty)",
    ),
    (
        "loopOutDuration",
        "thisProperty.loopOutDuration.bind(thisProperty)",
    ),
];

/// The JavaScript source of the wrapper function for an expression.
pub(crate) fn wrapper_source(source: &str) -> Result<String, String> {
    let r = rewrite::rewrite(source).map_err(|e| e.to_string())?;
    let mut prologue = String::new();
    for (name, init) in LOCALS {
        if r.idents.contains(*name) && !r.declared.contains(*name) {
            prologue.push_str(&format!("var {name}={init};"));
        }
    }
    if r.idents.contains("posterizeTime") && !r.declared.contains("posterizeTime") {
        prologue.push_str(
            "function posterizeTime(f){if(f>0){time=Math.floor(time*f+1e-9)/f;__now=time;__reseed();",
        );
        if r.idents.contains("value") && !r.declared.contains("value") {
            prologue.push_str("value=thisProperty.__pre(time);");
        }
        prologue.push_str("}return time;}");
    }
    for name in &r.undeclared_assignments {
        prologue.push_str(&format!("var {name};"));
    }
    // The AE objects are made only when the expression (or an AE name above)
    // uses them: most evaluations need one or none.
    let dynamic = ["eval", "Function", "arguments"]
        .iter()
        .any(|n| r.idents.contains(*n));
    let needs = |n: &str| dynamic || r.idents.contains(n) || prologue.contains(n);
    let mut head = String::from("(function(__c,__l,__p,__a,time){");
    if needs("thisComp") {
        head.push_str("var thisComp=new Comp(__c);");
    }
    if needs("thisLayer") {
        head.push_str("var thisLayer=new Layer(__c,__l);");
    }
    if needs("thisProperty") {
        head.push_str("var thisProperty=__prop(__c,__l,__p,__a);");
    }
    Ok(format!("{head}{prologue}{}}})", r.body))
}

fn compile_new<'js>(ctx: &Ctx<'js>, source: &str) -> Result<Function<'js>, String> {
    let wrapped = wrapper_source(source)?;
    let mut opts = EvalOptions::default();
    opts.global = true;
    opts.strict = false;
    opts.filename = Some(EXPR_FILE.into());
    ctx.eval_with_options::<Function<'js>, _>(wrapped, opts)
        .catch(ctx)
        .map_err(|e| describe(e))
}

fn call<'js>(
    ctx: &Ctx<'js>,
    func: &Function<'js>,
    cx: &Cx<'_, '_>,
    e: &ExprProp,
) -> Result<rquickjs::Value<'js>, String> {
    let call: Function<'js> = ctx.globals().get("__call").map_err(|e| e.to_string())?;
    let seed = seed_of(e, cx.comp, cx.layer);
    call.call((
        func.clone(),
        cx.comp as i32,
        cx.layer as i32,
        e.prim.as_str(),
        &*e.attr,
        cx.ct.as_seconds(),
        seed,
    ))
    .catch(ctx)
    .map_err(|e| describe(e))
}

fn describe(e: CaughtError<'_>) -> String {
    match e {
        CaughtError::Exception(ex) => {
            let msg = ex.message().unwrap_or_default();
            let line = ex.stack().and_then(|s| {
                let i = s.find(&format!("{EXPR_FILE}:"))?;
                let digits: String = s[i + EXPR_FILE.len() + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse::<usize>().ok()
            });
            let name = ex
                .get::<_, String>("name")
                .unwrap_or_else(|_| "Error".to_owned());
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

/// Convert a JavaScript result to the property's value type (`template` is the
/// pre-expression value).
fn to_value(js: rquickjs::Value<'_>, template: &Value) -> Result<Value, String> {
    if let Some(s) = js.as_string() {
        let s = s.to_string().map_err(|e| e.to_string())?;
        return match template {
            Value::TextDocument(doc) => {
                let mut doc = doc.clone();
                doc.text = s;
                Ok(Value::TextDocument(doc))
            }
            Value::String(_) => Ok(Value::String(s)),
            _ => Err(format!(
                "Expression result must be a number, not the string {s:?}"
            )),
        };
    }
    if let Value::Bezier(old) = template
        && let Some(o) = js.as_object()
    {
        return path_from_js(o, old).map(Value::Bezier);
    }
    if let Value::TextDocument(doc) = template {
        // Numbers on Source Text show as text, as in AE.
        let mut doc = doc.clone();
        doc.text = js_display(&js);
        return Ok(Value::TextDocument(doc));
    }
    let comps: Vec<f64> = if let Some(n) = js.as_number() {
        vec![n]
    } else if let Some(b) = js.as_bool() {
        vec![f64::from(u8::from(b))]
    } else if let Some(arr) = js.as_array() {
        let mut out = Vec::with_capacity(arr.len());
        for i in 0..arr.len() {
            let v: rquickjs::Value<'_> = arr.get(i).map_err(|e| e.to_string())?;
            match v
                .as_number()
                .or_else(|| v.as_bool().map(|b| f64::from(u8::from(b))))
            {
                Some(n) => out.push(n),
                None => {
                    return Err("Undefined value used in expression (could be an out of range array subscript?)".into());
                }
            }
        }
        out
    } else if js.is_undefined() {
        return Err("Expression result is undefined".into());
    } else {
        return Err("Expression result must be a number or an array".into());
    };
    if comps.iter().any(|v| !v.is_finite()) {
        return Err("Expression result is not a finite number".into());
    }
    let pick = |i: usize, fallback: f64| comps.get(i).copied().unwrap_or(fallback);
    Ok(match template {
        Value::Float(_) => Value::Float(pick(0, 0.0)),
        Value::Int(_) => Value::Int(pick(0, 0.0).round() as i64),
        Value::Bool(_) => Value::Bool(pick(0, 0.0) != 0.0),
        Value::Time(_) => Value::Time(Time::from_seconds(pick(0, 0.0))),
        Value::Vec2(t) => Value::Vec2([pick(0, t[0]), pick(1, t[1])]),
        Value::Vec3(t) => Value::Vec3([pick(0, t[0]), pick(1, t[1]), pick(2, t[2])]),
        Value::Color(t) => {
            Value::Color([pick(0, t[0]), pick(1, t[1]), pick(2, t[2]), pick(3, t[3])])
        }
        Value::FloatList(_) => Value::FloatList(comps),
        other => other.clone(),
    })
}

/// A path as expressions see it (`points()`, `value` of a path property):
/// points and tangents rounded to four decimals, as AE returns them.
fn path_to_js<'js>(ctx: &Ctx<'js>, p: &bae_model::BezierPath) -> rquickjs::Result<Object<'js>> {
    let round = |v: &[[f64; 2]]| -> Vec<Vec<f64>> {
        v.iter()
            .map(|p| p.iter().map(|x| (x * 1e4).round() / 1e4).collect())
            .collect()
    };
    let o = Object::new(ctx.clone())?;
    o.set("__path", true)?;
    o.set("points", round(&p.vertices))?;
    o.set("inTangents", round(&p.in_tangents))?;
    o.set("outTangents", round(&p.out_tangents))?;
    o.set("closed", p.closed)?;
    Ok(o)
}

/// A path returned by a path expression (`createPath(…)`, another path's
/// value); `old` keeps its mask feather points.
fn path_from_js(
    o: &Object<'_>,
    old: &bae_model::BezierPath,
) -> Result<bae_model::BezierPath, String> {
    let pairs = |k: &str| -> Result<Vec<[f64; 2]>, String> {
        let v: Option<Vec<Vec<f64>>> = o.get(k).map_err(|e| e.to_string())?;
        Ok(v.unwrap_or_default()
            .into_iter()
            .map(|p| {
                [
                    p.first().copied().unwrap_or(0.0),
                    p.get(1).copied().unwrap_or(0.0),
                ]
            })
            .collect())
    };
    let vertices = pairs("points")?;
    if vertices.is_empty() {
        return Err("Path must have at least one point".into());
    }
    let fill = |mut v: Vec<[f64; 2]>| {
        v.resize(vertices.len(), [0.0; 2]);
        v
    };
    Ok(bae_model::BezierPath {
        in_tangents: fill(pairs("inTangents")?),
        out_tangents: fill(pairs("outTangents")?),
        closed: o
            .get::<_, Option<bool>>("closed")
            .ok()
            .flatten()
            .unwrap_or(true),
        feather: old.feather.clone(),
        vertices,
    })
}

fn js_display(v: &rquickjs::Value<'_>) -> String {
    if let Some(n) = v.as_number() {
        let mut s = format!("{n}");
        if s.ends_with(".0") {
            s.truncate(s.len() - 2);
        }
        return s;
    }
    if let Some(b) = v.as_bool() {
        return b.to_string();
    }
    if let Some(arr) = v.as_array() {
        return (0..arr.len())
            .filter_map(|i| arr.get::<rquickjs::Value<'_>>(i).ok())
            .map(|x| js_display(&x))
            .collect::<Vec<_>>()
            .join(",");
    }
    String::new()
}

fn components(v: &Value) -> Vec<f64> {
    match v {
        Value::Float(x) => vec![*x],
        Value::Int(x) => vec![*x as f64],
        Value::Bool(b) => vec![f64::from(u8::from(*b))],
        Value::Time(t) => vec![t.as_seconds()],
        Value::Vec2(x) => x.to_vec(),
        Value::Vec3(x) => x.to_vec(),
        Value::Color(x) => x.to_vec(),
        Value::FloatList(x) => x.clone(),
        _ => vec![num(v)],
    }
}

// The property's random seed: FNV-1a over its identity (prim, 0, attr, comp
// id, layer index), stable across runs and machines. The prim and attr part is
// hashed once when the comp compiles ([`seed_prefix`]).

fn fnv(mut h: u32, bytes: impl IntoIterator<Item = u8>) -> u32 {
    for b in bytes {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// The FNV-1a state after the property's prim and attr.
pub(crate) fn seed_prefix(prim: &str, attr: &str) -> u32 {
    fnv(0x811c_9dc5, prim.bytes().chain([0]).chain(attr.bytes()))
}

fn seed_of(e: &ExprProp, comp: usize, layer: usize) -> u32 {
    seed_with(e.seed, comp, layer)
}

fn seed_with(prefix: u32, comp: usize, layer: usize) -> u32 {
    fnv(
        prefix,
        (comp as u32)
            .to_le_bytes()
            .into_iter()
            .chain((layer as u32).to_le_bytes()),
    )
}

// ---------------------------------------------------------------------------
// Host functions
// ---------------------------------------------------------------------------

fn throw<T>(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Result<T> {
    Err(rquickjs::Exception::throw_message(ctx, msg))
}

fn comp_or_throw(ctx: &Ctx<'_>, c: i32) -> rquickjs::Result<Arc<CompiledComp>> {
    match usize::try_from(c).ok().and_then(scope_comp) {
        Some(comp) => Ok(comp),
        None => throw(ctx, "composition is not available"),
    }
}

fn layer_or_throw(ctx: &Ctx<'_>, comp: &CompiledComp, l: i32) -> rquickjs::Result<usize> {
    match usize::try_from(l).ok().filter(|&l| l < comp.layers.len()) {
        Some(l) => Ok(l),
        None => throw(ctx, "layer index out of range"),
    }
}

/// Record that the expression on top of the stack read layer `(c, l)`.
fn note_dependency(c: usize, l: usize) {
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut()
            && let Some(top) = s.stack.last()
        {
            let from = (top.0, top.1);
            if from != (c, l) {
                s.deps.insert((from, (c, l)));
            }
        }
    });
}

/// Prim path of a property reference: empty means the layer itself.
fn own<'a>(prim: &'a str, layer: &'a crate::eval::CompiledLayer) -> &'a str {
    if prim.is_empty() {
        layer.path.as_str()
    } else {
        prim
    }
}

fn control_param_name(match_name: &str, name: &str) -> bool {
    let aliases = match match_name {
        "ADBE Slider Control" => ["Slider", "滑块"],
        "ADBE Point Control" => ["Point", "点"],
        "ADBE Point3D Control" => ["3D Point", "3D 点"],
        "ADBE Angle Control" => ["Angle", "角度"],
        "ADBE Checkbox Control" => ["Checkbox", "复选框"],
        "ADBE Color Control" => ["Color", "颜色"],
        "ADBE Layer Control" => ["Layer", "图层"],
        "ADBE Dropdown Control" => ["Menu", "菜单"],
        _ => return false,
    };
    aliases.contains(&name)
}

fn install_host<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<()> {
    let h = Object::new(ctx.clone())?;

    h.set(
        "prop",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String, t: f64, pre: bool| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                let Some(prop) = layer.find_prop(own(&prim, layer), &attr) else {
                    return throw(&ctx, &format!("property {attr} does not exist"));
                };
                note_dependency(c as usize, li);
                // AE's valueAtTime works on a grid of 1/1024 frame.
                let tick = comp.frame_rate.frame_duration().as_seconds() / 1024.0;
                let ct = Time::from_seconds((t / tick).round() * tick);
                let cx = Cx {
                    comp: c as usize,
                    layer: li,
                    ct,
                    lt: layer.timing.layer_time(ct),
                    js: Some(&ctx),
                };
                let v = if pre {
                    prop.sample_pre(&cx)
                } else {
                    prop.sample(&cx)
                };
                // A path is an object (points, tangents, closed), as AE's.
                if let Value::Bezier(p) = &v {
                    return path_to_js(&ctx, p)?.into_js(&ctx);
                }
                let mut out = components(&v);
                if !layer.switches.three_d
                    && own(&prim, layer) == layer.path.as_str()
                    && matches!(attr.as_str(), tr::POSITION | tr::ANCHOR_POINT | tr::SCALE)
                {
                    out.truncate(2);
                }
                out.into_js(&ctx)
            },
        )?,
    )?;

    h.set(
        "content",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, parent: String, which: rquickjs::Value<'js>| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                // The items of the layer's Contents, or of a group's Contents.
                let items: &[crate::eval::CompiledPrim] = if parent.is_empty() {
                    &layer.contents
                } else {
                    layer
                        .contents
                        .iter()
                        .find_map(|p| p.find_prim(&parent))
                        .and_then(|g| g.children.iter().find(|c| c.path.name() == "contents"))
                        .map_or(&[], |f| f.children.as_slice())
                };
                let name_of = |p: &crate::eval::CompiledPrim| {
                    p.static_str("name").unwrap_or(p.path.name()).to_owned()
                };
                let found = if let Some(n) = which.as_number() {
                    items.get((n as usize).wrapping_sub(1))
                } else {
                    let name = which
                        .as_string()
                        .and_then(|s| s.to_string().ok())
                        .unwrap_or_default();
                    items.iter().find(|p| name_of(p) == name)
                };
                let Some(item) = found else {
                    return Ok::<_, rquickjs::Error>(None);
                };
                let o = Object::new(ctx.clone())?;
                o.set("path", item.path.as_str())?;
                o.set("type", item.prim_type.as_ref().map_or("", |t| t.as_str()))?;
                o.set("op", item.static_str("operator").unwrap_or(""))?;
                o.set("name", name_of(item))?;
                Ok(Some(o))
            },
        )?,
    )?;

    h.set(
        "pathAt",
        Function::new(
            ctx.clone(),
            // `args`: percentage, time, kind (0 point, 1 tangent, 2 normal).
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String, args: Vec<f64>| {
                let (pct, t, kind) = (args[0], args[1], args[2] as i32);
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                let Some(prop) = layer.find_prop(own(&prim, layer), &attr) else {
                    return throw(&ctx, &format!("property {attr} does not exist"));
                };
                note_dependency(c as usize, li);
                let ct = Time::from_seconds(t);
                let cx = Cx {
                    comp: c as usize,
                    layer: li,
                    ct,
                    lt: layer.timing.layer_time(ct),
                    js: Some(&ctx),
                };
                let Value::Bezier(p) = prop.sample(&cx) else {
                    return throw(&ctx, "not a path property");
                };
                let Some((point, tangent)) = path::at(&p, pct) else {
                    return throw(&ctx, "the path has no points");
                };
                Ok::<_, rquickjs::Error>(match kind {
                    0 => point.to_vec(),
                    1 => tangent.to_vec(),
                    _ => path::normal(tangent).to_vec(),
                })
            },
        )?,
    )?;

    h.set(
        "vel",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String, t: f64, pre: bool| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                let Some(prop) = layer.find_prop(own(&prim, layer), &attr) else {
                    return throw(&ctx, &format!("property {attr} does not exist"));
                };
                note_dependency(c as usize, li);
                // AE: forward difference over one step of its 1/1024-frame time grid,
                // from the quantized time. Flicks cannot hold that grid exactly, so the
                // quotient uses the rounded times actually sampled.
                let tick = comp.frame_rate.frame_duration().as_seconds() / 1024.0;
                let q = (t / tick).round() * tick;
                let (t0, t1) = (Time::from_seconds(q), Time::from_seconds(q + tick));
                let at = |ct: Time| {
                    let cx = Cx {
                        comp: c as usize,
                        layer: li,
                        ct,
                        lt: layer.timing.layer_time(ct),
                        js: Some(&ctx),
                    };
                    components(&if pre {
                        prop.sample_pre(&cx)
                    } else {
                        prop.sample(&cx)
                    })
                };
                let (a, b) = (at(t0), at(t1));
                let dt = (t1 - t0).as_seconds();
                let mut out: Vec<f64> = a.iter().zip(&b).map(|(a, b)| (b - a) / dt).collect();
                if !layer.switches.three_d
                    && own(&prim, layer) == layer.path.as_str()
                    && matches!(attr.as_str(), tr::POSITION | tr::ANCHOR_POINT | tr::SCALE)
                {
                    out.truncate(2);
                }
                Ok::<_, rquickjs::Error>(out)
            },
        )?,
    )?;

    h.set(
        "keys",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                let mut out = Vec::new();
                if let Some(curve) = layer
                    .find_prop(own(&prim, layer), &attr)
                    .and_then(|p| p.curve())
                {
                    for (t, v) in curve.keys() {
                        out.push(layer.timing.comp_time(t).as_seconds());
                        out.extend(components(v));
                    }
                }
                Ok::<_, rquickjs::Error>(out)
            },
        )?,
    )?;

    h.set(
        "dims",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                let cx = Cx {
                    comp: c as usize,
                    layer: li,
                    ct: Time::ZERO,
                    lt: Time::ZERO,
                    js: None,
                };
                Ok::<_, rquickjs::Error>(
                    layer
                        .find_prop(own(&prim, layer), &attr)
                        .map_or(1, |p| components(&p.sample_pre(&cx)).len())
                        as i32,
                )
            },
        )?,
    )?;

    h.set(
        "layerIndex",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, which: rquickjs::Value<'js>| {
                let comp = comp_or_throw(&ctx, c)?;
                if let Some(n) = which.as_number() {
                    let i = n as i64 - 1;
                    return Ok::<_, rquickjs::Error>(
                        if i >= 0 && (i as usize) < comp.layers.len() {
                            i as i32
                        } else {
                            -1
                        },
                    );
                }
                let name = which
                    .as_string()
                    .and_then(|s| s.to_string().ok())
                    .unwrap_or_default();
                Ok::<_, rquickjs::Error>(
                    comp.layers
                        .iter()
                        .position(|l| *l.name == *name)
                        .map_or(-1, |i| i as i32),
                )
            },
        )?,
    )?;

    h.set(
        "layerInfo",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, c: i32, l: i32| {
            let comp = comp_or_throw(&ctx, c)?;
            let li = layer_or_throw(&ctx, &comp, l)?;
            let layer = &comp.layers[li];
            let o = Object::new(ctx.clone())?;
            o.set("path", layer.path.as_str())?;
            o.set("name", &*layer.name)?;
            o.set("inPoint", layer.timing.in_point.as_seconds())?;
            o.set("outPoint", layer.timing.out_point.as_seconds())?;
            o.set("startTime", layer.timing.start_time.as_seconds())?;
            o.set("stretch", layer.timing.stretch)?;
            o.set("parent", layer.parent.map_or(-1, |p| p as i32))?;
            o.set("enabled", layer.switches.enabled)?;
            let (w, h) = layer.source_size(&comp);
            o.set("width", w)?;
            o.set("height", h)?;
            let source = layer
                .source_comp()
                .and_then(scope_comp_id)
                .map_or(-1, |i| i as i32);
            o.set("sourceComp", source)?;
            Ok::<_, rquickjs::Error>(o)
        })?,
    )?;

    h.set(
        "compInfo",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, c: i32| {
            let comp = comp_or_throw(&ctx, c)?;
            let o = Object::new(ctx.clone())?;
            o.set("name", comp.name.as_str())?;
            o.set("width", comp.width)?;
            o.set("height", comp.height)?;
            o.set("duration", comp.duration.as_seconds())?;
            o.set(
                "frameDuration",
                comp.frame_rate.frame_duration().as_seconds(),
            )?;
            o.set("pixelAspect", comp.pixel_aspect)?;
            o.set("numLayers", comp.layers.len() as i32)?;
            o.set("displayStartTime", 0.0)?;
            o.set("bgColor", comp.bg_color.to_vec())?;
            o.set("shutterAngle", comp.motion_blur.shutter_angle)?;
            o.set("shutterPhase", comp.motion_blur.shutter_phase)?;
            let cam = comp
                .layers
                .iter()
                .position(|l| l.kind == bae_model::LayerKind::Camera && l.switches.enabled)
                .map_or(-1, |i| i as i32);
            o.set("activeCamera", cam)?;
            Ok::<_, rquickjs::Error>(o)
        })?,
    )?;

    h.set(
        "compByName",
        Function::new(ctx.clone(), |name: String| {
            SCOPE.with(|s| {
                s.borrow()
                    .as_ref()
                    .and_then(|s| s.by_name.get(&name).copied())
                    .map_or(-1, |i| i as i32)
            })
        })?,
    )?;

    h.set(
        "effect",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, which: rquickjs::Value<'js>| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let effects = &comp.layers[li].effects;
                let found = if let Some(n) = which.as_number() {
                    effects.get((n as usize).wrapping_sub(1))
                } else {
                    let name = which
                        .as_string()
                        .and_then(|s| s.to_string().ok())
                        .unwrap_or_default();
                    effects
                        .iter()
                        .find(|e| e.name == name)
                        .or_else(|| effects.iter().find(|e| e.match_name == name))
                };
                let Some(e) = found else {
                    return Ok::<_, rquickjs::Error>(None);
                };
                let info = Object::new(ctx.clone())?;
                info.set("path", e.path.as_str())?;
                info.set("active", e.enabled)?;
                info.set("layerControl", e.match_name == "ADBE Layer Control")?;
                Ok(Some(info))
            },
        )?,
    )?;

    h.set(
        "param",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, effect: String, which: rquickjs::Value<'js>| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let Some(e) = comp.layers[li]
                    .effects
                    .iter()
                    .find(|e| e.path.as_str() == effect)
                else {
                    return Ok::<_, rquickjs::Error>(None);
                };
                let id = if let Some(n) = which.as_number() {
                    format!("{:04}", n as i64)
                } else {
                    let s = which
                        .as_string()
                        .and_then(|s| s.to_string().ok())
                        .unwrap_or_default();
                    // The model stores parameter IDs only. Recognize the expression
                    // control names, and keep existing matchName and ID lookups.
                    if control_param_name(&e.match_name, &s) {
                        "0001".into()
                    } else {
                        s.rsplit('-').next().unwrap_or(&s).to_owned()
                    }
                };
                if e.match_name == "ADBE Layer Control" && id == "0001" {
                    return Ok::<_, rquickjs::Error>(Some("param:0001".to_owned()));
                }
                Ok::<_, rquickjs::Error>(
                    e.params
                        .iter()
                        .find(|(i, _)| *i == id)
                        .map(|(i, _)| format!("param:{i}")),
                )
            },
        )?,
    )?;

    h.set(
        "effectLayer",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String, t: f64| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let layer = &comp.layers[li];
                note_dependency(c as usize, li);
                let Some(prop) = layer.find_prop(&prim, &attr) else {
                    return Ok::<_, rquickjs::Error>(li as i32);
                };
                let ct = Time::from_seconds(t);
                let cx = Cx {
                    comp: c as usize,
                    layer: li,
                    ct,
                    lt: layer.timing.layer_time(ct),
                    js: Some(&ctx),
                };
                let value = prop.sample(&cx);
                let index = match &value {
                    Value::PrimRef(path) => {
                        comp.layers.iter().position(|layer| layer.path == *path)
                    }
                    // Older imports may store the original AE layer index.
                    Value::Int(index) => usize::try_from(index.saturating_sub(1))
                        .ok()
                        .filter(|i| *i < comp.layers.len()),
                    _ => None,
                };
                Ok(index.map_or(-1, |i| i as i32))
            },
        )?,
    )?;

    h.set(
        "mask",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, which: rquickjs::Value<'js>| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                let masks = &comp.layers[li].masks;
                let found = if let Some(n) = which.as_number() {
                    masks.get((n as usize).wrapping_sub(1))
                } else {
                    let name = which
                        .as_string()
                        .and_then(|s| s.to_string().ok())
                        .unwrap_or_default();
                    masks.iter().find(|m| m.name == name)
                };
                Ok::<_, rquickjs::Error>(found.map(|m| m.path.to_string()))
            },
        )?,
    )?;

    h.set(
        "xform",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, t: f64, kind: i32, p: Vec<f64>| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                note_dependency(c as usize, li);
                let ct = Time::from_seconds(t);
                let m = comp.world_at(li, ct, c as usize, Some(&ctx));
                let v = glam::DVec3::new(
                    p.first().copied().unwrap_or(0.0),
                    p.get(1).copied().unwrap_or(0.0),
                    p.get(2).copied().unwrap_or(0.0),
                );
                let r = match kind {
                    // toComp of a 3D layer is in the comp's view: through the active
                    // camera (AE 26: matches the default camera's projection exactly).
                    0 if comp.layers[li].switches.three_d => {
                        let w = m.transform_point3(v);
                        let (view, zoom) = comp.camera_at(ct, c as usize, Some(&ctx));
                        let pc = view.transform_point3(w);
                        let (cx, cy) = (comp.width as f64 / 2.0, comp.height as f64 / 2.0);
                        if pc.z.abs() < 1e-12 {
                            glam::DVec3::new(cx, cy, 0.0)
                        } else {
                            glam::DVec3::new(cx + pc.x * zoom / pc.z, cy + pc.y * zoom / pc.z, pc.z)
                        }
                    }
                    0 | 4 => m.transform_point3(v),
                    1 | 5 => m.inverse().transform_point3(v),
                    2 | 6 => m.transform_vector3(v),
                    _ => m.inverse().transform_vector3(v),
                };
                Ok::<_, rquickjs::Error>(r.to_array().to_vec())
            },
        )?,
    )?;

    h.set(
        "seed",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, c: i32, l: i32, prim: String, attr: String| {
                let comp = comp_or_throw(&ctx, c)?;
                let li = layer_or_throw(&ctx, &comp, l)?;
                Ok::<_, rquickjs::Error>(seed_with(
                    seed_prefix(own(&prim, &comp.layers[li]), &attr),
                    c as usize,
                    li,
                ))
            },
        )?,
    )?;

    ctx.globals().set("__h", h)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_declares_used_names_only() {
        let w = wrapper_source("value + wiggle(2, 10)").unwrap();
        assert!(w.contains("var value=thisProperty.__pre(time);"), "{w}");
        assert!(w.contains("var wiggle="), "{w}");
        assert!(!w.contains("var position="), "{w}");
        // User declarations shadow AE names.
        let w = wrapper_source("var scale = 2; value * scale").unwrap();
        assert!(!w.contains("var scale=thisLayer"), "{w}");
        // Undeclared assignments become locals (no leaking globals).
        let w = wrapper_source("freq = 2; freq").unwrap();
        assert!(w.contains("var freq;"), "{w}");
        // Only the AE objects it uses.
        let w = wrapper_source("thisComp.layer(index - 1).rotation + 1").unwrap();
        assert!(
            w.contains("var thisComp=") && w.contains("var thisLayer="),
            "{w}"
        );
        assert!(!w.contains("var thisProperty="), "{w}");
        let w = wrapper_source("time * 10").unwrap();
        assert!(!w.contains("var this"), "{w}");
    }

    #[test]
    fn syntax_errors_are_reported() {
        let e = wrapper_source("1 +").unwrap_err();
        assert!(e.contains("SyntaxError"), "{e}");
    }
}
