@compute @workgroup_size(8, 8)
fn gradient_wipe(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let c = bae_load_input(p);
    if params.transition_completion >= 1.0 { bae_store(p, vec4<f32>(0)); return; }
    let luma = dot(bae_unpremultiply(c).rgb, vec3<f32>(0.299, 0.587, 0.114));
    let t = params.transition_completion;
    let softness = params.transition_softness;
    var mask: f32;
    if (frame.flags & BAE_FRAME_FLOAT) != 0u {
        let v = select(luma, 1.0 - luma, params.invert_gradient != 0u);
        mask = select(saturate((v - t*(1.0 + softness) + softness) / max(softness,1e-8)), step(t,v), softness == 0.0);
    } else {
        let white = select(255.0, 32768.0, (frame.flags & BAE_FRAME_16_BPC) != 0u);
        let level = floor(saturate(luma) * white + 0.49999);
        let value = select(level, white-level, params.invert_gradient != 0u);
        let spread = round(softness * white);
        let threshold = round(t * (white + spread));
        mask = select(saturate((value - threshold + spread) / max(1.0, spread)),
                      saturate(value - floor(t * white) + 0.5), spread == 0.0);
    }
    bae_store(p, c * mask);
}
