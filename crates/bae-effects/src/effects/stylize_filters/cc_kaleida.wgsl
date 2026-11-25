fn effect(p:vec2f)->vec4f {
    let size=max(1.,0.01*v(2)*u.layer.z);let d=angle2(p-pt(1),-radians(v(4)))/size;
    let sides=array(4.,6.,3.,4.,8.,6.,4.,8.,5.);
    let mode=clamp(i32(v(3))-1,0,8);let wedge=3.14159265/sides[mode];
    let a=atan2(d.y,d.x);let wrapped=abs(fract(a/(2.*wedge)+0.5)*2.-1.)*wedge;
    let segment=floor(a/wedge);let r=length(d);
    let radius=select(r,abs(fract(r*0.5)*2.-1.),mode!=1);
    var q=vec2f(cos(wrapped),sin(wrapped))*radius;
    if mode==2||mode==4||mode==7 {q=angle2(q,select(0.,wedge,fract(segment*0.5)>0.));}
    let center=select(pt(1),pos(vec2f(0.5)),v(5)>0.5);
    return source(pos(fract(uv(center+angle2(q*size,radians(v(4)))))));
}
