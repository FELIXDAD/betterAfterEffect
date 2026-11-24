// Lens-filter approximation. Preset RGB values follow the reference table.
const FILTERS = array<u32, 20>(
    0xec8a00u, 0xfa9600u, 0xebb113u, 0x006dffu, 0x005dffu, 0x00b5ffu,
    0xea1a1au, 0xf38417u, 0xf9e31cu, 0x19c919u, 0x1dcbeau, 0x1d35eau,
    0x9b1deau, 0xe318e3u, 0xac7a33u, 0xff0000u, 0x0022cdu, 0x008c00u,
    0xffd500u, 0x00c2b1u,
);

// The Cooling 80 chart is explained by a 3×3 transform in gamma-2.4
// linearized RGB. Use its opponent-color response for the other filter hues.
fn filter_axis(c: vec3f) -> vec3f {
    return vec3f(-0.820 * c.r - 0.025 * c.g - 0.455 * c.b,
                 -0.013 * c.r - 0.862 * c.g + 0.028 * c.b,
                  0.027 * c.r + 0.184 * c.g + 0.490 * c.b);
}

// Preserve Luminosity responds differently on the primary, secondary and
// neutral axes. These two temperature responses are fitted to AE's chart.
// The secondary terms are RG, GB and BR pair minima, and the neutral term
// is their common minimum. Coefficients are deltas from the original pixel.
fn warm_preserve(c: vec3f) -> vec3f {
    let s = min(c, c.gbr);
    let n = min(s.r, s.g);
    return vec3f(
        dot(c, vec3f(-0.0136, 0.0383, 0.0371)) + dot(s, vec3f(0.0913, -0.0090, 0.1347)) + n * 0.0457,
        dot(c, vec3f(0.0, -0.1261, -0.0048)) + dot(s, vec3f(0.1162, -0.0981, 0.0057)) + n * 0.0718,
        dot(c, vec3f(0.0017, -0.0138, -0.2571)) + dot(s, vec3f(0.0180, -0.0186, 0.1989)) + n * 0.1183);
}

fn cool_preserve(c: vec3f) -> vec3f {
    let s = min(c, c.gbr);
    let n = min(s.r, s.g);
    return vec3f(
        dot(c, vec3f(-0.2774, 0.0050, -0.0155)) + dot(s, vec3f(0.1844, 0.1190, -0.5661)) + n * 0.3446,
        dot(c, vec3f(0.0002, -0.2455, 0.0259)) + dot(s, vec3f(0.1070, -0.0991, 0.0331)) + n * 0.7522,
        dot(c, vec3f(0.0642, 0.2788, -0.1724)) + dot(s, vec3f(0.0194, 1.2057, 1.0325)) + n * 0.0350);
}

fn photo_filter(c: vec4f, px: BaePixel) -> vec4f {
    var filter_color = params.color.rgb;
    if params.preset >= 1u && params.preset <= 20u {
        let packed = FILTERS[params.preset - 1u];
        let bits = (vec3u(packed) >> vec3u(16, 8, 0)) & vec3u(255u);
        filter_color = vec3f(bits) / 255.0;
    }
    let straight = bae_input_straight(c);
    let density = params.density * 0.01;
    let linear_space = (frame.flags & BAE_FRAME_LINEAR) != 0u;
    let rgb = straight.rgb;
    var linear = rgb;
    if !linear_space {
        linear = sign(rgb) * pow(abs(rgb), vec3f(2.4));
    }
    let white = min(filter_color.r, min(filter_color.g, filter_color.b));
    let chroma = filter_color - white;
    // A blue filter can have a secondary green component (e.g. Cooling 80).
    // Subtract that component before weighting the other two primaries.
    let green = chroma.g - (109.0 / 255.0) * chroma.b;
    var out = linear + density * (
        chroma.b * filter_axis(linear)
        + chroma.r * filter_axis(linear.gbr).brg
        + green    * filter_axis(linear.brg).gbr
        - (1.0 - white - max(chroma.r, max(chroma.g, chroma.b))) * linear);
    if params.preserve != 0u {
        let amber = chroma.r * (255.0 / 236.0);
        let green_only = green - amber * (138.0 / 255.0);
        out = linear + density * (
            4.0 * amber * warm_preserve(linear)
            + (chroma.b * cool_preserve(linear)
                + green_only * cool_preserve(linear.brg).gbr) / 0.65);
    }
    if !linear_space {
        out = sign(out) * pow(abs(out), vec3f(1.0 / 2.4));
    }
    return vec4f(out * straight.a, straight.a);
}
