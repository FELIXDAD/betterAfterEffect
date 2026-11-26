fn aberrated(ray: vec3f, origin: vec3f, distance: f32, eye: Eye, amount: f32) -> vec4f {
    let cosine = clamp(dot(ray,origin),-1.0,1.0);
    let tangent = ray*cosine-origin;
    let delta = radians(amount*0.12)*distance;
    if length(tangent)<0.00001 { return sample_ray(ray,eye.rect,eye.fov); }
    return sample_ray(ray*cos(delta)+normalize(tangent)*sin(delta),eye.rect,eye.fov);
}
@compute @workgroup_size(8,8)
fn vr_chromatic_aberration(@builtin(global_invocation_id) id: vec3u) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let ray = sphere_ray(uv_at(center,eye.rect),eye.fov);
    let origin = sphere_ray(params.point_of_interest/rect_size(frame.layer_bounds),eye.fov);
    let angle = acos(clamp(dot(ray,origin),-1.0,1.0));
    var falloff = smoothstep(0.0,max(0.00001,PI*params.falloff_distance*0.01),angle);
    if params.invert_falloff != 0u { falloff = 1.0-falloff; }
    let r = bae_unpremultiply(aberrated(ray,origin,falloff,eye,params.aberration_red)).r;
    let g = bae_unpremultiply(aberrated(ray,origin,falloff,eye,params.aberration_green)).g;
    let b = bae_unpremultiply(aberrated(ray,origin,falloff,eye,params.aberration_blue)).b;
    let a = texel(p).a;
    bae_store(p,bae_depth_clamp(vec4f(vec3f(r,g,b)*a,a)));
}
