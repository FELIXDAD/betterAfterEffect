struct State {
    pos: vec3f, birth: f32, vel: vec3f, variation: f32
}

fn life() -> f32 {
    switch cfg.kind {
        case WORLD: {
            return max(.001, f(5));
        }
        case SYSTEMS, DRIZZLE: {
            return max(.001, f(2));
        }
        default: {
            return 0.;
        }
    }
}

fn spawn(id: u32, generation: i32, birth: f32) -> State {
    let key = hash(id + u32(generation) * 0x9e3779b9u);
    let q = noise3(key) * 2.-1.;
    let r = noise3(key + 3u) * 2.-1.;
    var s = State(vec3f(q.xy * cfg.size, 0.), birth, vec3f(0.), random(key + 6u));
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

fn birth_cycle(id: u32, t: f32, l: f32) -> i32 {
    return i32(floor(t / l-(f32(id) + .5) / max(1., f32(cfg.total))));
}

fn born(id: u32, cycle: i32, l: f32) -> f32 {
    return (f32(cycle) + (f32(id) + .5) / max(1., f32(cfg.total))) * l;
}

fn motion_age(t: f32, birth: f32) -> f32 {
    let hold = select(0., f(91) * .01 * life(), cfg.kind == WORLD);
    return max(0., t - birth - hold);
}

fn at_time(id: u32, t: f32) -> State {
    let l = life();
    var cycle = 0;
    var birth = 0.;
    if l>0. {
        cycle = birth_cycle(id, t, l);
        birth = born(id, cycle, l);
    }
    return integrate(spawn(id, cycle, birth), motion_age(t,birth));
}
