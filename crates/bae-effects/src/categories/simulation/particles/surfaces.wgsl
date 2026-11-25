fn ball_property(mode:u32, color:vec3f, xy:vec2f)->f32{
    switch mode {
        case 1u:{return color.r;} case 2u:{return color.g;} case 3u:{return color.b;}
        case 5u:{return max(color.r,max(color.g,color.b))-min(color.r,min(color.g,color.b));}
        case 6u:{return length(color)/sqrt(3.);}
        case 7u:{return length(xy);} case 8u:{return abs(xy.x)+abs(xy.y);}
        case 9u:{return sqrt(max(0.,1.-dot(xy,xy)));}
        default:{return lum(color);}
    }
}
fn ball(id:u32)->Sprite {
    let pitch=max(1.,f(6)+1.);
    let columns=u32(ceil(cfg.size.x/pitch));
    let cell=vec2f(f32(id%columns),f32(id/columns));
    let root=min((cell+.5)*pitch,cfg.size-.5);
    var c=straight(source_at(root));
    if f(9)>1.{
        let radius=pitch*select(.2,.35,f(9)>2.);
        c=straight((source_at(root)*4.+source_at(root+vec2f(radius,0.))+source_at(root-vec2f(radius,0.))
            +source_at(root+vec2f(0.,radius))+source_at(root-vec2f(0.,radius)))/8.);
    }
    let uv=(root/cfg.size-.5)*2.;
    var p=vec3f(root-cfg.size*.5,f(11)*ball_property(u32(f(12)),c.rgb,uv));
    let twist=u32(f(4));
    var w=uv.x;
    switch twist{
        case 2u:{w=uv.y;} case 3u:{w=abs(uv.x);} case 4u:{w=abs(uv.y);}
        case 5u:{w=length(uv);} case 6u:{w=random(id);}
        case 7u,8u,9u:{w=c[twist-7u];} case 10u:{w=lum(c.rgb);}
        case 11u:{w=abs(uv.x)+abs(uv.y);} case 12u:{w=max(abs(uv.x),abs(uv.y));}
        case 13u:{w=pow(1.-root.y/cfg.size.y,2.);} default:{}
    }
    p=vec3f(rot(p.xy,radians(f(5)*w)),p.z);
    let axes=array<vec3f,9>(vec3f(1,0,0),vec3f(0,1,0),vec3f(0,0,1),vec3f(1,1,0),vec3f(1,0,1),vec3f(0,1,1),vec3f(1),vec3f(1,0,.067),vec3f(1,1,.067));
    let axis=normalize(axes[clamp(u32(f(2)),1u,9u)-1u]);let a=radians(f(3));
    p=p*cos(a)+cross(axis,p)*sin(a)+axis*dot(axis,p)*(1.-cos(a));
    let scatter=(noise3(hash(id))-.5)*2.*f(1);
    p+=vec3f(rot(scatter.xy,radians(f(8))),scatter.z);
    let depth=cfg.size.x/max(cfg.size.x*.15,cfg.size.x-p.z);
    return sprite(cfg.size*.5+p.xy*depth,vec2f(.5*pitch*f(7)*.01*depth),0.,SPHERE,c);
}
fn hair_map(p:vec2f)->f32{
    let c=straight(map_at(map0,0u,p/cfg.size));
    switch u32(f(9)){
        case 1u:{return c.r;}case 2u:{return c.g;}case 3u:{return c.b;}case 4u:{return c.a;}
        case 6u:{return (max(c.r,max(c.g,c.b))+min(c.r,min(c.g,c.b)))*.5;}
        case 7u:{return fract(atan2(sqrt(3.)*(c.g-c.b),2.*c.r-c.g-c.b)/TAU+1.);}
        case 8u:{return 1.-min(c.r,min(c.g,c.b))/max(.00001,max(c.r,max(c.g,c.b)));}
        default:{return lum(c.rgb);}
    }
}
fn hair(id:u32)->Sprite{
    let strand=id/4u;
    let randoms=noise3(hash(strand));
    let root=randoms.xy*cfg.size;
    let c=straight(source_at(root));
    let step=max(.5,f(10)*.25);
    let gradient=vec2f(hair_map(root+vec2f(step,0.))-hair_map(root-vec2f(step,0.)),hair_map(root+vec2f(0.,step))-hair_map(root-vec2f(0.,step)));
    let angle=(randoms.z-.5)*TAU*f(11);
    let tangent=normalize(vec2f(sin(angle),-.7)-gradient*f(7)*.03);
    let length_px=f(1)*(.6+.4*randoms.z);
    let gravity=vec2f(0.,f(3)*select(.6+.6*randoms.z,1.,f(4)>0.));
    let t=f32(id%4u)*.25;
    let end=t+.25;
    let a=root+length_px*(tangent*t+gravity*t*t);
    let b=root+length_px*(tangent*end+gravity*end*end);
    let v=b-a;
    let light=dir(f(20));
    let diffuse=abs(dot(normalize(v+.0001),light))*cos(radians(f(19)));
    let spec=pow(clamp(diffuse,0.,1.),1./max(.001,f(26)));
    let color=mix(rgb(14),c.rgb,f(15))*(f(23)+f(24)*(.45+diffuse))*.01+f(25)*spec*.01;
    return sprite((a+b)*.5,vec2f(length(v)*.5+.25,max(.15,f(2)*(1.-t*.75))),atan2(v.y,v.x),LINE,vec4f(color,c.a*f(16)));
}
fn mercury(b:Birth,t:f32)->Sprite{
    let age=t-b.time;
    let phase=clamp(age/max(.0001,b.life),0.,1.);
    let randoms=noise3(hash(b.id))*2.-1.;
    var v=randoms.xy;
    let axis=dir(f(4));
    let extra=f(10);
    switch u32(f(11)){
        case 2u:{v+=sin(randoms.yx*6.+age*2.)*extra*.35;}
        case 3u,4u,5u:{v=rot(v,age*extra*(1.+.5*randoms.z)*TAU);}
        case 6u:{v=vec2f(v.x*.3,-abs(v.y)-.3);}
        case 8u,9u,10u,11u:{v=axis+v*extra*.15;if f(11)>=10. && randoms.z<0.{v=-v;}if f(11)==9. || f(11)==11.{v=normalize(v+.0001);}}
        case 13u,14u:{v=axis*(1.+randoms.y*.4)+vec2f(-axis.y,axis.x)*randoms.x*(.1+extra*.05);if f(11)==14.{v=v.yx;}}
        default:{}
    }
    let drag=max(0.,f(9));
    let travel=select(age,(1.-exp(-drag*age))/max(.0001,drag),drag>.0001);
    let center=pt(3)+randoms.xy*vec2f(f(1),f(2))+v*f(5)*cfg.size.x*.22*travel+vec2f(0.,.14*cfg.size.x*f(8)*age*travel);
    var envelope=1.;
    switch u32(f(13)){case 1u:{envelope=1.-phase;}case 2u:{envelope=phase;}case 3u:{envelope=sin(3.14159*phase);}case 4u:{envelope=pow(1.-phase,2.);}default:{}}
    let r=mix(f(14),f(15),phase)*cfg.size.x*.06;
    return sprite(center,vec2f(max(0.,r)),0.,MERCURY_FIELD,vec4f(1.,1.,1.,select(0.,max(.05,envelope)*f(12),age>=0. && age<b.life)));
}
