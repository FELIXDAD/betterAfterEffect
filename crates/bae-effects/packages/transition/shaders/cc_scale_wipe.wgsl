@compute @workgroup_size(8, 8)
fn cc_scale_wipe(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let pos = vec2f(p) + 0.5;
    let src = bae_premultiply(bae_input_straight(bae_load_input(p)));
    let stretch=params.stretch*0.01;
    if stretch==0.0 {bae_store(p,src);return;}
    let a=radians(params.direction);
    let axis=vec2f(sin(a),-cos(a))*sign(stretch);
    let q=dot(pos-params.center,axis);
    let uv=pos-axis*max(0.0,q)*abs(stretch);
    bae_store(p,bae_depth_clamp(bae_premultiply(bae_input_straight(bae_sample_input(uv)))));
}
