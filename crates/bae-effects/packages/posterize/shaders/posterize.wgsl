fn posterize(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let levels = f32(params.level);
    let bins = min(floor(saturate(bae_unpremultiply(c).rgb) * levels), vec3<f32>(levels - 1.0));
    return vec4<f32>(bins * (c.a / (levels - 1.0)), c.a);
}
