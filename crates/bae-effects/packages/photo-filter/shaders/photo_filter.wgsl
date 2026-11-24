// Linear filter colors for the 20 AE presets, fitted to the RGB grid.
// Temperature presets can have coefficients outside display gamut.
const FILTER_COLORS = array<vec3f, 20>(
    vec3f(0.840653, 0.256482, -0.050174),
    vec3f(0.957165, 0.304958, -0.056703),
    vec3f(0.831240, 0.440219, 0.007384),
    vec3f(-0.300970, 0.149982, 1.696585),
    vec3f(-0.084522, 0.107724, 1.804264),
    vec3f(-0.790284, 0.458325, 2.251649),
    vec3f(0.822874, 0.011005, 0.010998),
    vec3f(0.896582, 0.231637, 0.009353),
    vec3f(0.950213, 0.769250, 0.013002),
    vec3f(0.009856, 0.584152, 0.010030),
    vec3f(0.012365, 0.596190, 0.821379),
    vec3f(0.011914, 0.034639, 0.821260),
    vec3f(0.327388, 0.011321, 0.822006),
    vec3f(0.767552, 0.008687, 0.767638),
    vec3f(0.412129, 0.194964, 0.033485),
    vec3f(1.293696, -0.126765, -0.006859),
    vec3f(-0.141220, 0.015342, 0.612655),
    vec3f(-0.148166, 0.264740, -0.033347),
    vec3f(1.017467, 0.668948, -0.081837),
    vec3f(-0.017593, 0.536062, 0.436503),
);

// Red and green unit filter responses from AE's custom color mode. The
// blue response is I - R - G, so an uncolored white filter is neutral.
const RED_FILTER = mat3x3f(
    vec3f(0.536818, -0.098288, 0.003188),
    vec3f(0.287432, 0.134576, -0.021681),
    vec3f(0.175441, -0.037013, 0.017702),
);
const GREEN_FILTER = mat3x3f(
    vec3f(0.287246, 0.135417, -0.021720),
    vec3f(-0.354892, 0.833873, -0.090604),
    vec3f(0.067595, 0.030010, 0.111889),
);

fn photo_gamma(color: vec3f, power: f32) -> vec3f {
    return sign(color) * pow(abs(color), vec3f(power));
}

// Preserve Luminosity restores brightness on clamped, gamma-encoded RGB.
// Shifting by a neutral offset preserves chroma until a component clips;
// scale the chroma back at that point to retain the target brightness.
fn photo_set_luminosity(filtered: vec3f, original: vec3f) -> vec3f {
    let weights = vec3f(0.30, 0.59, 0.11);
    let lo = min(0.0, min(original.r, min(original.g, original.b)));
    let hi = max(1.0, max(original.r, max(original.g, original.b)));
    let color = clamp(filtered, vec3f(lo), vec3f(hi));
    let luminosity = dot(original, weights);
    let delta = color - dot(color, weights);
    let dark = min(delta.r, min(delta.g, delta.b));
    let bright = max(delta.r, max(delta.g, delta.b));
    var scale = 1.0;
    if luminosity + dark < lo {
        scale = (lo - luminosity) / dark;
    }
    if luminosity + bright > hi {
        scale = min(scale, (hi - luminosity) / bright);
    }
    return vec3f(luminosity) + scale * delta;
}

fn photo_filter(c: vec4f, px: BaePixel) -> vec4f {
    let straight = bae_input_straight(c);
    let is_linear = (frame.flags & BAE_FRAME_LINEAR) != 0u;
    var filter_color: vec3f;
    if params.preset <= 20u {
        filter_color = FILTER_COLORS[params.preset - 1u];
    } else {
        filter_color = photo_gamma(params.color.rgb, 2.4);
    }

    let rgb = straight.rgb;
    let linear = select(photo_gamma(rgb, 2.4), rgb, is_linear);
    let filtered = linear * filter_color.b
        + (RED_FILTER * linear) * (filter_color.r - filter_color.b)
        + (GREEN_FILTER * linear) * (filter_color.g - filter_color.b);
    let mixed = mix(linear, filtered, params.density * 0.01);
    var out = mixed;
    if !is_linear || params.preserve != 0u {
        out = photo_gamma(out, 1.0 / 2.4);
    }
    if params.preserve != 0u {
        let original = select(rgb, photo_gamma(rgb, 1.0 / 2.4), is_linear);
        out = photo_set_luminosity(out, original);
        if is_linear {
            out = photo_gamma(out, 2.4);
        }
    }
    return vec4f(out * straight.a, straight.a);
}
