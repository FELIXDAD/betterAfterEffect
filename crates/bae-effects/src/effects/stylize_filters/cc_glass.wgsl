fn height_at(p:vec2f)->f32 {
    let c=fx_unpremult(layer_sample(map1,u.images[1],p,true));
    switch i32(v(3)){case 1:{return c.r;}case 2:{return c.g;}case 3:{return c.b;}case 4:{return c.a;}case 5:{return fx_luma(c.rgb);}default:{return (min(c.r,min(c.g,c.b))+max(c.r,max(c.g,c.b)))*0.5;}}
}
fn effect(p:vec2f)->vec4f {
    let h=max(1.,v(4)*u.scale*0.3);let offset=vec2f(h,0);
    let slope=vec2f(height_at(p+offset)-height_at(p-offset),height_at(p+offset.yx)-height_at(p-offset.yx))*v(5)*u.scale/(2.*h);
    let normal=normalize(vec3f(-slope,1.));let c=source(p+normal.xy*v(6)*u.scale);
    var light=vec3f(angle2(vec2f(0,-1),radians(v(14))),v(12)*0.01);
    if v(11)==2. {light=vec3f(pt(13)-p,u.layer.w*v(12)*0.01);}
    let l=normalize(light+vec3f(0,0,0.00001));let rgb=u.values[10].rgb*v(9)*0.01;
    let diffuse=max(0.,dot(normal,l))*v(18)*0.01;
    let shine=pow(max(0.,dot(normal,normalize(l+vec3f(0,0,1)))),1./max(0.001,v(20)))*v(19)*0.01;
    return vec4f(c.rgb*(v(17)*0.01+diffuse*rgb)+rgb*mix(vec3f(c.a),c.rgb,v(21)*0.01)*shine,c.a);
}
