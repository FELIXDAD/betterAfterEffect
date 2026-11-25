fn change_to_color(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let rgb = bae_unpremultiply(c).rgb;
    let hls = cc_hls(rgb);
    let from_hls = cc_hls(params.from_color.rgb);
    let to_hls = cc_hls(params.to_color.rgb);
    let hue_delta = fract(hls.x - from_hls.x + 0.5) - 0.5;
    let distance = vec3<f32>(2.0 * abs(hue_delta), abs(hls.yz - from_hls.yz));
    let tolerance = vec3<f32>(params.hue, params.lightness, params.saturation);
    // AE's matte uses a sum of normalized distances. A unit change in
    // lightness or saturation weighs one eighth as much as a half hue turn.
    let ratio = distance / max(tolerance, vec3<f32>(1e-7));
    let w = cc_weight(dot(ratio, vec3<f32>(1, 0.125, 0.125)), 1.0 - params.softness, params.softness);
    if params.view_correction_matte != 0u {
        let a = select(c.a, 1.0, c.a > 0.0 && w == 0.0);
        return vec4<f32>(vec3<f32>(w * c.a), a);
    }

    var result = to_hls;
    if params.change_by == 2u {
        // The transform scales the shortest signed hue shift before the RGB mix.
        let hue_step = fract(to_hls.x - from_hls.x + 0.5) - 0.5;
        result = vec3<f32>(hls.x + hue_step * w,
            clamp(hls.yz + (to_hls.yz - from_hls.yz) * w, vec2<f32>(0), vec2<f32>(1)));
    }
    if params.change == 1u || params.change == 3u { result.y = hls.y; }
    if params.change <= 2u { result.z = hls.z; }
    return vec4<f32>(mix(rgb, cc_rgb(result), w) * c.a, c.a);
}
