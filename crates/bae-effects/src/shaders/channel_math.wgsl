// Shared straight-color operations for the channel and color effects.
fn fx_luma(rgb: vec3f) -> f32 {
    return dot(rgb, vec3f(0.299, 0.587, 0.114));
}

fn fx_hsl(rgb: vec3f) -> vec3f {
    let lo = min(rgb.r, min(rgb.g, rgb.b));
    let hi = max(rgb.r, max(rgb.g, rgb.b));
    let d = hi - lo;
    let l = (hi + lo) * 0.5;
    if d <= 0.0 {
        return vec3f(0.0, 0.0, l);
    }
    var h: f32;
    if hi == rgb.r {
        h = (rgb.g - rgb.b) / d;
    } else if hi == rgb.g {
        h = 2.0 + (rgb.b - rgb.r) / d;
    } else {
        h = 4.0 + (rgb.r - rgb.g) / d;
    }
    let denom = 1.0 - abs(2.0 * l - 1.0);
    let s = select(0.0, d / denom, abs(denom) > 1e-6);
    return vec3f(fract(h / 6.0), s, l);
}

fn fx_rgb(hsl: vec3f) -> vec3f {
    let h = fract(hsl.x);
    let hue = clamp(abs(fract(h + vec3f(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0,
                    vec3f(0.0), vec3f(1.0));
    let chroma = (1.0 - abs(2.0 * hsl.z - 1.0)) * hsl.y;
    return vec3f(hsl.z) + (hue - 0.5) * chroma;
}

// Channel enum shared by Shift Channels (alpha first) and Set Matte
// (alpha fourth, converted to this order by the caller).
fn fx_channel(c: vec4f, mode: u32) -> f32 {
    switch mode {
        case 1u: { return c.a; }
        case 2u, 3u, 4u: { return c[mode - 2u]; }
        case 5u: { return fx_luma(c.rgb); }
        case 6u: { return fx_hsl(c.rgb).x; }
        case 7u: { return fx_hsl(c.rgb).z; }
        case 8u: {
            let lo = min(c.r, min(c.g, c.b));
            let hi = max(c.r, max(c.g, c.b));
            return (hi - lo) * (1.0 - abs(hi + lo - 1.0));
        }
        case 9u: { return 1.0; }
        default: { return 0.0; }
    }
}

fn fx_unpremult(c: vec4f) -> vec4f {
    if c.a != 0.0 {
        return vec4f(c.rgb / c.a, c.a);
    }
    return vec4f(0.0);
}
