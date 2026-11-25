struct Temporal {
    input_image: FxImage,
    map_image: FxImage,
    previous: FxImage,
    roi: vec4i,
    layer: vec4i,
    flags: vec4u,
    data: vec4f,
}

@group(0) @binding(0) var<uniform> u: Temporal;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var map_tex: texture_2d<f32>;
@group(0) @binding(3) var prev_tex: texture_2d<f32>;
@group(0) @binding(4) var output_tex: texture_storage_2d<WORKING_FORMAT, write>;

fn echo(color: vec4f, previous: vec4f) -> vec4f {
    var sample = color * u.data.x;
    if u.flags.y == 7u && u.data.y != 0.0 {
        sample = trunc(sample * u.data.y + 0.00001) / u.data.y;
    }
    if u.flags.z != 0u { return sample; }
    let front = sample + previous * (1.0 - sample.a);
    let back = previous + sample * (1.0 - previous.a);
    switch u.flags.y {
        case 2u: { return vec4f(max(front.rgb, back.rgb), front.a); }
        case 3u: { return vec4f(min(front.rgb, back.rgb), front.a); }
        case 4u: { return previous + sample - previous * sample; }
        case 5u: { return front; }
        case 6u: { return back; }
        case 7u: { return previous + sample; }
        default: { return vec4f(previous.rgb + sample.rgb, front.a); }
    }
}

// Time Difference: `a` (the layer) against `b` (the target), straight colour.
fn difference(a: vec4f, b: vec4f) -> vec4f {
    let ca = select(vec3f(0.0), a.rgb / a.a, a.a > 0.0);
    let cb = select(vec3f(0.0), b.rgb / b.a, b.a > 0.0);
    let d = (ca - cb) * u.data.x;
    var rgb = saturate(select(0.5 + 0.5 * d, abs(d), u.flags.z != 0u));
    var alpha = a.a;
    switch u.flags.y {
        case 2u: { alpha = b.a; }
        case 3u: { alpha = 0.5 * (a.a + b.a); }
        case 4u: { alpha = max(a.a, b.a); }
        case 5u: { alpha = 1.0; }
        case 7u: { alpha = fx_luma(rgb); }
        case 8u: { alpha = max(rgb.r, max(rgb.g, rgb.b)); }
        case 10u: { alpha = saturate(abs(a.a - b.a) * u.data.x); }
        case 11u: {
            rgb = vec3f(saturate(abs(a.a - b.a) * u.data.x));
            alpha = 1.0;
        }
        default: {}
    }
    return vec4f(rgb * alpha, alpha);
}

fn map_level(px: vec2f) -> u32 {
    let map = u.map_image;
    let size = vec2f(map.size) / map.scale;
    let layer = vec2f(u.layer.zw - u.layer.xy);
    let pos = px - vec2f(u.layer.xy);
    let local = select(pos - (layer - size)*0.5, pos*size/layer, u.flags.w != 0u);
    if any(local < vec2f(0)) || any(local >= size) { return 127u; }
    let inset = vec2f(0.5 / map.scale);
    let c = fx_bilinear(map_tex, map, map.origin + clamp(local, inset, size-inset), true);
    return u32(round(saturate(fx_luma(c.rgb)) * 255.0));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let px = vec2f(p) + 0.5;
    if u.flags.x == 3u {
        textureStore(output_tex, id.xy, vec4f(f32(map_level(px)) / 255.0, 0.0, 0.0, 1.0));
        return;
    }
    if u.flags.x == 1u {
        let level = u32(round(textureLoad(map_tex, id.xy, 0).r * 255.0));
        if level < u.flags.y || level > u.flags.z { return; }
    }
    var out = fx_sample(input_tex, u.input_image, px, false);
    if u.flags.x == 4u {
        out = difference(out, fx_sample(map_tex, u.map_image, px, false));
    }
    if u.flags.x == 0u {
        let prev = fx_sample(prev_tex, u.previous, px, false);
        let v = echo(out, prev);
        out = select(v, saturate(v), u.flags.w != 0u);
    }
    textureStore(output_tex, id.xy, out);
}
