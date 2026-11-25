fn threshold_at(c: vec4<f32>, level: f32) -> vec4<f32> {
    let luma = dot(bae_unpremultiply(c).rgb, vec3<f32>(0.299, 0.587, 0.114));
    var white = luma >= level;
    if (frame.flags & BAE_FRAME_FLOAT) == 0u && level < 1.0 {
        // Integer Threshold rounds the luminance to an 8-bit level.
        white = round(luma * 255.0) > floor(level * 255.0);
    }
    return vec4<f32>(vec3<f32>(select(0.0, c.a, white)), c.a);
}

fn threshold(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    return threshold_at(c, params.level);
}

fn threshold_legacy(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    return threshold_at(c, params.level / 255.0);
}
