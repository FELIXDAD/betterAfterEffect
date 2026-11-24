// Hue in turns, lightness and saturation in unit intervals.
fn cc_hls(c: vec3<f32>) -> vec3<f32> {
    let hi = max(max(c.r, c.g), c.b);
    let lo = min(min(c.r, c.g), c.b);
    let d = hi - lo;
    let l = (hi + lo) * 0.5;
    if d == 0.0 { return vec3<f32>(0, l, 0); }
    var h = (c.g - c.b) / d;
    if c.r != hi {
        h = select(4.0 + (c.r - c.g) / d, 2.0 + (c.b - c.r) / d, c.g == hi);
    }
    return vec3<f32>(fract(h / 6.0), l, d / max(1e-6, 1.0 - abs(2.0*l - 1.0)));
}

fn cc_rgb(hls: vec3<f32>) -> vec3<f32> {
    let wheel = clamp(abs(fract(hls.x + vec3<f32>(0, 2./3., 1./3.))*6.-3.)-1., vec3<f32>(0), vec3<f32>(1));
    return hls.y + (wheel - 0.5) * ((1.0 - abs(2.0*hls.y - 1.0)) * hls.z);
}

fn cc_shift(v: f32, amount: f32) -> f32 {
    return v + amount * select(v, 1.0-v, amount > 0.0);
}

fn cc_weight(distance: f32, tolerance: f32, softness: f32) -> f32 {
    if distance <= tolerance { return 1.0; }
    return clamp(1.0 - (distance - tolerance) / max(softness, 1e-7), 0.0, 1.0);
}
