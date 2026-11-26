fn evaluate(p:vec2f)->vec4f {
    let size=max(vec2f(1.),vec2f(u.layer.zw-u.layer.xy));
    let zoom=value(46u)/max(.1,value(48u));
    let uv=select((p-vec2f(u.layer.xy))/size,(p-u.values[4].xy)/max(1.,size.x*zoom)+.5,value(68u)==1.);
    let point=u.images[1].origin+uv*vec2f(u.images[1].size)/u.images[1].scale;
    return fx_bilinear(map_a,u.images[1],point,false);
}
