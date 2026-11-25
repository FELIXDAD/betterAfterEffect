@compute @workgroup_size(8,8,1)
fn main(@builtin(global_invocation_id) gid:vec3u) {
    let q=vec2i(gid.xy);
    if any(q>=u.rect.zw-u.rect.xy) {return;}
    let p=vec2f(q+u.rect.xy)+0.5;
    var color=warp(p);
    if u.hdr==0u {let a=clamp(color.a,0.,1.);color=vec4f(clamp(color.rgb,vec3f(0),vec3f(a)),a);}
    textureStore(dst,q,color);
}
