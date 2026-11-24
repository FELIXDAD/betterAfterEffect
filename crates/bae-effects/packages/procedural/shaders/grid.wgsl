fn grid(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    var size = abs(params.corner - params.anchor);
    if params.size_from != 1u {
        size = vec2<f32>(params.width, select(params.height, params.width, params.size_from == 2u)) * frame.downsample;
    }
    size = max(size, vec2<f32>(0.0001));
    let q = (vec2<f32>(px.pos) - params.anchor) / size;
    let d = abs(q - round(q)) * size;
    let feather = vec2<f32>(params.feather_width, params.feather_height) * frame.downsample;
    // AE feathers inwards. Integrate a one-pixel footprint at hard edges.
    let half_width = max(0.0, (bae_scale(params.border) - 1.0) * 0.5);
    let hard = saturate(0.5 + half_width - d);
    let soft = pow(saturate((half_width - d) / max(feather, vec2<f32>(0.0001))), vec2<f32>(4.0));
    let coverage = select(hard, soft, feather > vec2<f32>(0.0));
    let lines = select(0.0, saturate(coverage.x + coverage.y), params.border > 0.0);
    let alpha = select(lines, 1.0 - lines, params.invert_grid != 0u) * params.opacity * 0.01;
    if params.blending_mode == 3u { return c * alpha; }
    return generated_blend(c, params.color.rgb, alpha, select(params.blending_mode, params.blending_mode - 1u, params.blending_mode > 3u));
}
