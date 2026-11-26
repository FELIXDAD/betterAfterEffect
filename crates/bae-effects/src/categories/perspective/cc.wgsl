fn cross_uv(ray:vec3f)->vec2f {
    let a=abs(ray);
    var face=vec2f(1,1); var xy=vec2f(0);
    if a.x>=a.y && a.x>=a.z {
        face.x=select(0.0,2.0,ray.x>0); xy=vec2f(-sign(ray.x)*ray.z,ray.y)/a.x;
    } else if a.y>=a.z {
        face.y=select(0.0,2.0,ray.y>0); xy=vec2f(ray.x,-sign(ray.y)*ray.z)/a.y;
    } else if ray.z>=0 { xy=ray.xy/a.z; }
    else { face.y=3.0; xy=vec2f(ray.x,-ray.y)/a.z; }
    return (face+0.5+0.5*xy)/vec2f(3,4);
}
fn evaluate(p:vec2f)->vec4f {
    if !has(0u) { return vec4f(0); }
    let zoom=max(0.0001,u.values[61].w*u.scale);
    var q=(p-0.5*vec2f(u.layer.xy+u.layer.zw))/zoom;
    q*=exp2(value(5u)*0.006*min(dot(q,q),8.0));
    let d=normalize(u.values[61].xyz*q.x+u.values[62].xyz*q.y+u.values[63].xyz);
    let pan=radians(value(2u));
    let ray=vec3f(cos(pan)*d.x+sin(pan)*d.z,d.y,-sin(pan)*d.x+cos(pan)*d.z);
    var uv=vec2f(fract(0.5+atan2(ray.x,ray.z)/6.2831853),0.5+asin(clamp(ray.y,-1.0,1.0))/3.14159265);
    if value(4u)==2.0 { uv=0.5+ray.xy/max(0.00001,length(ray.xy))*acos(clamp(ray.z,-1.0,1.0))/6.2831853; }
    if value(4u)==3.0 { uv=cross_uv(ray); }
    let img=u.images[1]; let pixels=vec2f(img.size);
    let v=clamp(uv*pixels,vec2f(0.5),pixels-0.5);
    let pos=img.origin+v/img.scale;
    if !enabled(3u) { return fx_sample(map_a,img,pos,false); }
    return fx_bilinear(map_a,img,pos,false);
}
