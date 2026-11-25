fn cc_light_wipe(input_color:vec4f,px:BaePixel)->vec4f{
    let src=bae_premultiply(bae_input_straight(input_color));
    let t=params.completion*0.01;
    if t<=0.0 {return src;}if t>=1.0{return vec4f(0);}
    let size=vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let pos=vec2f(px.pos)+0.5-params.center;
    let ext=max(abs(params.center-vec2f(frame.layer_bounds.min)),abs(vec2f(frame.layer_bounds.max)-params.center));
    let a=radians(params.direction);let axis=vec2f(cos(a),sin(a));
    let xy=abs(vec2f(dot(pos,axis),dot(pos,vec2f(-axis.y,axis.x))));
    let norm=max(vec2f(1),vec2f(dot(ext,abs(axis)),dot(ext,abs(axis.yx))));
    var d=length(pos)/max(length(ext),1.0);
    if params.shape==1u {d=xy.x/norm.x;}
    if params.shape==3u {d=max(xy.x/norm.x,xy.y/norm.y);}
    if params.reverse_transition!=0u{d=1.0-d;}
    let aa=2.0/max(1.0,min(size.x,size.y));let mask=smoothstep(t-aa,t+aa,d);
    let glow=exp(-abs(d-t)*45.0)*params.intensity*0.01*sin(t*3.14159);
    let rgb=select(params.color.rgb,bae_unpremultiply(src).rgb,params.color_from_source!=0u);
    return vec4f((src.rgb+rgb*src.a*glow)*mask,src.a*mask);
}
