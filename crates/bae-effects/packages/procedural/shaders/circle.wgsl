fn circle(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let r = bae_scale(params.radius);
    let distance = length(vec2<f32>(px.pos)+0.5-params.center);
    var outer = r;
    var inner = 0.0;
    let relative = params.edge >= 4u;
    let thickness = bae_scale(params.context_slider)*select(1.0,abs(params.radius)*0.01,relative);
    if params.edge == 2u {
        outer = max(r,thickness);
        inner = min(r,thickness);
    } else if params.edge >= 3u {
        inner = r-thickness*0.5;
        outer = r+thickness*0.5;
    }
    let feather = vec2<f32>(params.feather_outer_edge,params.feather_inner_edge)
                 * frame.downsample.x * select(1.0,abs(params.radius)*0.01,params.edge==5u);
    let width = 1.5+2.0*feather;
    let out_t = (outer-0.75-distance)/width.x+0.5;
    let in_t = (distance-inner+0.75)/width.y+0.5;
    let coverage = smoothstep(0.0,1.0,out_t) * select(1.0,smoothstep(0.0,1.0,in_t),inner>0.0);
    let alpha = select(coverage,1.0-coverage,params.invert_circle!=0u)*params.opacity*0.01;
    if params.blending_mode == 3u { return c*alpha; }
    return generated_blend(c,params.color.rgb,alpha,select(params.blending_mode,params.blending_mode-1u,params.blending_mode>3u));
}
