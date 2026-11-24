fn gradient_ramp(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let delta = params.end_of_ramp - params.start_of_ramp;
    let offset = vec2<f32>(px.pos) - params.start_of_ramp;
    let length_sq = max(dot(delta, delta), 1e-12);
    let t = select(dot(delta, offset) / length_sq, length(offset) * inverseSqrt(length_sq), params.ramp_shape == 2u);
    let scatter = (bae_hash(0u, px.pos) - 0.5) * params.ramp_scatter / 255.0;
    let color = mix(params.start_color.rgb, params.end_color.rgb, saturate(t + scatter));
    return mix(vec4<f32>(color * c.a, c.a), c, params.blend_with_original);
}
