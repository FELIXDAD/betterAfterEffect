fn unmult(c: vec4f, px: BaePixel) -> vec4f {
    let src = bae_input_straight(c);
    let white = params.background_color == 2u;
    let signal = select(src.rgb, 1.0 - src.rgb, white);
    let peak = max(signal.r, max(signal.g, signal.b));
    let coverage = saturate((peak - params.black_level) / max(params.softness, 0.00001));
    var rgb = src.rgb;
    if params.remove_color_matting != 0u {
        let straight = signal / max(coverage, 0.00001);
        rgb = select(straight, 1.0 - straight, white);
    }
    if params.clip_hdr_result != 0u { rgb = saturate(rgb); }
    return bae_premultiply(vec4f(rgb, src.a * coverage));
}
