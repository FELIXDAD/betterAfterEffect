fn point(id: u32) -> vec2f { return u.values[id].xy; }
fn perpendicular(v: vec2f) -> vec2f { return vec2f(-v.y, v.x); }
fn warped(p: vec2f) -> vec4f {
    return fx_bilinear(input_tex, u.images[0], p, false);
}

fn bend(p: vec2f) -> vec2f {
    let axis = point(4u) - point(5u);
    let span = length(axis);
    if span < 0.001 { return p; }
    let dir = axis / span;
    let t = clamp(dot(p - point(5u), dir) / span, 0.0, 1.0);
    var shape = t*t;
    switch u32(value(2u)) {
        case 2u: { shape = sin(t * 6.2831853); }
        case 3u: { shape = 1.0 - abs(2.0*t-1.0); }
        case 4u: { shape = smoothstep(0.0, 1.0, t); }
        default: {}
    }
    return p - perpendicular(dir) * value(1u) * select(u.scale, span * 0.01, enabled(3u)) * shape;
}

fn blob_height(p: vec2f) -> f32 {
    let c = aux_a(p);
    let channels = array<u32,6>(2,3,4,1,5,7);
    return fx_channel(c,channels[clamp(u32(value(3u)),1u,6u)-1u]);
}
fn blob(p: vec2f) -> vec4f {
    if !has(0u) { return warped(p); }
    let radius = max(1.0, value(4u) * u.scale * 0.25);
    let dx = vec2f(radius,0); let dy = dx.yx;
    let l = blob_height(p-dx); let r = blob_height(p+dx);
    let t = blob_height(p-dy); let b = blob_height(p+dy);
    let h = (4.0*blob_height(p) + l+r+t+b)/8.0;
    let cutoff = value(5u)*0.01;
    let edge = max(0.002, 0.25*(abs(l-r)+abs(t-b)));
    let coverage = smoothstep(cutoff-edge,cutoff+edge,h);
    let normal = normalize(vec3f(l-r,t-b,0.35));
    let angle = radians(value(13u));
    var light_xy = vec2f(sin(angle),-cos(angle));
    if value(10u)==2.0 { light_xy = (point(12u)-p)/max(1.0,0.25*length(vec2f(u.layer.zw-u.layer.xy))); }
    let light = normalize(vec3f(light_xy, value(11u)*0.01 + 0.00001));
    // Coverage keeps the blob layer's own alpha in place: sampling alpha at the
    // refracted point read transparency inside every edge and eroded the layer.
    // Refraction moves only the colour, and by at most 2 px.
    let base = src(p);
    let refracted = src(p+normal.xy*min(radius,2.0));
    let c = vec4f(mix(base.rgb,refracted.rgb,saturate(refracted.a)),base.a);
    if value(22u)==2.0 {
        return output(surface_lights(p,normal,c.rgb,value(16u)*0.01,value(17u)*0.01,value(18u)*0.01,value(19u),value(20u)*0.01),c.a*coverage);
    }
    let diffuse = max(0.0,dot(normal,light))*value(17u)*0.01;
    let shine = pow(max(0.0,dot(reflect(-light,normal),vec3f(0,0,1))),1.0/max(0.001,value(19u)))*value(18u)*0.01;
    let lit = u.values[9].rgb * (value(8u)*0.01);
    return output(c.rgb*(value(16u)*0.01+lit*diffuse)+lit*shine*mix(vec3f(1),c.rgb,value(20u)*0.01),c.a*coverage);
}

fn vortex(p:vec2f, center:vec2f, amount:f32, size:f32) -> vec2f {
    let v = p-center;
    let d = length(v)/size;
    let gain = amount * select(0.025,0.00125,enabled(1u));
    return v * gain / pow(max(d,0.01), 2.0/(1.0+value(8u)));
}
fn flow_position(p:vec2f) -> vec2f {
    let size = vec2f(u.layer.zw-u.layer.xy);
    let radius = max(1.0,min(size.x,size.y)*0.5);
    var q = p + vortex(p,point(2u),value(3u),radius) + vortex(p,point(4u),value(5u),radius);
    if enabled(6u) {
        let origin=vec2f(u.layer.xy);
        let tile=1.0-abs(fract((q-origin)/(2.0*size))*2.0-1.0);
        q=origin+clamp(tile*size,vec2f(0.5),size-0.5);
    }
    return q;
}
fn flow(p:vec2f) -> vec4f {
    if value(3u)==0.0 && value(5u)==0.0 { return warped(p); }
    if value(7u)==1.0 { return fx_sample(input_tex,u.images[0],flow_position(p),false); }
    if value(7u)==2.0 { return warped(flow_position(p)); }
    let d=vec2f(0.25);
    return 0.25*(warped(flow_position(p-d))+warped(flow_position(p+d))
        +warped(flow_position(p+vec2f(d.x,-d.y)))+warped(flow_position(p+vec2f(-d.x,d.y))));
}

fn ripple(p:vec2f) -> vec4f {
    let v=p-point(1u);
    let range=max(1.0,length(vec2f(u.layer.zw-u.layer.xy))*0.5);
    let age=clamp(length(v)/range,0.0,1.0)*63.0;
    // The last 64 uniform slots hold the pulse level at successive delays.
    let i=min(u32(age),62u);
    let slope=(value(170u+i)-value(169u+i))*0.01;
    let height=mix(value(169u+i),value(170u+i),fract(age));
    if enabled(5u) { return gray(0.5+(height-value(232u))*value(4u)*0.00005); }
    let shift=slope*value(4u)*u.scale*(1.0-age/63.0);
    return warped(p-v/max(length(v),1.0)*shift);
}

fn slant(p:vec2f) -> vec4f {
    let amount=value(1u)*0.01;
    let angle=amount*1.5707963;
    let sy=value(3u)*0.01*select(cos(angle),1.0,enabled(2u));
    if abs(sy)<0.00001 { return vec4f(0); }
    let y=(p.y-point(4u).y)/sy;
    let q=vec2f(p.x+y*select(sin(angle),amount,enabled(2u)),point(4u).y+y);
    let c=warped(q);
    if enabled(5u) { return output(u.values[6].rgb,c.a); }
    return c;
}
fn smear(p:vec2f) -> vec2f {
    let start=point(1u);
    let delta=(point(2u)-start)*value(3u)*0.01;
    let along=clamp(dot(p-start,delta)/max(0.0001,dot(delta,delta)),0.0,1.0);
    let distance_to_stroke=distance(p,start+along*delta);
    let t=clamp(1.0-distance_to_stroke/max(0.001,value(4u)*u.scale),0.0,1.0);
    var weight=t;
    switch u32(value(5u)) {
        case 2u: { weight=sqrt(t*(2.0-t)); }
        case 3u: { weight=t*t*(3.0-2.0*t); }
        case 4u: { weight=t*t*t; }
        default: {}
    }
    return p-delta*along*weight;
}
fn split(p:vec2f, second:f32) -> vec4f {
    let axis=point(2u)-point(1u);
    let span=length(axis);
    if span<0.001 { return warped(p); }
    let dir=axis/span;
    let t=clamp(dot(p-point(1u),dir)/span,0.0,1.0);
    let n=perpendicular(dir);
    let h=dot(p-point(1u),n);
    let gap=sin(3.14159265*t)*select(value(3u),second,h>=0.0)*0.01*span*0.5;
    if abs(h)<gap { return vec4f(0); }
    return warped(p-n*sign(h)*gap);
}
fn evaluate(p:vec2f) -> vec4f {
    switch u.stage {
        case 0u: { return warped(bend(p)); }
        case 1u: { return blob(p); }
        case 2u: { return flow(p); }
        case 3u: { return ripple(p); }
        case 4u: { return slant(p); }
        case 5u: { return warped(smear(p)); }
        case 6u: { return split(p,value(3u)); }
        default: { return split(p,value(4u)); }
    }
}
