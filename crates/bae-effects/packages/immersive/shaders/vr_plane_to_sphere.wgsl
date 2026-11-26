@compute @workgroup_size(8,8)
fn vr_plane_to_sphere(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let size = rect_size(frame.layer_bounds);
    let dst = vec3f(params.projection_tilt_x_axis,params.projection_pan_y_axis + eye.side*params.stereo_disparity_degrees*0.5,params.projection_roll_z_axis);
    let source = vec3f(params.source_tilt_x_axis,params.source_pan_y_axis,params.source_roll_z_axis);
    let ray = rotate_ray(sphere_ray(uv_at(center,eye.rect),eye.fov),dst,true);
    // Intersect the ray with the source plane, rotated about its own center.
    let local = rotate_ray(ray,source,true);
    let normal = rotate_ray(vec3f(0,0,1),source,false);
    let distance = normal.z / select(local.z,1.0,abs(local.z)<0.000001);
    if abs(local.z)<0.000001 || distance <= 0.0 || params.scale_degrees <= 0.0 { bae_store(p,vec4f(0)); return; }
    let hit = local*distance - rotate_ray(vec3f(0,0,1),source,true);
    let half_width = tan(radians(min(params.scale_degrees,179.9))*0.5);
    let uv = 0.5+vec2f(hit.x,-hit.y*size.x/size.y)/(2.0*half_width);
    let edge = min(uv,1.0-uv);
    let feather = smoothstep(0.0,max(0.000001,params.feather*0.005),min(edge.x,edge.y));
    bae_store(p,sample_uv(uv,frame.layer_bounds,vec2f(0))*feather);
}
