// AE's grey-line response has four axial neighbours. Alpha is sharpened
// independently. Extend the visible colour into fully transparent neighbours.
fn at(p: vec2<i32>, fallback: vec3<f32>) -> vec4<f32> {
    let rect = bae_rect_input();
    if any(p < rect.min) || any(p >= rect.max) { return vec4<f32>(0); }
    let c = bae_load_input(p);
    return vec4<f32>(select(fallback, bae_unpremultiply(c).rgb, c.a > 0.0), c.a);
}

@compute @workgroup_size(8, 8)
fn sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let c = bae_unpremultiply(bae_load_input(p));
    let dx = vec2<i32>(1, 0);
    let dy = vec2<i32>(0, 1);
    let laplacian = 4.0 * c - at(p-dx,c.rgb) - at(p+dx,c.rgb)
                            - at(p-dy,c.rgb) - at(p+dy,c.rgb);
    let sharp = c + laplacian * (f32(params.amount) / 64.0);
    let alpha = saturate(sharp.a);
    bae_store(p, bae_depth_clamp(vec4<f32>(sharp.rgb * alpha, alpha)));
}
