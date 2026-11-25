const TAU:f32=6.283185307;
fn rotation(a:vec3f, order:i32)->mat3x3f {
    let c=cos(a);let s=sin(a);
    let x=mat3x3f(vec3f(1,0,0),vec3f(0,c.x,s.x),vec3f(0,-s.x,c.x));
    let y=mat3x3f(vec3f(c.y,0,-s.y),vec3f(0,1,0),vec3f(s.y,0,c.y));
    let z=mat3x3f(vec3f(c.z,s.z,0),vec3f(-s.z,c.z,0),vec3f(0,0,1));
    switch order {
        case 2:{return y*z*x;}case 3:{return z*x*y;}case 4:{return x*z*y;}
        case 5:{return y*x*z;}case 6:{return x*y*z;}default:{return z*y*x;}
    }
}
fn surface_color(st:vec2f)->vec4f {
    let margin=.5/layer_size();
    return sample_source(layer_position(vec2f(clamp(fract(st.x),margin.x,1.-margin.x),clamp(st.y,margin.y,1.-margin.y))));
}
fn illumination(color:vec4f,n:vec3f,light:u32,shading:u32)->vec4f {
    let z=clamp(value(light+2u)*.01,-1.,1.);let dir=rotate_2d(vec2f(0,-1),radians(value(light+3u)));
    let l=vec3f(dir*sqrt(max(0.,1.-z*z)),z);
    let diffuse=max(0.,dot(n,l));let half_vector=normalize(l+vec3f(0,0,1.00001));
    let specular=pow(max(0.,dot(n,half_vector)),1./max(.0001,value(shading+3u)*value(shading+3u)));
    let light_rgb=u.values[light+1u].rgb*value(light)*.01;
    let tint=mix(vec3f(color.a),color.rgb,value(shading+4u)*.01);
    return vec4f(color.rgb*(value(shading)*.01+diffuse*light_rgb*value(shading+1u)*.01)
      +tint*specular*light_rgb*value(shading+2u)*.01,color.a);
}
