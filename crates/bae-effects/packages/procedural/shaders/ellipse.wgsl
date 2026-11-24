fn ellipse(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let radii = max(vec2<f32>(params.width,params.height)*0.5*frame.downsample,vec2<f32>(1e-5));
    let delta = vec2<f32>(px.pos)-params.center;
    // Nearest point on the first quadrant of the ellipse.
    let q = abs(delta);
    var angle = atan2(q.y*radii.x,max(q.x*radii.y,1e-8));
    let k = radii.y*radii.y-radii.x*radii.x;
    for (var i=0;i<3;i++) {
        let cs = vec2<f32>(cos(angle),sin(angle));
        let err = k*cs.x*cs.y+q.x*radii.x*cs.y-q.y*radii.y*cs.x;
        let deriv = k*(cs.x*cs.x-cs.y*cs.y)+dot(q*radii,cs);
        if abs(deriv)>1e-5 { angle=clamp(angle-err/deriv,0.0,1.5707963); }
    }
    let distance = min(length(q-radii*vec2<f32>(cos(angle),sin(angle))),
                       min(length(q-vec2<f32>(radii.x,0)),length(q-vec2<f32>(0,radii.y))));
    let half_width = bae_scale(params.thickness)*0.5;
    let w = saturate(0.5+(half_width-distance)/(1.0+(2.0*half_width-1.0)*params.softness));
    let color = mix(params.outside_color,params.inside_color,saturate(1.0-distance/max(half_width,1e-5)));
    let a = select(0.0,w,params.width>0.0 && params.height>0.0 && params.thickness>0.0);
    let generated = vec4<f32>(color.rgb*a,a);
    return select(generated,generated+(1.0-a)*c,params.composite_on_original!=0u);
}
