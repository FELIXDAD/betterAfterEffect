fn bezier(a:vec2f,b:vec2f,c:vec2f,d:vec2f,t:f32)->vec2f {
    let s=1.-t;
    return s*s*s*a+3.*s*s*t*b+3.*s*t*t*c+t*t*t*d;
}
fn surface(q:vec2f)->vec2f {
    let top=bezier(pt(1),pt(2),pt(3),pt(4),q.x);
    let bottom=bezier(pt(10),pt(9),pt(8),pt(7),q.x);
    let left=bezier(pt(1),pt(12),pt(11),pt(10),q.y);
    let right=bezier(pt(4),pt(5),pt(6),pt(7),q.y);
    return mix(top,bottom,q.y)+mix(left,right,q.x)-mix(mix(pt(1),pt(4),q.x),mix(pt(10),pt(7),q.x),q.y);
}
fn inverse_surface(p:vec2f)->vec2f {
    var q=uv(p);
    for(var i=0;i<12;i++) {
        let a=surface(q);let dx=surface(q+vec2f(0.001,0))-a;let dy=surface(q+vec2f(0,0.001))-a;
        let err=p-a;let det=cross2(dx,dy);
        if dot(err,err)<0.00001 || abs(det)<1e-10 {break;}
        q+=0.001*vec2f(cross2(err,dy),cross2(dx,err))/det;
        if i>=i32(v(14))+1 {break;}
    }
    return q;
}
fn warp_style(p:vec2f)->vec4f {
    var q=uv(p)*2.-1.;
    q/=max(vec2f(0.05),1.+0.008*vec2f(v(4)*q.y,v(5)*q.x));
    let vertical=v(2)>1.5;
    if vertical {q=q.yx;}
    let bend=v(3)*0.01;
    let arch=1.-q.x*q.x;
    switch i32(v(1)) {
        case 1:{let r=1.+0.5*bend*q.y;q.x/=max(0.25,r);q.y+=bend*arch;}
        case 2:{q.y=(q.y-bend*arch)/(1.+abs(bend)*arch);}
        case 3:{q.y=(q.y+bend*arch)/(1.+abs(bend)*arch);}
        case 4:{q.y+=bend*arch;}
        case 5:{q.y/=max(0.05,1.+bend*arch);}
        case 6:{q.y-=bend*arch*(1.+q.y)*0.5;q.x*=1.-0.35*bend*q.y;}
        case 7:{q.y+=bend*arch*(1.-q.y)*0.5;q.x*=1.+0.35*bend*q.y;}
        case 8:{q.y+=bend*sin(q.x*3.14159);}
        case 9:{q.y+=bend*cos(q.x*4.71239);}
        case 10:{q.y/=max(0.1,1.+bend*(arch-q.x));}
        case 11:{q.y+=bend*q.x;}
        case 12:{q*=1.-0.7*bend*max(0.,1.-length(q));}
        case 13:{q*=1.-0.45*bend*max(0.,1.-q.x*q.x)*max(0.,1.-q.y*q.y);}
        case 14:{q=rotate(q,-bend*max(0.,1.-length(q))*2.);}
        default:{q.x/=max(0.1,1.-0.75*bend*(1.-q.y*q.y));}
    }
    if vertical {q=q.yx;}
    return sample_at(pos(0.5*q+0.5));
}
fn magnify(p:vec2f)->vec4f {
    let d=p-pt(1);let mag=v(3)*0.01;
    let dist=select(length(d),max(abs(d.x),abs(d.y)),v(10)>1.5);
    let r=v(2)*u.scale*select(1.,mag,v(11)>1.5);
    let feather=v(4)*u.scale*select(1.,mag,v(11)>2.5);
    let weight=(1.-smoothstep(max(0.,r-feather),r+0.001,dist))*v(5)*0.01;
    var q=pt(1)+d/mag;
    let c=sample_at(p);
    if v(8)==3. {q+=(fract(sin(dot(floor(p),vec2f(12.9898,78.233)))*4375.5453)-0.5)*min(mag-1.,4.);}
    var lens=sample_at(q);
    if v(8)==2. {let h=vec2f(0.5,0)*min(mag-1.,2.);lens=0.25*(sample_at(q+h)+sample_at(q-h)+sample_at(q+h.yx)+sample_at(q-h.yx));}
    if v(12)==2. {return mix(c,lens,weight);}
    return generated_blend(c,fx_unpremult(lens).rgb,lens.a*weight,u32(v(12)));
}
fn page(p:vec2f)->vec4f {
    let control=i32(v(8));var direction=rotate(vec2f(1,0),radians(v(2)));
    if control>=3 {
        let corner=vec2f(f32((control-3)%2),f32((control-3)/2));
        direction=unit(pos(corner)-pt(1));
    }
    let along=dot(p-pt(1),direction);let radius=max(1.,v(3)*u.scale);
    if along>radius {return vec4f(0);}
    if along<0. {return select(sample_at(p),vec4f(0),v(7)==2.);}
    let theta=asin(clamp(along/radius,0.,1.));
    let front=sample_at(p+direction*(radius*theta-along));
    let back_pos=p+direction*(radius*(3.14159-theta)-along);
    let paper=u.values[6].rgb;
    var back=fx_bilinear(aux,u.images[1],back_pos,false);
    back=vec4f(mix(paper*back.a,back.rgb,v(9)*0.01),back.a);
    let normal=vec3f(direction*sin(theta),cos(theta));
    let light=vec3f(rotate(vec2f(0.65,0),radians(v(4))),0.76);
    let shade=0.45+0.55*abs(dot(normal,light));
    let f=select(front,vec4f(0),v(7)==2.);
    let b=select(vec4f(back.rgb*shade,back.a),vec4f(0),v(7)==3.);
    return b+f*(1.-b.a);
}
fn warp(p:vec2f)->vec4f {
    switch u.kind {
        case 1u:{return sample_at(pos(inverse_surface(p)));}
        case 2u:{return magnify(p);}
        case 3u:{return warp_style(p);}
        case 4u:{return page(p);}
        default:{
            let d=p-pt(1);let size=u.layer.z*v(2)*0.01;let r=length(d)/max(size,0.001);
            if r>=1. || size<=0. {return vec4f(0);}
            let power=exp2(0.02*v(3));return sample_at(pt(1)+d*pow(max(r,0.00001),power-1.));
        }
    }
}
