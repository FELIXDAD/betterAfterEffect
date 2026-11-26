struct Sampling {
    src: FxImage, unused: FxImage, roi: vec4i, bounds: vec4i,
    center: vec2f, radius: f32, mode: u32,
}
@group(0) @binding(0) var<uniform> u: Sampling;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(3) var mean: texture_storage_2d<WORKING_FORMAT, write>;
var<workgroup> colors: array<vec4f, 64>;
var<workgroup> weights: array<u32, 64>;

@compute @workgroup_size(8,8)
fn main(@builtin(local_invocation_index) lane: u32) {
    let count = i32(floor(u.radius));
    let side = u32(2 * count + 1);
    var sum = vec4f(0.0);
    var weight = 0u;
    for (var i = lane; i < side * side; i += 64u) {
        let delta = vec2f(vec2i(i32(i % side), i32(i / side)) - vec2i(count));
        if dot(delta, delta) > u.radius * u.radius { continue; }
        let pos = floor(u.center + delta);
        if any(vec2i(pos) < u.bounds.xy) || any(vec2i(pos) >= u.bounds.zw) { continue; }
        let c = fx_sample(src, u.src, pos + 0.5, true);
        if u.mode == 1u && c.a == 0.0 { continue; }
        sum += select(c, vec4f(c.rgb * c.a, c.a), u.mode == 3u);
        weight += 1u;
    }
    colors[lane] = sum;
    weights[lane] = weight;
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        workgroupBarrier();
        if lane < stride {
            colors[lane] += colors[lane + stride];
            weights[lane] += weights[lane + stride];
        }
    }
    if lane == 0u {
        textureStore(mean, vec2i(0), colors[0] / f32(max(weights[0], 1u)));
    }
}
