fn neutral_offset(pick:vec3f,sliders:vec3f)->vec3f {
    return dot(pick,vec3f(0.299,0.587,0.114))-pick+sliders*0.01;
}
fn neutralize(rgb:vec3f)->vec3f {
    let l=dot(rgb,vec3f(0.299,0.587,0.114));
    let t=saturate((l-params.black_point*0.01)/((params.white_point-params.black_point)*0.01));
    let dark=neutral_offset(params.shadows_unbalance.rgb,vec3f(params.red_shadows,params.green_shadows,params.blue_shadows));
    let mid=neutral_offset(params.midtones_unbalance.rgb,vec3f(params.red_midtones,params.green_midtones,params.blue_midtones));
    let light=neutral_offset(params.highlights_unbalance.rgb,vec3f(params.red_highlights,params.green_highlights,params.blue_highlights));
    var shift=mix(dark,mid,smoothstep(0.0,0.5,t));
    if t>0.5 { shift=mix(mid,light,smoothstep(0.5,1.0,t)); }
    let pin=params.pinning*0.005;
    let strength=select(1.0,smoothstep(0.0,max(0.0001,pin),min(t,1.0-t)),pin>0.0);
    return rgb+shift*strength*(1.0-params.blend_w_original*0.01);
}
fn cc_color_neutralizer(c:vec4f,px:BaePixel)->vec4f {
    if params.view==1u { return bae_premultiply(vec4f(neutralize(bae_input_straight(c).rgb),c.a)); }
    var x=px.uv.x;
    if params.view==3u { x=floor(x*16.0)/15.0; }
    let corrected=neutralize(vec3f(x));
    if px.uv.y<0.1 { return vec4f(vec3f(x),1.0); }
    if px.uv.y<0.5 { return vec4f(corrected,1.0); }
    let y=(1.0-px.uv.y)*2.0;
    let dy=2.0/max(1.0,f32(frame.layer_bounds.max.y-frame.layer_bounds.min.y));
    return vec4f(select(vec3f(0.08),vec3f(1),abs(corrected-y)<vec3f(1.5*dy)),1.0);
}
