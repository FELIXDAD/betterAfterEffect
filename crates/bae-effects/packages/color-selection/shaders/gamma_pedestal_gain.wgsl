fn gamma_pedestal_gain(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let rgb = saturate(bae_unpremultiply(c).rgb);
    let gamma = vec3<f32>(params.red_gamma, params.green_gamma, params.blue_gamma);
    let pedestal = vec3<f32>(params.red_pedestal, params.green_pedestal, params.blue_pedestal);
    let gain = vec3<f32>(params.red_gain, params.green_gain, params.blue_gain);
    let level = mix(pedestal, gain, pow(rgb, 1.0 / gamma));
    let shadows = max(1.0 - 2.0 * level, vec3<f32>(0.0));
    let output = level + (params.black_stretch - 1.0) * level * shadows * shadows;
    return vec4<f32>(saturate(output) * c.a, c.a);
}
