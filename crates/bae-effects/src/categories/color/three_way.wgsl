fn ramp(x:f32, lo:f32, hi:f32) -> f32 {
    return clamp((x-lo)/max(hi-lo,1e-6),0.,1.);
}

fn interval(x:f32, id:u32) -> f32 {
    let lo=value(id)*0.01;
    let hi=value(id+2)*0.01;
    let left=select(ramp(x,lo-value(id+1)*0.01,lo),1.,x>=lo);
    let right=select(1.-ramp(x,hi,hi+value(id+3)*0.01),1.,x<=hi);
    return select(left*right, max(left,right), id==92u && lo>hi);
}

fn wheel(rgb:vec3f,id:u32) -> vec3f {
    let hsl=fx_hsl(rgb);
    let angle=value(id+2)/360.;
    let saturation=value(id+7)*0.01;
    let balanced=select(fx_rgb(vec3f(hsl.x+angle,hsl.y*saturation,hsl.z)),rgb,angle==0. && saturation==1.);
    let tint=fx_rgb(vec3f(value(id+5)/360.,1.,0.5));
    return balanced+(tint-fx_luma(tint))*value(id+3)*value(id+4)*0.0002;
}

fn evaluate(pt:vec2f) -> vec4f {
    let original=src(pt);
    let rgb=original.rgb;
    let luma=fx_luma(rgb);
    let hsl=fx_hsl(rgb);
    var mask=interval(hsl.x,92u)*interval(hsl.y,100u)*interval(luma,108u);
    if value(119)==2. {mask=1.-mask;}
    if u.stage==1u {return vec4f(vec3f(mask),1.);}
    if has(0u) {mask=aux_a(pt).r;}
    let thin=value(115)*0.01;
    mask=clamp((mask-max(thin,0.))/max(1.-abs(thin),0.001)-min(thin,0.),0.,1.);
    if value(119)==3. {mask=1.-mask;}

    let local=(pt-vec2f(u.layer.xy))/max(vec2f(u.layer.zw-u.layer.xy),vec2f(1.));
    if enabled(2) && select(local.x,local.y,value(3)==1.)<value(4)*0.01 {return output(rgb,original.a);}
    if value(1)==3. {return output(vec3f(mask),original.a);}
    let shadow=1.-ramp(luma,(value(10)-value(11))/255.,(value(10)+value(11))/255.);
    let high=(1.-shadow)*ramp(luma,(value(12)-value(13))/255.,(value(12)+value(13))/255.);
    let mid=1.-shadow-high;
    if value(1)==4. {return output(vec3f(mid*0.5+high),original.a);}

    let neutral=vec3f(0.501961);
    let black=u.values[25].rgb+value(28)/255.;
    let white=u.values[27].rgb+value(30)/255.-1.;
    let gamma=clamp(vec3f(value(29))*(neutral/max(u.values[26].rgb,vec3f(0.001))),vec3f(0.01),vec3f(100.));
    let input_levels=pow(max((rgb-black)/max(white-black,vec3f(0.001)),vec3f(0.)),1./gamma);
    let master=wheel(input_levels,17u);
    let shadows=wheel(master,34u)-u.values[5].rgb;
    let midtones=wheel(master,51u)+neutral-u.values[6].rgb;
    let highlights=wheel(master,68u)+1.-u.values[7].rgb;
    let corrected=(shadows*shadow+midtones*mid+highlights*high)*(value(32)-value(31))/255.+value(31)/255.;
    let mixed=mix(rgb,corrected,mask);
    return output(select(mixed,vec3f(fx_luma(mixed)),value(1)==2.),original.a);
}
