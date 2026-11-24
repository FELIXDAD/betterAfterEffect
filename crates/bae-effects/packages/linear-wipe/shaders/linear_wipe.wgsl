fn linear_wipe(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let t = params.transition_completion * 0.01;
    if t >= 1.0 { return vec4<f32>(0.0); }
    let angle = radians(params.wipe_angle);
    let dir = vec2<f32>(sin(angle), -cos(angle));
    let size = vec2<f32>(frame.layer_bounds.max - frame.layer_bounds.min);
    let span = dot(abs(dir), size);
    let d = dot((px.uv - 0.5) * size, dir);
    let feather = bae_scale(params.feather);
    let dist = d - (t - 0.5) * span;
    if feather <= 0.0 { return c * (floor(saturate(0.5 + dist) * 16.0) / 16.0); }
    let z = dist / max(0.4, feather * 0.378);
    // Logistic approximation to a Gaussian edge integral.
    return c * (0.5 + 0.5 * tanh(clamp(z * (0.798 + 0.0357*z*z), -8.0, 8.0)));
}
