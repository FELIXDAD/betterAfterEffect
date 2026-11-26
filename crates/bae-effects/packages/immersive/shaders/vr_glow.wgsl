fn highlight(c: vec4f) -> vec4f {
    let straight = bae_unpremultiply(c);
    let luma = bae_luma(straight);
    let gate = smoothstep(params.luma_threshold-0.02,params.luma_threshold+0.02,luma);
    return c*gate;
}
// The highlights spread by three widening disk passes (cascade.wgsl).
@compute @workgroup_size(8,8)
fn vr_glow_fine(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let radius = cascade_radius(bae_scale(params.glow_radius),0u);
    let count = cascade_fine_taps(radius);
    let jitter = disk_jitter(p);
    var bloom = highlight(texel(p));
    var weights = 1.0;
    for (var i=0u; i<count; i++) {
        let tap = disk_offset(i,count,radius,jitter);
        let w = exp(-3.0*tap.z);
        bloom += w*highlight(nearby(n,tap.xy));
        weights += w;
    }
    bae_store(p,bloom/weights);
}
@compute @workgroup_size(8,8)
fn vr_glow_mid(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let radius = cascade_radius(bae_scale(params.glow_radius),1u);
    let jitter = disk_jitter(p);
    var bloom = bae_load_a(p);
    var weights = 1.0;
    for (var i=0u; i<CASCADE_TAPS; i++) {
        let tap = disk_offset(i,CASCADE_TAPS,radius,jitter);
        let w = exp(-3.0*tap.z);
        bloom += w*nearby_a(n,tap.xy);
        weights += w;
    }
    bae_store(p,bloom/weights);
}
@compute @workgroup_size(8,8)
fn vr_glow(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let center = texel(p);
    let radius = cascade_radius(bae_scale(params.glow_radius),2u);
    let jitter = disk_jitter(p);
    var bloom = bae_load_b(p);
    var weights = 1.0;
    for (var i=0u; i<CASCADE_TAPS; i++) {
        let tap = disk_offset(i,CASCADE_TAPS,radius,jitter);
        let w = exp(-3.0*tap.z);
        bloom += w*nearby_b(n,tap.xy);
        weights += w;
    }
    bloom *= params.glow_brightness/weights;
    var rgb = mix(vec3f(bae_luma(bloom)),bloom.rgb,params.glow_saturation);
    if params.use_tint_color != 0u { rgb = params.tint_color.rgb*bae_luma(bloom); }
    bae_store(p,bae_depth_clamp(vec4f(center.rgb+rgb,center.a+(1.0-center.a)*saturate(bloom.a))));
}
