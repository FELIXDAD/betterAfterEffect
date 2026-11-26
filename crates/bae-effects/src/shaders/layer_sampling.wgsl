// Projection coordinates are in scaled layer space. Input and auxiliary textures
// may have different origins and pixel densities.
fn layer_size() -> vec2f {
    return max(vec2f(u.layer.zw - u.layer.xy), vec2f(1.0));
}
fn layer_uv(p: vec2f) -> vec2f {
    return (p - vec2f(u.layer.xy)) / layer_size();
}
fn layer_position(uv: vec2f) -> vec2f {
    return vec2f(u.layer.xy) + uv * layer_size();
}
fn rotate_2d(p: vec2f, angle: f32) -> vec2f {
    let c = cos(angle);
    let s = sin(angle);
    return vec2f(c * p.x - s * p.y, s * p.x + c * p.y);
}
fn sample_layer(tex: texture_2d<f32>, image: FxImage, p: vec2f, stretch: bool) -> vec4f {
    if image.size.x == 0u { return vec4f(0.0); }
    let size = vec2f(image.size) / image.scale;
    let centered = p - layer_position(vec2f(0.5)) + 0.5 * size;
    return fx_bilinear(tex, image, image.origin + select(centered, layer_uv(p) * size, stretch), false);
}
fn sample_source(p: vec2f) -> vec4f {
    return fx_bilinear(input_tex, u.images[0], p, false);
}
fn evaluate(p: vec2f) -> vec4f {
    let c = shade(p);
    if u.float_depth != 0u { return c; }
    let alpha = clamp(c.a, 0.0, 1.0);
    return vec4f(clamp(c.rgb, vec3f(0.0), vec3f(alpha)), alpha);
}
