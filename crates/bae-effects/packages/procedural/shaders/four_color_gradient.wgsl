fn four_color_gradient(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let p = vec2<f32>(px.pos);
    let distances = vec4<f32>(distance(p, params.point_1), distance(p, params.point_2),
        distance(p, params.point_3), distance(p, params.point_4));
    // AE's Blend adds a squared radius in the denominator of inverse-square weights.
    let radius = bae_scale(params.blend);
    let weights = 1.0 / (distances * distances + radius * radius);
    let rgb = (weights.x * params.color_1.rgb + weights.y * params.color_2.rgb +
        weights.z * params.color_3.rgb + weights.w * params.color_4.rgb) / dot(weights, vec4<f32>(1));
    let jitter = (bae_hash(0u, px.pos) - 0.5) * params.jitter / 255.0;
    return c.a * generated_blend(vec4<f32>(bae_unpremultiply(c).rgb, 1.0), rgb + jitter, params.opacity * 0.01, params.blending_mode);
}
