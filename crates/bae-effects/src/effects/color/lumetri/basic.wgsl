struct Basic {
    image:FxImage, origin:vec2i, floating:u32, flags:u32, bounds:vec4i,
    balance:vec3f, sat:f32, light:vec4f,
    specular_gain:f32,
    reserved:array<vec4f,9>,
    curves:array<vec4f,48>,
}
@group(0) @binding(0) var<uniform> u:Basic;
@group(0) @binding(1) var src:texture_2d<f32>;
@group(0) @binding(2) var dst:texture_storage_2d<WORKING_FORMAT,write>;
fn response(x:f32,curve:u32)->f32 {
    if x<=0. || x>=1. {return x;}
    let pos=x*63.;
    let i=min(62u,u32(pos))+64u*curve;
    return mix(u.curves[i/4u][i%4u],u.curves[(i+1u)/4u][(i+1u)%4u],fract(pos));
}
fn tone(value:f32)->f32 {
    var x=value;
    if u.light.x>1. && x>=0. {
        x=select(1.-pow(max(0.,1.-x),u.light.x),x,x>1.);
    } else {
        x*=u.light.x;
    }
    var gamma=sign(x)*pow(abs(x),1./2.4);
    gamma=response(response(response(gamma,0u),1u),2u);
    x=sign(gamma)*pow(abs(gamma),2.4);
    return x;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
    if any(id.xy>=textureDimensions(dst)) {return;}
    let c=fx_sample(src,u.image,vec2f(u.origin)+vec2f(id.xy)+.5,true);
    let scaled=linear(c.rgb)*u.light.y/u.light.w;
    let rgb=scaled+u.light.z*(1.-scaled);
    let low=least(rgb);
    let high=greatest(rgb);
    let a=tone(low);
    let b=tone(high);
    var mapped=vec3f(a)+(rgb-low)*((b-a)/max(1e-12,high-low));
    let peak=greatest(mapped);
    if u.specular_gain!=1. && peak>1. {
        mapped*=(1.+(peak-1.)*u.specular_gain)/peak;
    }
    mapped*=u.light.w*linear(u.balance);
    textureStore(dst,id.xy,fx_output(encoded(saturation(mapped,u.sat)),c.a,u.floating!=0u));
}
