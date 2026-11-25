// Match-name adapters use the same pipeline and the same overlap functions as generated colors.
fn bae_unpremultiply(c:vec4f)->vec4f{return fx_unpremult(c);}
fn from_layer(p:vec2f,stretch:bool)->vec4f{return layer_sample(map1,u.images[1],p,stretch);}
fn overlap(base:vec3f,color:vec3f,mode:u32)->vec3f {
    return generated_blend(vec4f(base,1.),color,1.,mode).rgb;
}
fn selection(c:vec4f,channel:u32,invert:bool)->vec4f{
    var s=fx_unpremult(c);
    if channel==2u{s=vec4f(vec3f(fx_luma(s.rgb)),s.a);}
    if channel>=3u{s=vec4f(vec3f(s[min(channel-3u,3u)]),s.a);}
    if invert{s=vec4f(1.-s.rgb,s.a);}
    return vec4f(s.rgb*s.a,s.a);
}
fn composite(base:vec4f,top:vec4f,opacity:f32,mode:u32)->vec4f{
    let c=fx_unpremult(top);let a=c.a*opacity;
    switch mode {
        case 33u:{return base*a;}case 34u:{return base*(fx_luma(c.rgb)*a);}
        case 35u:{return base*(1.-a);}case 36u:{return base*(1.-fx_luma(c.rgb)*a);}
        case 37u:{return vec4f(base.rgb+top.rgb*opacity,min(1.,base.a+a));}
        case 38u:{return vec4f(base.rgb*(1.-a)+top.rgb*opacity,max(base.a,a));}
        default:{return generated_blend(base,c.rgb,a,mode);}
    }
}
fn arithmetic(b:vec3f,s:vec3f,op:u32)->vec3f {
    let bi=vec3u(clamp(round(b*255.),vec3f(0),vec3f(255)));
    let si=vec3u(clamp(round(s*255.),vec3f(0),vec3f(255)));
    switch op {
        case 1u:{return vec3f(bi&si)/255.;}
        case 2u:{return vec3f(bi|si)/255.;}
        case 3u:{return vec3f(bi^si)/255.;}
        case 5u:{return b-s;}
        case 9u:{return select(vec3f(0),b,b>=s);}
        case 10u:{return select(vec3f(1),b,b<=s);}
        case 11u:{return clamp(b,min(s,1.-s),max(s,1.-s));}
        default:{
            let modes=array<u32,16>(2,2,2,2,4,2,16,14,15,2,2,2,5,6,7,9);
            return overlap(b,s,modes[min(op,15u)]);
        }
    }
}
fn compound_value(b:vec3f,s:vec3f,op:u32,overflow:u32)->vec3f{
    let ops=array<u32,15>(0,4,5,12,6,1,2,3,8,7,7,8,13,14,15);
    var r=arithmetic(b,s,ops[clamp(op,1u,15u)-1u]);
    switch overflow{
        case 1u:{r=saturate(r);}
        case 2u:{r=select(fract(r),r,(r>=vec3f(0))&(r<=vec3f(1)));}
        case 3u:{if op==2u {r*=.5;}else if op==3u{r=.5*(r+1.);}}
        default:{}
    }
    return r;
}
fn effect(p:vec2f)->vec4f {
    let b=source(p);let bs=fx_unpremult(b);
    switch u.mode{
        case 1u:{ // Arithmetic
            var rgb=arithmetic(bs.rgb,vec3f(v(2),v(3),v(4))/255.,u32(v(1)));
            if v(5)!=0.{rgb=saturate(rgb);}return vec4f(rgb*b.a,b.a);
        }
        case 2u:{ // Blend
            if u.images[1].size.x==0u{return b;}
            let top=from_layer(p,v(4)==2.);let modes=array<u32,5>(2,21,19,14,15);
            let mode=u32(v(2));var result=top;
            if mode!=1u {result=composite(b,top,1.,modes[clamp(mode,1u,5u)-1u]);}
            return mix(result,b,v(3));
        }
        case 3u:{ // Calculations
            let src=selection(b,u32(v(2)),v(3)!=0.);
            let top=selection(from_layer(p,v(10)!=0.),u32(v(7)),v(9)!=0.);
            let modes=array<u32,37>(2,1,2,14,5,12,12,2,4,15,6,11,11,2,7,8,9,25,26,27,2,16,16,17,2,19,20,21,22,2,33,34,35,36,2,37,38);
            let out=composite(src,top,v(8),modes[clamp(u32(v(12)),1u,37u)-1u]);
            if v(13)!=0.{return vec4f(fx_unpremult(out).rgb*b.a,b.a);}return out;
        }
        case 4u:{ // Compound Arithmetic
            let t=fx_unpremult(from_layer(p,v(5)!=0.));let op=u32(v(2));let overflow=u32(v(4));let ch=u32(v(3));
            var rgb=bs.rgb;var a=bs.a;
            if ch!=3u{rgb=compound_value(bs.rgb,t.rgb,op,overflow);}
            if ch!=1u{a=compound_value(vec3f(bs.a),vec3f(t.a),op,overflow).r;}
            return mix(vec4f(rgb*a,a),b,v(6)*.01);
        }
        default:{ // CC Composite
            let t=from_layer(p,false);let opacity=v(1)*.01;let choice=u32(v(2));
            let modes=array<u32,41>(1,2,2,2,14,5,23,12,12,2,4,15,6,24,11,11,2,7,8,9,25,26,27,28,2,16,16,17,2,19,20,21,22,2,33,34,35,36,2,37,38);
            var out=composite(b,t,opacity,modes[clamp(choice,1u,41u)-1u]);
            if choice==2u{out=b+(1.-b.a)*t*opacity;}
            if v(3)!=0.{return vec4f(fx_unpremult(out).rgb*b.a,b.a);}return out;
        }
    }
}