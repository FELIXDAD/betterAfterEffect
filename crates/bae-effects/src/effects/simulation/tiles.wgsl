// Each image fragment retains its source rectangle as it moves.
fn gradient(which: u32, uv: vec2f, offset: vec2f) -> vec4f {
    if which == 0u {
        return map_at(map0, 0u, uv + offset);
    }
    return map_at(map1, 1u, uv + offset);
}

fn channel(option: u32, uv: vec2f) -> f32 {
    if option<2u {
        return 0.;
    }
    let which = (option-2u) / 9u;
    let c = (option-2u)%9u;
    if (which == 0u && f(10) == 0.) || (which == 1u && f(12) == 0.) {
        return 0.;
    }
    let a = gradient(which, uv, vec2f(0.));
    if c == 0u {
        return lum(a.rgb)-.5;
    }
    if c <= 4u {
        return a[c-1u]-.5;
    }
    let d = select(vec2f(0., 1.), vec2f(1., 0.), c == 5u || c == 7u) / cfg.size;
    let b = gradient(which, uv, d)-gradient(which, uv, -d);
    return select(lum(b.rgb), b.r, c >= 7u);
}

fn component(id: u32, uv: vec2f) -> f32 {
    return f(id + 4u) + channel(u32(f(id)), uv) * f(id + 2u);
}

fn rotate3(v: vec3f, axis: u32, a: f32) -> vec3f {
    if axis == 0u {
        return vec3f(v.x, rot(v.yz, a));
    }
    if axis == 1u {
        let q = rot(v.xz, -a);
        return vec3f(q.x, v.y, q.y);
    }
    return vec3f(rot(v.xy, a), v.z);
}

fn rotate_order(v: vec3f, a: vec3f, order: u32) -> vec3f {
    let orders = array<vec3u, 6>(vec3u(0, 1, 2), vec3u(0, 2, 1), vec3u(1, 0, 2), vec3u(1, 2, 0), vec3u(2, 0, 1), vec3u(2, 1, 0));
    let seq = orders[clamp(order, 1u, 6u)-1u];
    var q = v;
    for (var i = 0u;i<3u;i++) {
        q = rotate3(q, seq[i], a[seq[i]]);
    }
    return q;
}

fn camera(v: vec3f, base: u32) -> vec2f {
    if f(base) == 3. && frame.timing.z > 0. {
        let q = frame.projection * vec4f(v, 1.);
        return q.xy / max(.001, q.w);
    }
    let d = v-vec3f(cfg.size * .5, 0.);
    let angles = radians(vec3f(f(base + 2u), f(base + 4u), f(base + 6u)));
    let order = u32(f(base + 14u));
    let offset = cfg.size * .5-pt(base + 8u);
    var p = d;
    if order>6u {
        p += vec3f(offset, 0.);
    }
    p = rotate_order(p, angles, ((max(1u, order)-1u)%6u) + 1u);
    if order <= 6u {
        p += vec3f(offset, 0.);
    }
    let focal = cfg.size.x * max(.01, f(base + 12u) / 35.);
    let z = max(.01, cfg.size.x * f(base + 10u) + p.z);
    var xy = cfg.size * .5 + p.xy * focal / z;
    if f(base) == 2. {
        let uv = xy / cfg.size;
        xy = mix(mix(pt(base + 16u), pt(base + 18u), uv.x), mix(pt(base + 20u), pt(base + 22u), uv.x), uv.y);
    }
    return xy;
}

fn card_vertex(origin: vec2f, v: vec2f, scale: vec2f, a: vec3f, shift: vec3f) -> vec2f {
    let order = u32(f(16));
    var local = vec3f(v, 0.);
    let seqs = array<vec3u, 6>(vec3u(0, 1, 2), vec3u(0, 2, 1), vec3u(1, 0, 2), vec3u(1, 2, 0), vec3u(2, 0, 1), vec3u(2, 1, 0));
    let seq = seqs[clamp(order, 1u, 6u)-1u];
    for (var i = 0u;i<3u;i++) {
        switch seq[i] {
            case 0u: {
                local += shift;
            }
            case 1u: {
                local = rotate_order(local, a, u32(f(14)));
            }
            default: {
                local *= vec3f(scale, 1.);
            }
        }
    }
    return camera(vec3f(origin, 0.) + local, 66u);
}

// Seeded Voronoi cells give glass, stone and polygon patterns interlocking
// edges. The source coordinates stay attached to each moving fragment.
fn irregular(pattern:u32)->bool {
    return pattern==1u || (pattern>=4u && pattern<=17u);
}
fn fracture_seed(cell:vec2i,pattern:u32)->vec2f {
    let id=hash(u32(cell.x)*0x9e3779b9u ^ u32(cell.y));
    let offset=noise3(id).xy-.5;
    let shift=vec2f(select(-.23,.23,(u32(cell.y)&1u)==0u),0.);
    var jitter=offset*.76;
    if pattern==11u {jitter=shift;}
    if pattern==5u || pattern==9u || pattern==10u {jitter=vec2f(shift.x,offset.y*.28);}
    if pattern==15u {jitter=vec2f(offset.x*.75,offset.y*.08);}
    if pattern==17u {jitter=vec2f(shift.x,-shift.x);}
    let angle=radians(f(12));
    let origin=pt(14)/max(1.,cfg.size.x/max(1.,f(10)));
    let phase=sin(dot(vec2f(cell)-origin,vec2f(.7,1.3)))*.12;
    return vec2f(cell)+.5+rot(jitter+vec2f(phase,-phase)*sin(angle),angle);
}
fn fracture_distance(uv:vec2f,owner:vec2f)->f32 {
    let spacing=cfg.size.x/max(1.,f(10));
    let pos=uv*cfg.size/spacing;
    let cell=vec2i(floor(owner*cfg.size/spacing));
    let pattern=u32(f(6));let d=pos-fracture_seed(cell,pattern);
    let own=dot(d,d);
    var second=100.;
    let limit=vec2i(ceil(cfg.size/spacing));
    for(var y=-1;y<=1;y++){
        for(var x=-1;x<=1;x++){
            let off=vec2i(x,y);let other=cell+off;
            if (x==0&&y==0)||any(other<vec2i(0))||any(other>=limit){continue;}
            let delta=pos-fracture_seed(other,pattern);
            second=min(second,dot(delta,delta));
        }
    }
    return second-own;
}

fn tiles(id: u32, t: f32) -> Sprite {
    var spacing = 1.;
    switch cfg.kind {
        case STARS: {
            spacing = max(1., f(4));
        }
        case POLLY: {
            spacing = max(1., f(7));
        }
        case SHATTER: {
            spacing = cfg.size.x / max(1., f(10));
        }
        default: {
        }
    }
    var step = vec2f(spacing);
    if cfg.kind == CARDS {
        let rows = max(1., round(f(4)));
        let cols = select(max(1., round(rows * cfg.size.x / cfg.size.y)), max(1., round(f(6))), f(2) == 1.);
        step = cfg.size / vec2f(cols, rows);
    }
    let split = (cfg.kind == POLLY && f(8) <= 2.) || (cfg.kind == SHATTER && f(6) >= 19.);
    let cell = select(id, id / 2u, split);
    let mirror = select(1., -1., split && id%2u == 1u);
    let brick = cfg.kind == SHATTER && f(6) == 3.;
    let cols = max(1u, u32(ceil(cfg.size.x / step.x-.00001))) + select(0u, 1u, brick);
    let col = cell%cols;
    let row = cell / cols;
    var start = vec2f(f32(col), f32(row)) * step;
    if brick && row%2u == 1u {
        start.x -= step.x * .5;
    }
    let low = clamp(start, vec2f(0.), cfg.size);
    let high = clamp(start + step, vec2f(0.), cfg.size);
    let half = (high-low) * .5;
    let origin = (high + low) * .5;
    let uv = origin / cfg.size;
    let q = noise3(hash(id));
    var pos = origin;
    var edges = half;
    var angle = 0.;
    var style = TILE;
    var tex = 1.;
    var color = vec4f(1.);
    if cfg.kind == CARDS {
        let shift = vec3f(component(18u, uv), component(24u, uv), component(30u, uv)) * cfg.size.x;
        let a = radians(vec3f(component(36u, uv), component(42u, uv), component(48u, uv)));
        let scale = vec2f(component(54u, uv), component(60u, uv));
        pos = card_vertex(origin, vec2f(0.), scale, a, shift);
        let ex = card_vertex(origin, vec2f(half.x, 0.), scale, a, shift)-pos;
        let ey = card_vertex(origin, vec2f(0., half.y), scale, a, shift)-pos;
        let back = ex.x * ey.y-ex.y * ey.x<0.;
        let n = rotate_order(vec3f(0., 0., 1.), a, u32(f(14)));
        let light = normalize(vec3f((pt(98)-origin) / cfg.size, f(100)) + .00001);
        let l = select(vec3f(0., 0., 1.), light, f(102) == 1.);
        let intensity = f(104) + f(94) * (f(106) * abs(dot(n, l)) + f(108) * pow(abs(dot(n, l)), max(1., f(110))));
        color = vec4f(rgb(96) * intensity, 1.);
        if f(102) == 3. && frame.timing.w > 0. {
            let direction = normalize(frame.light.xyz - vec3f(origin, 0.) + .00001);
            color = vec4f(frame.light_color.rgb * (f(104) + frame.light.w * f(106) * abs(dot(n,direction))), 1.);
        }
        tex = select(1., 4., back && f(8)>0.);
        return Sprite(vec4f(pos, ex), vec4f(ey, uv), vec4f(half / cfg.size, style, tex), color);
    }
    if cfg.kind == STARS {
        let radial = origin-cfg.size * .5;
        let phase = fract(q.z + t * f(2) * .2 + f(3) / 360.);
        let spread = f(1) * .01;
        pos += radial * spread * (1./max(.03, 1.-phase)-1.);
        edges *= f(5) * .01 * (1.+phase * spread);
        style = DISC;
        color = straight(source_at(origin));
        tex = 0.;
        color = vec4f(color.rgb * (1.-f(7) * .01 * (1.-phase)), color.a);
    } else if cfg.kind == SCATTER {
        let side = select(f(3), f(2), origin.x >= cfg.size.x * .5);
        let radial = origin-cfg.size * .5;
        pos = cfg.size * .5 + rot(radial, radians(side) * length(radial) / cfg.size.x)+ (q.xy * 2.-1.) * f(1);
        color = straight(source_at(origin));
        tex = 0.;
    } else if cfg.kind == POLLY {
        let dt = max(0., t-f(10));
        let radial = origin-pt(4);
        let direction = normalize(radial + vec2f(.0001));
        let velocity = rot(direction, (q.x-.5) * TAU * f(5)) * f(1) * (1.+(q.y-.5) * f(6) * 2.);
        pos += velocity * dt + vec2f(0., .24 * cfg.size.y * f(2) * dt * dt);
        angle = (q.z * 2.-1.) * radians(f(3)) * dt;
        edges.x *= cos(angle * .7);
        if f(8) <= 2. {
            style = TRIANGLE;
        }
        if f(8) == 1. || f(8) == 4. {
            color = straight(source_at(origin));
            tex = 0.;
        }
    } else if cfg.kind == SHATTER {
        let pattern = u32(f(6));
        var v = vec3f(0.);
        var released = false;
        for (var j = 0u;j<2u;j++) {
            let force = 24u + 8u * j;
            let delta = (origin-pt(force)) / cfg.size.x;
            let dist = length(vec3f(delta, f(force + 2u)));
            let falloff = max(0., 1.-dist / max(.00001, f(force + 4u)));
            released = released || (falloff>0. && f(force + 6u) != 0.);
            v += normalize(vec3f(delta, .01) + (q-.5) * f(44) + .000001) * falloff * f(force + 6u) * cfg.size.x * .18;
        }
        if f(20)>0. {
            let grad = lum(map_at(map1, 1u, uv).rgb);
            released = released && (select(grad, 1.-grad, f(22)>0.) >= 1.-f(18) * .01);
        }
        if pattern == 1u && f(8)>0. && f(144)>0. {
            released = released && lum(map_at(map0, 0u, uv).rgb)<.99;
        }
        let dt = select(0., t, released);
        let damp = 1./(1.+dt * max(0., f(46)));
        let mass = max(.1, 1.+(q.y-.5) * f(48) * .02);
        let xy = origin + v.xy * dt * damp / mass + dir(f(52)) * f(50) * cfg.size.y * .12 * dt * dt;
        pos = camera(vec3f(xy, v.z * dt + sin(radians(f(54))) * dt * dt * cfg.size.y), 72u);
        angle = dt * (q.z * 2. - 1.) * TAU * f(40);
        if f(2)==2. || f(2)==4. {pos=xy;}
        if f(42) != 2. {
            edges.x *= cos(dt * q.y * TAU * f(40));
        }
        if f(2)>1. {
            style = WIRE;
            tex = 0.;
            color = vec4f(.55, .85, 1., 1.);
        } else {
            let tumble=cos(dt*q.y*TAU*f(40));
            let face=select(60u,68u,tumble<0.);
            let mode=u32(f(face));
            let tint=select(vec3f(1.),rgb(56),mode==1u||mode==3u||mode==4u||mode==6u);
            let normal=normalize(vec3f(sin(angle),sin(dt*q.y*TAU*f(40)),tumble)+.00001);
            let location=vec3f(origin,0.);
            var light_pos=vec3f(pt(104),f(106)*cfg.size.x);
            var light_color=rgb(102)*f(100);
            if f(108)==3. && frame.timing.w>0. {light_pos=frame.light.xyz;light_color=frame.light_color.rgb*frame.light.w;}
            let light=select(vec3f(0.,0.,1.),normalize(light_pos-location+.00001),f(108)!=2.);
            let ndl=abs(dot(normal,light));
            let lit=vec3f(f(110))+light_color*(f(112)*ndl+f(114)*pow(ndl,max(1.,f(116))));
            color=vec4f(tint*lit,f(58));
            if mode==1u||mode==4u {tex=0.;}
            else if f(face+2u)>0.{tex=select(4.,5.,face==68u);}
            if abs(tumble)<min(.8,f(16)) && released {
                edges.x=half.x*max(.01,f(16));
                tex=select(1.,6.,f(66)>0.);
                if f(64)==1.||f(64)==4.{tex=0.;}
                color=vec4f(mix(color.rgb,rgb(56),.3)*.65,color.a);
            }
            if pattern >= 19u {
                style = TRIANGLE;
            }
        }
        if (f(4) == 2. && released) || (f(4) == 3. && !released) {
            color.a = 0.;
        }
    }
    let extent=select(1.,2.8,cfg.kind==SHATTER && irregular(u32(f(6))));
    var result = sprite(pos, edges * mirror * extent, angle, style, color);
    result.y = vec4f(result.y.xy, uv);
    result.brush = vec4f(half * mirror * extent / cfg.size, style, tex);
    return result;
}
