struct Controls {
    image: FxImage, ramp: FxImage, roi: vec4i, planes: vec4i,
    method: u32, grid: f32, hdr: u32, has_ramp: u32,
    values: array<vec4f,9>,
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
