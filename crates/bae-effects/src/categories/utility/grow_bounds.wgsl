// Grow Bounds: the layer unchanged inside a larger buffer (transparent around it).
fn evaluate(p: vec2f) -> vec4f {
    let c = fx_sample(input_tex, u.images[0], p, true);
    return output(c.rgb, c.a);
}
