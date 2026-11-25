// Screen difference on the dominant axis. Normalize against the sampled
// backing colour so a lit screen reaches zero alpha at the default gain.
fn excess(rgb:vec3f, key:vec3f, balance:f32) -> f32 {
    var v=rgb.gbr;
    if key.r>=max(key.g,key.b) {v=rgb.rgb;}
    else if key.b>=key.g {v=rgb.brg;}
    return v.x-mix(v.y,v.z,balance);
}
fn raw_matte(source:vec3f)->f32 {
    let key=u.values[4].rgb;
    let balance=value(6)*0.01;
    let denom=excess(key,key,balance);
    if denom<=0.0001 {return 1.0;}
    let bias=select(u.values[8].rgb,u.values[7].rgb,enabled(9u));
    let weights=max(bias,vec3f(0.001));
    let rgb=source * dot(weights,vec3f(1./3.))/weights;
    return saturate(1.-excess(rgb,key,balance)*value(5)*0.01/denom);
}
fn clipped(a:f32)->f32 {
    return saturate((a-value(12)*0.01)/max((value(13)-value(12))*0.01,1e-6));
}
fn corrected(rgb:vec3f, base:u32)->vec3f {
    // Foreground and edge correction have the same controls at different IDs.
    let sat=value(base)*0.01;
    let contrast=value(base+1u)*0.01;
    let luma=dot(rgb,vec3f(0.299,0.587,0.114));
    var c=(mix(vec3f(luma),rgb,sat)-0.5)*(1.+contrast)/max(1.-contrast,0.01)+0.5+value(base+2u)*0.01;
    let suppress=u32(value(base+4u));
    if suppress>=2u {
        let secondary=suppress>=5u;
        let channel=(suppress-2u)%3u;
        var v=select(c,1.-c,secondary);
        let limit=mix(v[(channel+1u)%3u],v[(channel+2u)%3u],value(base+5u)*0.01);
        let drop=max(0.,v[channel]-limit)*value(base+6u)*0.01;
        v-=select(vec3f(0.),vec3f(drop),vec3u(0,1,2)==vec3u(channel));
        c=select(v,1.-v,secondary);
    }
    let hue=radians(value(base+9u));
    let axis=vec3f(0.57735027);
    let tint=vec3f(cos(hue),cos(hue-2.0944),cos(hue+2.0944))*value(base+10u)*0.01;
    return c*cos(hue)+cross(axis,c)*sin(hue)+axis*dot(axis,c)*(1.-cos(hue))+tint;
}
fn replace_colour(rgb:vec3f,source:vec3f,weight:f32,id:u32)->vec3f {
    let mode=u32(value(id));
    if mode==1u {return rgb;}
    var replacement=source;
    if mode==3u {replacement=u.values[id+1u].rgb;}
    if mode==4u {replacement=u.values[id+1u].rgb*dot(source,vec3f(0.299,0.587,0.114))*2.;}
    return mix(rgb,replacement,saturate(weight));
}
fn crop_axis(x:f32,a:f32,b:f32,mode:u32)->f32 {
    let width=max(b-a,1.);
    if mode==3u {return a+width*(1.-abs(fract((x-a)/width*0.5)*2.-1.));}
    if mode==4u {return a+fract((x-a)/width)*width;}
    return clamp(x,min(a+0.5,b-0.5),max(a+0.5,b-0.5));
}
fn evaluate(pixel:vec2f)->vec4f {
    if u.stage == 2u {
        return fx_bilinear(input_tex, u.images[0], pixel, false);
    }
    if u.stage == 1u {
        let raw = raw_matte(src(pixel).rgb);
        var inside = 0.;
        var outside = 0.;
        if has(0u) { inside = aux_a(pixel).a; if enabled(25u) { inside = 1.-inside; } }
        if has(1u) { outside = aux_b(pixel).a; if enabled(33u) { outside = 1.-outside; } }
        // Raw values consumed independently by the final pass. Only alpha is filtered.
        return vec4f(raw, inside, outside, clipped(raw));
    }
    let size=vec2f(u.layer.zw-u.layer.xy);
    let lo=vec2f(u.layer.xy)+size*vec2f(value(75),value(77))*0.01;
    let hi=vec2f(u.layer.xy)+size*vec2f(value(76),value(78))*0.01;
    var p=pixel;
    if p.x<lo.x || p.x>=hi.x {
        if value(71)==1. {return output(u.values[73].rgb,value(74)*0.01);}
        p.x=crop_axis(p.x,lo.x,hi.x,u32(value(71)));
    }
    if p.y<lo.y || p.y>=hi.y {
        if value(72)==1. {return output(u.values[73].rgb,value(74)*0.01);}
        p.y=crop_axis(p.y,lo.y,hi.y,u32(value(72)));
    }
    let source=src(p);
    let view=u32(value(2));
    if view==1u {return output(source.rgb,source.a);}
    if view==2u {return gray(source.a);}
    let matte = fx_bilinear(map_a, u.images[1], p, false);
    let raw = matte.r;
    var inside = matte.g;
    let outside = matte.b;
    var screen = aux_b(p).a;
    if has(2u) {
        let blurred = aux_c(p).a;
        screen = mix(screen, raw, saturate(4. * blurred * (1. - blurred)));
    }
    if value(28)==2. {inside=max(inside,source.a);}
    let coverage=max(screen,inside)*(1.-outside);
    let alpha=coverage*select(1.,source.a,value(28)==3.);
    switch view {
        case 5u: {return gray(screen);}
        case 6u: {return gray(inside);}
        case 7u: {return gray(outside);}
        case 8u: {return gray(alpha);}
        case 9u: {return gray(select(select(0.5,1.,alpha>=0.999),0.,alpha<=0.001));}
        case 10u: {return output(source.rgb,alpha);}
        default: {}
    }
    let edge_width=max(value(54)+abs(value(55)),0.)*u.scale;
    var edge=4.*coverage*(1.-coverage);
    if edge_width>0. {
        let dx=vec2f(edge_width,0.);
        let span=abs(fx_bilinear(map_a,u.images[1],p+dx,false).r-fx_bilinear(map_a,u.images[1],p-dx,false).r);
        edge=saturate(edge+span*sign(value(55))+0.5*span*sign(value(54)));
    }
    edge=pow(saturate(edge),exp2((value(53)-50.)/50.));
    if view==4u {return gray(edge);}
    let bias=u.values[7].rgb;
    var rgb=source.rgb-(1.-raw)*u.values[4].rgb;
    if enabled(3) {rgb/=max(raw,0.0001);}
    rgb+=(bias-vec3f(dot(bias,vec3f(1./3.))))*(1.-raw);
    rgb=replace_colour(rgb,source.rgb,max(screen-raw,0.),19u);
    rgb=replace_colour(rgb,source.rgb,max(inside-screen,0.),26u);
    if enabled(36){rgb=corrected(rgb,37u);}
    if enabled(52){rgb=mix(rgb,corrected(rgb,56u),edge);}
    return output(rgb,select(alpha,source.a,view==3u));
}
