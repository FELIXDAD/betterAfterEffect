fn unpack(c: vec4f) -> vec2f {
    let b = vec4u(round(c * 255.0));
    return vec2f(f32(b.x * 256u + b.y), f32(b.z * 256u + b.w)) / 64.0 - 512.0;
}

fn load_flow(t: texture_2d<f32>, p: vec2i, size: vec2i) -> vec2f {
    return unpack(textureLoad(t, clamp(p, vec2i(0), size - 1), 0));
}

// Flow (render px) at level-texel position `q` (texel centres at +0.5).
fn flow_at(t: texture_2d<f32>, q: vec2f, size: vec2i) -> vec2f {
    let s = q - 0.5;
    let i = vec2i(floor(s));
    let f = fract(s);
    return mix(
        mix(load_flow(t, i, size), load_flow(t, i + vec2i(1, 0), size), f.x),
        mix(load_flow(t, i + vec2i(0, 1), size), load_flow(t, i + vec2i(1, 1), size), f.x),
        f.y);
}
