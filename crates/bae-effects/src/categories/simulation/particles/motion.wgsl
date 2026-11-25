struct State {
    pos: vec3f, birth: f32, vel: vec3f, variation: f32, touched_floor: bool
}

fn spawn(id: u32, birth: f32) -> State {
    let key = hash(id);
    let q = noise3(key) * 2.-1.;
    let r = noise3(key + 3u) * 2.-1.;
    var s = State(vec3f(q.xy * cfg.size, 0.), birth, vec3f(0.), random(key + 6u), false);
    let width = cfg.size.x;
    switch cfg.kind {
        case WORLD, SYSTEMS: {
            var center = vec3f(pt(4), 0.);
            var spread = vec3f(f(5), f(6), 0.) * .01 * width;
            var speed = f(10);
            var anim = u32(f(9));
            var angle = f(14);
            var extra = f(15);
            var axis = vec3f(dir(angle), 0.);
            if cfg.kind == WORLD {
                center = vec3f(cfg.size * .5, 0.) + vec3f(f(7), f(8), f(9)) * width;
                spread = vec3f(f(10), f(11), f(12)) * width;
                speed = f(16);
                anim = u32(f(15));
                angle = f(20);
                extra = f(19);
                axis = normalize(vec3f(f(95), f(96), f(97)) + vec3f(0., .00001, 0.));
            }
            var inherited = vec3f(0.);
            if cfg.history.x > 1u {
                var lo = 0u;
                var hi = cfg.history.x - 1u;
                while hi > lo + 1u {
                    let mid = (lo + hi) / 2u;
                    if cfg.trajectory[mid].w <= birth { lo = mid; } else { hi = mid; }
                }
                let a = cfg.trajectory[lo];
                let b = cfg.trajectory[hi];
                let dt = max(.000001, b.w - a.w);
                center = mix(a.xyz, b.xyz, clamp((birth - a.w) / dt, 0., 1.));
                inherited = (b.xyz - a.xyz) / dt * select(f(11), f(17), cfg.kind == WORLD) * .01;
            }
            s.pos = center + spread * q;
            var v = r;
            if cfg.kind != WORLD {
                v.z = 0.;
            }
            if (cfg.kind == WORLD && (anim == 2u || anim == 3u || anim == 14u)) || (cfg.kind != WORLD && anim >= 8u) {
                v = axis * (.65 + .35 * r.y) + r * max(.015, abs(extra)) * .4;
            } else if (cfg.kind == WORLD && anim == 9u) || (cfg.kind != WORLD && anim == 6u) {
                v = vec3f(r.x * .22, -abs(r.y)-.4, r.z * .15);
            } else if anim == 11u {
                v = vec3f(abs(r.x), r.y * .1, r.z * .15);
            }
            if cfg.kind == WORLD && anim == 5u {
                v *= .3;
            }
            s.vel = v * speed * width * .23 + inherited;
        }
        case DRIZZLE: {
            s.pos = vec3f((q.xy * .5 + .5) * cfg.size, 0.);
        }
        case RAIN, SNOW, BUBBLES: {
            s.pos = vec3f((q.xy * .5 + .5) * cfg.size, r.z);
            if cfg.kind == RAIN {
                s.vel = vec3f(f(5) * (1.+r.x * f(6) * .01) + q.x * f(7), f(4), 0.);
            }
            if cfg.kind == SNOW {
                s.vel = vec3f(f(7) * (1.+r.x * f(8) * .01) + q.x * f(9), f(5) * (1.+r.y * f(6) * .01), 0.);
            }
            if cfg.kind == BUBBLES {
                s.vel = vec3f(0., -f(2) * cfg.size.y * .18, 0.);
            }
            s.vel *= .35 + .65 * s.variation;
        }
        default: {
        }
    }
    return s;
}

fn integrate(s: State, dt: f32) -> State {
    var g = vec3f(0.);
    var drag = 0.;
    switch cfg.kind {
        case WORLD: {
            g = vec3f(f(100), f(101), f(102)) * f(18) * cfg.size.x * .45;
            drag = f(41);
        }
        case SYSTEMS: {
            g.y = f(12) * cfg.size.x * .45;
            drag = f(13);
        }
        default: {
        }
    }
    var next = s;
    if drag>.001 {
        let decay = exp(-drag * dt);
        let travel = (1.-decay) / drag;
        next.pos += s.vel * travel + g * ((dt-travel) / drag);
        next.vel = s.vel * decay + g * travel;
    } else {
        next.pos += dt * (s.vel + .5 * g * dt);
        next.vel += g * dt;
    }
    return next;
}

struct Birth { id: u32, time: f32, life: f32 }

fn birth_at(slot: u32) -> Birth {
    var span = frame.first_interval;
    if frame.interval_count > 1u {
        var lo = 0u;
        var hi = frame.interval_count;
        while lo + 1u < hi {
            let mid = (lo + hi) / 2u;
            if intervals[mid].slot <= slot { lo = mid; } else { hi = mid; }
        }
        span = intervals[lo];
    }
    let i = slot - span.slot;
    // Invert the integral of r(t) = rate + slope*t within this interval.
    let n = span.ordinal + f32(i);
    let dt = 2.*n / max(1e-12, span.rate + sqrt(max(0., span.rate*span.rate + 2.*span.slope*n)));
    return Birth(span.first_id+i, span.start+dt, span.life+dt*span.life_slope);
}

fn at_age(id: u32, birth: f32, age: f32) -> State {
    var s = spawn(id,birth);
    let action = u32(f(66));
    let collide = cfg.kind == WORLD && action > 1u;
    let turbulent = (cfg.kind == WORLD && f(15)>=13.) || (cfg.kind == SYSTEMS && f(9)==2.);
    if !collide && !turbulent { return integrate(s,age); }
    let phase = noise3(hash(id))*TAU;
    let floor_y = cfg.size.y * .5 + f(3)*cfg.size.x;
    let bounce = clamp((f(67)+(s.variation-.5)*f(68))*.01,0.,2.);
    let scatter = (s.variation-.5)*f(69)*.01;
    // Collision and noise forces use a fixed grid anchored at birth.
    for (var step = 0u; f32(step)/60. < age; step++) {
        let dt = min(1./60., age-f32(step)/60.);
        if turbulent {
            let strength = select(f(15),f(19),cfg.kind==WORLD)*cfg.size.x*0.07;
            let field = sin(s.pos.yzx * (8./cfg.size.x) + phase + vec3f(f32(step)/30.));
            s.vel += field*strength*dt;
        }
        var next = integrate(s,dt);
        if collide && next.pos.y >= floor_y && next.vel.y >= 0. {
            let hit = dt * clamp((floor_y-s.pos.y)/max(1e-6,next.pos.y-s.pos.y),0.,1.);
            var contact = integrate(s,hit);
            contact.pos.y = floor_y;
            contact.touched_floor = true;
            if action == 3u { contact.vel = vec3f(0.); return contact; }
            let impact = max(0.,contact.vel.y);
            contact.vel.y = select(0.,-impact*bounce,action == 4u);
            contact.vel.x += select(0.,impact*scatter,action == 4u);
            next = integrate(contact,dt-hit);
            if next.pos.y >= floor_y {
                next.pos.y = floor_y;
                next.vel.y = min(0.,next.vel.y);
            }
        }
        s = next;
    }
    return s;
}
