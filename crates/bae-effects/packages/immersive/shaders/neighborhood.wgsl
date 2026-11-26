struct Neighborhood { eye: Eye, ray: vec3f, east: vec3f, north: vec3f, step_angle: f32 }
fn neighborhood(p: vec2i) -> Neighborhood {
    let center = vec2f(p)+0.5;
    let eye = vr_eye(center);
    let uv = uv_at(center,eye.rect);
    let ray = sphere_ray(uv,eye.fov);
    let lon = (uv.x-0.5)*eye.fov.x;
    let east = vec3f(cos(lon),0,-sin(lon));
    return Neighborhood(eye,ray,east,cross(ray,east),eye.fov.y/rect_size(eye.rect).y);
}
fn nearby(n: Neighborhood, offset: vec2f) -> vec4f {
    let angle = length(offset)*n.step_angle;
    let tangent = (n.east*offset.x + n.north*offset.y) / max(length(offset),0.000001);
    return sample_ray(n.ray*cos(angle) + tangent*sin(angle),n.eye.rect,n.eye.fov);
}
// Equal-area spiral on a disk, measured in render pixels at the equator.
fn disk_offset(i: u32, count: u32, radius: f32) -> vec2f {
    let a = f32(i)*2.39996323;
    return radius*sqrt((f32(i)+0.5)/f32(count))*vec2f(cos(a),sin(a));
}
