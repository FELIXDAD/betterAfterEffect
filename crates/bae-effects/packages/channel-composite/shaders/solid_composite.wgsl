fn solid_composite(c:vec4f,px:BaePixel)->vec4f{
    let solid=params.color.rgb;let a=params.opacity*.01;
    let background=vec4f(solid*a,a);
    let mode=array<u32,21>(2,2,4,5,6,7,8,9,2,11,12,2,14,15,16,17,2,19,20,21,22);
    return bae_depth_clamp(generated_blend(background,bae_unpremultiply(c).rgb,c.a*params.source_opacity*.01,mode[clamp(params.blending_mode,1u,21u)-1u]));
}