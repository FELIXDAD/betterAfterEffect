//! A small YAML subset parser, enough for OpenColorIO configs (no new
//! dependency, no binary-size cost).
//!
//! Supported: block mappings and sequences (including `- key: value` items
//! and sequences at the same indentation as their key), flow collections
//! (`[a, b]`, `{k: v}`, nested, spanning lines), tags (`!<ColorSpace>`,
//! `!Tag`), plain / single-quoted / double-quoted scalars, literal (`|`) and
//! folded (`>`) block scalars, comments and `---` / `...` markers. Not
//! supported (OCIO configs do not use them): anchors and aliases, complex keys,
//! multiple documents.

use std::fmt;

/// A parsed node: an optional tag and a value.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Tag without the `!<…>` decoration (`ColorSpace`).
    pub tag: Option<String>,
    /// Value.
    pub value: Value,
}

/// A YAML value. Scalars stay strings; typed accessors convert on demand.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Empty / `~` / `null`.
    Null,
    /// Any scalar.
    Scalar(String),
    /// Sequence.
    Seq(Vec<Node>),
    /// Mapping, in document order.
    Map(Vec<(String, Node)>),
}

/// A parse error with a 1-based line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YamlError {
    /// Line of the error.
    pub line: usize,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for YamlError {}

impl Node {
    fn new(value: Value) -> Self {
        Node { tag: None, value }
    }

    /// Mapping lookup.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.value {
            Value::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Scalar text (`None` for collections and null).
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }

    /// Scalar as f64.
    pub fn as_f64(&self) -> Option<f64> {
        let s = self.as_str()?.trim();
        match s {
            ".inf" | "+.inf" | ".Inf" => Some(f64::INFINITY),
            "-.inf" | "-.Inf" => Some(f64::NEG_INFINITY),
            _ => s.parse().ok(),
        }
    }

    /// Scalar as bool (`true/false/yes/no/on/off`).
    pub fn as_bool(&self) -> Option<bool> {
        match self.as_str()?.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Some(true),
            "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    /// Sequence items.
    pub fn as_seq(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Seq(s) => Some(s),
            _ => None,
        }
    }

    /// Mapping entries.
    pub fn as_map(&self) -> Option<&[(String, Node)]> {
        match &self.value {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// A sequence of numbers.
    pub fn as_f64_seq(&self) -> Option<Vec<f64>> {
        self.as_seq()?.iter().map(Node::as_f64).collect()
    }

    /// `true` for null.
    pub fn is_null(&self) -> bool {
        matches!(self.value, Value::Null)
    }
}

struct Line {
    indent: usize,
    text: String,
    number: usize,
}

/// Strips a trailing comment (a `#` at the start or after whitespace, outside quotes).
fn strip_comment(s: &str) -> &str {
    let bytes = s.as_bytes();
    let (mut single, mut double) = (false, false);
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'\\' if double => i += 1,
            b'#' if !single && !double && (i == 0 || bytes[i - 1].is_ascii_whitespace()) => {
                return &s[..i];
            }
            _ => {}
        }
        i += 1;
    }
    s
}

/// Parses a document.
pub fn parse(src: &str) -> Result<Node, YamlError> {
    let mut lines = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let raw = raw.trim_end_matches('\r');
        if raw.contains('\t') && raw.trim_start().starts_with('\t') {
            return Err(YamlError {
                line: n + 1,
                message: "tab indentation".into(),
            });
        }
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        lines.push(Line {
            indent,
            text: raw[indent..].to_string(),
            number: n + 1,
        });
    }
    let mut p = Parser { lines, i: 0 };
    p.skip_blank();
    if p.i >= p.lines.len() {
        return Ok(Node::new(Value::Null));
    }
    let indent = p.lines[p.i].indent;
    let node = p.block(indent)?;
    p.skip_blank();
    if p.i < p.lines.len() {
        return Err(p.err("unexpected content after the document"));
    }
    Ok(node)
}

struct Parser {
    lines: Vec<Line>,
    i: usize,
}

fn is_seq_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

impl Parser {
    fn err(&self, message: impl Into<String>) -> YamlError {
        let line = self
            .lines
            .get(self.i)
            .map_or(self.lines.len(), |l| l.number);
        YamlError {
            line,
            message: message.into(),
        }
    }

    /// The comment-stripped, trimmed text of line `i`.
    fn content(&self, i: usize) -> String {
        strip_comment(&self.lines[i].text).trim_end().to_string()
    }

    fn skip_blank(&mut self) {
        while self.i < self.lines.len() {
            let t = self.content(self.i);
            if t.is_empty() || t == "---" || t == "..." || t.starts_with('%') {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn block(&mut self, indent: usize) -> Result<Node, YamlError> {
        self.skip_blank();
        let text = self.content(self.i);
        if is_seq_item(&text) {
            self.seq(indent)
        } else if split_key(&text).is_some() {
            self.map(indent)
        } else {
            // A lone scalar / flow node.
            self.i += 1;
            self.inline_value(&text, indent)
        }
    }

    fn map(&mut self, indent: usize) -> Result<Node, YamlError> {
        let mut entries: Vec<(String, Node)> = Vec::new();
        loop {
            self.skip_blank();
            if self.i >= self.lines.len() || self.lines[self.i].indent != indent {
                break;
            }
            let text = self.content(self.i);
            if is_seq_item(&text) {
                break;
            }
            let Some((key, rest)) = split_key(&text) else {
                return Err(self.err(format!("expected `key: value`, found `{text}`")));
            };
            self.i += 1;
            let value = self.value_after(rest.trim(), indent)?;
            entries.push((key, value));
        }
        Ok(Node::new(Value::Map(entries)))
    }

    fn seq(&mut self, indent: usize) -> Result<Node, YamlError> {
        let mut items = Vec::new();
        loop {
            self.skip_blank();
            if self.i >= self.lines.len() || self.lines[self.i].indent != indent {
                break;
            }
            let text = self.content(self.i);
            if !is_seq_item(&text) {
                break;
            }
            let rest = text[1..].trim_start().to_string();
            let (tag, after_tag) = take_tag(&rest);
            let item = if after_tag.trim().is_empty() {
                self.i += 1;
                let mut n = self.nested_or_null(indent)?;
                n.tag = tag.or(n.tag);
                n
            } else if !after_tag.trim_start().starts_with(['[', '{', '"', '\''])
                && split_key(after_tag.trim_start()).is_some()
            {
                // `- key: value` starts a mapping whose keys align after the dash.
                let inner = after_tag.trim_start();
                let line = &mut self.lines[self.i];
                line.indent = indent + (text.len() - inner.len());
                line.text = inner.to_string();
                let new_indent = line.indent;
                let mut n = self.map(new_indent)?;
                n.tag = tag;
                n
            } else {
                self.i += 1;
                let mut n = self.inline_value(after_tag.trim(), indent)?;
                n.tag = tag.or(n.tag);
                n
            };
            items.push(item);
        }
        Ok(Node::new(Value::Seq(items)))
    }

    /// After `key:` with nothing (but maybe a tag) on the line.
    fn nested_or_null(&mut self, indent: usize) -> Result<Node, YamlError> {
        self.skip_blank();
        if self.i < self.lines.len() {
            let next = &self.lines[self.i];
            let next_text = self.content(self.i);
            if next.indent > indent || (next.indent == indent && is_seq_item(&next_text)) {
                let ni = next.indent;
                return self.block(ni);
            }
        }
        Ok(Node::new(Value::Null))
    }

    fn value_after(&mut self, rest: &str, indent: usize) -> Result<Node, YamlError> {
        let (tag, rest) = take_tag(rest);
        let rest = rest.trim();
        let mut node = if rest.is_empty() {
            self.nested_or_null(indent)?
        } else if rest.starts_with('|') || rest.starts_with('>') {
            self.block_scalar(rest, indent)
        } else {
            self.inline_value(rest, indent)?
        };
        if tag.is_some() {
            node.tag = tag;
        }
        Ok(node)
    }

    /// Literal / folded block scalar following a `key: |` line.
    fn block_scalar(&mut self, header: &str, indent: usize) -> Node {
        let folded = header.starts_with('>');
        let keep = header.contains('+');
        let strip = header.contains('-');
        let mut raw: Vec<(usize, String)> = Vec::new();
        while self.i < self.lines.len() {
            let l = &self.lines[self.i];
            if l.text.trim().is_empty() {
                raw.push((usize::MAX, String::new()));
                self.i += 1;
                continue;
            }
            if l.indent <= indent {
                break;
            }
            raw.push((l.indent, l.text.clone()));
            self.i += 1;
        }
        // Trailing blank lines belong to the chomping, not the content.
        let mut trailing = 0;
        while raw.last().is_some_and(|(i, _)| *i == usize::MAX) {
            raw.pop();
            trailing += 1;
        }
        let base = raw
            .iter()
            .filter(|(i, _)| *i != usize::MAX)
            .map(|(i, _)| *i)
            .min()
            .unwrap_or(0);
        let lines: Vec<String> = raw
            .iter()
            .map(|(i, t)| {
                if *i == usize::MAX {
                    String::new()
                } else {
                    format!("{}{}", " ".repeat(i - base), t)
                }
            })
            .collect();
        let mut s = if folded {
            let mut out = String::new();
            for (k, l) in lines.iter().enumerate() {
                if k > 0 {
                    out.push(if l.is_empty() || lines[k - 1].is_empty() {
                        '\n'
                    } else {
                        ' '
                    });
                }
                out.push_str(l);
            }
            out
        } else {
            lines.join("\n")
        };
        if !strip {
            s.push('\n');
        }
        if keep {
            s.push_str(&"\n".repeat(trailing));
        }
        Node::new(Value::Scalar(s))
    }

    /// A scalar or flow collection starting on the current (already consumed)
    /// line; flow collections and quoted scalars may continue on later lines.
    fn inline_value(&mut self, text: &str, _indent: usize) -> Result<Node, YamlError> {
        let mut buf = text.to_string();
        let line_no = self
            .lines
            .get(self.i.saturating_sub(1))
            .map_or(0, |l| l.number);
        if buf.starts_with(['[', '{']) || buf.starts_with('"') || buf.starts_with('\'') {
            while !balanced(&buf) {
                if self.i >= self.lines.len() {
                    return Err(YamlError {
                        line: line_no,
                        message: "unterminated flow collection or quoted scalar".into(),
                    });
                }
                let next = self.content(self.i);
                buf.push(' ');
                buf.push_str(next.trim());
                self.i += 1;
            }
            let mut f = Flow {
                s: buf.as_bytes(),
                pos: 0,
                line: line_no,
            };
            let node = f.node(false)?;
            f.ws();
            if f.pos != f.s.len() {
                return Err(YamlError {
                    line: line_no,
                    message: format!("trailing characters after value: `{}`", &buf[f.pos..]),
                });
            }
            return Ok(node);
        }
        // Plain scalar, possibly continued on more-indented lines.
        while self.i < self.lines.len() {
            let next = self.content(self.i);
            if next.is_empty()
                || self.lines[self.i].indent <= _indent
                || split_key(&next).is_some()
                || is_seq_item(&next)
            {
                break;
            }
            buf.push(' ');
            buf.push_str(next.trim());
            self.i += 1;
        }
        Ok(plain(&buf))
    }
}

fn plain(s: &str) -> Node {
    let s = s.trim();
    if s.is_empty() || s == "~" || s == "null" || s == "Null" || s == "NULL" {
        Node::new(Value::Null)
    } else {
        Node::new(Value::Scalar(s.to_string()))
    }
}

/// `!<Tag> rest` / `!Tag rest` → (tag, rest).
fn take_tag(s: &str) -> (Option<String>, &str) {
    let t = s.trim_start();
    if let Some(r) = t.strip_prefix("!<") {
        if let Some(end) = r.find('>') {
            return (Some(r[..end].to_string()), &r[end + 1..]);
        }
    } else if let Some(r) = t.strip_prefix('!') {
        let end = r
            .find(|c: char| c.is_whitespace() || c == '{' || c == '[')
            .unwrap_or(r.len());
        return (Some(r[..end].to_string()), &r[end..]);
    }
    (None, s)
}

/// Splits `key: rest` (key may be quoted). `None` if the line is not a mapping entry.
fn split_key(text: &str) -> Option<(String, &str)> {
    let t = text;
    if let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let mut f = Flow {
            s: t.as_bytes(),
            pos: 0,
            line: 0,
        };
        let key = f.quoted(q as u8).ok()?;
        let rest = &t[f.pos..];
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':')?;
        if !(rest.is_empty() || rest.starts_with(' ')) {
            return None;
        }
        return Some((key, rest));
    }
    if t.starts_with(['[', '{', '-', '!', '|', '>']) && !t.starts_with("-:") {
        if t.starts_with('-') && !is_seq_item(t) {
            // A plain scalar starting with '-' (e.g. a negative number key).
        } else {
            return None;
        }
    }
    let bytes = t.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
            let key = t[..i].trim();
            if key.is_empty() {
                return None;
            }
            return Some((key.to_string(), &t[i + 1..]));
        }
    }
    None
}

/// `true` when brackets and quotes are balanced (outside quotes).
fn balanced(s: &str) -> bool {
    let b = s.as_bytes();
    let mut depth: i32 = 0;
    let (mut single, mut double) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if double {
            match c {
                b'\\' => i += 1,
                b'"' => double = false,
                _ => {}
            }
        } else if single {
            if c == b'\'' {
                if b.get(i + 1) == Some(&b'\'') {
                    i += 1;
                } else {
                    single = false;
                }
            }
        } else {
            match c {
                b'"' => double = true,
                b'\'' => single = true,
                b'[' | b'{' => depth += 1,
                b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    depth <= 0 && !single && !double
}

struct Flow<'a> {
    s: &'a [u8],
    pos: usize,
    line: usize,
}

impl Flow<'_> {
    fn err(&self, m: impl Into<String>) -> YamlError {
        YamlError {
            line: self.line,
            message: m.into(),
        }
    }

    fn ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn node(&mut self, is_key: bool) -> Result<Node, YamlError> {
        self.ws();
        let mut tag = None;
        if self.peek() == Some(b'!') {
            let rest = std::str::from_utf8(&self.s[self.pos..]).unwrap_or("");
            let (t, after) = take_tag(rest);
            tag = t;
            self.pos = self.s.len() - after.len();
            self.ws();
        }
        let mut node = match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    match self.peek() {
                        Some(b']') => {
                            self.pos += 1;
                            break;
                        }
                        Some(b',') => {
                            self.pos += 1;
                        }
                        None => return Err(self.err("unterminated `[`")),
                        _ => items.push(self.node(false)?),
                    }
                }
                Node::new(Value::Seq(items))
            }
            Some(b'{') => {
                self.pos += 1;
                let mut entries = Vec::new();
                loop {
                    self.ws();
                    match self.peek() {
                        Some(b'}') => {
                            self.pos += 1;
                            break;
                        }
                        Some(b',') => {
                            self.pos += 1;
                        }
                        None => return Err(self.err("unterminated `{`")),
                        _ => {
                            let key = self.node(true)?;
                            self.ws();
                            let value = if self.peek() == Some(b':') {
                                self.pos += 1;
                                self.ws();
                                if matches!(self.peek(), Some(b',') | Some(b'}')) {
                                    Node::new(Value::Null)
                                } else {
                                    self.node(false)?
                                }
                            } else {
                                Node::new(Value::Null)
                            };
                            let k = key.as_str().unwrap_or("").to_string();
                            entries.push((k, value));
                        }
                    }
                }
                Node::new(Value::Map(entries))
            }
            Some(q @ (b'"' | b'\'')) => Node::new(Value::Scalar(self.quoted(q)?)),
            _ => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c == b',' || c == b']' || c == b'}' {
                        break;
                    }
                    if c == b':'
                        && (is_key || matches!(self.s.get(self.pos + 1), Some(b' ') | None))
                    {
                        if is_key {
                            break;
                        }
                        break;
                    }
                    self.pos += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.pos]).unwrap_or("");
                plain(text)
            }
        };
        if tag.is_some() {
            node.tag = tag;
        }
        Ok(node)
    }

    fn quoted(&mut self, q: u8) -> Result<String, YamlError> {
        self.pos += 1;
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            if q == b'\'' {
                if c == b'\'' {
                    if self.peek() == Some(b'\'') {
                        self.pos += 1;
                        out.push(b'\'');
                        continue;
                    }
                    return Ok(String::from_utf8_lossy(&out).into_owned());
                }
                out.push(c);
            } else {
                match c {
                    b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                    b'\\' => {
                        let e = self.peek().ok_or_else(|| self.err("bad escape"))?;
                        self.pos += 1;
                        match e {
                            b'n' => out.push(b'\n'),
                            b't' => out.push(b'\t'),
                            b'r' => out.push(b'\r'),
                            b'0' => out.push(0),
                            b' ' => out.push(b' '),
                            other => out.push(other),
                        }
                    }
                    _ => out.push(c),
                }
            }
        }
        Err(self.err("unterminated quoted scalar"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
ocio_profile_version: 2.1   # comment
name: "test config"
search_path:
  - luts
  - 'more luts'
roles:
  scene_linear: ACEScg
  data: Raw
description: |
  First line
    indented line

  after blank
displays:
  sRGB - Display:
    - !<View> {name: Raw, colorspace: Raw}
    - !<View> {name: "ACES 1.0 - SDR Video", view_transform: ACES 1.0 - SDR Video, display_colorspace: <USE_DISPLAY_NAME>}
colorspaces:
  - !<ColorSpace>
    name: ACEScg
    aliases: [lin_ap1, ACES - ACEScg]
    isdata: false
    to_scene_reference: !<BuiltinTransform> {style: ACEScg_to_ACES2065-1}

  - !<ColorSpace>
    name: Gamma
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [1, 0, 0, 0,
                                       0, 1, 0, 0,
                                       0, 0, 1, 0, 0, 0, 0, 1]}
        - !<ExponentTransform> {value: 2.2, style: pass_thru, direction: inverse}
  - name: plain item
    isdata: true
"#;

    #[test]
    fn parses_ocio_shapes() {
        let doc = parse(SAMPLE).unwrap();
        assert_eq!(doc.get("ocio_profile_version").unwrap().as_f64(), Some(2.1));
        assert_eq!(doc.get("name").unwrap().as_str(), Some("test config"));
        let sp: Vec<_> = doc
            .get("search_path")
            .unwrap()
            .as_seq()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert_eq!(sp, ["luts", "more luts"]);
        assert_eq!(
            doc.get("roles").unwrap().get("data").unwrap().as_str(),
            Some("Raw")
        );
        assert_eq!(
            doc.get("description").unwrap().as_str(),
            Some("First line\n  indented line\n\nafter blank\n")
        );
        let views = doc
            .get("displays")
            .unwrap()
            .get("sRGB - Display")
            .unwrap()
            .as_seq()
            .unwrap();
        assert_eq!(views.len(), 2);
        assert_eq!(views[1].tag.as_deref(), Some("View"));
        assert_eq!(
            views[1].get("name").unwrap().as_str(),
            Some("ACES 1.0 - SDR Video")
        );
        assert_eq!(
            views[1].get("display_colorspace").unwrap().as_str(),
            Some("<USE_DISPLAY_NAME>")
        );
        let cs = doc.get("colorspaces").unwrap().as_seq().unwrap();
        assert_eq!(cs.len(), 3);
        assert_eq!(cs[0].tag.as_deref(), Some("ColorSpace"));
        assert_eq!(
            cs[0].get("aliases").unwrap().as_seq().unwrap()[1].as_str(),
            Some("ACES - ACEScg")
        );
        assert_eq!(cs[0].get("isdata").unwrap().as_bool(), Some(false));
        let b = cs[0].get("to_scene_reference").unwrap();
        assert_eq!(b.tag.as_deref(), Some("BuiltinTransform"));
        assert_eq!(
            b.get("style").unwrap().as_str(),
            Some("ACEScg_to_ACES2065-1")
        );
        let g = cs[1].get("from_scene_reference").unwrap();
        assert_eq!(g.tag.as_deref(), Some("GroupTransform"));
        let ch = g.get("children").unwrap().as_seq().unwrap();
        assert_eq!(ch[0].get("matrix").unwrap().as_f64_seq().unwrap().len(), 16);
        assert_eq!(ch[1].get("value").unwrap().as_f64(), Some(2.2));
        assert_eq!(cs[2].get("name").unwrap().as_str(), Some("plain item"));
        assert_eq!(cs[2].tag, None);
    }

    #[test]
    fn quoting_and_errors() {
        let d = parse("a: 'it''s'\nb: \"x\\ty\"\nc: ~\nd: [ ]\ne: {}\n").unwrap();
        assert_eq!(d.get("a").unwrap().as_str(), Some("it's"));
        assert_eq!(d.get("b").unwrap().as_str(), Some("x\ty"));
        assert!(d.get("c").unwrap().is_null());
        assert_eq!(d.get("d").unwrap().as_seq().unwrap().len(), 0);
        assert_eq!(d.get("e").unwrap().as_map().unwrap().len(), 0);
        assert!(parse("a: [1, 2\n").is_err());
        let e = parse("a: 1\n  - x\n").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
