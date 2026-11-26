fn bae_unpremultiply(c: vec4f) -> vec4f { return fx_unpremult(c); }

fn soft_alpha(p: vec2f) -> f32 {
    return fx_bilinear(map_a, u.images[1], p, false).a;
}

fn shade(p: vec2f) -> vec4f {
    let shape = fx_sample(input_tex, u.images[0], p, false).a;
    if u.stage == 5u { return vec4f(0,0,0,shape); }
    let base = fx_sample(map_b, u.images[2], p, false);
    let geometry = u.values[202];
    let light = u.values[203];
    var color = u.values[200];
    var mode = u32(light.w);
    var coverage = soft_alpha(p - geometry.xy);
    switch u.stage {
        case 0u, 2u: {
            // Behind the source, including the uncovered part of antialiased edges.
            let a = saturate(coverage * geometry.z) * color.a;
            return base + vec4f(color.rgb * a, a) * (1.0 - base.a);
        }
        case 1u: { coverage = 1.0 - coverage; }
        case 3u: { coverage = select(1.0 - coverage, coverage, geometry.w != 0.0); }
        case 4u: {
            let gradient = vec2f(soft_alpha(p+vec2f(1,0))-soft_alpha(p-vec2f(1,0)),
                                 soft_alpha(p+vec2f(0,1))-soft_alpha(p-vec2f(0,1)));
            var slope = dot(gradient,light.xy);
            if light.z != 0.0 { slope *= 1.0 - 2.0 * coverage; }
            if slope < 0.0 { color = u.values[201]; mode = u32(value(204)); }
            coverage = abs(slope);
        }
        default: {}
    }
    let a = saturate(coverage * geometry.z) * color.a * shape;
    let rgb = fx_unpremult(base).rgb;
    let blended = generated_blend(vec4f(rgb,1.0),color.rgb,1.0,mode).rgb;
    return vec4f(base.rgb + (blended-rgb)*a, base.a);
}
