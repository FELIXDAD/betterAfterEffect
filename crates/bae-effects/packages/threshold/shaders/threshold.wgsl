fn threshold_at(c: vec4<f32>, level: f32) -> vec4<f32> {
    let luma = dot(bae_unpremultiply(c).rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4f(vec3f(c.a * threshold_step(luma, level)), c.a);
}

fn threshold(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    return threshold_at(c, params.level);
}

fn threshold_legacy(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    return threshold_at(c, params.level / 255.0);
}
