// Instance attributes: center/first edge, second edge/texture center,
// texture half-extent/shape/texture selector, straight RGBA.
struct Sprite {
    x: vec4f, y: vec4f, brush: vec4f, color: vec4f
}

@group(0) @binding(3) var<storage, read_write> sprites: array<Sprite>;
const LINE = 1.;
const STAR = 2.;
const SPHERE = 3.;
const SOFT = 4.;
const BUBBLE = 5.;
const TILE = 6.;
const TRIANGLE = 7.;
const FLAKE = 8.;
const DISC = 9.;
const WIRE = 10.;
fn sprite(pos: vec2f, half: vec2f, angle: f32, style: f32, color: vec4f) -> Sprite {
    return Sprite(vec4f(pos, rot(vec2f(half.x, 0.), angle)), vec4f(rot(vec2f(0., half.y), angle), 0., 0.), vec4f(0., 0., style, 0.), color);
}

fn particle(id: u32, t: f32, s: State, lifespan: f32) -> Sprite {
    let world = cfg.kind == WORLD;
    let age = clamp((t-s.birth) / max(.000001,lifespan), 0., 1.);
    let rv = s.variation;
    let q = noise3(hash(id));
    var pos = s.pos;
    var vel = s.vel;
    let w = cfg.size.x;
    let anim = u32(select(f(9), f(15), world));
    let extra = select(f(15), f(19), world);
    if (world && anim >= 6u && anim <= 8u) || (!world && anim >= 3u && anim <= 5u) {
        let center = select(pt(4), cfg.size * .5 + vec2f(f(7), f(8)) * w, world);
        let angle = age * (2.+extra * 4.) * TAU;
        pos = vec3f(center + rot(pos.xy-center, angle), pos.z);
        vel = vec3f(rot(vel.xy, angle), vel.z);
    }
    var visible = s.birth >= 0. && t-s.birth < lifespan;
    if world {
        let floor_y = cfg.size.y * .5 + f(3) * w;
        let below = pos.y>floor_y;
        if f(64) == 2. {
            visible = visible && !below;
        }
        if f(64) == 3. {
            visible = visible && (below || s.touched_floor);
        }
        if f(65) == 2. {
            pos.y = floor_y + abs(pos.y-floor_y);
        }
        if f(65) == 3. {
            pos.y = floor_y;
        }
    }
    var projected = pos.xy;
    var perspective = 1.;
    if world {
        let point = frame.projection * vec4f(pos, 1.);
        let w = max(.001, point.w);
        projected = point.xy / w;
        visible = visible && point.w > select(.001, 1., point.z > 0.);
        let offset = point + frame.projection[0];
        let scale = select(distance(projected, offset.xy / max(.001, offset.w)), point.z / w, point.z > 0.);
        perspective = clamp(scale, .01, 12.);
    }
    let variation = select(f(21), f(26) * .01, world);
    let size = mix(select(f(19), f(24), world), select(f(20), f(25), world), age) *w * .018 * perspective * max(0., 1.+(rv * 2.-1.) * variation);
    let typ = u32(select(f(18), f(23), world));
    var style = DISC;
    var half = vec2f(size);
    var angle = select(0., radians(f(42) * rv + f(43) * (t-s.birth)), world);
    if typ == 1u {
        style = LINE;
        angle = atan2(vel.y, vel.x);
        half = vec2f(max(.6, length(vel.xy) * .008 * perspective + size * .4), max(.48, size * .12));
    } else if typ == 2u {
        style = STAR;
    } else if typ == 4u {
        style = SPHERE;
    } else if typ == 5u || typ == 6u {
        style = SOFT;
    } else if typ == 7u {
        style = BUBBLE;
    } else if typ >= 9u {
        style = TILE;
        if (world && (typ == 11u || typ == 13u || typ == 15u)) || (!world && (typ == 10u || typ == 13u || typ == 14u)) {
            style = TRIANGLE;
        }
        if (world && typ >= 18u && typ <= 22u) || (!world && typ >= 16u) {
            style = select(SPHERE, BUBBLE, typ == 22u || (!world && typ == 20u));
        }
        if world && typ >= 25u {
            style = select(DISC, SOFT, typ == 26u);
        }
        if world && (f(49)>0. || typ == 9u || typ == 10u) {
            angle = atan2(vel.y, vel.x);
        }
    }
    var opacity = select(f(23), f(27) * .01, world);
    if !world {
        switch u32(f(22)) {
            case 2u: {
                opacity *= 1.-age * age;
            }
            case 3u: {
                opacity *= 1.-age;
            }
            case 4u: {
                opacity *= age;
            }
            case 5u: {
                opacity *= sin(age * 3.14159);
            }
            case 6u: {
                opacity *= abs(sin(age * TAU * 2.));
            }
            default: {
            }
        }
    }
    let origin = select(pt(4), cfg.size * .5 + vec2f(f(7), f(8)) * w, world);
    let src = straight(source_at(origin));
    let cmap = u32(select(f(24) + 1., f(28), world));
    var c0 = select(rgb(25), rgb(29), world);
    var c1 = select(rgb(26), rgb(30), world);
    if cmap == 1u && world {
        let phase = min(age * 4., 3.99999);
        let idx = 74u + u32(phase);
        c0 = mix(rgb(idx), rgb(idx + 1u), fract(phase));
        c1 = c0;
    } else {
        if cmap == 3u || cmap == 5u {
            c0 = src.rgb;
        }
        if cmap >= 4u {
            c1 = src.rgb;
        }
    }
    var c = mix(c0, c1, age);
    if world {
        c *= 1.-clamp(f(40) * .01 * rv, 0., 1.);
        if f(82)>1. {
            let fog = clamp(pos.z / (max(.001, f(83)) * w), 0., 1.);
            if f(82) == 3. {
                c = mix(c, rgb(84), fog);
            } else {
                opacity *= 1.-fog;
            }
        }
    } else if f(29)>0. {
        opacity *= src.a;
    }
    var result = sprite(projected, half, angle, style, vec4f(c, select(0., opacity, visible)));
    if world && f(45)>0. && (typ == 13u || typ == 14u || typ >= 24u) {
        result.y = vec4f(result.y.xy, vec2f(.5) + (q.xy-.5) * f(46) * .01);
        let crop = .5 * (1. - clamp(f(46) * .01, 0., .999));
        var texture_index = 2.;
        if f(47) == 1. {
            for (var i = 1u; i < 4u; i++) {
                if s.birth >= frame.maps[i].x { texture_index = 2. + f32(i); }
            }
        } else if f(47) == 3. {
            texture_index += round(clamp((t-s.birth) / max(.001,frame.maps[3].x), 0., 1.) * 3.);
        }
        result.brush = vec4f(crop, crop, style, texture_index);
    }
    if !world && (typ==14u || typ==15u) {
        result.y = vec4f(result.y.xy,vec2f(.5));
        result.brush = vec4f(.5,.5,style,1.);
    }
    return result;
}

fn weather(id: u32, t: f32, s: State) -> Sprite {
    let rain = cfg.kind == RAIN;
    let q = noise3(hash(id));
    var pos = s.pos.xy;
    if cfg.kind == BUBBLES {
        pos.x += sin(t * f(4) * TAU + q.x * TAU) * f(3);
        pos = fract(pos / cfg.size) * cfg.size;
        let size = max(0., f(5)) * (.4 + s.variation) * cfg.size.x * .01;
        let reflection = straight(source_at(pos + vec2f(size * .5, -size)));
        let base = select(reflection.rgb, vec3f(.7, .85, 1.), reflection.a<.001);
        let tint = select(mix(base, vec3f(.8, .93, 1.), .45), mix(base, vec3f(.8), .65), f(6)>1.);
        return sprite(pos, vec2f(size), 0., BUBBLE, vec4f(tint, .75));
    }
    let depth = select(f(4), f(3), rain);
    let z = 1./(1.+q.z * depth * .0005);
    pos += select(pt(28), pt(19), rain)-cfg.size * .5;
    var size = f(2) * (.4 + z);
    var style = LINE;
    var angle = atan2(s.vel.y, s.vel.x);
    var half = vec2f(max(1., length(s.vel.xy) * .006 * z), size * .28);
    if !rain {
        var phase = t * f(13) * (1.+(q.z-.5) * f(14) * .02) + q.x * TAU;
        if f(15)>0. {
            phase += sin(t * .61 + q.y * TAU);
        }
        pos.x += sin(phase) * f(11) * (1.+(q.x-.5) * f(12) * .02);
        size *= max(0., 1.+(q.y-.5) * f(3) * .02);
        half = vec2f(size, size * (1.-f(16) * .009 * abs(sin(phase))));
        style = FLAKE;
        angle = phase;
    }
    let ground = clamp(select(f(29), f(20), rain) * .01, 0., 2.);
    let embed = select(f(30), f(21), rain) * .01;
    let floor_y = max(1., cfg.size.y * (ground + (1.-ground) * z * embed));
    pos = fract(pos / vec2f(cfg.size.x, floor_y)) * vec2f(cfg.size.x, floor_y);
    let radius = select(vec2f(f(22), f(23)), vec2f(f(12), f(13)), rain);
    let src = straight(source_at(pos + (q.xy-.5) * radius));
    let influence = select(f(21), f(11), rain) * .01 * src.a;
    var color = select(rgb(18), rgb(8), rain) * mix(vec3f(1.), src.rgb, clamp(influence, 0., 1.));
    if rain && f(18) == 1. {
        color = mix(color, src.rgb, .5 * src.a);
    }
    let source_depth = f(144);
    let visibility = select(1., 1.-source_at(pos).a, q.z > source_depth);
    return sprite(pos, half, angle, style, vec4f(color, select(f(19), f(9), rain) * .01 * z * visibility));
}
