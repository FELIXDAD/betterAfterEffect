// Intersect the viewing ray with the rotated image plane and project back
// onto its two local axes. The same intersection shows the mirrored back face.
fn evaluate(p:vec2f)->vec4f {
    let size=vec2f(u.layer.zw-u.layer.xy);
    let center=vec2f(u.layer.xy)+size*0.5;
    let focal=max(1.,size.x*1.3);
    let a=radians(value(1));
    let b=radians(value(2));
    let right=vec3f(cos(a), sin(a)*sin(b), -sin(a)*cos(b));
    let down=vec3f(0.,cos(b),sin(b));
    let normal=cross(right,down);
    let ray=vec3f((p-center)/focal,1.);
    let z=focal*(1.+value(3)*0.01);
    let denom=dot(normal,ray);
    if abs(denom)<1e-5 {return vec4f(0.);}
    let t=normal.z*z/denom;
    if t<=0. {return vec4f(0.);}
    let hit=ray*t-vec3f(0.,0.,z);
    let local=vec2f(dot(hit,right),dot(hit,down));
    let edge=size*0.5-abs(local);
    if any(edge<vec2f(0.)) {return vec4f(0.);}
    if u.stage==1u {return vec4f(f32(min(edge.x,edge.y)<max(t/focal,1.)));}
    let color=src(center+local);
    let light=normalize(vec3f(-0.45,-0.65,-1.));
    let halfway=normalize(light-normalize(ray));
    let shine=select(0.,pow(max(0.,dot(-normal,halfway)),60.),enabled(4));
    return output(color.rgb+shine,color.a);
}
