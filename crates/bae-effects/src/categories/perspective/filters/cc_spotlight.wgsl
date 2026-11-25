fn shade(p:vec2f)->vec4f{
    let c=fx_sample(input_tex, u.images[0], p, false);let light=vec3f(u.values[1].xy,max(.001,layer_size().y*value(3)*.01));
    let axis=normalize(vec3f(u.values[2].xy,0)-light);
    let ray=normalize(vec3f(p,0)-light);
    let angle=acos(clamp(dot(ray,axis),-1.,1.));let outer=max(.0001,radians(value(4))*.5);
    let coverage=(1.-smoothstep(outer*(1.-max(.0001,value(5))),outer,angle))*value(7)*.01;
    var rgb=u.values[6].rgb;let mode=i32(value(8));
    if mode>=6 {let gel=sample_layer(map_a,u.images[1],p,true);rgb*=fx_unpremult(gel).rgb;}
    switch mode{
        case 1,6:{return vec4f(rgb*coverage,clamp(coverage,0.,1.));}
        case 2,7:{return vec4f(c.rgb+rgb*coverage*c.a,c.a);}
        case 3,8:{return vec4f(c.rgb+rgb*coverage,max(c.a,min(1.,coverage)));}
        default:{return vec4f(c.rgb*rgb*coverage,c.a);}
    }
}
