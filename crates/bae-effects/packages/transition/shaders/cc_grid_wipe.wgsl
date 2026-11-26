fn cc_grid_wipe(input_color:vec4f,px:BaePixel)->vec4f{
    let src=bae_premultiply(bae_input_straight(input_color));
    let t=params.completion;
    if t<=0.0 {return src;} if t>=1.0 {return vec4f(0);}
    let size=vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let rel=vec2f(px.pos)+0.5-params.center;
    let a=radians(params.rotation);
    let axis=vec2f(cos(a),sin(a));
    let pitch=max(1.0,size.x/params.tiles);
    let cell=abs(fract(vec2f(dot(rel,axis),dot(rel,vec2f(-axis.y,axis.x)))/pitch+0.5)-0.5)*2.0;
    var metric=max(cell.x,cell.y);
    if params.shape==1u {metric=cell.x;}
    if params.shape==2u {metric=length(cell)*0.707107;}
    metric=pow(saturate(metric),params.border/75.0);
    if params.reverse_transition!=0u {metric=1.0-metric;}
    return src*smoothstep(t-1.0/pitch,t+1.0/pitch,metric);
}
