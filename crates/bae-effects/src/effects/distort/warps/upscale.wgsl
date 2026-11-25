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
    var wx:array<f32,4>;var wy:array<f32,4>;
    for(var i=0;i<4;i++){wx[i]=cubic_kernel(f.x-f32(i-1));wy[i]=cubic_kernel(f.y-f32(i-1));}
    var a=0.;for(var j=0;j<4;j++){var row=0.;for(var i=0;i<4;i++){
        row+=fx_load(src,u.images[0],p+vec2i(i-1,j-1),false).a*wx[i];
    }a+=row*wy[j];}return a;
}
fn warp(p:vec2f)->vec4f {
    let center=pos(vec2f(0.5));let q=center+(p-center)/max(1.,v(1)*0.01);
    let logical=(q-u.images[0].origin)*u.images[0].scale-0.5;
    let texel=vec2i(floor(logical));let f=fract(logical);
    // Separable Lanczos-3: 12 kernel evaluations per pixel instead of 72.
    var wx:array<f32,6>;var wy:array<f32,6>;var sx=0.;var sy=0.;
    for(var i=0;i<6;i++){
        wx[i]=lanczos(f.x-f32(i-2));wy[i]=lanczos(f.y-f32(i-2));sx+=wx[i];sy+=wy[i];
    }
    var base=vec4f(0);
    for(var j=0;j<6;j++){var row=vec4f(0);for(var i=0;i<6;i++){
        row+=fx_load(src,u.images[0],texel+vec2i(i-2,j-2),false)*wx[i];
    }base+=row*wy[j];}
    base/=max(0.00001,sx*sy);
    let half_px=vec2f(0.5/u.images[0].scale,0);
    let blur=(sample_at(q+half_px)+sample_at(q-half_px)+sample_at(q+half_px.yx)+sample_at(q-half_px.yx))*0.25;
    let weight=1.-100./max(100.,v(1));
    let detail=base-blur;let clean=mix(base,blur,weight*v(2)*0.01);
    let sharp=clean+detail*(weight*v(7)*0.02);
    var alpha=sharp.a;if v(5)==2. {alpha=cubic_alpha(texel,f);}
    return vec4f(sharp.rgb,alpha);
}
