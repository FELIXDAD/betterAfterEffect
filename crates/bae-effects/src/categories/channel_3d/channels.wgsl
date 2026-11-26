struct Controls {
    image: FxImage, ramp: FxImage, roi: vec4i, planes: vec4i,
    method: u32, grid: f32, hdr: u32, has_ramp: u32,
    values: array<vec4f,9>,
    selected:u32, rank_count:u32, pad:vec2u, ranks:array<vec4i,100>,
}
@group(0) @binding(0) var<uniform> u: Controls;
@group(0) @binding(1) var beauty: texture_2d<f32>;
@group(0) @binding(2) var planes: texture_2d_array<u32>;
@group(0) @binding(3) var ramp: texture_2d<f32>;
@group(0) @binding(4) var dst: texture_storage_2d<WORKING_FORMAT,write>;
fn v(i:u32)->f32 { return u.values[i].x; }
fn word(id:i32, p:vec2i)->u32 {
    if id == -1 || any(p<vec2i(0)) || any(p>=vec2i(textureDimensions(planes))) { return 0u; }
    return textureLoad(planes,p,select(id,-2-id,id < -1),0).r;
}
fn scalar(id:i32,p:vec2i)->f32 {
    let bits=word(id,p);
    return select(bitcast<f32>(bits),f32(bits),id < -1);
}
fn color_id(id:u32)->vec3f {
    if id==0u {return vec3f(0.0);}
    let n = id * 2654435761u;
    return vec3f(vec3u(n>>16u,n>>8u,n)&vec3u(255u))/255.0;
}
fn level(x:f32,black:f32,white:f32)->f32 {
    if white==black {return step(black,x);}
    return (x-black)/(white-black);
}
fn extracted(p:vec2i)->vec3f {
    let a=scalar(u.planes.x,p);
    switch u32(v(1)) {
        case 1u: {
            var depth=level(a,v(2),v(3));
            if v(6)!=0.0 {depth=1.0-depth;}
            return vec3f(depth);
        }
        case 2u,8u: {return color_id(select(u32(a),word(u.planes.x,p),u.planes.x < -1));}
        case 5u: {return vec3f(a);}
        default: {
            let rgb=vec3f(a,scalar(u.planes.y,p),scalar(u.planes.z,p));
            return select(rgb,rgb*0.5+0.5,v(1)==4.0);
        }
    }
}
fn numeric_id(id:i32,p:vec2i)->u32 {
    if id == -1 {return 0u;}
    let bits=word(id,p);
    return select(u32(bitcast<f32>(bits)),bits,id < -1);
}
fn id_matte(q:vec2i)->f32 {
    var a=select(0.0,1.0,numeric_id(u.planes.x,q)==u.selected && u.planes.x != -1);
    if v(5)!=0.0 && u.planes.y != -1 { a*=clamp(scalar(u.planes.y,q),0.0,1.0); }
    return a;
}
fn crypto(q:vec2i,c:vec4f)->vec4f {
    var mask=0.0;
    var colors=vec3f(0.0);
    for(var i=0u;i<u.rank_count;i++){
        let rank=u.ranks[i];
        let id0=word(rank.x,q);
        let id1=word(rank.z,q);
        let w0=scalar(rank.y,q);
        let w1=scalar(rank.w,q);
        colors+=color_id(id0)*w0+color_id(id1)*w1;
        if u.selected!=0u {
            mask+=select(0.0,w0,id0==u.selected)+select(0.0,w1,id1==u.selected);
        }
    }
    mask=clamp(mask,0.0,1.0);
    switch u32(v(2)) {
        case 2u: {return vec4f(colors*mask,mask);}
        case 3u: {return vec4f(fx_unpremult(c).rgb*mask,mask);}
        case 4u: {return vec4f(vec3f(mask),1);}
        default: {return vec4f(colors,1);}
    }
}
fn evaluate(p:vec2f)->vec4f {
    let q=vec2i(floor(p*u.grid));
    let c=fx_bilinear(beauty,u.image,p,false);
    let z=scalar(u.planes.x,q);
    switch u.method {
        case 0u: {
            var rgb=extracted(q);
            if v(4)!=0.0 {
                rgb=rgb*0.5+0.125*(extracted(q+vec2i(1,0))+extracted(q-vec2i(1,0))+extracted(q+vec2i(0,1))+extracted(q-vec2i(0,1)));
            }
            if v(5)!=0.0 || u.hdr==0u {rgb=clamp(rgb,vec3f(0),vec3f(1));}
            return vec4f(rgb,1);
        }
        case 1u: {
            if u.planes.x == -1 {return c;}
            let distance=max(0.0,abs(z-v(1))-v(3)*0.5);
            let falloff=mix(1000.0,1.0,v(4)*0.01);
            let radius=v(2)/u.grid*(1.0-exp(-distance/falloff));
            if radius<0.01 {return c;}
            var sum=c;
            for(var i=1u;i<=32u;i++){
                let angle=f32(i)*2.39996323;
                let delta=radius*sqrt(f32(i)/32.0)*vec2f(cos(angle),sin(angle));
                sum+=fx_bilinear(beauty,u.image,p+delta,false);
            }
            return sum/33.0;
        }
        case 2u: {
            if u.planes.x == -1 {return c;}
            var mask=step(v(1),z);
            if v(2)>0.0 {mask=smoothstep(v(1)-v(2)*0.5,v(1)+v(2)*0.5,z);}
            if v(3)!=0.0 {mask=1.0-mask;}
            return c*mask;
        }
        case 4u: {
            let radius=v(3);
            var mask=id_matte(q);
            if radius>0.0 {
                var sum=mask*4.0;
                for(var i=0u;i<16u;i++) {
                    let a=f32(i)*2.39996323;
                    sum+=id_matte(q+vec2i(round(vec2f(cos(a),sin(a))*radius*sqrt((f32(i)+0.5)/16.0))));
                }
                mask=sum/20.0;
            }
            return c*select(mask,1.0-mask,v(4)!=0.0);
        }
        case 5u: {return crypto(q,c);}
        case 6u: {
            let ids=u.planes;
            var out=select(vec4f(scalar(ids.x,q),scalar(ids.y,q),scalar(ids.z,q),scalar(ids.w,q)),c,ids==vec4i(-1));
            if v(7)!=0.0 && out.a!=0.0 {out=vec4f(out.rgb/out.a,out.a);}
            if v(5)!=0.0 || v(6)!=1.0 {
                out=vec4f(level(out.r,v(5),v(6)),level(out.g,v(5),v(6)),level(out.b,v(5),v(6)),out.a);
            }
            return out;
        }
        case 7u: {
            let id=numeric_id(u.planes.x,q);
            let mask=select(0.0,1.0,id==u.selected && u.planes.x != -1);
            switch u32(v(3)) {
                case 2u: {return vec4f(vec3f(mask),1);}
                case 3u: {return c*mask;}
                case 4u: {return vec4f(vec3f(scalar(u.planes.x,q)),1);}
                default: {return vec4f(color_id(id),1);}
            }
        }
        default: {
            if u.planes.x == -1 {return c;}
            var density=clamp(level(z,v(2),v(3)),0.0,1.0);
            density=pow(density,exp2(2.0-v(5)*0.04));
            if u.has_ramp!=0u {
                let rgb=fx_bilinear(ramp,u.ramp,p,true).rgb;
                density*=mix(1.0,dot(rgb,vec3f(0.2126,0.7152,0.0722)),v(8)*0.01);
            }
            let fog=clamp(density*v(4)*0.01,0.0,1.0);
            let a=mix(c.a,select(c.a,1.0,v(6)!=0.0),fog);
            return vec4f(c.rgb*(1.0-fog)+u.values[1].rgb*(a-c.a*(1.0-fog)),a);
        }
    }
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u){
    let q=u.roi.xy+vec2i(id.xy);
    if all(q<u.roi.zw){textureStore(dst,vec2i(id.xy),evaluate(vec2f(q)+0.5));}
}
