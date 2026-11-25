struct Settings {
    width: u32,
    has_gate: u32,
    weight: f32,
    padding: u32,
}
@group(0) @binding(0) var<uniform> u: Settings;
@group(0) @binding(1) var dst: texture_storage_2d<WORKING_FORMAT, write>;
@group(0) @binding(2) var previous: texture_2d<f32>;
@group(0) @binding(3) var next: texture_2d<f32>;
@group(0) @binding(4) var gate: texture_2d<f32>;

var<workgroup> differences: array<f32, 64>;

@compute @workgroup_size(8, 8)
fn scene_compare(@builtin(local_invocation_index) lane: u32) {
    let a_size = vec2f(textureDimensions(previous));
    let b_size = vec2f(textureDimensions(next));
    var diff = 0.0;
    // Compare a regular 16 × 16 grid, in normalized layer coordinates.
    for (var i = lane; i < 256u; i += 64u) {
        let uv = (vec2f(f32(i % 16u), f32(i / 16u)) + 0.5) / 16.0;
        let a = textureLoad(previous, vec2i(uv * a_size), 0);
        let b = textureLoad(next, vec2i(uv * b_size), 0);
        let d = abs(a - b);
        diff += max(max(d.r, d.g), max(d.b, d.a));
    }
    differences[lane] = diff;
    workgroupBarrier();
    if lane == 0u {
        var total = 0.0;
        for (var i = 0u; i < 64u; i++) { total += differences[i]; }
        let intact = u.has_gate == 0u || textureLoad(gate, vec2i(0), 0).r > 0.0;
        let same_scene = intact && total / 256.0 < 0.22;
        textureStore(dst, vec2i(0), vec4f(f32(same_scene), 0.0, 0.0, 1.0));
    }
}

@compute @workgroup_size(8, 8)
fn mix_temporal_stats(@builtin(global_invocation_id) id: vec3u) {
    if id.x > u.width || id.y != 0u { return; }
    let allowed = u.has_gate == 0u || textureLoad(gate, vec2i(0), 0).r > 0.0;
    let weight = select(0.0, u.weight, allowed);
    var count = 1.0;
    if textureDimensions(previous).x > u.width {
        count = textureLoad(previous, vec2i(i32(u.width), 0), 0).r;
    }
    if id.x == u.width {
        textureStore(dst, id.xy, vec4f(count + weight, 0.0, 0.0, 1.0));
    } else {
        let a = textureLoad(previous, id.xy, 0);
        let b = textureLoad(next, id.xy, 0);
        textureStore(dst, id.xy, mix(a, b, weight / (count + weight)));
    }
}
