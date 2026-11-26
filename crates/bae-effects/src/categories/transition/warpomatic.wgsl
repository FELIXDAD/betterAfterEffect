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
fn evaluate(p:vec2f)->vec4f { return warp(p,value(1u)*0.01); }
