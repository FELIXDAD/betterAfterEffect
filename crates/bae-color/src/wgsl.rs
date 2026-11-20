//! WGSL code generation: turns a [`ColorTransform`] into a self-contained WGSL
//! snippet that a render-graph pass splices into its own shader.
//!
//! # Contract with the render graph (workstream 04)
//!
//! [`ColorTransform::to_wgsl`]`(prefix)` returns a [`GpuSnippet`] whose `wgsl`
//! defines, with every identifier starting with `prefix`:
//!
//! ```wgsl
//! struct P_Params { v: array<vec4<f32>, N> }          // N = snippet.params.len()
//! fn P_apply(c: vec3<f32>, p: P_Params /*, t0: texture_3d<f32>, ... */) -> vec3<f32>
//! fn P_apply_premul(c: vec4<f32>, p: P_Params /*, t0 ... */) -> vec4<f32>
//! ```
//!
//! The host shader declares the uniform and the LUT textures with whatever
//! group / binding it likes and calls the entry point:
//!
//! ```wgsl
//! @group(1) @binding(0) var<uniform> color_in: P_Params;
//! @group(1) @binding(1) var color_in_lut0: texture_3d<f32>;
//! // ...
//! let c = P_apply_premul(textureSample(src, smp, uv), color_in, color_in_lut0);
//! ```
//!
//! * Uniform data: [`GpuSnippet::params_bytes`] (an array of `vec4<f32>`, so
//!   the std140 / WGSL uniform layout is just the bytes in order).
//! * Textures: [`GpuSnippet::textures`], in parameter order. All are
//!   `Rgba32Float`, sampled only with `textureLoad` (no sampler, no filtering
//!   feature needed): bind them with `TextureSampleType::Float { filterable:
//!   false }` and the view dimension given by [`LutTexture::dimension`].
//! * Pipeline caching: snippets of transforms with equal
//!   [`ColorTransform::structure_key`] have identical WGSL (for the same
//!   prefix); only the uniform bytes and textures differ.
//! * `P_apply_premul` takes and returns premultiplied RGBA; it unpremultiplies
//!   only when the transform is not a pure matrix ([`ColorTransform::is_linear`]).
//! * Several snippets can live in one shader as long as their prefixes differ.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::FxHasher;

use crate::lut::{Interpolation, Lut1d, Lut3d};
use crate::ops::{FLT_MIN, NegativeStyle};
use crate::transfer::{TransferFunction, k};
use crate::transform::{ColorOp, ColorTransform};

/// Width of the 2D texture that stores a 1D LUT (longer LUTs wrap into rows).
pub const LUT1D_TEXTURE_WIDTH: usize = 4096;

/// Texture dimension of a LUT.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LutDimension {
    /// `texture_2d<f32>` (1D LUT wrapped into rows of [`LUT1D_TEXTURE_WIDTH`]).
    D2,
    /// `texture_3d<f32>`.
    D3,
}

/// A LUT texture the host must create and bind (format `Rgba32Float`).
#[derive(Clone, Debug, PartialEq)]
pub struct LutTexture {
    /// View dimension.
    pub dimension: LutDimension,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Depth in texels (1 for 2D).
    pub depth: u32,
    /// RGBA texels, x fastest, then y, then z (`width * height * depth * 4`
    /// floats): the layout `Queue::write_texture` expects with
    /// `bytes_per_row = width * 16`.
    pub data: Arc<[f32]>,
}

impl LutTexture {
    /// The texel data as bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.data.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn from_lut1d(l: &Lut1d) -> Self {
        let n = l.entries.len();
        let w = n.clamp(1, LUT1D_TEXTURE_WIDTH);
        let h = n.div_ceil(w).max(1);
        let mut data = vec![0.0f32; w * h * 4];
        for (i, e) in l.entries.iter().enumerate() {
            data[i * 4..i * 4 + 3].copy_from_slice(e);
            data[i * 4 + 3] = 1.0;
        }
        LutTexture {
            dimension: LutDimension::D2,
            width: w as u32,
            height: h as u32,
            depth: 1,
            data: data.into(),
        }
    }

    fn from_lut3d(l: &Lut3d) -> Self {
        let mut data = Vec::with_capacity(l.data.len() * 4);
        for e in &l.data {
            data.extend_from_slice(&[e[0], e[1], e[2], 1.0]);
        }
        let s = l.size as u32;
        LutTexture {
            dimension: LutDimension::D3,
            width: s,
            height: s,
            depth: s,
            data: data.into(),
        }
    }
}

/// Generated WGSL for one transform. See the module docs for the contract.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuSnippet {
    /// Identifier prefix of every generated item.
    pub prefix: String,
    /// The WGSL source (struct, helpers, entry points).
    pub wgsl: String,
    /// Uniform data, one `vec4<f32>` per element (at least one element).
    pub params: Vec<[f32; 4]>,
    /// LUT textures, in entry-point parameter order.
    pub textures: Vec<LutTexture>,
    /// Structure hash (equal to [`ColorTransform::structure_key`] mixed with
    /// nothing else; the prefix is not part of it).
    pub key: u64,
    /// `true` when the transform is the identity: the host may skip it.
    pub is_identity: bool,
}

impl GpuSnippet {
    /// Name of the RGB entry point (`{prefix}_apply`).
    pub fn entry(&self) -> String {
        format!("{}_apply", self.prefix)
    }

    /// Name of the premultiplied RGBA entry point (`{prefix}_apply_premul`).
    pub fn entry_premul(&self) -> String {
        format!("{}_apply_premul", self.prefix)
    }

    /// Name of the uniform struct (`{prefix}_Params`).
    pub fn params_type(&self) -> String {
        format!("{}_Params", self.prefix)
    }

    /// Uniform bytes (little-endian f32, 16 bytes per element).
    pub fn params_bytes(&self) -> Vec<u8> {
        self.params
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    /// Size of the uniform buffer in bytes.
    pub fn params_size(&self) -> u64 {
        (self.params.len() * 16) as u64
    }

    /// Builds a call expression, e.g. `call("c", "u_color", &["lut0"])` →
    /// `P_apply(c, u_color, lut0)`; `premul` selects the RGBA entry point.
    pub fn call(&self, color: &str, params: &str, textures: &[&str], premul: bool) -> String {
        let mut s = format!(
            "{}({color}, {params}",
            if premul {
                self.entry_premul()
            } else {
                self.entry()
            }
        );
        for t in textures {
            s.push_str(", ");
            s.push_str(t);
        }
        s.push(')');
        s
    }
}

fn v4(v: [f64; 3], w: f64) -> [f32; 4] {
    [v[0] as f32, v[1] as f32, v[2] as f32, w as f32]
}

/// Formats an f64 as a WGSL float literal (always with `.` or exponent).
fn lit(x: f64) -> String {
    let s = format!("{x:?}");
    if x < 0.0 { format!("({s})") } else { s }
}

struct Gen<'a> {
    p: &'a str,
    params: Vec<[f32; 4]>,
    helpers: String,
    emitted: HashSet<String>,
    body: String,
    tex_params: Vec<String>,
    tex_args: Vec<String>,
    textures: Vec<LutTexture>,
}

impl Gen<'_> {
    /// Reserves `n` vec4 slots, returns the first index.
    fn slots(&mut self, values: &[[f32; 4]]) -> usize {
        let i = self.params.len();
        self.params.extend_from_slice(values);
        i
    }

    fn helper(&mut self, name: &str, code: impl FnOnce(&str) -> String) -> String {
        let full = format!("{}_{name}", self.p);
        if self.emitted.insert(full.clone()) {
            let src = code(&full);
            self.helpers.push_str(&src);
            self.helpers.push('\n');
        }
        full
    }

    fn op(&mut self, op: &ColorOp) {
        match op {
            ColorOp::Matrix(a) => {
                let rows: Vec<[f32; 4]> = (0..3)
                    .map(|i| {
                        [
                            a.m[i][0] as f32,
                            a.m[i][1] as f32,
                            a.m[i][2] as f32,
                            a.offset[i] as f32,
                        ]
                    })
                    .collect();
                let i = self.slots(&rows);
                let _ = writeln!(
                    self.body,
                    "    c = vec3<f32>(dot(p.v[{i}].xyz, c) + p.v[{i}].w, dot(p.v[{j}].xyz, c) + p.v[{j}].w, dot(p.v[{k}].xyz, c) + p.v[{k}].w);",
                    j = i + 1,
                    k = i + 2
                );
            }
            ColorOp::Decode(tf) | ColorOp::Encode(tf) => {
                let decode = matches!(op, ColorOp::Decode(_));
                let dir = if decode { "decode" } else { "encode" };
                let name = format!("{}_{dir}", tf.id());
                let tf = *tf;
                let f = self.helper(&name, |full| curve_fn(full, tf, decode));
                if let TransferFunction::Gamma(g) = tf {
                    let e = if decode { g } else { 1.0 / g };
                    let i = self.slots(&[[e as f32, 0.0, 0.0, 0.0]]);
                    let _ = writeln!(self.body, "    c = {f}(c, p.v[{i}].x);");
                } else {
                    let _ = writeln!(self.body, "    c = {f}(c);");
                }
            }
            ColorOp::Clamp { min, max } => {
                let i = self.slots(&[[*min as f32, *max as f32, 0.0, 0.0]]);
                let _ = writeln!(
                    self.body,
                    "    c = clamp(c, vec3<f32>(p.v[{i}].x), vec3<f32>(p.v[{i}].y));"
                );
            }
            ColorOp::Lut1d(l) => {
                let f = self.helper("lut1d", lut1d_fn);
                let t = self.textures.len();
                let tex = LutTexture::from_lut1d(l);
                let i = self.slots(&[
                    [l.entries.len() as f32, tex.width as f32, 0.0, 0.0],
                    [l.domain_min[0], l.domain_min[1], l.domain_min[2], 0.0],
                    [l.domain_max[0], l.domain_max[1], l.domain_max[2], 0.0],
                ]);
                self.textures.push(tex);
                self.tex_params.push(format!("t{t}: texture_2d<f32>"));
                self.tex_args.push(format!("t{t}"));
                let _ = writeln!(
                    self.body,
                    "    c = {f}(t{t}, c, p.v[{i}].x, p.v[{i}].y, p.v[{j}].xyz, p.v[{k}].xyz);",
                    j = i + 1,
                    k = i + 2
                );
            }
            ColorOp::Power(p) => {
                let e = p.exponent;
                let i = self.slots(&[v4(e, 0.0)]);
                let (name, expr) = match p.negative {
                    NegativeStyle::Clamp => ("power_clamp", "pow(max(v, vec3<f32>(0.0)), e)"),
                    NegativeStyle::Mirror => ("power_mirror", "sign(v) * pow(abs(v), e)"),
                    NegativeStyle::PassThru | NegativeStyle::Linear => (
                        "power_pass",
                        "select(v, pow(max(v, vec3<f32>(0.0)), e), v >= vec3<f32>(0.0))",
                    ),
                };
                let f = self.helper(name, |n| format!("fn {n}(v: vec3<f32>, e: vec3<f32>) -> vec3<f32> {{\n    return {expr};\n}}\n"));
                let _ = writeln!(self.body, "    c = {f}(c, p.v[{i}].xyz);");
            }
            ColorOp::Moncurve(m) => {
                let seg: Vec<(f64, f64)> = (0..3).map(|c| m.segment(c)).collect();
                let i = self.slots(&[
                    v4(m.gamma, 0.0),
                    v4(m.offset, 0.0),
                    v4([seg[0].0, seg[1].0, seg[2].0], 0.0),
                    v4([seg[0].1, seg[1].1, seg[2].1], 0.0),
                ]);
                let mirror = m.negative == NegativeStyle::Mirror;
                let name = format!(
                    "moncurve_{}{}",
                    if m.inverse { "inv" } else { "fwd" },
                    if mirror { "_mirror" } else { "" }
                );
                let inverse = m.inverse;
                let f = self.helper(&name, |n| {
                    let power = if inverse {
                        "(1.0 + o) * pow(max(x, vec3<f32>(0.0)), 1.0 / g) - o"
                    } else {
                        "pow((x + o) / (1.0 + o), g)"
                    };
                    let (pre, post) = if mirror { ("abs(v)", "sign(v) * r") } else { ("v", "r") };
                    format!(
                        "fn {n}(v: vec3<f32>, g: vec3<f32>, o: vec3<f32>, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {{\n    let x = {pre};\n    let r = select({power}, x * s, x < b);\n    return {post};\n}}\n"
                    )
                });
                let _ = writeln!(
                    self.body,
                    "    c = {f}(c, p.v[{i}].xyz, p.v[{}].xyz, p.v[{}].xyz, p.v[{}].xyz);",
                    i + 1,
                    i + 2,
                    i + 3
                );
            }
            ColorOp::Log(l) => {
                let ln_b = l.base.ln();
                let k = l.log_slope.map(|s| s / ln_b);
                let mut brk = [-1e38; 3];
                let mut ts = [0.0; 3];
                let mut to = [0.0; 3];
                for c in 0..3 {
                    if let Some((lin_brk, log_brk, slope, off)) = l.toe(c) {
                        brk[c] = if l.inverse { log_brk } else { lin_brk };
                        ts[c] = slope;
                        to[c] = off;
                    }
                }
                let i = self.slots(&[
                    v4(k, 0.0),
                    v4(l.log_offset, 0.0),
                    v4(l.lin_slope, 0.0),
                    v4(l.lin_offset, 0.0),
                    v4(brk, 0.0),
                    v4(ts, 0.0),
                    v4(to, 0.0),
                ]);
                let inverse = l.inverse;
                let f = self.helper(if inverse { "log_inv" } else { "log_fwd" }, |n| {
                    let body = if inverse {
                        "    let a = (exp((v - q_lo) / q_k) - q_io) / q_is;\n    return select(a, (v - q_to) / q_ts, v <= q_b);\n".to_string()
                    } else {
                        format!(
                            "    let a = q_k * log(max(q_is * v + q_io, vec3<f32>({}))) + q_lo;\n    return select(a, q_ts * v + q_to, v <= q_b);\n",
                            lit(FLT_MIN)
                        )
                    };
                    format!(
                        "fn {n}(v: vec3<f32>, q_k: vec3<f32>, q_lo: vec3<f32>, q_is: vec3<f32>, q_io: vec3<f32>, q_b: vec3<f32>, q_ts: vec3<f32>, q_to: vec3<f32>) -> vec3<f32> {{\n{body}}}\n"
                    )
                });
                let args: Vec<String> = (0..7).map(|j| format!("p.v[{}].xyz", i + j)).collect();
                let _ = writeln!(self.body, "    c = {f}(c, {});", args.join(", "));
            }
            ColorOp::Cdl(cdl) => {
                let (power, sat) = if cdl.inverse {
                    (cdl.power.map(|p| 1.0 / p), 1.0 / cdl.saturation)
                } else {
                    (cdl.power, cdl.saturation)
                };
                let i = self.slots(&[v4(cdl.slope, sat), v4(cdl.offset, 0.0), v4(power, 0.0)]);
                let clamp = cdl.clamp;
                let inverse = cdl.inverse;
                let name = format!(
                    "cdl_{}{}",
                    if inverse { "inv" } else { "fwd" },
                    if clamp { "" } else { "_noclamp" }
                );
                let f = self.helper(&name, |n| {
                    let c01 = |e: &str| if clamp { format!("clamp({e}, vec3<f32>(0.0), vec3<f32>(1.0))") } else { e.to_string() };
                    let luma = "vec3<f32>(0.2126, 0.7152, 0.0722)";
                    let body = if inverse {
                        format!(
                            "    var x = {};\n    let l = dot(x, {luma});\n    x = {};\n    x = select(x, pow(max(x, vec3<f32>(0.0)), q_pw), x > vec3<f32>(0.0));\n    return (x - q_of) / q_sl;\n",
                            c01("v"),
                            c01("l + q_sat * (x - l)")
                        )
                    } else {
                        format!(
                            "    var x = {};\n    x = select(x, pow(max(x, vec3<f32>(0.0)), q_pw), x > vec3<f32>(0.0));\n    let l = dot(x, {luma});\n    return {};\n",
                            c01("v * q_sl + q_of"),
                            c01("l + q_sat * (x - l)")
                        )
                    };
                    format!("fn {n}(v: vec3<f32>, q_sl: vec3<f32>, q_of: vec3<f32>, q_pw: vec3<f32>, q_sat: f32) -> vec3<f32> {{\n{body}}}\n")
                });
                let _ = writeln!(
                    self.body,
                    "    c = {f}(c, p.v[{i}].xyz, p.v[{}].xyz, p.v[{}].xyz, p.v[{i}].w);",
                    i + 1,
                    i + 2
                );
            }
            ColorOp::Lut3d(l) => {
                let f = match l.interpolation {
                    Interpolation::Linear => self.helper("lut3d_linear", |n| lut3d_fn(n, false)),
                    Interpolation::Tetrahedral => self.helper("lut3d_tetra", |n| lut3d_fn(n, true)),
                };
                let t = self.textures.len();
                let i = self.slots(&[
                    [l.size as f32, 0.0, 0.0, 0.0],
                    [l.domain_min[0], l.domain_min[1], l.domain_min[2], 0.0],
                    [l.domain_max[0], l.domain_max[1], l.domain_max[2], 0.0],
                ]);
                self.textures.push(LutTexture::from_lut3d(l));
                self.tex_params.push(format!("t{t}: texture_3d<f32>"));
                self.tex_args.push(format!("t{t}"));
                let _ = writeln!(
                    self.body,
                    "    c = {f}(t{t}, c, p.v[{i}].x, p.v[{j}].xyz, p.v[{k}].xyz);",
                    j = i + 1,
                    k = i + 2
                );
            }
        }
    }
}

impl ColorTransform {
    /// Generates the WGSL snippet. `prefix` must be a valid WGSL identifier
    /// start (letters, digits, `_`; not starting with a digit or `__`).
    pub fn to_wgsl(&self, prefix: &str) -> GpuSnippet {
        let mut g = Gen {
            p: prefix,
            params: Vec::new(),
            helpers: String::new(),
            emitted: HashSet::new(),
            body: String::new(),
            tex_params: Vec::new(),
            tex_args: Vec::new(),
            textures: Vec::new(),
        };
        for op in self.ops() {
            g.op(op);
        }
        if g.params.is_empty() {
            g.params.push([0.0; 4]);
        }
        let n = g.params.len();
        let mut src = String::new();
        let _ = writeln!(
            src,
            "struct {prefix}_Params {{ v: array<vec4<f32>, {n}>, }}\n"
        );
        src.push_str(&g.helpers);
        let tex_decl: String = g.tex_params.iter().map(|t| format!(", {t}")).collect();
        let tex_args: String = g.tex_args.iter().map(|t| format!(", {t}")).collect();
        let _ = writeln!(
            src,
            "fn {prefix}_apply(c_in: vec3<f32>, p: {prefix}_Params{tex_decl}) -> vec3<f32> {{\n    var c = c_in;\n{}    return c;\n}}\n",
            g.body
        );
        if self.is_linear() {
            let _ = writeln!(
                src,
                "fn {prefix}_apply_premul(c: vec4<f32>, p: {prefix}_Params{tex_decl}) -> vec4<f32> {{\n    return vec4<f32>({prefix}_apply(c.rgb, p{tex_args}), c.a);\n}}"
            );
        } else {
            let _ = writeln!(
                src,
                "fn {prefix}_apply_premul(c: vec4<f32>, p: {prefix}_Params{tex_decl}) -> vec4<f32> {{\n    if (c.a <= 0.0) {{ return c; }}\n    return vec4<f32>({prefix}_apply(c.rgb / c.a, p{tex_args}) * c.a, c.a);\n}}"
            );
        }
        let mut h = FxHasher::default();
        self.structure_key().hash(&mut h);
        n.hash(&mut h);
        GpuSnippet {
            prefix: prefix.to_string(),
            wgsl: src,
            params: g.params,
            textures: g.textures,
            key: h.finish(),
            is_identity: self.is_identity(),
        }
    }
}

/// WGSL for one curve direction, vectorized over RGB with `select`.
fn curve_fn(name: &str, tf: TransferFunction, decode: bool) -> String {
    use TransferFunction as T;
    let l = lit;
    let v3 = |x: f64| format!("vec3<f32>({})", lit(x));
    // Odd-mirrored curves: compute on |v| and restore the sign.
    let mirrored = |body: String| {
        format!(
            "fn {name}(v: vec3<f32>) -> vec3<f32> {{\n    let a = abs(v);\n{body}    return sign(v) * r;\n}}\n"
        )
    };
    let plain = |body: String| format!("fn {name}(v: vec3<f32>) -> vec3<f32> {{\n{body}}}\n");
    match (tf, decode) {
        (T::Linear, _) => plain("    return v;\n".into()),
        (T::Srgb, true) => mirrored(format!(
            "    let r = select(pow((a + 0.055) / 1.055, vec3<f32>(2.4)), a / 12.92, a <= {});\n",
            v3(k::SRGB_DECODE_CUT)
        )),
        (T::Srgb, false) => mirrored(format!(
            "    let r = select(1.055 * pow(a, vec3<f32>(1.0 / 2.4)) - 0.055, a * 12.92, a <= {});\n",
            v3(k::SRGB_ENCODE_CUT)
        )),
        (T::Rec709, true) => mirrored(format!(
            "    let r = select(pow((a + {am1}) / {a}, vec3<f32>(1.0 / 0.45)), a / 4.5, a < {cut});\n",
            am1 = l(k::REC709_A - 1.0),
            a = l(k::REC709_A),
            cut = v3(4.5 * k::REC709_B)
        )),
        (T::Rec709, false) => mirrored(format!(
            "    let r = select({a} * pow(a, vec3<f32>(0.45)) - {am1}, a * 4.5, a < {cut});\n",
            am1 = l(k::REC709_A - 1.0),
            a = l(k::REC709_A),
            cut = v3(k::REC709_B)
        )),
        (T::Gamma(_), _) => format!(
            "fn {name}(v: vec3<f32>, e: f32) -> vec3<f32> {{\n    return sign(v) * pow(abs(v), vec3<f32>(e));\n}}\n"
        ),
        (T::Pq, true) => mirrored(format!(
            "    let e = pow(a, vec3<f32>({inv_m2}));\n    let r = pow(max(e - {c1}, vec3<f32>(0.0)) / ({c2} - {c3} * e), vec3<f32>({inv_m1})) * {scale};\n",
            inv_m2 = l(1.0 / k::PQ_M2),
            inv_m1 = l(1.0 / k::PQ_M1),
            c1 = l(k::PQ_C1),
            c2 = l(k::PQ_C2),
            c3 = l(k::PQ_C3),
            scale = l(k::PQ_SCALE)
        )),
        (T::Pq, false) => mirrored(format!(
            "    let y = pow(a / {scale}, vec3<f32>({m1}));\n    let r = pow(({c1} + {c2} * y) / (1.0 + {c3} * y), vec3<f32>({m2}));\n",
            m1 = l(k::PQ_M1),
            m2 = l(k::PQ_M2),
            c1 = l(k::PQ_C1),
            c2 = l(k::PQ_C2),
            c3 = l(k::PQ_C3),
            scale = l(k::PQ_SCALE)
        )),
        (T::Hlg, true) => mirrored(format!(
            "    let r = select((exp((a - {c}) / {ha}) + {hb}) / 12.0, a * a / 3.0, a <= vec3<f32>(0.5));\n",
            c = l(k::hlg_c()),
            ha = l(k::HLG_A),
            hb = l(k::HLG_B)
        )),
        (T::Hlg, false) => mirrored(format!(
            "    let r = select({ha} * log(max(12.0 * a - {hb}, vec3<f32>(1e-6))) + {c}, sqrt(3.0 * a), a <= vec3<f32>(1.0 / 12.0));\n",
            c = l(k::hlg_c()),
            ha = l(k::HLG_A),
            hb = l(k::HLG_B)
        )),
        (T::AcesCc, true) => plain(format!(
            "    let p = exp2(v * 17.52 - 9.72);\n    let lo = (p - {h16}) * 2.0;\n    return select(select(vec3<f32>({max}), p, v < {hi}), lo, v < {lo_cut});\n",
            h16 = l(2f64.powi(-16)),
            max = l(k::ACES_HALF_MAX),
            hi = v3((k::ACES_HALF_MAX.log2() + 9.72) / 17.52),
            lo_cut = v3((9.72 - 15.0) / 17.52)
        )),
        (T::AcesCc, false) => plain(format!(
            "    let tiny = (log2({h16} + max(v, vec3<f32>(0.0)) * 0.5) + 9.72) / 17.52;\n    let big = (log2(max(v, vec3<f32>({h15}))) + 9.72) / 17.52;\n    return select(select(big, tiny, v < vec3<f32>({h15})), vec3<f32>({floor}), v <= vec3<f32>(0.0));\n",
            h16 = l(2f64.powi(-16)),
            h15 = l(2f64.powi(-15)),
            floor = l((-16.0 + 9.72) / 17.52)
        )),
        (T::AcesCct, true) => plain(format!(
            "    return select(exp2(v * 17.52 - 9.72), (v - {b}) / {a}, v <= {brk});\n",
            a = l(k::ACESCCT_A),
            b = l(k::ACESCCT_B),
            brk = v3(k::ACESCCT_Y_BRK)
        )),
        (T::AcesCct, false) => plain(format!(
            "    return select((log2(max(v, vec3<f32>(1e-30))) + 9.72) / 17.52, {a} * v + {b}, v <= {brk});\n",
            a = l(k::ACESCCT_A),
            b = l(k::ACESCCT_B),
            brk = v3(k::ACESCCT_X_BRK)
        )),
        (T::SLog3, true) => plain(format!(
            "    let hi = exp2(((v * 1023.0 - 420.0) / 261.5) * {log2_10}) * 0.19 - 0.01;\n    let lo = (v * 1023.0 - 95.0) * {cut_lin} / {span};\n    return select(hi, lo, v < {cut});\n",
            log2_10 = l(std::f64::consts::LOG2_10),
            cut_lin = l(k::SLOG3_CUT_LIN),
            span = l(k::SLOG3_CUT_CV - 95.0),
            cut = v3(k::SLOG3_CUT_CV / 1023.0)
        )),
        (T::SLog3, false) => plain(format!(
            "    let hi = (420.0 + log2(max((v + 0.01) / 0.19, vec3<f32>(1e-30))) * {log10_2} * 261.5) / 1023.0;\n    let lo = (v * {span} / {cut_lin} + 95.0) / 1023.0;\n    return select(hi, lo, v < {cut});\n",
            log10_2 = l(std::f64::consts::LOG10_2),
            cut_lin = l(k::SLOG3_CUT_LIN),
            span = l(k::SLOG3_CUT_CV - 95.0),
            cut = v3(k::SLOG3_CUT_LIN)
        )),
        (T::LogC3, true) => plain(format!(
            "    let hi = (exp2(((v - {d}) / {c}) * {log2_10}) - {b}) / {a};\n    let lo = (v - {f}) / {e};\n    return select(lo, hi, v > {cut});\n",
            a = l(k::LOGC3_A),
            b = l(k::LOGC3_B),
            c = l(k::LOGC3_C),
            d = l(k::LOGC3_D),
            e = l(k::LOGC3_E),
            f = l(k::LOGC3_F),
            log2_10 = l(std::f64::consts::LOG2_10),
            cut = v3(k::LOGC3_E * k::LOGC3_CUT + k::LOGC3_F)
        )),
        (T::LogC3, false) => plain(format!(
            "    let hi = {c} * log2(max({a} * v + {b}, vec3<f32>(1e-30))) * {log10_2} + {d};\n    let lo = {e} * v + {f};\n    return select(lo, hi, v > {cut});\n",
            a = l(k::LOGC3_A),
            b = l(k::LOGC3_B),
            c = l(k::LOGC3_C),
            d = l(k::LOGC3_D),
            e = l(k::LOGC3_E),
            f = l(k::LOGC3_F),
            log10_2 = l(std::f64::consts::LOG10_2),
            cut = v3(k::LOGC3_CUT)
        )),
        (T::LogC4, true) => {
            let (a, b, c, s, t) = k::logc4();
            plain(format!(
                "    let hi = (exp2(14.0 * (v - {c}) / {b} + 6.0) - 64.0) / {a};\n    let lo = v * {s} + {t};\n    return select(lo, hi, v >= vec3<f32>(0.0));\n",
                a = l(a),
                b = l(b),
                c = l(c),
                s = l(s),
                t = l(t)
            ))
        }
        (T::LogC4, false) => {
            let (a, b, c, s, t) = k::logc4();
            plain(format!(
                "    let hi = (log2(max({a} * v + 64.0, vec3<f32>(1e-30))) - 6.0) / 14.0 * {b} + {c};\n    let lo = (v - {t}) / {s};\n    return select(lo, hi, v >= {tv});\n",
                a = l(a),
                b = l(b),
                c = l(c),
                s = l(s),
                t = l(t),
                tv = v3(t)
            ))
        }
        (T::VLog, true) => plain(format!(
            "    let hi = exp2(((v - {d}) / {c}) * {log2_10}) - {b};\n    let lo = (v - 0.125) / 5.6;\n    return select(hi, lo, v < {cut});\n",
            b = l(k::VLOG_B),
            c = l(k::VLOG_C),
            d = l(k::VLOG_D),
            log2_10 = l(std::f64::consts::LOG2_10),
            cut = v3(k::VLOG_CUT2)
        )),
        (T::VLog, false) => plain(format!(
            "    let hi = {c} * log2(max(v + {b}, vec3<f32>(1e-30))) * {log10_2} + {d};\n    let lo = 5.6 * v + 0.125;\n    return select(hi, lo, v < {cut});\n",
            b = l(k::VLOG_B),
            c = l(k::VLOG_C),
            d = l(k::VLOG_D),
            log10_2 = l(std::f64::consts::LOG10_2),
            cut = v3(k::VLOG_CUT1)
        )),
        (T::CanonLog3, true) => plain(format!(
            "    let c = v;\n    let neg = -(exp2(((({on}) - c) / {s}) * {log2_10}) - 1.0) / {sc};\n    let mid = (c - {lo}) / {ls};\n    let pos = (exp2(((c - {op}) / {s}) * {log2_10}) - 1.0) / {sc};\n    return select(select(pos, mid, c <= {cp}), neg, c < {cn}) * {refl};\n",
            on = l(k::CLOG3_OFF_NEG),
            op = l(k::CLOG3_OFF_POS),
            s = l(k::CLOG3_SLOPE),
            sc = l(k::CLOG3_SCALE),
            lo = l(k::CLOG3_LIN_OFF),
            ls = l(k::CLOG3_LIN_SLOPE),
            cp = v3(k::CLOG3_CUT_POS),
            cn = v3(k::CLOG3_CUT_NEG),
            refl = l(k::CLOG3_REFLECTION),
            log2_10 = l(std::f64::consts::LOG2_10)
        )),
        (T::CanonLog3, false) => plain(format!(
            "    let x = v / {refl};\n    let neg = -{s} * log2(max(-x * {sc} + 1.0, vec3<f32>(1e-30))) * {log10_2} + {on};\n    let mid = {ls} * x + {lo};\n    let pos = {s} * log2(max(x * {sc} + 1.0, vec3<f32>(1e-30))) * {log10_2} + {op};\n    return select(select(pos, mid, x <= {lc}), neg, x < {nlc});\n",
            on = l(k::CLOG3_OFF_NEG),
            op = l(k::CLOG3_OFF_POS),
            s = l(k::CLOG3_SLOPE),
            sc = l(k::CLOG3_SCALE),
            lo = l(k::CLOG3_LIN_OFF),
            ls = l(k::CLOG3_LIN_SLOPE),
            lc = v3(k::CLOG3_LIN_CUT_POS),
            nlc = v3(k::CLOG3_LIN_CUT_NEG),
            refl = l(k::CLOG3_REFLECTION),
            log10_2 = l(std::f64::consts::LOG10_2)
        )),
    }
}

fn lut1d_fn(name: &str) -> String {
    format!(
        "fn {name}_at(t: texture_2d<f32>, i: i32, w: i32) -> vec4<f32> {{
    return textureLoad(t, vec2<i32>(i % w, i / w), 0);
}}
fn {name}(t: texture_2d<f32>, c: vec3<f32>, n: f32, w: f32, dmin: vec3<f32>, dmax: vec3<f32>) -> vec3<f32> {{
    let x = clamp((c - dmin) / (dmax - dmin), vec3<f32>(0.0), vec3<f32>(1.0)) * (n - 1.0);
    let i0 = min(floor(x), vec3<f32>(n - 2.0));
    let f = x - i0;
    let wi = i32(w);
    let i = vec3<i32>(i0);
    let r = mix({name}_at(t, i.x, wi).x, {name}_at(t, i.x + 1, wi).x, f.x);
    let g = mix({name}_at(t, i.y, wi).y, {name}_at(t, i.y + 1, wi).y, f.y);
    let b = mix({name}_at(t, i.z, wi).z, {name}_at(t, i.z + 1, wi).z, f.z);
    return vec3<f32>(r, g, b);
}}
"
    )
}

fn lut3d_fn(name: &str, tetrahedral: bool) -> String {
    let head = format!(
        "fn {name}(t: texture_3d<f32>, c: vec3<f32>, n: f32, dmin: vec3<f32>, dmax: vec3<f32>) -> vec3<f32> {{
    let x = clamp((c - dmin) / (dmax - dmin), vec3<f32>(0.0), vec3<f32>(1.0)) * (n - 1.0);
    let i0 = min(floor(x), vec3<f32>(n - 2.0));
    let f = x - i0;
    let b = vec3<i32>(i0);
    let c000 = textureLoad(t, b, 0).xyz;
    let c111 = textureLoad(t, b + vec3<i32>(1, 1, 1), 0).xyz;
"
    );
    let body = if tetrahedral {
        // Same tetrahedron choice (strict comparisons, same order) as Lut3d::eval.
        "    let c100 = textureLoad(t, b + vec3<i32>(1, 0, 0), 0).xyz;
    let c010 = textureLoad(t, b + vec3<i32>(0, 1, 0), 0).xyz;
    let c001 = textureLoad(t, b + vec3<i32>(0, 0, 1), 0).xyz;
    let c110 = textureLoad(t, b + vec3<i32>(1, 1, 0), 0).xyz;
    let c101 = textureLoad(t, b + vec3<i32>(1, 0, 1), 0).xyz;
    let c011 = textureLoad(t, b + vec3<i32>(0, 1, 1), 0).xyz;
    var a: vec3<f32>; var bb: vec3<f32>; var w: vec3<f32>;
    if (f.x > f.y) {
        if (f.y > f.z) { a = c100; bb = c110; w = vec3<f32>(f.x, f.y, f.z); }
        else if (f.x > f.z) { a = c100; bb = c101; w = vec3<f32>(f.x, f.z, f.y); }
        else { a = c001; bb = c101; w = vec3<f32>(f.z, f.x, f.y); }
    } else if (f.z > f.y) { a = c001; bb = c011; w = vec3<f32>(f.z, f.y, f.x); }
    else if (f.z > f.x) { a = c010; bb = c011; w = vec3<f32>(f.y, f.z, f.x); }
    else { a = c010; bb = c110; w = vec3<f32>(f.y, f.x, f.z); }
    return c000 + w.x * (a - c000) + w.y * (bb - a) + w.z * (c111 - bb);
}
"
    } else {
        "    let c100 = textureLoad(t, b + vec3<i32>(1, 0, 0), 0).xyz;
    let c010 = textureLoad(t, b + vec3<i32>(0, 1, 0), 0).xyz;
    let c110 = textureLoad(t, b + vec3<i32>(1, 1, 0), 0).xyz;
    let c001 = textureLoad(t, b + vec3<i32>(0, 0, 1), 0).xyz;
    let c101 = textureLoad(t, b + vec3<i32>(1, 0, 1), 0).xyz;
    let c011 = textureLoad(t, b + vec3<i32>(0, 1, 1), 0).xyz;
    let y0 = mix(mix(c000, c100, f.x), mix(c010, c110, f.x), f.y);
    let y1 = mix(mix(c001, c101, f.x), mix(c011, c111, f.x), f.y);
    return mix(y0, y1, f.z);
}
"
    };
    head + body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::builtin;

    #[test]
    fn identity_snippet_is_valid_shape() {
        let s = ColorTransform::identity().to_wgsl("cc");
        assert!(s.is_identity);
        assert_eq!(s.params.len(), 1);
        assert!(
            s.wgsl
                .contains("fn cc_apply(c_in: vec3<f32>, p: cc_Params)")
        );
        assert_eq!(s.call("x", "u", &[], true), "cc_apply_premul(x, u)");
    }

    #[test]
    fn helpers_are_emitted_once_and_prefixed() {
        let t = builtin::SLOG3_SGAMUT3.transform_to(&builtin::SRGB).unwrap();
        let t = t.then(&builtin::SRGB.transform_to(&builtin::REC709).unwrap());
        let s = t.to_wgsl("in0");
        assert_eq!(s.wgsl.matches("fn in0_slog3_decode").count(), 1);
        for line in s
            .wgsl
            .lines()
            .filter(|l| l.starts_with("fn ") || l.starts_with("struct "))
        {
            let name = line.split_whitespace().nth(1).unwrap();
            assert!(name.starts_with("in0_"), "{line}");
        }
    }

    #[test]
    fn same_structure_same_wgsl() {
        let a = builtin::DISPLAY_P3
            .transform_to(&builtin::ACESCG)
            .unwrap()
            .to_wgsl("p");
        let b = builtin::SRGB
            .transform_to(&builtin::ACESCG)
            .unwrap()
            .to_wgsl("p");
        assert_eq!(a.key, b.key);
        assert_eq!(a.wgsl, b.wgsl);
        assert_ne!(a.params, b.params);
    }
}
