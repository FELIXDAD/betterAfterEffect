// `px`: object units per pixel. Cap edges fade over a pixel like the sphere's rim.
fn cylinder_face(hit:vec3f,rot:mat3x3f,r:f32,h:f32,inside:bool,px:f32)->vec4f{
    let cover=clamp((h*.5-abs(hit.y))/px,0.,1.);
    if cover<=0.{return vec4f(0);}
    let st=vec2f(.5+atan2(hit.x,hit.z)/TAU,hit.y/h+.5);
    let normal=rot*vec3f(hit.x/r,0,hit.z/r);
    return illumination(surface_color(st),select(normal,-normal,inside),15u,21u)*cover;
}
fn shade(p:vec2f)->vec4f{
    let depth=max(.001,1.+value(6)*u.scale/max(1.,layer_size().x));
    let center=layer_position(vec2f(.5))+vec2f(value(4),value(5))*u.scale;
    let radius=layer_size().x*value(2)*.01/TAU;let height=layer_size().y;
    if radius<=0.{return vec4f(0);}
    let rot=rotation(radians(vec3f(value(9),value(10),value(11))),i32(value(27)));
    let origin=transpose(rot)*vec3f((p-center)*depth,0);
    let ray=transpose(rot)*vec3f(0,0,1);
    let a=dot(ray.xz,ray.xz);let b=dot(origin.xz,ray.xz);
    let discr=b*b-a*(dot(origin.xz,origin.xz)-radius*radius);
    if discr<=0. || a<.000001{return vec4f(0);}
    let far=origin+ray*((-b-sqrt(discr))/a);
    let near=origin+ray*((-b+sqrt(discr))/a);
    // Silhouette coverage: the ray's distance from the axis, in pixels.
    let edge=clamp((radius-sqrt(max(0.,dot(origin.xz,origin.xz)-b*b/a)))/depth,0.,1.);
    if value(13)==3.{return cylinder_face(far,rot,radius,height,true,depth)*edge;}
    var c=cylinder_face(near,rot,radius,height,false,depth);
    if value(13)==1. && c.a<1.{c+=(1.-c.a)*cylinder_face(far,rot,radius,height,true,depth);}
    return c*edge;
}
