struct WarpUniforms {
    images: array<FxImage,2>, rect: vec4i, layer: vec4f,
    kind: u32, hdr: u32, scale: f32, time: f32,
    values: array<vec4f,15>, grid_size: vec4f,
    GRID_FIELD
}
@group(0) @binding(0) var<uniform> u: WarpUniforms;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var aux: texture_2d<f32>;
@group(0) @binding(3) var dst: texture_storage_2d<WORKING_FORMAT,write>;
fn v(i:u32)->f32 { return u.values[i].x; }
fn pt(i:u32)->vec2f { return u.values[i].xy; }
fn uv(p:vec2f)->vec2f { return (p-u.layer.xy)/u.layer.zw; }
fn pos(q:vec2f)->vec2f { return u.layer.xy+q*u.layer.zw; }
fn sample_at(p:vec2f)->vec4f { return fx_bilinear(src,u.images[0],p,false); }
fn rotate(p:vec2f,a:f32)->vec2f { return vec2f(cos(a)*p.x-sin(a)*p.y,sin(a)*p.x+cos(a)*p.y); }
fn cross2(a:vec2f,b:vec2f)->f32 {return a.x*b.y-a.y*b.x;}
fn unit(d:vec2f)->vec2f {return d/max(length(d),0.00001);}
fn inside(q:vec2f)->bool {return all(q>=vec2f(0)) && all(q<=vec2f(1));}
