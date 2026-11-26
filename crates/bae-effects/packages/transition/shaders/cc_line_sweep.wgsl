fn cc_line_sweep(c:vec4f,px:BaePixel)->vec4f {
    let t=params.completion*0.01;
    if t<=0.0 { return c; }
    if t>=1.0 { return vec4f(0); }
    let a=radians(params.direction);
    // AE: 0° is up, turning clockwise (the default 180° sweeps top to bottom).
    let axis=vec2f(sin(a),-cos(a)); let n=vec2f(-axis.y,axis.x);
    let size=vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let p=(px.uv-0.5)*size;
    let width=max(1.0,dot(abs(axis),size)); let span=max(1.0,dot(abs(n),size));
    let thickness=max(1.0,bae_scale(params.thickness));
    let count=ceil(span/thickness);
    let line=clamp(floor((dot(p,n)+0.5*span)/thickness),0.0,count-1.0);
    let delay=(1.0-params.slant/99.0);
    let progress=t*(1.0+(count-1.0)*delay)-line*delay;
    let along=0.5+dot(p,axis)/width*select(1.0,-1.0,params.flip_direction!=0u);
    return c*saturate((along-progress)*width+0.5);
}
