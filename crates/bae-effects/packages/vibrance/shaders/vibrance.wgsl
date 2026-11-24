// AE 26's chart separates a nonlinear vibrance adjustment from a
// saturation pass in approximately gamma-2.26 linearized RGB.
fn vibrance(c: vec4f, px: BaePixel) -> vec4f {
    let straight = bae_input_straight(c);
    var rgb = clamp(straight.rgb, vec3f(0.0), vec3f(1.0));
    let hi = max(rgb.r, max(rgb.g, rgb.b));
    let lo = min(rgb.r, min(rgb.g, rgb.b));
    let hue = fx_hsl(rgb).x;
    let skin = 1.0 - smoothstep(0.04, 0.16, abs(fract(hue + 0.43) - 0.5));
    let distance = (hi - rgb) / max(hi, 1e-6);
    rgb = clamp(rgb - 0.008 * params.vibrance * (1.0 - 0.09 * skin)
                    * lo * distance * distance, vec3f(0.0), vec3f(1.0));
    if params.saturation != 0.0 {
        let linear = pow(rgb, vec3f(2.26));
        let gray = dot(linear, vec3f(0.286, 0.715, -0.001));
        rgb = pow(clamp(vec3f(gray) + (linear - gray) * (1.0 + 0.01 * params.saturation),
                        vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.26));
    }
    let a = clamp(straight.a, 0.0, 1.0);
    return vec4f(rgb * a, a);
}
