// Exposure (ADBE Exposure2, AE-FX-043): in linear light (unless bypassed),
// x * 2^exposure + offset, then gamma with negative values mirrored around 0.
// Alpha unchanged.
//
// Without a linearized working space AE linearizes with a pure 2.4 power curve,
// not the piecewise sRGB curve (fitted to AE 26 renders: max error 0.5 of 255
// for 2.4, 8.6 for 2.2, 5 for sRGB).
fn exposure_pow(c: vec4<f32>, p: f32) -> vec4<f32> {
    if ((frame.flags & BAE_FRAME_LINEAR) != 0u) {
        return c;
    }
    return vec4<f32>(sign(c.rgb) * pow(abs(c.rgb), vec3<f32>(p)), c.a);
}

fn exposure_ch(x0: f32, e: f32, o: f32, g: f32) -> f32 {
    let x = x0 * exp2(e) + o;
    return sign(x) * pow(abs(x), 1.0 / max(g, 1e-4));
}

fn exposure(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    var v = s;
    if (params.bypass_linear == 0u) {
        v = exposure_pow(s, 2.4);
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
        v = exposure_pow(v, 1.0 / 2.4);
    }
    return vec4<f32>(v.rgb * c.a, c.a);
}
