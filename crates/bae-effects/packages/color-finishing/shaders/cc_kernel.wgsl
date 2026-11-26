@compute @workgroup_size(8,8)
fn cc_kernel(@builtin(global_invocation_id) id:vec3u) {
    let p=frame.target_rect.min+vec2i(id.xy);if !bae_in_target(p){return;}
    let bounds=bae_rect_input();
    let weights=array(params.coefficient_2,params.coefficient_3,params.coefficient_4,
        params.coefficient_7,params.coefficient_8,params.coefficient_9,
        params.coefficient_12,params.coefficient_13,params.coefficient_14);
    var rgb=vec3f(0);
    for(var i=0;i<9;i++){let offset=vec2i(i%3-1,i/3-1);
        rgb+=bae_input_straight(bae_load_input(clamp(p+offset,bounds.min,bounds.max-1))).rgb*weights[i];
    }
    let c=bae_input_straight(bae_load_input(p));rgb/=max(0.01,params.divider);
    if params.absolute_values!=0u {rgb=abs(rgb);}
    bae_store(p,bae_depth_clamp(bae_premultiply(vec4f(mix(rgb,c.rgb,params.blend_w_original*0.01),c.a))));
}
