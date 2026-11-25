fn sphere_face(n:vec3f,rot:mat3x3f,inside:bool)->vec4f{
    let object=transpose(rot)*n;
    let st=vec2f(.5+atan2(object.x,object.z)/TAU,.5+asin(clamp(object.y,-1.,1.))/(.5*TAU));
    var c=surface_color(st);
    let normal=select(n,-n,inside);c=illumination(c,normal,10u,16u);
    if v(21)>0. && u.images[1].size.x>0u{
        let ray=reflect(vec3f(0,0,-1),normal);
        let r_uv=vec2f(.5+atan2(ray.x,ray.z)/TAU,.5+asin(clamp(ray.y,-1.,1.))/(.5*TAU));
        let refl=layer_sample(map1,u.images[1],pos(r_uv),true);
        c=vec4f(mix(c.rgb,fx_unpremult(refl).rgb*c.a,v(21)*.01*refl.a),c.a);
    }
    if inside && v(23)>.5 {c=vec4f(c.rgb*.5,c.a);}
    if v(24)>.5 {c*=abs(n.z);}
    return c;
}
fn effect(p:vec2f)->vec4f{
    let radius=v(6)*u.scale;if radius<=0.{return vec4f(0);}
    let xy=(p-pt(7))/radius;let d=dot(xy,xy);if d>=1.{return vec4f(0);}
    let z=sqrt(1.-d);let rot=rotation(radians(vec3f(v(2),v(3),v(4))),i32(v(26)));
    let edge=clamp((1.-sqrt(d))*radius,0.,1.);let mode=i32(v(8));
    if mode==3{return sphere_face(vec3f(xy,-z),rot,true)*edge;}
    var c=sphere_face(vec3f(xy,z),rot,false);
    if mode==1 && c.a<1.{c+=(1.-c.a)*sphere_face(vec3f(xy,-z),rot,true);}
    return c*edge;
}