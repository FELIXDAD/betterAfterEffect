fn venetian_blinds(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let t = params.transition_completion * 0.01;
    if t >= 1.0 { return vec4<f32>(0.0); }
    let angle = radians(params.direction);
    let dir = vec2<f32>(cos(angle), sin(angle));
    let width = max(0.0001, bae_scale(params.width));
    let size = vec2<f32>(frame.layer_bounds.max - frame.layer_bounds.min);
    let coord = dot((px.uv - 0.5) * size - 0.5, dir);
    let x = (fract(coord / width - t * 0.5 + 0.5) - 0.5) * width;
    let dist = abs(x) - width * t * 0.5;
    let feather = bae_scale(params.feather);
    if feather <= 0.0 { return c * saturate(0.5 + dist); }
    let sigma = max(0.4, feather * 0.48);
    // Nearby periods overlap at large feather. The limiting value is the open fraction.
    if sigma > width { return c * (1.0 - t); }
    var covered = 0.0;
    for (var i = -3; i <= 3; i++) {
        let offset = x + f32(i)*width;
        let lo = (offset - 0.5*t*width)/sigma;
        let hi = (offset + 0.5*t*width)/sigma;
        covered += 0.5 * (tanh(clamp(hi * (0.798+0.0357*hi*hi), -8.0, 8.0)) - tanh(clamp(lo * (0.798+0.0357*lo*lo), -8.0, 8.0)));
    }
    return c * saturate(1.0-covered);
}
