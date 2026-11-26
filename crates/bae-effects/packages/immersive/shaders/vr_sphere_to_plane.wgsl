@compute @workgroup_size(8,8)
fn vr_sphere_to_plane(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let uv = uv_at(vec2f(p)+0.5,frame.output_rect)*2.0-1.0;
    let size = rect_size(frame.output_rect);
    let ray = normalize(vec3f(uv.x,-uv.y*size.y/size.x,1));
    let angles = vec3f(params.projection_tilt_x_axis,params.projection_pan_y_axis,params.projection_roll_z_axis);
    bae_store(p,read_projection(rotate_ray(ray,angles,params.invert_rotation != 0u),params.input_format,90.0));
}
