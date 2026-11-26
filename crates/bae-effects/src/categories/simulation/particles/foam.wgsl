fn foam(b:Birth,t:f32)->Sprite{
    let age=max(0.,t-b.time);
    let phase=age/max(.001,b.life);
    let q=noise3(hash(b.id))*2.-1.;
    let scale=max(.001,f(46));let w=cfg.size.x*scale/max(.1,f(48));
    let root=pt(4)+rot(q.xy*vec2f(f(6),f(8))*cfg.size*select(1.,scale,f(12)>0.),radians(f(10)));
    let viscosity=1.+f(42);
    let steps=select(1u,select(4u,8u,f(70)>2.),f(70)>1.);
    var probe=root;
    var flow=vec2f(0.);
    if f(64)>0.{
        for(var i=0u;i<steps;i++){
            let uv=select(probe/cfg.size,(probe-pt(4))/w+.5,f(68)==1.);
            let texel=1./max(vec2f(1.),frame.maps[2].zw);
            let slope=vec2f(
                lum(map_at(map2,2u,uv+vec2f(texel.x,0.)).rgb)-lum(map_at(map2,2u,uv-vec2f(texel.x,0.)).rgb),
                lum(map_at(map2,2u,uv+vec2f(0.,texel.y)).rgb)-lum(map_at(map2,2u,uv-vec2f(0.,texel.y)).rgb));
            let direction=-slope*f(66);
            flow+=direction/f32(steps);
            probe+=(direction+dir(f(32))*f(30))*age*w*.035/f32(steps);
        }
    }
    let wind=dir(f(32))*f(30)+flow;
    let velocity=dir(f(28)+q.z*20.)*f(26);
    let drift=wind*age + velocity*(1.-exp(-viscosity*age))/viscosity;
    let wobble=vec2f(sin(age*2.+q.z*8.),cos(age*1.3+q.x*7.))*min(age,1.)*(f(34)*.01+f(36)*.04);
    var pos=root+w*(drift*.035+wobble+q.xy*f(38)*min(age,2.)*.007/(1.+f(44)));
    if f(40)>0.{pos=mix(pos,cfg.size*(1.-abs(fract(pos/cfg.size*.5)*2.-1.)),clamp(f(40),0.,1.));}
    let r=w*.022*f(16)*max(.05,1.+f(18)*q.z)*(1.-exp(-age/max(.01,f(22))));
    let view=u32(f(2));
    var color=vec3f(.8,.93,1.);
    if f(52)>3.{color=.55+.4*cos(vec3f(.1,2.,4.)+f(52)*.71);}
    if f(58)>0.{color=mix(color,map_at(map1,1u,pos/cfg.size).rgb,clamp(f(60)*(1.-f(62)*.5),0.,1.));}
    let orientation=select(0.,atan2(wind.y+velocity.y,wind.x+velocity.x)+select(0.,q.z,f(56)==2.),f(56)>1.);
    var s=sprite(pos,vec2f(r),orientation,select(BUBBLE,DISC,view!=3u),vec4f(color,select(0.,min(1.,f(24)*.1),phase<1.)));
    if f(50)>1. && view==3u{s.brush.z=SPHERE;}
    if view!=3u{s.color=vec4f(.25,.65,1.,s.color.a*.65);}
    if f(52)==1. && f(54)>0. && view==3u{s.y=vec4f(s.y.xy,.5,.5);s.brush=vec4f(.5,.5,TILE,2.);}
    return s;
}
