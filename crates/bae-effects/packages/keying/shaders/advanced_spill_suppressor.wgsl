fn advanced_spill_suppressor(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let src = bae_input_straight(c);
    let rgb = src.rgb;
    var corrected = rgb;
    if params.method == 1u {
        corrected.g = min(rgb.g, 0.5*(rgb.r+rgb.b));
    } else {
        let key = params.key_color.rgb;
        let hi = max(key.r,max(key.g,key.b));
        let lo = min(key.r,min(key.g,key.b));
        let axis = (key-lo)/max(hi-lo,1e-6);
        let peak = dot(rgb,axis)/max(dot(axis,axis),1e-6);
        let complement = vec3<f32>(1.0)-axis;
        let other = dot(rgb,complement)/max(dot(complement,complement),1e-6);
        let excess = max(0.0,peak-other);
        let cutoff = (1.0-params.tolerance*0.01)*(1.0-params.range*0.01);
        let spill = max(0.0,excess-cutoff) * params.desaturation*0.02;
        corrected += spill*(complement-axis*params.spill_color_correction*0.008);
        corrected += dot(rgb-corrected,vec3<f32>(0.299,0.587,0.114))*params.luma_correction*0.01;

    }
    return vec4<f32>(mix(rgb,corrected,params.suppression/100.0)*src.a,src.a);
}
