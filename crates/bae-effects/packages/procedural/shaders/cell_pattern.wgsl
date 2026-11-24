// A moving site per lattice cell. Wrap the site labels, not the distances,
// so a tile seam has the same neighbours as any other pixel.
fn cell_random(cell: vec2<i32>, seed: u32) -> f32 {
    var h = u32(cell.x)*0x9e3779b9u + u32(cell.y)*0x85ebca6bu + seed*0xc2b2ae35u;
    h ^= h>>16u;
    h *= 0x7feb352du;
    h ^= h>>15u;
    return f32(h>>8u)/16777216.0;
}

fn sample_cell(pos: vec2<f32>) -> vec4<f32> {
    let q = (pos-params.offset)/(params.size*frame.downsample);
    let base = vec2<i32>(floor(q));
    let tiles = vec2<i32>(params.horizontal_cells,params.vertical_cells);
    var f1 = 1e8;
    var f2 = 1e8;
    var value = 0.0;
    var closest = q;
    let seed = u32(params.random_seed);
    let cycle = max(1.0,params.cycle_in_revolutions);
    let turns = params.evolution/360.0;
    for (var y=-2; y<=2; y++) {
        for (var x=-2; x<=2; x++) {
            let cell = base+vec2<i32>(x,y);
            var label = cell;
            if params.enable_tiling != 0u { label = ((cell%tiles)+tiles)%tiles; }
            let phase = vec2<f32>(cell_random(label,seed+1u),cell_random(label,seed+2u));
            let speed = vec2<f32>(cell_random(label,seed+3u),cell_random(label,seed+4u))+0.5;
            var motion = speed*turns;
            if params.cycle_evolution != 0u { motion = round(speed*cycle)*fract(turns/cycle); }
            let site = vec2<f32>(cell)+0.5+0.5*params.disperse*sin((phase+motion)*6.2831853);
            let delta = q-site;
            let d = dot(delta,delta);
            if d<f1 {
                f2=f1; f1=d; closest=site; value=cell_random(label,seed+5u);
            } else { f2=min(f2,d); }
        }
    }
    let near = sqrt(f1);
    let gap = sqrt(f2)-near;
    if params.cell_pattern == 5u || params.cell_pattern == 11u {
        let sample_pos = vec2<i32>(floor(closest*params.size*frame.downsample+params.offset));
        let rect = bae_rect_input();
        let cell_color = bae_premultiply(bae_input_straight(bae_load_input(clamp(sample_pos, rect.min, rect.max-1))));
        return select(cell_color,vec4<f32>(cell_color.a-cell_color.rgb,cell_color.a),params.invert!=0u);
    }
    var gray = 1.0-0.62*f1;
    switch params.cell_pattern {
        case 2u,8u: { gray=0.76*gap; }
        case 3u,9u: { gray=0.47+0.40*value; }
        case 4u,10u: { gray=pow(value,1.7); }
        case 7u: { gray=saturate(sqrt(5.0*gap)); }
        case 12u: { gray=0.47+0.25*gap; }
        case 13u: { gray=1.0-saturate(0.72*abs(near-gap)); }
        default: {}
    }
    gray=(gray-0.5)*params.context_slider/100.0+0.5;
    switch params.overflow {
        case 2u: { gray=0.5+0.5*tanh(2.0*(gray-0.5)); }
        case 3u: { gray=1.0-abs(fract(gray*0.5)*2.0-1.0); }
        default: { gray=saturate(gray); }
    }
    gray = select(gray,1.0-gray,params.invert!=0u);
    return vec4<f32>(vec3<f32>(gray),1.0);
}

@compute @workgroup_size(8,8)
fn cell_pattern(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if !bae_in_target(p) { return; }
    let pos = vec2<f32>(p)+0.5;
    var color = vec4<f32>(0);
    if params.cell_pattern>=8u && params.cell_pattern<=11u {
        for (var y=0;y<2;y++) {
            for (var x=0;x<2;x++) {
                color += 0.25*sample_cell(pos+vec2<f32>(f32(x),f32(y))*0.5-0.25);
            }
        }
    } else { color=sample_cell(pos); }
    bae_store(p,bae_depth_clamp(color));
}
