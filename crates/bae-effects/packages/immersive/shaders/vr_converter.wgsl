@compute @workgroup_size(8,8)
fn vr_converter(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let ray = projected_ray(uv_at(vec2f(p)+0.5,frame.output_rect),params.output);
    if dot(ray,ray) < 0.5 { bae_store(p,vec4f(0)); return; }
    let angles = vec3f(params.tilt_x_axis,params.pan_y_axis,params.roll_z_axis);
    let d = rotate_ray(ray,angles,params.invert_rotation != 0u);
    bae_store(p,read_projection(d,params.input-1u,params.field_of_view));
}
