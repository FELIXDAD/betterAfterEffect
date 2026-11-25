// Convert colour and evaluate the depth map once per source texel.
fn light(c: vec4f) -> vec3f {
    var rgb = c.rgb;
    if enabled(22) { rgb = sign(rgb) * pow(abs(rgb), vec3f(2.2)); }
    let luma = fx_luma(rgb);
    let gain = max(0., luma - value(18)) * value(17) * 0.05;
    return (rgb + gain * mix(vec3f(1.), rgb / max(luma, 0.001), value(19) * 0.01)) * c.a;
}
fn finish(c: vec4f) -> vec4f {
    var rgb = c.rgb / max(c.a, 1e-6);
    if enabled(22) { rgb = sign(rgb) * pow(abs(rgb), vec3f(1./2.2)); }
    return output(rgb, c.a);
}
fn prepare(p: vec2f) -> vec4f {
    let c = src(p);
    if !has(0u) { return vec4f(light(c), c.a); }
    let layer_size = vec2f(u.layer.zw - u.layer.xy);
    let size = vec2f(u.images[1].size) / u.images[1].scale;
    let xy = p - vec2f(u.layer.xy);
    let uv = select(xy - 0.5 * (layer_size - size), xy * size / max(layer_size, vec2f(1.)), value(12) == 2.);
    let map = aux_a(u.images[1].origin + uv);
    var depth = fx_luma(map.rgb);
    if value(11) > 1. { depth = map[u32(value(11)) - 2u]; }
    if enabled(14) { depth = 1. - depth; }
    return vec4f(light(c), abs(saturate(depth) - value(13)));
}
fn aperture_offset(v: vec2f) -> vec2f {
    let a = radians(value(6));
    let point = v * vec2f(value(5), 1.) * (value(1) * u.scale);
    return vec2f(cos(a)*point.x - sin(a)*point.y, sin(a)*point.x + cos(a)*point.y);
}
fn border(p: vec2f) -> vec2f {
    if enabled(21) { return clamp(p, vec2f(u.layer.xy) + 0.5, vec2f(u.layer.zw) - 0.5); }
    return p;
}
fn packed(p: vec2f) -> vec4f { return fx_bilinear(input_tex, u.images[0], border(p), false); }
fn coverage(p: vec2f) -> f32 { return fx_bilinear(map_a, u.images[1], border(p), false).a; }
fn gather(p: vec2f, mapped: bool) -> vec4f {
    let radius = value(1) * u.scale;
    let sector = 6.2831853 / (value(3) + 2.);
    let center = packed(p);
    var sum = vec4f(center.rgb, select(center.a, coverage(p), mapped));
    var norm = 1.;
    let count = i32(clamp(ceil(radius*2.), 16., 80.));
    for (var i=0; i<count; i++) {
        let angle = f32(i) * 2.399963;
        let radial = sqrt((f32(i)+0.5)/f32(count));
        let edge = cos(0.5*sector) / cos((fract(angle/sector+0.5)-0.5)*sector);
        let q = p + aperture_offset(vec2f(cos(angle),sin(angle)) * radial * mix(edge, 1., value(4)*0.01));
        let tap = packed(q);
        let coc = select(radius, radius*tap.a, mapped);
        if mapped && coc == 0. { continue; }
        var w = saturate(coc-radius*radial+0.5) * radius*radius/max(1.,coc*coc);
        w *= 1. + 0.25*radial*radial*value(7)*0.01;
        sum += vec4f(tap.rgb, select(tap.a,coverage(q),mapped))*w;
        norm += w;
    }
    // An in-focus cell receiving no scattered light can use the exact source pixel
    // during reconstruction. Coverage in this intermediate is otherwise nonnegative.
    if mapped && u.index == 1u && norm == 1. { return vec4f(0., 0., 0., -1.); }
    return finish(sum/norm);
}
fn axis(id: u32) -> vec2f {
    let angle = f32(id) * 2.0943951;
    return aperture_offset(vec2f(cos(angle), sin(angle)));
}
fn rhombi(p: vec2f, first: bool) -> vec4f {
    let along = axis(u.index);
    let b = axis(1u);
    let c = axis(2u);
    let count = i32(clamp(ceil(value(1)*u.scale), 2., 32.));
    var sum = vec4f(0.);
    var norm = 0.;
    for (var i=0; i<count; i++) {
        let t = (f32(i)+0.5)/f32(count);
        let w = 1. + 0.125*t*t*value(7)*0.01;
        if first {
            sum += packed(p + along*t)*w;
        } else {
            sum += w * (fx_bilinear(map_a,u.images[1],p+b*t,false)
                + fx_bilinear(map_a,u.images[1],p+c*t,false)
                + fx_bilinear(map_b,u.images[2],p+c*t,false))/3.;
        }
        norm += w;
    }
    return sum/norm;
}
fn depth_resolve(p: vec2f) -> vec4f {
    let sharp = finish(vec4f(fx_bilinear(map_b, u.images[2], border(p), false).rgb, src(border(p)).a));
    let image = u.images[1];
    let xy = (p - image.origin) * image.scale - 0.5;
    let base = vec2i(floor(xy));
    let f = fract(xy);
    var sum = vec4f(0.);
    for (var y = 0; y < 2; y++) { for (var x = 0; x < 2; x++) {
        let tap = fx_load(map_a, image, base + vec2i(x,y), false);
        let w = select(1.-f.x, f.x, x==1) * select(1.-f.y, f.y, y==1);
        sum += w * select(tap, sharp, tap.a == -1.);
    }}
    return sum;
}
fn evaluate(p: vec2f) -> vec4f {
    switch u.stage {
        case 0u: { return prepare(p); }
        case 1u, 2u: { return gather(p,u.stage==1u); }
        case 3u: { return rhombi(p,true); }
        case 4u: { return finish(rhombi(p,false)); }
        default: {
            if has(1u) { return depth_resolve(p); }
            return fx_bilinear(map_a, u.images[1], p, false);
        }
    }
}
