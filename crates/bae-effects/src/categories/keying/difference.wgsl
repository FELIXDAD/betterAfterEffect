fn evaluate(p: vec2f) -> vec4f {
    let original = fx_sample(input_tex,u.images[0],p,true);
    let size = vec2f(u.layer.zw-u.layer.xy);
    let plate_size = vec2f(u.images[1].size)/u.images[1].scale;
    let local = p-vec2f(u.layer.xy);
    let q = select(local-(size-plate_size)*0.5, local*plate_size/max(size,vec2f(1)), value(3u)==2);
    let plate = aux_a(q+u.images[1].origin);
    let delta = distance(fx_sample(map_b,u.images[2],p,true).rgb,plate.rgb)/sqrt(3.0);
    let threshold = value(4u)*0.01;
    let softness = value(5u)*0.01;
    var keep = select(0.0,1.0,delta>threshold);
    if softness>0.0 { keep = smoothstep(threshold,threshold+softness,delta); }
    let alpha = original.a*keep;
    if value(1u)==3 {return gray(alpha);}
    return output(original.rgb,alpha);
}
