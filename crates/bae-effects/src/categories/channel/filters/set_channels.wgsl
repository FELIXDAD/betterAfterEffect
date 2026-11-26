fn pick(tex:texture_2d<f32>,img:FxImage,p:vec2f,own:vec4f,which:u32)->f32 {
    var c=own;
    if img.size.x!=0u {c=fx_unpremult(sample_layer(tex,img,p,value(9)!=0.));}
    let map=array<u32,10>(2,3,4,1,5,6,7,8,9,10);
    return fx_channel(c,map[clamp(which,1u,10u)-1u]);
}
fn shade(p:vec2f)->vec4f {
    let c=fx_unpremult(fx_sample(input_tex, u.images[0], p, false));
    let r=pick(map_a,u.images[1],p,c,u32(value(2)));
    let g=pick(map_b,u.images[2],p,c,u32(value(4)));
    let b=pick(map_c,u.images[3],p,c,u32(value(6)));
    let a=pick(map_d,u.images[4],p,c,u32(value(8)));
    return vec4f(vec3f(r,g,b)*a,a);
}
