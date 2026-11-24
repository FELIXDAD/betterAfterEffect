fn noise(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let seed = frame.seed ^ bitcast<u32>(frame.frame_index);
    let r = bae_hash(seed, px.pos);
    let samples = select(vec3<f32>(r), vec3<f32>(r, bae_hash(seed + 1u, px.pos), bae_hash(seed + 2u, px.pos)), params.noise_type != 0u);
    var rgb = bae_unpremultiply(c).rgb + (samples - 0.5) * params.amount_of_noise * 0.01;
    if (frame.flags & BAE_FRAME_FLOAT) == 0u {
        rgb = select(fract(rgb), saturate(rgb), params.clipping != 0u);
    }
    return vec4<f32>(rgb * c.a, c.a);
}
