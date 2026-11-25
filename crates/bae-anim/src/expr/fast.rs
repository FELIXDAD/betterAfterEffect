//! Expressions evaluated in Rust without entering JavaScript: arithmetic on
//! numbers, `time`, `value` and array literals, `Math.*`, `linear` / `ease` /
//! `easeIn` / `easeOut`, `wiggle(freq, amp[, octaves[, ampMult]])` and the
//! `loopIn` / `loopOut` family. The source is recognised once when the comp
//! compiles; everything else (statements, other names) runs in QuickJS.
//!
//! The results equal the JavaScript prelude's: the same AE vector arithmetic
//! (`$bm_sum` …), noise and seeds. An evaluation the fast path cannot finish
//! exactly (a shape it does not handle) returns `None` and falls back to JS.

/// A compiled fast expression.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fast(Node);

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Num(f64),
    Str(String),
    Time,
    Value,
    Arr(Vec<Node>),
    Neg(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    Call(Fun, Vec<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fun {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    Wiggle,
    /// (out, by duration)
    Loop(bool, bool),
    Math(MathFn),
    DegToRad,
    RadToDeg,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MathFn {
    Abs,
    Floor,
    Ceil,
    Round,
    Sqrt,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Exp,
    Log,
    Pow,
    Min,
    Max,
}

/// A value while evaluating: a number or an array (AE vectors).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum V {
    N(f64),
    A(Vec<f64>),
    S(String),
}

/// What an expression reads from its property and layer.
pub(crate) trait Env {
    /// `time` (comp seconds).
    fn now(&self) -> f64;
    /// The pre-expression value at comp time `t` (seconds), as JS sees it.
    fn pre(&self, t: f64) -> Vec<f64>;
    /// Keyframes: comp time (seconds) and value, in order.
    fn keys(&self) -> Vec<(f64, Vec<f64>)>;
    /// Pre-expression velocity at `t` (AE's forward difference).
    fn pre_velocity(&self, t: f64) -> Vec<f64>;
    /// The property's random seed (`wiggle`).
    fn seed(&self) -> u32;
}

impl Fast {
    pub(crate) fn compile(source: &str) -> Option<Fast> {
        let tokens = lex(source)?;
        let mut p = Parser { t: &tokens, i: 0 };
        let node = p.expr()?;
        while p.peek() == Some(&Tok::Semi) {
            p.i += 1;
        }
        (p.i == tokens.len()).then_some(Fast(node))
    }

    /// The result's components, or `None` when JS has to evaluate it.
    pub(crate) fn eval(&self, env: &dyn Env) -> Option<Vec<f64>> {
        match eval(&self.0, env)? {
            V::N(n) => Some(vec![n]),
            V::A(a) => Some(a),
            V::S(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Lexer and parser
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Punct(char),
    Semi,
}

fn lex(src: &str) -> Option<Vec<Tok>> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                i += 1;
                if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(Tok::Num(src[start..i].parse().ok()?));
        } else if c.is_ascii_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.' || b[i] == b'$') {
                i += 1;
            }
            out.push(Tok::Ident(src[start..i].to_owned()));
        } else if c == '"' || c == '\'' {
            let end = src[i + 1..].find(c)? + i + 1;
            let s = &src[i + 1..end];
            if s.contains('\\') {
                return None;
            }
            out.push(Tok::Str(s.to_owned()));
            i = end + 1;
        } else if "()[],+-*/%".contains(c) {
            // Comments and anything else are left to JS.
            if c == '/' && matches!(b.get(i + 1), Some(b'/' | b'*')) {
                return None;
            }
            out.push(Tok::Punct(c));
            i += 1;
        } else if c == ';' {
            out.push(Tok::Semi);
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

struct Parser<'a> {
    t: &'a [Tok],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Punct(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Option<Node> {
        let mut lhs = self.term()?;
        loop {
            let op = if self.eat('+') {
                Op::Add
            } else if self.eat('-') {
                Op::Sub
            } else {
                return Some(lhs);
            };
            lhs = Node::Bin(op, Box::new(lhs), Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Option<Node> {
        let mut lhs = self.unary()?;
        loop {
            let op = if self.eat('*') {
                Op::Mul
            } else if self.eat('/') {
                Op::Div
            } else if self.eat('%') {
                Op::Rem
            } else {
                return Some(lhs);
            };
            lhs = Node::Bin(op, Box::new(lhs), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Option<Node> {
        if self.eat('-') {
            return Some(Node::Neg(Box::new(self.unary()?)));
        }
        if self.eat('+') {
            return self.unary();
        }
        self.primary()
    }

    fn list(&mut self, close: char) -> Option<Vec<Node>> {
        let mut items = Vec::new();
        if self.eat(close) {
            return Some(items);
        }
        loop {
            items.push(self.expr()?);
            if self.eat(close) {
                return Some(items);
            }
            if !self.eat(',') {
                return None;
            }
        }
    }

    fn primary(&mut self) -> Option<Node> {
        let tok = self.peek()?.clone();
        self.i += 1;
        match tok {
            Tok::Num(n) => Some(Node::Num(n)),
            Tok::Str(s) => Some(Node::Str(s)),
            Tok::Punct('(') => {
                let e = self.expr()?;
                self.eat(')').then_some(e)
            }
            Tok::Punct('[') => Some(Node::Arr(self.list(']')?)),
            Tok::Ident(name) => match name.as_str() {
                "time" => Some(Node::Time),
                "value" => Some(Node::Value),
                "Math.PI" => Some(Node::Num(std::f64::consts::PI)),
                "Math.E" => Some(Node::Num(std::f64::consts::E)),
                _ => {
                    let fun = function(&name)?;
                    if !self.eat('(') {
                        return None;
                    }
                    Some(Node::Call(fun, self.list(')')?))
                }
            },
            _ => None,
        }
    }
}

fn function(name: &str) -> Option<Fun> {
    use MathFn::*;
    Some(match name {
        "linear" => Fun::Linear,
        "ease" => Fun::Ease,
        "easeIn" => Fun::EaseIn,
        "easeOut" => Fun::EaseOut,
        "wiggle" => Fun::Wiggle,
        "loopOut" => Fun::Loop(true, false),
        "loopIn" => Fun::Loop(false, false),
        "loopOutDuration" => Fun::Loop(true, true),
        "loopInDuration" => Fun::Loop(false, true),
        "degreesToRadians" => Fun::DegToRad,
        "radiansToDegrees" => Fun::RadToDeg,
        _ => Fun::Math(match name.strip_prefix("Math.")? {
            "abs" => Abs,
            "floor" => Floor,
            "ceil" => Ceil,
            "round" => Round,
            "sqrt" => Sqrt,
            "sin" => Sin,
            "cos" => Cos,
            "tan" => Tan,
            "asin" => Asin,
            "acos" => Acos,
            "atan" => Atan,
            "atan2" => Atan2,
            "exp" => Exp,
            "log" => Log,
            "pow" => Pow,
            "min" => Min,
            "max" => Max,
            _ => return None,
        }),
    })
}

// ---------------------------------------------------------------------------
// Evaluation (the prelude's semantics)
// ---------------------------------------------------------------------------

/// A property value as JS sees it: one component is a number.
fn unwrap(c: Vec<f64>) -> V {
    if c.len() == 1 { V::N(c[0]) } else { V::A(c) }
}

fn bin(op: Op, a: V, b: V) -> Option<V> {
    use V::*;
    Some(match (op, a, b) {
        (Op::Add, N(x), N(y)) => N(x + y),
        (Op::Sub, N(x), N(y)) => N(x - y),
        (Op::Mul, N(x), N(y)) => N(x * y),
        (Op::Div, N(x), N(y)) => N(x / y),
        (Op::Rem, N(x), N(y)) => N(x % y),
        // $bm_sum / $bm_sub: a number goes to the first component.
        (Op::Add, A(mut a), N(y)) => {
            *a.first_mut()? += y;
            A(a)
        }
        (Op::Add, N(x), A(mut b)) => {
            *b.first_mut()? = x + b[0];
            A(b)
        }
        (Op::Sub, A(mut a), N(y)) => {
            *a.first_mut()? -= y;
            A(a)
        }
        (Op::Sub, N(x), A(mut b)) => {
            *b.first_mut()? = x - b[0];
            A(b)
        }
        (Op::Add | Op::Sub, A(a), A(b)) => {
            let n = a.len().max(b.len());
            let at = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
            A((0..n)
                .map(|i| if op == Op::Add { at(&a, i) + at(&b, i) } else { at(&a, i) - at(&b, i) })
                .collect())
        }
        // $bm_mul / $bm_div.
        (Op::Mul, A(a), N(y)) => A(a.into_iter().map(|x| x * y).collect()),
        (Op::Mul, N(x), A(b)) => A(b.into_iter().map(|y| x * y).collect()),
        (Op::Div, A(a), N(y)) => A(a.into_iter().map(|x| x / y).collect()),
        (Op::Div, N(x), A(b)) => A(b.into_iter().map(|y| x / y).collect()),
        (Op::Mul | Op::Div, A(a), A(b)) => A(a
            .iter()
            .zip(&b)
            .map(|(x, y)| if op == Op::Mul { x * y } else { x / y })
            .collect()),
        _ => return None,
    })
}

fn num(v: &V) -> Option<f64> {
    match v {
        V::N(n) => Some(*n),
        _ => None,
    }
}

/// JavaScript's Math.round (halves round up).
fn js_round(x: f64) -> f64 {
    let r = x.floor();
    if x - r >= 0.5 { r + 1.0 } else { r }
}

fn math(f: MathFn, args: &[f64]) -> Option<f64> {
    use MathFn::*;
    let a = |i: usize| args.get(i).copied();
    Some(match f {
        Abs => a(0)?.abs(),
        Floor => a(0)?.floor(),
        Ceil => a(0)?.ceil(),
        Round => js_round(a(0)?),
        Sqrt => a(0)?.sqrt(),
        Sin => a(0)?.sin(),
        Cos => a(0)?.cos(),
        Tan => a(0)?.tan(),
        Asin => a(0)?.asin(),
        Acos => a(0)?.acos(),
        Atan => a(0)?.atan(),
        Atan2 => a(0)?.atan2(a(1)?),
        Exp => a(0)?.exp(),
        Log => a(0)?.ln(),
        Pow => a(0)?.powf(a(1)?),
        Min => args.iter().copied().fold(f64::INFINITY, f64::min),
        Max => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

/// `__hash` of the prelude (32-bit integer mixing, as `Math.imul`).
pub(crate) fn hash(a: u32, b: u32) -> u32 {
    let mut h = a ^ b.wrapping_add(0x9e37_79b9).wrapping_mul(0x85eb_ca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 16)
}

/// `__noise1`: 1-D gradient noise in [-1, 1].
pub(crate) fn noise1(seed: u32, x: f64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let grad = |i: f64| (f64::from(hash(seed, i as i64 as u32)) / 4_294_967_296.0) * 2.0 - 1.0;
    let a = grad(i) * f;
    let b = grad(i + 1.0) * (f - 1.0);
    (a + (b - a) * u) * 2.0
}

fn interp(shape: fn(f64) -> f64, args: Vec<V>) -> Option<V> {
    let (t, t0, t1, mut v1, mut v2) = match args.len() {
        3 => {
            let mut it = args.into_iter();
            (num(&it.next()?)?, 0.0, 1.0, it.next()?, it.next()?)
        }
        5 => {
            let mut it = args.into_iter();
            (
                num(&it.next()?)?,
                num(&it.next()?)?,
                num(&it.next()?)?,
                it.next()?,
                it.next()?,
            )
        }
        _ => return None,
    };
    let (mut t0, mut t1) = (t0, t1);
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
        std::mem::swap(&mut v1, &mut v2);
    }
    let f = if t1 == t0 {
        if t < t0 { 0.0 } else { 1.0 }
    } else {
        (t - t0) / (t1 - t0)
    };
    let p = shape(f.clamp(0.0, 1.0));
    let delta = bin(Op::Sub, v2, v1.clone())?;
    bin(Op::Add, v1, bin(Op::Mul, delta, V::N(p))?)
}

fn wiggle(env: &dyn Env, args: &[f64]) -> Option<V> {
    let (freq, amp) = (*args.first()?, *args.get(1)?);
    let octaves = args.get(2).copied().unwrap_or(1.0);
    let mult = args.get(3).copied().unwrap_or(0.5);
    if args.len() > 4 {
        return None;
    }
    let t = env.now();
    let seed = env.seed();
    let w = |dim: usize| {
        let (mut s, mut a, mut f) = (0.0, 1.0, freq);
        for o in 0..(octaves.floor() as i64).max(1) {
            s += noise1(hash(seed, (dim as u32).wrapping_mul(131).wrapping_add(o as u32)), t * f) * a;
            a *= mult;
            f *= 2.0;
        }
        s * amp
    };
    Some(match unwrap(env.pre(t)) {
        V::N(x) => V::N(x + w(0)),
        V::A(a) => V::A(a.into_iter().enumerate().map(|(i, x)| x + w(i)).collect()),
        V::S(_) => return None,
    })
}

/// `__loop` of the prelude.
fn looped(env: &dyn Env, out: bool, by_duration: bool, args: &[V]) -> Option<V> {
    let kind = match args.first() {
        None => "cycle".to_owned(),
        Some(V::S(s)) => s.clone(),
        Some(_) => return None,
    };
    let n = match args.get(1) {
        None => 0.0,
        Some(V::N(n)) => *n,
        Some(_) => return None,
    };
    let (num_keys, duration) = if by_duration { (0.0, Some(n)) } else { (n, None) };
    let ks = env.keys();
    let t = env.now();
    let pre = |t: f64| unwrap(env.pre(t));
    if ks.len() < 2 {
        return Some(pre(t));
    }
    let count = ks.len() as f64;
    let pick = |k: f64| (k > 0.0 && k < count).then_some(k as usize);
    let (first, last);
    if out {
        last = ks[ks.len() - 1].0;
        first = match duration {
            Some(d) if d > 0.0 => last - d,
            _ => ks[pick(num_keys).map_or(0, |k| ks.len() - 1 - k)].0,
        };
        if t <= last {
            return Some(pre(t));
        }
    } else {
        first = ks[0].0;
        last = match duration {
            Some(d) if d > 0.0 => first + d,
            _ => ks[pick(num_keys).unwrap_or(ks.len() - 1)].0,
        };
        if t >= first {
            return Some(pre(t));
        }
    }
    let span = last - first;
    if span <= 0.0 {
        return Some(pre(t));
    }
    if kind == "continue" {
        let vel = |at: f64| unwrap(env.pre_velocity(at));
        return if out {
            bin(Op::Add, pre(last), bin(Op::Mul, vel(last - 0.001), V::N(t - last))?)
        } else {
            bin(Op::Sub, pre(first), bin(Op::Mul, vel(first + 0.001), V::N(first - t))?)
        };
    }
    let rel = if out { t - first } else { last - t };
    let cycles = (rel / span).floor();
    let r = rel - cycles * span;
    if kind == "pingpong" {
        let forward = cycles % 2.0 == 0.0;
        let local = match (out, forward) {
            (true, true) | (false, false) => first + r,
            _ => last - r,
        };
        return Some(pre(local));
    }
    let v = pre(if out { first + r } else { last - r });
    if kind == "offset" {
        let delta = bin(Op::Sub, pre(last), pre(first))?;
        let shift = bin(Op::Mul, delta, V::N(cycles))?;
        return bin(if out { Op::Add } else { Op::Sub }, v, shift);
    }
    Some(v)
}

fn eval(node: &Node, env: &dyn Env) -> Option<V> {
    Some(match node {
        Node::Num(n) => V::N(*n),
        Node::Str(s) => V::S(s.clone()),
        Node::Time => V::N(env.now()),
        Node::Value => unwrap(env.pre(env.now())),
        Node::Arr(items) => V::A(
            items
                .iter()
                .map(|n| num(&eval(n, env)?))
                .collect::<Option<Vec<f64>>>()?,
        ),
        Node::Neg(n) => V::N(-num(&eval(n, env)?)?),
        Node::Bin(op, a, b) => bin(*op, eval(a, env)?, eval(b, env)?)?,
        Node::Call(fun, args) => {
            let vals = args
                .iter()
                .map(|a| eval(a, env))
                .collect::<Option<Vec<V>>>()?;
            let nums = || vals.iter().map(num).collect::<Option<Vec<f64>>>();
            match fun {
                Fun::Linear => interp(|f| f, vals)?,
                Fun::Ease => interp(|f| f * f * (3.0 - 2.0 * f), vals)?,
                Fun::EaseIn => interp(|f| f * f * (2.0 - f), vals)?,
                Fun::EaseOut => interp(|f| f * (1.0 + f - f * f), vals)?,
                Fun::Wiggle => wiggle(env, &nums()?)?,
                Fun::Loop(out, by_duration) => looped(env, *out, *by_duration, &vals)?,
                Fun::Math(f) => V::N(math(*f, &nums()?)?),
                Fun::DegToRad => V::N(*nums()?.first()? * std::f64::consts::PI / 180.0),
                Fun::RadToDeg => V::N(*nums()?.first()? * 180.0 / std::f64::consts::PI),
            }
        }
    })
}
