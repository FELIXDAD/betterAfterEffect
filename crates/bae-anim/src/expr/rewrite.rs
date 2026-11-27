//! Source rewriting for AE expressions.
//!
//! AE's JavaScript engine gives arithmetic operators vector semantics (`value + [10,
//! 0]` adds component-wise) by rewriting `a + b` into `$bm_sum(a, b)` and so on. This
//! module does the same with a small JavaScript parser:
//!
//! - binary `+ - * / %` → `$bm_sum / $bm_sub / $bm_mul / $bm_div / $bm_mod`, unary `-x`
//!   → `$bm_neg(x)`, compound assignments `a += b` → `a = $bm_sum(a, b)`;
//! - an expression's result is the completion value of the script (the value of the last
//!   expression statement executed), so outside function bodies every expression
//!   statement `e;` becomes `__c = (e);` and the whole program becomes the body of a
//!   function that returns `__c`.
//!
//! The parser covers the JavaScript AE users write (ES5 plus `let`/`const`, arrow
//! functions, template literals, spread), including automatic semicolon insertion.
//! Template literal contents are copied verbatim.

use std::collections::HashSet;
use std::fmt::Write as _;

/// A syntax error, reported like AE reports expression errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SyntaxError at line {}: {}", self.line, self.message)
    }
}

/// A rewritten expression.
#[derive(Clone, Debug)]
pub struct Rewritten {
    /// Function body returning the completion value. Top-level statements keep their
    /// source line, so error line numbers match the user's text.
    pub body: String,
    /// Every identifier the expression reads or writes.
    pub idents: HashSet<String>,
    /// Names the expression declares (`var`/`let`/`const`, functions, parameters).
    pub declared: HashSet<String>,
    /// Names assigned without a declaration (sloppy-mode globals in plain JS); the
    /// wrapper declares them so expressions do not leak globals into each other.
    pub undeclared_assignments: Vec<String>,
}

/// Rewrite an expression into the body of a function returning its completion value.
pub fn rewrite(source: &str) -> Result<Rewritten, SyntaxError> {
    let tokens = lex(source)?;
    let mut p = Parser {
        t: tokens,
        i: 0,
        idents: HashSet::new(),
        declared: HashSet::new(),
        assigned: Vec::new(),
    };
    let mut stmts = Vec::new();
    while !p.at_end() {
        let line = p.t[p.i].line;
        stmts.push((line, p.statement()?));
    }
    let mut out = String::from("var __c;");
    let mut current = 1;
    let mut pr = Printer { out: &mut out };
    for (line, s) in &stmts {
        while current < *line {
            pr.w("\n");
            current += 1;
        }
        pr.stmt(s, true);
    }
    out.push_str("return __c;");
    let mut undeclared = Vec::new();
    for a in p.assigned {
        if !p.declared.contains(&a) && !undeclared.contains(&a) {
            undeclared.push(a);
        }
    }
    Ok(Rewritten {
        body: out,
        idents: p.idents,
        declared: p.declared,
        undeclared_assignments: undeclared,
    })
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(String),
    Str(String),
    Template(String),
    Regex(String),
    Punct(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    /// A line break precedes this token (for ASI and restricted productions).
    nl: bool,
    line: usize,
}

const PUNCTS: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=",
    "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
    "<<", ">>", "**", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%",
    "&", "|", "^", "!", "~", "?", ":", "=", ".", "@", "#",
];

const KEYWORDS_BEFORE_EXPR: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
];

/// Categories shown by the expression editor. Offsets returned by
/// [`expression_tokens`] are UTF-8 byte ranges in the original source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpressionToken {
    Keyword,
    Identifier,
    Number,
    String,
    Operator,
    Comment,
}

impl ExpressionToken {
    fn classify(tok: Option<&Tok>) -> Self {
        match tok {
            None => Self::Comment,
            Some(Tok::Ident(s))
                if matches!(
                    s.as_str(),
                    "as" | "async"
                        | "await"
                        | "break"
                        | "case"
                        | "catch"
                        | "class"
                        | "const"
                        | "continue"
                        | "debugger"
                        | "default"
                        | "delete"
                        | "do"
                        | "else"
                        | "export"
                        | "extends"
                        | "false"
                        | "finally"
                        | "for"
                        | "from"
                        | "function"
                        | "get"
                        | "if"
                        | "import"
                        | "in"
                        | "instanceof"
                        | "let"
                        | "new"
                        | "null"
                        | "of"
                        | "return"
                        | "set"
                        | "static"
                        | "super"
                        | "switch"
                        | "this"
                        | "throw"
                        | "true"
                        | "try"
                        | "typeof"
                        | "var"
                        | "void"
                        | "while"
                        | "with"
                        | "yield"
                ) =>
            {
                Self::Keyword
            }
            Some(Tok::Ident(_)) => Self::Identifier,
            Some(Tok::Num(_)) => Self::Number,
            Some(Tok::Str(_) | Tok::Template(_) | Tok::Regex(_)) => Self::String,
            Some(Tok::Punct(_) | Tok::Eof) => Self::Operator,
        }
    }
}

/// Scan the editable source with the evaluator's lexer, including comments and
/// literals that the user has not finished typing.
pub fn expression_tokens(src: &str) -> Vec<(std::ops::Range<usize>, ExpressionToken)> {
    let offsets: Vec<_> = src
        .char_indices()
        .map(|(i, _)| i)
        .chain([src.len()])
        .collect();
    let mut spans = Vec::new();
    let _ = scan::<true>(src, |start, end, tok| {
        spans.push((offsets[start]..offsets[end], ExpressionToken::classify(tok)));
    });
    spans
}

fn lex(src: &str) -> Result<Vec<Token>, SyntaxError> {
    scan::<false>(src, |_, _, _| {})
}

fn scan<const EDITING: bool>(
    src: &str,
    mut visit: impl FnMut(usize, usize, Option<&Tok>),
) -> Result<Vec<Token>, SyntaxError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut nl = false;
    let err = |line, m: &str| SyntaxError {
        line,
        message: m.to_owned(),
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' || c == '\r' || c == '\u{2028}' || c == '\u{2029}' {
            if c == '\n' || c == '\r' {
                line += 1;
                if c == '\r' && chars.get(i + 1) == Some(&'\n') {
                    i += 1;
                }
            }
            nl = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
            continue;
        }
        let start = i;
        // Comments.
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                i += 1;
            }
            visit(start, i, None);
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                if chars[i] == '\n' {
                    line += 1;
                    nl = true;
                }
                i += 1;
            }
            if i >= chars.len() && !EDITING {
                return Err(err(line, "unterminated comment"));
            }
            i = (i + 2).min(chars.len());
            visit(start, i, None);
            continue;
        }
        let tok = if c.is_alphabetic() || c == '_' || c == '$' || c == '\\' {
            while i < chars.len()
                && (chars[i].is_alphanumeric()
                    || chars[i] == '_'
                    || chars[i] == '$'
                    || chars[i] == '\\')
            {
                i += 1;
            }
            Tok::Ident(chars[start..i].iter().collect())
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            if c == '0' && matches!(chars.get(i + 1), Some('x' | 'X' | 'o' | 'O' | 'b' | 'B')) {
                i += 2;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
            } else {
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    i += 1;
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < chars.len() && chars[i] == 'n' {
                    i += 1;
                }
            }
            Tok::Num(chars[start..i].iter().collect())
        } else if c == '"' || c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    i += 1;
                }
                if i < chars.len() && (chars[i] == '\n') {
                    if EDITING {
                        break;
                    }
                    return Err(err(line, "unterminated string"));
                }
                i += 1;
            }
            if chars.get(i) == Some(&c) {
                i += 1;
            } else if EDITING {
                i = i.min(chars.len());
            } else {
                return Err(err(line, "unterminated string"));
            }
            Tok::Str(chars[start..i].iter().collect())
        } else if c == '`' {
            i += 1;
            let mut depth = 0usize;
            while i < chars.len() {
                match chars[i] {
                    '\\' => i += 1,
                    '`' if depth == 0 => break,
                    '$' if chars.get(i + 1) == Some(&'{') => {
                        depth += 1;
                        i += 1;
                    }
                    '}' if depth > 0 => depth -= 1,
                    '\n' => line += 1,
                    _ => {}
                }
                i += 1;
            }
            if i >= chars.len() && !EDITING {
                return Err(err(line, "unterminated template literal"));
            }
            i = (i + 1).min(chars.len());
            Tok::Template(chars[start..i].iter().collect())
        } else if c == '/' && regex_allowed(out.last()) {
            i += 1;
            let mut class = false;
            while i < chars.len() {
                match chars[i] {
                    '\\' => i += 1,
                    '[' => class = true,
                    ']' => class = false,
                    '/' if !class => break,
                    '\n' if EDITING => break,
                    '\n' => return Err(err(line, "unterminated regular expression")),
                    _ => {}
                }
                i += 1;
            }
            if chars.get(i) == Some(&'/') {
                i += 1;
            } else if EDITING {
                i = i.min(chars.len());
            } else {
                return Err(err(line, "unterminated regular expression"));
            }
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            Tok::Regex(chars[start..i].iter().collect())
        } else {
            let rest: String = chars[i..chars.len().min(i + 4)].iter().collect();
            match PUNCTS.iter().find(|p| rest.starts_with(**p)) {
                Some(p) => {
                    i += p.chars().count();
                    Tok::Punct(p)
                }
                None if EDITING => {
                    i += 1;
                    continue;
                }
                None => return Err(err(line, &format!("unexpected character '{c}'"))),
            }
        };
        visit(start, i, Some(&tok));
        out.push(Token { tok, nl, line });
        nl = false;
    }
    out.push(Token {
        tok: Tok::Eof,
        nl: true,
        line,
    });
    Ok(out)
}

#[cfg(test)]
#[test]
fn editor_spans_preserve_partial_literals_and_unicode_offsets() {
    use ExpressionToken::{Comment, Identifier, Number, String as Literal};
    let src = "値 / 2; /[)]/; 'unfinished\n next; /* [";
    let tokens: Vec<_> = expression_tokens(src)
        .into_iter()
        .map(|(r, k)| (&src[r], k))
        .collect();
    for expected in [
        ("値", Identifier),
        ("2", Number),
        ("/[)]/", Literal),
        ("'unfinished", Literal),
        ("next", Identifier),
        ("/* [", Comment),
    ] {
        assert!(tokens.contains(&expected), "{tokens:?}");
    }
    assert!(lex(src).is_err());
}

fn regex_allowed(prev: Option<&Token>) -> bool {
    match prev.map(|t| &t.tok) {
        None => true,
        Some(Tok::Num(_) | Tok::Str(_) | Tok::Template(_) | Tok::Regex(_)) => false,
        Some(Tok::Ident(s)) => KEYWORDS_BEFORE_EXPR.contains(&s.as_str()),
        Some(Tok::Punct(p)) => !matches!(*p, ")" | "]" | "}" | "++" | "--"),
        Some(Tok::Eof) => false,
    }
}

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Expr {
    /// Literal or identifier, printed verbatim.
    Atom(String),
    Array(Vec<Option<Expr>>),
    Object(Vec<PropDef>),
    Function {
        is_arrow: bool,
        is_async: bool,
        name: Option<String>,
        params: Vec<Expr>,
        body: FnBody,
    },
    Unary(&'static str, Box<Expr>),
    /// `typeof`, `delete`, `void`, `await`.
    UnaryWord(String, Box<Expr>),
    Update {
        op: &'static str,
        prefix: bool,
        arg: Box<Expr>,
    },
    Binary(&'static str, Box<Expr>, Box<Expr>),
    /// `instanceof` / `in`.
    BinaryWord(String, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        optional: bool,
    },
    New(Box<Expr>, Option<Vec<Expr>>),
    Member {
        obj: Box<Expr>,
        prop: String,
        optional: bool,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
        optional: bool,
    },
    Seq(Vec<Expr>),
    Spread(Box<Expr>),
    /// Tagged template: `tag\`...\``.
    Tagged(Box<Expr>, String),
}

#[derive(Clone, Debug)]
enum PropDef {
    KeyValue(PropKey, Expr),
    Shorthand(String),
    Method {
        kind: Option<String>,
        key: PropKey,
        params: Vec<Expr>,
        body: Vec<Stmt>,
    },
    Spread(Expr),
}

#[derive(Clone, Debug)]
enum PropKey {
    Atom(String),
    Computed(Expr),
}

#[derive(Clone, Debug)]
enum FnBody {
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
}

#[derive(Clone, Debug)]
enum Stmt {
    Var(String, Vec<(Expr, Option<Expr>)>),
    Expr(Expr),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For {
        init: Option<Box<Stmt>>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    ForIn {
        decl: Option<String>,
        target: Expr,
        of: bool,
        iter: Expr,
        body: Box<Stmt>,
    },
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Return(Option<Expr>),
    Break(Option<String>),
    Continue(Option<String>),
    Throw(Expr),
    Try {
        block: Vec<Stmt>,
        param: Option<Expr>,
        handler: Option<Vec<Stmt>>,
        finalizer: Option<Vec<Stmt>>,
    },
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Function(Expr),
    Labeled(String, Box<Stmt>),
    Empty,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    t: Vec<Token>,
    i: usize,
    idents: HashSet<String>,
    declared: HashSet<String>,
    assigned: Vec<String>,
}

type PResult<T> = Result<T, SyntaxError>;

/// Names bound by a binding pattern.
fn pattern_names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Atom(a) => out.push(a.clone()),
        Expr::Array(items) => items.iter().flatten().for_each(|i| pattern_names(i, out)),
        Expr::Object(props) => {
            for p in props {
                match p {
                    PropDef::KeyValue(_, v) | PropDef::Spread(v) => pattern_names(v, out),
                    PropDef::Shorthand(n) => out.push(n.clone()),
                    PropDef::Method { .. } => {}
                }
            }
        }
        Expr::Assign(_, t, _) | Expr::Spread(t) => pattern_names(t, out),
        _ => {}
    }
}

impl Parser {
    fn declare(&mut self, pattern: &Expr) {
        let mut names = Vec::new();
        pattern_names(pattern, &mut names);
        self.declared.extend(names);
    }

    fn peek(&self) -> &Tok {
        &self.t[self.i].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.t[(self.i + n).min(self.t.len() - 1)].tok
    }

    fn nl_before(&self) -> bool {
        self.t[self.i].nl
    }

    fn at_end(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }

    fn bump(&mut self) -> Tok {
        let t = self.t[self.i].tok.clone();
        if self.i + 1 < self.t.len() {
            self.i += 1;
        }
        t
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == w)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.is_word(w) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(SyntaxError {
            line: self.t[self.i].line,
            message: msg.into(),
        })
    }

    fn expect_punct(&mut self, p: &str) -> PResult<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            let found = describe(self.peek());
            self.error(format!("expected '{p}', found {found}"))
        }
    }

    fn ident(&mut self) -> PResult<String> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            other => {
                self.i -= 1;
                self.error(format!("expected identifier, found {}", describe(&other)))
            }
        }
    }

    /// Statement terminator with automatic semicolon insertion.
    fn semicolon(&mut self) -> PResult<()> {
        // Also before `else` on the same line: `if (a) b else c` is common in AE
        // expressions (the legacy ExtendScript engine accepted it).
        if self.eat_punct(";")
            || self.is_punct("}")
            || self.at_end()
            || self.nl_before()
            || self.is_word("else")
        {
            return Ok(());
        }
        let found = describe(self.peek());
        self.error(format!("expected ';', found {found}"))
    }

    fn statement(&mut self) -> PResult<Stmt> {
        if self.eat_punct(";") {
            return Ok(Stmt::Empty);
        }
        if self.is_punct("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if let Tok::Ident(w) = self.peek().clone() {
            match w.as_str() {
                "var" | "let" | "const"
                    if !matches!(
                        self.peek_at(1),
                        Tok::Punct("=" | "." | "(" | "[" | ";" | ",")
                    ) =>
                {
                    let s = self.var_decl()?;
                    self.semicolon()?;
                    return Ok(s);
                }
                "function" => {
                    let f = self.function(false, false)?;
                    return Ok(Stmt::Function(f));
                }
                "async" if matches!(self.peek_at(1), Tok::Ident(s) if s == "function") => {
                    self.bump();
                    let f = self.function(false, true)?;
                    return Ok(Stmt::Function(f));
                }
                "if" => {
                    self.bump();
                    self.expect_punct("(")?;
                    let test = self.expression()?;
                    self.expect_punct(")")?;
                    let then = Box::new(self.statement()?);
                    let otherwise = if self.eat_word("else") {
                        Some(Box::new(self.statement()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(test, then, otherwise));
                }
                "for" => return self.for_statement(),
                "while" => {
                    self.bump();
                    self.expect_punct("(")?;
                    let test = self.expression()?;
                    self.expect_punct(")")?;
                    return Ok(Stmt::While(test, Box::new(self.statement()?)));
                }
                "do" => {
                    self.bump();
                    let body = Box::new(self.statement()?);
                    if !self.eat_word("while") {
                        return self.error("expected 'while'");
                    }
                    self.expect_punct("(")?;
                    let test = self.expression()?;
                    self.expect_punct(")")?;
                    self.eat_punct(";");
                    return Ok(Stmt::DoWhile(body, test));
                }
                "return" => {
                    self.bump();
                    let arg = if self.is_punct(";")
                        || self.is_punct("}")
                        || self.at_end()
                        || self.nl_before()
                    {
                        None
                    } else {
                        Some(self.expression()?)
                    };
                    self.semicolon()?;
                    return Ok(Stmt::Return(arg));
                }
                "break" | "continue" => {
                    self.bump();
                    let label = match self.peek() {
                        Tok::Ident(l) if !self.nl_before() => {
                            let l = l.clone();
                            self.bump();
                            Some(l)
                        }
                        _ => None,
                    };
                    self.semicolon()?;
                    return Ok(if w == "break" {
                        Stmt::Break(label)
                    } else {
                        Stmt::Continue(label)
                    });
                }
                "throw" => {
                    self.bump();
                    let e = self.expression()?;
                    self.semicolon()?;
                    return Ok(Stmt::Throw(e));
                }
                "try" => {
                    self.bump();
                    let block = self.block()?;
                    let (mut param, mut handler, mut finalizer) = (None, None, None);
                    if self.eat_word("catch") {
                        if self.eat_punct("(") {
                            let p = self.binding_target()?;
                            self.declare(&p);
                            param = Some(p);
                            self.expect_punct(")")?;
                        }
                        handler = Some(self.block()?);
                    }
                    if self.eat_word("finally") {
                        finalizer = Some(self.block()?);
                    }
                    return Ok(Stmt::Try {
                        block,
                        param,
                        handler,
                        finalizer,
                    });
                }
                "switch" => {
                    self.bump();
                    self.expect_punct("(")?;
                    let disc = self.expression()?;
                    self.expect_punct(")")?;
                    self.expect_punct("{")?;
                    let mut cases = Vec::new();
                    while !self.eat_punct("}") {
                        let test = if self.eat_word("case") {
                            Some(self.expression()?)
                        } else if self.eat_word("default") {
                            None
                        } else {
                            return self.error("expected 'case' or 'default'");
                        };
                        self.expect_punct(":")?;
                        let mut body = Vec::new();
                        while !self.is_word("case")
                            && !self.is_word("default")
                            && !self.is_punct("}")
                        {
                            if self.at_end() {
                                return self.error("unterminated switch");
                            }
                            body.push(self.statement()?);
                        }
                        cases.push((test, body));
                    }
                    return Ok(Stmt::Switch(disc, cases));
                }
                _ => {
                    if matches!(self.peek_at(1), Tok::Punct(":")) && !is_reserved(&w) {
                        self.bump();
                        self.bump();
                        return Ok(Stmt::Labeled(w, Box::new(self.statement()?)));
                    }
                }
            }
        }
        let e = self.expression()?;
        self.semicolon()?;
        Ok(Stmt::Expr(e))
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect_punct("{")?;
        let mut out = Vec::new();
        while !self.eat_punct("}") {
            if self.at_end() {
                return self.error("missing '}'");
            }
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn var_decl(&mut self) -> PResult<Stmt> {
        let kind = self.ident()?;
        let mut decls = Vec::new();
        loop {
            let target = self.binding_target()?;
            self.declare(&target);
            let init = if self.eat_punct("=") {
                Some(self.assignment()?)
            } else {
                None
            };
            decls.push((target, init));
            if !self.eat_punct(",") {
                break;
            }
        }
        Ok(Stmt::Var(kind, decls))
    }

    /// Identifier or destructuring pattern (parsed as an array/object literal).
    fn binding_target(&mut self) -> PResult<Expr> {
        if self.is_punct("[") || self.is_punct("{") {
            self.primary()
        } else {
            Ok(Expr::Atom(self.ident()?))
        }
    }

    fn for_statement(&mut self) -> PResult<Stmt> {
        self.bump();
        self.expect_punct("(")?;
        // for (var x in/of ...) / for (x in/of ...)
        let save = self.i;
        let decl = match self.peek() {
            Tok::Ident(k) if k == "var" || k == "let" || k == "const" => {
                let k = k.clone();
                self.bump();
                Some(k)
            }
            _ => None,
        };
        if let Ok(target) = self.binding_target_or_lhs(decl.is_some())
            && (self.is_word("in") || self.is_word("of"))
        {
            if decl.is_some() {
                self.declare(&target);
            } else if let Expr::Atom(a) = &target {
                self.assigned.push(a.clone());
            }
            let of = self.is_word("of");
            self.bump();
            let iter = if of {
                self.assignment()?
            } else {
                self.expression()?
            };
            self.expect_punct(")")?;
            let body = Box::new(self.statement()?);
            return Ok(Stmt::ForIn {
                decl,
                target,
                of,
                iter,
                body,
            });
        }
        self.i = save;
        let init = if self.is_punct(";") {
            None
        } else if matches!(self.peek(), Tok::Ident(k) if k == "var" || k == "let" || k == "const") {
            Some(Box::new(self.var_decl_no_in()?))
        } else {
            Some(Box::new(Stmt::Expr(self.expression_no_in()?)))
        };
        self.expect_punct(";")?;
        let test = if self.is_punct(";") {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect_punct(";")?;
        let update = if self.is_punct(")") {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect_punct(")")?;
        let body = Box::new(self.statement()?);
        Ok(Stmt::For {
            init,
            test,
            update,
            body,
        })
    }

    fn binding_target_or_lhs(&mut self, decl: bool) -> PResult<Expr> {
        if decl {
            self.binding_target()
        } else {
            self.unary_postfix()
        }
    }

    fn var_decl_no_in(&mut self) -> PResult<Stmt> {
        // `in` cannot appear unparenthesised in a for-init; our expression parser stops at
        // `in` only via binary precedence, so parse normally (for-in was handled above).
        self.var_decl()
    }

    fn expression_no_in(&mut self) -> PResult<Expr> {
        self.expression()
    }

    fn function(&mut self, is_expr: bool, is_async: bool) -> PResult<Expr> {
        if !self.eat_word("function") {
            return self.error("expected 'function'");
        }
        self.eat_punct("*");
        let name = match self.peek() {
            Tok::Ident(n) => {
                let n = n.clone();
                self.bump();
                self.declared.insert(n.clone());
                Some(n)
            }
            _ if is_expr => None,
            _ => return self.error("expected function name"),
        };
        let params = self.params()?;
        let body = self.block()?;
        Ok(Expr::Function {
            is_arrow: false,
            is_async,
            name,
            params,
            body: FnBody::Block(body),
        })
    }

    fn params(&mut self) -> PResult<Vec<Expr>> {
        self.expect_punct("(")?;
        let mut params = Vec::new();
        while !self.eat_punct(")") {
            if self.eat_punct("...") {
                params.push(Expr::Spread(Box::new(self.binding_target()?)));
            } else {
                let target = self.binding_target()?;
                if self.eat_punct("=") {
                    let d = self.assignment()?;
                    params.push(Expr::Assign("=", Box::new(target), Box::new(d)));
                } else {
                    params.push(target);
                }
            }
            if !self.eat_punct(",") {
                self.expect_punct(")")?;
                break;
            }
        }
        for p in &params {
            self.declare(p);
        }
        Ok(params)
    }

    fn expression(&mut self) -> PResult<Expr> {
        let first = self.assignment()?;
        if !self.is_punct(",") {
            return Ok(first);
        }
        let mut list = vec![first];
        while self.eat_punct(",") {
            list.push(self.assignment()?);
        }
        Ok(Expr::Seq(list))
    }

    fn arrow_ahead(&self) -> bool {
        // `x =>` or `(...) =>`
        match self.peek() {
            Tok::Ident(_) => matches!(self.peek_at(1), Tok::Punct("=>")),
            Tok::Punct("(") => {
                let mut depth = 0usize;
                let mut j = self.i;
                while j < self.t.len() {
                    match &self.t[j].tok {
                        Tok::Punct("(" | "[" | "{") => depth += 1,
                        Tok::Punct(")" | "]" | "}") => {
                            depth -= 1;
                            if depth == 0 {
                                return matches!(
                                    self.t.get(j + 1).map(|t| &t.tok),
                                    Some(Tok::Punct("=>"))
                                );
                            }
                        }
                        Tok::Eof => return false,
                        _ => {}
                    }
                    j += 1;
                }
                false
            }
            _ => false,
        }
    }

    fn assignment(&mut self) -> PResult<Expr> {
        let is_async = self.is_word("async")
            && matches!(self.peek_at(1), Tok::Ident(_) | Tok::Punct("("))
            && !self.t[self.i + 1].nl;
        if is_async {
            let save = self.i;
            self.bump();
            if self.arrow_ahead() {
                return self.arrow(true);
            }
            self.i = save;
        }
        if self.arrow_ahead() {
            return self.arrow(false);
        }
        let lhs = self.conditional()?;
        if let Tok::Punct(op) = self.peek().clone()
            && matches!(
                op,
                "=" | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "**="
                    | "<<="
                    | ">>="
                    | ">>>="
                    | "&="
                    | "|="
                    | "^="
                    | "&&="
                    | "||="
                    | "??="
            )
        {
            self.bump();
            if let Expr::Atom(a) = &lhs {
                self.assigned.push(a.clone());
            }
            let rhs = self.assignment()?;
            return Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn arrow(&mut self, is_async: bool) -> PResult<Expr> {
        let params = if self.is_punct("(") {
            self.params()?
        } else {
            let name = self.ident()?;
            self.declared.insert(name.clone());
            vec![Expr::Atom(name)]
        };
        self.expect_punct("=>")?;
        let body = if self.is_punct("{") {
            FnBody::Block(self.block()?)
        } else {
            FnBody::Expr(Box::new(self.assignment()?))
        };
        Ok(Expr::Function {
            is_arrow: true,
            is_async,
            name: None,
            params,
            body,
        })
    }

    fn conditional(&mut self) -> PResult<Expr> {
        let test = self.binary(0)?;
        if self.eat_punct("?") {
            let a = self.assignment()?;
            self.expect_punct(":")?;
            let b = self.assignment()?;
            return Ok(Expr::Cond(Box::new(test), Box::new(a), Box::new(b)));
        }
        Ok(test)
    }

    fn binary_op(&self) -> Option<(BinOp, u8)> {
        let p = match self.peek() {
            Tok::Punct(p) => *p,
            Tok::Ident(w) if w == "instanceof" => {
                return Some((BinOp::Word("instanceof".into()), 9));
            }
            Tok::Ident(w) if w == "in" => return Some((BinOp::Word("in".into()), 9)),
            _ => return None,
        };
        let prec = match p {
            "??" => 1,
            "||" => 2,
            "&&" => 3,
            "|" => 4,
            "^" => 5,
            "&" => 6,
            "==" | "!=" | "===" | "!==" => 7,
            "<" | ">" | "<=" | ">=" => 9,
            "<<" | ">>" | ">>>" => 10,
            "+" | "-" => 11,
            "*" | "/" | "%" => 12,
            "**" => 13,
            _ => return None,
        };
        Some((BinOp::Punct(p), prec))
    }

    fn binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = self.binary_op() {
            if prec < min_prec {
                break;
            }
            self.bump();
            // `**` is right-associative.
            let next = if prec == 13 { prec } else { prec + 1 };
            let rhs = self.binary(next)?;
            lhs = match op {
                BinOp::Punct(p) => Expr::Binary(p, Box::new(lhs), Box::new(rhs)),
                BinOp::Word(w) => Expr::BinaryWord(w, Box::new(lhs), Box::new(rhs)),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        match self.peek().clone() {
            Tok::Punct(op @ ("!" | "~" | "+" | "-")) => {
                self.bump();
                Ok(Expr::Unary(op, Box::new(self.unary()?)))
            }
            Tok::Punct(op @ ("++" | "--")) => {
                self.bump();
                Ok(Expr::Update {
                    op,
                    prefix: true,
                    arg: Box::new(self.unary()?),
                })
            }
            Tok::Ident(w) if matches!(w.as_str(), "typeof" | "delete" | "void" | "await") => {
                self.bump();
                Ok(Expr::UnaryWord(w, Box::new(self.unary()?)))
            }
            _ => self.unary_postfix(),
        }
    }

    fn unary_postfix(&mut self) -> PResult<Expr> {
        let e = self.call_member()?;
        if let Tok::Punct(op @ ("++" | "--")) = self.peek().clone()
            && !self.nl_before()
        {
            self.bump();
            return Ok(Expr::Update {
                op,
                prefix: false,
                arg: Box::new(e),
            });
        }
        Ok(e)
    }

    fn args(&mut self) -> PResult<Vec<Expr>> {
        self.expect_punct("(")?;
        let mut out = Vec::new();
        while !self.eat_punct(")") {
            if self.eat_punct("...") {
                out.push(Expr::Spread(Box::new(self.assignment()?)));
            } else {
                out.push(self.assignment()?);
            }
            if !self.eat_punct(",") {
                self.expect_punct(")")?;
                break;
            }
        }
        Ok(out)
    }

    fn call_member(&mut self) -> PResult<Expr> {
        let mut e = if self.is_word("new") {
            self.bump();
            if self.eat_punct(".") {
                // new.target
                let p = self.ident()?;
                Expr::Atom(format!("new.{p}"))
            } else {
                let callee = self.member_only()?;
                let args = if self.is_punct("(") {
                    Some(self.args()?)
                } else {
                    None
                };
                Expr::New(Box::new(callee), args)
            }
        } else {
            self.primary()?
        };
        loop {
            if self.eat_punct(".") {
                let prop = self.property_name()?;
                e = Expr::Member {
                    obj: Box::new(e),
                    prop,
                    optional: false,
                };
            } else if self.eat_punct("?.") {
                if self.is_punct("(") {
                    let args = self.args()?;
                    e = Expr::Call {
                        callee: Box::new(e),
                        args,
                        optional: true,
                    };
                } else if self.eat_punct("[") {
                    let idx = self.expression()?;
                    self.expect_punct("]")?;
                    e = Expr::Index {
                        obj: Box::new(e),
                        index: Box::new(idx),
                        optional: true,
                    };
                } else {
                    let prop = self.property_name()?;
                    e = Expr::Member {
                        obj: Box::new(e),
                        prop,
                        optional: true,
                    };
                }
            } else if self.eat_punct("[") {
                let idx = self.expression()?;
                self.expect_punct("]")?;
                e = Expr::Index {
                    obj: Box::new(e),
                    index: Box::new(idx),
                    optional: false,
                };
            } else if self.is_punct("(") {
                let args = self.args()?;
                e = Expr::Call {
                    callee: Box::new(e),
                    args,
                    optional: false,
                };
            } else if let Tok::Template(t) = self.peek().clone() {
                self.bump();
                e = Expr::Tagged(Box::new(e), t);
            } else {
                break;
            }
        }
        Ok(e)
    }

    /// Callee of `new`: member expressions without calls.
    fn member_only(&mut self) -> PResult<Expr> {
        let mut e = if self.is_word("new") {
            self.bump();
            let callee = self.member_only()?;
            let args = if self.is_punct("(") {
                Some(self.args()?)
            } else {
                None
            };
            Expr::New(Box::new(callee), args)
        } else {
            self.primary()?
        };
        loop {
            if self.eat_punct(".") {
                let prop = self.property_name()?;
                e = Expr::Member {
                    obj: Box::new(e),
                    prop,
                    optional: false,
                };
            } else if self.eat_punct("[") {
                let idx = self.expression()?;
                self.expect_punct("]")?;
                e = Expr::Index {
                    obj: Box::new(e),
                    index: Box::new(idx),
                    optional: false,
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn property_name(&mut self) -> PResult<String> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            Tok::Punct("#") => Ok(format!("#{}", self.ident()?)),
            other => {
                self.i -= 1;
                self.error(format!(
                    "expected property name, found {}",
                    describe(&other)
                ))
            }
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.bump();
                Ok(Expr::Atom(n))
            }
            Tok::Str(s) | Tok::Template(s) | Tok::Regex(s) => {
                self.bump();
                Ok(Expr::Atom(s))
            }
            Tok::Ident(w) => {
                if w == "function" {
                    return self.function(true, false);
                }
                if w == "async" && matches!(self.peek_at(1), Tok::Ident(s) if s == "function") {
                    self.bump();
                    return self.function(true, true);
                }
                if w == "class" {
                    return self.error("classes are not supported in expressions");
                }
                self.bump();
                self.idents.insert(w.clone());
                Ok(Expr::Atom(w))
            }
            Tok::Punct("(") => {
                self.bump();
                let e = self.expression()?;
                self.expect_punct(")")?;
                Ok(e)
            }
            Tok::Punct("[") => {
                self.bump();
                let mut items = Vec::new();
                while !self.eat_punct("]") {
                    if self.is_punct(",") {
                        self.bump();
                        items.push(None);
                        continue;
                    }
                    let item = if self.eat_punct("...") {
                        Expr::Spread(Box::new(self.assignment()?))
                    } else {
                        self.assignment()?
                    };
                    items.push(Some(item));
                    if !self.eat_punct(",") {
                        self.expect_punct("]")?;
                        break;
                    }
                }
                Ok(Expr::Array(items))
            }
            Tok::Punct("{") => self.object(),
            other => self.error(format!("unexpected {}", describe(&other))),
        }
    }

    fn object(&mut self) -> PResult<Expr> {
        self.expect_punct("{")?;
        let mut props = Vec::new();
        while !self.eat_punct("}") {
            if self.eat_punct("...") {
                props.push(PropDef::Spread(self.assignment()?));
            } else {
                // get/set/async methods
                let mut kind = None;
                if let Tok::Ident(w) = self.peek().clone()
                    && matches!(w.as_str(), "get" | "set" | "async")
                    && !matches!(self.peek_at(1), Tok::Punct(":" | "(" | "," | "}"))
                {
                    self.bump();
                    kind = Some(w);
                }
                let key = match self.bump() {
                    Tok::Ident(s) | Tok::Str(s) | Tok::Num(s) => PropKey::Atom(s),
                    Tok::Punct("[") => {
                        let e = self.assignment()?;
                        self.expect_punct("]")?;
                        PropKey::Computed(e)
                    }
                    other => {
                        self.i -= 1;
                        return self
                            .error(format!("unexpected {} in object literal", describe(&other)));
                    }
                };
                if self.is_punct("(") {
                    let params = self.params()?;
                    let body = self.block()?;
                    props.push(PropDef::Method {
                        kind,
                        key,
                        params,
                        body,
                    });
                } else if self.eat_punct(":") {
                    props.push(PropDef::KeyValue(key, self.assignment()?));
                } else if let PropKey::Atom(name) = key {
                    if self.eat_punct("=") {
                        // Shorthand with default (only valid in patterns).
                        let d = self.assignment()?;
                        props.push(PropDef::KeyValue(
                            PropKey::Atom(name.clone()),
                            Expr::Assign("=", Box::new(Expr::Atom(name)), Box::new(d)),
                        ));
                    } else {
                        self.idents.insert(name.clone());
                        props.push(PropDef::Shorthand(name));
                    }
                } else {
                    return self.error("expected ':' in object literal");
                }
            }
            if !self.eat_punct(",") {
                self.expect_punct("}")?;
                break;
            }
        }
        Ok(Expr::Object(props))
    }
}

enum BinOp {
    Punct(&'static str),
    Word(String),
}

fn is_reserved(w: &str) -> bool {
    matches!(
        w,
        "var"
            | "let"
            | "const"
            | "function"
            | "if"
            | "else"
            | "for"
            | "while"
            | "do"
            | "return"
            | "break"
            | "continue"
            | "throw"
            | "try"
            | "catch"
            | "finally"
            | "switch"
            | "case"
            | "default"
            | "new"
            | "delete"
            | "typeof"
            | "void"
            | "in"
            | "instanceof"
            | "this"
            | "null"
            | "true"
            | "false"
    )
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) | Tok::Num(s) => format!("'{s}'"),
        Tok::Str(_) => "string".into(),
        Tok::Template(_) => "template literal".into(),
        Tok::Regex(_) => "regular expression".into(),
        Tok::Punct(p) => format!("'{p}'"),
        Tok::Eof => "end of expression".into(),
    }
}

// ---------------------------------------------------------------------------
// Printer
// ---------------------------------------------------------------------------

struct Printer<'a> {
    out: &'a mut String,
}

fn vector_fn(op: &str) -> Option<&'static str> {
    Some(match op {
        "+" => "$bm_sum",
        "-" => "$bm_sub",
        "*" => "$bm_mul",
        "/" => "$bm_div",
        "%" => "$bm_mod",
        _ => return None,
    })
}

impl Printer<'_> {
    fn w(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// `completion`: rewrite expression statements to `__c = (...)` (top level and
    /// blocks outside function bodies).
    fn stmt(&mut self, s: &Stmt, completion: bool) {
        match s {
            Stmt::Var(kind, decls) => {
                self.var(kind, decls);
                self.w(";");
            }
            Stmt::Expr(e) => {
                if completion {
                    self.w("__c=(");
                    self.expr(e);
                    self.w(");");
                } else {
                    // Avoid an expression statement starting with `{` or `function`.
                    self.w("(");
                    self.expr(e);
                    self.w(");");
                }
            }
            Stmt::Block(b) => {
                self.w("{");
                for s in b {
                    self.stmt(s, completion);
                }
                self.w("}");
            }
            Stmt::If(test, then, otherwise) => {
                self.w("if(");
                self.expr(test);
                self.w(")");
                self.stmt_as_block(then, completion);
                if let Some(o) = otherwise {
                    self.w("else ");
                    self.stmt_as_block(o, completion);
                }
            }
            Stmt::For {
                init,
                test,
                update,
                body,
            } => {
                self.w("for(");
                match init.as_deref() {
                    Some(Stmt::Var(kind, decls)) => self.var(kind, decls),
                    Some(Stmt::Expr(e)) => self.expr(e),
                    _ => {}
                }
                self.w(";");
                if let Some(t) = test {
                    self.expr(t);
                }
                self.w(";");
                if let Some(u) = update {
                    self.expr(u);
                }
                self.w(")");
                self.stmt_as_block(body, completion);
            }
            Stmt::ForIn {
                decl,
                target,
                of,
                iter,
                body,
            } => {
                self.w("for(");
                if let Some(d) = decl {
                    self.w(d);
                    self.w(" ");
                }
                self.pattern(target);
                self.w(if *of { " of " } else { " in " });
                self.expr(iter);
                self.w(")");
                self.stmt_as_block(body, completion);
            }
            Stmt::While(test, body) => {
                self.w("while(");
                self.expr(test);
                self.w(")");
                self.stmt_as_block(body, completion);
            }
            Stmt::DoWhile(body, test) => {
                self.w("do ");
                self.stmt_as_block(body, completion);
                self.w("while(");
                self.expr(test);
                self.w(");");
            }
            Stmt::Return(e) => {
                self.w("return");
                if let Some(e) = e {
                    self.w(" ");
                    self.expr(e);
                }
                self.w(";");
            }
            Stmt::Break(l) | Stmt::Continue(l) => {
                self.w(if matches!(s, Stmt::Break(_)) {
                    "break"
                } else {
                    "continue"
                });
                if let Some(l) = l {
                    self.w(" ");
                    self.w(l);
                }
                self.w(";");
            }
            Stmt::Throw(e) => {
                self.w("throw ");
                self.expr(e);
                self.w(";");
            }
            Stmt::Try {
                block,
                param,
                handler,
                finalizer,
            } => {
                self.w("try{");
                for s in block {
                    self.stmt(s, completion);
                }
                self.w("}");
                if let Some(h) = handler {
                    self.w("catch");
                    if let Some(p) = param {
                        self.w("(");
                        self.pattern(p);
                        self.w(")");
                    }
                    self.w("{");
                    for s in h {
                        self.stmt(s, completion);
                    }
                    self.w("}");
                }
                if let Some(f) = finalizer {
                    self.w("finally{");
                    for s in f {
                        self.stmt(s, completion);
                    }
                    self.w("}");
                }
            }
            Stmt::Switch(disc, cases) => {
                self.w("switch(");
                self.expr(disc);
                self.w("){");
                for (test, body) in cases {
                    match test {
                        Some(t) => {
                            self.w("case ");
                            self.expr(t);
                            self.w(":");
                        }
                        None => self.w("default:"),
                    }
                    for s in body {
                        self.stmt(s, completion);
                    }
                }
                self.w("}");
            }
            Stmt::Function(f) => self.function_decl(f),
            Stmt::Labeled(l, body) => {
                self.w(l);
                self.w(":");
                self.stmt(body, completion);
            }
            Stmt::Empty => self.w(";"),
        }
    }

    /// Function declarations stay declarations (they are hoisted).
    fn function_decl(&mut self, f: &Expr) {
        let Expr::Function {
            is_async,
            name,
            params,
            body: FnBody::Block(body),
            ..
        } = f
        else {
            self.expr(f);
            self.w(";");
            return;
        };
        if *is_async {
            self.w("async ");
        }
        self.w("function ");
        self.w(name.as_deref().unwrap_or(""));
        self.w("(");
        self.patterns(params);
        self.w(")");
        self.body(body);
    }

    /// Binding patterns: object patterns print without the parentheses an object
    /// literal expression gets.
    fn pattern(&mut self, e: &Expr) {
        match e {
            Expr::Object(_) => {
                let start = self.out.len();
                self.expr(e);
                // Strip the "(" ... ")" wrapper of the object literal.
                self.out.remove(start);
                self.out.pop();
            }
            Expr::Assign("=", t, d) => {
                self.pattern(t);
                self.w("=");
                self.expr(d);
            }
            Expr::Spread(t) => {
                self.w("...");
                self.pattern(t);
            }
            _ => self.expr(e),
        }
    }

    fn patterns(&mut self, items: &[Expr]) {
        for (i, a) in items.iter().enumerate() {
            if i > 0 {
                self.w(",");
            }
            self.pattern(a);
        }
    }

    fn stmt_as_block(&mut self, s: &Stmt, completion: bool) {
        self.w("{");
        self.stmt(s, completion);
        self.w("}");
    }

    fn var(&mut self, kind: &str, decls: &[(Expr, Option<Expr>)]) {
        self.w(kind);
        self.w(" ");
        for (i, (t, init)) in decls.iter().enumerate() {
            if i > 0 {
                self.w(",");
            }
            self.pattern(t);
            if let Some(e) = init {
                self.w("=");
                self.expr(e);
            }
        }
    }

    fn list(&mut self, items: &[Expr]) {
        for (i, a) in items.iter().enumerate() {
            if i > 0 {
                self.w(",");
            }
            self.expr(a);
        }
    }

    fn body(&mut self, b: &[Stmt]) {
        self.w("{");
        for s in b {
            self.stmt(s, false);
        }
        self.w("}");
    }

    /// Print an expression; compound sub-expressions are parenthesised.
    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Atom(a) => self.w(a),
            Expr::Array(items) => {
                self.w("[");
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        self.w(",");
                    }
                    if let Some(it) = it {
                        self.expr(it);
                    }
                }
                if matches!(items.last(), Some(None)) {
                    self.w(",");
                }
                self.w("]");
            }
            Expr::Object(props) => {
                self.w("({");
                for (i, p) in props.iter().enumerate() {
                    if i > 0 {
                        self.w(",");
                    }
                    match p {
                        PropDef::KeyValue(k, v) => {
                            self.key(k);
                            self.w(":");
                            self.expr(v);
                        }
                        PropDef::Shorthand(n) => self.w(n),
                        PropDef::Method {
                            kind,
                            key,
                            params,
                            body,
                        } => {
                            if let Some(k) = kind {
                                self.w(k);
                                self.w(" ");
                            }
                            self.key(key);
                            self.w("(");
                            self.patterns(params);
                            self.w(")");
                            self.body(body);
                        }
                        PropDef::Spread(e) => {
                            self.w("...");
                            self.expr(e);
                        }
                    }
                }
                self.w("})");
            }
            Expr::Function {
                is_arrow,
                is_async,
                name,
                params,
                body,
            } => {
                self.w("(");
                if *is_async {
                    self.w("async ");
                }
                if *is_arrow {
                    self.w("(");
                    self.patterns(params);
                    self.w(")=>");
                    match body {
                        FnBody::Block(b) => self.body(b),
                        FnBody::Expr(e) => {
                            self.w("(");
                            self.expr(e);
                            self.w(")");
                        }
                    }
                } else {
                    self.w("function");
                    if let Some(n) = name {
                        self.w(" ");
                        self.w(n);
                    }
                    self.w("(");
                    self.patterns(params);
                    self.w(")");
                    if let FnBody::Block(b) = body {
                        self.body(b);
                    }
                }
                self.w(")");
            }
            Expr::Unary(op, arg) => {
                if *op == "-"
                    && !matches!(**arg, Expr::Atom(ref a) if a.starts_with(|c: char| c.is_ascii_digit() || c == '.'))
                {
                    self.w("$bm_neg(");
                    self.expr(arg);
                    self.w(")");
                } else {
                    self.w("(");
                    self.w(op);
                    self.expr(arg);
                    self.w(")");
                }
            }
            Expr::UnaryWord(w, arg) => {
                self.w("(");
                self.w(w);
                self.w(" ");
                self.expr(arg);
                self.w(")");
            }
            Expr::Update { op, prefix, arg } => {
                self.w("(");
                if *prefix {
                    self.w(op);
                    self.expr(arg);
                } else {
                    self.expr(arg);
                    self.w(op);
                }
                self.w(")");
            }
            Expr::Binary(op, a, b) => {
                if let Some(f) = vector_fn(op) {
                    self.w(f);
                    self.w("(");
                    self.expr(a);
                    self.w(",");
                    self.expr(b);
                    self.w(")");
                } else {
                    self.w("(");
                    self.expr(a);
                    self.w(op);
                    self.expr(b);
                    self.w(")");
                }
            }
            Expr::BinaryWord(w, a, b) => {
                self.w("(");
                self.expr(a);
                let _ = write!(self.out, " {w} ");
                self.expr(b);
                self.w(")");
            }
            Expr::Assign(op, target, value) => {
                self.w("(");
                self.expr(target);
                let base = op.strip_suffix('=').filter(|b| !b.is_empty());
                match base.and_then(vector_fn) {
                    Some(f) => {
                        self.w("=");
                        self.w(f);
                        self.w("(");
                        self.expr(target);
                        self.w(",");
                        self.expr(value);
                        self.w(")");
                    }
                    None => {
                        self.w(op);
                        self.expr(value);
                    }
                }
                self.w(")");
            }
            Expr::Cond(t, a, b) => {
                self.w("(");
                self.expr(t);
                self.w("?");
                self.expr(a);
                self.w(":");
                self.expr(b);
                self.w(")");
            }
            Expr::Call {
                callee,
                args,
                optional,
            } => {
                self.callee(callee);
                if *optional {
                    self.w("?.");
                }
                self.w("(");
                self.list(args);
                self.w(")");
            }
            Expr::New(callee, args) => {
                self.w("(new ");
                self.callee(callee);
                if let Some(a) = args {
                    self.w("(");
                    self.list(a);
                    self.w(")");
                }
                self.w(")");
            }
            Expr::Member {
                obj,
                prop,
                optional,
            } => {
                self.callee(obj);
                if matches!(**obj, Expr::Atom(ref a) if a.chars().all(|c| c.is_ascii_digit())) {
                    self.w(" ");
                }
                self.w(if *optional { "?." } else { "." });
                self.w(prop);
            }
            Expr::Index {
                obj,
                index,
                optional,
            } => {
                self.callee(obj);
                if *optional {
                    self.w("?.");
                }
                self.w("[");
                self.expr(index);
                self.w("]");
            }
            Expr::Seq(list) => {
                self.w("(");
                self.list(list);
                self.w(")");
            }
            Expr::Spread(e) => {
                self.w("...");
                self.expr(e);
            }
            Expr::Tagged(tag, t) => {
                self.callee(tag);
                self.w(t);
            }
        }
    }

    /// Objects of member/call chains: atoms and chains print bare, anything else in
    /// parentheses.
    fn callee(&mut self, e: &Expr) {
        match e {
            Expr::Atom(_)
            | Expr::Member { .. }
            | Expr::Index { .. }
            | Expr::Call { .. }
            | Expr::Array(_)
            | Expr::Tagged(..) => self.expr(e),
            _ => {
                self.w("(");
                self.expr(e);
                self.w(")");
            }
        }
    }

    fn key(&mut self, k: &PropKey) {
        match k {
            PropKey::Atom(a) => self.w(a),
            PropKey::Computed(e) => {
                self.w("[");
                self.expr(e);
                self.w("]");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rw(s: &str) -> String {
        rewrite(s).unwrap().body.replace('\n', "")
    }

    #[test]
    fn operators_become_vector_calls() {
        assert_eq!(
            rw("value + [10, 0]"),
            "var __c;__c=($bm_sum(value,[10,0]));return __c;"
        );
        assert_eq!(
            rw("a + b * c - d / 2"),
            "var __c;__c=($bm_sub($bm_sum(a,$bm_mul(b,c)),$bm_div(d,2)));return __c;"
        );
        assert_eq!(rw("-x"), "var __c;__c=($bm_neg(x));return __c;");
        assert_eq!(rw("-5"), "var __c;__c=((-5));return __c;");
        assert_eq!(rw("x += 2"), "var __c;__c=((x=$bm_sum(x,2)));return __c;");
        assert_eq!(rw("2 ** 3 ** 2"), "var __c;__c=((2**(3**2)));return __c;");
    }

    #[test]
    fn completion_value_and_asi() {
        let out = rw("freq = 2;\namp = 30\nwiggle(freq, amp)");
        assert_eq!(
            out,
            "var __c;__c=((freq=2));__c=((amp=30));__c=(wiggle(freq,amp));return __c;"
        );
        // `if` branches carry the completion value; function bodies do not.
        let out = rw("function f(x){ x * 2 } if (time > 1) f(1) else 0");
        assert_eq!(
            out,
            "var __c;function f(x){($bm_mul(x,2));}if((time>1)){__c=(f(1));}else {__c=(0);}return __c;"
        );
    }

    #[test]
    fn statements_and_literals() {
        let src = r#"
            var s = "a/b" + 'c'; // comment
            /* block */ let r = /ab+c/i.test(s);
            const o = {a: 1, "b": [1, 2], c() { return 3 }, ...rest};
            for (var i = 0; i < 3; i++) { s += i }
            for (const k in o) {}
            switch (i) { case 1: s = 1; break; default: s = 2 }
            try { throw new Error("x") } catch (e) { s = e.message } finally {}
            const f = (a, b = 2) => a - b;
            x = cond ? a : b;
            arr[0].y?.z;
        "#;
        let out = rw(src);
        assert!(out.contains("$bm_sum(\"a/b\",'c')"), "{out}");
        assert!(out.contains("/ab+c/i.test(s)"), "{out}");
        assert!(out.contains("(s=$bm_sum(s,i))"), "{out}");
        assert!(out.contains("((a,b=2)=>($bm_sub(a,b)))"), "{out}");
    }

    #[test]
    fn regex_vs_division() {
        assert_eq!(
            rw("a / b / c"),
            "var __c;__c=($bm_div($bm_div(a,b),c));return __c;"
        );
        assert_eq!(rw("(a) / 2"), "var __c;__c=($bm_div(a,2));return __c;");
    }

    #[test]
    fn syntax_errors_have_lines() {
        let e = rewrite("a = 1;\nb = (2").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
