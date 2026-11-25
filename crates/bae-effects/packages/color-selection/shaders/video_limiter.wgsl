fn video_limiter(c:vec4f,px:BaePixel)->vec4f {
    let rgb=bae_unpremultiply(c).rgb;
    let levels=array(1.,1.03,1.05,1.09);
    let knee=array(0.,0.03,0.05,0.10,0.20);
    let limit=levels[params.clip_level-1u];let width=knee[params.compression_before_clipping-1u];
    let peak=max(rgb.r,max(rgb.g,rgb.b));let outside=peak>limit || min(rgb.r,min(rgb.g,rgb.b))<0.;
    if params.gamut_warning!=0u && outside {return vec4f(params.gamut_warning_color.rgb*c.a,c.a);}
    var ceiling=min(peak,limit);
    if width>0. && peak>limit-width {ceiling=limit-width*exp(-(peak-limit+width)/width);}
    let safe=limit_signal(max(rgb,vec3f(0)),peak,ceiling);
    return vec4f(safe*c.a,c.a);
}
