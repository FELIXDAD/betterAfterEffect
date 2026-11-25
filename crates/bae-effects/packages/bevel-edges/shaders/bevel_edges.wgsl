fn bevel_edges(c:vec4f,px:BaePixel)->vec4f{
    let rim=min(px.uv,1.-px.uv);
    let size=vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let width=params.edge_thickness*min(size.x,size.y);
    if width<=0. {return c;}
    let edge=rim*size;let depth=min(edge.x,edge.y);
    let sign=select(vec2f(-1.),vec2f(1.),px.uv>vec2f(.5));
    let normal=select(vec2f(0,sign.y),vec2f(sign.x,0),edge.x<edge.y);
    let a=radians(params.light_angle);let light=vec2f(cos(a),sin(a));
    let amount=dot(normal,light)*(1.-smoothstep(0.,width,depth))*params.light_intensity;
    return bae_depth_clamp(vec4f(c.rgb+c.a*params.light_color.rgb*amount,c.a));
}