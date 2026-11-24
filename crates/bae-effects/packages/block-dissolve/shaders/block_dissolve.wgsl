fn block_visible(cell: vec2<i32>) -> f32 {
    return select(0.0, 1.0, bae_hash(0u, cell) >= params.transition_completion * 0.01);
}

fn block_dissolve(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    if params.transition_completion >= 100.0 { return vec4<f32>(0.0); }
    let size = vec2<f32>(params.block_width, params.block_height);
    let grid = (px.layer_pos - 0.5/frame.downsample) / size;
    let cell = vec2<i32>(floor(grid));
    let offset = fract(grid) - 0.5;
    let neighbor = vec2<i32>(sign(offset));
    let soft = max(vec2<f32>(params.feather), select(vec2<f32>(0.0), size, params.soft_edges_best_quality != 0u));
    let w = saturate(0.5 - (0.5 - abs(offset)) * size / max(vec2<f32>(0.0001), soft));
    let top = mix(block_visible(cell), block_visible(cell + vec2<i32>(neighbor.x, 0)), w.x);
    let bottom = mix(block_visible(cell + vec2<i32>(0, neighbor.y)), block_visible(cell + neighbor), w.x);
    return c * mix(top, bottom, w.y);
}
