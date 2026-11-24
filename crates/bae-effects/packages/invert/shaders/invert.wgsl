fn invert(c: vec4f, px: BaePixel) -> vec4f {
    let s = bae_input_straight(c);
    let mode = params.channel;
    var rgb = s.rgb;
    if mode <= 4u {
        let channels = vec3u(2u, 3u, 4u);
        let selected = channels == vec3u(mode);
        rgb = select(select(rgb, 1.0 - rgb, selected), 1.0 - rgb, mode == 1u);
    } else if mode >= 6u && mode <= 9u {
        var hsl = fx_hsl(rgb);
        if mode == 6u || mode == 7u {
            hsl.x = fract(0.5 - hsl.x);
        }
        if mode == 6u || mode == 8u {
            hsl.z = 1.0 - hsl.z;
        }
        if mode == 6u || mode == 9u {
            let chroma = max(rgb.r, max(rgb.g, rgb.b)) - min(rgb.r, min(rgb.g, rgb.b));
            let gamut = 1.0 - abs(2.0 * hsl.z - 1.0);
            hsl.y = clamp((1.0 - chroma) / max(gamut, 1e-6), 0.0, 1.0);
        }
        rgb = fx_rgb(hsl);
    } else if mode >= 11u && mode <= 14u {
        var yiq = vec3f(fx_luma(rgb),
                        dot(rgb, vec3f(0.5959, -0.2746, -0.3213)),
                        dot(rgb, vec3f(0.2115, -0.5227, 0.3112)));
        if mode == 11u || mode == 12u { yiq.x = 1.0 - yiq.x; }
        if mode == 11u || mode == 13u { yiq.y = -yiq.y; }
        if mode == 11u || mode == 14u { yiq.z = -yiq.z; }
        rgb = vec3f(dot(yiq, vec3f(1.0, 0.9561, 0.6207)),
                    dot(yiq, vec3f(1.0, -0.2722, -0.6472)),
                    dot(yiq, vec3f(1.0, -1.1069, 1.7048)));
    } else if mode == 16u {
        let a = mix(1.0 - s.a, s.a, params.blend * 0.01);
        return vec4f(rgb * a, a);
    }
    // In integer projects AE clips the inverted channels before blending.
    if (frame.flags & BAE_FRAME_FLOAT) == 0u {
        rgb = clamp(rgb, vec3f(0.0), vec3f(1.0));
    }
    return vec4f(mix(rgb, s.rgb, params.blend * 0.01) * s.a, s.a);
}
