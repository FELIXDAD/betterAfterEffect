// Layer masks (src/mask.rs, AE-CMP-038..045).
//
// vs_edge/fs_edge: exact-area coverage accumulation. Each instance is one path
// edge; its quad spans the edge's rows from the edge to the texel containing the
// right bound of the path (right of it all windings cancel). A texel receives the signed area of the texel that lies to the right of
// the edge (within the edge's rows), so after additive blending every texel holds
// its winding number, fractional along edges (anti-aliasing). R: path, G:
// expansion outline.
//
// vs_full/fs_pass: resolve (fill rule, expansion, layer-frame clip), separable
// Gaussian feather, and the combine pass (bilinear upsample, inversion and
// opacity, mode via fixed-function blending).
//
// Every mask of a frame shares atlas pages: each raster is a region with a zero
// gutter, draws are scissored to it, and reads stay inside it (src_rect).

struct Edge {
    p0: vec2<f32>,
    p1: vec2<f32>,
    // Right end of the quad (texels), channel (0 path, 1 outline), target size.
    xmax: f32,
    chan: u32,
    size: vec2<f32>,
}

struct Pass {
    // Target texel (in its page) -> source texel (in its region): s = t * map.xy + map.zw.
    map: vec4<f32>,
    // Source texture size (w, h, 1/w, 1/h).
    src: vec4<f32>,
    // Blur direction (x, y), sigma (source texels), radius (taps each side).
    blur: vec4<f32>,
    // Layer frame in source texels (x0, y0, x1, y1).
    clip: vec4<f32>,
    // Output = a + b * coverage (x, y); kernel normalisation (z).
    value: vec4<f32>,
    // Source kind (0 winding, 1 coverage texels, 2 coverage bilinear), fill rule
    // (0 nonzero, 1 even-odd), expansion (0 none, 1 grow, 2 shrink), clip on.
    mode: vec4<u32>,
    // The source region in its atlas page: origin (x, y), size (w, h) in texels.
    src_rect: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> edges: array<Edge>;
@group(0) @binding(1) var<storage, read> passes: array<Pass>;
@group(0) @binding(2) var t_in: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;

struct EdgeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) edge: u32,
}

@vertex
fn vs_edge(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> EdgeOut {
    let e = edges[ii];
    let x0 = floor(min(e.p0.x, e.p1.x));
    let x1 = max(floor(e.xmax) + 1.0, x0 + 1.0);
    let y0 = floor(min(e.p0.y, e.p1.y));
    let y1 = max(ceil(max(e.p0.y, e.p1.y)), y0 + 1.0);
    let cx = vi == 1u || vi == 4u || vi == 5u;
    let cy = vi == 2u || vi == 3u || vi == 5u;
    let p = vec2<f32>(select(x0, x1, cx), select(y0, y1, cy));
    var out: EdgeOut;
    out.pos = vec4<f32>(p.x / e.size.x * 2.0 - 1.0, 1.0 - p.y / e.size.y * 2.0, 0.0, 1.0);
    out.edge = ii;
    return out;
}

// Antiderivative of g(t) = clamp(1 - t, 0, 1): the covered fraction of a texel
// row [0, 1] to the right of x = t.
fn area_g(t: f32) -> f32 {
    if t <= 0.0 {
        return t;
    }
    if t >= 1.0 {
        return 0.5;
    }
    return t - 0.5 * t * t;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    let e = edges[in.edge];
    let cell = floor(in.pos.xy);
    let ya = max(min(e.p0.y, e.p1.y), cell.y);
    let yb = min(max(e.p0.y, e.p1.y), cell.y + 1.0);
    if yb <= ya {
        discard;
    }
    let dy = e.p1.y - e.p0.y;
    let slope = (e.p1.x - e.p0.x) / dy;
    let ta = e.p0.x + (ya - e.p0.y) * slope - cell.x;
    let tb = e.p0.x + (yb - e.p0.y) * slope - cell.x;
    var avg: f32;
    if max(ta, tb) <= 0.0 {
        avg = 1.0;
    } else if min(ta, tb) >= 1.0 {
        discard;
    } else if abs(tb - ta) < 1e-4 {
        avg = clamp(1.0 - 0.5 * (ta + tb), 0.0, 1.0);
    } else {
        avg = (area_g(tb) - area_g(ta)) / (tb - ta);
    }
    let v = sign(dy) * (yb - ya) * avg;
    return select(vec4<f32>(v, 0.0, 0.0, 0.0), vec4<f32>(0.0, v, 0.0, 0.0), e.chan == 1u);
}

struct FullOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) pass_index: u32,
}

@vertex
fn vs_full(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> FullOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: FullOut;
    out.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.pass_index = ii;
    return out;
}

fn fill(w: f32, even_odd: bool) -> f32 {
    if even_odd {
        return abs(w - 2.0 * round(0.5 * w));
    }
    return min(abs(w), 1.0);
}

// Overlap of texel `cell` with the clip rectangle.
fn clip_cov(c: vec4<f32>, cell: vec2<f32>) -> f32 {
    let x = clamp(min(cell.x + 1.0, c.z) - max(cell.x, c.x), 0.0, 1.0);
    let y = clamp(min(cell.y + 1.0, c.w) - max(cell.y, c.y), 0.0, 1.0);
    return x * y;
}

// Coverage of texel `cell` of the source region (0 outside it).
fn load_cov(pd: Pass, cell: vec2<i32>) -> f32 {
    if any(cell < vec2<i32>(0)) || any(cell >= vec2<i32>(pd.src_rect.zw)) {
        return 0.0;
    }
    let t = textureLoad(t_in, cell + vec2<i32>(pd.src_rect.xy), 0);
    if pd.mode.x != 0u {
        return t.x;
    }
    var c = fill(t.x, pd.mode.y == 1u);
    if pd.mode.z == 1u {
        c = max(c, min(abs(t.y), 1.0));
    } else if pd.mode.z == 2u {
        c = c * (1.0 - min(abs(t.y), 1.0));
    }
    if pd.mode.w != 0u {
        c = c * clip_cov(pd.clip, vec2<f32>(cell));
    }
    return c;
}

@fragment
fn fs_pass(in: FullOut) -> @location(0) vec4<f32> {
    let pd = passes[in.pass_index];
    let s = in.pos.xy * pd.map.xy + pd.map.zw;
    var c: f32;
    if pd.mode.x == 2u {
        // Half a texel into the region's zero gutter, no further.
        let sl = clamp(s, vec2<f32>(-0.5), pd.src_rect.zw + 0.5);
        c = textureSampleLevel(t_in, s_linear, (sl + pd.src_rect.xy) * pd.src.zw, 0.0).x;
    } else {
        let base = vec2<i32>(floor(s));
        let r = i32(pd.blur.w);
        if r == 0 {
            c = load_cov(pd, base);
        } else {
            let dir = vec2<i32>(pd.blur.xy);
            let k = -0.5 / (pd.blur.z * pd.blur.z);
            var acc = 0.0;
            for (var i = -r; i <= r; i++) {
                acc += exp(f32(i * i) * k) * load_cov(pd, base + dir * i);
            }
            c = acc * pd.value.z;
        }
    }
    let v = clamp(pd.value.x + pd.value.y * c, 0.0, 1.0);
    return vec4<f32>(v, 0.0, 0.0, 1.0);
}
