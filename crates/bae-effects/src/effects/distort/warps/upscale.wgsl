fn lanczos(x:f32)->f32 {
    let a=abs(x)*3.14159265;
    if a<0.0001 {return 1.;} if abs(x)>=3. {return 0.;}
    return 3.*sin(a)*sin(a/3.)/(a*a);
}
fn cubic_kernel(x:f32)->f32 {
    let a=abs(x);
    if a<=1. {return 1.+a*a*(1.5*a-2.5);}
    if a<2. {return 2.+a*(-4.+a*(2.5-0.5*a));} return 0.;
}
fn cubic_alpha(p:vec2i,f:vec2f)->f32 {
    var a=0.;for(var j=-1;j<=2;j++){for(var i=-1;i<=2;i++){
        a+=fx_load(src,u.images[0],p+vec2i(i,j),false).a*cubic_kernel(f.x-f32(i))*cubic_kernel(f.y-f32(j));
    }}return a;
}
fn warp(p:vec2f)->vec4f {
    let center=pos(vec2f(0.5));let q=center+(p-center)/max(1.,v(1)*0.01);
    let logical=(q-u.images[0].origin)*u.images[0].scale-0.5;
    let texel=vec2i(floor(logical));let f=fract(logical);
    var base=vec4f(0);var weight_sum=0.;
    for(var j=-2;j<=3;j++){for(var i=-2;i<=3;i++){
        let w=lanczos(f.x-f32(i))*lanczos(f.y-f32(j));
        base+=fx_load(src,u.images[0],texel+vec2i(i,j),false)*w;
        weight_sum+=w;
    }}
    base/=max(0.00001,weight_sum);
    let half_px=vec2f(0.5/u.images[0].scale,0);
    let blur=(sample_at(q+half_px)+sample_at(q-half_px)+sample_at(q+half_px.yx)+sample_at(q-half_px.yx))*0.25;
    let weight=1.-100./max(100.,v(1));
    let detail=base-blur;let clean=mix(base,blur,weight*v(2)*0.01);
    let sharp=clean+detail*(weight*v(7)*0.02);
    return vec4f(sharp.rgb,select(sharp.a,cubic_alpha(texel,f),v(5)==2.));
}
