fn turbulent_noise(c:vec4f,px:BaePixel)->vec4f {
    let phase=params.evolution/180.;
    let seed=u32(params.random_seed)*747796405u + bitcast<u32>(i32(floor(phase)));
    let e=Evolution(seed,seed+1u,smoothstep(0.,1.,fract(phase)));
    return render_fractal(c,px,e,vec2f(0.),0.,false,params.turbulence_factor,true);
}
