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

fn fractal_noise(c:vec4f,px:BaePixel)->vec4f {
    return render_fractal(c,px,evolution(),bae_to_layer(params.sub_offset),params.sub_rotation,params.center_subscale!=0u,0.,false);
}
