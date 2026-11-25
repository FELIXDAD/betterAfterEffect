fn evaluate(p:vec2f)->vec4f {
    let c=src(p);
    let r=value(1)*u.scale;
    let sigma=max(value(2)/255.,0.0001);
    let smart=u.stage==1u;
    let chroma=smart || enabled(3);
    var sum=c.rgb;var norm=1.;
    let taps=i32(clamp(ceil(r),1.,5.));
    for(var y=-taps;y<=taps;y++){for(var x=-taps;x<=taps;x++){
        if x==0 && y==0{continue;}
        let uv=vec2f(f32(x),f32(y))/f32(taps);
        let q=clamp(p+uv*r,vec2f(u.layer.xy)+0.5,vec2f(u.layer.zw)-0.5);
        let other=src(q);let d=other.rgb-c.rgb;
        let distance=select(pow(abs(fx_luma(d)),2.),dot(d,d)/3.,chroma);
        let w=exp(-2.*dot(uv,uv)-0.5*distance/(sigma*sigma))*other.a;
        sum+=w*other.rgb;norm+=w;
    }}
    let rgb=sum/norm;
    if smart && value(3)>1.{
        let gx=src(p+vec2f(1.,0.)).rgb-src(p-vec2f(1.,0.)).rgb;
        let gy=src(p+vec2f(0.,1.)).rgb-src(p-vec2f(0.,1.)).rgb;
        let edge=smoothstep(sigma,sigma*2.,max(length(gx),length(gy)));
        return output(select(vec3f(edge),mix(rgb,vec3f(1.),edge),value(3)==3.),c.a);
    }
    return output(rgb,c.a);
}
