@compute @workgroup_size(8,8)
fn vr_color_gradients(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let ray = sphere_ray(uv_at(center,eye.rect),eye.fov);
    let points = array<vec2f,8>(params.point_1,params.point_2,params.point_3,params.point_4,
        params.point_5,params.point_6,params.point_7,params.point_8);
    let colors = array<vec3f,8>(params.color_1.rgb,params.color_2.rgb,params.color_3.rgb,params.color_4.rgb,
        params.color_5.rgb,params.color_6.rgb,params.color_7.rgb,params.color_8.rgb);
    let power = 0.5 + 2.5*params.gradient_power*0.01;
    let softness = 0.0001 + params.gradient_blending*0.001;
    var rgb = vec3f(0);
    var weights = 0.0;
    for(var i=0; i<params.number_of_points; i++) {
        let origin = sphere_ray(points[i]/rect_size(frame.layer_bounds),eye.fov);
        let chord = max(0.0,1.0-dot(ray,origin));
        let w = pow(chord+softness,-power);
        rgb += w*colors[i];
        weights += w;
    }
    bae_store(p,composite_generator(p,rgb/weights));
}
