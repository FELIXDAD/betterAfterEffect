fn bolt_noise(t: f32, seed: u32) -> f32 {
    var s = 0.0;
    for (var i=0u;i<u32(params.complexity);i++) {
        let freq = exp2(f32(i)+1.0);
        let x = t*freq+params.conductivity_state;
        let k = i32(floor(x));
        let a = bae_hash(seed+17u*i,vec2i(k,1));
        let b = bae_hash(seed+17u*i,vec2i(k+1,1));
        var u = fract(x);
        if params.fractal_type>=2u { u=mix(u,smoothstep(0.0,1.0,u),f32(params.fractal_type-1u)*0.5); }
        s += (mix(a,b,u)-0.5)/freq;
    }
    return s;
}

fn bolt(p: vec2f, start: vec2f, axis: vec2f, seed: u32, branch: bool) -> f32 {
    let len = max(length(axis),1.0);
    let dir = axis/len;
    let t = dot(p-start,dir)/len;
    if t<0.0 || t>1.0 { return 1e5; }
    let taper = sin(3.14159*t);
    let offset = bolt_noise(t,seed)*len*0.36*params.turbulence*taper;
    return abs(dot(p-start,vec2f(-dir.y,dir.x))-offset)
         + t*bae_scale(params.decay+params.core_drain)*select(f32(params.decay_main_core),1.0,branch);
}

fn advanced_lightning(input_color: vec4f, px: BaePixel) -> vec4f {
    let p = vec2f(px.pos)+0.5;
    let src = bae_premultiply(bae_input_straight(input_color));
    var axis = params.context_control-params.origin;
    let seed = frame.seed+521u*params.lightning_type;
    if params.lightning_type==8u { axis.x=0.0; }
    let normal = vec2f(-axis.y,axis.x)/max(length(axis),1.0);
    var d = bolt(p,params.origin,axis,seed,false);
    let branches = u32(clamp(length(axis)*params.forking/max(bae_scale(params.min_forkdistance),1.0)*3.0,0.0,18.0));
    var fork_d = 1e5;
    for (var i=0u;i<branches;i++) {
        let t = (f32(i)+0.5)/f32(branches);
        let root = params.origin+axis*t+normal*bolt_noise(t,seed)*length(axis)*0.36*params.turbulence*sin(3.14159*t);
        let rand = bae_hash(seed,vec2i(i32(i),3))*2.0-1.0;
        let twig = axis*0.22+normal*length(axis)*rand*(0.15+params.fork_variation*0.003);
        fork_d = min(fork_d,bolt(p,root,twig*(params.termination_threshold*0.01),seed+i+1u,true));
    }
    if params.lightning_type==6u || params.lightning_type==7u || params.lightning_type==9u {
        d=min(d,bolt(p,params.origin,-axis,seed+93u,false));
    }
    let obstacle = select(src.a,1.0-src.a,params.alpha_obstacle<0.0)*abs(params.alpha_obstacle)*0.01;
    let core = max(1.0-smoothstep(0.0,max(bae_scale(params.core_radius),0.5),d),
        (1.0-smoothstep(0.0,max(bae_scale(params.core_radius)*0.65,0.5),fork_d))*params.fork_strength*0.01);
    let glow_d = min(d, fork_d/max(params.fork_strength*0.01,0.001));
    let glow = exp(-glow_d*4.0/max(bae_scale(params.glow_radius),0.1))*params.glow_opacity*0.01;
    let a = core*params.core_opacity*0.01*(1.0-obstacle);
    let g = glow*select(1.0-obstacle,1.0,params.main_core_collisions_only!=0u);
    let light = vec4f(params.core_color.rgb*a+params.glow_color.rgb*g*(1.0-a),a+g*(1.0-a));
    return light+select(vec4f(0),src*(1.0-light.a),params.composite_on_original!=0u);
}
