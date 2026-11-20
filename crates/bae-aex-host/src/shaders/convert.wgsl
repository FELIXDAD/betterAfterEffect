// BAE <-> AE world conversion on the GPU (bae-aex-host, workstream 20).
//
// encode_*: read a premultiplied RGBA working texture (rgba16float or
//           rgba32float) region and write a packed AE world into a storage
//           buffer whose rows are `row_words` u32 apart. The buffer is then
//           copied into (or *is*) a MAP_READ staging buffer that the plug-in
//           reads in place as its PF_EffectWorld-style input.
// decode_*: the reverse, writing an rgba16float storage texture.
//
// Memory orders (little-endian):
//   argb8   : one u32 per pixel, bytes A,R,G,B   (white = 255)
//   argb16  : two u32 per pixel, u16 A,R | G,B   (white = 32768)
//   argb32f : four u32 per pixel, f32 A,R,G,B
//   bgra128 : four u32 per pixel, f32 B,G,R,A    (AE GPU worlds)
// Encoding matches bae_aex_host::pixel (the CPU reference) bit for bit for
// every format; NaN is replaced by 0 (FloatPolicy::ZeroNan). Decoding into
// rgba16float may differ from the CPU reference by at most 1 half-float ulp
// because the hardware float->half rounding mode of storage-texture writes is
// implementation defined (Metal on Apple GPUs does not round to nearest-even
// for every value); 8 bpc -> half -> 8 bpc still round-trips exactly because
// 1 half ulp is far below half an 8-bit step.
//
// NaN is detected with a bit test, never with `x != x`: shader compilers
// running in fast-math mode (Metal) are allowed to fold `x != x` to false.

struct Params {
    // Top-left of the region inside the texture.
    origin: vec2<i32>,
    // Region size in pixels.
    size: vec2<u32>,
    // Row stride of the packed buffer, in u32 words (rowbytes / 4).
    row_words: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var dst_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<storage, read_write> out_words: array<u32>;
@group(0) @binding(3) var<storage, read> in_words: array<u32>;
@group(0) @binding(4) var<uniform> params: Params;

fn load_px(p: vec2<u32>) -> vec4<f32> {
    return textureLoad(src_tex, vec2<i32>(p) + params.origin, 0);
}

// True where the lane is a NaN (exponent all ones, non-zero mantissa).
fn is_nan4(v: vec4<f32>) -> vec4<bool> {
    return (bitcast<vec4<u32>>(v) & vec4<u32>(0x7fffffffu)) > vec4<u32>(0x7f800000u);
}

fn no_nan(v: vec4<f32>) -> vec4<f32> {
    return select(v, vec4<f32>(0.0), is_nan4(v));
}

fn to16(x: f32) -> u32 {
    // NaN -> 0 like the CPU path (saturating cast of NaN is 0 there).
    let nan = (bitcast<u32>(x) & 0x7fffffffu) > 0x7f800000u;
    let c = select(clamp(x, 0.0, 1.0), 0.0, nan);
    return u32(c * 32768.0 + 0.5);
}

fn from16(w: u32) -> f32 {
    return f32(min(w, 32768u)) * (1.0 / 32768.0);
}

fn in_bounds(p: vec2<u32>) -> bool {
    return p.x < params.size.x && p.y < params.size.y;
}

@compute @workgroup_size(8, 8)
fn encode_argb8(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let c = no_nan(load_px(p));
    let q = vec4<u32>(clamp(vec4<f32>(c.a, c.r, c.g, c.b), vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0 + 0.5);
    out_words[p.y * params.row_words + p.x] = q.x | (q.y << 8u) | (q.z << 16u) | (q.w << 24u);
}

@compute @workgroup_size(8, 8)
fn encode_argb16(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let c = load_px(p);
    let base = p.y * params.row_words + p.x * 2u;
    out_words[base] = to16(c.a) | (to16(c.r) << 16u);
    out_words[base + 1u] = to16(c.g) | (to16(c.b) << 16u);
}

@compute @workgroup_size(8, 8)
fn encode_argb32f(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let c = no_nan(load_px(p));
    let base = p.y * params.row_words + p.x * 4u;
    out_words[base] = bitcast<u32>(c.a);
    out_words[base + 1u] = bitcast<u32>(c.r);
    out_words[base + 2u] = bitcast<u32>(c.g);
    out_words[base + 3u] = bitcast<u32>(c.b);
}

@compute @workgroup_size(8, 8)
fn encode_bgra128(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let c = no_nan(load_px(p));
    let base = p.y * params.row_words + p.x * 4u;
    out_words[base] = bitcast<u32>(c.b);
    out_words[base + 1u] = bitcast<u32>(c.g);
    out_words[base + 2u] = bitcast<u32>(c.r);
    out_words[base + 3u] = bitcast<u32>(c.a);
}

fn store_px(p: vec2<u32>, rgba: vec4<f32>) {
    textureStore(dst_tex, vec2<i32>(p) + params.origin, rgba);
}

@compute @workgroup_size(8, 8)
fn decode_argb8(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let w = in_words[p.y * params.row_words + p.x];
    let argb = vec4<f32>(f32(w & 0xffu), f32((w >> 8u) & 0xffu), f32((w >> 16u) & 0xffu), f32(w >> 24u)) * (1.0 / 255.0);
    store_px(p, vec4<f32>(argb.y, argb.z, argb.w, argb.x));
}

@compute @workgroup_size(8, 8)
fn decode_argb16(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let base = p.y * params.row_words + p.x * 2u;
    let w0 = in_words[base];
    let w1 = in_words[base + 1u];
    store_px(p, vec4<f32>(from16(w0 >> 16u), from16(w1 & 0xffffu), from16(w1 >> 16u), from16(w0 & 0xffffu)));
}

@compute @workgroup_size(8, 8)
fn decode_argb32f(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let base = p.y * params.row_words + p.x * 4u;
    let a = bitcast<f32>(in_words[base]);
    let r = bitcast<f32>(in_words[base + 1u]);
    let g = bitcast<f32>(in_words[base + 2u]);
    let b = bitcast<f32>(in_words[base + 3u]);
    store_px(p, vec4<f32>(r, g, b, a));
}

@compute @workgroup_size(8, 8)
fn decode_bgra128(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = id.xy;
    if (!in_bounds(p)) { return; }
    let base = p.y * params.row_words + p.x * 4u;
    let b = bitcast<f32>(in_words[base]);
    let g = bitcast<f32>(in_words[base + 1u]);
    let r = bitcast<f32>(in_words[base + 2u]);
    let a = bitcast<f32>(in_words[base + 3u]);
    store_px(p, vec4<f32>(r, g, b, a));
}
