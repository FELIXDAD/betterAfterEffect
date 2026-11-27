// Dense optical flow (DIS-style: coarse-to-fine inverse-search patches,
// densified per pixel) and its consumers. Flow is stored in working-format
// textures as two 16-bit fixed-point numbers per texel (render pixels, ±512 at
// 1/64), so every format holds it exactly.

struct Flow {
    img_a: FxImage,
    img_b: FxImage,
    // The flow region (render grid) and the output ROI.
    region: vec4i,
    roi: vec4i,
    // Level size, patch grid size.
    size: vec4u,
    // x: 2^level, y: phase / shutter start, z: shutter span, w: unused.
    data: vec4f,
    // x: reverse (search B → A), y: samples, z: iterations, w: has coarser flow.
    flags: vec4u,
    // xy: the coarser level's size (its flow is in `tex2` while searching).
    coarse: vec4u,
    guide_a: FxImage,
    guide_b: FxImage,
}

@group(0) @binding(0) var<uniform> u: Flow;
@group(0) @binding(1) var tex1: texture_2d<f32>;
@group(0) @binding(2) var tex2: texture_2d<f32>;
@group(0) @binding(3) var tex3: texture_2d<f32>;
@group(0) @binding(4) var tex4: texture_2d<f32>;
@group(0) @binding(5) var out_tex: texture_storage_2d<WORKING_FORMAT, write>;

@group(0) @binding(6) var guide_a: texture_2d<f32>;
@group(0) @binding(7) var guide_b: texture_2d<f32>;

const PATCH: i32 = 8;
fn block_extent() -> i32 { return i32(u.data.w); }
fn stride() -> i32 { return block_extent() / 2; }
fn sample_offset(x: i32, y: i32) -> vec2i {
    return (vec2i(x,y) * (block_extent()-1) + (PATCH-1)/2) / (PATCH-1);
}

fn pack(v: vec2f) -> vec4f {
    let q = vec2u(clamp(round((v + 512.0) * 64.0), vec2f(0.0), vec2f(65535.0)));
    return vec4f(vec4u(q.x >> 8u, q.x & 255u, q.y >> 8u, q.y & 255u)) / 255.0;
}

// Template and search luma at level texel `p` (B → A searches swap them).
fn lum(p: vec2i, size: vec2i, search: bool) -> f32 {
    let c = textureLoad(tex1, clamp(p, vec2i(0), size - 1), 0);
    return select(c.x, c.y, search != (u.flags.x != 0u));
}

fn lum_bilinear(q: vec2f, size: vec2i, search: bool) -> f32 {
    let i = vec2i(floor(q));
    let f = fract(q);
    return mix(
        mix(lum(i, size, search), lum(i + vec2i(1, 0), size, search), f.x),
        mix(lum(i + vec2i(0, 1), size, search), lum(i + vec2i(1, 1), size, search), f.x),
        f.y);
}

// Level `size` texels of both frames' luma, averaged over 2^level render px.
@compute @workgroup_size(8, 8)
fn luma_base(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size.xy) { return; }
    let n = i32(u.data.x);
    var sum = vec2f(0.0);
    for (var y = 0; y < n; y++) {
        for (var x = 0; x < n; x++) {
            let px = vec2f(u.region.xy + vec2i(id.xy) * n + vec2i(x, y)) + 0.5;
            sum += vec2f(
                dot(u.data.yzw, fx_sample(tex1, u.img_a, px, false).rgb),
                dot(u.data.yzw, fx_sample(tex2, u.img_b, px, false).rgb));
        }
    }
    textureStore(out_tex, id.xy, vec4f(sum / f32(n * n), 0.0, 1.0));
}

// Half-size level from `tex1` (2 × 2 average).
@compute @workgroup_size(8, 8)
fn luma_down(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size.xy) { return; }
    let prev = vec2i(u.size.zw);
    let p = vec2i(id.xy) * 2;
    let c = textureLoad(tex1, min(p, prev - 1), 0)
        + textureLoad(tex1, min(p + vec2i(1, 0), prev - 1), 0)
        + textureLoad(tex1, min(p + vec2i(0, 1), prev - 1), 0)
        + textureLoad(tex1, min(p + vec2i(1, 1), prev - 1), 0);
    textureStore(out_tex, id.xy, c * 0.25);
}

fn patch_origin(i: vec2i, size: vec2i) -> vec2i {
    return clamp(i * stride(), vec2i(0), max(size - block_extent(), vec2i(0)));
}

// Mean-normalised SSD of the patch at `o` displaced by `d` (level px).
fn patch_cost(o: vec2i, d: vec2f, size: vec2i) -> f32 {
    var ma = 0.0;
    var mb = 0.0;
    if u.flags.y != 0u {
        for (var y = 0; y < PATCH; y++) {
            for (var x = 0; x < PATCH; x++) {
                let p = o + sample_offset(x,y);
                ma += lum(p, size, false);
                mb += lum_bilinear(vec2f(p) + d, size, true);
            }
        }
    }
    let shift = select(0.0, (mb-ma) / f32(PATCH*PATCH), u.flags.y != 0u);
    var e = 0.0;
    for (var y = 0; y < PATCH; y++) {
        for (var x = 0; x < PATCH; x++) {
            let p = o + sample_offset(x,y);
            let r = lum_bilinear(vec2f(p) + d, size, true) - lum(p, size, false) - shift;
            e += r * r;
        }
    }
    return e;
}

// One patch: inverse-compositional Gauss-Newton from the coarser flow.
@compute @workgroup_size(8, 8)
fn search(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size.zw) { return; }
    let size = vec2i(u.size.xy);
    let o = patch_origin(vec2i(id.xy), size);
    let scale = u.data.x;
    var d = vec2f(0.0);
    if u.flags.w != 0u {
        let centre = vec2f(o) + f32(block_extent()) * 0.5;
        d = flow_at(tex2, centre * 0.5, vec2i(u.coarse.xy)) / scale;
    }
    // Template gradients and Hessian.
    var h = vec3f(1e-4, 0.0, 1e-4);
    var ma = 0.0;
    for (var y = 0; y < PATCH; y++) {
        for (var x = 0; x < PATCH; x++) {
            let p = o + sample_offset(x,y);
            let g = 0.5 * vec2f(
                lum(p + vec2i(1, 0), size, false) - lum(p - vec2i(1, 0), size, false),
                lum(p + vec2i(0, 1), size, false) - lum(p - vec2i(0, 1), size, false));
            h += vec3f(g.x * g.x, g.x * g.y, g.y * g.y);
            ma += lum(p, size, false);
        }
    }
    ma /= f32(PATCH * PATCH);
    // Penalise movement away from the coarse estimate in low-texture patches.
    let stiffness = exp2(clamp(u.data.y, -5.0, 5.0)*2.0) * 0.0001;
    h.x += stiffness;
    h.z += stiffness;
    let det = h.x * h.z - h.y * h.y;
    let start = d;
    for (var it = 0u; it < u.flags.z; it++) {
        var mb = 0.0;
        if u.flags.y != 0u {
            for (var y = 0; y < PATCH; y++) {
                for (var x = 0; x < PATCH; x++) {
                    mb += lum_bilinear(vec2f(o + sample_offset(x,y)) + d, size, true);
                }
            }
        }
        mb /= f32(PATCH * PATCH);
        var b = vec2f(0.0);
        for (var y = 0; y < PATCH; y++) {
            for (var x = 0; x < PATCH; x++) {
                let p = o + sample_offset(x,y);
                let g = 0.5 * vec2f(
                    lum(p + vec2i(1, 0), size, false) - lum(p - vec2i(1, 0), size, false),
                    lum(p + vec2i(0, 1), size, false) - lum(p - vec2i(0, 1), size, false));
                let e = lum_bilinear(vec2f(p) + d, size, true) - lum(p, size, false) - select(0.0, mb-ma, u.flags.y != 0u);
                b += g * e;
            }
        }
        b += stiffness * (d - start);
        let step = vec2f(h.z * b.x - h.y * b.y, h.x * b.y - h.y * b.x) / det;
        d -= step;
        if dot(step, step) < 1e-4 { break; }
    }
    // Keep the coarser estimate when the search made the match worse.
    if patch_cost(o, d, size) > patch_cost(o, start, size) {
        d = start;
    }
    textureStore(out_tex, id.xy, pack(d * scale));
}

// Per-pixel flow: the covering patches' flows weighted by how well each one
// matches at this pixel.
@compute @workgroup_size(8, 8)
fn densify(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size.xy) { return; }
    let size = vec2i(u.size.xy);
    let grid = vec2i(u.size.zw);
    let p = vec2i(id.xy);
    let scale = u.data.x;
    let a = lum(p, size, false);
    var sum = vec2f(0.0);
    var total = 0.0;
    let lo = max((p - block_extent()) / stride() - 1, vec2i(0));
    let hi = min(p / stride() + 1, grid - 1);
    for (var j = lo.y; j <= hi.y; j++) {
        for (var i = lo.x; i <= hi.x; i++) {
            let o = patch_origin(vec2i(i, j), size);
            if any(p < o) || any(p >= o + block_extent()) { continue; }
            let f = load_flow(tex2, vec2i(i, j), grid);
            let r = lum_bilinear(vec2f(p) + f / scale, size, true) - a;
            let w = 1.0 / max(abs(r), max(0.0001, u.data.z * 0.4));
            let confidence = 1.0 / (1.0 + 8.0 * u.data.z * abs(r));
            sum += w * confidence * f;
            total += w;
        }
    }
    textureStore(out_tex, id.xy, pack(select(vec2f(0.0), sum / total, total > 0.0)));
}

// Edge-aware relaxation of the finest vectors. Negative local smoothness sharpens changes.
@compute @workgroup_size(8, 8)
fn smooth_vectors(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size.xy) { return; }
    let p = vec2i(id.xy);
    let size = vec2i(u.size.xy);
    let v = load_flow(tex2, p, size);
    let center = lum(p, size, false);
    let offsets = array<vec2i, 4>(vec2i(-1,0), vec2i(1,0), vec2i(0,-1), vec2i(0,1));
    var sum = v;
    var total = 1.0;
    for (var i=0; i<4; i++) {
        let q = p + offsets[i];
        let w = exp(-32.0 * abs(center-lum(q,size,false)));
        sum += w * load_flow(tex2,q,size);
        total += w;
    }
    textureStore(out_tex,id.xy,pack(v + clamp(u.data.y,-5.0,5.0)*0.15*(sum/total-v)));
}

// Flow (render px) at render pixel centre `px`, from a level texture in `t`.
fn flow_px(t: texture_2d<f32>, px: vec2f) -> vec2f {
    let q = (px - vec2f(u.region.xy)) / u.data.x;
    return flow_at(t, q, vec2i(u.size.xy));
}

// Pixel Motion Blur: average along the flow over the shutter.
@compute @workgroup_size(8, 8)
fn vector_blur(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let px = vec2f(p) + 0.5;
    // A neighbour's motion whose blur reaches this pixel smears over it too
    // (the fastest such motion wins), so moving edges blur outwards.
    var v = flow_px(tex3, px);
    let reach = max(abs(u.data.y), abs(u.data.y+u.data.z));
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let off = vec2f(f32(x), f32(y)) * 4.0;
            let c = flow_px(tex3, px + off);
            if length(c) * reach >= length(off) && dot(c, c) > dot(v, v) {
                v = c;
            }
        }
    }
    let n = max(u.flags.y, 1u);
    var sum = vec4f(0.0);
    for (var k = 0u; k < n; k++) {
        let s = u.data.y + u.data.z * (f32(k) + 0.5) / f32(n);
        sum += fx_bilinear(tex1, u.img_a, px + v * s, false);
    }
    textureStore(out_tex, id.xy, sum / f32(n));
}

// How well motion `v` (A → B) explains render pixel `px` at phase `w`: the
// colour difference between A and B sampled along it, on a 5-tap cross.
fn tween_cost(px: vec2f, v: vec2f, w: f32) -> f32 {
    let taps = array<vec2f, 5>(vec2f(0.0), vec2f(1.5, 0.0), vec2f(-1.5, 0.0), vec2f(0.0, 1.5), vec2f(0.0, -1.5));
    var e = 0.0;
    for (var i = 0; i < 5; i++) {
        let o = px + taps[i];
        let d = fx_bilinear(guide_a, u.guide_a, o - w * v, false)
            - fx_bilinear(guide_b, u.guide_b, o + (1.0 - w) * v, false);
        e += dot(d, d);
    }
    return e;
}

fn cubic_weights(x: f32) -> vec4f {
    let x2 = x*x;
    let x3 = x2*x;
    return 0.5*vec4f(-x+2.0*x2-x3, 2.0-5.0*x2+3.0*x3, x+4.0*x2-3.0*x3, -x2+x3);
}

fn sample_warp(t: texture_2d<f32>, img: FxImage, px: vec2f) -> vec4f {
    if u.flags.y == 0u { return fx_bilinear(t,img,px,false); }
    let p = (px-img.origin)*img.scale-0.5;
    let base = vec2i(floor(p))-1;
    let wx = cubic_weights(fract(p.x));
    let wy = cubic_weights(fract(p.y));
    var sum = vec4f(0.0);
    for (var y=0; y<4; y++) {
        for (var x=0; x<4; x++) {
            sum += wx[x]*wy[y]*fx_load(t,img,base+vec2i(x,y),false);
        }
    }
    return sum;
}

// Frame interpolation at phase `data.y` between A and B. Each pixel picks the
// motion through it from both flows (and one fixed-point step of each) by
// which one A and B agree on, so moving objects land in between instead of
// leaving a double image.
@compute @workgroup_size(8, 8)
fn interpolate(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let px = vec2f(p) + 0.5;
    let w = u.data.y;
    let c1 = flow_px(tex3, px);
    let c2 = -flow_px(tex4, px);
    let candidates = array<vec2f, 4>(
        c1,
        c2,
        flow_px(tex3, px - w * c1),
        -flow_px(tex4, px + (1.0 - w) * c2),
    );
    var best = c1;
    var cost = tween_cost(px, c1, w);
    for (var i = 1; i < 4; i++) {
        let e = tween_cost(px, candidates[i], w);
        if e < cost {
            cost = e;
            best = candidates[i];
        }
    }
    let a = sample_warp(tex1, u.img_a, px - w * best);
    let b = sample_warp(tex2, u.img_b, px + (1.0 - w) * best);
    textureStore(out_tex, id.xy, select(mix(a,b,w), select(a,b,w>=0.5), u.flags.x != 0u));
}
