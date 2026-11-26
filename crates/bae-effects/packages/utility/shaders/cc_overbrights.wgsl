fn cc_overbrights(c:vec4f,px:BaePixel)->vec4f {
    let rgb=bae_input_straight(c).rgb;
    let hot=rgb>vec3f(1.0);
    var flagged=any(hot);
    switch params.channel {
        case 2u,6u: { flagged=(max(rgb.r,max(rgb.g,rgb.b))+min(rgb.r,min(rgb.g,rgb.b)))*0.5>1.0; }
        case 3u,7u: { flagged=dot(rgb,vec3f(0.299,0.587,0.114))>1.0; }
        default: {}
    }
    if params.channel<=3u {
        return vec4f(select(vec3f(0),vec3f(1),hot)*f32(flagged)*c.a,c.a);
    }
    return select(c,vec4f(params.clip_color.rgb*c.a,c.a),flagged);
}
