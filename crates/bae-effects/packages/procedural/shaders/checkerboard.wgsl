// Integral of an alternating +1/-1 stripe, with the cell as the length unit.
fn stripe_integral(t: vec2<f32>) -> vec2<f32> {
    return 1.0-abs(fract(0.5*t)*2.0-1.0);
}

fn checkerboard(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    var size = abs(params.corner - params.anchor);
    if params.size_from != 1u {
        size = vec2<f32>(params.width, select(params.height,params.width,params.size_from==2u))*frame.downsample;
    }
    size = max(size,vec2<f32>(0.0001));
    let pos = (vec2<f32>(px.pos)-params.anchor)/size;
    let span = (vec2<f32>(1)+vec2<f32>(params.feather_width,params.feather_height)*frame.downsample)/size;
    let stripes = (stripe_integral(pos+span*0.5)-stripe_integral(pos-span*0.5))/span;
    let curved = mix(stripes, stripes*(1.5-0.5*stripes*stripes),vec2<f32>(min(params.feather_width,1.0),min(params.feather_height,1.0)));
    let alpha = saturate(0.5 + 0.5*sign(curved.x*curved.y)*min(abs(curved.x),abs(curved.y)))*params.opacity*0.01;
    if params.blending_mode == 3u { return c*alpha; }
    return generated_blend(c,params.color.rgb,alpha, select(params.blending_mode,params.blending_mode-1u,params.blending_mode>3u));
}
