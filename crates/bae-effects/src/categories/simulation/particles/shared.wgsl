// Positions and velocities are in full-resolution layer pixels and seconds.
struct Config {
    kind: u32, padding0: u32, seed: u32, padding1: u32,
    size: vec2f, padding2: vec2u, control: array<vec4f, 145>,
    history: vec4u, trajectory: array<vec4f, 64>,
}

struct BirthInterval {
    slot: u32, first_id: u32, start: f32, ordinal: f32,
    rate: f32, slope: f32, life: f32, life_slope: f32,
}
@group(0) @binding(1) var<storage, read> intervals: array<BirthInterval>;

struct Frame {
    timing: vec4f, viewport: vec4f, source: vec4f,
    maps: array<vec4f, 4>,
    projection: mat4x4f,
    light: vec4f,
    light_color: vec4f,
    side_size: vec4f, count: u32, interval_count: u32, padding: vec2u, first_interval: BirthInterval,
}

@group(0) @binding(0) var<uniform> cfg: Config;
@group(0) @binding(4) var<uniform> frame: Frame;
@group(0) @binding(5) var image: texture_2d<f32>;
@group(0) @binding(6) var map0: texture_2d<f32>;
@group(0) @binding(7) var map1: texture_2d<f32>;
@group(0) @binding(8) var map2: texture_2d<f32>;
@group(0) @binding(9) var map3: texture_2d<f32>;
@group(0) @binding(10) var side_map: texture_2d<f32>;
@group(0) @binding(11) var select_explosion:texture_2d<f32>;
@group(0) @binding(12) var select_sprite:texture_2d<f32>;
@group(0) @binding(13) var select_gravity:texture_2d<f32>;
@group(0) @binding(14) var select_repeller:texture_2d<f32>;
@group(0) @binding(15) var select_repelled:texture_2d<f32>;
const WORLD = 0u;
const SYSTEMS = 1u;
const RAIN = 2u;
const SNOW = 3u;
const BUBBLES = 4u;
const DRIZZLE = 5u;
const STARS = 6u;
const POLLY = 8u;
const SHATTER = 9u;
const CARDS = 10u;
const PLAYGROUND=11u;
const BALL=12u;
const MERCURY=13u;
const HAIR=14u;
const MERCURY_FIELD=12.;
const TAU = 6.28318530718;
fn f(i: u32) -> f32 {
    return cfg.control[i].x;
}

fn pt(i: u32) -> vec2f {
    return cfg.control[i].xy * cfg.size;
}

// An animated emitter where it was at `birth`, from the sampled trajectory:
// particles already emitted stay on their paths. `current` when not animated.
fn emitter_at(birth: f32, current: vec2f) -> vec2f {
    if cfg.history.x <= 1u { return current; }
    var lo = 0u;
    var hi = cfg.history.x - 1u;
    while hi > lo + 1u {
        let mid = (lo + hi) / 2u;
        if cfg.trajectory[mid].w <= birth { lo = mid; } else { hi = mid; }
    }
    let a = cfg.trajectory[lo];
    let b = cfg.trajectory[hi];
    return mix(a.xy, b.xy, clamp((birth - a.w) / max(.000001, b.w - a.w), 0., 1.));
}

fn rgb(i: u32) -> vec3f {
    return cfg.control[i].rgb;
}

fn hash(bits: u32) -> u32 {
    var x = bits^(bits>>16u);
    x *= 0x7feb352du;
    x^=x>>15u;
    x *= 0x846ca68bu;
    return x^(x>>16u);
}

fn random(i: u32) -> f32 {
    return f32(hash(i^cfg.seed)>>8u) / 16777216.;
}

fn noise3(i: u32) -> vec3f {
    return vec3f(random(i), random(i + 1u), random(i + 2u));
}

fn dir(deg: f32) -> vec2f {
    let a = radians(deg);
    return vec2f(sin(a), -cos(a));
}

fn rot(v: vec2f, a: f32) -> vec2f {
    let c = cos(a);
    let s = sin(a);
    return vec2f(c * v.x-s * v.y, s * v.x + c * v.y);
}

fn straight(c: vec4f) -> vec4f {
    return vec4f(c.rgb / max(0.00001, c.a), c.a);
}

fn source_at(pos: vec2f) -> vec4f {
    let xy = vec2i(floor(pos * frame.timing.y + frame.source.xy));
    if any(xy<vec2i(0)) || any(xy >= vec2i(frame.source.zw)) {
        return vec4f(0.);
    }
    return textureLoad(image, xy, 0);
}

fn map_at(tex: texture_2d<f32>, index: u32, uv: vec2f) -> vec4f {
    let r = frame.maps[index];
    return textureLoad(tex, vec2i(clamp(uv, vec2f(0.), vec2f(0.999999)) * r.zw), 0);
}

fn lum(c: vec3f) -> f32 {
    return dot(c, vec3f(.2126, .7152, .0722));
}
