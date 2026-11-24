fn strobe_random(index: i32) -> f32 {
    var bits = bitcast<u32>(index) * 0x9e3779b9u + u32(params.random_seed);
    bits = (bits ^ (bits >> 16u)) * 0x7feb352du;
    bits = (bits ^ (bits >> 15u)) * 0x846ca68bu;
    return f32((bits ^ (bits >> 16u)) >> 8u) / 16777216.0;
}

fn strobe_color(c: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    switch params.operation {
        case 2: { return c + light; }
        case 3: { return c - light; }
        case 4: { return c * light; }
        case 5: { return abs(c - light); }
        case 6, 7, 8: {
            let a = vec3<u32>(round(saturate(c) * 255.0));
            let b = vec3<u32>(round(saturate(light) * 255.0));
            let bits = select(select(a ^ b, a | b, params.operation == 7), a & b, params.operation == 6);
            return vec3<f32>(bits) / 255.0;
        }
        case 9, 10: {
            let weights = vec3<f32>(0.299, 0.587, 0.114);
            let lighter = dot(c, weights) > dot(light, weights);
            return select(light, c, lighter == (params.operation == 9));
        }
        case 11: { return min(c, light); }
        case 12: { return max(c, light); }
        case 13: { return c + light - c * light; }
        default: { return light; }
    }
}

fn strobe(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let period = params.period;
    if period == 0.0 { return c; }

    let jitter = (2.0 * strobe_random(frame.frame_index) - 1.0) * (params.probability * 0.01);
    let phase = fract(frame.time / period + jitter) * period;
    if phase > 0.0 && phase >= params.duration { return c; }

    let blend = params.blend * 0.01;
    if params.mode == 2 { return c * blend; }
    let color = strobe_color(bae_unpremultiply(c).rgb, params.color.rgb);
    return mix(bae_depth_clamp(vec4<f32>(color * c.a, c.a)), c, blend);
}
