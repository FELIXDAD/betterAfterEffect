//! Text decoding for placeholder text layers (§9.5–9.10): character codes →
//! Unicode through `ToUnicode`, simple-font encodings, or (for Illustrator's
//! CID fonts, which carry no `ToUnicode`) the ASCII range of the Adobe
//! character collections. Glyph outlines are not read; text that cannot be
//! decoded becomes U+FFFD and the import reports it.

use std::collections::HashMap;

use super::file::PdfFile;
use super::lexer::{Dict, Lexer, Obj, Token};

/// A font, as far as decoding text needs it.
#[derive(Debug, Default)]
pub(crate) struct Font {
    /// PostScript name without the subset prefix (`ABCDEF+`).
    pub name: String,
    /// Bytes per character code (1 for simple fonts, 2 for Type0 with a
    /// two-byte CMap).
    two_byte: bool,
    /// From the `ToUnicode` CMap.
    to_unicode: HashMap<u32, String>,
    /// Simple fonts: code → char from the encoding.
    simple: Option<[char; 256]>,
    /// Type0 without ToUnicode: an Adobe character collection whose CIDs
    /// 1–95 are ASCII 0x20–0x7E.
    adobe_ascii: bool,
    /// Type0 with a UCS-2 / UTF-16 CMap: codes are Unicode.
    unicode_cmap: bool,
}

impl Font {
    pub fn load(pdf: &PdfFile<'_>, obj: &Obj) -> Font {
        let Some(dict) = pdf.resolve(obj).as_dict().cloned() else {
            return Font::default();
        };
        let base = match pdf.dict_get(&dict, b"BaseFont") {
            Obj::Name(n) => String::from_utf8_lossy(&n).into_owned(),
            _ => String::new(),
        };
        let name = match base.split_once('+') {
            Some((prefix, rest))
                if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) =>
            {
                rest.to_owned()
            }
            _ => base,
        };
        let mut font = Font {
            name,
            ..Font::default()
        };
        if let s @ Obj::Stream(_) = pdf.dict_get(&dict, b"ToUnicode")
            && let Ok(data) = pdf.stream_data(&s)
        {
            font.to_unicode = parse_cmap(&data);
        }
        let subtype = pdf.dict_get(&dict, b"Subtype");
        if subtype.as_name() == Some(b"Type0") {
            font.two_byte = true;
            if let Obj::Name(enc) = pdf.dict_get(&dict, b"Encoding") {
                let e = String::from_utf8_lossy(&enc);
                font.unicode_cmap = e.contains("UCS2") || e.contains("UTF16");
            }
            let descendant = match pdf.dict_get(&dict, b"DescendantFonts") {
                Obj::Array(a) => a.first().map(|d| pdf.resolve(d)),
                _ => None,
            };
            if let Some(info) = descendant
                .as_ref()
                .and_then(Obj::as_dict)
                .map(|d| pdf.dict_get(d, b"CIDSystemInfo"))
                && let Some(info) = info.as_dict()
                && let Obj::Str(reg) = pdf.dict_get(info, b"Registry")
                && let Obj::Str(ord) = pdf.dict_get(info, b"Ordering")
            {
                font.adobe_ascii = reg == b"Adobe"
                    && matches!(ord.as_slice(), b"GB1" | b"Japan1" | b"CNS1" | b"Korea1");
            }
        } else {
            font.simple = Some(simple_encoding(pdf, &dict));
        }
        font
    }

    /// Decodes a shown string.
    pub fn decode(&self, bytes: &[u8]) -> String {
        let mut out = String::new();
        if self.two_byte {
            for pair in bytes.chunks(2) {
                let code = pair.iter().fold(0u32, |acc, &b| acc << 8 | u32::from(b));
                if let Some(s) = self.to_unicode.get(&code) {
                    out.push_str(s);
                } else if self.unicode_cmap {
                    out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                } else if self.adobe_ascii && (1..=95).contains(&code) {
                    out.push(char::from((code + 31) as u8));
                } else {
                    out.push('\u{FFFD}');
                }
            }
        } else {
            for &b in bytes {
                if let Some(s) = self.to_unicode.get(&u32::from(b)) {
                    out.push_str(s);
                } else if let Some(table) = &self.simple {
                    out.push(table[usize::from(b)]);
                } else {
                    out.push(char::from(b));
                }
            }
        }
        out
    }
}

/// WinAnsi-like base table with `/Differences` applied (glyph names for
/// letters, digits, common punctuation and `uniXXXX`).
fn simple_encoding(pdf: &PdfFile<'_>, dict: &Dict) -> [char; 256] {
    let mut table: [char; 256] = std::array::from_fn(|i| {
        let c = i as u8;
        match c {
            0x80..=0x9F => WIN_ANSI_80[usize::from(c - 0x80)],
            _ => char::from(c),
        }
    });
    if let Some(enc) = pdf.dict_get(dict, b"Encoding").as_dict()
        && let Obj::Array(diffs) = pdf.dict_get(enc, b"Differences")
    {
        let mut code = 0usize;
        for d in &diffs {
            match d {
                Obj::Int(i) => code = (*i).clamp(0, 255) as usize,
                Obj::Name(n) => {
                    if let Some(c) = glyph_char(n)
                        && code < 256
                    {
                        table[code] = c;
                    }
                    code += 1;
                }
                _ => {}
            }
        }
    }
    table
}

const WIN_ANSI_80: [char; 32] = [
    '€', '\u{FFFD}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{FFFD}', 'Ž',
    '\u{FFFD}', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{FFFD}',
    'ž', 'Ÿ',
];

fn glyph_char(name: &[u8]) -> Option<char> {
    let s = std::str::from_utf8(name).ok()?;
    if s.chars().count() == 1 {
        return s.chars().next();
    }
    if let Some(hex) = s.strip_prefix("uni").filter(|h| h.len() == 4) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    Some(match s {
        "space" => ' ',
        "zero" => '0',
        "one" => '1',
        "two" => '2',
        "three" => '3',
        "four" => '4',
        "five" => '5',
        "six" => '6',
        "seven" => '7',
        "eight" => '8',
        "nine" => '9',
        "period" => '.',
        "comma" => ',',
        "colon" => ':',
        "semicolon" => ';',
        "hyphen" | "minus" => '-',
        "exclam" => '!',
        "question" => '?',
        "quoteright" => '’',
        "quoteleft" => '‘',
        "quotedbl" => '"',
        "quotesingle" => '\'',
        "parenleft" => '(',
        "parenright" => ')',
        "slash" => '/',
        "ampersand" => '&',
        "at" => '@',
        "numbersign" => '#',
        "percent" => '%',
        "dollar" => '$',
        "asterisk" => '*',
        "plus" => '+',
        "equal" => '=',
        "underscore" => '_',
        "endash" => '–',
        "emdash" => '—',
        "ellipsis" => '…',
        "bullet" => '•',
        _ => return None,
    })
}

/// `bfchar` / `bfrange` of a ToUnicode CMap.
fn parse_cmap(data: &[u8]) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let mut lx = Lexer::new(data, 0);
    let code = |b: &[u8]| b.iter().fold(0u32, |acc, &x| acc << 8 | u32::from(x));
    let utf16 = |b: &[u8]| {
        let units: Vec<u16> = b
            .chunks(2)
            .map(|c| c.iter().fold(0u16, |a, &x| a << 8 | u16::from(x)))
            .collect();
        String::from_utf16_lossy(&units)
    };
    while let Some(tok) = lx.token() {
        match tok {
            Token::Keyword(k) if k == b"beginbfchar" => {
                while let Some(Token::Str(src)) = lx.token() {
                    if let Some(Token::Str(dst)) = lx.token() {
                        map.insert(code(&src), utf16(&dst));
                    }
                }
            }
            Token::Keyword(k) if k == b"beginbfrange" => {
                while let (Some(Token::Str(lo)), Some(Token::Str(hi))) = (lx.token(), lx.token()) {
                    let (lo, hi) = (code(&lo), code(&hi));
                    if hi < lo || hi - lo > 0xFFFF {
                        continue;
                    }
                    match lx.token() {
                        Some(Token::Str(dst)) => {
                            let mut units: Vec<u16> = dst
                                .chunks(2)
                                .map(|c| c.iter().fold(0u16, |a, &x| a << 8 | u16::from(x)))
                                .collect();
                            for c in lo..=hi {
                                map.insert(c, String::from_utf16_lossy(&units));
                                if let Some(last) = units.last_mut() {
                                    *last = last.wrapping_add(1);
                                }
                            }
                        }
                        Some(Token::ArrayStart) => {
                            let mut c = lo;
                            while let Some(Token::Str(dst)) = lx.token() {
                                map.insert(c, utf16(&dst));
                                c += 1;
                            }
                        }
                        _ => break,
                    }
                }
            }
            _ => {}
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmap_ranges_and_chars() {
        let cmap = b"2 beginbfchar <0003> <0020> <0029> <0048> endbfchar
            1 beginbfrange <0046> <0048> <0065> endbfrange
            1 beginbfrange <0050> <0051> [<4F60> <597D>] endbfrange";
        let m = parse_cmap(cmap);
        assert_eq!(m[&0x29], "H");
        assert_eq!(m[&0x47], "f");
        assert_eq!(m[&0x51], "好");
    }

    #[test]
    fn adobe_collections_decode_ascii() {
        let f = Font {
            two_byte: true,
            adobe_ascii: true,
            ..Font::default()
        };
        assert_eq!(
            f.decode(&[0, 0x29, 0, 0x46, 0, 0x4D, 0, 0x4D, 0, 0x50]),
            "Hello"
        );
        assert_eq!(f.decode(&[0x12, 0x34]), "\u{FFFD}");
    }
}
