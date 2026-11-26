fn composite_generator(p: vec2i, rgb: vec3f) -> vec4f {
    let base = texel(p);
    let result = generated_blend(vec4f(bae_unpremultiply(base).rgb,1),rgb,
        params.opacity*0.01,params.blending_mode);
    return bae_depth_clamp(base.a*result);
}
