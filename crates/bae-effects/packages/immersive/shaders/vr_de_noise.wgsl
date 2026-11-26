@compute @workgroup_size(8,8)
fn vr_de_noise(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let center = texel(p);
    let c = bae_unpremultiply(center);
    var sum = vec3f(0.0);
    var weight = 0.0;
    var samples: array<vec3f,9>;
    for (var i=0u; i<9u; i++) {
        let xy = vec2f(f32(i%3u)-1.0,f32(i/3u)-1.0);
        let tap = bae_unpremultiply(nearby(n,xy*bae_scale(1.0)));
        samples[i] = select(c.rgb,tap.rgb,tap.a>0.0);
        let diff = samples[i]-c.rgb;
        let w = exp(-dot(diff,diff)/max(0.000001,3.0*params.noise_level*params.noise_level))*tap.a;
        sum += w*tap.rgb;
        weight += w;
    }
    var rgb = mix(c.rgb,sum/max(weight,0.000001),sqrt(params.noise_level));
    if params.noise_type == 2u {
        // The component median rejects black and white impulse noise.
        for(var i=1u; i<9u; i++) {
            for(var j=i; j>0u; j--) {
                let a = samples[j-1u];
                let b = samples[j];
                samples[j-1u] = min(a,b);
                samples[j] = max(a,b);
            }
        }
        let delta = abs(c.rgb-samples[4]);
        let outlier = max(delta.x,max(delta.y,delta.z));
        rgb = mix(c.rgb,samples[4],smoothstep(0.0,max(0.001,1.0-params.noise_level),outlier));
    }
    bae_store(p,bae_depth_clamp(vec4f(rgb*center.a,center.a)));
}
