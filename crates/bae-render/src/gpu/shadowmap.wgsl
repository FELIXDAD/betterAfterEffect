// Receiver-space shadow map of a lit 3D layer (graph::ShadowMapNode): per
// shadow-casting light, how much of it reaches each point of the layer past
// its casters (`shadow` in shadow.wgsl), sampled in the layer's own pixels. A
// horizontal strip of slots, slot k for light `slots[k]`. It is drawn when
// the layer, its lights or its casters change; lit draws sample it (`shade`
// in composite.wgsl) instead of testing every caster per pixel.

struct Params {
    // Layer pixels covered (x0, y0, x1, y1).
    rect: vec4<f32>,
    // Slot size in texels (w, h), slot count, lighting record offset.
    size: vec4<f32>,
    // The light of each slot (8 at most).
    slots: array<vec4<u32>, 2>,
}

@group(0) @binding(0) var<uniform> m: Params;
@group(0) @binding(1) var<storage, read> lighting: array<vec4<f32>>;
@group(0) @binding(2) var t_shadow: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the viewport.
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_map(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let w = m.size.x;
    let slot = min(u32(pos.x / w), u32(m.size.z) - 1u);
    let li = m.slots[slot / 4u][slot % 4u];
    // Texel centres span the rect.
    let local = vec2<f32>((pos.x - f32(slot) * w) / w, pos.y / m.size.y);
    let u = mix(m.rect.xy, m.rect.zw, local);
    let o = u32(m.size.w);
    let p = lighting[o].xyz * u.x + lighting[o + 1u].xyz * u.y + lighting[o + 2u].xyz;
    let eye = lighting[o + 4u];
    let pixel = select(-eye.w, eye.w * length(eye.xyz - p), eye.w > 0.0);
    let b = o + 7u + 5u * li;
    return vec4<f32>(shadow(o, p, u, li, lighting[b], lighting[b + 1u], lighting[b + 4u], pixel), 1.0);
}
