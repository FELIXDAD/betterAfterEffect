fn cc_toner(c:vec4f,px:BaePixel)->vec4f {
    let rgb=bae_unpremultiply(c).rgb;
    let l=dot(rgb,vec3f(0.299,0.587,0.114));var ramp=params.midtones.rgb;
    switch params.tones {
        case 1u:{ramp=mix(params.shadows.rgb,params.highlights.rgb,l);}
        case 2u:{ramp=tone_ramp(l,params.shadows.rgb,params.midtones.rgb,params.highlights.rgb);}
        case 3u:{
            if l<0.5 {ramp=tone_ramp(l*2.,params.shadows.rgb,params.darktones.rgb,params.midtones.rgb);}
            else {ramp=tone_ramp((l-0.5)*2.,params.midtones.rgb,params.brights.rgb,params.highlights.rgb);}
        }
        default:{}
    }
    return vec4f(mix(ramp,rgb,params.blend_w_original)*c.a,c.a);
}
