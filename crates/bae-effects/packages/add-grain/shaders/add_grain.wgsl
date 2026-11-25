fn grain_field(pos: vec2<f32>, scale: f32, seed: u32) -> f32 {
    let stock_size = select(select(0.75, 1.25, params.preset == 5u), 2.0, params.preset == 1u);
    let uv = pos / max(vec2<f32>(0.01), scale * stock_size * vec2<f32>(params.aspect_ratio, 1.0));
    return grain_lattice(uv,seed,params.softness);
}

// Midtone RGB deviations relative to the default grain, measured on the AE ramp.
fn stock_intensity(preset: u32) -> vec3<f32> {
    switch preset {
        case 3u: { return vec3<f32>(3.641, 5.123, 5.952); }
        case 4u: { return vec3<f32>(4.137, 2.055, 4.352); }
        case 5u: { return vec3<f32>(2.546, 1.218, 3.226); }
        case 6u: { return vec3<f32>(2.096, 2.974, 4.584); }
        case 7u: { return vec3<f32>(4.384, 3.071, 3.111); }
        case 8u: { return vec3<f32>(2.972, 3.081, 5.471); }
        case 9u: { return vec3<f32>(2.963, 3.353, 4.896); }
        case 10u: { return vec3<f32>(4.800, 3.747, 7.634); }
        case 11u: { return vec3<f32>(2.569, 4.057, 6.544); }
        case 12u: { return vec3<f32>(4.865, 5.531, 7.205); }
        case 13u: { return vec3<f32>(1.867, 2.378, 4.018); }
        case 14u: { return vec3<f32>(4.556, 4.681, 7.116); }
        case 15u: { return vec3<f32>(4.390, 3.281, 5.307); }
        default: { return vec3<f32>(1); }
    }
}

fn channel_grain(pos: vec2<f32>, scale: f32, channel: u32) -> f32 {
    let time = frame.time * frame.frame_rate * params.animation_speed;
    let seed = u32(params.random_seed) + channel*41u + bitcast<u32>(i32(floor(time)))*11u;
    let first = grain_field(pos, scale * params.size, seed);
    if params.animate_smoothly == 0u { return first; }
    return mix(first, grain_field(pos, scale * params.size, seed+11u), smoothstep(0.0,1.0,fract(time)));
}

fn match_difference(rgb: vec3<f32>, reference: vec3<f32>) -> f32 {
    let delta = abs(rgb - reference);
    switch params.match_color_using {
        case 4u: {
            let weights = vec3<f32>(1.0,-0.5,-0.5);
            let a = atan2(0.866*(rgb.g-rgb.b), dot(weights,rgb));
            let b = atan2(0.866*(reference.g-reference.b), dot(weights,reference));
            return abs(fract((a-b)/6.2831853 + 0.5)-0.5)*2.0;
        }
        case 5u: { return abs((max(rgb.r,max(rgb.g,rgb.b))-min(rgb.r,min(rgb.g,rgb.b))) -
                              (max(reference.r,max(reference.g,reference.b))-min(reference.r,min(reference.g,reference.b)))); }
        case 6u: { return abs(dot(rgb-reference,vec3<f32>(0.299,0.587,0.114))); }
        case 7u: { return delta.r; }
        case 8u: { return delta.g; }
        case 9u: { return delta.b; }
        default: { return max(delta.r,max(delta.g,delta.b)); }
    }
}

fn match_weight(rgb: vec3<f32>) -> f32 {
    let d = match_difference(rgb, params.matching_color.rgb);
    let matched = 1.0 - smoothstep(params.matching_tolerance,
        params.matching_tolerance + max(0.0001,params.matching_softness),d);
    return select(matched, 1.0-matched, params.invert_match != 0u);
}

fn matching_matte(rgb: vec3<f32>, p: vec2<i32>) -> f32 {
    if params.blur_matte == 0.0 { return match_weight(rgb); }
    let weights = array<f32,5>(1.0,4.0,6.0,4.0,1.0);
    let spacing = max(0.5, bae_scale(params.blur_matte) * 0.5);
    var value = 0.0;
    for (var y=0; y<5; y++) {
        for (var x=0; x<5; x++) {
            let delta = vec2<f32>(f32(x-2),f32(y-2))*spacing;
            let sample = bae_sample_input(vec2<f32>(p)+0.5+delta);
            value += weights[x]*weights[y]*match_weight(bae_unpremultiply(sample).rgb);
        }
    }
    return value/256.0;
}

@compute @workgroup_size(8,8)
fn add_grain(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let c = bae_load_input(p);
    let rgb = bae_unpremultiply(c).rgb;
    let pos = bae_to_layer(vec2<f32>(p) + 0.5);
    var retain = params.amount * 0.01;
    if retain > 0.0 && params.match_color_using > 2u {
        retain *= matching_matte(rgb, p);
    }
    if params.viewing_mode == 2u { bae_store(p,vec4<f32>(vec3<f32>(retain),1)); return; }
    if params.viewing_mode == 1u {
        let edge = abs(pos - bae_to_layer(params.center)) - 0.5*vec2<f32>(params.width,params.height);
        let d = max(edge.x,edge.y);
        if d >= 0.0 { bae_store(p,c); return; }
        if d >= -2.0 && params.show_box != 0u { bae_store(p,params.box_color); return; }
    }
    var grain = vec3<f32>(channel_grain(pos,params.red_size,0u),
                          channel_grain(pos,params.green_size,1u),channel_grain(pos,params.blue_size,2u));
    let mono = dot(grain,vec3<f32>(0.299,0.587,0.114));
    grain = mix(vec3<f32>(mono), grain, select(params.saturation,0.0,params.monochromatic != 0u));
    let lum = dot(rgb,vec3<f32>(0.299,0.587,0.114));
    let lows = saturate(1.0 - lum/max(0.0001,params.midpoint));
    let highs = saturate((lum-params.midpoint)/max(0.0001,1.0-params.midpoint));
    let shadows = vec3<f32>(params.red_shadows,params.green_shadows,params.blue_shadows)*params.shadows;
    let mids = vec3<f32>(params.red_midtones,params.green_midtones,params.blue_midtones)*params.midtones;
    let highlights = vec3<f32>(params.red_highlights,params.green_highlights,params.blue_highlights)*params.highlights;
    grain *= (lows*shadows + (1.0-lows-highs)*mids + highs*highlights)
        * vec3<f32>(params.red_intensity,params.green_intensity,params.blue_intensity)
        * mix(vec3<f32>(1),params.tint_color.rgb,params.tint_amount*0.01) * (vec3<f32>(0.032,0.029,0.033)*params.intensity) * stock_intensity(params.preset);
    switch params.blending_mode {
        case 1u: { grain *= sqrt(max(vec3<f32>(0.01),4.0*rgb*(1.0-rgb))); }
        case 2u: { grain *= rgb; }
        case 4u: { grain *= 1.0-rgb; }
        case 5u: { grain *= 2.0*min(rgb,1.0-rgb); }
        default: {}
    }
    bae_store(p,bae_depth_clamp(vec4<f32>((rgb + (1.0-retain)*grain)*c.a,c.a)));
}
