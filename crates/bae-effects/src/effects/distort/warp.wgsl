// Layer coordinates denote pixel edges; texel (0, 0) is centred at (0.5, 0.5).
struct Uniforms {
    src: vec4<i32>, dst: vec4<i32>, layer: vec4<f32>,
    kind: u32, scale: f32, time: f32, hdr: u32,
    params: array<vec4<f32>, 15>, inverse: mat3x3<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;
// Discriminants of the Rust Kind enum.
const TRANSFORM = 0u;
const TURBULENT = 1u;
const WAVE = 2u;
const BULGE = 3u;
const TWIRL = 4u;
const POLAR = 5u;
const CORNER = 6u;
const OFFSET = 7u;
const MIRROR = 8u;
const SPHERIZE = 9u;
const RIPPLE = 10u;
const OPTICS = 11u;
const TAU = 6.28318530718;
fn val(i: u32) -> f32 {
    return u.params[i].x;
}

fn pt(i: u32) -> vec2<f32> {
    return u.params[i].xy;
}

fn texel(i: vec2<i32>, wrap: bool) -> vec4<f32> {
    var q = i - u.src.xy;
    let size = u.src.zw - u.src.xy;
    if wrap {
        q = ((q % size) + size) % size;
    }
    if any(q < vec2(0)) || any(q >= size) {
        return vec4(0.);
    }
    return textureLoad(src, q, 0);
}

fn linear(p: vec2<f32>, wrap: bool) -> vec4<f32> {
    let uv = p - 0.5;
    let i = vec2<i32>(floor(uv));
    let f = fract(uv);
    return mix(mix(texel(i, wrap), texel(i + vec2(1, 0), wrap), f.x),
        mix(texel(i + vec2(0, 1), wrap), texel(i + vec2(1, 1), wrap), f.x), f.y);
}
// Integrate premultiplied texel coverage over the source footprint when a
// destination pixel covers several source pixels. Very large footprints use
// stratified sampling to bound the cost of near-singular projections.
fn area(p: vec2<f32>, footprint: vec2<f32>) -> vec4<f32> {
    let w = max(footprint, vec2(1.));
    if all(w < vec2(1.001)) {
        return linear(p, false);
    }
    let lo = p - 0.5 * w;
    let hi = p + 0.5 * w;
    if any(hi <= vec2<f32>(u.src.xy)) || any(lo >= vec2<f32>(u.src.zw)) {
        return vec4(0.);
    }
    var c = vec4(0.);
    if any(w > vec2(12.)) {
        for (var y = 0; y < 4; y++) {
            for (var x = 0; x < 4; x++) {
                c += linear(lo + (vec2<f32>(f32(x), f32(y)) + 0.5) * w * 0.25, false);
            }
        }
        return c / 16.;
    }
    for (var y = i32(floor(lo.y)); y < i32(ceil(hi.y)); y++) {
        let wy = min(hi.y, f32(y) + 1.) - max(lo.y, f32(y));
        for (var x = i32(floor(lo.x)); x < i32(ceil(hi.x)); x++) {
            let wx = min(hi.x, f32(x) + 1.) - max(lo.x, f32(x));
            c += texel(vec2(x, y), false) * wx * wy;
        }
    }
    return c / (w.x * w.y);
}

fn cubic_weights(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5 * vec4(-t + 2. * t2 - t3, 2. - 5. * t2 + 3. * t3, t + 4. * t2 - 3. * t3, -t2 + t3);
}

fn cubic(p: vec2<f32>) -> vec4<f32> {
    let uv = p - 0.5;
    let i = vec2<i32>(floor(uv)) - 1;
    let x = cubic_weights(fract(uv.x));
    let y = cubic_weights(fract(uv.y));
    var c = vec4(0.);
    for (var j = 0; j < 4; j++) {
        for (var k = 0; k < 4; k++) {
            c += texel(i + vec2(k, j), false) * x[k] * y[j];
        }
    }
    return c;
}

fn rot(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2(c * p.x - s * p.y, s * p.x + c * p.y);
}

fn center() -> vec2<f32> {
    return u.layer.xy + 0.5 * u.layer.zw;
}

// Value noise changes continuously between lattice points, including in evolution.
fn lattice(i: vec3<i32>) -> f32 {
    let k = vec3<u32>(i) * vec3(1597334677u, 3812015801u, 2798796415u);
    var n = k.x ^ k.y ^ k.z;
    n = (n ^ (n>>16u)) * 2246822519u;
    return f32(n ^ (n>>13u)) / 2147483648. - 1.;
}

fn noise(p: vec3<f32>, smoother: bool) -> f32 {
    let i = vec3<i32>(floor(p));
    let t = fract(p);
    let s = select(t * t * (3. - 2. * t), t * t * t * (t * (6. * t - 15.) + 10.), smoother);
    var row: array<f32, 4>;
    for (var j = 0; j < 4; j++) {
        var base = i + vec3(0, j % 2, j / 2);
        if u.kind == TURBULENT {
            if val(8) > 0. {
                let period = 2 * i32(val(9));
                base.z = ((base.z % period) + period) % period;
            }
            base.z += 13 * i32(val(10));
        }
        row[j] = mix(lattice(base), lattice(base + vec3(1, 0, 0)), s.x);
    }
    return mix(mix(row[0], row[1], s.y), mix(row[2], row[3], s.y), s.z);
}

// The AE coordinate probes show that vertical displacement is constant down
// each column and horizontal displacement across each row. Bulge and Twist
// use the same scalar field, with their vectors a quarter-turn apart.
fn turbulence_field(p: vec3<f32>, mode: i32) -> vec2<f32> {
    let smoother = mode >= 5 && mode <= 7;
    let a = vec3(14.31, 5.19, 2.07);
    let b = vec3(3.79, 41.83, 7.17);
    if mode >= 9 {
        let h = noise(vec3(0., p.y, p.z) + b, false);
        let v = noise(vec3(p.x, 0., p.z) + a, false);
        return 0.65 * vec2(select(h, 0., mode == 9), select(v, 0., mode == 10));
    }
    if mode == 2 || mode == 3 || mode == 6 || mode == 7 {
        let step = vec2(0.35, 0.);
        let dx = noise(p + a + step.xyy, smoother) - noise(p + a - step.xyy, smoother);
        let dy = noise(p + a + step.yxy, smoother) - noise(p + a - step.yxy, smoother);
        return 0.65 * select(vec2(dx, dy), vec2(-dy, dx), mode == 3 || mode == 7);
    }
    return 0.65 * vec2(noise(p + a, smoother), noise(p + b, smoother));
}

fn turbulent(p: vec2<f32>) -> vec2<f32> {
    let mode = i32(val(1));
    let size = val(3) * u.scale;
    var xy = (p - pt(4)) / size;
    // Two independent lattice planes per turn. Wrapping the integer plane
    // index gives looping evolution a continuous seam without moving the field.
    var z = val(6) / 180.;
    var d = vec2(0.);
    var norm = 0.;
    var amp = 1.;
    for (var i = 0; i < i32(ceil(val(5))); i++) {
        let w = amp * min(1., val(5) - f32(i));
        d += w * turbulence_field(vec3(xy, z), mode);
        norm += w;
        amp *= 0.5;
        xy = xy * 2. + 3.7;
        z = z * 2. + 0.9;
    }
    let pin = i32(val(12));
    let locked = pin >= 11;
    let edges = select(pin, pin - 8, locked);
    let span = min(vec2(size * 0.9), 0.5 * u.layer.zw);
    let lo = clamp((p - u.layer.xy) / span, vec2(0.), vec2(1.));
    let hi = clamp((u.layer.xy + u.layer.zw - p) / span, vec2(0.), vec2(1.));
    var weight = vec2(1.);
    switch edges {
        case 3: {
            weight = min(lo, hi);
        }
        case 4: {
            weight.y = min(lo.y, hi.y);
        }
        case 5: {
            weight.x = min(lo.x, hi.x);
        }
        case 6: {
            weight.x = lo.x;
        }
        case 7: {
            weight.x = hi.x;
        }
        case 8: {
            weight.y = lo.y;
        }
        case 9: {
            weight.y = hi.y;
        }
        default: {
        }
    }
    weight = 0.5 - 0.5 * cos(weight * TAU * 0.5);
    if locked {
        weight = vec2(min(weight.x, weight.y));
    }
    return p + d * (val(2) * u.scale / norm) * weight;
}

fn waveform(x: f32, mode: i32) -> f32 {
    // The seven periodic shapes share a 2048-step phase table in AE 26.
    let t = floor(fract(x) * 2048.) / 2048.;
    let tri = 4. * abs(t - 0.5) - 1.;
    let circle = sqrt(max(0., 1. - pow(2. * fract(2. * t) - 1., 2.)));
    switch mode {
        case 2: {
            return select(1., -1., t < 0.5);
        }
        case 3: {
            return tri;
        }
        case 4: {
            return 1. - 2. * t;
        }
        case 5: {
            return select(circle, -circle, t < 0.5);
        }
        case 6: {
            return 2. * sqrt(max(0., 1. - pow(2. * t - 1., 2.))) - 1.;
        }
        case 7: {
            return select(1. - circle, circle - 1., t < 0.5);
        }
        case 8: {
            return lattice(vec3(i32(floor(t * val(3) * 2.)), 0, 0));
        }
        case 9: {
            return 0.35 * noise(vec3(t * val(3) * 0.75, 0.5, 0.25), true);
        }
        default: {
            return - sin(TAU * t);
        }
    }
}

fn wave(p: vec2<f32>) -> vec2<f32> {
    let a = radians(val(4));
    let axis = vec2(sin(a), -cos(a));
    let dir = select(axis, vec2(0.), abs(axis) < vec2(1e-6));
    let perp = vec2(-dir.y, dir.x);
    let xy = p - u.layer.xy;
    let phase = dot(xy, dir) / (2. * val(3) * u.scale) - val(5) * u.time + val(7) / 1440.;
    let n = clamp((xy - 0.75) / (u.layer.zw - vec2(0.5)), vec2(0.), vec2(1.));
    let e = min(4. * min(n, 1. - n), vec2(1.));
    var weight = 1.;
    switch i32(val(6)) {
        case 2: {
            weight = e.x * e.y;
        }
        case 3: {
            weight = 1. - min(e.x, e.y);
        }
        case 5: {
            weight = min(4. * n.x, 1.);
        }
        case 6: {
            weight = min(4. * n.y, 1.);
        }
        case 7: {
            weight = min(4. * (1. - n.x), 1.);
        }
        case 8: {
            weight = min(4. * (1. - n.y), 1.);
        }
        case 10: {
            weight = e.y;
        }
        case 11: {
            weight = e.x;
        }
        default: {
        }
    }
    return p + perp * val(2) * u.scale * waveform(phase, i32(val(1))) * weight;
}

fn bulge(p: vec2<f32>) -> vec2<f32> {
    let radii = vec2(val(1), val(2)) * u.scale;
    let d = p - (pt(3) + 0.5);
    if min(radii.x, radii.y) == 0. || val(4) == 0. {
        return p;
    }
    let r = length(d / radii);
    if r >= 1. || r < 0.00001 {
        return p;
    }
    var delta = val(4) * 0.997 * (r / sin(r * TAU * 0.25) - 1.);
    if val(5) > 0. {
        delta *= smoothstep(0., val(5) * u.scale, (1. - r) * max(radii.x, radii.y));
    }
    if val(7) > 0. {
        let uv = (p - 0.5 - u.layer.xy) / u.layer.zw;
        let edge = 4.*min(uv, 1.-uv);
        let t=clamp(min(edge.x,edge.y),0.,1.);
        delta *= mix(t,smoothstep(0.,1.,t),0.65);
    }
    return p + delta * d;
}

fn radial(p: vec2<f32>) -> vec2<f32> {
    let size = u.layer.zw;
    switch u.kind {
        case TWIRL: {
            let c = pt(3) + 0.5;
            let d = p - c;
            let r = length(d);
            let radius = val(2) * 0.01 * max(size.x, size.y);
            if r >= radius {
                return p;
            }
            let t = mix(round(r), r, 0.35) / radius;
            let floor_weight = mix(0.0939, 0.0974, clamp((val(2) - 30.) / 50., 0., 1.));
            let falloff = max(0., (exp(-2.455 * t * t) - floor_weight) / (1. - floor_weight));
            return c + rot(d, -radians(val(1)) * falloff);
        }
        case SPHERIZE: {
            let c = pt(2) + 0.5;
            let d = p - c;
            let r = length(d);
            let radius = val(1) * u.scale;
            if r >= radius || r == 0. {
                return p;
            }
            let t = r / radius;
            return c + d * t / sin(t * 0.25 * TAU);
        }
        case RIPPLE: {
            let d = p - pt(2) - 0.5;
            let r = length(d);
            let radius = val(1) * 0.01 * max(size.x, size.y);
            if radius == 0. || r == 0. {
                return p;
            }
            let t = r / radius;
            let k = mix(1.801, 1.875, clamp((val(1) - 30.) / 50., 0., 1.));
            let envelope = max(0., (exp(-k * t * t) - 0.093) / 0.907);
            let phase = 0.5 * r / (val(5) * u.scale) - val(4) * u.time + val(7) / 360.;
            let displacement = 0.25 * val(6) * u.scale * envelope * cos(TAU * phase);
            return p + select(vec2(1.), d / r, val(3) == 2.) * displacement;
        }
        default: {
            let d = p - pt(4);
            let r = length(d);
            let a = radians(val(1)) * 0.5;
            if a < 0.00001 || r == 0. {
                return p;
            }
            var span = size.x * 0.5;
            if val(3) == 2. {
                span = size.y * 0.5;
            } else if val(3) == 3. {
                span = length(size) * 0.5;
            }
            let k = tan(a) / span;
            let rr = r * k;
            let reverse = val(2) > 0.;
            var zoom2 = 1.;
            if val(5) > 0. {
                let corner = length(0.5 * size) * k;
                zoom2 = select(1. + 0.384 * corner * corner, 1. / (1. + corner * corner), reverse);
            }
            let denom = zoom2 + select(-rr * rr, rr * rr, reverse);
            if denom <= 0. {
                return vec2(-1e7);
            }
            return pt(4) + d * inverseSqrt(denom);
        }
    }
}

fn polar(p: vec2<f32>) -> vec2<f32> {
    let xy = p - 0.5 - u.layer.xy;
    let d = xy - 0.5 * u.layer.zw;
    var s = vec2(0.);
    if val(2) == 1. {
        let a = select(atan2(d.x, d.y), 0., all(d == vec2(0.)));
        s = vec2((0.5 + a / TAU) * u.layer.z * 0.99664, length(d) * 2.);
    } else {
        let a = TAU * xy.x / u.layer.z;
        s = 0.5 * u.layer.zw - vec2(cos(a), sin(a)) * xy.y * 0.5 * length(u.layer.zw) / u.layer.w;
    }
    return mix(p, u.layer.xy + 0.5 + s, val(1));
}

// AE Low quality averages a 2 × 2 grid of displaced nearest texels.
fn wave_color(p: vec2<f32>) -> vec4<f32> {
    var c = vec4(0.);
    let count = 2 * i32(val(8));
    for (var y = 0; y < count; y++) {
        for (var x = 0; x < count; x++) {
            let sub = p - 0.5 + (vec2<f32>(f32(x), f32(y)) + 0.5) / f32(count);
            let probe = sub + 0.5 / f32(count);
            let displacement = floor((wave(probe) - probe) * f32(count) + 0.0001) / f32(count);
            c += texel(vec2<i32>(floor(sub + displacement)), false);
        }
    }
    return c / f32(count * count);
}

fn color(p: vec2<f32>) -> vec4<f32> {
    var q = p;
    switch u.kind {
        case TRANSFORM, CORNER: {
            let bias = select(0., 0.5, u.kind == CORNER);
            let v = u.inverse * vec3(p - bias, 1.);
            if abs(v.z) < 1e-8 {
                return vec4(0.);
            }
            q = v.xy / v.z + bias;
            let dx = (u.inverse[0].xy - u.inverse[0].z * v.xy / v.z) / v.z;
            let dy = (u.inverse[1].xy - u.inverse[1].z * v.xy / v.z) / v.z;
            let footprint = abs(dx) + abs(dy);
            if u.kind == TRANSFORM {
                let opacity = val(8) * 0.01;
                if val(12) == 2. {
                    return cubic(q) * opacity;
                }
                // Pure rotations and skews preserve AE's bilinear sampling.
                if min(abs(val(3)), select(abs(val(4)), abs(val(3)), val(11) > 0.)) < 100. {
                    let width = vec2(length(vec2(dx.x, dy.x)), length(vec2(dx.y, dy.y)));
                    return area(q, 0.992 * width) * opacity;
                }
                return linear(q, false) * opacity;
            }
            return area(q, footprint);
        }
        case TURBULENT: {
            q = turbulent(p);
        }
        case WAVE: {
            return wave_color(p);
        }
        case BULGE: {
            q = bulge(p);
        }
        case TWIRL: {
            q = radial(p);
            if all(q == p) {
                return linear(p, false);
            }
            let dx = radial(p + vec2(0.5, 0.)) - radial(p - vec2(0.5, 0.));
            let dy = radial(p + vec2(0., 0.5)) - radial(p - vec2(0., 0.5));
            return area(q, abs(dx) + abs(dy));
        }
        case SPHERIZE, RIPPLE: {
            q = radial(p);
        }
        case OPTICS: {
            q = radial(p);
            if val(2) == 0. && all(q > vec2(-1e6)) {
                let jac = abs(vec2(radial(p + vec2(0.5, 0.)).x - radial(p - vec2(0.5, 0.)).x,
                radial(p + vec2(0., 0.5)).y - radial(p - vec2(0., 0.5)).y));
                let footprint = min(3.2 * jac - 3., 1.8 * jac - 0.2);
                return area(q, footprint);
            }
        }
        case POLAR: {
            q = polar(p);
            if val(2) == 1. && q.x >= f32(u.src.z) - 0.5 && q.y >= f32(u.src.y) + 0.5 && q.y <= f32(u.src.w) - 0.5 {
                return linear(q, true);
            }
        }
        case OFFSET: {
            return mix(linear(p + center() - pt(1), true), linear(p, false), val(2));
        }
        case MIRROR: {
            let n = rot(vec2(1., 0.), radians(val(2)));
            q = p - 2. * max(0., dot(p - pt(1) - 0.5, n)) * n;
        }
        default: {
        }
    }
    return linear(q, false);
}
@compute @workgroup_size(8, 8, 1)
fn warp(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = vec2<i32>(id.xy);
    if any(t >= u.dst.zw - u.dst.xy) {
        return;
    }
    let p = vec2<f32>(t + u.dst.xy) + 0.5;
    var c = vec4(0.);
    var samples = 1;
    if u.kind == TURBULENT && val(14) == 2. && val(2) != 0. {
        samples = 2;
    }
    if u.kind == BULGE && val(6) == 2. && val(4) != 0. {
        let delta = abs(p - pt(3));
        let radius = vec2(val(1), val(2)) * u.scale;
        if all(radius > vec2(0.)) && length(max(delta - 0.5, vec2(0.)) / radius) < 1. {
            samples = 2;
        }
    }
    if samples == 1 {
        c = color(p);
    } else {
        for (var y = 0; y < samples; y++) {
            for (var x = 0; x < samples; x++) {
                c += color(p + (vec2<f32>(f32(x), f32(y)) + 0.5) / f32(samples) - 0.5);
            }
        }
        c /= f32(samples * samples);
    }
    if u.hdr == 0u {
        let alpha = clamp(c.a, 0., 1.);
        c = vec4(clamp(c.rgb, vec3(0.), vec3(alpha)), alpha);
    }
    textureStore(dst, t, c);
}
