fn velocity(p:vec2f)->vec2f{
    let field=u.values[10];
    let at=(p-field.xy)/field.z;
    return flow_at(map_a,at,vec2i(textureDimensions(map_a)));
}
fn evaluate(p:vec2f)->vec4f{
    let lo=vec2f(u.layer.xy);let size=max(vec2f(1),vec2f(u.layer.zw-u.layer.xy));
    let uv=(p-lo)/size;let dir=u32(value(2u));let horizontal=dir>=3u;
    var v=velocity(p);
    if value(4u)==1.0{
        // Fit a smooth scan-line motion profile across the readout axis.
        v=vec2f(0);
        for(var i=0;i<7;i++){
            let t=(f32(i)+0.5)/7.0;
            v+=velocity(lo+size*select(vec2f(t,uv.y),vec2f(uv.x,t),horizontal))/7.0;
        }
    }
    let phase=(select(uv.y,uv.x,horizontal)-0.5)*select(1.0,-1.0,dir==2u || dir==4u);
    let rgba=src(p+v*phase*u.values[10].w*value(1u)*0.01);
    return output(rgba.rgb,rgba.a);
}
