fn shift_channels(c: vec4f, px: BaePixel) -> vec4f {
    let s = bae_input_straight(c);
    let a = fx_channel(s, params.alpha);
    let rgb = vec3f(fx_channel(s, params.red), fx_channel(s, params.green),
                   fx_channel(s, params.blue));
    return vec4f(rgb * a, a);
}
