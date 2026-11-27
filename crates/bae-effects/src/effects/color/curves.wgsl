struct Curves {
    origin: vec2<i32>,
    hdr: u32,
    active_channels: u32,
    lut: array<vec4<u32>,640>,
}
@group(0) @binding(0) var<uniform> u: Curves;
@group(0) @binding(1) var dst: texture_storage_2d<WORKING_FORMAT,write>;
@group(0) @binding(2) var src: texture_2d<f32>;

fn curve_sample(ch:u32, i:u32) -> f32 {
    let pair = u.lut[ch*128u+i/8u][(i/2u)%4u];
    return f32((pair >> (16u*(i%2u))) & 65535u)/65535.;
}

fn curve(ch:u32,x:f32) -> f32 {
    if (u.active_channels & (1u << ch)) == 0u { return x; }
    let pos = x*1023.;
    let i = u32(clamp(floor(pos),0.,1022.));
    let t = select(clamp(pos-f32(i),0.,1.),pos-f32(i),u.hdr!=0u);
    return mix(curve_sample(ch,i),curve_sample(ch,i+1u),t);
}

@compute @workgroup_size(8,8)
fn apply_curves(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=textureDimensions(dst)) { return; }
    let c=textureLoad(src,vec2<i32>(id.xy)+u.origin,0);
    let rgb=select(vec3<f32>(0),c.rgb/max(c.a,1e-12),c.a>0.);
    let a=clamp(curve(4u,c.a),0.,1.);
    let result=vec3<f32>(curve(0u,curve(1u,rgb.r)),curve(0u,curve(2u,rgb.g)),curve(0u,curve(3u,rgb.b)));
    textureStore(dst,vec2<i32>(id.xy),vec4<f32>(result*a,a));
}
