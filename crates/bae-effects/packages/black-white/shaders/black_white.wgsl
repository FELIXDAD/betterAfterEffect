// AE processes Black & White at up to 16 bpc.
fn black_white(c: vec4f, px: BaePixel) -> vec4f {
    let straight = bae_input_straight(c);
    let rgb = clamp(straight.rgb, vec3f(0.0), vec3f(1.0));
    let base = min(rgb.r, min(rgb.g, rgb.b));
    let d = rgb - base;
    let secondary = min(d, d.gbr);
    let primary = d - secondary - secondary.brg;
    let gray = clamp(base + 0.01 * (
        dot(primary, vec3f(params.reds, params.greens, params.blues))
        + dot(secondary, vec3f(params.yellows, params.cyans, params.magentas))), 0.0, 1.0);
    var out = vec3f(gray);
    if params.tint != 0u {
        out += params.tint_color.rgb - fx_luma(params.tint_color.rgb);
        let lo = min(out.r, min(out.g, out.b));
        if lo < 0.0 {
            out = vec3f(gray) + (out - gray) * gray / (gray - lo);
        }
        let hi = max(out.r, max(out.g, out.b));
        if hi > 1.0 {
            out = vec3f(gray) + (out - gray) * (1.0 - gray) / (hi - gray);
        }
    }
    let a = clamp(straight.a, 0.0, 1.0);
    return vec4f(out * a, a);
}
