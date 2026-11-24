// The AE operator supports 8 bpc even in a higher-depth project.
fn eight_bit(c: vec4<f32>) -> vec4<f32> {
    let a = round(saturate(c.a)*255.0)/255.0;
    let rgb = round(saturate(bae_unpremultiply(c).rgb)*255.0)/255.0;
    return vec4<f32>(rgb*a,a);
}

fn color_at(p: vec2<i32>) -> vec3<f32> {
    let r = bae_rect_input();
    return eight_bit(bae_load_input(clamp(p, r.min, r.max - 1))).rgb;
}

@compute @workgroup_size(8, 8)
fn find_edges(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    var dx = vec3<f32>(0.0);
    var dy = vec3<f32>(0.0);
    for (var i = -1; i <= 1; i++) {
        let w = select(1.0, 2.0, i == 0);
        dx += w * (color_at(p + vec2<i32>(1, i)) - color_at(p + vec2<i32>(-1, i)));
        dy += w * (color_at(p + vec2<i32>(i, 1)) - color_at(p + vec2<i32>(i, -1)));
    }
    let src = eight_bit(bae_load_input(p));
    let strength = min(vec3<f32>(src.a), sqrt(dx * dx + dy * dy));
    let edge = select(src.a - strength, strength, params.invert != 0u);
    bae_store(p, bae_depth_clamp(vec4<f32>(mix(edge, src.rgb, params.blend), src.a)));
}
