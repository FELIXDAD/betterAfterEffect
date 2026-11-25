fn height_at(p:vec2f)->f32 {
    let c=fx_unpremult(sample_layer(map_a,u.images[1],p,true));
    switch i32(value(3)){case 1:{return c.r;}case 2:{return c.g;}case 3:{return c.b;}case 4:{return c.a;}case 5:{return fx_luma(c.rgb);}default:{return (min(c.r,min(c.g,c.b))+max(c.r,max(c.g,c.b)))*0.5;}}
}
fn shade(p:vec2f)->vec4f {
    let h=max(1.,value(4)*u.scale*0.3);let offset=vec2f(h,0);
    let slope=vec2f(height_at(p+offset)-height_at(p-offset),height_at(p+offset.yx)-height_at(p-offset.yx))*value(5)*u.scale/(2.*h);
    let normal=normalize(vec3f(-slope,1.));let c=sample_source(p+normal.xy*value(6)*u.scale);
    var light=vec3f(rotate_2d(vec2f(0,-1),radians(value(14))),value(12)*0.01);
    if value(11)==2. {light=vec3f(u.values[13].xy-p,layer_size().y*value(12)*0.01);}
    let l=normalize(light+vec3f(0,0,0.00001));let rgb=u.values[10].rgb*value(9)*0.01;
    let diffuse=max(0.,dot(normal,l))*value(18)*0.01;
    let shine=pow(max(0.,dot(normal,normalize(l+vec3f(0,0,1)))),1./max(0.001,value(20)))*value(19)*0.01;
    return vec4f(c.rgb*(value(17)*0.01+diffuse*rgb)+rgb*mix(vec3f(c.a),c.rgb,value(21)*0.01)*shine,c.a);
}
