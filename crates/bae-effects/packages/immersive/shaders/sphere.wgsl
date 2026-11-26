const PI = 3.14159265359;
const TAU = 6.28318530718;
const FULL = vec2f(TAU, PI);

fn rect_size(rect: BaeRect) -> vec2f {
    return max(vec2f(rect.max - rect.min), vec2f(1.0));
}
fn uv_at(p: vec2f, rect: BaeRect) -> vec2f {
    return (p - vec2f(rect.min)) / rect_size(rect);
}
// +Z is forward, +X is east and +Y is up.
fn sphere_ray(uv: vec2f, fov: vec2f) -> vec3f {
    let a = (uv - 0.5) * fov;
    return vec3f(sin(a.x) * cos(a.y), -sin(a.y), cos(a.x) * cos(a.y));
}
fn sphere_uv(ray: vec3f, fov: vec2f) -> vec2f {
    let longitude = atan2(ray.x, select(ray.z, 1.0, abs(ray.z) + abs(ray.x) < 0.000001));
    return vec2f(longitude, -atan2(ray.y, length(ray.xz))) / fov + 0.5;
}
fn turn(v: vec2f, a: f32) -> vec2f {
    return vec2f(cos(a)*v.x - sin(a)*v.y, sin(a)*v.x + cos(a)*v.y);
}
fn rotate_ray(ray: vec3f, angles: vec3f, inverse: bool) -> vec3f {
    let a = radians(angles);
    if inverse {
        let z = vec3f(turn(ray.xy, -a.z), ray.z);
        let y = turn(z.zx, -a.y);
        return vec3f(y.y, turn(vec2f(z.y,y.x), -a.x));
    }
    let x = vec3f(ray.x, turn(ray.yz, a.x));
    let y = turn(x.zx, a.y);
    return vec3f(turn(vec2f(y.y,x.y), a.z),y.x);
}
fn texel(p: vec2i) -> vec4f {
    let c = bae_load_input(p);
    if (frame.flags & BAE_FRAME_INPUT_STRAIGHT) != 0u { return bae_premultiply(c); }
    if (frame.flags & BAE_FRAME_INPUT_OPAQUE) != 0u { return vec4f(c.rgb,1.0); }
    return c;
}
fn wrap_index(x: i32, n: i32) -> i32 { return ((x % n) + n) % n; }
// Bilinear footprints cross longitude zero and fold over either pole into
// the opposite meridian. Each eye has an independent integer texel domain.
fn sphere_tap(p: vec2i, rect: BaeRect, fov: vec2f) -> vec4f {
    let span = max(rect.max - rect.min, vec2i(1));
    var q = p;
    var half_texel = false;
    if (q.y < 0 || q.y >= span.y) && fov.y >= PI - 0.00001 {
        q.y = select(-1-q.y, 2*span.y-1-q.y, q.y >= span.y);
        if fov.x >= TAU - 0.00001 {
            q.x += span.x / 2;
            half_texel = (span.x % 2) != 0;
        }
    }
    q.y = clamp(q.y, 0, span.y-1);
    if fov.x >= TAU - 0.00001 {
        q.x = wrap_index(q.x, span.x);
    } else { q.x = clamp(q.x,0,span.x-1); }
    let c = texel(rect.min + q);
    if half_texel {
        return 0.5*(c+texel(rect.min+vec2i((q.x+1)%span.x,q.y)));
    }
    return c;
}
fn sample_uv(uv: vec2f, rect: BaeRect, fov: vec2f) -> vec4f {
    if any(uv < vec2f(-0.00001)) || any(uv > vec2f(1.00001)) { return vec4f(0.0); }
    let p = uv * rect_size(rect) - 0.5;
    let q = vec2i(floor(p));
    let f = fract(p);
    return mix(mix(sphere_tap(q,rect,fov),sphere_tap(q+vec2i(1,0),rect,fov),f.x),
               mix(sphere_tap(q+vec2i(0,1),rect,fov),sphere_tap(q+1,rect,fov),f.x),f.y);
}
fn sample_ray(ray: vec3f, rect: BaeRect, fov: vec2f) -> vec4f {
    return sample_uv(sphere_uv(ray,fov),rect,fov);
}
