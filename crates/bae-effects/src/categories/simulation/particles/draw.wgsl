@compute @workgroup_size(128)
fn advance_emitter(@builtin(global_invocation_id) tid: vec3u) {
    if tid.x >= frame.count {
        return;
    }
    let particle_birth = birth_at(tid.x);
    let id = particle_birth.id;
    let birth = particle_birth.time;
    let life = particle_birth.life;
    let age = max(0.,frame.timing.x-birth);
    if age >= life {
        sprites[tid.x] = Sprite();
        return;
    }
    let hold = select(0., f(91) * .01 * life, cfg.kind == WORLD);
    sprites[tid.x] = particle(id,frame.timing.x,at_age(id,birth,max(0.,age-hold)),life);
}

@compute @workgroup_size(128)
fn advance_weather(@builtin(global_invocation_id) tid: vec3u) {
    if tid.x >= frame.count {
        return;
    }
    let id = frame.first_interval.first_id + tid.x;
    sprites[tid.x] = weather(id, frame.timing.x, integrate(spawn(id,0.),frame.timing.x));
}

@compute @workgroup_size(128)
fn advance_drizzle(@builtin(global_invocation_id) tid: vec3u) {
    if tid.x >= frame.count {
        return;
    }
    let birth = birth_at(tid.x);
    let s = spawn(birth.id,birth.time);
    let age = clamp((frame.timing.x-s.birth) / max(.000001,birth.life), 0., 1.);
    let r = max(1., age * f(6) * cfg.size.x * .001);
    sprites[tid.x] = sprite(s.pos.xy, vec2f(r), 0., 11., vec4f(age, f(3) / 360., 0., select(0., 1.-age, s.birth >= 0.)));
}

@compute @workgroup_size(128)
fn advance_tiles(@builtin(global_invocation_id) tid: vec3u) {
    if tid.x >= frame.count {
        return;
    }
    sprites[tid.x] = tiles(frame.first_interval.first_id + tid.x, frame.timing.x);
}

struct Vary {
    @builtin(position) pos: vec4f, @location(0) local: vec2f, @location(1) uv: vec2f,
    @location(2) @interpolate(flat) brush: vec2f, @location(3) @interpolate(flat) color: vec4f,
    @location(4) @interpolate(flat) origin: vec2f,
}

@vertex
fn vertex(@builtin(vertex_index) id: u32, @location(0) x: vec4f, @location(1) y: vec4f, @location(2) brush: vec4f, @location(3) color: vec4f) -> Vary {
    let q = vec2f(f32(id&1u), f32(id>>1u)) * 2.-1.;
    let xy = x.xy + x.zw * q.x + y.xy * q.y;
    let ndc = (xy-frame.viewport.xy) / frame.viewport.zw * 2.-1.;
    return Vary(vec4f(ndc.x, -ndc.y, 0., 1.), q, y.zw + brush.xy * q, brush.zw, color,y.zw);
}

@fragment
fn fragment(v: Vary) -> @location(0) vec4f {
    var c = v.color;
    var alpha = c.a;
    let kind = u32(v.brush.x);
    let r = length(v.local);
    if alpha <= 0. {
        discard;
    }
    if kind==u32(MERCURY_FIELD){let weight=max(0.,1.-r*r);return vec4f(weight*weight*alpha);}
    let aa = clamp(dot(abs(v.local) / max(.001, r), fwidth(v.local)), .02, .65);
    if kind == u32(TRIANGLE) && v.local.x + v.local.y>0. {
        discard;
    }
    let irregular_piece=cfg.kind==SHATTER && irregular(u32(f(6)));
    if irregular_piece {
        if any(v.uv<vec2f(0.))||any(v.uv>=vec2f(1.)){discard;}
        let edge=fracture_distance(v.uv,v.origin);
        if edge<0.{discard;}
        if kind==u32(WIRE){alpha*=1.-smoothstep(.025,.11,edge);}
    }
    if kind == u32(WIRE) && !irregular_piece {
        alpha *= smoothstep(.83, 1., max(abs(v.local.x), abs(v.local.y)));
    }
    if kind != u32(TILE) && kind != u32(TRIANGLE) && kind != u32(WIRE) {
        var radius = 1.;
        if kind == u32(STAR) {
            radius = .5 + .5 * cos(5.*atan2(v.local.y, v.local.x));
        }
        alpha *= 1.-smoothstep(radius-aa * .5, radius + aa * .5, r);
        if kind == u32(SOFT) {
            alpha *= max(0., 1.-r * r);
        }
        if kind == u32(SPHERE) || kind == u32(BUBBLE) {
            let n = vec3f(v.local, sqrt(max(0., 1.-r * r)));
            var light = normalize(vec3f(-.45, -.65, 1.));
            if cfg.kind == BALL { light=normalize(vec3f(f(10)*.01,-.35,1.)); }
            if cfg.kind == WORLD {
                light = normalize(vec3f(f(87), -f(88), f(89)) + .00001);
            }
            let spec = pow(max(0., dot(n, light)), 20.);
            if kind == u32(BUBBLE) {
                let rim = pow(min(r, 1.), 4.);
                var shade = .14 + .86 * rim;
                if cfg.kind == BUBBLES {
                    switch u32(f(7)) {
                        case 1u: {
                            shade = .55;
                        }
                        case 2u: {
                            shade = .6 + .4 * rim;
                        }
                        case 3u: {
                            c = vec4f(c.rgb * .55, c.a);
                        }
                        case 5u: {
                            shade = 1.-rim;
                        }
                        default: {
                        }
                    }
                }
                alpha *= clamp(shade + spec, 0., 1.);
                c = vec4f(c.rgb * (.6 + .4 * rim) + spec, c.a);
            } else {
                c = vec4f(c.rgb * (.3 + .7 * max(0., dot(n, light))) + spec * .28, c.a);
            }
        }
        if kind == 11u {
            let slope = sin((r-v.color.x) * TAU * max(.1, abs(v.color.y)) * 3.) * sin(r * 3.14159);
            return vec4f(v.local / max(.001, r) * slope * alpha, 0., alpha);
        }
    }
    if alpha <= 0. {
        discard;
    }
    let uv = v.uv;
    var sample_color = vec4f(1.);
    switch u32(v.brush.y) {
        case 1u: {
            sample_color = straight(source_at(uv * cfg.size));
        }
        case 2u: {
            sample_color = straight(map_at(map0, 0u, uv));
        }
        case 3u: {sample_color = straight(map_at(map1,1u,uv));}
        case 4u: {
            sample_color = straight(map_at(map2, 2u, uv));
        }
        case 5u: {
            sample_color = straight(map_at(map3, 3u, uv));
        }
        case 6u: {sample_color=straight(textureLoad(side_map,vec2i(clamp(uv,vec2f(0.),vec2f(.999999))*frame.side_size.xy),0));}
        default: {}
    }
    alpha = clamp(alpha * sample_color.a, 0., 1.);
    return vec4f(c.rgb * sample_color.rgb * alpha, alpha);
}

@compute @workgroup_size(128)
fn advance_ball(@builtin(global_invocation_id) tid:vec3u) {
    if tid.x < frame.count { sprites[tid.x] = ball(frame.first_interval.first_id + tid.x); }
}

@compute @workgroup_size(128)
fn advance_hair(@builtin(global_invocation_id) tid:vec3u) {
    if tid.x < frame.count { sprites[tid.x] = hair(frame.first_interval.first_id + tid.x); }
}

@compute @workgroup_size(128)
fn advance_mercury(@builtin(global_invocation_id) tid:vec3u) {
    if tid.x < frame.count { sprites[tid.x] = mercury(birth_at(tid.x), frame.timing.x); }
}


@compute @workgroup_size(128)
fn advance_playground(@builtin(global_invocation_id) tid:vec3u) {
    if tid.x < frame.count { sprites[tid.x] = playground(birth_at(tid.x), frame.timing.x); }
}
