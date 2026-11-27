const TAU = 6.2831853;

fn ripple(p:vec2f, center:vec2f, base:u32, size:vec2f, t:f32) -> f32 {
    let d = (p-center)/max(1.,size.x);
    let a = radians(value(base+8u));
    let xy = vec2f(d.x*cos(a)+d.y*sin(a),d.y*cos(a)-d.x*sin(a));
    let extent = max(vec2f(0.001),vec2f(value(base+4u),value(base+6u)));
    var r = length(xy / extent) * min(extent.x,extent.y);
    if value(base)==2. {r=length(vec2f(max(0.,abs(xy.x)-extent.x*.5),xy.y*extent.x/extent.y));}
    let speed=max(0.001,value(30u)*0.25);
    let delay=r/speed;
    if t<delay {return 0.;}
    return value(base+10u)*sin((t-delay)*TAU*value(base+12u)+radians(value(base+14u)))
        *exp(-value(32u)*(t+delay)*2.)/sqrt(1.+32.*r);
}

fn height_at(p:vec2f, size:vec2f, origin:vec2f,t:f32)->f32{
    var h=0.;
    let masks=array<u32,16>(0,15,1,2,4,8,3,5,6,9,10,12,7,11,13,14);
    let edges=masks[clamp(u32(value(20u)),1u,16u)-1u];
    for(var k=0u;k<2u;k++){
        let id=42u+16u*k;
        if value(id+10u)==0. {continue;}
        let c=u.values[id+2u].xy-origin;
        h+=ripple(p,c,id,size,t);
        if (edges&1u)!=0u {h+=ripple(p,vec2f(-c.x,c.y),id,size,t);}
        if (edges&2u)!=0u {h+=ripple(p,vec2f(c.x,-c.y),id,size,t);}
        if (edges&4u)!=0u {h+=ripple(p,vec2f(2.*size.x-c.x,c.y),id,size,t);}
        if (edges&8u)!=0u {h+=ripple(p,vec2f(c.x,2.*size.y-c.y),id,size,t);}
    }
    return h;
}

fn evaluate(pixel:vec2f)->vec4f {
    let origin=vec2f(u.layer.xy);
    let size=max(vec2f(1.),vec2f(u.layer.zw-u.layer.xy));
    var p=pixel-origin;
    // Grid Resolution follows the render resolution when 92 is on (fewer
    // cells at a lower resolution); otherwise it stays as set.
    let spacing=max(1.,size.x/max(10.,value(26u)*select(1.,u.scale,enabled(92u))));
    let wire=value(2u)==2.;
    if wire{
        let a=radians(value(4u));
        let tilt=max(0.15,abs(cos(radians(value(6u)))));
        let d=(p-size*0.5)/vec2f(1.,tilt);
        p=size*0.5+vec2f(d.x*cos(a)+d.y*sin(a),d.y*cos(a)-d.x*sin(a));
    }
    if any(p<vec2f(0.)) || any(p>=size){return vec4f(0.);}
    let coord=p/spacing;
    let cell=floor(coord)*spacing;
    let f=fract(coord);
    let t=max(0.,u.time+value(24u));
    let h=mix(mix(height_at(cell,size,origin,t),height_at(cell+vec2f(spacing,0.),size,origin,t),f.x),
              mix(height_at(cell+vec2f(0.,spacing),size,origin,t),height_at(cell+vec2f(spacing),size,origin,t),f.x),f.y);
    var ground=0.;
    if has(0u){
        let g=aux_a(u.images[1].origin+(p/size)*vec2f(u.images[1].size)/u.images[1].scale);
        ground=smoothstep(0.,max(.001,1.-value(38u)),dot(g.rgb,vec3f(.2126,.7152,.0722)))*value(40u)*g.a;
    }
    let water=h*(1.-ground)+ground*(1.+value(90u)*h);
    if wire{
        let q=(p+vec2f(0.,water*value(8u)*size.y*.25))/spacing;
        let edge=abs(fract(q+0.5)-0.5)*spacing;
        let line=1.-smoothstep(.35,1.25,min(edge.x,edge.y));
        return output(vec3f(.52,.78,1.)*(.65+.35*clamp(water+0.5,0.,1.)),line);
    }
    let grey=pow(clamp(value(10u)+value(12u)*water,0.,1.),1./max(.001,value(14u)));
    let alpha=select(1.,clamp(1.-ground/max(.001,value(18u)),0.,1.),value(16u)==2.);
    return output(vec3f(grey),alpha);
}
