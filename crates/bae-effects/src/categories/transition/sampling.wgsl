fn original(p:vec2f)->vec4f { return fx_bilinear(input_tex,u.images[0],p,false); }
fn centered(p:vec2f,img:FxImage)->vec2f {
    return p-0.5*vec2f(u.layer.xy+u.layer.zw)+img.origin+0.5*vec2f(img.size)/img.scale;
}
fn reveal(p:vec2f)->vec4f {
    if !has(0u) { return vec4f(0); }
    return fx_bilinear(map_a,u.images[1],centered(p,u.images[1]),false);
}
