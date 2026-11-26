// Canonical faces: right, left, top, bottom, front, back.
fn grid(format: u32) -> vec2i {
    if format == 1u { return vec2i(4,3); }
    if format == 7u { return vec2i(6,1); }
    return vec2i(3,2);
}
fn face_cell(face: u32, format: u32) -> vec2i {
    if format == 1u {
        return array<vec2i,6>(vec2i(2,1),vec2i(0,1),vec2i(1,0),vec2i(1,2),vec2i(1,1),vec2i(3,1))[face];
    }
    var index = i32(face);
    if format == 6u { index = array<i32,6>(1,3,4,5,0,2)[face]; }
    let g = grid(format);
    return vec2i(index % g.x,index / g.x);
}
fn face_ray(face: u32, uv: vec2f) -> vec3f {
    let q = uv * 2.0 - 1.0;
    switch face {
        case 0u: { return vec3f(1,-q.y,-q.x); }
        case 1u: { return vec3f(-1,-q.y,q.x); }
        case 2u: { return vec3f(q.x,1,q.y); }
        case 3u: { return vec3f(q.x,-1,-q.y); }
        case 4u: { return vec3f(q.x,-q.y,1); }
        default: { return vec3f(-q.x,-q.y,-1); }
    }
}
struct FaceUV { face: u32, uv: vec2f }
fn project_cube(d: vec3f) -> FaceUV {
    let a = abs(d);
    if a.x >= max(a.y,a.z) {
        return FaceUV(select(1u,0u,d.x>0), vec2f(-sign(d.x)*d.z,-d.y)/a.x*0.5+0.5);
    }
    if a.y >= a.z {
        return FaceUV(select(3u,2u,d.y>0), vec2f(d.x,sign(d.y)*d.z)/a.y*0.5+0.5);
    }
    return FaceUV(select(5u,4u,d.z>0),vec2f(sign(d.z)*d.x,-d.y)/a.z*0.5+0.5);
}
fn cube_texel(face: u32, p: vec2i, span: vec2f, format: u32) -> vec4f {
    let d = face_ray(face,(vec2f(p)+0.5)/span);
    let mapped = project_cube(d);
    let cell = vec2f(face_cell(mapped.face,format));
    let rect = frame.layer_bounds;
    return texel(rect.min + clamp(vec2i(floor((cell+mapped.uv)*span)),vec2i(0),rect.max-rect.min-1));
}
fn is_cube(format: u32) -> bool { return format == 1u || (format >= 5u && format <= 7u); }
fn projected_ray(uv: vec2f, format: u32) -> vec3f {
    if is_cube(format) {
        let pos = uv * vec2f(grid(format));
        for(var face=0u; face<6u; face++) {
            if all(vec2i(floor(pos)) == face_cell(face,format)) { return normalize(face_ray(face,fract(pos))); }
        }
        return vec3f(0);
    }
    if format == 2u || format == 4u {
        let q = uv*2.0-1.0;
        let r = length(q);
        if r > 1.0 { return vec3f(0); }
        let a = select(2.0*asin(min(r,1.0)), r*PI*0.5, format==4u);
        return vec3f(q.x,-q.y,0) * sin(a)/max(r,0.000001) + vec3f(0,0,cos(a));
    }
    return sphere_ray(uv,FULL);
}
fn read_projection(d: vec3f, format: u32, fov: f32) -> vec4f {
    let rect = frame.layer_bounds;
    let size = rect_size(rect);
    if is_cube(format) {
        let face = project_cube(d);
        let span = size / vec2f(grid(format));
        let pos = face.uv*span-0.5;
        let p = vec2i(floor(pos));
        let f = fract(pos);
        return mix(mix(cube_texel(face.face,p,span,format),cube_texel(face.face,p+vec2i(1,0),span,format),f.x),
                   mix(cube_texel(face.face,p+vec2i(0,1),span,format),cube_texel(face.face,p+1,span,format),f.x),f.y);
    }
    if format == 0u {
        if d.z <= 0.000001 { return vec4f(0); }
        let t = tan(radians(clamp(fov,0.01,179.9))*0.5);
        return sample_uv(0.5+vec2f(d.x,-d.y*size.x/size.y)/(2.0*t*d.z),rect,vec2f(0));
    }
    if format == 2u || format == 4u {
        let a = acos(clamp(d.z,-1.0,1.0));
        if format == 4u && a > PI*0.5 { return vec4f(0.0); }
        let r = select(sin(a*0.5),a/(PI*0.5),format==4u);
        let axis = select(vec2f(1,0),vec2f(d.x,-d.y)/max(length(d.xy),0.000001),length(d.xy)>0.000001);
        return sample_uv(0.5+0.5*r*axis,rect,vec2f(0));
    }
    return sample_ray(d,rect,FULL);
}
