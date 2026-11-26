fn highlight(c: vec4f) -> vec4f {
    let straight = bae_unpremultiply(c);
    let luma = bae_luma(straight);
    let gate = smoothstep(params.luma_threshold-0.02,params.luma_threshold+0.02,luma);
    return c*gate;
}
@compute @workgroup_size(8,8)
fn vr_glow(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let n = neighborhood(p);
    let center = texel(p);
    var bloom = highlight(center);
    var weights = 1.0;
    for(var i=0u; i<48u; i++) {
        let w = exp(-3.0*(f32(i)+0.5)/48.0);
        bloom += w*highlight(nearby(n,disk_offset(i,48u,bae_scale(params.glow_radius))));
        weights += w;
    }
    bloom *= params.glow_brightness/weights;
    var rgb = mix(vec3f(bae_luma(bloom)),bloom.rgb,params.glow_saturation);
    if params.use_tint_color != 0u { rgb = params.tint_color.rgb*bae_luma(bloom); }
    bae_store(p,bae_depth_clamp(vec4f(center.rgb+rgb,center.a+(1.0-center.a)*saturate(bloom.a))));
}
