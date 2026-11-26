fn burn_noise(q:vec2f) -> f32 {
    let cell=vec2i(floor(q)); let t=smoothstep(vec2f(0),vec2f(1),fract(q));
    let seed=u32(params.random_seed);
    return mix(mix(bae_hash(seed,cell),bae_hash(seed,cell+vec2i(1,0)),t.x),
               mix(bae_hash(seed,cell+vec2i(0,1)),bae_hash(seed,cell+vec2i(1,1)),t.x),t.y);
}
fn cc_burn_film(c:vec4f,px:BaePixel)->vec4f {
    let progress=params.burn*0.01;
    if progress<=0.0 { return c; }
    if progress>=1.0 { return vec4f(0); }
    let p=vec2f(px.pos)+0.5-params.center;
    let farthest=max(abs(params.center-vec2f(frame.layer_bounds.min)),abs(vec2f(frame.layer_bounds.max)-params.center));
    let q=p/max(1.0,length(farthest));
    let noise=0.65*burn_noise(q*10.0)+0.35*burn_noise(q*36.0);
    let edge=length(q)*(0.75+noise*0.5)-progress*1.25;
    let coverage=smoothstep(-0.004,0.004,edge);
    let ember=1.0-smoothstep(0.0,0.065,abs(edge));
    let straight=bae_input_straight(c);
    return bae_premultiply(vec4f(mix(straight.rgb,vec3f(1.0,0.27,0.025),ember),straight.a*coverage));
}
