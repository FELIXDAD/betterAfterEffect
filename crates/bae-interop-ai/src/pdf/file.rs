//! PDF file structure (ISO 32000-1 §7.5): cross-reference tables and streams,
//! incremental updates, object streams, stream filters. A damaged or missing
//! cross-reference falls back to scanning the file for `n g obj`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;

use super::lexer::{Dict, Lexer, Obj, Stream, Token, is_white};
use crate::error::Error;

#[derive(Clone, Copy, Debug)]
enum Entry {
    Offset(usize),
    Compressed { stream: u32, index: u32 },
}

/// An opened PDF.
pub(crate) struct PdfFile<'a> {
    data: &'a [u8],
    xref: HashMap<u32, Entry>,
    pub trailer: Dict,
    cache: RefCell<HashMap<u32, Obj>>,
    /// Decoded object streams: (object number, offset in the decoded data).
    obj_streams: RefCell<HashMap<u32, Rc<ObjStream>>>,
}

const MAX_RESOLVE: usize = 32;

/// A decoded object stream: its data and (object number, offset) pairs.
type ObjStream = (Vec<u8>, Vec<(u32, usize)>);

impl<'a> PdfFile<'a> {
    pub fn open(data: &'a [u8]) -> Result<Self, Error> {
        let mut file = PdfFile {
            data,
            xref: HashMap::new(),
            trailer: Dict::default(),
            cache: RefCell::new(HashMap::new()),
            obj_streams: RefCell::new(HashMap::new()),
        };
        let parsed = file.read_xref_chain();
        if !parsed || file.root().is_none() {
            file.reconstruct();
        }
        if file.trailer.get(b"Encrypt").is_some() {
            return Err(Error::Unsupported("encrypted PDF files".into()));
        }
        if file.root().is_none() {
            return Err(Error::Malformed("no document catalog".into()));
        }
        Ok(file)
    }

    /// The document catalog.
    pub fn root(&self) -> Option<Dict> {
        self.resolve(self.trailer.get(b"Root")?).as_dict().cloned()
    }

    fn read_xref_chain(&mut self) -> bool {
        let Some(mut offset) = find_startxref(self.data) else {
            return false;
        };
        let mut seen = Vec::new();
        let mut first = true;
        while !seen.contains(&offset) && seen.len() < 64 {
            seen.push(offset);
            let Some(trailer) = self.read_xref_section(offset) else {
                return !first;
            };
            if first {
                self.trailer = trailer.clone();
                first = false;
            }
            // Hybrid files: a cross-reference stream next to the table.
            if let Some(stm) = trailer.get(b"XRefStm").and_then(Obj::as_i64) {
                let _ = self.read_xref_section(stm.max(0) as usize);
            }
            match trailer.get(b"Prev").and_then(Obj::as_i64) {
                Some(prev) if prev >= 0 => offset = prev as usize,
                _ => break,
            }
        }
        true
    }

    /// Reads one cross-reference section (table or stream); entries already
    /// known (from newer sections) win. Returns its trailer dictionary.
    fn read_xref_section(&mut self, offset: usize) -> Option<Dict> {
        let mut lx = Lexer::new(self.data, offset);
        match lx.token()? {
            Token::Keyword(k) if k == b"xref" => {
                loop {
                    match lx.token()? {
                        Token::Int(start) => {
                            let Some(Token::Int(count)) = lx.token() else {
                                return None;
                            };
                            lx.skip_white();
                            for i in 0..count.max(0) {
                                // Entries are 20 bytes: oooooooooo ggggg n\r\n
                                let line = self.data.get(lx.pos..lx.pos + 20)?;
                                let off = parse_uint(&line[0..10]);
                                let kind = line[17];
                                if kind == b'n'
                                    && let Some(num) = u32::try_from(start + i).ok()
                                    && off > 0
                                {
                                    self.xref.entry(num).or_insert(Entry::Offset(off));
                                }
                                lx.pos += 20;
                                // Tolerate 19-byte entries (single EOL char).
                                if !self
                                    .data
                                    .get(lx.pos.wrapping_sub(1))
                                    .is_some_and(|&b| is_white(b))
                                {
                                    lx.pos -= 1;
                                }
                                lx.skip_white();
                            }
                        }
                        Token::Keyword(k) if k == b"trailer" => {
                            return lx.object()?.as_dict().cloned();
                        }
                        _ => return None,
                    }
                }
            }
            Token::Int(num) => {
                let (dict, data) = self.read_indirect_stream_at(offset)?;
                if dict.get(b"Type").and_then(Obj::as_name) != Some(b"XRef") {
                    return None;
                }
                let _ = num;
                self.read_xref_stream(&dict, &data);
                Some(dict)
            }
            _ => None,
        }
    }

    fn read_indirect_stream_at(&self, offset: usize) -> Option<(Dict, Vec<u8>)> {
        let obj = self.parse_indirect_at(offset)?;
        match obj {
            Obj::Stream(s) => {
                let data = decode_stream(&s).ok()?;
                Some((s.dict.clone(), data))
            }
            _ => None,
        }
    }

    fn read_xref_stream(&mut self, dict: &Dict, data: &[u8]) {
        let w: Vec<usize> = dict
            .get(b"W")
            .and_then(Obj::as_array)
            .map(|a| {
                a.iter()
                    .map(|o| o.as_i64().unwrap_or(0).clamp(0, 8) as usize)
                    .collect()
            })
            .unwrap_or_default();
        if w.len() < 3 {
            return;
        }
        let size = dict.get(b"Size").and_then(Obj::as_i64).unwrap_or(0).max(0);
        let index: Vec<i64> = dict
            .get(b"Index")
            .and_then(Obj::as_array)
            .map(|a| a.iter().filter_map(Obj::as_i64).collect())
            .unwrap_or_else(|| vec![0, size]);
        let row = w[0] + w[1] + w[2];
        if row == 0 {
            return;
        }
        let mut rows = data.chunks_exact(row);
        for pair in index.as_chunks::<2>().0 {
            for i in 0..pair[1].max(0) {
                let Some(r) = rows.next() else { return };
                let field = |k: usize, default: u64| {
                    let start: usize = w[..k].iter().sum();
                    if w[k] == 0 {
                        default
                    } else {
                        r[start..start + w[k]]
                            .iter()
                            .fold(0u64, |acc, &b| acc << 8 | u64::from(b))
                    }
                };
                let Ok(num) = u32::try_from(pair[0] + i) else {
                    continue;
                };
                let entry = match field(0, 1) {
                    1 => Entry::Offset(field(1, 0) as usize),
                    2 => Entry::Compressed {
                        stream: field(1, 0) as u32,
                        index: field(2, 0) as u32,
                    },
                    _ => continue,
                };
                self.xref.entry(num).or_insert(entry);
            }
        }
    }

    /// Rebuilds the cross-reference by scanning for `n g obj` (last wins) and
    /// finds a trailer.
    fn reconstruct(&mut self) {
        self.xref.clear();
        self.cache.borrow_mut().clear();
        let d = self.data;
        let mut i = 0;
        while let Some(rel) = find(&d[i..], b"obj") {
            let at = i + rel;
            i = at + 3;
            // Walk back over "<num> <gen> ".
            let mut j = at;
            while j > 0 && is_white(d[j - 1]) {
                j -= 1;
            }
            let gen_end = j;
            while j > 0 && d[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j == gen_end {
                continue;
            }
            let num_end = {
                let mut k = j;
                while k > 0 && is_white(d[k - 1]) {
                    k -= 1;
                }
                k
            };
            let mut k = num_end;
            while k > 0 && d[k - 1].is_ascii_digit() {
                k -= 1;
            }
            if k == num_end
                || (k > 0 && !is_white(d[k - 1]) && !matches!(d[k - 1], b'>' | b')' | b']'))
            {
                continue;
            }
            if let Ok(num) = u32::try_from(parse_uint(&d[k..num_end])) {
                self.xref.insert(num, Entry::Offset(k));
            }
        }
        // Trailer: the last `trailer` dictionary with /Root, or an XRef stream.
        let mut trailer = None;
        let mut pos = 0;
        while let Some(rel) = find(&d[pos..], b"trailer") {
            let at = pos + rel + 7;
            if let Some(Obj::Dict(t)) = Lexer::new(d, at).object()
                && t.get(b"Root").is_some()
            {
                trailer = Some(t);
            }
            pos = at;
        }
        if trailer.is_none() {
            let mut nums: Vec<u32> = self.xref.keys().copied().collect();
            nums.sort_unstable();
            for num in nums {
                if let Some(dict) = self.get(num).as_dict()
                    && dict.get(b"Type").and_then(Obj::as_name) == Some(b"Catalog")
                {
                    trailer = Some(Dict(vec![(b"Root".to_vec(), Obj::Ref(num, 0))]));
                }
            }
        }
        if let Some(t) = trailer {
            self.trailer = t;
        }
        self.obj_streams.borrow_mut().clear();
    }

    /// Parses `n g obj <object> [stream … endstream]` at `offset`.
    fn parse_indirect_at(&self, offset: usize) -> Option<Obj> {
        let mut lx = Lexer::new(self.data, offset);
        let (Some(Token::Int(_)), Some(Token::Int(_))) = (lx.token(), lx.token()) else {
            return None;
        };
        if lx.token() != Some(Token::Keyword(b"obj".to_vec())) {
            return None;
        }
        let obj = lx.object()?;
        if lx.token() == Some(Token::Keyword(b"stream".to_vec())) {
            let Obj::Dict(dict) = obj else { return None };
            // Data starts after the EOL following `stream`.
            let mut start = lx.pos;
            if self.data.get(start) == Some(&b'\r') {
                start += 1;
            }
            if self.data.get(start) == Some(&b'\n') {
                start += 1;
            }
            let declared = dict
                .get(b"Length")
                .map(|l| self.resolve(l))
                .and_then(|l| l.as_i64())
                .filter(|&l| l >= 0)
                .map(|l| l as usize);
            let end = match declared {
                Some(len)
                    if start + len <= self.data.len()
                        && find(
                            &self.data[start + len..(start + len + 32).min(self.data.len())],
                            b"endstream",
                        )
                        .is_some() =>
                {
                    start + len
                }
                _ => {
                    // Wrong /Length: search for endstream.
                    let rel = find(&self.data[start..], b"endstream")?;
                    let mut e = start + rel;
                    while e > start && matches!(self.data[e - 1], b'\r' | b'\n') {
                        e -= 1;
                    }
                    e
                }
            };
            return Some(Obj::Stream(Rc::new(Stream {
                dict,
                data: self.data[start..end].to_vec(),
            })));
        }
        Some(obj)
    }

    /// Object `num` (null when missing).
    pub fn get(&self, num: u32) -> Obj {
        if let Some(o) = self.cache.borrow().get(&num) {
            return o.clone();
        }
        let obj = match self.xref.get(&num).copied() {
            Some(Entry::Offset(off)) => self.parse_indirect_at(off).unwrap_or(Obj::Null),
            Some(Entry::Compressed { stream, index }) => self
                .object_in_stream(stream, num, index)
                .unwrap_or(Obj::Null),
            None => Obj::Null,
        };
        self.cache.borrow_mut().insert(num, obj.clone());
        obj
    }

    fn object_in_stream(&self, stream: u32, num: u32, index: u32) -> Option<Obj> {
        let decoded = {
            let cached = self.obj_streams.borrow().get(&stream).cloned();
            match cached {
                Some(d) => d,
                None => {
                    let Obj::Stream(s) = self.get(stream) else {
                        return None;
                    };
                    let data = decode_stream(&s).ok()?;
                    let n = s.dict.get(b"N").and_then(Obj::as_i64).unwrap_or(0).max(0) as usize;
                    let first = s
                        .dict
                        .get(b"First")
                        .and_then(Obj::as_i64)
                        .unwrap_or(0)
                        .max(0) as usize;
                    let mut lx = Lexer::new(&data, 0);
                    let mut table = Vec::with_capacity(n.min(100_000));
                    for _ in 0..n {
                        match (lx.token(), lx.token()) {
                            (Some(Token::Int(o)), Some(Token::Int(off))) => {
                                table.push((o as u32, first + off.max(0) as usize));
                            }
                            _ => break,
                        }
                    }
                    let d = Rc::new((data, table));
                    self.obj_streams.borrow_mut().insert(stream, d.clone());
                    d
                }
            }
        };
        let (data, table) = &*decoded;
        let off = table
            .get(index as usize)
            .filter(|(o, _)| *o == num)
            .or_else(|| table.iter().find(|(o, _)| *o == num))
            .map(|(_, off)| *off)?;
        Lexer::new(data, off).object()
    }

    /// Follows references (bounded).
    pub fn resolve(&self, obj: &Obj) -> Obj {
        let mut cur = obj.clone();
        for _ in 0..MAX_RESOLVE {
            match cur {
                Obj::Ref(n, _) => cur = self.get(n),
                other => return other,
            }
        }
        Obj::Null
    }

    /// A dictionary entry, resolved.
    pub fn dict_get(&self, dict: &Dict, key: &[u8]) -> Obj {
        dict.get(key).map_or(Obj::Null, |o| self.resolve(o))
    }

    /// Decoded data of a stream object (references resolved).
    pub fn stream_data(&self, obj: &Obj) -> Result<Vec<u8>, Error> {
        match self.resolve(obj) {
            Obj::Stream(s) => {
                // Resolve indirect /Filter and /DecodeParms.
                let mut s2 = Stream {
                    dict: s.dict.clone(),
                    data: Vec::new(),
                };
                for key in [b"Filter".as_slice(), b"DecodeParms"] {
                    if let Some(v) = s.dict.get(key) {
                        let r = self.resolve(v);
                        s2.dict.0.retain(|(k, _)| k != key);
                        s2.dict.0.push((key.to_vec(), r));
                    }
                }
                s2.data = s.data.clone();
                decode_stream(&s2)
            }
            _ => Err(Error::Malformed("expected a stream".into())),
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn parse_uint(s: &[u8]) -> usize {
    s.iter()
        .filter(|b| b.is_ascii_digit())
        .fold(0usize, |acc, &b| {
            acc.saturating_mul(10).saturating_add(usize::from(b - b'0'))
        })
}

fn find_startxref(data: &[u8]) -> Option<usize> {
    let tail_start = data.len().saturating_sub(2048);
    let tail = &data[tail_start..];
    let rel = tail.windows(9).rposition(|w| w == b"startxref")?;
    let mut lx = Lexer::new(data, tail_start + rel + 9);
    match lx.token()? {
        Token::Int(off) if off >= 0 && (off as usize) < data.len() => Some(off as usize),
        _ => None,
    }
}

/// Applies a stream's filters.
pub(crate) fn decode_stream(s: &Stream) -> Result<Vec<u8>, Error> {
    let filters: Vec<Vec<u8>> = match s.dict.get(b"Filter") {
        Some(Obj::Name(n)) => vec![n.clone()],
        Some(Obj::Array(a)) => a
            .iter()
            .filter_map(|o| o.as_name().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    };
    let parms: Vec<Option<Dict>> = match s.dict.get(b"DecodeParms") {
        Some(Obj::Dict(d)) => vec![Some(d.clone())],
        Some(Obj::Array(a)) => a.iter().map(|o| o.as_dict().cloned()).collect(),
        _ => Vec::new(),
    };
    let mut data = s.data.clone();
    for (i, f) in filters.iter().enumerate() {
        let parm = parms.get(i).cloned().flatten();
        data = match f.as_slice() {
            b"FlateDecode" | b"Fl" => predict(inflate(&data), parm.as_ref())?,
            b"ASCII85Decode" | b"A85" => ascii85(&data)?,
            b"ASCIIHexDecode" | b"AHx" => ascii_hex(&data),
            b"RunLengthDecode" | b"RL" => run_length(&data),
            b"LZWDecode" | b"LZW" => predict(lzw(&data, parm.as_ref()), parm.as_ref())?,
            other => {
                return Err(Error::Unsupported(format!(
                    "PDF stream filter {}",
                    String::from_utf8_lossy(other)
                )));
            }
        };
    }
    Ok(data)
}

/// zlib inflate, keeping whatever decodes before a corruption.
fn inflate(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dec = flate2::read::ZlibDecoder::new(data);
    let mut buf = [0u8; 16 * 1024];
    loop {
        match dec.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
        }
    }
    if out.is_empty() && !data.is_empty() {
        // Raw deflate without a zlib header.
        let mut dec = flate2::read::DeflateDecoder::new(data);
        loop {
            match dec.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
            }
        }
    }
    out
}

/// PNG / TIFF predictors (§7.4.4.4).
fn predict(data: Vec<u8>, parm: Option<&Dict>) -> Result<Vec<u8>, Error> {
    let Some(p) = parm else { return Ok(data) };
    let int = |k: &[u8], d: i64| p.get(k).and_then(Obj::as_i64).unwrap_or(d);
    let predictor = int(b"Predictor", 1);
    if predictor < 2 {
        return Ok(data);
    }
    let colors = int(b"Colors", 1).clamp(1, 32) as usize;
    let bpc = int(b"BitsPerComponent", 8).clamp(1, 16) as usize;
    let columns = int(b"Columns", 1).clamp(1, 1 << 20) as usize;
    let bpp = (colors * bpc).div_ceil(8).max(1);
    let row_len = (colors * bpc * columns).div_ceil(8);
    if predictor == 2 {
        if bpc != 8 {
            return Err(Error::Unsupported(
                "TIFF predictor with non-8-bit samples".into(),
            ));
        }
        let mut out = data;
        for row in out.chunks_mut(row_len) {
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(row[i - bpp]);
            }
        }
        return Ok(out);
    }
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row_len];
    for chunk in data.chunks(row_len + 1) {
        let Some((&kind, raw)) = chunk.split_first() else {
            break;
        };
        let mut row = raw.to_vec();
        row.resize(row_len, 0);
        for i in 0..row_len {
            let a = if i >= bpp { row[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            row[i] = match kind {
                1 => row[i].wrapping_add(a),
                2 => row[i].wrapping_add(b),
                3 => row[i].wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => {
                    let p = i16::from(a) + i16::from(b) - i16::from(c);
                    let (pa, pb, pc) = (
                        (p - i16::from(a)).abs(),
                        (p - i16::from(b)).abs(),
                        (p - i16::from(c)).abs(),
                    );
                    let pred = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    row[i].wrapping_add(pred)
                }
                _ => row[i],
            };
        }
        out.extend_from_slice(&row);
        prev = row;
    }
    Ok(out)
}

fn ascii85(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(data.len() * 4 / 5);
    let mut group = [0u32; 5];
    let mut n = 0;
    let body = data.strip_prefix(b"<~").unwrap_or(data);
    for &c in body {
        match c {
            b'~' => break,
            b'z' if n == 0 => out.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                group[n] = u32::from(c - b'!');
                n += 1;
                if n == 5 {
                    let v = group.iter().fold(0u64, |acc, &d| acc * 85 + u64::from(d));
                    out.extend_from_slice(&(v as u32).to_be_bytes());
                    n = 0;
                }
            }
            c if is_white(c) => {}
            _ => return Err(Error::Malformed("bad ASCII85 data".into())),
        }
    }
    if n > 1 {
        for g in group.iter_mut().skip(n) {
            *g = 84;
        }
        let v = group.iter().fold(0u64, |acc, &d| acc * 85 + u64::from(d));
        out.extend_from_slice(&(v as u32).to_be_bytes()[..n - 1]);
    }
    Ok(out)
}

fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut hi: Option<u8> = None;
    for &c in data {
        if c == b'>' {
            break;
        }
        let v = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => continue,
        };
        match hi.take() {
            Some(h) => out.push(h << 4 | v),
            None => hi = Some(v),
        }
    }
    if let Some(h) = hi {
        out.push(h << 4);
    }
    out
}

fn run_length(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&len) = data.get(i) {
        i += 1;
        match len {
            128 => break,
            0..=127 => {
                let n = usize::from(len) + 1;
                let end = (i + n).min(data.len());
                out.extend_from_slice(&data[i..end]);
                i = end;
            }
            _ => {
                if let Some(&b) = data.get(i) {
                    out.extend(std::iter::repeat_n(b, 257 - usize::from(len)));
                }
                i += 1;
            }
        }
    }
    out
}

fn lzw(data: &[u8], parm: Option<&Dict>) -> Vec<u8> {
    let early = parm
        .and_then(|p| p.get(b"EarlyChange"))
        .and_then(Obj::as_i64)
        .unwrap_or(1);
    let mut table: Vec<Vec<u8>> = (0..=255u8).map(|b| vec![b]).collect();
    table.push(Vec::new());
    table.push(Vec::new());
    let mut out = Vec::new();
    let mut width = 9;
    let mut bits: u32 = 0;
    let mut nbits = 0;
    let mut prev: Option<Vec<u8>> = None;
    for &byte in data {
        bits = bits << 8 | u32::from(byte);
        nbits += 8;
        while nbits >= width {
            let code = ((bits >> (nbits - width)) & ((1 << width) - 1)) as usize;
            nbits -= width;
            match code {
                256 => {
                    table.truncate(258);
                    width = 9;
                    prev = None;
                    continue;
                }
                257 => return out,
                _ => {}
            }
            let entry = if code < table.len() {
                table[code].clone()
            } else if let Some(p) = &prev {
                let mut e = p.clone();
                e.push(p[0]);
                e
            } else {
                return out;
            };
            out.extend_from_slice(&entry);
            if let Some(p) = prev.take()
                && !p.is_empty()
            {
                let mut n = p;
                n.push(entry[0]);
                table.push(n);
            }
            prev = Some(entry);
            let limit = (1usize << width) - usize::from(early == 1);
            if table.len() >= limit && width < 12 {
                width += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters() {
        assert_eq!(ascii85(b"<~87cURD]i,\"Ebo80~>").unwrap(), b"Hello World!");
        assert_eq!(ascii_hex(b"48656C6C6F>"), b"Hello");
        assert_eq!(
            run_length(&[2, b'a', b'b', b'c', 254, b'x', 128]),
            b"abcxxx"
        );
        // zlib of "hello" via flate2.
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, b"hello").unwrap();
        assert_eq!(inflate(&enc.finish().unwrap()), b"hello");
    }

    #[test]
    fn png_up_predictor() {
        let mut p = Dict::default();
        p.0.push((b"Predictor".to_vec(), Obj::Int(12)));
        p.0.push((b"Columns".to_vec(), Obj::Int(2)));
        let rows = vec![2, 1, 2, 2, 1, 1];
        assert_eq!(predict(rows, Some(&p)).unwrap(), vec![1, 2, 2, 3]);
    }

    #[test]
    fn broken_files_are_errors_not_panics() {
        for data in [
            &b""[..],
            b"%PDF-1.4\n",
            b"%PDF-1.4\n1 0 obj <</Type/Catalog>> endobj\nstartxref\n999999\n%%EOF",
        ] {
            let _ = PdfFile::open(data);
        }
        // A catalog without a usable xref is still found by scanning.
        let f = PdfFile::open(b"%PDF-1.4\n7 0 obj <</Type/Catalog/Pages 8 0 R>> endobj\ntrailer <</Root 7 0 R>>\n%%EOF").unwrap();
        assert!(f.root().is_some());
    }
}
