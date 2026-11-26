fn cc_color_offset(c:vec4f, px:BaePixel)->vec4f {
    let rgb=bae_unpremultiply(c).rgb;
    let phase=vec3f(params.red_phase,params.green_phase,params.blue_phase)/360.;
    if all(phase==vec3f(0)) {return c;}
    let shifted=rgb+phase;
    var mapped=fract(shifted);
    if params.overflow==2u {mapped=1.-abs(fract(shifted*0.5)*2.-1.);}
    if params.overflow==3u {mapped=select(mapped,1.-mapped,fract(floor(shifted)*0.5)>vec3f(0));}
    return vec4f(select(mapped,rgb,phase==vec3f(0))*c.a,c.a);
}
