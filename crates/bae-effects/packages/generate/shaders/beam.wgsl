fn beam(input_color: vec4f, px: BaePixel) -> vec4f {
    let ab = params.ending_point-params.starting_point;
    let pos = vec2f(px.pos)+0.5-params.starting_point;
    let t = dot(pos,ab)/max(dot(ab,ab),1e-6);
    var head = params.time*(1.0+params.length);
    if params.perspective_3d != 0u {
        head = mix(head,head*head/(1.0+params.length),0.35);
    }
    let start = max(0.0,head-params.length);
    let end = min(1.0,head);
    let u = clamp(t,start,max(end,start));
    let radius = bae_scale(mix(params.starting_thickness,params.ending_thickness,u))*0.5;
    let dist = length(pos-ab*u);
    let coverage = (1.0-smoothstep(radius*(1.0-params.softness),radius+0.5,dist))*f32(end>start && radius>0.0);
    let col = mix(params.inside_color,params.outside_color,smoothstep(0.0,max(radius,0.001),dist));
    let beam_color = bae_premultiply(vec4f(col.rgb,col.a*coverage));
    return beam_color + select(vec4f(0),bae_premultiply(bae_input_straight(input_color))*(1.0-beam_color.a),params.composite_on_original!=0u);
}
