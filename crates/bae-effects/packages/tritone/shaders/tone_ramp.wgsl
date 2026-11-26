fn tone_ramp(l:f32, shadows:vec3f, midtones:vec3f, highlights:vec3f)->vec3f {
    if l<=0.5 {return mix(shadows,midtones,l*2.);}
    return mix(midtones,highlights,(l-0.5)*2.);
}
