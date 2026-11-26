fn evaluate(p: vec2f) -> vec4f {
    let original = fx_sample(input_tex,u.images[0],p,false);
    let a = u.values[1].xy;
    let line = u.values[2].xy-a;
    let half_width = 0.5*value(4u)*u.scale;
    if half_width<=0.0 || dot(line,line)<0.000001 {return original;}
    let t = clamp(dot(p-a,line)/dot(line,line),0.0,1.0);
    let center = a+t*line;
    let r = distance(p,center);
    let feather = max(0.5,(1.0-value(5u))*half_width);
    let coverage = 1.0-smoothstep(max(0.0,half_width-feather),half_width+0.5,r);
    if coverage==0.0 {return original;}
    let mode = u32(value(3u));
    var replacement = vec4f(0.0);
    if mode==2u {
        if value(7u)==0.0 {return original;}
        if has(0u) { replacement = fx_bilinear(map_a,u.images[1],p,false); }
    } else if mode>=3u {
        let normal = normalize(vec2f(-line.y,line.x));
        let axis = select(normal,vec2f(1.0,0.0),mode==4u);
        let span = half_width+1.0;
        let side = select(-1.0,1.0,dot(p-center,axis)>=0.0);
        let near = fx_bilinear(input_tex,u.images[0],p+axis*span*side,false);
        let far = fx_bilinear(input_tex,u.images[0],p-axis*span*side,false);
        replacement = mix(near,far,0.5*value(6u));
    }
    return mix(original,replacement,coverage);
}
