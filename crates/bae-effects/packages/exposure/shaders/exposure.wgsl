// Exposure (ADBE Exposure2, AE-FX-043): in linear light (unless bypassed),
// x * 2^exposure + offset, then gamma with negative values mirrored around 0.
// Alpha unchanged.
fn exposure_ch(x0: f32, e: f32, o: f32, g: f32) -> f32 {
    let x = x0 * exp2(e) + o;
    return sign(x) * pow(abs(x), 1.0 / max(g, 1e-4));
}

fn exposure(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    var v = s;
    if (params.bypass_linear == 0u) {
        v = bae_to_linear(s);
    }
    if (params.channels == 1u) {
        let e = params.master_exposure;
        let o = params.master_offset;
        let g = params.master_gamma;
        v = vec4<f32>(exposure_ch(v.r, e, o, g), exposure_ch(v.g, e, o, g),
            exposure_ch(v.b, e, o, g), v.a);
    } else {
        v = vec4<f32>(
            exposure_ch(v.r, params.red_exposure, params.red_offset, params.red_gamma),
            exposure_ch(v.g, params.green_exposure, params.green_offset, params.green_gamma),
            exposure_ch(v.b, params.blue_exposure, params.blue_offset, params.blue_gamma),
            v.a);
    }
    if (params.bypass_linear == 0u) {
        v = bae_from_linear(v);
    }
    return vec4<f32>(v.rgb * c.a, c.a);
}
