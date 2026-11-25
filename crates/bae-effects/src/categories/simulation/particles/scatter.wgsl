struct Scatter {
    input: FxImage, roi: vec4i, layer: vec4i,
    amount: f32, right: f32, left: f32, mode: u32,
}
@group(0) @binding(0) var<uniform> u: Scatter;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var output_tex: texture_storage_2d<WORKING_FORMAT, write>;

fn hash32(x: u32) -> u32 {
    var h = x ^ (x >> 16u);
    h *= 0x7feb352du;
    h ^= h >> 15u;
    h *= 0x846ca68bu;
    return h ^ (h >> 16u);
}
fn random(x: u32) -> f32 { return f32(hash32(x) >> 8u) / 16777216.; }

fn blend(a: vec4f, b: vec4f) -> vec4f {
    let alpha = a.a + (1. - a.a) * b.a;
    switch u.mode {
        case 2u: { return vec4f(a.rgb + b.rgb - a.rgb * b.rgb, alpha); }
        case 3u: { return vec4f(a.rgb + b.rgb, alpha); }
        case 4u: { return vec4f(a.rgb + b.rgb, min(1., a.a + b.a)); }
        default: { return a + (1. - a.a) * b; }
    }
}

// Radius is invariant under the forward twist. Invert each half separately so
// opposing twists can leave holes or overlap at the centre line.
fn untwist(p: vec2f) -> vec4f {
    if u.right == 0. && u.left == 0. { return fx_sample(source, u.input, p, false); }
    let center = (vec2f(u.layer.xy) + vec2f(u.layer.zw)) * .5;
    let r = p - center;
    let k = -length(r) / max(1., f32(u.layer.z - u.layer.x));
    var color = vec4f(0.);
    for (var side = 0u; side < 2u; side++) {
        let a = k * select(u.left, u.right, side == 1u);
        let q = center + vec2f(cos(a)*r.x - sin(a)*r.y, sin(a)*r.x + cos(a)*r.y);
        if (q.x >= center.x) == (side == 1u) {
            color = blend(fx_bilinear(source, u.input, q, false), color);
        }
    }
    return color;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3u) {
    let xy = vec2i(gid.xy);
    if any(xy >= u.roi.zw - u.roi.xy) { return; }
    let pos = u.roi.xy + xy;
    let p = vec2f(pos) + .5;
    let coverage = min(abs(u.amount), 1.);
    var scattered = vec4f(0.);
    if coverage > 0. {
        let key = hash32(bitcast<u32>(pos.x)) ^ hash32(bitcast<u32>(pos.y) + 37u);
        // Displaced source pixels have approximately Poisson(1) occupancy.
        let v = random(key);
        let hits = u32(v > .367879) + u32(v > .735759) + u32(v > .919699) + u32(v > .981012);
        for (var i = 0u; i < hits; i++) {
            let sample = key + 2u * i;
            let jitter = (2. * vec2f(random(sample + 1u), random(sample + 2u)) - 1.) * u.amount;
            scattered = blend(untwist(p + jitter), scattered);
        }
    }
    if coverage < 1. { scattered = mix(untwist(p), scattered, coverage); }
    textureStore(output_tex, xy, scattered);
}
