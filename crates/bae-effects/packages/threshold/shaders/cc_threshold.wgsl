fn cc_threshold(c:vec4f,px:BaePixel)->vec4f {
    let rgb=bae_unpremultiply(c).rgb;
    var signal=vec3f(dot(rgb,vec3f(.299,.587,.114)));
    switch params.channel {
        case 2u:{signal=rgb;}
        case 3u:{let high=max(rgb.r,max(rgb.g,rgb.b));signal=vec3f((high-min(rgb.r,min(rgb.g,rgb.b)))/max(high,.00001));}
        case 4u:{signal=vec3f(c.a);}
        default:{}
    }
    var bits=vec3f(threshold_step(signal.r,params.threshold),threshold_step(signal.g,params.threshold),threshold_step(signal.b,params.threshold));
    if params.invert!=0u{bits=1.-bits;}
    let out=select(vec4f(bits*c.a,c.a),vec4f(rgb*bits.x,bits.x),params.channel==4u);
    return mix(out,c,params.blend_w_original);
}