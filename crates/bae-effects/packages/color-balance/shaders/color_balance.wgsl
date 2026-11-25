// Per-channel tone curves fitted to the AE 26 grayscale ramp. Each
// adjustment works on its channel's code value, independent of the others.
fn color_balance(c: vec4f, px: BaePixel) -> vec4f {
    let straight = bae_input_straight(c);
    let rgb = clamp(straight.rgb, vec3f(0.0), vec3f(1.0));
    let inverse = 1.0 - rgb;
    let shadows = vec3f(params.shadow_red, params.shadow_green, params.shadow_blue) * 0.01;
    let midtones = vec3f(params.midtone_red, params.midtone_green, params.midtone_blue) * 0.01;
    let highlights = vec3f(params.highlight_red, params.highlight_green, params.highlight_blue) * 0.01;
    let shift = 0.392 * shadows * pow(rgb, vec3f(0.603)) * pow(inverse, vec3f(1.103))
        + 0.766 * midtones * pow(rgb, vec3f(0.588)) * pow(inverse, vec3f(1.104))
        + 0.420 * highlights * pow(rgb, vec3f(0.725)) * pow(inverse, vec3f(1.094));
    var out = clamp(rgb + shift, vec3f(0.0), vec3f(1.0));
    if params.preserve != 0u {
        // Preserve Luminosity uses different curve shapes. A fourth-degree
        // fit on the ramp captures the toe and shoulder of all three bands.
        let s = -0.00653 + rgb * (-0.1410 + rgb * (1.366 + rgb * (-1.353 + rgb * 0.5883)));
        let m = -1.2202 + rgb * (4.9752 + rgb * (-8.4245 + rgb * (9.1241 - rgb * 3.8210)));
        let h = 0.3809 + rgb * (0.8607 + rgb * (-2.8451 + rgb * (2.4706 - rgb * 0.8696)));
        out = clamp(rgb + s * shadows + m * midtones + h * highlights, vec3f(0.0), vec3f(1.0));
    }
    let a = clamp(straight.a, 0.0, 1.0);
    return vec4f(out * a, a);
}
