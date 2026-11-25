@compute @workgroup_size(8, 8)
fn cc_radial_scalewipe(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let pos = vec2f(p) + 0.5;
    let src = bae_premultiply(bae_input_straight(bae_load_input(p)));
    let t=params.completion;
    if t<=0.0 {bae_store(p,src);return;} if t>=1.0 {bae_store(p,vec4f(0));return;}
    let d=pos-params.center;
    var uv=params.center+d/(1.0-t);
    if params.reverse_transition==0u {
        let extent=max(abs(params.center-vec2f(frame.layer_bounds.min)),abs(vec2f(frame.layer_bounds.max)-params.center));
        let radius=length(extent);
        let r=length(d);
        if r<=radius*t {bae_store(p,vec4f(0));return;}
        uv=params.center+d*((r-radius*t)/max(r*(1.0-t),0.00001));
    }
    bae_store(p,bae_depth_clamp(bae_premultiply(bae_input_straight(bae_sample_input(uv)))));
}
