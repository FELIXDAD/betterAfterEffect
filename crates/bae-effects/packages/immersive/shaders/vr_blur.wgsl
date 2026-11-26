// Three widening disk passes (cascade.wgsl), seam- and pole-aware.
@compute @workgroup_size(8,8)
fn vr_blur_fine(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let radius = cascade_radius(bae_scale(params.blurriness),0u);
    let count = cascade_fine_taps(radius);
    let jitter = disk_jitter(p);
    var sum = texel(p);
    var weights = 1.0;
    for (var i=0u; i<count; i++) {
        let tap = disk_offset(i,count,radius,jitter);
        let w = exp(-2.5*tap.z);
        sum += w*nearby(n,tap.xy);
        weights += w;
    }
    bae_store(p,sum/weights);
}
@compute @workgroup_size(8,8)
fn vr_blur_mid(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let radius = cascade_radius(bae_scale(params.blurriness),1u);
    let jitter = disk_jitter(p);
    var sum = bae_load_a(p);
    var weights = 1.0;
    for (var i=0u; i<CASCADE_TAPS; i++) {
        let tap = disk_offset(i,CASCADE_TAPS,radius,jitter);
        let w = exp(-2.5*tap.z);
        sum += w*nearby_a(n,tap.xy);
        weights += w;
    }
    bae_store(p,sum/weights);
}
@compute @workgroup_size(8,8)
fn vr_blur(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let radius = cascade_radius(bae_scale(params.blurriness),2u);
    let jitter = disk_jitter(p);
    var sum = bae_load_b(p);
    var weights = 1.0;
    for (var i=0u; i<CASCADE_TAPS; i++) {
        let tap = disk_offset(i,CASCADE_TAPS,radius,jitter);
        let w = exp(-2.5*tap.z);
        sum += w*nearby_b(n,tap.xy);
        weights += w;
    }
    bae_store(p,bae_depth_clamp(sum/weights));
}
