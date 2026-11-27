struct FxImage {
    origin: vec2f,
    scale: f32,
    alpha_mode: u32,
    size: vec2u,
    padding: vec2u,
}

fn fx_load(t: texture_2d<f32>, img: FxImage, p: vec2i, straight: bool) -> vec4f {
    if any(p < vec2i(0)) || any(p >= vec2i(img.size)) {
        return vec4f(0.0);
    }
    let c = textureLoad(t, p, 0);
    if straight {
        switch img.alpha_mode {
            case 1u: { return c; }
            case 2u: { return vec4f(c.rgb, 1.0); }
            default: { return fx_unpremult(c); }
        }
    }
    switch img.alpha_mode {
        case 1u: { return vec4f(c.rgb * c.a, c.a); }
        case 2u: { return vec4f(c.rgb, 1.0); }
        default: { return c; }
    }
}

fn fx_sample(t: texture_2d<f32>, img: FxImage, p: vec2f, straight: bool) -> vec4f {
    let q = vec2i(floor((p - img.origin) * img.scale));
    return fx_load(t, img, q, straight);
}

fn fx_bilinear(t: texture_2d<f32>, img: FxImage, p: vec2f, straight: bool) -> vec4f {
    let texel = (p - img.origin) * img.scale - 0.5;
    let f = fract(texel);
    let q = vec2i(floor(texel));
    if all(f == vec2f(0.0)) { return fx_load(t, img, q, straight); }
    return mix(
        mix(fx_load(t, img, q, straight), fx_load(t, img, q + vec2i(1, 0), straight), f.x),
        mix(fx_load(t, img, q + vec2i(0, 1), straight), fx_load(t, img, q + vec2i(1, 1), straight), f.x),
        f.y);
}

fn fx_output(rgb: vec3f, alpha: f32, is_float: bool) -> vec4f {
    if is_float {
        return vec4f(rgb * alpha, alpha);
    }
    let a = clamp(alpha, 0.0, 1.0);
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)) * a, a);
}
