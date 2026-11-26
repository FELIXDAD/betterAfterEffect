@compute @workgroup_size(8, 8)
fn cc_light_sweep(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let pos = vec2f(p) + 0.5;
    let src = bae_premultiply(bae_input_straight(bae_load_input(p)));
    let a = radians(params.direction);
    let normal = vec2f(cos(a),sin(a));
    let t = saturate(1.0-abs(dot(pos-params.center,normal))/max(bae_scale(params.width)*0.5,0.001));
    var band = t;
    if params.shape==2u {band=smoothstep(0.0,1.0,t);}
    if params.shape==3u {band=t*t*t;}
    let edge = abs(bae_sample_input(pos+normal*bae_scale(params.edge_thickness)).a-bae_sample_input(pos-normal*bae_scale(params.edge_thickness)).a);
    let light = band*(params.sweep_intensity+edge*params.edge_intensity)*0.01;
    var rgb = src.rgb + params.light_color.rgb*src.a*light;
    if params.light_reception==2u {rgb=mix(src.rgb,params.light_color.rgb*src.a,saturate(light));}
    var result = vec4f(rgb,src.a);
    if params.light_reception==3u {result=vec4f(params.light_color.rgb,1)*src.a*saturate(light);}
    bae_store(p,bae_depth_clamp(result));
}
