struct SetMatte {
    input_image: FxImage,
    matte_image: FxImage,
    roi: vec4i,
    layer: vec4i,
    matte_size: vec2f,
    channel: u32,
    invert: u32,
    stretch: u32,
    composite: u32,
    premultiply: u32,
    is_float: u32,
}

@group(0) @binding(0) var<uniform> u: SetMatte;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var matte_tex: texture_2d<f32>;
@group(0) @binding(3) var dst: texture_storage_2d<WORKING_FORMAT, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let pixel = vec2f(p) + 0.5;
    let c = fx_sample(input_tex, u.input_image, pixel, true);
    let size = vec2f(u.layer.zw - u.layer.xy);
    let local = pixel - vec2f(u.layer.xy);
    let centered = local - (size - u.matte_size) * 0.5;
    let stretched = local * u.matte_size / max(size, vec2f(1.0));
    let sample_pos = select(centered, stretched, u.stretch != 0u);

    var matte = vec4f(0.0);
    if all(sample_pos >= vec2f(0.0)) && all(sample_pos < u.matte_size) {
        let inset = vec2f(0.5 / u.matte_image.scale);
        let sample_center = clamp(sample_pos, inset, max(inset, u.matte_size - inset));
        matte = fx_bilinear(matte_tex, u.matte_image, sample_center + u.matte_image.origin, u.premultiply == 0u);
    }
    var a = fx_channel(matte, u.channel);
    if u.invert != 0u { a = 1.0 - a; }
    if u.composite != 0u { a *= c.a; }
    textureStore(dst, id.xy, fx_output(c.rgb, a, u.is_float != 0u));
}
