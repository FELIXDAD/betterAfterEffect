// Brightness & Contrast (ADBE Brightness & Contrast 2, AE-FX-042), straight
// colour, alpha unchanged.
//
// Modern mode: the Photoshop-style tone curve (Skottie's fit of AE; the exact
// AE curve is UNVERIFIED), values clamped to 0..1. Legacy mode: linear, keeps
// HDR values.
const BC_PI: f32 = 3.14159265358979;

fn bc_modern(x0: f32, b: f32, c: f32) -> f32 {
    var x = clamp(x0, 0.0, 1.0);
    x = 1.0 - pow(1.0 - x, exp2(1.8 * b));
    x = x + (-c / (3.0 * BC_PI)) * sin(2.0 * BC_PI * x);
    return clamp(x, 0.0, 1.0);
}

fn brightness_contrast(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    var rgb: vec3<f32>;
    if (params.use_legacy != 0u) {
        let b = clamp(params.brightness, -100.0, 100.0) / 255.0;
        let k = clamp(params.contrast, -100.0, 100.0) / 100.0;
        var gain = 1.0 + k;
        if (k > 0.0) {
            gain = 1.0 / max(1.0 - k, 1e-4);
        }
        rgb = gain * s.rgb + vec3<f32>(0.5 * (1.0 - gain) + b * max(gain, 1.0));
    } else {
        let b = clamp(params.brightness, -150.0, 150.0) / 150.0;
        let k = clamp(params.contrast, -50.0, 100.0) / 100.0;
        rgb = vec3<f32>(bc_modern(s.r, b, k), bc_modern(s.g, b, k), bc_modern(s.b, b, k));
    }
    return vec4<f32>(rgb * c.a, c.a);
}
