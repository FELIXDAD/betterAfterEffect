// Small kernels shared by the Rust effects.

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;

struct Params {
    rect: vec4<i32>,
    // 0 = straight → premultiplied, 1 = opaque (alpha forced to 1).
    mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@compute @workgroup_size(8, 8, 1)
fn to_premultiplied(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = vec2<i32>(id.xy);
    if (any(t >= u.rect.zw - u.rect.xy)) {
        return;
    }
    let c = textureLoad(src, t, 0);
    var o = vec4<f32>(c.rgb * c.a, c.a);
    if (u.mode == 1u) {
        o = vec4<f32>(c.rgb, 1.0);
    }
    textureStore(dst, t, o);
}
