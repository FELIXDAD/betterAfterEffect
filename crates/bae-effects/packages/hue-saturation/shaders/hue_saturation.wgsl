// Hue/Saturation (ADBE HUE SATURATION, AE-FX-041). Skottie's reconstruction of
// AE's model: lightness, then saturation (around the chroma mid-point, limited
// so channels stay inside 0..1), then an HSL hue rotation. Colorize replaces
// hue and saturation and keeps the (adjusted) HSL lightness. Straight colour.

fn hs_hue_to_rgb(h: f32) -> vec3<f32> {
    let r = abs(h * 6.0 - 3.0) - 1.0;
    let g = 2.0 - abs(h * 6.0 - 2.0);
    let b = 2.0 - abs(h * 6.0 - 4.0);
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn hs_hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let ch = (1.0 - abs(2.0 * hsl.z - 1.0)) * hsl.y;
    return (hs_hue_to_rgb(fract(hsl.x)) - 0.5) * ch + hsl.z;
}

// Hue of `c` in 0..1 (0 for greys).
fn hs_hue(c: vec3<f32>) -> f32 {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let d = mx - mn;
    if (d <= 1e-6) {
        return 0.0;
    }
    var h: f32;
    if (mx == c.r) {
        h = (c.g - c.b) / d;
        if (h < 0.0) {
            h = h + 6.0;
        }
    } else if (mx == c.g) {
        h = (c.b - c.r) / d + 2.0;
    } else {
        h = (c.r - c.g) / d + 4.0;
    }
    return h / 6.0;
}

// HSL hue rotation, written with the channel minimum and maximum (which an
// HSL round trip preserves) so it is also defined for HDR values.
fn hs_rotate_hue(c: vec3<f32>, turns: f32) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    if (mx - mn <= 1e-6) {
        return c;
    }
    return vec3<f32>(mn) + (mx - mn) * hs_hue_to_rgb(fract(hs_hue(c) + turns));
}

fn hs_lightness(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let l = amount / 100.0;
    if (l < 0.0) {
        return c * (1.0 + l);
    }
    return c * (1.0 - l) + vec3<f32>(l);
}

// Weight of hue `h` (0..1) for range `r` (2 = Reds .. 7 = Magentas): 1 in the
// 30° core, linear over 30° of falloff on each side (Photoshop defaults).
fn hs_range_weight(h: f32, r: u32) -> f32 {
    let center = f32(r - 2u) * 60.0;
    var d = abs(h * 360.0 - center);
    d = min(d, 360.0 - d);
    return clamp((45.0 - d) / 30.0, 0.0, 1.0);
}

fn hue_saturation(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    var rgb = s.rgb;
    if (params.colorize != 0u) {
        let mx = max(rgb.r, max(rgb.g, rgb.b));
        let mn = min(rgb.r, min(rgb.g, rgb.b));
        let l = clamp(0.5 * (mx + mn), 0.0, 1.0);
        let l2 = hs_lightness(vec3<f32>(l), params.colorize_lightness).x;
        rgb = hs_hsl_to_rgb(vec3<f32>(params.colorize_hue / 360.0,
            params.colorize_saturation / 100.0, l2));
    } else {
        let original = rgb;
        rgb = hs_lightness(rgb, params.master_lightness);
        let sat = params.master_saturation / 100.0;
        if (sat != 0.0) {
            let mn = min(rgb.r, min(rgb.g, rgb.b));
            let mx = max(rgb.r, max(rgb.g, rgb.b));
            let mid = 0.5 * (mn + mx);
            let ch = max(mx - mn, 1e-4);
            var k = 1.0 + sat;
            if (sat > 0.0) {
                k = min(min(1.0 / (1.0 - min(sat, 0.999)), 126.0),
                    max(min(mid, 1.0 - mid), 0.0) / ch * 2.0);
                k = max(k, 1.0);
            }
            rgb = vec3<f32>(mid) + (rgb - vec3<f32>(mid)) * k;
        }
        if (params.master_hue != 0.0) {
            rgb = hs_rotate_hue(rgb, params.master_hue / 360.0);
        }
        if (params.channel_control >= 2u) {
            let w = hs_range_weight(hs_hue(original), params.channel_control);
            rgb = mix(original, rgb, w);
        }
    }
    return vec4<f32>(rgb * c.a, c.a);
}
