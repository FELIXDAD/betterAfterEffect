fn change_color(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let rgb = bae_unpremultiply(c).rgb;
    let key = params.color_to_change.rgb;
    let hls = cc_hls(rgb);
    let delta = (rgb - key) * (255.0 / 256.0);
    let tolerance = params.matching_tolerance / 100.0;
    let softness = params.matching_softness / 100.0;
    // AE ramps the mask linearly in squared distance. Each comparison
    // space has its own scale; the chroma metric is invariant to grey shifts.
    var distance2 = 0.5 * dot(delta, delta);
    var scale = 1.5;
    if params.match_colors == 2u {
        let d = 2.0 * abs(fract(hls.x - cc_hls(key).x + 0.5) - 0.5) * (255.0 / 256.0);
        scale = sqrt(3.0) * 0.5;
        distance2 = d*d*scale;
    }
    if params.match_colors == 3u {
        let uv = delta.rb - delta.g;
        scale = sqrt(6.0);
        distance2 = 0.358*uv.x*uv.x - 0.183*uv.x*uv.y + 0.1995*uv.y*uv.y;
    }
    var w = cc_weight(distance2, tolerance * tolerance * scale, softness);
    if params.invert_color_correction_mask != 0u { w = 1.0-w; }
    if params.view == 2u {
        return select(vec4<f32>(0), vec4<f32>(vec3<f32>(w), 1), w > 0.0 && c.a > 0.0);
    }
    let changed = cc_rgb(vec3<f32>(hls.x + params.hue_transform/360.0,
        clamp(hls.yz + vec2<f32>(params.lightness_transform, params.saturation_transform)/100.0,
              vec2<f32>(0), vec2<f32>(1))));
    return vec4<f32>(mix(rgb, changed, w)*c.a, c.a);
}
