// Without a mask selection, AE uses the input alpha and ignores feather.
fn fill(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let alpha = select(c.a, 1.0 - c.a, params.invert != 0u) * params.opacity;
    return vec4<f32>(params.color.rgb * alpha, alpha);
}
