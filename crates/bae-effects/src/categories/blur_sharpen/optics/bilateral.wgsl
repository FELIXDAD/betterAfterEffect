fn evaluate(p:vec2f)->vec4f {
    let c=src(p);
    let r=value(1)*u.scale;
    let sigma=max(value(2)/255.,0.0001);
    let smart=u.stage==1u;
    let chroma=smart || enabled(3);
    if u.index == 1u {
        // Joint bilateral upsample: compare four coarse guidance samples to the
        // full-resolution center so a colour edge cannot leak across the tile.
        let image = u.images[1];
        let xy = (p-image.origin)*image.scale-0.5;
        let base = vec2i(floor(xy)); let f = fract(xy);
        var sum = vec3f(0.); var weight = 0.;
        for (var y=0; y<=1; y++) { for (var x=0; x<=1; x++) {
            let texel = clamp(base + vec2i(x,y), vec2i(0), vec2i(image.size)-1);
            let pos = image.origin + (vec2f(texel)+0.5)/image.scale;
            let d = aux_b(pos).rgb - c.rgb;
            let distance = select(pow(abs(fx_luma(d)),2.), dot(d,d)/3., chroma);
            let w = select(1.-f.x,f.x,x==1)*select(1.-f.y,f.y,y==1)*exp(-0.5*distance/(sigma*sigma));
            sum += fx_load(map_a,image,texel,true).rgb*w; weight += w;
        }}
        if weight > 1e-6 {return output(sum/weight,c.a);}
        return output(c.rgb,c.a);
    }
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
