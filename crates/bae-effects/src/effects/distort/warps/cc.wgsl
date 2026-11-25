fn bending(p:vec2f)->vec4f {
    let a=pt(2);let b=pt(3);
    let axis=b-a;let extent=max(length(axis),1.);let forward=axis/extent;let side=vec2f(-forward.y,forward.x);
    let start=p-a;let y=dot(start,forward);let x=dot(start,side);
    var t=y/extent;let angle=radians(v(1));
    if abs(angle)<0.00001 {return sample_at(p);}
    if t<0. {
        if v(4)==1. {return vec4f(0);}
        if v(4)==2. {return sample_at(p);}
        if v(4)==4. {t=-t;}
    }
    let radius=extent/angle;
    let theta=atan2(y*sign(radius),(radius-x)*sign(radius));
    var q=a+forward*(theta*radius)+side*(radius-sign(radius)*length(vec2f(radius-x,y)));
    if t>1. && v(5)==1. {
        let end=a+forward*(radius*sin(angle))+side*(radius*(1.-cos(angle)));
        q=b+rotate(p-end,-angle);
    }
    return sample_at(q);
}
fn warp(p:vec2f)->vec4f {
    switch u.kind {
        case 6u:{return bending(p);}
        case 7u:{
            let size=max(1.,v(3)*u.scale);let a=radians(v(4));let q=rotate(p-pos(vec2f(0.5)),-a)/size;
            let scale=vec2f(v(1),v(2))*0.01;if any(abs(scale)<vec2f(0.00001)) {return vec4f(0);}
            let offset=(fract(q)-0.5)/scale;
            if v(5)>0.5 && any(abs(offset)>vec2f(0.5)) {return vec4f(0);}
            return sample_at(pos(vec2f(0.5))+rotate((floor(q)+0.5+offset)*size,a));
        }
        default:{
            let scale=v(1);let base=sample_at(p);
            if scale<0.00001 {return base*v(3);}
            let q=fract((p-pt(2))/u.layer.zw/scale+0.5);
            return mix(sample_at(pos(q)),base,v(3));
        }
    }
}
