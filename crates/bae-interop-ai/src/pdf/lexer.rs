//! PDF tokens and objects (ISO 32000-1 §7.2, §7.3). Shared by the file
//! structure parser and the content stream interpreter. Never panics on
//! malformed input; nesting depth is bounded.

use std::rc::Rc;

/// A PDF object. Names and strings are raw bytes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Obj {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Array(Vec<Obj>),
    Dict(Dict),
    Stream(Rc<Stream>),
    Ref(u32, u16),
}

/// A dictionary; small, so a vector of pairs.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Dict(pub Vec<(Vec<u8>, Obj)>);

impl Dict {
    pub fn get(&self, key: &[u8]) -> Option<&Obj> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// A stream: its dictionary and its still-encoded data.
#[derive(Debug, PartialEq)]
pub(crate) struct Stream {
    pub dict: Dict,
    pub data: Vec<u8>,
}

impl Obj {
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Obj::Int(i) => Some(i as f64),
            Obj::Real(r) if r.is_finite() => Some(r),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Obj::Int(i) => Some(i),
            Obj::Real(r) if r.is_finite() => Some(r as i64),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&[u8]> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Obj]> {
        match self {
            Obj::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) => Some(d),
            Obj::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }
}

/// A token of a PDF byte stream.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Int(i64),
    Real(f64),
    Name(Vec<u8>),
    Str(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    DictStart,
    DictEnd,
    /// Any other regular-character run: `obj`, `R`, `true`, content operators …
    Keyword(Vec<u8>),
}

pub(crate) fn is_white(b: u8) -> bool {
    matches!(b, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

pub(crate) fn is_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

const MAX_DEPTH: usize = 64;

/// A cursor over PDF bytes.
pub(crate) struct Lexer<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn skip_white(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if is_white(b) {
                self.pos += 1;
            } else if b == b'%' {
                while let Some(&c) = self.data.get(self.pos) {
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// The next token, or `None` at the end of the data.
    pub fn token(&mut self) -> Option<Token> {
        self.skip_white();
        let b = *self.data.get(self.pos)?;
        match b {
            b'/' => {
                self.pos += 1;
                Some(Token::Name(self.name_body()))
            }
            b'(' => {
                self.pos += 1;
                Some(Token::Str(self.literal_string()))
            }
            b'<' => {
                if self.data.get(self.pos + 1) == Some(&b'<') {
                    self.pos += 2;
                    Some(Token::DictStart)
                } else {
                    self.pos += 1;
                    Some(Token::Str(self.hex_string()))
                }
            }
            b'>' => {
                self.pos += if self.data.get(self.pos + 1) == Some(&b'>') {
                    2
                } else {
                    1
                };
                Some(Token::DictEnd)
            }
            b'[' => {
                self.pos += 1;
                Some(Token::ArrayStart)
            }
            b']' => {
                self.pos += 1;
                Some(Token::ArrayEnd)
            }
            b'{' | b'}' | b')' => {
                self.pos += 1;
                Some(Token::Keyword(vec![b]))
            }
            _ => {
                let start = self.pos;
                while let Some(&c) = self.data.get(self.pos) {
                    if is_white(c) || is_delim(c) {
                        break;
                    }
                    self.pos += 1;
                }
                let word = &self.data[start..self.pos];
                Some(number(word).unwrap_or_else(|| Token::Keyword(word.to_vec())))
            }
        }
    }

    fn name_body(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(&c) = self.data.get(self.pos) {
            if is_white(c) || is_delim(c) {
                break;
            }
            self.pos += 1;
            if c == b'#'
                && let (Some(h), Some(l)) = (
                    self.data.get(self.pos).and_then(|&x| hex_val(x)),
                    self.data.get(self.pos + 1).and_then(|&x| hex_val(x)),
                )
            {
                out.push(h << 4 | l);
                self.pos += 2;
                continue;
            }
            out.push(c);
        }
        out
    }

    fn literal_string(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut depth = 1usize;
        while let Some(&c) = self.data.get(self.pos) {
            self.pos += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                b'\\' => {
                    let Some(&e) = self.data.get(self.pos) else {
                        break;
                    };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.data.get(self.pos) {
                                    Some(&d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push((v & 0xff) as u8);
                        }
                        other => out.push(other),
                    }
                }
                _ => out.push(c),
            }
        }
        out
    }

    fn hex_string(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while let Some(&c) = self.data.get(self.pos) {
            self.pos += 1;
            if c == b'>' {
                break;
            }
            if let Some(v) = hex_val(c) {
                match hi.take() {
                    Some(h) => out.push(h << 4 | v),
                    None => hi = Some(v),
                }
            }
        }
        if let Some(h) = hi {
            out.push(h << 4);
        }
        out
    }

    /// Parses one object (references `n g R` included). `None` at the end of
    /// the data or on a stray closing token.
    pub fn object(&mut self) -> Option<Obj> {
        self.object_depth(0)
    }

    fn object_depth(&mut self, depth: usize) -> Option<Obj> {
        if depth > MAX_DEPTH {
            return None;
        }
        let tok = self.token()?;
        self.object_from(tok, depth)
    }

    /// Completes an object whose first token was already read.
    pub fn object_from(&mut self, tok: Token, depth: usize) -> Option<Obj> {
        if depth > MAX_DEPTH {
            return None;
        }
        Some(match tok {
            Token::Int(i) => {
                // `n g R`?
                let save = self.pos;
                if let (Ok(num), Some(Token::Int(g))) = (u32::try_from(i), self.token()) {
                    let save2 = self.pos;
                    if self.token() == Some(Token::Keyword(b"R".to_vec())) {
                        return Some(Obj::Ref(num, u16::try_from(g).unwrap_or(0)));
                    }
                    self.pos = save2;
                }
                self.pos = save;
                Obj::Int(i)
            }
            Token::Real(r) => Obj::Real(r),
            Token::Name(n) => Obj::Name(n),
            Token::Str(s) => Obj::Str(s),
            Token::ArrayStart => {
                let mut items = Vec::new();
                loop {
                    match self.token() {
                        None | Some(Token::ArrayEnd) => break,
                        Some(Token::DictEnd) => continue,
                        Some(t) => match self.object_from(t, depth + 1) {
                            Some(o) => items.push(o),
                            None => break,
                        },
                    }
                }
                Obj::Array(items)
            }
            Token::DictStart => {
                let mut pairs = Vec::new();
                loop {
                    match self.token() {
                        None | Some(Token::DictEnd) => break,
                        Some(Token::Name(key)) => {
                            let Some(t) = self.token() else { break };
                            if t == Token::DictEnd {
                                break;
                            }
                            if let Some(v) = self.object_from(t, depth + 1) {
                                pairs.push((key, v));
                            }
                        }
                        Some(_) => {} // malformed: skip
                    }
                }
                Obj::Dict(Dict(pairs))
            }
            Token::ArrayEnd | Token::DictEnd => return None,
            Token::Keyword(k) => match k.as_slice() {
                b"true" => Obj::Bool(true),
                b"false" => Obj::Bool(false),
                b"null" => Obj::Null,
                _ => Obj::Null,
            },
        })
    }
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// A numeric token (`12`, `-3`, `+.5`, `4.`), or `None`.
fn number(word: &[u8]) -> Option<Token> {
    let (sign, digits) = match word.first()? {
        b'+' => (1.0, &word[1..]),
        b'-' => (-1.0, &word[1..]),
        _ => (1.0, word),
    };
    if digits.is_empty() {
        return None;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for &c in digits {
        match c {
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot => seen_dot = true,
            // Some writers emit `--5` or `1.-2`; treat as not a number.
            _ => return None,
        }
    }
    if !seen_digit {
        return None;
    }
    let text = std::str::from_utf8(digits).ok()?;
    if seen_dot {
        let v: f64 = text
            .parse()
            .ok()
            .or_else(|| format!("{text}0").parse().ok())?;
        Some(Token::Real(sign * v))
    } else {
        match text.parse::<i64>() {
            Ok(v) => Some(Token::Int(if sign < 0.0 { -v } else { v })),
            Err(_) => text.parse::<f64>().ok().map(|v| Token::Real(sign * v)),
        }
    }
}

/// Decodes a PDF text string (UTF-16BE with BOM, UTF-8 with BOM, else
/// PDFDocEncoding, read as Latin-1).
pub(crate) fn text_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|&b| char::from(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Obj {
        Lexer::new(s.as_bytes(), 0).object().unwrap()
    }

    #[test]
    fn objects() {
        assert_eq!(parse("  42 "), Obj::Int(42));
        assert_eq!(parse("-.5"), Obj::Real(-0.5));
        assert_eq!(parse("4."), Obj::Real(4.0));
        assert_eq!(parse("/A#20B"), Obj::Name(b"A B".to_vec()));
        assert_eq!(parse("(a\\(b\\)\\101\\\nc)"), Obj::Str(b"a(b)Ac".to_vec()));
        assert_eq!(parse("<48 65 6C6>"), Obj::Str(b"Hel`".to_vec()));
        assert_eq!(parse("12 0 R"), Obj::Ref(12, 0));
        assert_eq!(
            parse("[1 2 0 R /N]"),
            Obj::Array(vec![Obj::Int(1), Obj::Ref(2, 0), Obj::Name(b"N".to_vec())])
        );
        let d = parse("<</Type/Page/Kids[1 0 R]/V true % comment\n/N null>>");
        let d = d.as_dict().unwrap();
        assert_eq!(d.get(b"Type"), Some(&Obj::Name(b"Page".to_vec())));
        assert_eq!(d.get(b"V"), Some(&Obj::Bool(true)));
        assert_eq!(d.get(b"N"), Some(&Obj::Null));
    }

    #[test]
    fn malformed_input_does_not_panic() {
        for s in [
            "<<", "[", "(", "<", "<</A", "[[[[", ")", ">>", "1 0", "/", "<<//>>",
        ] {
            let mut l = Lexer::new(s.as_bytes(), 0);
            let _ = l.object();
        }
        let deep = "[".repeat(10_000);
        let _ = Lexer::new(deep.as_bytes(), 0).object();
    }

    #[test]
    fn text_strings() {
        assert_eq!(
            text_string(b"\xFE\xFF\x56\xFE\x5C\x42\x00\x20\x00\x31"),
            "图层 1"
        );
        assert_eq!(text_string(b"Layer"), "Layer");
    }
}
