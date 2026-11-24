// Blur kernels shared by Gaussian Blur, Fast Box Blur, Glow, Drop Shadow and
// Fill (feather). All images are premultiplied working-format textures; every
// coordinate below is in the image space of the call (render pixels for the
// effect input/output, level texels for pyramid levels), and each texture's
// texel (0, 0) sits at its `*_origin`.
//
// Addressing (`clamp_mode`): 0 = transparent outside `src_rect` (AE default),
// 1 = clamp to `src_rect` (AE "Repeat Edge Pixels").
//
// Linear light (`load_gamma`, `store_gamma`; 1 = off): the first step raises
// the straight colour of what it reads to `load_gamma`, the last step writes
// its result raised to `store_gamma` (AE's Gaussian Blur blurs in linear light
// in non-linear projects).

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;

struct Params {
    src_rect: vec4<i32>,
    dst_rect: vec4<i32>,
    src_origin: vec2<i32>,
    dst_origin: vec2<i32>,
    // Pyramid grid: level texel t covers [base + t * factor, base + (t + 1) * factor).
    base: vec2<i32>,
    factor: vec2<i32>,
    axis: u32,
    clamp_mode: u32,
    radius: i32,
    sigma: f32,
    // Box blur: fractional weight of the outermost taps, run length per thread.
    frac: f32,
    seg: i32,
    load_gamma: f32,
    store_gamma: f32,
    // conv_line: normalised weights by offset, symmetric and zero padded:
    // weights[WPAD + d] for d in -WPAD..=WPAD (0 where |d| > radius).
    // `weights` is read as a flat f32 table of 60 entries.
    weights: array<vec4<f32>, 15>,
}

// Premultiplied colour with its straight colour raised to `g` (sign kept).
fn regamma(c: vec4<f32>, g: f32) -> vec4<f32> {
    if (g == 1.0 || c.a <= 0.0) {
        return c;
    }
    let s = c.rgb / c.a;
    return vec4<f32>(sign(s) * pow(abs(s), vec3<f32>(g)) * c.a, c.a);
}

// Source texel `q` (texture coordinates), in the working light.
fn texel(q: vec2<i32>) -> vec4<f32> {
    return regamma(textureLoad(src, q, 0), u.load_gamma);
}

fn load_src(p: vec2<i32>) -> vec4<f32> {
    var q = p;
    if (u.clamp_mode == 1u) {
        q = clamp(p, u.src_rect.xy, u.src_rect.zw - vec2<i32>(1));
    } else if (any(p < u.src_rect.xy) || any(p >= u.src_rect.zw)) {
        return vec4<f32>(0.0);
    }
    return texel(q - u.src_origin);
}

fn store_dst(p: vec2<i32>, c: vec4<f32>) {
    textureStore(dst, p - u.dst_origin, regamma(c, u.store_gamma));
}

fn axis_step() -> vec2<i32> {
    return select(vec2<i32>(0, 1), vec2<i32>(1, 0), u.axis == 0u);
}

// ---- Separable convolution along one axis (Gaussian, or box up to MAX_TAPS) --
//
// A workgroup computes a tile of TA outputs along the axis × TC lines across
// it; the TA + 2R source texels of each line are loaded once into workgroup
// memory. Each invocation computes OUT consecutive outputs, so every cached
// texel read feeds OUT accumulators (the tap loop is bound by workgroup-memory
// reads otherwise). Weights (normalised) come from the uniform and are laid
// out symmetrically, zero padded, so the loop has no branches.

const OUT: i32 = 4;
const TA: i32 = 128; // 32 invocations × OUT
const TC: i32 = 4;
const MAX_TAPS: i32 = 24;
const LINE: i32 = 176; // TA + 2 * MAX_TAPS
const WPAD: i32 = 28;  // MAX_TAPS + OUT: offset of tap 0 in line_weights

var<workgroup> line_cache: array<vec4<f32>, 704>; // TC * LINE
var<workgroup> line_weights: array<f32, 60>;      // 2 * WPAD + 1, rounded up

@compute @workgroup_size(32, 4, 1)
fn conv_line(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let r = clamp(u.radius, 0, MAX_TAPS);
    let la = i32(lid.x);
    let lc = i32(lid.y);
    let along0 = i32(wg.x) * TA;
    let across = i32(wg.y) * TC + lc;
    var origin: vec2<i32>;
    var step: vec2<i32>;
    if (u.axis == 0u) {
        origin = u.dst_rect.xy + vec2<i32>(along0, across);
        step = vec2<i32>(1, 0);
    } else {
        origin = u.dst_rect.xy + vec2<i32>(across, along0);
        step = vec2<i32>(0, 1);
    }
    let row = lc * LINE;
    for (var k = la; k < TA + 2 * r; k = k + 32) {
        line_cache[row + k] = load_src(origin + step * (k - r));
    }
    // The symmetric weight table into workgroup memory (read as a broadcast in
    // the tap loop; reading it from the uniform with a computed index is slower).
    let li = lc * 32 + la;
    if (li < 60) {
        line_weights[li] = u.weights[li / 4][li % 4];
    }
    workgroupBarrier();
    // Outputs a0 .. a0 + OUT - 1; output j reads cache entries base + j .. base + j + 2r.
    let a0 = la * OUT;
    let base = row + a0;
    var acc0 = vec4<f32>(0.0);
    var acc1 = vec4<f32>(0.0);
    var acc2 = vec4<f32>(0.0);
    var acc3 = vec4<f32>(0.0);
    for (var t = 0; t < 2 * r + OUT; t = t + 1) {
        let v = line_cache[base + t];
        // Offset of entry t for output j: t - j - r.
        let o = WPAD + t - r;
        acc0 = acc0 + v * line_weights[o];
        acc1 = acc1 + v * line_weights[o - 1];
        acc2 = acc2 + v * line_weights[o - 2];
        acc3 = acc3 + v * line_weights[o - 3];
    }
    let p = origin + step * a0;
    let end = u.dst_rect.zw;
    if (any(p >= end)) {
        return;
    }
    store_dst(p, acc0);
    if (any(p + step >= end)) {
        return;
    }
    store_dst(p + step, acc1);
    if (any(p + step * 2 >= end)) {
        return;
    }
    store_dst(p + step * 2, acc2);
    if (any(p + step * 3 >= end)) {
        return;
    }
    store_dst(p + step * 3, acc3);
}

// ---- Block-average downsample -------------------------------------------------
//
// Destination texel t (level coordinates) = mean of the source block
// [base + t * factor, base + (t + 1) * factor).

@compute @workgroup_size(8, 8, 1)
fn downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = u.dst_rect.xy + vec2<i32>(id.xy);
    if (any(t >= u.dst_rect.zw)) {
        return;
    }
    let p0 = u.base + t * u.factor;
    var acc = vec4<f32>(0.0);
    if (all(p0 >= u.src_rect.xy) && all(p0 + u.factor <= u.src_rect.zw)) {
        // Block inside the source: plain loads, no addressing per texel.
        let q0 = p0 - u.src_origin;
        if (u.factor.x % 4 == 0) {
            // Four independent loads per step so they are issued together.
            for (var y = 0; y < u.factor.y; y = y + 1) {
                for (var x = 0; x < u.factor.x; x = x + 4) {
                    let q = q0 + vec2<i32>(x, y);
                    let a = texel(q);
                    let b = texel(q + vec2<i32>(1, 0));
                    let c = texel(q + vec2<i32>(2, 0));
                    let d = texel(q + vec2<i32>(3, 0));
                    acc = acc + ((a + b) + (c + d));
                }
            }
        } else {
            for (var y = 0; y < u.factor.y; y = y + 1) {
                for (var x = 0; x < u.factor.x; x = x + 1) {
                    acc = acc + texel(q0 + vec2<i32>(x, y));
                }
            }
        }
    } else {
        for (var y = 0; y < u.factor.y; y = y + 1) {
            for (var x = 0; x < u.factor.x; x = x + 1) {
                acc = acc + load_src(p0 + vec2<i32>(x, y));
            }
        }
    }
    store_dst(t, acc / f32(u.factor.x * u.factor.y));
}

// ---- Bilinear upsample from a pyramid level -----------------------------------
//
// Output pixel p (centre p + 0.5) reads the level at texel coordinate
// (p + 0.5 - base) / factor - 0.5 (level texel t has its centre at
// base + (t + 0.5) * factor).

@compute @workgroup_size(8, 8, 1)
fn upsample(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = u.dst_rect.xy + vec2<i32>(id.xy);
    if (any(p >= u.dst_rect.zw)) {
        return;
    }
    let f = vec2<f32>(u.factor);
    let q = (vec2<f32>(p - u.base) + vec2<f32>(0.5)) / f - vec2<f32>(0.5);
    let i = vec2<i32>(floor(q));
    let w = q - floor(q);
    var a: vec4<f32>;
    var b: vec4<f32>;
    var c: vec4<f32>;
    var d: vec4<f32>;
    if (all(i >= u.src_rect.xy) && all(i + vec2<i32>(1) < u.src_rect.zw)) {
        let j = i - u.src_origin;
        a = textureLoad(src, j, 0);
        b = textureLoad(src, j + vec2<i32>(1, 0), 0);
        c = textureLoad(src, j + vec2<i32>(0, 1), 0);
        d = textureLoad(src, j + vec2<i32>(1, 1), 0);
    } else {
        a = load_src(i);
        b = load_src(i + vec2<i32>(1, 0));
        c = load_src(i + vec2<i32>(0, 1));
        d = load_src(i + vec2<i32>(1, 1));
    }
    store_dst(p, mix(mix(a, b, w.x), mix(c, d, w.x), w.y));
}

// ---- Box filter along one axis (sliding window) --------------------------------
//
// Width 2 * radius + 1 plus the two outermost taps at weight `frac` (fractional
// radius). Invocation x = the row (axis 0) or column (axis 1); invocation y =
// a run of `seg` consecutive outputs along the axis, computed with a running
// sum (O(1) per output).

@compute @workgroup_size(64, 1, 1)
fn box_line(@builtin(global_invocation_id) id: vec3<u32>) {
    let step = axis_step();
    let across = select(u.dst_rect.x, u.dst_rect.y, u.axis == 0u) + i32(id.x);
    let across_end = select(u.dst_rect.z, u.dst_rect.w, u.axis == 0u);
    if (across >= across_end) {
        return;
    }
    let along0 = select(u.dst_rect.y, u.dst_rect.x, u.axis == 0u) + i32(id.y) * u.seg;
    let along_end = min(select(u.dst_rect.w, u.dst_rect.z, u.axis == 0u), along0 + u.seg);
    if (along0 >= along_end) {
        return;
    }
    var start: vec2<i32>;
    if (u.axis == 0u) {
        start = vec2<i32>(along0, across);
    } else {
        start = vec2<i32>(across, along0);
    }
    let r = u.radius;
    let norm = 1.0 / (f32(2 * r + 1) + 2.0 * u.frac);
    var sum = vec4<f32>(0.0);
    for (var k = -r; k <= r; k = k + 1) {
        sum = sum + load_src(start + step * k);
    }
    var p = start;
    for (var a = along0; a < along_end; a = a + 1) {
        var v = sum;
        if (u.frac > 0.0) {
            v = v + (load_src(p - step * (r + 1)) + load_src(p + step * (r + 1))) * u.frac;
        }
        store_dst(p, v * norm);
        sum = sum + load_src(p + step * (r + 1)) - load_src(p - step * r);
        p = p + step;
    }
}
