// Levels (Individual Controls) (ADBE Pro Levels2, AE-FX-039): per-channel
// levels first, then the RGB composite levels; alpha has its own levels.
// Same mapping as Levels (AE-FX-038).
fn pl_map(x: f32, ib: f32, iw: f32, g: f32, ob: f32, ow: f32) -> f32 {
    var t: f32;
    if (iw == ib) {
        t = select(0.0, 1.0, x >= ib);
    } else {
        t = (x - ib) / (iw - ib);
    }
    t = pow(max(t, 0.0), 1.0 / max(g, 1e-4));
    var y = ob + (ow - ob) * t;
    if (params.clip_black == 1u) {
        y = max(y, min(ob, ow));
    }
    if (params.clip_white == 1u) {
        y = min(y, max(ob, ow));
    }
    return y;
}

fn pl_rgb(x: f32) -> f32 {
    return pl_map(x, params.rgb_input_black, params.rgb_input_white, params.rgb_gamma,
        params.rgb_output_black, params.rgb_output_white);
}

fn pro_levels(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    let r = pl_map(s.r, params.red_input_black, params.red_input_white, params.red_gamma,
        params.red_output_black, params.red_output_white);
    let g = pl_map(s.g, params.green_input_black, params.green_input_white, params.green_gamma,
        params.green_output_black, params.green_output_white);
    let b = pl_map(s.b, params.blue_input_black, params.blue_input_white, params.blue_gamma,
        params.blue_output_black, params.blue_output_white);
    let a = clamp(pl_map(s.a, params.alpha_input_black, params.alpha_input_white,
        params.alpha_gamma, params.alpha_output_black, params.alpha_output_white), 0.0, 1.0);
    let rgb = vec3<f32>(pl_rgb(r), pl_rgb(g), pl_rgb(b));
    return vec4<f32>(rgb * a, a);
}
