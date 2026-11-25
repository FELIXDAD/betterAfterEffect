fn radial_wipe(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let t = params.transition_completion * 0.01;
    if t >= 1.0 { return vec4<f32>(0.0); }
    let p = vec2<f32>(px.pos) + 0.5 - params.wipe_center;
    const tau = 6.28318530718;
    var turns = fract((atan2(p.x, -p.y) - radians(params.start_angle)) / tau);
    if params.wipe == 2u { turns = fract(-turns); }
    let extent = t;
    if params.wipe == 3u { turns = fract(turns + 0.5 * t); }
    // Signed distance to the nearer bounding ray, including the start seam.
    let inside = turns < extent;
    let angle = select(min(turns - extent, 1.0 - turns), min(turns, extent - turns), inside);
    let d = length(p) * sin(min(angle * tau, tau * 0.25));
    let dist = select(d, -d, inside);
    if params.feather <= 0.0 { return c * saturate(0.5 + dist); }
    let z = dist / max(0.4, bae_scale(params.feather) * 0.378);
    return c * (0.5 + 0.5 * tanh(clamp(z * (0.798 + 0.0357*z*z), -8.0, 8.0)));
}
