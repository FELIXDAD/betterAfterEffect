fn rotate(v:vec2f,a:f32)->vec2f {
    let cs=cos(a); let sn=sin(a);
    return vec2f(cs*v.x-sn*v.y,sn*v.x+cs*v.y);
}
fn hex_tile(p:vec2f)->vec4f {
    let radius=max(1.0,value(2u)*u.scale);
    let source=u.values[3].xy;
    let origin=select(source,0.5*vec2f(u.layer.xy+u.layer.zw),enabled(4u));
    let angle=radians(value(5u));
    let q=rotate(p-origin,-angle)/radius;
    // A triangular lattice consists of two staggered rectangular lattices.
    let pitch=vec2f(1.7320508,3.0);
    let a=q-pitch*round(q/pitch);
    let b=q-pitch*(round(q/pitch-0.5)+0.5);
    var cell=select(b,a,dot(a,a)<=dot(b,b));
    let center=q-cell;
    if value(1u)>1.0 {
        let sector=atan2(cell.y,cell.x)+3.14159265;
        var theta=sector-floor(sector/1.04719755)*1.04719755;
        if value(1u)==3.0 { theta=min(theta,1.04719755-theta); }
        cell=length(cell)*vec2f(cos(theta),sin(theta));
    }
    return fx_bilinear(input_tex,u.images[0],source+rotate(radius*(cell+center*value(6u)*0.01),angle),false);
}
fn channel(c:vec4f, index:u32)->f32 {
    let channels=array<u32,8>(2,3,4,1,5,7,6,8);
    return fx_channel(c,channels[clamp(index,1u,8u)-1u]);
}
fn relief(p:vec2f)->f32 {
    let low=value(6u)*0.01; let high=value(7u)*0.01;
    let h=channel(aux_a(p),u32(value(3u)));
    if abs(high-low)<0.0001 { return step(low,h); }
    return saturate((h-low)/(high-low));
}
fn plastic(p:vec2f)->vec4f {
    let c=fx_sample(input_tex,u.images[0],p,true);
    if !has(0u) { return output(c.rgb,c.a); }
    let d=max(1.0,value(4u)*u.scale*0.25);
    let normal=normalize(vec3f(vec2f(relief(p-vec2f(d,0))-relief(p+vec2f(d,0)),
        relief(p-vec2f(0,d))-relief(p+vec2f(0,d)))*value(5u)*u.scale/(2.0*d),1.0));
    let angle=radians(value(15u));
    var xy=vec2f(sin(angle),-cos(angle));
    if value(12u)==2.0 { xy=(u.values[14].xy-p)/max(1.0,0.25*length(vec2f(u.layer.zw-u.layer.xy))); }
    let light=normalize(vec3f(xy,value(13u)*0.01+0.00001));
    if value(26u)==2.0 {
        return output(surface_lights(p,normal,c.rgb,value(19u)*0.01,value(20u)*0.01,value(22u)*0.01,value(23u),value(24u)*0.01),c.a);
    }
    let lambert=max(0.0,dot(normal,light));
    let reflection=max(0.0,reflect(-light,normal).z);
    let spec=pow(reflection,1.0/max(0.001,value(23u)))*value(22u)*0.01;
    let lit=u.values[11].rgb*value(10u)*0.01;
    let ambient=u.values[16].rgb*value(19u)*0.01;
    let dust=(1.0-normal.z)*value(21u)*0.01;
    return output(c.rgb*(ambient+lit*lambert*value(20u)*0.01)+lit*(dust+spec*mix(vec3f(1),c.rgb,value(24u)*0.01)),c.a);
}
fn block_load(p:vec2f)->vec4f {
    let scans=clamp(round(value(2u)),1.0,16.0);
    let scan=value(1u)*0.01*(scans+f32(enabled(3u)))+f32(!enabled(3u));
    let local=p-vec2f(u.layer.xy);
    let extent=vec2f(u.layer.zw-u.layer.xy);
    let completed_scan=floor(scan)+f32(local.y/extent.y<fract(scan));
    if completed_scan==0.0 { return vec4f(0); }
    let step_size=exp2(min(scans+1.0-completed_scan,ceil(log2(max(extent.x,extent.y)))));
    let grid=(local-0.5)/step_size;
    let pos=floor(grid)*step_size+vec2f(u.layer.xy)+0.5;
    let hi=vec2f(u.layer.zw)-0.5;
    let a=fx_sample(input_tex,u.images[0],pos,false);
    if !enabled(4u) || step_size<=1.0 { return a; }
    let b=fx_sample(input_tex,u.images[0],min(pos+vec2f(step_size,0),hi),false);
    let c=fx_sample(input_tex,u.images[0],min(pos+vec2f(0,step_size),hi),false);
    let d=fx_sample(input_tex,u.images[0],min(pos+step_size,hi),false);
    return mix(mix(a,b,fract(grid.x)),mix(c,d,fract(grid.x)),fract(grid.y));
}
fn smoothie(p:vec2f)->vec4f {
    if !has(0u) { return fx_sample(input_tex,u.images[0],p,false); }
    let d=value(3u)*u.scale*0.25;
    var c=aux_a(p)*0.5;
    c+=0.125*(aux_a(p+vec2f(d,0))+aux_a(p-vec2f(d,0))+aux_a(p+vec2f(0,d))+aux_a(p-vec2f(0,d)));
    let v=saturate(channel(c,u32(value(2u))));
    let phase=value(6u)/360.0;
    var t=v;
    if phase!=0.0 { t=fract(v+phase); }
    if value(7u)>=3.0 { t=1.0-abs(2.0*t-1.0); }
    if value(7u)==2.0 || value(7u)==4.0 { t=1.0-t; }
    let rgb=src(mix(u.values[4].xy,u.values[5].xy,t)).rgb;
    return output(rgb,fx_sample(input_tex,u.images[0],p,true).a);
}
fn evaluate(p:vec2f)->vec4f {
    switch u.stage {
        case 0u: { return hex_tile(p); }
        case 1u: { return plastic(p); }
        case 2u: { return block_load(p); }
        default: { return smoothie(p); }
    }
}
