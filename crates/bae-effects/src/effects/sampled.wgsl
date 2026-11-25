struct FxSettings {
    images:array<FxImage,5>, rect:vec4i, layer:vec4f,
    scale:f32, hdr:u32, time:f32, mode:u32, values:array<vec4f,33>,
}
@group(0) @binding(0) var<uniform> u:FxSettings;
@group(0) @binding(1) var src:texture_2d<f32>;
@group(0) @binding(2) var map1:texture_2d<f32>;
@group(0) @binding(3) var map2:texture_2d<f32>;
@group(0) @binding(4) var map3:texture_2d<f32>;
@group(0) @binding(5) var map4:texture_2d<f32>;
@group(0) @binding(6) var dst:texture_storage_2d<WORKING_FORMAT,write>;
fn v(i:u32)->f32{return u.values[i].x;}
fn pt(i:u32)->vec2f{return u.values[i].xy;}
fn uv(p:vec2f)->vec2f{return (p-u.layer.xy)/u.layer.zw;}
fn pos(q:vec2f)->vec2f{return u.layer.xy+q*u.layer.zw;}
fn source(p:vec2f)->vec4f{return fx_bilinear(src,u.images[0],p,false);}
fn angle2(p:vec2f,a:f32)->vec2f{return vec2f(cos(a)*p.x-sin(a)*p.y,sin(a)*p.x+cos(a)*p.y);}
fn layer_sample(tex:texture_2d<f32>,img:FxImage,p:vec2f,stretch:bool)->vec4f{
    if img.size.x==0u{return vec4f(0);}
    var q=p-pos(vec2f(0.5))+img.origin+0.5*vec2f(img.size)/img.scale;
    if stretch{q=img.origin+uv(p)*vec2f(img.size)/img.scale;}
    return fx_bilinear(tex,img,q,false);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) gid:vec3u){
    let p=vec2i(gid.xy);if any(p>=u.rect.zw-u.rect.xy){return;}
    var c=effect(vec2f(p+u.rect.xy)+0.5);
    if u.hdr==0u{let a=clamp(c.a,0.,1.);c=vec4f(clamp(c.rgb,vec3f(0),vec3f(a)),a);}
    textureStore(dst,p,c);
}
