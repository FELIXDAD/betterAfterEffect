// Soft, finite line segment with the opacity envelope sampled along its length.
fn dash_coverage(point: vec2f, start: vec2f, end: vec2f, radius: f32, hardness: f32,
                 interval: vec2f, dash_length: f32, gradient: vec4f) -> f32 {
    let rel = point - start;
    let delta = end - start;
    let t = dot(rel, delta) / max(dot(delta, delta), 1e-9);
    let h = clamp(t, 0.0, 1.0);
    let dist = length(rel - h * delta);
    let outside = mix(radius + 0.1, 0.6 * radius + 0.85, hardness);
    let feather = max(1.0, mix(radius * 0.8, 1.0, hardness));
    let alpha = clamp((outside - dist) / feather, 0.0, 1.0);
    if alpha <= 0.0 { return 0.0; }
    // Blur the opacity envelope along the contour as well as across it.
    // Only extrapolate the first and last pieces of a dash.
    let before = interval.x <= 0.00001;
    let after = interval.y >= 0.99999;
    let s = clamp(t, select(0.0, t, before && t < 0.0), select(1.0, t, after && t > 1.0));
    let along = mix(interval.x, interval.y, s);
    let stride = mix(radius, 0.8, hardness) / (4.0 * dash_length);
    var envelope = 0.0;
    for (var tap = -4; tap <= 4; tap++) {
        let q = along + f32(tap) * stride;
        if q >= 0.0 && q <= 1.0 {
            let m = gradient.w;
            let opacity = select(
                mix(gradient.y, gradient.z, (q - m) / (1.0 - m)),
                mix(gradient.x, gradient.y, q / m),
                q <= m,
            );
            envelope += opacity * f32(5 - abs(tap));
        }
    }
    return alpha * envelope * 0.04;
}
