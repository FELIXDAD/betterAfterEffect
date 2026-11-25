struct Sampled {
    images: array<FxImage,5>,
    roi: vec4i,
    layer: vec4i,
    presence: u32,
    stage: u32,
    index: u32,
    float_depth: u32,
    scale: f32,
    time: f32,
    fps: f32,
    pad: f32,
    values: array<vec4f,233>,
}
@group(0) @binding(0) var<uniform> u: Sampled;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var map_a: texture_2d<f32>;
@group(0) @binding(3) var map_b: texture_2d<f32>;
@group(0) @binding(4) var map_c: texture_2d<f32>;
@group(0) @binding(5) var map_d: texture_2d<f32>;
@group(0) @binding(6) var out_tex: texture_storage_2d<WORKING_FORMAT, write>;
fn value(id: u32) -> f32 { return u.values[id].x; }
fn enabled(id: u32) -> bool { return value(id) != 0.0; }
fn has(i:u32) -> bool { return (u.presence & (1u << i)) != 0u; }
fn src(p:vec2f) -> vec4f { return fx_bilinear(input_tex,u.images[0],p,true); }
fn aux_a(p:vec2f) -> vec4f { return fx_bilinear(map_a,u.images[1],p,true); }
fn aux_b(p:vec2f) -> vec4f { return fx_bilinear(map_b,u.images[2],p,true); }
fn aux_c(p:vec2f) -> vec4f { return fx_bilinear(map_c,u.images[3],p,true); }
fn gray(x:f32) -> vec4f { return vec4f(vec3f(x),1.0); }
fn output(rgb:vec3f, alpha:f32) -> vec4f { return fx_output(rgb,alpha,u.float_depth!=0u); }
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
    let xy = u.roi.xy + vec2i(id.xy);
    if all(xy < u.roi.zw) { textureStore(out_tex, vec2i(id.xy), evaluate(vec2f(xy)+0.5)); }
}
