fn burn(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(select(1.0 - min(vec3<f32>(1), (1.0 - b) / s), vec3<f32>(0), s <= vec3<f32>(0)),
                  vec3<f32>(1), b >= vec3<f32>(1));
}

fn dodge(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return 1.0 - burn(1.0 - b, 1.0 - s);
}

fn hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(1.0 - 2.0 * (1.0 - b) * (1.0 - s), 2.0 * b * s, s <= vec3<f32>(0.5));
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.30, 0.59, 0.11));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let range = sat(c);
    if range == 0.0 { return vec3<f32>(0); }
    return (c - min(c.r, min(c.g, c.b))) * (s / range);
}

fn set_lum(c: vec3<f32>, light: f32) -> vec3<f32> {
    var v = c + (light - lum(c));
    let lo = min(v.r, min(v.g, v.b));
    let hi = max(v.r, max(v.g, v.b));
    if lo < 0.0 && light > lo { v = light + (v - light) * (light / (light - lo)); }
    if hi > 1.0 && hi > light { v = light + (v - light) * ((1.0 - light) / (hi - light)); }
    return saturate(v);
}

// Mode numbers follow the 4-Color Gradient menu (including separators).
fn generated_blend(c: vec4<f32>, color: vec3<f32>, alpha: f32, mode: u32) -> vec4<f32> {
    if mode == 1u { return vec4<f32>(color * alpha, alpha); }
    let base = bae_unpremultiply(c).rgb;
    var overlap = color;
    switch mode {
        case 4u: { overlap = base + color; }
        case 5u: { overlap = base * color; }
        case 6u: { overlap = base + color - base * color; }
        case 7u: { overlap = hard_light(color, base); }
        case 8u: {
            let d = select(sqrt(max(base, vec3<f32>(0))), ((16.0 * base - 12.0) * base + 4.0) * base, base <= vec3<f32>(0.25));
            overlap = select(base + (2.0 * color - 1.0) * (d - base), base - (1.0 - 2.0 * color) * base * (1.0 - base), color <= vec3<f32>(0.5));
        }
        case 9u: { overlap = hard_light(base, color); }
        case 11u: { overlap = dodge(base, color); }
        case 12u: { overlap = burn(base, color); }
        case 14u: { overlap = min(base, color); }
        case 15u: { overlap = max(base, color); }
        case 16u: { overlap = abs(base - color); }
        case 17u: { overlap = base + color - 2.0 * base * color; }
        case 19u: { overlap = set_lum(set_sat(color, sat(base)), lum(base)); }
        case 20u: { overlap = set_lum(set_sat(base, sat(color)), lum(base)); }
        case 21u: { overlap = set_lum(color, lum(base)); }
        case 22u: { overlap = set_lum(base, lum(color)); }
        case 23u: {overlap=base+color-1.;}
        case 24u: {overlap=min(vec3f(1.),base+color);}
        case 25u: {overlap=base+2.*color-1.;}
        case 26u: {overlap=select(dodge(base,2.*color-1.),burn(base,2.*color),color<=vec3f(.5));}
        case 27u: {overlap=select(max(base,2.*color-1.),min(base,2.*color),color<=vec3f(.5));}
        case 28u: {overlap=step(vec3f(1.),base+color);}
        default: {}
    }
    return vec4<f32>((1.0 - alpha) * c.rgb + alpha * mix(color, overlap, c.a), alpha + (1.0 - alpha) * c.a);
}
