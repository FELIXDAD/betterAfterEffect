fn shade(p:vec2f)->vec4f {
    let size=max(1.,0.01*value(2)*layer_size().x);let d=rotate_2d(p-u.values[1].xy,-radians(value(4)))/size;
    let sides=array(4.,6.,3.,4.,8.,6.,4.,8.,5.);
    let mode=clamp(i32(value(3))-1,0,8);let wedge=3.14159265/sides[mode];
    let a=atan2(d.y,d.x);let wrapped=abs(fract(a/(2.*wedge)+0.5)*2.-1.)*wedge;
    let segment=floor(a/wedge);let r=length(d);
    let radius=select(r,abs(fract(r*0.5)*2.-1.),mode!=1);
    var q=vec2f(cos(wrapped),sin(wrapped))*radius;
    if mode==2||mode==4||mode==7 {q=rotate_2d(q,select(0.,wedge,fract(segment*0.5)>0.));}
    let center=select(u.values[1].xy,layer_position(vec2f(0.5)),value(5)>0.5);
    return sample_source(layer_position(fract(layer_uv(center+rotate_2d(q*size,radians(value(4)))))));
}
