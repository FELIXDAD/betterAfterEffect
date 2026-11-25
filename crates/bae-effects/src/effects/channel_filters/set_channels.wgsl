fn pick(tex:texture_2d<f32>,img:FxImage,p:vec2f,own:vec4f,which:u32)->f32 {
    var c=own;
    if img.size.x!=0u {c=fx_unpremult(layer_sample(tex,img,p,v(9)!=0.));}
    let map=array<u32,10>(2,3,4,1,5,6,7,8,9,10);
    return fx_channel(c,map[clamp(which,1u,10u)-1u]);
}
fn effect(p:vec2f)->vec4f {
    let c=fx_unpremult(source(p));
    let r=pick(map1,u.images[1],p,c,u32(v(2)));
    let g=pick(map2,u.images[2],p,c,u32(v(4)));
    let b=pick(map3,u.images[3],p,c,u32(v(6)));
    let a=pick(map4,u.images[4],p,c,u32(v(8)));
    return vec4f(vec3f(r,g,b)*a,a);
}