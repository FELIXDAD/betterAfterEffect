fn iris_wipe(input_color:vec4f,px:BaePixel)->vec4f{
    let src=bae_premultiply(bae_input_straight(input_color));
    let radius=bae_scale(params.outer_radius);
    if radius<=0.0 {return src;}
    let points=max(3.0,round(params.iris_points));
    let xy=vec2f(px.pos)+0.5-params.iris_center;
    let theta=atan2(xy.y,xy.x)-radians(params.rotation)+1.5707963;
    let half_sector=3.14159265/points;
    let a=(fract(theta/(2.0*half_sector))*2.0)*half_sector;
    let inner=select(radius*cos(half_sector),bae_scale(params.inner_radius),params.use_inner_radius!=0u);
    let folded=min(a,2.0*half_sector-a);
    let edge=vec2f(inner*cos(half_sector)-radius,inner*sin(half_sector));
    let local=length(xy)*vec2f(cos(folded),sin(folded))-vec2f(radius,0);
    let distance=(local.x*edge.y-local.y*edge.x)/max(length(edge),0.001);
    let feather=max(0.5,bae_scale(params.feather)*0.5);
    return src*smoothstep(-feather,feather,distance);
}
