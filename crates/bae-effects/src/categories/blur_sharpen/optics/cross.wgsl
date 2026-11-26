fn evaluate(p:vec2f)->vec4f {
    let a=aux_a(p);let b=aux_b(p);
    if value(1)==0.{return output(b.rgb,b.a);}
    if value(2)==0.{return output(a.rgb,a.a);}
    var rgb=(a.rgb+b.rgb)*0.5;
    switch u32(value(3)){
        case 2u:{rgb=a.rgb+b.rgb;}
        case 3u:{rgb=1.-(1.-a.rgb)*(1.-b.rgb);}
        case 4u:{rgb=a.rgb*b.rgb;}
        case 5u:{rgb=max(a.rgb,b.rgb);}
        case 6u:{rgb=min(a.rgb,b.rgb);}
        default:{}
    }
    return output(rgb,(a.a+b.a)*0.5);
}
