// Adjustment layer mix (graph::MixNode): out = base + (over - base) * coverage.a,
// per output texel. The three inputs live in the same space but may have their
// own rects and scales: each is read at the output texel's centre in space
// pixels; outside its rect an input is transparent.

struct Img {
    // Covered region in space pixels (x0, y0, x1, y1).
    rect: vec4<f32>,
    // x: space pixels per texel; y: alpha (0 premultiplied, 1 straight, 2 opaque).
    info: vec4<f32>,
}

struct Params {
    // Output texel (0, 0)'s corner in space pixels, and space pixels per texel.
    origin: vec2<f32>,
    scale: f32,
    pad: f32,
    base: Img,
    over: Img,
    cover: Img,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var t_base: texture_2d<f32>;
@group(0) @binding(2) var t_over: texture_2d<f32>;
@group(0) @binding(3) var t_cover: texture_2d<f32>;

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the viewport.
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn fetch(t: texture_2d<f32>, img: Img, q: vec2<f32>) -> vec4<f32> {
    if (any(q < img.rect.xy) || any(q >= img.rect.zw)) {
        return vec4<f32>(0.0);
    }
    let texel = vec2<i32>(floor((q - img.rect.xy) / img.info.x));
    var c = textureLoad(t, texel, 0);
    if (img.info.y == 1.0) {
        c = vec4<f32>(c.rgb * c.a, c.a);
    } else if (img.info.y == 2.0) {
        c.a = 1.0;
    }
    return c;
}

@fragment
fn fs_mix(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // pos.xy is the texel centre (x + 0.5).
    let q = u.origin + pos.xy * u.scale;
    let b = fetch(t_base, u.base, q);
    let e = fetch(t_over, u.over, q);
    let k = clamp(fetch(t_cover, u.cover, q).a, 0.0, 1.0);
    return mix(b, e, k);
}
