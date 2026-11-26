fn lens_flare(input_color: vec4f, px: BaePixel) -> vec4f {
    let size = vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let pos = vec2f(px.pos)+0.5;
    let delta = (pos-params.flare_center)/max(size.y,1.0);
    let d = length(delta);
    let strength = params.flare_brightness*0.01*(1.0-params.blend_with_original*0.01);
    let lens = f32(params.lens_type);
    var light = vec3f(1,0.72,0.46)*exp(-d*(18.0+lens*3.0)) + vec3f(exp(-d*150.0)*2.0);
    let axis = vec2f(frame.layer_bounds.min)+size*0.5-params.flare_center;
    for (var i=1u;i<=7u;i++) {
        let t = f32(i);
        let offset = (pos-params.flare_center-axis*t*(0.23+0.025*lens))/max(size.y,1.0);
        let blades = 5.0+lens;
        let sector = 6.2831853/blades;
        let angle = atan2(offset.y,offset.x);
        let ghost = length(offset)*cos((fract(angle/sector+0.5)-0.5)*sector);
        let radius = 0.014+0.065*fract(t*0.618+lens*0.2);
        let ring = exp(-abs(vec3f(ghost)-radius*vec3f(0.96,1,1.045))*290.0);
        let disk = 1.0-smoothstep(radius*0.7,radius,ghost);
        light += (0.55+0.45*cos(t+lens+vec3f(0,2,4)))*(disk*0.13+ring*0.11);
    }
    let src = bae_premultiply(bae_input_straight(input_color));
    let emission = light*strength;
    return vec4f(src.rgb+emission,src.a+(1.0-src.a)*saturate(max(emission.r,max(emission.g,emission.b))));
}
