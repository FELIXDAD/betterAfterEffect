fn key_hue(c: vec3<f32>) -> f32 {
    let hi = max(c.r, max(c.g, c.b));
    let span = hi - min(c.r, min(c.g, c.b));
    if span == 0.0 { return 0.0; }
    if hi == c.r { return fract((c.g-c.b)/(6.0*span)); }
    if hi == c.g { return ((c.b-c.r)/span+2.0)/6.0; }
    return ((c.r-c.g)/span+4.0)/6.0;
}

fn linear_color_key(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    if params.view == 2u { return c; }
    let rgb = bae_unpremultiply(c).rgb;
    let key = params.key_color.rgb;
    let d = (rgb-key)*(255.0/256.0);
    var distance = 0.5*dot(d,d);
    var scale = 1.5;
    if params.match_colors == 2u {
        let hue = 2.0*abs(fract(key_hue(rgb)-key_hue(key)+0.5)-0.5)*(255.0/256.0);
        scale = sqrt(3.0)*0.5;
        distance = hue*hue*scale;
    } else if params.match_colors == 3u {
        let uv = d.rb-d.g;
        scale = sqrt(6.0);
        distance = 0.358*uv.x*uv.x-0.183*uv.x*uv.y+0.1995*uv.y*uv.y;
    }
    let tol = params.matching_tolerance / 100.0;
    let kept = saturate((distance-tol*tol*scale)/max(1e-8,params.matching_softness/100.0));
    let a = select(c.a*kept,max(c.a,1.0-kept),params.key_operation==2u);
    if params.view == 3u { return vec4<f32>(vec3<f32>(a),1.0); }
    return vec4<f32>(rgb*a,a);
}
