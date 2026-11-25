fn lab_f(t: f32) -> f32 {
    return select(7.787*t+16.0/116.0, pow(max(t,0.0),1.0/3.0),t>0.008856);
}

fn color_range(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let src = bae_input_straight(c);
    let rgb = src.rgb;
    var channels = rgb*255.0;
    if params.color_space == 1u {
        let v = pow(max(rgb,vec3<f32>(0)),vec3<f32>(1.8));
        // Apple RGB primaries and a D65 neutral, with the 1.8 transfer above.
        let x = lab_f(dot(v,vec3<f32>(0.4731,0.3328,0.1941)));
        let y = lab_f(dot(v,vec3<f32>(0.2446,0.6720,0.0834)));
        let z = lab_f(dot(v,vec3<f32>(0.0231,0.1297,0.8472)));
        channels = round(vec3<f32>((116.0*y-16.0)*2.55, 128.0+500.0*(x-y),128.0+200.0*(y-z)));
    } else if params.color_space == 2u {
        let y = dot(rgb,vec3<f32>(0.30,0.59,0.11));
        channels = round(vec3<f32>(y,0.5+(rgb.b-y)*0.39,0.5+(rgb.r-y)*0.685)*255.0);
    }
    let lo = vec3<f32>(params.min_l_y_r,params.min_a_u_g,params.min_b_v_b);
    let hi = vec3<f32>(params.max_l_y_r,params.max_a_u_g,params.max_b_v_b);
    let outside = max(max(lo-channels, channels-hi),vec3<f32>(0));
    let t = saturate(outside/max(params.fuzziness,1e-6));
    let keep = select(2.0*t*t,1.0-2.0*(1.0-t)*(1.0-t),t>vec3<f32>(0.5));
    return bae_premultiply(src) * (1.0-(1.0-keep.x)*(1.0-keep.y)*(1.0-keep.z));
}
