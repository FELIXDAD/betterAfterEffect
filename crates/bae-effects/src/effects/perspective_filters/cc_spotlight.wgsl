fn effect(p:vec2f)->vec4f{
    let c=source(p);let light=vec3f(pt(1),max(.001,u.layer.w*v(3)*.01));
    let axis=normalize(vec3f(pt(2),0)-light);
    let ray=normalize(vec3f(p,0)-light);
    let angle=acos(clamp(dot(ray,axis),-1.,1.));let outer=max(.0001,radians(v(4))*.5);
    let coverage=(1.-smoothstep(outer*(1.-max(.0001,v(5))),outer,angle))*v(7)*.01;
    var rgb=u.values[6].rgb;let mode=i32(v(8));
    if mode>=6 {let gel=layer_sample(map1,u.images[1],p,true);rgb*=fx_unpremult(gel).rgb;}
    switch mode{
        case 1,6:{return vec4f(rgb*coverage,clamp(coverage,0.,1.));}
        case 2,7:{return vec4f(c.rgb+rgb*coverage*c.a,c.a);}
        case 3,8:{return vec4f(c.rgb+rgb*coverage,max(c.a,min(1.,coverage)));}
        default:{return vec4f(c.rgb*rgb*coverage,c.a);}
    }
}