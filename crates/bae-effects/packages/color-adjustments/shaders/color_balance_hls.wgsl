fn color_balance_hls(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let src = bae_unpremultiply(c).rgb;
    let light = params.lightness / 100.0;
    let adjusted = mix(src, vec3<f32>(select(0.0, 1.0, light > 0.0)), abs(light));
    let hls = cc_hls(adjusted);
    let saturation = select(0.0, cc_shift(hls.z, params.saturation / 100.0), hls.z > 0.0);
    let rgb = cc_rgb(vec3<f32>(hls.x + params.hue / 360.0, hls.y, saturation));
    return vec4<f32>(rgb * c.a, c.a);
}
