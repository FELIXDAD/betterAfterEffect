fn cylinder_face(hit:vec3f,rot:mat3x3f,r:f32,h:f32,inside:bool)->vec4f{
    if abs(hit.y)>h*.5{return vec4f(0);}
    let st=vec2f(.5+atan2(hit.x,hit.z)/TAU,hit.y/h+.5);
    let normal=rot*vec3f(hit.x/r,0,hit.z/r);
    return illumination(surface_color(st),select(normal,-normal,inside),15u,21u);
}
fn effect(p:vec2f)->vec4f{
    let depth=max(.001,1.+v(6)*u.scale/max(1.,u.layer.z));
    let center=pos(vec2f(.5))+vec2f(v(4),v(5))*u.scale;
    let radius=u.layer.z*v(2)*.01/TAU;let height=u.layer.w;
    if radius<=0.{return vec4f(0);}
    let rot=rotation(radians(vec3f(v(9),v(10),v(11))),i32(v(27)));
    let origin=transpose(rot)*vec3f((p-center)*depth,0);
    let ray=transpose(rot)*vec3f(0,0,1);
    let a=dot(ray.xz,ray.xz);let b=dot(origin.xz,ray.xz);
    let discr=b*b-a*(dot(origin.xz,origin.xz)-radius*radius);
    if discr<=0. || a<.000001{return vec4f(0);}
    let far=origin+ray*((-b-sqrt(discr))/a);
    let near=origin+ray*((-b+sqrt(discr))/a);
    if v(13)==3.{return cylinder_face(far,rot,radius,height,true);}
    var c=cylinder_face(near,rot,radius,height,false);
    if v(13)==1. && c.a<1.{c+=(1.-c.a)*cylinder_face(far,rot,radius,height,true);}
    return c;
}