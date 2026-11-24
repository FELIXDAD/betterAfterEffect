fn color_hue(c: vec3<f32>) -> f32 {
    let hi = max(c.r, max(c.g,c.b));
    let d = hi - min(c.r, min(c.g,c.b));
    if d == 0.0 { return 0.0; }
    if hi == c.r { return fract((c.g-c.b)/(6.0*d)); }
    if hi == c.g { return (2.0+(c.b-c.r)/d)/6.0; }
    return (4.0+(c.r-c.g)/d)/6.0;
}

fn leave_color(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let rgb = saturate(bae_unpremultiply(c).rgb);
    let key = params.color_to_leave.rgb;
    let tolerance = params.tolerance / 100.0;
    let softness = max(params.edge_softness / 100.0, 1e-7);
    let delta = rgb - key;
    let radius = round(255.0 * sqrt(3.0) * tolerance) / 255.0;
    var w = saturate((dot(delta, delta) - radius * radius) / (2.0 * softness));
    if params.match_colors == 2u {
        let hues = round(vec2<f32>(color_hue(rgb), color_hue(key)) * 256.0) / 256.0;
        let d = 2.0 * abs(fract(hues.x - hues.y + 0.5) - 0.5);
        w = saturate((d*d - tolerance*tolerance) * (sqrt(3.0) / 2.0) / softness);
    }
    let gray = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(mix(rgb, vec3<f32>(gray), w*params.amount_to_decolor/100.0)*c.a, c.a);
}
