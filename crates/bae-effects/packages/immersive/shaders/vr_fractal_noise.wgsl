@compute @workgroup_size(8,8)
fn vr_fractal_noise(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let ray = sphere_ray(uv_at(center,eye.rect),eye.fov);
    let rotation = vec3f(params.tilt_x_axis,params.pan_y_axis,params.roll_z_axis);
    let sub_rotation = vec3f(params.sub_tilt_x_axis,params.sub_pan_y_axis,params.sub_roll_z_axis);
    var direction = rotate_ray(ray,rotation,true);
    var frequency = 64.0/params.scale;
    var amplitude = 1.0;
    var sum = 0.0;
    var total = 0.0;
    var peak = 0.0;
    let phase = params.evolution / 180.0;
    for(var i=0; f32(i)<params.complexity && i<10; i++) {
        let weight = amplitude*min(1.0,params.complexity-f32(i));
        let point = direction*frequency + phase*vec3f(0.71,0.53,0.39);
        let seed = u32(params.random_seed)+u32(i)*1013u;
        var v = smooth_noise(point,seed);
        if params.fractal_type == 2u { v = pow(abs(2.0*v-1.0),1.5); }
        if params.fractal_type == 4u { v = 1.0-smoothstep(0.0,0.22,abs(2.0*v-1.0)); }
        sum += weight*v;
        peak = max(peak,weight*v);
        total += weight;
        direction = rotate_ray(direction,sub_rotation,true);
        frequency /= params.sub_scaling*0.01;
        amplitude *= params.sub_influence*0.01;
    }
    var v = select(sum/total,peak,params.fractal_type==3u);
    if params.invert != 0u { v = 1.0-v; }
    v = (v-0.5)*params.contrast*0.01+0.5+params.brightness*0.01;
    bae_store(p,composite_generator(p,vec3f(v)));
}
