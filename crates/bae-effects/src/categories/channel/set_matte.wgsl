fn evaluate(pixel:vec2f) -> vec4f {
    if !has(0u) { return fx_sample(input_tex,u.images[0],pixel,false); }
    let c = fx_sample(input_tex,u.images[0],pixel,true);
    let matte_size = vec2f(u.images[1].size)/u.images[1].scale;
    let channels = array<u32,10>(2,3,4,1,5,6,7,8,9,10);
    let size = vec2f(u.layer.zw - u.layer.xy);
    let local = pixel - vec2f(u.layer.xy);
    let centered = local - (size - matte_size) * 0.5;
    let stretched = local * matte_size / max(size, vec2f(1.0));
    let sample_pos = select(centered, stretched, enabled(4u));

    var matte = vec4f(0.0);
    if all(sample_pos >= vec2f(0.0)) && all(sample_pos < matte_size) {
        let inset = vec2f(0.5 / u.images[1].scale);
        let sample_center = clamp(sample_pos, inset, max(inset, matte_size - inset));
        matte = fx_bilinear(map_a, u.images[1], sample_center + u.images[1].origin, !enabled(6u));
    }
    var a = fx_channel(matte, channels[clamp(u32(value(2u)),1u,10u)-1u]);
    if enabled(3u) { a = 1.0 - a; }
    if enabled(5u) { a *= c.a; }
    return output(c.rgb,a);
}
