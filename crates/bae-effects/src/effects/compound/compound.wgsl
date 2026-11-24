struct Compound {
    input_image: FxImage,
    map_image: FxImage,
    roi: vec4i,
    layer: vec4i,
    flags: vec4u,
    data: vec4f,
    secondary: vec4f,
}

@group(0) @binding(0) var<uniform> u: Compound;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var map_tex: texture_2d<f32>;
@group(0) @binding(3) var output_tex: texture_storage_2d<WORKING_FORMAT, write>;

fn wrapped_pixel(t: texture_2d<f32>, img: FxImage, p: vec2i, straight: bool) -> vec4f {
    let n = vec2i(img.size);
    return fx_load(t, img, (p % n + n) % n, straight);
}

fn wrapped_sample(t: texture_2d<f32>, img: FxImage, p: vec2f, straight: bool) -> vec4f {
    let texel = (p - img.origin) * img.scale - 0.5;
    let f = fract(texel);
    let q = vec2i(floor(texel));
    return mix(
        mix(wrapped_pixel(t, img, q, straight), wrapped_pixel(t, img, q + vec2i(1, 0), straight), f.x),
        mix(wrapped_pixel(t, img, q + vec2i(0, 1), straight), wrapped_pixel(t, img, q + vec2i(1, 1), straight), f.x),
        f.y);
}

// All map effects use 1 tile, 2 center, 3 stretch in the shader.
fn sample_map(map_tex: texture_2d<f32>, image: FxImage, layer: vec4i, placement: u32, p: vec2f, outside: vec4f) -> vec4f {
    let size = vec2f(image.size) / image.scale;
    let bounds = vec2f(layer.zw - layer.xy);
    let local = p - vec2f(layer.xy);
    if placement == 1u {
        return wrapped_sample(map_tex, image, image.origin + local, true);
    }
    let q = select(local - (bounds - size) * 0.5, local * size / bounds, placement == 3u);
    if any(q < vec2f(0.0)) || any(q >= size) {
        return outside;
    }
    let inset = vec2f(0.5 / image.scale);
    return fx_bilinear(map_tex, image, image.origin + clamp(q, inset, size - inset), true);
}

fn wipe_mask(luma: f32) -> f32 {
    let t = u.data.x;
    let softness = u.data.y;
    if t >= 1.0 { return 0.0; }
    if u.flags.w == 0u {
        let v = select(luma, 1.0 - luma, u.flags.z != 0u);
        return select(saturate((v - t*(1.0 + softness) + softness) / max(softness, 1e-8)),
                      step(t, v), softness == 0.0);
    }
    let white = f32(u.flags.w);
    let level = floor(saturate(luma) * white + 0.49999);
    let v = select(level, white - level, u.flags.z != 0u);
    let spread = round(softness * white);
    let threshold = round(t * (white + spread));
    let fixed = floor(t * 65536.0) / 65536.0;
    return select(saturate((v - threshold + spread) / max(1.0, spread)),
                  saturate(v - floor(fixed * white) + 0.5), spread == 0.0);
}

fn displacement(c: vec4f, channel: u32) -> f32 {
    if channel == 9u { return select(127.0 / 128.0, 1.0, u.secondary.y != 0.0); }
    if channel == 11u { return -1.0; }
    if channel >= 10u { return 0.0; }
    let mode = select(select(channel, channel + 1u, channel < 4u), 1u, channel == 4u);
    let v = fx_channel(c, mode);
    let coverage = select(c.a, 1.0, channel == 4u);
    if u.secondary.y != 0.0 { return (2.0 * v - 1.0) * coverage; }
    let half_level = select(128.0, 16384.0, u.secondary.z != 0.0);
    let white = select(255.0, 32768.0, u.secondary.z != 0.0);
    let level = round(saturate(v) * white) - half_level;
    return level / half_level * coverage;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let px = vec2f(p) + 0.5;
    var out: vec4f;
    switch u.flags.x {
        case 0u: {
            let c = fx_sample(input_tex, u.input_image, px, false);
            out = c * wipe_mask(fx_luma(sample_map(map_tex, u.map_image, u.layer, u.flags.y, px, vec4f(0)).rgb));
        }
        case 1u: {
            if u.flags.z >= 9u && u.flags.w >= 9u {
                out = fx_sample(input_tex, u.input_image, px, false);
            } else {
                let map = sample_map(map_tex, u.map_image, u.layer, u.flags.y, px, vec4f(0.5));
                let d = vec2f(displacement(map, u.flags.z), displacement(map, u.flags.w));
                let q = px + d * u.data.xy;
                if u.secondary.x != 0.0 {
                    out = wrapped_sample(input_tex, u.input_image, q, false);
                } else {
                    out = fx_bilinear(input_tex, u.input_image, q, false);
                }
            }
        }
        case 2u: {
            let c = fx_sample(input_tex, u.input_image, px, true);
            let step = u.data.xy;
            let a = sample_map(map_tex, u.map_image, u.layer, u.flags.y, px - step, vec4f(0)).rgb;
            let b = sample_map(map_tex, u.map_image, u.layer, u.flags.y, px + step, vec4f(0)).rgb;
            out = fx_output(c.rgb + (b - a) * u.data.z, c.a, u.flags.z != 0u);
        }
        default: {
            let coverage = fx_sample(map_tex, u.map_image, px, false).a;
            let a = select(coverage, 1.0 - coverage, u.flags.y != 0u) * u.data.w;
            out = fx_output(u.data.rgb, a, u.flags.z != 0u);
            if u.flags.w != 0u {
                out += fx_sample(input_tex, u.input_image, px, false) * (1.0 - out.a);
            }
        }
    }
    textureStore(output_tex, id.xy, out);
}
