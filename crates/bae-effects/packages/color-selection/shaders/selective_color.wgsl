fn correction(rgb: vec3<f32>, ink: vec4<f32>, weight: f32) -> vec3<f32> {
    let delta = -(ink.rgb + ink.a * (1.0 + ink.rgb));
    let room = 1.0-rgb;
    let adjusted = select(delta, delta*room, params.method == 1u);
    return clamp(adjusted, -rgb, 1.0-rgb)*weight;
}

fn selective_color(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let rgb = saturate(bae_unpremultiply(c).rgb);
    let maximum = max(rgb.r,max(rgb.g,rgb.b));
    let minimum = min(rgb.r,min(rgb.g,rgb.b));
    let middle = rgb.r+rgb.g+rgb.b-maximum-minimum;
    let weights = array<f32,9>(
        select(0., maximum-middle, rgb.r == maximum), select(0., middle-minimum, rgb.b == minimum),
        select(0., maximum-middle, rgb.g == maximum), select(0., middle-minimum, rgb.r == minimum),
        select(0., maximum-middle, rgb.b == maximum), select(0., middle-minimum, rgb.g == minimum),
        max(2.*minimum-1.,0.), 1.-abs(maximum-0.5)-abs(minimum-0.5), max(1.-2.*maximum,0.));
    let inks = array<vec4<f32>,9>(
        vec4<f32>(params.reds_cyan, params.reds_magenta, params.reds_yellow, params.reds_black),
        vec4<f32>(params.yellows_cyan, params.yellows_magenta, params.yellows_yellow, params.yellows_black),
        vec4<f32>(params.greens_cyan, params.greens_magenta, params.greens_yellow, params.greens_black),
        vec4<f32>(params.cyans_cyan, params.cyans_magenta, params.cyans_yellow, params.cyans_black),
        vec4<f32>(params.blues_cyan, params.blues_magenta, params.blues_yellow, params.blues_black),
        vec4<f32>(params.magentas_cyan, params.magentas_magenta, params.magentas_yellow, params.magentas_black),
        vec4<f32>(params.whites_cyan, params.whites_magenta, params.whites_yellow, params.whites_black),
        vec4<f32>(params.neutrals_cyan, params.neutrals_magenta, params.neutrals_yellow, params.neutrals_black),
        vec4<f32>(params.blacks_cyan, params.blacks_magenta, params.blacks_yellow, params.blacks_black));
    let selected = vec4<f32>(params.cyan, params.magenta, params.yellow, params.black);
    var result = rgb;
    for (var i=0u; i<9u; i++) {
        let values = select(inks[i], selected, params.colors == i+1u && any(selected != vec4<f32>(0)));
        result += correction(rgb, values/100.0, weights[i]);
    }
    return vec4<f32>(saturate(result)*c.a,c.a);
}
