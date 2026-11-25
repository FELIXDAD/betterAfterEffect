fn channel_mixer(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = vec4<f32>(bae_unpremultiply(c).rgb, 1.0);
    let r = dot(s, vec4<f32>(params.red_red, params.red_green, params.red_blue, params.red_const));
    let g = dot(s, vec4<f32>(params.green_red, params.green_green, params.green_blue, params.green_const));
    let b = dot(s, vec4<f32>(params.blue_red, params.blue_green, params.blue_blue, params.blue_const));
    let mixed = select(vec3<f32>(r,g,b), vec3<f32>(r), params.monochrome != 0u) / 100.;
    return bae_premultiply(vec4<f32>(mixed, c.a));
}
