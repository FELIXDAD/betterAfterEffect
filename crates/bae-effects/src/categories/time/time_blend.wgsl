fn bae_unpremultiply(c: vec4f) -> vec4f {
    return fx_unpremult(c);
}

fn evaluate(p: vec2f) -> vec4f {
    let input = fx_sample(input_tex, u.images[0], p, false);
    let transfer_id = 1u + u.stage;
    let transfer = u32(value(transfer_id));
    let weight = clamp(value(transfer_id + 1u), 0., 1.);
    let blend = transfer == 1u || transfer == 16u;
    var previous = select(vec4f(0.), input, blend && value(transfer_id + 2u) == 2.);
    if has(0u) {
        previous = fx_sample(map_a, u.images[1], p, false);
    }
    if blend { return mix(input, previous, weight); }
    previous *= weight;
    if transfer == 2u || transfer == 17u { return input + previous * (1. - input.a); }
    if transfer == 3u || transfer == 18u { return previous + input * (1. - previous.a); }

    var mode = 2u;
    switch transfer {
        case 4u: { mode = 6u; }
        case 5u: { mode = 4u; }
        case 6u: { mode = 8u; }
        case 7u: { mode = 9u; }
        case 8u: { mode = 15u; }
        case 9u: { mode = 14u; }
        case 10u: { mode = 5u; }
        case 11u: { mode = 16u; }
        case 12u: { mode = 21u; }
        case 13u: { mode = 7u; }
        case 14u: { mode = 22u; }
        default: {}
    }
    return generated_blend(input, fx_unpremult(previous).rgb, previous.a, mode);
}
