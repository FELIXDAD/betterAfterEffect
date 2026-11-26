fn bae_unpremultiply(c:vec4f)->vec4f { return fx_unpremult(c); }
fn blend_range(v:f32,limits:vec4f)->f32 {
    let low=select(select(0.,1.,v>=limits.x),saturate((v-limits.x)/max(limits.y-limits.x,1e-7)),limits.y>limits.x);
    let high=select(select(0.,1.,v<=limits.w),saturate((limits.w-v)/max(limits.w-limits.z,1e-7)),limits.w>limits.z);
    return low*high;
}
fn original(p:vec2f)->f32 { return fx_sample(input_tex,u.images[0],p,false).a; }
fn field_at(p:vec2f)->vec4f { return fx_sample(map_b,u.images[2],p,false); }
fn shade(p:vec2f)->vec4f {
    let source=fx_sample(input_tex,u.images[0],p,false);
    let shape=source.a;
    if u.stage==5u {
        // Seed the contour in texel coordinates relative to each pixel. Small
        // signed offsets remain exact in half precision even on a large comp.
        let n=vec4f(original(p+vec2f(1,0)),original(p-vec2f(1,0)),
                    original(p+vec2f(0,1)),original(p-vec2f(0,1)));
        let gradient=vec2f(n.x-n.y,n.z-n.w)*0.5;
        let crossing = min(min(n.x,n.y), min(n.z,n.w)) < 0.5 && max(max(n.x,n.y),max(n.z,n.w)) >= 0.5;
        if (shape>0.0 && shape<1.0) || crossing || (shape>=0.5 && any(n<vec4f(0.5))) {
            var nearest=vec2f(0);
            var best=1e6;
            let offsets=array<vec2f,4>(vec2f(1,0),vec2f(-1,0),vec2f(0,1),vec2f(0,-1));
            for(var i=0u;i<4u;i++) {
                if (n[i]>=0.5)!=(shape>=0.5) {
                    let t=(0.5-shape)/(n[i]-shape);
                    if t<best {best=t;nearest=offsets[i]*t;}
                }
            }
            if best==1e6 {nearest=normalize(gradient+vec2f(1e-8))*clamp(0.5-shape,-0.5,0.5);}
            return vec4f(nearest,1,1);
        }
        return vec4f(0);
    }
    if u.stage==6u {
        var nearest=vec2f(0);
        var best=1e20;
        for(var y=-1;y<=1;y++) { for(var x=-1;x<=1;x++) {
            let d=vec2f(f32(x),f32(y))*f32(u.index);
            let seed=field_at(p+d);
            let delta=d+seed.xy;
            let distance2=dot(delta,delta);
            if seed.z>0.5 && distance2<best {best=distance2;nearest=delta;}
        }}
        return vec4f(nearest,select(0.0,1.0,best<1e20),1.0);
    }
    let base=fx_sample(map_a,u.images[1],p,false);
    let settings=u.values[200];
    var color=u.values[201];
    var coverage=shape;
    if u.stage==0u {return source*settings.x;}
    if u.stage==8u {return select(vec4f(0),base,has(0u));}
    if u.stage==9u {
        let fill=fx_sample(map_b,u.images[2],p,false)*settings.x;
        return style_blend(base,fx_unpremult(fill).rgb,fill.a,u.index,p);
    }
    if u.stage==7u {
        var back=vec4f(0);
        if has(1u) {back=fx_sample(map_b,u.images[2],p,false);}
        let area=u.values[202];
        if any(p<area.xy) || any(p>=area.zw) {return back;}
        var weight=settings.x;
        if has(2u) {
            let matte=fx_sample(map_c,u.images[3],p,false);
            var m=select(matte.a,dot(matte.rgb,vec3f(0.299,0.587,0.114)),u.index>=3u);
            if u.index==2u || u.index==4u {m=1.-m;}
            weight*=saturate(m);
        }
        if settings.z>0. {
            let src=fx_unpremult(source).rgb;
            let dst=fx_unpremult(back).rgb;
            let luma=vec3f(.299,.587,.114);
            let sv=vec4f(dot(src,luma),src);
            let dv=vec4f(dot(dst,luma),dst);
            for(var i=0u;i<u32(settings.z);i++) {
                weight*=blend_range(sv[i],u.values[16u+2u*i])*blend_range(dv[i],u.values[17u+2u*i]);
            }
        }
        var merged=mix(back,base,weight);
        let rgb=mix(fx_unpremult(back).rgb,fx_unpremult(merged).rgb,u.values[201].rgb);
        if settings.y!=0. {merged.a=back.a;}
        return vec4f(rgb*merged.a,merged.a);
    }
    if u.stage==2u {
        let frame=u.values[202];
        let axis=u.values[203];
        let v=(p-frame.xy)/max(frame.zw,vec2f(0.001));
        let t=dot(v,axis.xy);
        var f=0.5+t;
        switch u.index {
            case 2u: {f=length(v)*2.;}
            case 3u: {f=fract(atan2(v.y,v.x)/6.2831853-atan2(axis.y,axis.x)/6.2831853+1.);}
            case 4u: {f=abs(t)*2.;}
            case 5u: {f=2.*(abs(t)+abs(dot(v,vec2f(-axis.y,axis.x))));}
            default: {}
        }
        if axis.z!=0. {f=1.-f;}
        let n=saturate(f)*127.;
        color=mix(u.values[16u+u32(n)],u.values[16u+min(127u,u32(n)+1u)],fract(n));
    }
    if u.stage==3u {
        let d=u.values[202].xy;
        var satin=abs(fx_bilinear(map_b,u.images[2],p+d,false).a-fx_bilinear(map_b,u.images[2],p-d,false).a);
        if u.index!=0u {satin=1.-satin;}
        coverage*=satin;
    }
    if u.stage==4u {
        let seed=field_at(p);
        let distance=select(1e6,length(seed.xy),seed.z>0.5)*select(1.,-1.,shape>=0.5);
        let width=settings.z;
        var range=vec2f(0,width);
        if u.index==2u {range=vec2f(-width,0);}
        if u.index==3u {range=vec2f(-width,width)*0.5;}
        coverage=saturate(distance-range.x+0.5)*saturate(range.y-distance+0.5);
        return style_blend(base,color.rgb,coverage*settings.x*color.a,u32(settings.y),p);
    }
    let a=saturate(coverage*settings.x*color.a);
    let overlap=style_blend(vec4f(fx_unpremult(base).rgb,1.),color.rgb,1.,u32(settings.y),p).rgb;
    let alpha=max(base.a,a);
    return vec4f(mix(base.rgb,overlap*alpha,a/max(alpha,0.00001)),alpha);
}

@compute @workgroup_size(8,8)
fn distance_field(@builtin(global_invocation_id) id:vec3u) {
    let p=u.roi.xy+vec2i(id.xy);
    if all(p<u.roi.zw){textureStore(out_tex,vec2i(id.xy),shade(vec2f(p)+0.5));}
}
