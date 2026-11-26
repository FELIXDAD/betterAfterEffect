fn bae_unpremultiply(c: vec4f) -> vec4f { return fx_unpremult(c); }

fn soft_alpha(p: vec2f) -> f32 {
    return fx_bilinear(map_a, u.images[1], p, false).a;
}

fn shade(p: vec2f) -> vec4f {
    let shape = fx_sample(input_tex, u.images[0], p, false).a;
    if u.stage == 5u { return vec4f(0,0,0,shape); }
    let base = fx_sample(map_b, u.images[2], p, false);
    let geometry = u.values[202];
    let light = u.values[203];
    var color = u.values[200];
    var mode = u32(light.w);
    var coverage = soft_alpha(p - geometry.xy);
    let options=u.values[204];
    let paint=u.values[205];
    let exterior=u.stage==0u || u.stage==2u;
    var support=shape;
    var signed_distance=0.;
    if has(2u) {
        let field=fx_sample(map_c,u.images[3],p,false);
        signed_distance=select(1e5,length(field.xy),field.z>0.5)*select(1.,-1.,shape>=0.5);
        if u.stage==2u || u.stage==3u {
            let falloff=saturate(1.-abs(signed_distance)/max(options.w,0.001));
            coverage=select(falloff,1.-falloff,u.stage==3u);
        }
    }
    switch u.stage {
        case 0u, 2u: {
            support=1.;
            if (u.stage==0u && paint.w!=0.) || u.stage==2u {support=1.-shape;}
        }
        case 1u: {coverage=1.-coverage;}
        case 3u: {coverage=select(1.-coverage,coverage,geometry.w!=0.);}
        case 4u: {
            var gradient=vec2f(soft_alpha(p+vec2f(1,0))-soft_alpha(p-vec2f(1,0)),
                               soft_alpha(p+vec2f(0,1))-soft_alpha(p-vec2f(0,1)));
            if has(2u) {
                let field=fx_sample(map_c,u.images[3],p,false);
                let normal=field.xy/max(length(field.xy),0.001)*select(1.,-1.,signed_distance<0.);
                let t=saturate(1.-abs(signed_distance)/max(options.w,.001));
                let weight=select(select(0.,1.,t>0.),t,options.z==3.);
                gradient=normal*weight/max(options.w,1.);
            }
            var slope=dot(gradient,light.xy);
            if light.z!=0. {slope*=1.-2.*coverage;}
            if slope<0. {color=u.values[201];mode=u32(options.x);}
            coverage=abs(slope);
            if options.y==1. {support=1.-shape;}
            if options.y>=3. {support=1.;}
        }
        default: {}
    }
    var density=saturate(coverage*geometry.z);
    let random=style_random(p,19273u);
    density=saturate(density+(random-0.5)*4.*density*(1.-density)*paint.x);
    if paint.z!=0. {
        let t=saturate(density+(random-0.5)*paint.y)*127.;
        let stop=mix(u.values[16u+u32(t)],u.values[16u+min(127u,u32(t)+1u)],fract(t));
        color=vec4f(stop.rgb,color.a*stop.a);
    }
    let a=density*color.a*support;
    if exterior {
        if u.index!=0u {return style_blend(base,color.rgb,a,mode,p);}
        return base+vec4f(color.rgb*a,a)*(1.-base.a);
    }
    if u.stage==4u && options.y!=2. {return style_blend(base,color.rgb,a,mode,p);}
    let rgb=fx_unpremult(base).rgb;
    let blended=style_blend(vec4f(rgb,1.),color.rgb,1.,mode,p).rgb;
    let alpha=max(base.a,a);
    return vec4f(mix(base.rgb,blended*alpha,a/max(alpha,.00001)),alpha);
}
