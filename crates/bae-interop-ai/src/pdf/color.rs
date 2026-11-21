//! Colour spaces (§8.6) and functions (§7.10), enough for Illustrator output:
//! device, ICC-based, indexed, separation / DeviceN (spot colours) and Lab
//! colours become straight RGB without colour management; sampled,
//! exponential, stitching and PostScript calculator functions drive gradients
//! and tint transforms.

use super::file::PdfFile;
use super::lexer::{Dict, Lexer, Obj, Token};

const MAX_DEPTH: usize = 16;

/// A colour space.
#[derive(Clone, Debug)]
pub(crate) enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    Lab,
    Indexed {
        base: Box<ColorSpace>,
        hival: usize,
        lookup: Vec<u8>,
    },
    /// Separation (one tint) or DeviceN (several), mapped through `func`.
    Tint {
        n: usize,
        alt: Box<ColorSpace>,
        func: Function,
    },
    /// Pattern colour space (fills with patterns; the underlying space of
    /// uncoloured patterns is ignored).
    Pattern,
}

impl ColorSpace {
    pub fn components(&self) -> usize {
        match self {
            ColorSpace::Gray | ColorSpace::Indexed { .. } => 1,
            ColorSpace::Rgb | ColorSpace::Lab => 3,
            ColorSpace::Cmyk => 4,
            ColorSpace::Tint { n, .. } => *n,
            ColorSpace::Pattern => 0,
        }
    }

    /// The initial colour (black, tint 1, index 0).
    pub fn initial(&self) -> Vec<f64> {
        match self {
            ColorSpace::Cmyk => vec![0.0, 0.0, 0.0, 1.0],
            ColorSpace::Tint { n, .. } => vec![1.0; *n],
            ColorSpace::Lab => vec![0.0, 0.0, 0.0],
            other => vec![0.0; other.components()],
        }
    }

    /// Straight RGB in 0–1.
    pub fn to_rgb(&self, c: &[f64]) -> [f64; 3] {
        let at = |i: usize| c.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        match self {
            ColorSpace::Gray => [at(0); 3],
            ColorSpace::Rgb => [at(0), at(1), at(2)],
            ColorSpace::Cmyk => {
                let k = 1.0 - at(3);
                [(1.0 - at(0)) * k, (1.0 - at(1)) * k, (1.0 - at(2)) * k]
            }
            ColorSpace::Lab => lab_to_rgb(
                c.first().copied().unwrap_or(0.0),
                c.get(1).copied().unwrap_or(0.0),
                c.get(2).copied().unwrap_or(0.0),
            ),
            ColorSpace::Indexed {
                base,
                hival,
                lookup,
            } => {
                let i = (c.first().copied().unwrap_or(0.0).round().max(0.0) as usize).min(*hival);
                let n = base.components();
                let comps: Vec<f64> = (0..n)
                    .map(|k| f64::from(lookup.get(i * n + k).copied().unwrap_or(0)) / 255.0)
                    .collect();
                base.to_rgb(&comps)
            }
            ColorSpace::Tint { alt, func, .. } => alt.to_rgb(&func.eval(c)),
            ColorSpace::Pattern => [0.5; 3],
        }
    }

    /// Parses a colour space object (a name already looked up in the
    /// resources, or an array).
    pub fn parse(pdf: &PdfFile<'_>, obj: &Obj, warnings: &mut Vec<String>) -> ColorSpace {
        parse_cs(pdf, obj, warnings, 0)
    }
}

fn parse_cs(pdf: &PdfFile<'_>, obj: &Obj, warnings: &mut Vec<String>, depth: usize) -> ColorSpace {
    if depth > MAX_DEPTH {
        return ColorSpace::Rgb;
    }
    let obj = pdf.resolve(obj);
    let (family, args): (&[u8], &[Obj]) = match &obj {
        Obj::Name(n) => (n.as_slice(), &[]),
        Obj::Array(a) => match a.first().map(|o| pdf.resolve(o)) {
            Some(Obj::Name(_)) => (a[0].as_name().unwrap_or(b""), &a[1..]),
            _ => (b"", &[]),
        },
        _ => (b"", &[]),
    };
    match family {
        b"DeviceGray" | b"G" | b"CalGray" => ColorSpace::Gray,
        b"DeviceRGB" | b"RGB" | b"CalRGB" => ColorSpace::Rgb,
        b"DeviceCMYK" | b"CMYK" => ColorSpace::Cmyk,
        b"Lab" => ColorSpace::Lab,
        b"Pattern" => ColorSpace::Pattern,
        b"ICCBased" => {
            let stream = args.first().map(|o| pdf.resolve(o));
            let dict = stream.as_ref().and_then(Obj::as_dict);
            if let Some(alt) = dict.and_then(|d| d.get(b"Alternate")) {
                return parse_cs(pdf, alt, warnings, depth + 1);
            }
            match dict.and_then(|d| d.get(b"N")).and_then(Obj::as_i64) {
                Some(1) => ColorSpace::Gray,
                Some(4) => ColorSpace::Cmyk,
                _ => ColorSpace::Rgb,
            }
        }
        b"Indexed" | b"I" => {
            let base = args
                .first()
                .map_or(ColorSpace::Rgb, |b| parse_cs(pdf, b, warnings, depth + 1));
            let hival = args.get(1).and_then(Obj::as_i64).unwrap_or(0).clamp(0, 255) as usize;
            let lookup = match args.get(2).map(|o| pdf.resolve(o)) {
                Some(Obj::Str(s)) => s,
                Some(s @ Obj::Stream(_)) => pdf.stream_data(&s).unwrap_or_default(),
                _ => Vec::new(),
            };
            ColorSpace::Indexed {
                base: Box::new(base),
                hival,
                lookup,
            }
        }
        b"Separation" | b"DeviceN" => {
            let n = if family == b"Separation" {
                1
            } else {
                match args.first().map(|o| pdf.resolve(o)) {
                    Some(Obj::Array(names)) => names.len().max(1),
                    _ => 1,
                }
            };
            let alt = args
                .get(1)
                .map_or(ColorSpace::Cmyk, |a| parse_cs(pdf, a, warnings, depth + 1));
            let func = args
                .get(2)
                .and_then(|f| Function::parse(pdf, f, warnings))
                .unwrap_or_else(|| {
                    warnings
                        .push("spot colour without a usable tint transform; shown as grey".into());
                    Function::Constant(vec![0.5; alt.components()])
                });
            ColorSpace::Tint {
                n,
                alt: Box::new(alt),
                func,
            }
        }
        other => {
            warnings.push(format!(
                "colour space {} is not supported; using RGB",
                String::from_utf8_lossy(other)
            ));
            ColorSpace::Rgb
        }
    }
}

/// CIE L*a*b* (D50 white, as PDF's default) → sRGB, without adaptation.
fn lab_to_rgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let f = |t: f64| {
        if t > 6.0 / 29.0 {
            t * t * t
        } else {
            3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0)
        }
    };
    let (x, y, z) = (0.9642 * f(fx), f(fy), 0.8249 * f(fz));
    let lin = [
        3.1339 * x - 1.6169 * y - 0.4906 * z,
        -0.9788 * x + 1.9161 * y + 0.0335 * z,
        0.0719 * x - 0.2290 * y + 1.4052 * z,
    ];
    lin.map(|v| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// A PDF function.
#[derive(Clone, Debug)]
pub(crate) enum Function {
    /// Type 0, one input, linear interpolation.
    Sampled {
        domain: [f64; 2],
        encode: [f64; 2],
        decode: Vec<f64>,
        size: usize,
        outputs: usize,
        /// Samples normalized to 0–1, `size × outputs`.
        samples: Vec<f64>,
    },
    /// Type 2.
    Exponential {
        domain: [f64; 2],
        c0: Vec<f64>,
        c1: Vec<f64>,
        n: f64,
    },
    /// Type 3.
    Stitching {
        domain: [f64; 2],
        functions: Vec<Function>,
        bounds: Vec<f64>,
        encode: Vec<f64>,
    },
    /// Type 4.
    PostScript {
        domain: Vec<f64>,
        range: Vec<f64>,
        code: Vec<PsOp>,
    },
    /// An array of one-output functions (shadings allow this).
    Array(Vec<Function>),
    /// Fallback.
    Constant(Vec<f64>),
}

fn floats(o: Option<&Obj>) -> Vec<f64> {
    o.and_then(Obj::as_array)
        .map(|a| a.iter().filter_map(Obj::as_f64).collect())
        .unwrap_or_default()
}

fn pair(v: &[f64], d: [f64; 2]) -> [f64; 2] {
    match v {
        [a, b, ..] => [*a, *b],
        _ => d,
    }
}

impl Function {
    pub fn parse(pdf: &PdfFile<'_>, obj: &Obj, warnings: &mut Vec<String>) -> Option<Function> {
        parse_fn(pdf, obj, warnings, 0)
    }

    pub fn eval(&self, input: &[f64]) -> Vec<f64> {
        self.eval_depth(input, 0)
    }

    fn eval_depth(&self, input: &[f64], depth: usize) -> Vec<f64> {
        if depth > MAX_DEPTH {
            return Vec::new();
        }
        let x0 = input.first().copied().unwrap_or(0.0);
        match self {
            Function::Sampled {
                domain,
                encode,
                decode,
                size,
                outputs,
                samples,
            } => {
                let x = x0.clamp(domain[0].min(domain[1]), domain[0].max(domain[1]));
                let span = domain[1] - domain[0];
                let e = if span.abs() < 1e-12 {
                    encode[0]
                } else {
                    encode[0] + (x - domain[0]) / span * (encode[1] - encode[0])
                };
                let e = e.clamp(0.0, (*size).saturating_sub(1) as f64);
                let i0 = e.floor() as usize;
                let i1 = (i0 + 1).min(size.saturating_sub(1));
                let t = e - i0 as f64;
                (0..*outputs)
                    .map(|k| {
                        let s0 = samples.get(i0 * outputs + k).copied().unwrap_or(0.0);
                        let s1 = samples.get(i1 * outputs + k).copied().unwrap_or(s0);
                        let s = s0 + (s1 - s0) * t;
                        let d = pair(decode.get(2 * k..).unwrap_or(&[]), [0.0, 1.0]);
                        d[0] + s * (d[1] - d[0])
                    })
                    .collect()
            }
            Function::Exponential { domain, c0, c1, n } => {
                let x = x0.clamp(domain[0].min(domain[1]), domain[0].max(domain[1]));
                let xn = if *n == 1.0 { x } else { x.max(0.0).powf(*n) };
                c0.iter().zip(c1).map(|(a, b)| a + xn * (b - a)).collect()
            }
            Function::Stitching {
                domain,
                functions,
                bounds,
                encode,
            } => {
                let x = x0.clamp(domain[0].min(domain[1]), domain[0].max(domain[1]));
                let k = bounds
                    .iter()
                    .take_while(|b| x >= **b)
                    .count()
                    .min(functions.len().saturating_sub(1));
                let lo = if k == 0 { domain[0] } else { bounds[k - 1] };
                let hi = bounds.get(k).copied().unwrap_or(domain[1]);
                let e = pair(encode.get(2 * k..).unwrap_or(&[]), [0.0, 1.0]);
                let t = if (hi - lo).abs() < 1e-12 {
                    e[0]
                } else {
                    e[0] + (x - lo) / (hi - lo) * (e[1] - e[0])
                };
                functions
                    .get(k)
                    .map_or_else(Vec::new, |f| f.eval_depth(&[t], depth + 1))
            }
            Function::PostScript {
                domain,
                range,
                code,
            } => {
                let mut stack: Vec<f64> = input
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let d = pair(domain.get(2 * i..).unwrap_or(&[]), [f64::MIN, f64::MAX]);
                        v.clamp(d[0].min(d[1]), d[0].max(d[1]))
                    })
                    .collect();
                run_ps(code, &mut stack, 0);
                let n = range.len() / 2;
                let start = stack.len().saturating_sub(n);
                stack[start..]
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let r = pair(&range[2 * i..], [0.0, 1.0]);
                        v.clamp(r[0].min(r[1]), r[0].max(r[1]))
                    })
                    .collect()
            }
            Function::Array(fs) => fs
                .iter()
                .flat_map(|f| f.eval_depth(input, depth + 1).into_iter().take(1))
                .collect(),
            Function::Constant(c) => c.clone(),
        }
    }

    /// For one-input functions: inputs where the output changes slope
    /// (domain ends, stitching bounds, sample positions), sorted.
    pub fn breakpoints(&self) -> Vec<f64> {
        let mut out = Vec::new();
        self.collect_breaks(0.0, 1.0, &mut out, 0);
        out.sort_by(f64::total_cmp);
        out.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        out
    }

    /// Breakpoints of this function mapped through `x = a + b·input`… in the
    /// caller's input space: `map(t) = offset + scale * t`.
    fn collect_breaks(&self, offset: f64, scale: f64, out: &mut Vec<f64>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let map = |t: f64| offset + scale * t;
        match self {
            Function::Stitching {
                domain,
                functions,
                bounds,
                encode,
            } => {
                out.push(map(domain[0]));
                out.push(map(domain[1]));
                for (k, f) in functions.iter().enumerate() {
                    let lo = if k == 0 {
                        domain[0]
                    } else {
                        bounds.get(k - 1).copied().unwrap_or(domain[0])
                    };
                    let hi = bounds.get(k).copied().unwrap_or(domain[1]);
                    out.push(map(lo));
                    let e = pair(encode.get(2 * k..).unwrap_or(&[]), [0.0, 1.0]);
                    if (e[1] - e[0]).abs() > 1e-12 {
                        // inner t = e0 + (x - lo)/(hi - lo)·(e1 - e0)  ⇒  x = lo + (t - e0)·(hi - lo)/(e1 - e0)
                        let s = (hi - lo) / (e[1] - e[0]);
                        f.collect_breaks(map(lo - e[0] * s), scale * s, out, depth + 1);
                    }
                }
            }
            Function::Sampled { domain, size, .. } => {
                let n = (*size).clamp(2, 256);
                for i in 0..n {
                    out.push(map(
                        domain[0] + (domain[1] - domain[0]) * i as f64 / (n - 1) as f64
                    ));
                }
            }
            Function::Exponential { domain, n, .. } => {
                out.push(map(domain[0]));
                out.push(map(domain[1]));
                if (*n - 1.0).abs() > 1e-9 {
                    for i in 1..8 {
                        out.push(map(domain[0] + (domain[1] - domain[0]) * f64::from(i) / 8.0));
                    }
                }
            }
            Function::PostScript { .. } => {
                for i in 0..=16 {
                    out.push(map(f64::from(i) / 16.0));
                }
            }
            Function::Array(fs) => {
                for f in fs {
                    f.collect_breaks(offset, scale, out, depth + 1);
                }
            }
            Function::Constant(_) => {}
        }
    }
}

fn parse_fn(
    pdf: &PdfFile<'_>,
    obj: &Obj,
    warnings: &mut Vec<String>,
    depth: usize,
) -> Option<Function> {
    if depth > MAX_DEPTH {
        return None;
    }
    let obj = pdf.resolve(obj);
    if let Obj::Array(a) = &obj {
        let fs: Vec<Function> = a
            .iter()
            .filter_map(|f| parse_fn(pdf, f, warnings, depth + 1))
            .collect();
        return (!fs.is_empty()).then_some(Function::Array(fs));
    }
    let dict: Dict = obj.as_dict()?.clone();
    let domain = floats(dict.get(b"Domain"));
    let d2 = pair(&domain, [0.0, 1.0]);
    match dict.get(b"FunctionType").and_then(Obj::as_i64)? {
        0 => {
            let size = floats(dict.get(b"Size"));
            if size.len() != 1 {
                warnings.push("multi-input sampled functions are not supported".into());
                return None;
            }
            let size = (size[0].max(1.0) as usize).min(1 << 16);
            let range = floats(dict.get(b"Range"));
            let outputs = (range.len() / 2).max(1);
            let bps = dict
                .get(b"BitsPerSample")
                .and_then(Obj::as_i64)
                .unwrap_or(8);
            let data = pdf.stream_data(&obj).ok()?;
            let max = ((1u64 << bps.clamp(1, 32)) - 1) as f64;
            let mut samples = Vec::with_capacity(size * outputs);
            let mut bit = 0usize;
            for _ in 0..size * outputs {
                let mut v = 0u64;
                for _ in 0..bps.clamp(1, 32) {
                    let byte = data.get(bit / 8).copied().unwrap_or(0);
                    v = v << 1 | u64::from(byte >> (7 - bit % 8) & 1);
                    bit += 1;
                }
                samples.push(v as f64 / max);
            }
            let encode = floats(dict.get(b"Encode"));
            let decode = floats(dict.get(b"Decode"));
            Some(Function::Sampled {
                domain: d2,
                encode: pair(&encode, [0.0, (size - 1) as f64]),
                decode: if decode.is_empty() { range } else { decode },
                size,
                outputs,
                samples,
            })
        }
        2 => {
            let c0 = floats(dict.get(b"C0"));
            let c1 = floats(dict.get(b"C1"));
            Some(Function::Exponential {
                domain: d2,
                c0: if c0.is_empty() { vec![0.0] } else { c0 },
                c1: if c1.is_empty() { vec![1.0] } else { c1 },
                n: dict.get(b"N").and_then(Obj::as_f64).unwrap_or(1.0),
            })
        }
        3 => {
            let functions: Vec<Function> = dict
                .get(b"Functions")
                .map(|f| pdf.resolve(f))
                .and_then(|f| f.as_array().map(<[Obj]>::to_vec))
                .unwrap_or_default()
                .iter()
                .filter_map(|f| parse_fn(pdf, f, warnings, depth + 1))
                .collect();
            if functions.is_empty() {
                return None;
            }
            Some(Function::Stitching {
                domain: d2,
                functions,
                bounds: floats(dict.get(b"Bounds")),
                encode: floats(dict.get(b"Encode")),
            })
        }
        4 => {
            let data = pdf.stream_data(&obj).ok()?;
            let code = parse_ps(&mut Lexer::new(&data, 0), 0)?;
            Some(Function::PostScript {
                domain,
                range: floats(dict.get(b"Range")),
                code,
            })
        }
        _ => None,
    }
}

/// PostScript calculator operations (§7.10.5).
#[derive(Clone, Debug)]
pub(crate) enum PsOp {
    Num(f64),
    Op(Vec<u8>),
    If(Vec<PsOp>),
    IfElse(Vec<PsOp>, Vec<PsOp>),
}

fn parse_ps(lx: &mut Lexer<'_>, depth: usize) -> Option<Vec<PsOp>> {
    if depth > MAX_DEPTH {
        return None;
    }
    // Skip to the opening brace.
    loop {
        match lx.token()? {
            Token::Keyword(k) if k == b"{" => break,
            _ => {}
        }
    }
    let mut ops = Vec::new();
    let mut blocks: Vec<Vec<PsOp>> = Vec::new();
    loop {
        let save = lx.pos;
        match lx.token()? {
            Token::Int(i) => ops.push(PsOp::Num(i as f64)),
            Token::Real(r) => ops.push(PsOp::Num(r)),
            Token::Keyword(k) if k == b"}" => return Some(ops),
            Token::Keyword(k) if k == b"{" => {
                lx.pos = save;
                blocks.push(parse_ps(lx, depth + 1)?);
            }
            Token::Keyword(k) if k == b"if" => {
                let b = blocks.pop().unwrap_or_default();
                ops.push(PsOp::If(b));
            }
            Token::Keyword(k) if k == b"ifelse" => {
                let b2 = blocks.pop().unwrap_or_default();
                let b1 = blocks.pop().unwrap_or_default();
                ops.push(PsOp::IfElse(b1, b2));
            }
            Token::Keyword(k) => ops.push(PsOp::Op(k)),
            _ => {}
        }
    }
}

fn run_ps(code: &[PsOp], st: &mut Vec<f64>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let pop = |st: &mut Vec<f64>| st.pop().unwrap_or(0.0);
    for op in code {
        if st.len() > 1000 {
            return;
        }
        match op {
            PsOp::Num(v) => st.push(*v),
            PsOp::If(body) => {
                if pop(st) != 0.0 {
                    run_ps(body, st, depth + 1);
                }
            }
            PsOp::IfElse(a, e) => {
                if pop(st) != 0.0 {
                    run_ps(a, st, depth + 1);
                } else {
                    run_ps(e, st, depth + 1);
                }
            }
            PsOp::Op(name) => {
                let un = |st: &mut Vec<f64>, f: fn(f64) -> f64| {
                    let a = pop(st);
                    st.push(f(a));
                };
                let bin = |st: &mut Vec<f64>, f: fn(f64, f64) -> f64| {
                    let y = pop(st);
                    let x = pop(st);
                    st.push(f(x, y));
                };
                match name.as_slice() {
                    b"add" => bin(st, |x, y| x + y),
                    b"sub" => bin(st, |x, y| x - y),
                    b"mul" => bin(st, |x, y| x * y),
                    b"div" => bin(st, |x, y| if y == 0.0 { 0.0 } else { x / y }),
                    b"idiv" => bin(st, |x, y| {
                        if y as i64 == 0 {
                            0.0
                        } else {
                            ((x as i64) / (y as i64)) as f64
                        }
                    }),
                    b"mod" => bin(st, |x, y| {
                        if y as i64 == 0 {
                            0.0
                        } else {
                            ((x as i64) % (y as i64)) as f64
                        }
                    }),
                    b"exp" => bin(st, f64::powf),
                    b"atan" => bin(st, |x, y| {
                        let d = x.atan2(y).to_degrees();
                        if d < 0.0 { d + 360.0 } else { d }
                    }),
                    b"neg" => un(st, |x| -x),
                    b"abs" => un(st, f64::abs),
                    b"ceiling" => un(st, f64::ceil),
                    b"floor" => un(st, f64::floor),
                    b"round" => un(st, f64::round),
                    b"truncate" | b"cvi" => un(st, f64::trunc),
                    b"cvr" => {}
                    b"sqrt" => un(st, |x| x.max(0.0).sqrt()),
                    b"sin" => un(st, |x| x.to_radians().sin()),
                    b"cos" => un(st, |x| x.to_radians().cos()),
                    b"ln" => un(st, |x| if x > 0.0 { x.ln() } else { 0.0 }),
                    b"log" => un(st, |x| if x > 0.0 { x.log10() } else { 0.0 }),
                    b"eq" => bin(st, |x, y| if x == y { 1.0 } else { 0.0 }),
                    b"ne" => bin(st, |x, y| if x != y { 1.0 } else { 0.0 }),
                    b"gt" => bin(st, |x, y| if x > y { 1.0 } else { 0.0 }),
                    b"ge" => bin(st, |x, y| if x >= y { 1.0 } else { 0.0 }),
                    b"lt" => bin(st, |x, y| if x < y { 1.0 } else { 0.0 }),
                    b"le" => bin(st, |x, y| if x <= y { 1.0 } else { 0.0 }),
                    b"and" => bin(st, |x, y| ((x as i64) & (y as i64)) as f64),
                    b"or" => bin(st, |x, y| ((x as i64) | (y as i64)) as f64),
                    b"xor" => bin(st, |x, y| ((x as i64) ^ (y as i64)) as f64),
                    b"not" => un(st, |x| {
                        if x == 0.0 {
                            1.0
                        } else if x == 1.0 {
                            0.0
                        } else {
                            !(x as i64) as f64
                        }
                    }),
                    b"true" => st.push(1.0),
                    b"false" => st.push(0.0),
                    b"pop" => {
                        st.pop();
                    }
                    b"dup" => {
                        let a = st.last().copied().unwrap_or(0.0);
                        st.push(a);
                    }
                    b"exch" => {
                        let y = pop(st);
                        let x = pop(st);
                        st.extend([y, x]);
                    }
                    b"copy" => {
                        let n = (pop(st).max(0.0) as usize).min(st.len());
                        let start = st.len() - n;
                        let tail: Vec<f64> = st[start..].to_vec();
                        st.extend(tail);
                    }
                    b"index" => {
                        let n = pop(st).max(0.0) as usize;
                        let v = st.len().checked_sub(n + 1).map_or(0.0, |i| st[i]);
                        st.push(v);
                    }
                    b"roll" => {
                        let j = pop(st) as i64;
                        let n = (pop(st).max(0.0) as usize).min(st.len());
                        if n > 0 {
                            let start = st.len() - n;
                            let s = &mut st[start..];
                            let k = j.rem_euclid(n as i64) as usize;
                            s.rotate_right(k);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_and_gray() {
        assert_eq!(
            ColorSpace::Cmyk.to_rgb(&[0.0, 1.0, 1.0, 0.0]),
            [1.0, 0.0, 0.0]
        );
        assert_eq!(ColorSpace::Gray.to_rgb(&[0.25]), [0.25; 3]);
        let white = ColorSpace::Lab.to_rgb(&[100.0, 0.0, 0.0]);
        assert!(white.iter().all(|v| (v - 1.0).abs() < 0.02), "{white:?}");
    }

    #[test]
    fn functions() {
        let lin = Function::Exponential {
            domain: [0.0, 1.0],
            c0: vec![1.0, 0.5, 0.0],
            c1: vec![0.0, 0.25, 1.0],
            n: 1.0,
        };
        assert_eq!(lin.eval(&[0.5]), vec![0.5, 0.375, 0.5]);
        let st = Function::Stitching {
            domain: [0.0, 1.0],
            functions: vec![lin.clone(), lin],
            bounds: vec![0.4],
            encode: vec![0.0, 1.0, 0.0, 1.0],
        };
        assert_eq!(st.eval(&[0.4]), vec![1.0, 0.5, 0.0]);
        assert_eq!(st.breakpoints(), vec![0.0, 0.4, 1.0]);
        // { dup 0.5 mul exch 0 mul } : tint t -> (t/2, 0)
        let mut lx = Lexer::new(b"{ dup 0.5 mul exch 0 mul 1 index pop }", 0);
        let code = parse_ps(&mut lx, 0).unwrap();
        let ps = Function::PostScript {
            domain: vec![0.0, 1.0],
            range: vec![0.0, 1.0, 0.0, 1.0],
            code,
        };
        assert_eq!(ps.eval(&[0.8]), vec![0.4, 0.0]);
        let cond = parse_ps(&mut Lexer::new(b"{ 0.5 gt { 1 } { 0 } ifelse }", 0), 0).unwrap();
        let mut st = vec![0.7];
        run_ps(&cond, &mut st, 0);
        assert_eq!(st, vec![1.0]);
    }
}
