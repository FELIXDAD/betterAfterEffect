fn scatter_noise(p:vec2f)->vec2f {
    let i=vec2u(vec2i(floor(p)));
    let n=(i.x*17389u ^ i.y*12582917u) + 1823u;
    let h=(n^(n>>16u))*2246822519u;
    return vec2f(vec2u(h&65535u,(h^(h>>13u))*3266489917u&65535u))/32767.5-1.;
}
@compute @workgroup_size(8,8)
fn scatter(@builtin(global_invocation_id) id:vec3u) {
    let p=bae_target_pixel(id); if !bae_in_target(p){return;}
    let pos=vec2f(p)+0.5;
    let jitter=scatter_noise(pos+vec2f(317.,197.)*params.scatter_randomness);
    var offset=jitter*bae_scale(params.scatter_amount);
    if params.grain==2u{offset.y=0.;}else if params.grain==3u{offset.x=0.;}
    bae_store(p,bae_depth_clamp(bae_sample_input(pos+offset)));
}