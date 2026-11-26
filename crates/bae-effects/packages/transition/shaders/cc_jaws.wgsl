fn cc_jaws(c:vec4f,px:BaePixel)->vec4f {
    let t=params.completion;
    if t<=0.0 { return c; }
    if t>=1.0 { return vec4f(0); }
    let angle=radians(params.direction);
    let axis=vec2f(cos(angle),sin(angle)); let n=vec2f(-axis.y,axis.x);
    let size=vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let p=vec2f(px.pos)+0.5-params.center;
    let period=max(1.0,dot(abs(axis),size)/params.width);
    let phase=fract(dot(p,axis)/period);
    var tooth=1.0-4.0*abs(phase-0.5);
    switch params.shape {
        case 2u: { tooth=clamp(tooth*2.0,-1.0,1.0); }
        case 3u: { tooth=select(-1.0,1.0,phase>=0.5); }
        case 4u: { tooth=sin(phase*6.2831853); }
        default: {}
    }
    let half_span=dot(abs(n),size)*0.5;
    let offset=abs(dot(n,params.center-0.5*vec2f(frame.layer_bounds.max+frame.layer_bounds.min)));
    let edge=dot(p,n)-tooth*params.height*half_span;
    return c*saturate(abs(edge)-t*(half_span*(1.0+params.height)+offset)+0.5);
}
