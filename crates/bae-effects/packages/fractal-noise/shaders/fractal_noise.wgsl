// Evolution samples a continuous sequence of independent value lattices.
// Wrapping both plane indices gives the loop a continuous seam, including
// for negative revolutions. Two planes per turn keep a one-turn loop moving.
struct Evolution {
    first: u32,
    second: u32,
    weight: f32,
}

fn evolution() -> Evolution {
    let period = max(2, i32(round(params.cycle_in_revolutions * 2.0)));
    let turns = params.evolution / 360.0;
    let phase = select(turns * 2.0, fract(turns / params.cycle_in_revolutions) * f32(period),
                       params.cycle_evolution != 0u);
    var plane = i32(floor(phase));
    var next = plane + 1;
    if params.cycle_evolution != 0u {
        plane = ((plane % period) + period) % period;
        next = (plane + 1) % period;
    }
    let seed = u32(params.random_seed) * 747796405u;
    return Evolution(seed + bitcast<u32>(plane), seed + bitcast<u32>(next), smoothstep(0.0, 1.0, fract(phase)));
}

fn lattice(cell: vec2<i32>, e: Evolution) -> f32 {
    let a = bae_hash(e.first, cell) + bae_hash(e.first ^ 0xa341316cu, cell) - 1.0;
    let b = bae_hash(e.second, cell) + bae_hash(e.second ^ 0xa341316cu, cell) - 1.0;
    // Maintain the measured lattice contrast between evolution planes.
    let norm = inverseSqrt(e.weight * e.weight + (1.0-e.weight)*(1.0-e.weight));
    return saturate(0.5 + 0.59 * mix(a,b,e.weight) * norm);
}

fn cubic(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    return b + 0.5 * t * ((c - a) + t * (2.0*a - 5.0*b + 4.0*c - d + t*(3.0*(b - c) + d - a)));
}

fn value_noise(q: vec2<f32>, footprint: vec2<f32>, e: Evolution) -> f32 {
    if params.noise_type == 1u {
        let cell = vec2<i32>(floor(q));
        let offset = fract(q) - 0.5;
        let neighbor = vec2<i32>(sign(offset));
        let weight = saturate(0.5 - (0.5 - abs(offset)) / max(footprint, vec2<f32>(1e-6)));
        let a = mix(lattice(cell,e), lattice(cell+vec2<i32>(neighbor.x,0),e), weight.x);
        let b = mix(lattice(cell+vec2<i32>(0,neighbor.y),e), lattice(cell+neighbor,e), weight.x);
        return mix(a,b,weight.y);
    }
    let uv = q - 0.5;
    let cell = vec2<i32>(floor(uv));
    var w = fract(uv);
    if params.noise_type == 4u {
        var rows: array<f32, 4>;
        for (var i = 0; i < 4; i++) {
            let row = cell + vec2<i32>(0, i - 1);
            rows[i] = cubic(lattice(row - vec2<i32>(1,0), e), lattice(row, e),
                lattice(row + vec2<i32>(1,0), e), lattice(row + vec2<i32>(2,0), e), w.x);
        }
        return cubic(rows[0], rows[1], rows[2], rows[3], w.y);
    }
    if params.noise_type == 3u { w = w * w * (3.0 - 2.0 * w); }
    return mix(mix(lattice(cell, e), lattice(cell + vec2<i32>(1,0), e), w.x),
               mix(lattice(cell + vec2<i32>(0,1), e), lattice(cell + vec2<i32>(1,1), e), w.x), w.y);
}

fn rotate_point(p: vec2<f32>, degrees: f32) -> vec2<f32> {
    let a = radians(-degrees);
    return vec2<f32>(cos(a)*p.x - sin(a)*p.y, sin(a)*p.x + cos(a)*p.y);
}

fn fractal_noise(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let e = evolution();
    let size = (2.0/3.0) * select(vec2<f32>(params.scale_width, params.scale_height), vec2<f32>(params.scale), params.uniform_scaling != 0u);
    let origin = bae_to_layer(params.offset_turbulence);
    let sub_offset = bae_to_layer(params.sub_offset) / size;
    var q = rotate_point(px.layer_pos - origin, params.rotation) / size;
    // Block probes at scale 100 and 5 have the same subcell origin across seeds.
    q += vec2<f32>(0.663, 0.768);
    var footprint = 1.0 / (size * frame.downsample);
    var weight = 1.0;
    var sum = 0.0;
    var total = 0.0;
    var peak = 0.0;
    for (var i = 0; f32(i) < params.complexity && i < 20; i++) {
        let n = value_noise(q, footprint, e);
        let ridge = abs(2.0*n - 1.0);
        var v = n;
        switch params.fractal_type {
            case 3u: { v = ridge * ridge; }
            case 4u: { v = ridge; }
            case 5u: { v = sqrt(ridge); }
            case 7u, 8u, 9u: { q += vec2<f32>(n - 0.5, 0.5 - n) * (0.5 * f32(params.fractal_type - 6u)); }
            default: {}
        }
        let w = weight * min(1.0, params.complexity - f32(i));
        sum += v*w;
        total += w;
        peak = max(peak, v * min(w, 1.0));
        // Rescale the accumulator before increasing amplitudes overflow.
        if weight > 1e15 { total *= 1e-15; sum *= 1e-15; weight *= 1e-15; }
        weight *= params.sub_influence * 0.01;
        footprint *= 100.0 / params.sub_scaling;
        q = rotate_point(q - sub_offset, params.sub_rotation) * (100.0 / params.sub_scaling);
        if params.center_subscale == 0u { q += vec2<f32>(19.0, 37.0); }
        if params.perspective_offset != 0u { q += origin / size * (100.0 / params.sub_scaling - 1.0); }
    }
    var v = select(sum / total, peak, params.fractal_type == 11u);
    v = select(v, 1.0-v, params.invert != 0u);
    v = (v - 0.5) * params.contrast * 0.01 + 0.5 + params.brightness * 0.01;
    switch params.overflow {
        case 1u: { v = saturate(v); }
        case 2u: { v = 0.5 + 0.5*tanh(clamp(2.0*(v-0.5),-8.0,8.0)); }
        case 3u: { v = 1.0 - abs(2.0*fract(v*0.5) - 1.0); }
        default: {}
    }
    let mode = select(params.blending_mode, 22u, params.blending_mode == 21u);
    return c.a * generated_blend(vec4<f32>(bae_unpremultiply(c).rgb, 1.0), vec3<f32>(v), params.opacity * 0.01, mode);
}
