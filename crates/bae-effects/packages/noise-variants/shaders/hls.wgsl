fn hls_random(cell:vec2i, seed:u32, shape:u32)->f32 {
    let x=2.*bae_hash(seed,cell)-1.;
    if shape==2u {return sign(x)*x*x;}
    return x;
}
fn hls_noise(c:vec4f, px:BaePixel, phase:f32)->vec4f {
    let src=bae_input_straight(c);
    let strength=vec3f(params.hue/360.,params.saturation*0.01,params.lightness*0.01);
    if all(strength==vec3f(0.)) {return bae_premultiply(src);}
    let grain=params.noise==3u;
    let pos=px.layer_pos/select(1.,max(0.25,params.grain_size),grain);
    let cell=vec2i(floor(pos));
    let t=smoothstep(vec2f(0.),vec2f(1.),fract(pos));
    var noise:array<f32,3>;
    for(var j=0u;j<3u;j++){
        let base=bitcast<u32>(i32(floor(phase)))+j*10007u;
        var pair:array<f32,2>;
        for(var k=0u;k<2u;k++){
            let seed=base+k;
            let a=hls_random(cell,seed,params.noise);
            pair[k]=a;
            if grain {
                pair[k]=mix(mix(a,hls_random(cell+vec2i(1,0),seed,3u),t.x),
                            mix(hls_random(cell+vec2i(0,1),seed,3u),hls_random(cell+vec2i(1),seed,3u),t.x),t.y);
            }
        }
        noise[j]=mix(pair[0],pair[1],smoothstep(0.,1.,fract(phase)));
    }
    var hsl=fx_hsl(src.rgb)+vec3f(noise[0],noise[1],noise[2])*strength;
    hsl=vec3f(fract(hsl.x),saturate(hsl.yz));
    return bae_premultiply(vec4f(fx_rgb(hsl),src.a));
}
