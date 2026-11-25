fn remove_color_matting(c:vec4f,px:BaePixel)->vec4f{
    if c.a<=0. {return vec4f(0);}
    let rgb=bae_unpremultiply(c).rgb-(1.-c.a)*params.background_color.rgb;
    if params.clip!=0u {return vec4f(clamp(rgb,vec3f(0),vec3f(c.a)),c.a);}
    return bae_depth_clamp(vec4f(rgb,c.a));
}