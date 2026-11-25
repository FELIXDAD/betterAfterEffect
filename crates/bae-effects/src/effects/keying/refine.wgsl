fn evaluate(p:vec2f)->vec4f {
    let c=src(p);
    let cleaner=u.stage==0u;
    let soft=u.stage==1u;
    var radius=value(6);
    var contrast=value(16)*0.01;
    var shift=0.;
    if soft {
        radius=value(36)+value(27)+select(0.,value(25),enabled(24));
        contrast=value(30)*0.01;
        shift=value(33)*0.01;
    } else if !cleaner {
        radius=value(24);contrast=value(6)*0.01;shift=value(23)*0.01;
    }
    if !cleaner && enabled(11){radius+=value(2);}
    radius*=u.scale;
    var moments=vec4f(fx_luma(c.rgb),c.a,fx_luma(c.rgb)*c.a,fx_luma(c.rgb)*fx_luma(c.rgb));
    var coefficients=vec2f(0.);
    if u.index==0u {coefficients=aux_b(p).rg;}
    var count=1.;var lo=c.a;var hi=c.a;
    var foreground=c.rgb*c.a*c.a;var total=c.a*c.a;
    if radius>0. {
        for(var ring=1;ring<=2;ring++){
            for(var i=0;i<8;i++){
                let angle=(f32(i)+0.5*f32(ring))*0.785398;
                let sample=src(p+vec2f(cos(angle),sin(angle))*radius*f32(ring)*0.5);
                let w=1.;
                let l=fx_luma(sample.rgb);
                moments+=vec4f(l,sample.a,l*sample.a,l*l);
                count+=1.; lo=min(lo,sample.a);hi=max(hi,sample.a);
                if u.index==0u {coefficients+=aux_b(p+vec2f(cos(angle),sin(angle))*radius*f32(ring)*0.5).rg;}
                let fw=w*sample.a*sample.a;
                foreground+=sample.rgb*fw;total+=fw;
            }
        }
    }
    let edge=hi-lo;
    if u.index==1u {
        let m=moments/count;
        let slope=(m.z-m.x*m.y)/(max(0.,m.w-m.x*m.x)+select(0.001,0.02,soft && !enabled(24)));
        return vec4f(slope,m.y-slope*m.x,0.,1.);
    }
    let fit=coefficients/count;
    var a=saturate(fit.x*fx_luma(c.rgb)+fit.y);
    var temporal=0.;
    if cleaner {temporal=select(0.,0.35,enabled(2));}
    else {
        temporal=select(value(16),select(0.,value(29),value(35)>2.),soft)*0.005;
        if enabled(8) {temporal+=saturate(value(10)/360.)*value(9)/(value(9)+select(8.,4.,enabled(14)));}
    }
    if has(0u){a=mix(a,clamp(aux_a(p).a,lo,hi),saturate(temporal)*edge);}
    a=saturate((a-0.5+shift*edge)/max(1.-contrast,0.01)+0.5);
    if cleaner{return output(c.rgb,mix(c.a,a,value(1)*0.01));}
    if (soft && enabled(26)) || enabled(20){return gray(edge);}
    var rgb=c.rgb;
    if enabled(11) && total>0. {
        rgb=mix(rgb,foreground/total,value(22)*0.01*edge*select(1.-a,1.,enabled(12)));
    }
    return output(rgb,a);
}
