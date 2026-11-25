fn noise_alpha(c:vec4f, px:BaePixel)->vec4f {
    let src=bae_input_straight(c);
    if params.amount==0. {return bae_premultiply(src);}
    var phase=params.phase/360.;
    if params.noise>=4u {phase+=frame.time;}
    let cycle=max(1,i32(round(params.turns)));
    var seed=i32(floor(phase));var next=seed+1;
    if params.cycle!=0u {seed=(seed%cycle+cycle)%cycle;next=(seed+1)%cycle;}
    let cell=vec2i(floor(px.layer_pos));
    var n=mix(bae_hash(bitcast<u32>(seed),cell),bae_hash(bitcast<u32>(next),cell),smoothstep(0.,1.,fract(phase)))*2.-1.;
    if params.noise==2u || params.noise==5u {n=sign(n)*n*n;}
    let delta=params.amount*0.01*n;
    var a=src.a+delta;
    switch params.original_alpha {
        case 2u: {a=clamp(a,0.,src.a);}
        case 3u: {a=src.a*(1.+delta);}
        case 4u: {a=src.a+4.*src.a*(1.-src.a)*delta;}
        default: {}
    }
    if a<0. || a>1. {
        if params.overflow==3u {a=fract(a);}
        else if params.overflow==2u {a=1.-abs(2.*fract(0.5*a)-1.);}
    }
    return bae_premultiply(vec4f(src.rgb,saturate(a)));
}
