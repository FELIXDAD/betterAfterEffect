fn twist(p:vec2f,t:f32)->vec4f {
    if t<=0.0 { return original(p); }
    if t>=1.0 { return reveal(p); }
    let a=radians(value(5u));
    let axis=vec2f(cos(a),sin(a)); let n=vec2f(-axis.y,axis.x);
    let center=u.values[4].xy; let size=vec2f(u.layer.zw-u.layer.xy);
    let along=dot(p-center,axis);
    let u_line=clamp(along/max(1.0,dot(abs(axis),size))+0.5,0.0,1.0);
    let theta=3.14159265*clamp(2.0*t-u_line,0.0,1.0);
    let foreshortening=cos(theta);
    if abs(foreshortening)<0.0005 { return vec4f(0); }
    let q=center+axis*along+n*dot(p-center,n)/abs(foreshortening);
    var c=original(q);
    if foreshortening<0.0 { c=reveal(q); }
    if enabled(3u) { c=vec4f(c.rgb*(0.3+0.7*abs(foreshortening)),c.a); }
    return c;
}
fn evaluate(p:vec2f)->vec4f { return twist(p,value(2u)); }
