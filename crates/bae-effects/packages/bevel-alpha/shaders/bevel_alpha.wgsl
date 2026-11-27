@compute @workgroup_size(8, 8)
fn bevel_alpha(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let a = radians(params.angle);
    let d = vec2<f32>(sin(a), -cos(a)) * (0.90 * bae_scale(params.thickness));
    let pos = vec2<f32>(p) + 0.5;
    let height = bae_sample_input(pos - d).a - bae_sample_input(pos + d).a;
    let src = bae_load_input(p);
    let rgb = src.rgb + params.light.rgb * (height * src.a * params.intensity * 0.92);
    bae_store(p, vec4<f32>(clamp(rgb,vec3<f32>(0),vec3<f32>(saturate(src.a))), saturate(src.a)));
}
