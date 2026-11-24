fn broadcast_colors(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let rgb = saturate(bae_unpremultiply(c).rgb);
    let input = vec4<f32>(rgb * c.a, c.a);
    let ntsc = params.broadcast_locale == 1u;
    let gamma = select(2.8, 2.2, ntsc);
    let setup = select(0.0, 7.5, ntsc);
    let signal = pow(rgb, vec3<f32>(1.0 / gamma));
    let luma = dot(signal, vec3<f32>(0.299, 0.587, 0.114));
    let chroma = length((signal.br - luma) * vec2<f32>(0.493, 0.877));
    let peak = (params.maximum_signal_amplitude_ire - setup) / (100.0 - setup);
    let chroma_limit = 53.0 / (100.0 - setup);
    // The key switches at discrete signal levels even with an 8-bit input ramp.
    let levels = vec2<u32>(vec2<f32>(luma + chroma, chroma) * 1024.0);
    let limits = vec2<u32>(vec2<f32>(peak, chroma_limit) * 1024.0);
    let unsafe_color = chroma > 1e-6 && any(levels > limits);
    let mode = params.how_to_make_color_safe;
    if mode >= 4u {
        return select(input, vec4<f32>(0.0), unsafe_color == (mode == 4u));
    }
    if !unsafe_color { return input; }
    var safe: vec3<f32>;
    if mode == 2u {
        let gain = saturate(min(peak - luma, chroma_limit) / chroma);
        safe = luma + (signal - luma) * gain;
    } else {
        safe = signal * min(peak / (luma + chroma), chroma_limit / chroma);
    }
    return vec4<f32>(pow(saturate(safe), vec3<f32>(gamma)) * c.a, c.a);
}
