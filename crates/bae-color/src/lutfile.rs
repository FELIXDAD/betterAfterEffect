//! LUT file readers: `.cube` (Iridas / Resolve, 1D, 3D or shaper + 3D),
//! `.spi1d` and `.spi3d` (Sony Pictures Imageworks).

use std::path::Path;
use std::sync::Arc;

use crate::ColorError;
use crate::lut::{Interpolation, Lut1d, Lut3d};
use crate::transform::{ColorOp, ColorTransform};

fn perr(file: &str, line: usize, msg: impl Into<String>) -> ColorError {
    ColorError::Parse {
        what: file.to_string(),
        message: format!("line {line}: {}", msg.into()),
    }
}

fn floats(s: &str) -> Option<Vec<f32>> {
    s.split_whitespace().map(|t| t.parse::<f32>().ok()).collect()
}

/// Reads a LUT file into a transform (3D LUTs use `interpolation`).
pub fn load(path: &Path, interpolation: Interpolation) -> Result<ColorTransform, ColorError> {
    let text = std::fs::read_to_string(path).map_err(|e| ColorError::Io(format!("{}: {e}", path.display())))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    parse(name, &text, interpolation)
}

/// Parses LUT text; the format is chosen by the file name's extension.
pub fn parse(name: &str, text: &str, interpolation: Interpolation) -> Result<ColorTransform, ColorError> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "cube" => parse_cube(name, text, interpolation),
        "spi1d" => parse_spi1d(name, text).map(|l| ColorTransform::from_ops([ColorOp::Lut1d(Arc::new(l))])),
        "spi3d" => parse_spi3d(name, text, interpolation).map(|l| ColorTransform::from_ops([ColorOp::Lut3d(Arc::new(l))])),
        _ => Err(ColorError::Unsupported(format!("LUT format `.{ext}` ({name})"))),
    }
}

/// `.cube`: `LUT_1D_SIZE` and / or `LUT_3D_SIZE`, `DOMAIN_MIN/MAX` or
/// Resolve's `LUT_1D_INPUT_RANGE` / `LUT_3D_INPUT_RANGE`. A file with both
/// sizes is a 1D shaper followed by a 3D LUT.
pub fn parse_cube(name: &str, text: &str, interpolation: Interpolation) -> Result<ColorTransform, ColorError> {
    let mut size1 = 0usize;
    let mut size3 = 0usize;
    let mut dmin = [0.0f32; 3];
    let mut dmax = [1.0f32; 3];
    let mut range1: Option<[f32; 2]> = None;
    let mut range3: Option<[f32; 2]> = None;
    let mut data: Vec<[f32; 3]> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let key = it.next().unwrap_or("");
        let rest: Vec<&str> = it.collect();
        let num = |i: usize| -> Result<f32, ColorError> {
            rest.get(i).and_then(|t| t.parse().ok()).ok_or_else(|| perr(name, n + 1, format!("bad `{key}`")))
        };
        match key {
            "TITLE" => {}
            "LUT_1D_SIZE" => size1 = num(0)? as usize,
            "LUT_3D_SIZE" => size3 = num(0)? as usize,
            "DOMAIN_MIN" => dmin = [num(0)?, num(1)?, num(2)?],
            "DOMAIN_MAX" => dmax = [num(0)?, num(1)?, num(2)?],
            "LUT_1D_INPUT_RANGE" => range1 = Some([num(0)?, num(1)?]),
            "LUT_3D_INPUT_RANGE" => range3 = Some([num(0)?, num(1)?]),
            k if k.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+' || c == '.') => {
                let v = floats(line).filter(|v| v.len() == 3).ok_or_else(|| perr(name, n + 1, "expected 3 numbers"))?;
                data.push([v[0], v[1], v[2]]);
            }
            // Unknown keywords (LUT_IN_VIDEO_RANGE …) are ignored, as Resolve does.
            _ => {}
        }
    }
    let expected = size1 + size3 * size3 * size3;
    if size1 == 0 && size3 == 0 {
        return Err(perr(name, 0, "no LUT_1D_SIZE or LUT_3D_SIZE"));
    }
    if data.len() != expected {
        return Err(perr(name, 0, format!("expected {expected} entries, found {}", data.len())));
    }
    let mut ops = Vec::new();
    if size1 > 0 {
        let (lo, hi) = match range1 {
            Some(r) => ([r[0]; 3], [r[1]; 3]),
            None => (dmin, dmax),
        };
        ops.push(ColorOp::Lut1d(Arc::new(Lut1d {
            entries: data[..size1].to_vec(),
            domain_min: lo,
            domain_max: hi,
        })));
    }
    if size3 > 0 {
        if size3 < 2 {
            return Err(perr(name, 0, "LUT_3D_SIZE must be at least 2"));
        }
        let (lo, hi) = match range3 {
            Some(r) => ([r[0]; 3], [r[1]; 3]),
            // A shaper + 3D file applies DOMAIN_* to the shaper only.
            None if size1 > 0 => ([0.0; 3], [1.0; 3]),
            None => (dmin, dmax),
        };
        ops.push(ColorOp::Lut3d(Arc::new(Lut3d {
            size: size3,
            data: data[size1..].to_vec(),
            domain_min: lo,
            domain_max: hi,
            interpolation,
        })));
    }
    Ok(ColorTransform::from_ops(ops))
}

/// `.spi1d`: `Version 1`, `From lo hi`, `Length n`, `Components 1|3`, then the
/// values between `{` and `}`.
pub fn parse_spi1d(name: &str, text: &str) -> Result<Lut1d, ColorError> {
    let mut from = [0.0f32, 1.0];
    let mut length = 0usize;
    let mut components = 1usize;
    let mut entries: Vec<[f32; 3]> = Vec::new();
    let mut in_data = false;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if in_data {
            if line.starts_with('}') {
                in_data = false;
                continue;
            }
            let v = floats(line).ok_or_else(|| perr(name, n + 1, "bad value"))?;
            match (components, v.len()) {
                (1, 1) => entries.push([v[0]; 3]),
                (2, 2) => entries.push([v[0], v[1], v[1]]),
                (3, 3) => entries.push([v[0], v[1], v[2]]),
                _ => return Err(perr(name, n + 1, format!("expected {components} components"))),
            }
            continue;
        }
        let mut it = line.split_whitespace();
        match it.next() {
            Some("Version") => {}
            Some("From") => {
                let v: Vec<f32> = it.filter_map(|t| t.parse().ok()).collect();
                if v.len() != 2 {
                    return Err(perr(name, n + 1, "bad From"));
                }
                from = [v[0], v[1]];
            }
            Some("Length") => length = it.next().and_then(|t| t.parse().ok()).ok_or_else(|| perr(name, n + 1, "bad Length"))?,
            Some("Components") => {
                components = it.next().and_then(|t| t.parse().ok()).ok_or_else(|| perr(name, n + 1, "bad Components"))?
            }
            Some("{") => in_data = true,
            Some(other) => return Err(perr(name, n + 1, format!("unexpected `{other}`"))),
            None => {}
        }
    }
    if entries.len() != length || length < 2 {
        return Err(perr(name, 0, format!("Length {length} but {} values", entries.len())));
    }
    Ok(Lut1d {
        entries,
        domain_min: [from[0]; 3],
        domain_max: [from[1]; 3],
    })
}

/// `.spi3d`: `SPILUT 1.0`, `3 3`, `n n n`, then `i j k r g b` lines.
pub fn parse_spi3d(name: &str, text: &str, interpolation: Interpolation) -> Result<Lut3d, ColorError> {
    let mut lines = text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.trim().starts_with('#'));
    let header = lines.next().map(|(_, l)| l.trim()).unwrap_or("");
    if !header.starts_with("SPILUT") {
        return Err(perr(name, 1, "missing SPILUT header"));
    }
    lines.next();
    let (sn, sizes) = lines.next().ok_or_else(|| perr(name, 3, "missing sizes"))?;
    let s: Vec<usize> = sizes.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    if s.len() != 3 || s[0] != s[1] || s[1] != s[2] || s[0] < 2 {
        return Err(perr(name, sn + 1, "expected equal sizes ≥ 2"));
    }
    let n = s[0];
    let mut data = vec![[f32::NAN; 3]; n * n * n];
    for (ln, l) in lines {
        let v = floats(l).filter(|v| v.len() == 6).ok_or_else(|| perr(name, ln + 1, "expected `i j k r g b`"))?;
        let (i, j, k) = (v[0] as usize, v[1] as usize, v[2] as usize);
        if i >= n || j >= n || k >= n {
            return Err(perr(name, ln + 1, "index out of range"));
        }
        data[i + j * n + k * n * n] = [v[3], v[4], v[5]];
    }
    if data.iter().any(|e| e[0].is_nan()) {
        return Err(perr(name, 0, "missing entries"));
    }
    Ok(Lut3d {
        size: n,
        data,
        domain_min: [0.0; 3],
        domain_max: [1.0; 3],
        interpolation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_1d_3d_and_shaper() {
        let t = parse("a.cube", "TITLE \"x\"\nLUT_1D_SIZE 3\nDOMAIN_MAX 2 2 2\n0 0 0\n0.25 0.25 0.25\n1 1 1\n", Interpolation::Linear).unwrap();
        let o = t.apply_f64([1.0, 1.0, 2.0]);
        assert!((o[0] - 0.25).abs() < 1e-7 && (o[2] - 1.0).abs() < 1e-7);

        let mut body = String::new();
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    body.push_str(&format!("{} {} {}\n", r as f32 * 0.5, g as f32, b as f32));
                }
            }
        }
        let t = parse("b.cube", &format!("# comment\nLUT_3D_SIZE 2\n{body}"), Interpolation::Tetrahedral).unwrap();
        let o = t.apply_f64([0.5, 0.5, 0.5]);
        assert!((o[0] - 0.25).abs() < 1e-7 && (o[1] - 0.5).abs() < 1e-7);

        let shaper = format!("LUT_1D_SIZE 2\nLUT_3D_SIZE 2\nLUT_1D_INPUT_RANGE 0 4\n0 0 0\n1 1 1\n{body}");
        let t = parse("c.cube", &shaper, Interpolation::Linear).unwrap();
        assert_eq!(t.ops().len(), 2);
        let o = t.apply_f64([2.0, 2.0, 2.0]);
        assert!((o[0] - 0.25).abs() < 1e-7, "{o:?}");
        assert!(parse("d.cube", "LUT_3D_SIZE 2\n0 0 0\n", Interpolation::Linear).is_err());
    }

    #[test]
    fn spi_formats() {
        let l = parse_spi1d("a.spi1d", "Version 1\nFrom -0.5 1.5\nLength 3\nComponents 1\n{\n0\n0.5\n2\n}\n").unwrap();
        assert_eq!(l.domain_min, [-0.5; 3]);
        assert!((l.eval_channel(1, 0.5) - 0.5).abs() < 1e-7);
        let mut s = String::from("SPILUT 1.0\n3 3\n2 2 2\n");
        for i in 0..2 {
            for j in 0..2 {
                for k in 0..2 {
                    s.push_str(&format!("{i} {j} {k} {} {} {}\n", i as f32, j as f32 * 0.5, k as f32));
                }
            }
        }
        let l = parse_spi3d("b.spi3d", &s, Interpolation::Linear).unwrap();
        let o = l.eval([1.0, 1.0, 0.0]);
        assert!((o[1] - 0.5).abs() < 1e-7);
    }
}
