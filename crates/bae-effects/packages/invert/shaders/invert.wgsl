fn invert(c: vec4f, px: BaePixel) -> vec4f {
    let s = bae_input_straight(c);
    var rgb = s.rgb;
    switch params.channel {
        case 1u: { rgb = 1.0 - rgb; }
        case 2u, 3u, 4u: {
            let i = params.channel - 2u;
            rgb[i] = 1.0 - rgb[i];
        }
        case 6u, 7u, 8u, 9u: {
            var hsl = fx_hsl(rgb);
            if params.channel == 6u || params.channel == 7u {
                hsl.x = fract(0.5 - hsl.x);
            }
            if params.channel == 6u || params.channel == 8u {
                hsl.z = 1.0 - hsl.z;
            }
            if params.channel == 6u || params.channel == 9u {
                let chroma = max(rgb.r, max(rgb.g, rgb.b)) - min(rgb.r, min(rgb.g, rgb.b));
                let gamut = 1.0 - abs(2.0 * hsl.z - 1.0);
                hsl.y = clamp((1.0 - chroma) / max(gamut, 1e-6), 0.0, 1.0);
            }
            rgb = fx_rgb(hsl);
        }
        case 11u, 12u, 13u, 14u: {
            var yiq = vec3f(fx_luma(rgb),
                            dot(rgb, vec3f(0.5959, -0.2746, -0.3213)),
                            dot(rgb, vec3f(0.2115, -0.5227, 0.3112)));
            if params.channel == 11u || params.channel == 12u { yiq.x = 1.0 - yiq.x; }
            if params.channel == 11u || params.channel == 13u { yiq.y = -yiq.y; }
            if params.channel == 11u || params.channel == 14u { yiq.z = -yiq.z; }
            rgb = vec3f(dot(yiq, vec3f(1.0, 0.9561, 0.6207)),
                        dot(yiq, vec3f(1.0, -0.2722, -0.6472)),
                        dot(yiq, vec3f(1.0, -1.1069, 1.7048)));
        }
        case 16u: {
            let a = mix(1.0 - s.a, s.a, params.blend * 0.01);
            return vec4f(rgb * a, a);
        }
        default: {}
    }
    // In integer projects AE clips the inverted channels before blending.
    if (frame.flags & BAE_FRAME_FLOAT) == 0u {
        rgb = clamp(rgb, vec3f(0.0), vec3f(1.0));
    }
    return vec4f(mix(rgb, s.rgb, params.blend * 0.01) * s.a, s.a);
}
