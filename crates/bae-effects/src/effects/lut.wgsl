struct Settings {
    image: FxImage, origin: vec2i, size: vec2u, floating: u32, intensity: f32,
    @align(16) color: lut_Params,
}
@group(0) @binding(0) var<uniform> u: Settings;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT,write>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size) {return;}
    let c = fx_sample(src,u.image,vec2f(u.origin)+vec2f(id.xy)+0.5,true);
    textureStore(dst,id.xy,fx_output(mix(c.rgb, APPLY_LUT, u.intensity),c.a,u.floating!=0u));
}
