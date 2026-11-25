fn original(p:vec2f)->vec4f { return fx_bilinear(input_tex,u.images[0],p,false); }
fn centered(p:vec2f,img:FxImage)->vec2f {
    return p-0.5*vec2f(u.layer.xy+u.layer.zw)+img.origin+0.5*vec2f(img.size)/img.scale;
}
fn reveal(p:vec2f)->vec4f {
    if !has(0u) { return vec4f(0); }
    return fx_bilinear(map_a,u.images[1],centered(p,u.images[1]),false);
}
fn gradient_height(p:vec2f)->f32 {
    if !has(1u) { return 0.5; }
    return fx_luma(aux_b(centered(p,u.images[2])).rgb);
}
fn glass(p:vec2f,t:f32)->vec4f {
    if t<=0.0 { return original(p); }
    if t>=1.0 { return reveal(p); }
    let d=max(1.0,value(4u)*0.15*u.scale);
    let left=gradient_height(p-vec2f(d,0)); let right=gradient_height(p+vec2f(d,0));
    let top=gradient_height(p-vec2f(0,d)); let bottom=gradient_height(p+vec2f(0,d));
    let h=(4.0*gradient_height(p)+left+right+top+bottom)*0.125;
    let width=0.04+value(4u)*0.004;
    let k=smoothstep(h-width,h+width,t*(1.0+2.0*width)-width);
    let offset=vec2f(right-left,bottom-top)*value(5u)*u.scale;
    return mix(original(p+offset*k),reveal(p-offset*(1.0-k)),k);
}
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
fn reactor(p:vec2f)->f32 {
    let a=src(p).rgb; let b=fx_unpremult(reveal(p)).rgb;
    switch u32(value(3u)) {
        case 2u: { return (max(a.r,max(a.g,a.b))-min(a.r,min(a.g,a.b)))-(max(b.r,max(b.g,b.b))-min(b.r,min(b.g,b.b))); }
        case 3u: { return fx_luma(a-b); }
        case 4u: { return length(a-b)*0.57735; }
        default: { return fx_luma(a); }
    }
}
fn warp(p:vec2f,t:f32)->vec4f {
    if t<=0.0 { return original(p); }
    if t>=1.0 { return reveal(p); }
    let d=max(1.0,value(4u)*0.2*u.scale);
    var gradient=vec2f(reactor(p+vec2f(d,0))-reactor(p-vec2f(d,0)),reactor(p+vec2f(0,d))-reactor(p-vec2f(0,d)));
    if value(6u)==3.0 { gradient=vec2f(-gradient.y,gradient.x); }
    let delta=gradient*value(5u)*u.scale;
    let width=value(7u)*0.005;
    let blend=smoothstep(0.5-width,0.5+width,t);
    return mix(original(p+t*delta),reveal(p+(1.0-t)*delta*select(1.0,-1.0,value(6u)!=1.0)),blend);
}
fn evaluate(p:vec2f)->vec4f {
    switch u.stage {
        case 0u: { return glass(p,value(1u)*0.01); }
        case 1u: { return twist(p,value(2u)); }
        default: { return warp(p,value(1u)*0.01); }
    }
}
