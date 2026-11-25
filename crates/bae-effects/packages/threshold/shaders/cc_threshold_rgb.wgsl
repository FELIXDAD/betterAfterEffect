fn cc_threshold_rgb(c:vec4f,px:BaePixel)->vec4f{
    let s=bae_unpremultiply(c).rgb;
    let bits=vec3f(threshold_step(s.r,params.red_threshold),threshold_step(s.g,params.green_threshold),threshold_step(s.b,params.blue_threshold));
    let inv=vec3u(params.invert_red_channel,params.invert_green_channel,params.invert_blue_channel)!=vec3u(0);
    return mix(vec4f(select(bits,1.-bits,inv)*c.a,c.a),c,params.blend_w_original);
}