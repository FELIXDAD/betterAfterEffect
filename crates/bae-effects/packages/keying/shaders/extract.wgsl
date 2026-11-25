fn extract(c: vec4f, px: BaePixel) -> vec4f {
    let src = bae_input_straight(c);
    var level = dot(src.rgb, vec3f(0.299, 0.587, 0.114));
    if params.channel > 1u { level = src[params.channel - 2u]; }
    let x = level * 255.0;
    let lo = select(step(params.black_point, x),
        saturate((x - params.black_point) / max(params.black_softness, 0.0001)), params.black_softness > 0.0);
    let hi = select(step(x, params.white_point),
        saturate((params.white_point - x) / max(params.white_softness, 0.0001)), params.white_softness > 0.0);
    return bae_premultiply(src) * select(lo * hi, 1.0 - lo * hi, params.invert != 0u);
}
