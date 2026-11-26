@compute @workgroup_size(8,8)
fn vr_sharpen(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let step_px = bae_scale(1.0);
    let x = vec2f(step_px,0.0);
    let y = vec2f(0.0,step_px);
    let center = texel(p);
    let low = bae_unpremultiply((nearby(n,x)+nearby(n,-x)+nearby(n,y)+nearby(n,-y))*0.25);
    let straight = bae_unpremultiply(center);
    let detail = straight.rgb-low.rgb;
    bae_store(p,bae_depth_clamp(vec4f(center.rgb+center.a*detail*params.sharpen_amount*0.04,center.a)));
}
