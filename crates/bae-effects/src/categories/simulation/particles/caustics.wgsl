fn coordinates(p:vec2f, img:FxImage, zoom:f32,repeat:u32,stretch:bool)->vec2f{
    let size=max(vec2f(1.),vec2f(u.layer.zw-u.layer.xy));
    let tex_size=vec2f(img.size)/img.scale;
    let local=p-vec2f(u.layer.xy)-size*.5;
    var uv=local/select(tex_size,size,stretch)/max(.001,zoom)+.5;
    if repeat==2u {uv=fract(uv);}
    if repeat==3u {uv=1.-abs(fract(uv*.5)*2.-1.);}
    return img.origin+uv*tex_size;
}
fn height_at(p:vec2f)->f32{
    if !has(1u){return 0.;}
    let h=aux_b(coordinates(p,u.images[2],1.,1u,true));
    return dot(h.rgb,vec3f(.2126,.7152,.0722))*h.a;
}
fn bed(p:vec2f)->vec4f{
    if has(0u){return aux_a(coordinates(p,u.images[1],value(4u),u32(value(6u)),value(8u)==2.));}
    return src(coordinates(p,u.images[0],value(4u),u32(value(6u)),value(8u)==2.));
}
fn evaluate(p:vec2f)->vec4f {
    let width=f32(u.layer.z-u.layer.x);
    let step=max(1.,value(28u)*u.scale);
    let x=vec2f(step,0.);
    let center=height_at(p);
    let left=height_at(p-x); let right=height_at(p+x);
    let top=height_at(p-x.yx); let bottom=height_at(p+x.yx);
    let grad=vec2f(right-left,bottom-top)*(value(12u)*width/(2.*step));
    let normal=normalize(vec3f(-grad,1.));
    let bend=grad*width*value(14u)*(1.-1./max(.01,value(16u)));
    let q=p+bend;
    var base=bed(q);
    let blur=value(84u)*u.scale;
    if blur>0.{base=(base*4.+bed(q+vec2f(blur,0.))+bed(q-vec2f(blur,0.))+bed(q+vec2f(0.,blur))+bed(q-vec2f(0.,blur)))/8.;}
    let offset=u.values[34].xy-select(vec2f(u.layer.xy+u.layer.zw)*.5,p,value(38u)==1.);
    let light=normalize(vec3f(offset,max(0.01,value(36u)*width)));
    let diffuse=max(0.,dot(normal,light));
    let highlight=pow(max(0.,dot(normal,normalize(light+vec3f(0.,0.,1.)))),max(1.,value(46u)));
    let focus=(4.*center-left-right-top-bottom)*width*value(12u)/(step*step);
    let lighting=value(40u)+value(30u)*(diffuse*value(42u)+highlight*value(44u));
    var c=mix(base.rgb,u.values[20].rgb,clamp(value(22u),0.,1.))*u.values[32].rgb*lighting*(1.+clamp(focus*value(82u),-.95,8.));
    if has(2u){
        let sky=aux_c(coordinates(p-normal.xy*width*.5,u.images[3],value(72u),u32(value(74u)),value(76u)==2.));
        c=mix(c,sky.rgb,clamp(sky.a*value(80u)*(1.-normal.z*value(78u)),0.,1.));
    }
    return output(c,base.a+(1.-base.a)*value(22u));
}
