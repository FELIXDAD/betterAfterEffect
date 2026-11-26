//! Manifest expressions (spec §11.7): ROI margins and `identity_when`.
//!
//! ```text
//! expr  := or            or := and ("||" and)*        and := cmp ("&&" cmp)*
//! cmp   := sum (("=="|"!="|"<"|"<="|">"|">=") sum)?
//! sum   := prod (("+"|"-") prod)*                     prod := unary (("*"|"/") unary)*
//! unary := ("-"|"!") unary | atom
//! atom  := number | "params." key | "frame.downsample_x" | "frame.downsample_y"
//!        | func "(" expr ("," expr)* ")" | "(" expr ")"
//! func  := min | max | abs | floor | ceil | sqrt | clamp
//! ```
//!
//! Values are `f64`, booleans are 0/1. Expressions are parsed once at load;
//! evaluation walks the tree without allocating. Division by zero yields 0.

use std::fmt;

/// A parsed expression.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    nodes: Vec<Node>,
    root: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Node {
    Num(f64),
    /// Index into the scalar parameter keys given to [`Expr::parse`].
    Param(usize),
    InputAspect,
    DownsampleX,
    DownsampleY,
    Neg(usize),
    Not(usize),
    Bin(BinOp, usize, usize),
    Call1(Func1, usize),
    Call2(Func2, usize, usize),
    Clamp(usize, usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Func1 {
    Abs,
    Floor,
    Ceil,
    Sqrt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Func2 {
    Min,
    Max,
}

/// Values an expression reads.
pub trait ExprEnv {
    /// Value of the `index`-th scalar parameter key (as passed to [`Expr::parse`]).
    fn param(&self, index: usize) -> f64;
    /// Width divided by height of the input image.
    fn input_aspect(&self) -> f64;
    /// `frame.downsample_x` / `frame.downsample_y` (1 at full resolution).
    fn downsample(&self) -> [f64; 2];
}

/// A syntax error with its byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError {
    /// Byte offset in the source.
    pub offset: usize,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.offset)
    }
}

impl std::error::Error for ExprError {}

impl Expr {
    /// Parses `src`. `params` are the keys `params.<key>` may name (scalar
    /// params: float, int, angle, checkbox, popup); a reference stores the index.
    pub fn parse(src: &str, params: &[&str]) -> Result<Self, ExprError> {
        let mut p = Parser {
            src,
            pos: 0,
            params,
            nodes: Vec::new(),
        };
        let root = p.or()?;
        p.skip_ws();
        if p.pos != src.len() {
            return Err(p.error("unexpected input"));
        }
        Ok(Expr {
            nodes: p.nodes,
            root,
        })
    }

    /// Evaluates the expression.
    pub fn eval(&self, env: &dyn ExprEnv) -> f64 {
        self.node(self.root, env)
    }

    /// `true` if the value is non-zero.
    pub fn is_true(&self, env: &dyn ExprEnv) -> bool {
        self.eval(env) != 0.0
    }

    /// Indices of the parameters the expression reads.
    pub fn params(&self) -> impl Iterator<Item = usize> + '_ {
        self.nodes.iter().filter_map(|n| match n {
            Node::Param(i) => Some(*i),
            _ => None,
        })
    }

    fn node(&self, i: usize, env: &dyn ExprEnv) -> f64 {
        let b = |x: bool| f64::from(u8::from(x));
        match self.nodes[i] {
            Node::Num(x) => x,
            Node::Param(k) => env.param(k),
            Node::InputAspect => env.input_aspect(),
            Node::DownsampleX => env.downsample()[0],
            Node::DownsampleY => env.downsample()[1],
            Node::Neg(a) => -self.node(a, env),
            Node::Not(a) => b(self.node(a, env) == 0.0),
            Node::Bin(op, l, r) => {
                let x = self.node(l, env);
                // Short-circuit logic.
                match op {
                    BinOp::Or if x != 0.0 => return 1.0,
                    BinOp::And if x == 0.0 => return 0.0,
                    _ => {}
                }
                let y = self.node(r, env);
                match op {
                    BinOp::Or | BinOp::And => b(y != 0.0),
                    BinOp::Eq => b(x == y),
                    BinOp::Ne => b(x != y),
                    BinOp::Lt => b(x < y),
                    BinOp::Le => b(x <= y),
                    BinOp::Gt => b(x > y),
                    BinOp::Ge => b(x >= y),
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => {
                        if y == 0.0 {
                            0.0
                        } else {
                            x / y
                        }
                    }
                }
            }
            Node::Call1(f, a) => {
                let x = self.node(a, env);
                match f {
                    Func1::Abs => x.abs(),
                    Func1::Floor => x.floor(),
                    Func1::Ceil => x.ceil(),
                    Func1::Sqrt => x.max(0.0).sqrt(),
                }
            }
            Node::Call2(f, a, c) => {
                let (x, y) = (self.node(a, env), self.node(c, env));
                match f {
                    Func2::Min => x.min(y),
                    Func2::Max => x.max(y),
                }
            }
            Node::Clamp(a, lo, hi) => {
                let (x, lo, hi) = (self.node(a, env), self.node(lo, env), self.node(hi, env));
                x.max(lo).min(hi)
            }
        }
    }
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    params: &'a [&'a str],
    nodes: Vec<Node>,
}

impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> ExprError {
        ExprError {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn push(&mut self, n: Node) -> Result<usize, ExprError> {
        if self.nodes.len() >= 256 {
            return Err(self.error("expression too long (max 256 nodes)"));
        }
        self.nodes.push(n);
        Ok(self.nodes.len() - 1)
    }

    fn skip_ws(&mut self) {
        while self.src[self.pos..].starts_with([' ', '\t', '\n', '\r']) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, tok: &str) -> bool {
        self.skip_ws();
        if self.src[self.pos..].starts_with(tok) {
            self.pos += tok.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &str) -> Result<(), ExprError> {
        if self.eat(tok) {
            Ok(())
        } else {
            Err(self.error(format!("expected `{tok}`")))
        }
    }

    fn or(&mut self) -> Result<usize, ExprError> {
        let mut l = self.and()?;
        while self.eat("||") {
            let r = self.and()?;
            l = self.push(Node::Bin(BinOp::Or, l, r))?;
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<usize, ExprError> {
        let mut l = self.cmp()?;
        while self.eat("&&") {
            let r = self.cmp()?;
            l = self.push(Node::Bin(BinOp::And, l, r))?;
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<usize, ExprError> {
        let l = self.sum()?;
        let ops = [
            ("==", BinOp::Eq),
            ("!=", BinOp::Ne),
            ("<=", BinOp::Le),
            (">=", BinOp::Ge),
            ("<", BinOp::Lt),
            (">", BinOp::Gt),
        ];
        for (tok, op) in ops {
            if self.eat(tok) {
                let r = self.sum()?;
                return self.push(Node::Bin(op, l, r));
            }
        }
        Ok(l)
    }

    fn sum(&mut self) -> Result<usize, ExprError> {
        let mut l = self.prod()?;
        loop {
            let op = if self.eat("+") {
                BinOp::Add
            } else if self.eat("-") {
                BinOp::Sub
            } else {
                return Ok(l);
            };
            let r = self.prod()?;
            l = self.push(Node::Bin(op, l, r))?;
        }
    }

    fn prod(&mut self) -> Result<usize, ExprError> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat("*") {
                BinOp::Mul
            } else if self.eat("/") {
                BinOp::Div
            } else {
                return Ok(l);
            };
            let r = self.unary()?;
            l = self.push(Node::Bin(op, l, r))?;
        }
    }

    fn unary(&mut self) -> Result<usize, ExprError> {
        if self.eat("-") {
            let a = self.unary()?;
            return self.push(Node::Neg(a));
        }
        self.skip_ws();
        if self.src[self.pos..].starts_with('!') && !self.src[self.pos..].starts_with("!=") {
            self.pos += 1;
            let a = self.unary()?;
            return self.push(Node::Not(a));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<usize, ExprError> {
        self.skip_ws();
        let rest = &self.src[self.pos..];
        if self.eat("(") {
            let e = self.or()?;
            self.expect(")")?;
            return Ok(e);
        }
        if rest.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            let len = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E'))
                .unwrap_or(rest.len());
            let text = &rest[..len];
            let x: f64 = text
                .parse()
                .map_err(|_| self.error(format!("invalid number `{text}`")))?;
            self.pos += len;
            return self.push(Node::Num(x));
        }
        let len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(rest.len());
        let word = &rest[..len];
        if word.is_empty() {
            return Err(self.error("expected a number, `params.<key>`, a function or `(`"));
        }
        let start = self.pos;
        self.pos += len;
        if let Some(key) = word.strip_prefix("params.") {
            return match self.params.iter().position(|k| *k == key) {
                Some(i) => self.push(Node::Param(i)),
                None => {
                    self.pos = start;
                    Err(self.error(format!(
                        "unknown param `{key}` (expressions can read float, int, angle, checkbox and popup params)"
                    )))
                }
            };
        }
        match word {
            "frame.input_aspect" => return self.push(Node::InputAspect),
            "frame.downsample_x" => return self.push(Node::DownsampleX),
            "frame.downsample_y" => return self.push(Node::DownsampleY),
            "true" => return self.push(Node::Num(1.0)),
            "false" => return self.push(Node::Num(0.0)),
            _ => {}
        }
        let f1 = match word {
            "abs" => Some(Func1::Abs),
            "floor" => Some(Func1::Floor),
            "ceil" => Some(Func1::Ceil),
            "sqrt" => Some(Func1::Sqrt),
            _ => None,
        };
        let f2 = match word {
            "min" => Some(Func2::Min),
            "max" => Some(Func2::Max),
            _ => None,
        };
        if f1.is_none() && f2.is_none() && word != "clamp" {
            self.pos = start;
            return Err(self.error(format!("unknown name `{word}`")));
        }
        self.expect("(")?;
        let a = self.or()?;
        if let Some(f) = f1 {
            self.expect(")")?;
            return self.push(Node::Call1(f, a));
        }
        self.expect(",")?;
        let b = self.or()?;
        if let Some(f) = f2 {
            self.expect(")")?;
            return self.push(Node::Call2(f, a, b));
        }
        self.expect(",")?;
        let c = self.or()?;
        self.expect(")")?;
        self.push(Node::Clamp(a, b, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Env(Vec<f64>, [f64; 2]);
    impl ExprEnv for Env {
        fn param(&self, i: usize) -> f64 {
            self.0[i]
        }
        fn input_aspect(&self) -> f64 {
            2.0
        }
        fn downsample(&self) -> [f64; 2] {
            self.1
        }
    }

    fn eval(src: &str, params: &[(&str, f64)]) -> f64 {
        let keys: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
        let env = Env(params.iter().map(|(_, v)| *v).collect(), [0.5, 0.25]);
        Expr::parse(src, &keys).unwrap().eval(&env)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(eval("1 + 2 * 3", &[]), 7.0);
        assert_eq!(eval("(1 + 2) * 3", &[]), 9.0);
        assert_eq!(eval("-2 * -3", &[]), 6.0);
        assert_eq!(eval("10 / 4 - 1", &[]), 1.5);
        assert_eq!(eval("1 / 0", &[]), 0.0);
        assert_eq!(eval("1e2 + .5", &[]), 100.5);
    }

    #[test]
    fn params_frame_and_functions() {
        let p = [("radius", 7.2), ("mode", 2.0)];
        assert_eq!(eval("ceil(params.radius)", &p), 8.0);
        assert_eq!(eval("params.radius * frame.downsample_x", &p), 3.6);
        assert_eq!(eval("frame.downsample_y", &p), 0.25);
        assert_eq!(eval("max(params.radius, 10)", &p), 10.0);
        assert_eq!(eval("clamp(params.radius, 0, 5)", &p), 5.0);
        assert_eq!(eval("sqrt(16) + abs(-1) + floor(1.9)", &p), 6.0);
    }

    #[test]
    fn logic() {
        let p = [("amount", 0.0), ("mode", 2.0)];
        assert_eq!(eval("params.amount == 0", &p), 1.0);
        assert_eq!(eval("params.amount != 0 || params.mode == 2", &p), 1.0);
        assert_eq!(eval("params.amount == 0 && params.mode < 2", &p), 0.0);
        assert_eq!(eval("!(params.mode >= 2)", &p), 0.0);
        assert_eq!(eval("!false", &p), 1.0);
    }

    #[test]
    fn errors() {
        let keys = ["radius"];
        for bad in [
            "",
            "params.nope",
            "1 +",
            "foo(1)",
            "min(1)",
            "(1",
            "1 2",
            "params.radius ==",
            "clamp(1, 2)",
        ] {
            assert!(Expr::parse(bad, &keys).is_err(), "{bad:?}");
        }
        let e = Expr::parse("1 + params.radiuss", &keys).unwrap_err();
        assert_eq!(e.offset, 4);
        let e = Expr::parse("params.radius + params.radius", &keys).unwrap();
        assert_eq!(e.params().collect::<Vec<_>>(), vec![0, 0]);
    }
}
