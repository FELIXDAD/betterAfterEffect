// Solve the regularized RGB covariance system using an LDL-transpose factorization.
fn slopes(diag: vec3f, off: vec3f, cov: vec3f, eps: f32) -> vec3f {
    let d0 = max(diag.r, 0.) + eps;
    let l10 = off.x / d0;
    let l20 = off.y / d0;
    let d1 = max(diag.g + eps - l10 * off.x, eps);
    let l21 = (off.z - l20 * off.x) / d1;
    let d2 = max(diag.b + eps - l20 * off.y - l21 * l21 * d1, eps);
    let y1 = cov.y - l10 * cov.x;
    let b = (cov.z - l20 * cov.x - l21 * y1) / d2;
    let g = y1 / d1 - l21 * b;
    return vec3f(cov.x / d0 - l10*g - l20*b, g, b);
}

fn evaluate(p: vec2f) -> vec4f {
    let soft = u.stage == 1u;
    let cleaner = u.stage == 0u;
    if u.index < 4u {
        let c = src(p);
        switch u.index {
            case 0u: { return c; }
            case 1u: { return c*c; }
            case 2u: { return vec4f(c.r*c.g, c.r*c.b, c.g*c.b, 0.); }
            default: { return vec4f(c.rgb*c.a, 0.); }
        }
    }
    // Intermediate channels are packed statistics, independent of alpha.
    if u.index == 4u || u.index == 5u {
        let m = fx_bilinear(input_tex, u.images[0], p, false);
        let variance = fx_bilinear(map_a, u.images[1], p, false) - m*m;
        let ia = fx_bilinear(map_c, u.images[3], p, false).rgb;
        if u.index == 5u {
            return vec4f(ia / max(m.a, 1e-6), saturate(2.*sqrt(max(variance.a, 0.))));
        }
        let off = fx_bilinear(map_b,u.images[2],p,false).rgb - m.rrg*m.gbb;
        let eps = select(0.001, 0.02, soft && !enabled(24u));
        let a = slopes(variance.rgb, off, ia - m.rgb*m.a, eps);
        return vec4f(a, m.a-dot(a,m.rgb));
    }

    let c = src(p);
    var a = c.a;
    var edge = 0.;
    var foreground = c.rgb;
    if u.index == 6u {
        let fit = fx_bilinear(map_b,u.images[2],p,false);
        a = saturate(dot(fit.rgb,c.rgb) + fit.a);
        if has(2u) {
            let context = fx_bilinear(map_c,u.images[3],p,false);
            edge = context.a;
            foreground = context.rgb;
        }
    }
    var contrast = value(16u)*0.01;
    var shift = 0.;
    var temporal = select(0.,0.35,enabled(2u));
    if !cleaner {
        contrast = select(value(6u), value(30u), soft)*0.01;
        shift = select(value(23u),value(33u),soft)*0.01;
        temporal = select(value(16u),select(0.,value(29u),value(35u)>2.),soft)*0.005;
        if enabled(8u) { temporal += saturate(value(10u)/360.)*value(9u)/(value(9u)+select(8.,4.,enabled(14u))); }
    }
    if has(0u) { a = mix(a,clamp(aux_a(p).a,a-edge,a+edge),saturate(temporal)*edge); }
    a = saturate((a-0.5+shift*edge)/max(1.-contrast,0.01)+0.5);
    if cleaner { return output(c.rgb,mix(c.a,a,value(1u)*0.01)); }
    if (soft && enabled(26u)) || enabled(20u) { return gray(edge); }
    var rgb = c.rgb;
    if enabled(11u) {
        rgb = mix(rgb,foreground,value(22u)*0.01*edge*select(1.-a,1.,enabled(12u)));
    }
    return output(rgb,a);
}
