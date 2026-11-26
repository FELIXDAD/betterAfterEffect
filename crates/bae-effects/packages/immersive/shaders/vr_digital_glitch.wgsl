@compute @workgroup_size(8,8)
fn vr_digital_glitch(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let ray = sphere_ray(uv_at(center,eye.rect),eye.fov);
    let origin = sphere_ray(params.point_of_interest/rect_size(frame.layer_bounds),eye.fov);
    let displacement = ray-origin;
    let aspect = exp2(params.poi_aspect_ratio*0.02);
    let distance = length(displacement*vec3f(1.0/aspect,aspect,1));
    let mask = 1.0-smoothstep(params.poi_scale*0.018,params.poi_scale*0.022+0.00001,distance);
    let amp = mask*params.master_amplitude*0.01;
    if amp<=0.0 { bae_store(p,texel(p)); return; }
    let scale = vec3f(params.scale_x,params.scale_y,params.scale_z)*0.07+0.25;
    let shift = vec3f(params.translate_x,params.translate_y,params.translate_z);
    let phase = i32(floor(params.distortion_evolution/15.0));
    let seed = u32(params.random_seed);
    let density = 1.0+params.distortion_complexity*0.12;
    let field = ray*density*scale+shift;
    let cell = vec3i(floor(field));
    let damaged = cell_noise(cell,seed+bitcast<u32>(phase)) < params.distortion_rate*0.01;
    let block = vec3f(cell_noise(cell,seed+1u),cell_noise(cell,seed+2u),cell_noise(cell,seed+3u))*2.0-1.0;
    let fine = vec3i(floor(field/max(0.1,params.sub_scaling*0.01)));
    let jitter = (cell_noise(fine,seed+bitcast<u32>(phase))*2.0-1.0)*params.sub_influence*0.01;
    let strength = select(0.0,amp,damaged);
    let axes = vec3f(params.geometry_distortion_x,params.geometry_distortion_y,params.geometry_distortion_z)*0.14;
    let distorted = rotate_ray(ray,(block+jitter)*axes*strength,false);
    var c = sample_ray(distorted,eye.rect,eye.fov);
    let color_phase = bitcast<u32>(i32(floor(params.color_evolution/9.0)));
    let split = strength*params.color_distortion*0.07*(2.0*cell_noise(cell,seed+color_phase+9u)-1.0);
    let red = sample_ray(rotate_ray(distorted,vec3f(0,split,0),false),eye.rect,eye.fov);
    let blue = sample_ray(rotate_ray(distorted,vec3f(0,-split,0),false),eye.rect,eye.fov);
    var rgb = vec3f(bae_unpremultiply(red).r,bae_unpremultiply(c).g,bae_unpremultiply(blue).b);
    let grain_cell = vec3i(floor(ray*rect_size(eye.rect).y/(1.0+params.noise_scale*0.15)));
    let grain_seed = seed+bitcast<u32>(i32(floor(params.noise_evolution/8.0)));
    if cell_noise(grain_cell,grain_seed+13u) < params.noise_amount*0.01 {
        let gray = cell_noise(grain_cell,grain_seed);
        let grain = select(vec3f(gray),vec3f(gray,cell_noise(grain_cell,grain_seed+1u),cell_noise(grain_cell,grain_seed+2u)),params.color_noise!=0u);
        rgb += (grain-0.5)*params.noise_strength*0.02*amp;
    }
    bae_store(p,bae_depth_clamp(vec4f(rgb*c.a,c.a)));
}
