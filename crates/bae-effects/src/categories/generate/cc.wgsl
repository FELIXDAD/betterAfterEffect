fn bead(p:vec2f,i:u32)->vec2f {
    if value(8u)==1.0 { return p; }
    let phase=u.time*value(11u)*0.035+f32(i)*1.618;
    return p+vec2f(sin(phase)*value(9u),cos(phase*1.3)*value(10u))*u.scale;
}
fn evaluate(p:vec2f)->vec4f {
    let base=fx_sample(input_tex,u.images[0],p,true);
    let radius=value(2u)*0.5*u.scale;
    if radius<=0.0 || value(3u)<=0.0 { return output(base.rgb,base.a); }
    let count=u32(value(168u));
    let end=bead(u.values[1].xy,0u);
    var nearest=end;
    var d=distance(p,end);
    var previous=end;
    for(var i=0u;i<count;i++) {
        let next=bead(u.values[169u+i].xy,i+1u);
        let axis=next-previous;
        let along=clamp(dot(p-previous,axis)/max(0.0001,dot(axis,axis)),0.0,1.0);
        let segment=mix(previous,next,along);
        let candidate=mix(next,segment,saturate(value(6u)*0.01)*smoothstep(0.0,25.0,value(3u)));
        let distance_to_bead=distance(p,candidate);
        if distance_to_bead<d { d=distance_to_bead; nearest=candidate; }
        previous=next;
    }
    let alpha=saturate(radius-d+0.5);
    if alpha<=0.0 { return output(base.rgb,base.a); }
    let xy=(p-nearest)/max(radius,0.001);
    let n=normalize(vec3f(xy,sqrt(max(0.001,1.0-dot(xy,xy)))));
    let texture_color=src(p+n.xy*radius*2.0).rgb;
    let pigment=mix(vec3f(1),texture_color,value(5u)*0.01);
    let a=radians(value(19u));
    var direction=vec2f(sin(a),-cos(a));
    if value(16u)==2.0 { direction=(u.values[18].xy-p)/max(1.0,length(vec2f(u.layer.zw-u.layer.xy))*0.25); }
    let l=normalize(vec3f(direction,value(17u)*0.01+0.00001));
    let lit=u.values[15].rgb*value(14u)*0.01;
    let shine=pow(max(0.0,reflect(-l,n).z),1.0/max(0.001,value(25u)))*value(24u)*0.01;
    var rgb=pigment*(value(22u)*0.01+lit*max(0.0,dot(l,n))*value(23u)*0.01)+lit*shine*mix(vec3f(1),pigment,value(26u)*0.01);
    if value(28u)==2.0 { rgb=surface_lights(p,n,pigment,value(22u)*0.01,value(23u)*0.01,value(24u)*0.01,value(25u),value(26u)*0.01); }
    let out_alpha=alpha+base.a*(1.0-alpha);
    return output((rgb*alpha+base.rgb*base.a*(1.0-alpha))/max(0.00001,out_alpha),out_alpha);
}
