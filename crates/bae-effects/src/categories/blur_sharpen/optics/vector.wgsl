fn field(p:vec2f)->f32 {
    let bounds=vec2f(u.layer.zw-u.layer.xy);
    let uv=clamp((p-vec2f(u.layer.xy))/max(bounds,vec2f(1.)),vec2f(0.),vec2f(1.));
    let c=aux_a(u.images[1].origin+uv*vec2f(u.images[1].size)/u.images[1].scale);
    switch u32(value(6)){
        case 1u,2u,3u,4u:{return c[u32(value(6))-1u];}
        case 5u:{return fx_luma(c.rgb);}
        case 6u:{return fx_hsl(c.rgb).z;}
        case 7u:{return fx_hsl(c.rgb).x;}
        default:{return fx_hsl(c.rgb).y;}
    }
}
fn evaluate(p:vec2f)->vec4f {
    let c=src(p);let delta=max(1.,value(4)*u.scale);
    var axis=vec2f(field(p+vec2f(delta,0.))-field(p-vec2f(delta,0.)),field(p+vec2f(0.,delta))-field(p-vec2f(0.,delta)))/(2.*delta);
    let mode=u32(value(1));let magnitude=length(axis);
    if mode==2u {axis/=max(1e-5,magnitude);}
    if mode==3u {axis=vec2f(-axis.y,axis.x);}
    if mode>=4u {let a=6.2831853*field(p);axis=vec2f(cos(a),sin(a));}
    let a=radians(value(3));axis=vec2f(cos(a)*axis.x-sin(a)*axis.y,sin(a)*axis.x+cos(a)*axis.y);
    let span=axis*value(2)*u.scale;
    let n=i32(clamp(ceil(length(span)),1.,64.));
    var sum=vec4f(0.);var norm=0.;
    for(var i=0;i<n;i++){
        let t=(f32(i)+0.5)/f32(n);
        let q=src(p+span*(t-select(0.5,0.,mode==5u)));
        let w=select(1.,1.-t,mode==5u);sum+=vec4f(q.rgb*q.a,q.a)*w;norm+=w;
    }
    return sum/norm;
}
