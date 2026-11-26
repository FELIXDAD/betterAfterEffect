@compute @workgroup_size(8,8)
fn vr_rotate_sphere(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let ray = sphere_ray(uv_at(center,eye.rect),eye.fov);
    let angles = vec3f(params.tilt_x_axis,params.pan_y_axis,params.roll_z_axis);
    bae_store(p,sample_ray(rotate_ray(ray,angles,params.invert_rotation != 0u),eye.rect,eye.fov));
}
