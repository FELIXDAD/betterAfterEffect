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
fn nearby_ray(n: Neighborhood, offset: vec2f) -> vec3f {
    let angle = length(offset)*n.step_angle;
    let tangent = (n.east*offset.x + n.north*offset.y) / max(length(offset),0.000001);
    return n.ray*cos(angle) + tangent*sin(angle);
}
fn nearby(n: Neighborhood, offset: vec2f) -> vec4f {
    return sample_ray(nearby_ray(n,offset),n.eye.rect,n.eye.fov);
}
// Per-pixel rotation and radial jitter of the spiral (PCG 2D hash), so the
// sparsity left between taps reads as fine grain, not rings or copies.
fn disk_jitter(p: vec2i) -> vec2f {
    var v = vec2u(p) * 1664525u + 1013904223u;
    v.x += v.y * 1664525u; v.y += v.x * 1664525u;
    v ^= v >> vec2u(16u);
    v.x += v.y * 1664525u; v.y += v.x * 1664525u;
    v ^= v >> vec2u(16u);
    return vec2f(v >> vec2u(8u)) / 16777216.0;
}
// Equal-area spiral on a disk, measured in render pixels at the equator:
// the offset and its squared fraction of the radius (for the weight).
fn disk_offset(i: u32, count: u32, radius: f32, jitter: vec2f) -> vec3f {
    let t = (f32(i)+jitter.y)/f32(count);
    let a = f32(i)*2.39996323 + jitter.x*TAU;
    return vec3f(radius*sqrt(t)*vec2f(cos(a),sin(a)), t);
}
