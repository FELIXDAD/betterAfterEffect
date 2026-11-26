@compute @workgroup_size(8,8)
fn vr_blur(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let radius = bae_scale(params.blurriness);
    var sum = texel(p);
    var weights = 1.0;
    for (var i=0u; i<40u; i++) {
        let offset = disk_offset(i,40u,radius);
        let w = exp(-2.5*(f32(i)+0.5)/40.0);
        sum += w*nearby(n,offset);
        weights += w;
    }
    bae_store(p,bae_depth_clamp(sum/weights));
}
