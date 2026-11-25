// Layer coordinates denote pixel edges; texel (0, 0) is centred at (0.5, 0.5).
struct Uniforms {
    src: vec4<i32>, dst: vec4<i32>, layer: vec4<f32>,
    bulge_aspect_error: f32, scale: f32, time: f32, hdr: u32,
    params: array<vec4<f32>, 15>, inverse: mat3x3<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;
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
// Integrate the overlap in source space, retaining transparent coverage outside
// the texture in the denominator. A one-pixel footprint reduces to bilinear.
fn box_sample(p: vec2<f32>, footprint: vec2<f32>) -> vec4<f32> {
    let w = max(footprint, vec2(1.));
    if all(w < vec2(1.001)) { return linear(p, false); }
    let lo = max(p - 0.5 * w, vec2<f32>(u.src.xy));
    let hi = min(p + 0.5 * w, vec2<f32>(u.src.zw));
    if any(hi <= lo) { return vec4(0.); }
    var c = vec4(0.);
    for (var y = i32(floor(lo.y)); y < i32(ceil(hi.y)); y++) {
        let wy = min(hi.y, f32(y) + 1.) - max(lo.y, f32(y));
        for (var x = i32(floor(lo.x)); x < i32(ceil(hi.x)); x++) {
            let wx = min(hi.x, f32(x) + 1.) - max(lo.x, f32(x));
            c += textureLoad(src, vec2(x, y) - u.src.xy, 0) * wx * wy;
        }
    }
    return c / (w.x * w.y);
}

fn area(p: vec2<f32>, footprint: vec2<f32>) -> vec4<f32> {
    let w = max(footprint, vec2(1.));
    if all(w <= vec2(12.)) { return box_sample(p, w); }
    // Bound the cost of near-singular corner pins and extreme twist angles.
    let lo = p - 0.5 * w;
    if any(p + 0.5 * w <= vec2<f32>(u.src.xy)) || any(lo >= vec2<f32>(u.src.zw)) {
        return vec4(0.);
    }
    var c = vec4(0.);
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            c += linear(lo + (vec2<f32>(f32(x), f32(y)) + 0.5) * w * 0.25, false);
        }
    }
    return c / 16.;
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

fn noise_row(i: vec3<i32>, blend: f32, evolution: bool) -> f32 {
    var base = i;
    if evolution {
        if val(8) > 0. {
            let period = 2 * i32(val(9));
            base.z = ((base.z % period) + period) % period;
        }
        base.z += 13 * i32(val(10));
    }
    return mix(lattice(base), lattice(base + vec3(1, 0, 0)), blend);
}

fn noise(p: vec3<f32>, smoother: bool, evolution: bool) -> f32 {
    let i = vec3<i32>(floor(p));
    let t = fract(p);
    let s = select(t * t * (3. - 2. * t), t * t * t * (t * (6. * t - 15.) + 10.), smoother);
    let z0 = mix(noise_row(i, s.x, evolution), noise_row(i + vec3(0, 1, 0), s.x, evolution), s.y);
    let z1 = mix(noise_row(i + vec3(0, 0, 1), s.x, evolution), noise_row(i + vec3(0, 1, 1), s.x, evolution), s.y);
    return mix(z0, z1, s.z);
}

// The AE coordinate probes show that vertical displacement is constant down
// each column and horizontal displacement across each row. Bulge and Twist
// use the same scalar field, with their vectors a quarter-turn apart.
fn turbulence_field(p: vec3<f32>, mode: i32) -> vec2<f32> {
    let smoother = mode >= 5 && mode <= 7;
    let a = vec3(14.31, 5.19, 2.07);
    let b = vec3(3.79, 41.83, 7.17);
    if mode >= 9 {
        let h = noise(vec3(0., p.y, p.z) + b, false, true);
        let v = noise(vec3(p.x, 0., p.z) + a, false, true);
        return 0.65 * vec2(select(h, 0., mode == 9), select(v, 0., mode == 10));
    }
    if mode == 2 || mode == 3 || mode == 6 || mode == 7 {
        let step = vec2(0.35, 0.);
        let dx = noise(p + a + step.xyy, smoother, true) - noise(p + a - step.xyy, smoother, true);
        let dy = noise(p + a + step.yxy, smoother, true) - noise(p + a - step.yxy, smoother, true);
        return 0.65 * select(vec2(dx, dy), vec2(-dy, dx), mode == 3 || mode == 7);
    }
    return 0.65 * vec2(noise(p + a, smoother, true), noise(p + b, smoother, true));
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
    if edges == 3 {
        weight = min(lo, hi);
    } else if edges == 4 {
        weight.y = min(lo.y, hi.y);
    } else if edges == 5 {
        weight.x = min(lo.x, hi.x);
    } else if edges == 6 {
        weight.x = lo.x;
    } else if edges == 7 {
        weight.x = hi.x;
    } else if edges == 8 {
        weight.y = lo.y;
    } else if edges == 9 {
        weight.y = hi.y;
    }
    weight = 0.5 - 0.5 * cos(weight * TAU * 0.5);
    if locked {
        weight = vec2(min(weight.x, weight.y));
    }
    return p + d * (val(2) * u.scale / norm) * weight;
}

fn waveform(x: f32, mode: i32) -> f32 {
    // The seven periodic shapes share a 2048-step phase table in AE 26.
    // A division can round just below an exact table boundary on Metal or D3D.
    let t = fract(floor(x * 2048. + 0.001) / 2048.);
    let tri = 4. * abs(t - 0.5) - 1.;
    let half_arc = 2. * fract(2. * t) - 1.;
    let circle = sqrt(max(0., 1. - half_arc * half_arc));
    if mode == 2 {
        return select(1., -1., t < 0.5);
    }
    if mode == 3 {
        return tri;
    }
    if mode == 4 {
        return 1. - 2. * t;
    }
    if mode == 5 {
        return select(circle, -circle, t < 0.5);
    }
    if mode == 6 {
        let arc = 2. * t - 1.;
        return 2. * sqrt(max(0., 1. - arc * arc)) - 1.;
    }
    if mode == 7 {
        return select(1. - circle, circle - 1., t < 0.5);
    }
    if mode == 8 {
        return lattice(vec3(i32(floor(t * val(3) * 2.)), 0, 0));
    }
    if mode == 9 {
        return 0.35 * noise(vec3(t * val(3) * 0.75, 0.5, 0.25), true, false);
    }
    return -sin(TAU * t);
}

fn wave_displacement(p: vec2<f32>) -> vec2<f32> {
    let a = radians(val(4));
    let axis = vec2(sin(a), -cos(a));
    let cardinal = round(axis);
    let dir = select(axis, cardinal, abs(axis - cardinal) < vec2(1e-6));
    let perp = vec2(-dir.y, dir.x);
    let xy = p - u.layer.xy;
    let phase = dot(xy, dir) / (2. * val(3) * u.scale) - val(5) * u.time + val(7) / 1440.;
    let n = clamp((xy - 0.75) / (u.layer.zw - vec2(0.5)), vec2(0.), vec2(1.));
    let e = min(4. * min(n, 1. - n), vec2(1.));
    var weight = 1.;
    let pin = i32(val(6));
    if pin == 2 {
        weight = e.x * e.y;
    } else if pin == 3 {
        weight = 1. - min(e.x, e.y);
    } else if pin == 5 {
        weight = min(4. * n.x, 1.);
    } else if pin == 6 {
        weight = min(4. * n.y, 1.);
    } else if pin == 7 {
        weight = min(4. * (1. - n.x), 1.);
    } else if pin == 8 {
        weight = min(4. * (1. - n.y), 1.);
    } else if pin == 10 {
        weight = e.y;
    } else if pin == 11 {
        weight = e.x;
    }
    return perp * val(2) * u.scale * waveform(phase, i32(val(1))) * weight;
}

// Measured on the pin, taper and wave amplitude ramps.
fn bell_falloff(t: f32) -> f32 {
    return max(0., (exp(-2.3781 * t * t) - 0.09255) / 0.90745);
}

// Coordinate grids put the radius steps at 1/16 px. On Pythagorean
// diagonals, rounding the slope and its length separately matters.
fn radial_distance(delta: vec2<f32>) -> f32 {
    let axes = abs(delta);
    let major = max(axes.x, axes.y);
    if major == 0. { return 0.; }
    let slope = floor(256. * min(axes.x, axes.y) / major + 1e-5) / 256.;
    let length_factor = round(16384. * sqrt(1. + slope * slope)) / 16384.;
    return floor(16. * major * length_factor) / 16.;
}

// The comb's stripe width measures the square area sampler independently
// of the angular falloff measured by the coordinate ramps.
fn twirl_profile(t: f32) -> vec2<f32> {
    let d = t - 0.50088;
    return max(vec2(0.), (exp(-vec2(3.106 * t * t, 9.477 * d * d)) - 0.09255) / 0.90745);
}

fn spherize(p: vec2<f32>) -> vec2<f32> {
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

fn ripple(p: vec2<f32>) -> vec2<f32> {
    let d = p - pt(2) - 0.5;
    let r = radial_distance(d);
    let radius = length(u.layer.zw) * val(1) * 0.01;
    if r == 0. || r >= radius { return p; }
    // The two neighbouring radii carry displacement in asymmetric mode
    // and displacement per unit radius in symmetric mode.
    let samples = floor(r) + vec2(0., 1.);
    let span = ceil(radius) + 2.;
    let envelope = vec2(bell_falloff(samples.x / span), bell_falloff(samples.y / span));
    let phase = 0.5 * samples / (val(5) * u.scale) - val(4) * u.time + val(7) / 360.;
    var wave = cos(TAU * phase);
    let symmetric = val(3) == 2.;
    if symmetric {
        wave /= max(samples, vec2(1.));
        if samples.x == 0. { wave.x = 0.; }
    }
    let amplitude = mix(envelope.x, envelope.y, fract(r)) * mix(wave.x, wave.y, fract(r));
    let height = 0.25 * val(6) * u.scale * amplitude;
    return p + select(vec2(1.), d, symmetric) * height;
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
            let displacement = floor(wave_displacement(probe) * f32(count) + 0.0001) / f32(count);
            c += texel(vec2<i32>(floor(sub + displacement)), false);
        }
    }
    return c / f32(count * count);
}

fn projective(p: vec2<f32>, corner: bool) -> vec4<f32> {
    let bias = select(0., 0.5, corner);
    let v = u.inverse * vec3(p - bias, 1.);
    if abs(v.z) < 1e-8 {
        return vec4(0.);
    }
    let q = v.xy / v.z + bias;
    let dx = (u.inverse[0].xy - u.inverse[0].z * v.xy / v.z) / v.z;
    let dy = (u.inverse[1].xy - u.inverse[1].z * v.xy / v.z) / v.z;
    let footprint = abs(dx) + abs(dy);
    if !corner {
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

fn transform_color(p: vec2<f32>) -> vec4<f32> {
    return projective(p, false);
}

fn corner_color(p: vec2<f32>) -> vec4<f32> {
    if val(0) != 0. && val(6) == 0. {
        // Non-perspective pins interpolate all four corners bilinearly.
        let a = pt(2);
        let dx = pt(3) - a;
        let dy = pt(4) - a;
        let twist = pt(5) - pt(3) - pt(4) + a;
        let low = vec2(val(10), val(9)) * 0.01;
        let span = 1. + low + vec2(val(11), val(12)) * 0.01;
        var q = (p - u.layer.xy) / u.layer.zw;
        if val(7) != 0. {
            q = q * span - low;
            return linear(a + dx*q.x + dy*q.y + twist*q.x*q.y, false);
        }
        for (var i=0; i<8; i++) {
            let err = p - (a + dx*q.x + dy*q.y + twist*q.x*q.y);
            let x = dx + twist*q.y;
            let y = dy + twist*q.x;
            let det = x.x*y.y - x.y*y.x;
            if abs(det)<1e-7 || dot(err,err)<1e-6 {break;}
            q += vec2(err.x*y.y - err.y*y.x, x.x*err.y - x.y*err.x)/det;
        }
        return linear(u.layer.xy + u.layer.zw*(q+low)/span,false);
    }
    return projective(p, true);
}

fn twirl_color(p: vec2<f32>) -> vec4<f32> {
    let c = pt(3) + 0.5;
    let d = p - c;
    let r = radial_distance(d);
    let radius = length(u.layer.zw) * val(2) * 0.01;
    if r >= radius || val(1) == 0. { return linear(p, false); }
    let lo = floor(2. * r) * 0.5;
    let span = ceil(radius);
    let profile = mix(twirl_profile(lo / span), twirl_profile((lo + 0.5) / span), r - lo);
    let angle = radians(val(1));
    return area(c + rot(d, -angle * profile.x), vec2(1. + abs(angle) * profile.y));
}

fn optics_color(p: vec2<f32>) -> vec4<f32> {
    let size = u.layer.zw;
    let d = p - pt(4);
    let r = length(d);
    let a = radians(val(1)) * 0.5;
    if a < 0.00001 || r == 0. {
        return linear(p, false);
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
        let edge_scale = inverseSqrt(1. + corner * corner);
        let optimal = 2. / (1. + edge_scale);
        zoom2 = select(optimal * optimal, edge_scale * edge_scale, reverse);
    }
    let denom = zoom2 + select(-rr * rr, rr * rr, reverse);
    if denom <= 0. {
        return vec4(0.);
    }
    let magnification = inverseSqrt(denom);
    let q = pt(4) + d * magnification;
    if reverse { return linear(q, false); }
    // Measured stripe coverage grows with the change in radial magnification
    // from the centre. Project this width onto the two source axes.
    let width = 2. * TAU * (magnification - inverseSqrt(zoom2)) * abs(d) / r;
    return box_sample(q, width);
}

fn polar_color(p: vec2<f32>) -> vec4<f32> {
    let q = polar(p);
    if val(2) == 1. && q.x >= f32(u.src.z) - 0.5 && q.y >= f32(u.src.y) + 0.5 && q.y <= f32(u.src.w) - 0.5 {
        return linear(q, true);
    }
    return linear(q, false);
}

fn offset_color(p: vec2<f32>) -> vec4<f32> {
    return mix(linear(p + center() - pt(1), true), linear(p, false), val(2));
}

fn mirror_color(p: vec2<f32>) -> vec4<f32> {
    let n = rot(vec2(1., 0.), radians(val(2)));
    let q = p - 2. * max(0., dot(p - pt(1) - 0.5, n)) * n;
    return linear(q, false);
}

fn spherize_color(p: vec2<f32>) -> vec4<f32> {
    return linear(spherize(p), false);
}

fn ripple_color(p: vec2<f32>) -> vec4<f32> {
    return linear(ripple(p), false);
}

fn bulge_color(p: vec2<f32>) -> vec4<f32> {
    let radii = vec2(val(1), val(2)) * u.scale;
    let d = p - (pt(3) + 0.5);
    if min(radii.x, radii.y) == 0. || val(4) == 0. { return linear(p, false); }
    let r = length(d / radii);
    var inside = r < 1.;
    if abs(r - 1.) < 2e-7 {
        // High AA has a hard cutoff. At an exact lattice tie all coordinates
        // below are representable, so the scaled aspect error preserves the
        // last-bit decision of the original ellipse without GPU doubles.
        let square = max(0., radii.x * radii.x - d.x * d.x);
        let estimate = sqrt(square);
        let vertical = estimate + fma(-estimate, estimate, square) / max(2. * estimate, 1e-20);
        let y = abs(d.y);
        if y > 0. && y * radii.x == vertical * radii.y {
            let step = vertical - bitcast<f32>(bitcast<u32>(vertical) - 1u);
            let ulps = round(-y * u.bulge_aspect_error / step);
            let scaled_y = bitcast<f32>(bitcast<u32>(vertical) - u32(max(0., ulps)));
            let radius2 = radii.x * radii.x;
            let prev_x = bitcast<f32>(bitcast<u32>(radii.x) - 1u);
            let prev_r2 = bitcast<f32>(bitcast<u32>(radius2) - 1u);
            let cutoff = select(radius2, prev_r2, radius2 - prev_r2 < radii.x * (radii.x - prev_x));
            inside = fma(d.x, d.x, scaled_y * scaled_y) < cutoff;
        } else {
            inside = y * radii.x < vertical * radii.y;
        }
    }
    // The angular lookup advances in steps of 1/400 radian.
    let angle = floor(r * (100. * TAU)) / 400.;
    if !inside || angle == 0. { return linear(p, false); }
    let ratio = r / sin(angle);
    var delta = val(4) * (ratio - 1.);
    if val(5) > 0. {
        let distance = (1. - r) * max(radii.x, radii.y);
        delta *= 1. - bell_falloff(distance / (val(5) * u.scale));
    }
    if val(7) > 0. {
        let uv = (p - 0.5 - u.layer.xy) / u.layer.zw;
        let edge = 4. * min(uv, 1. - uv);
        delta *= bell_falloff(1. - clamp(min(edge.x, edge.y), 0., 1.));
    }
    let q = p + delta * d;
    let width = select(1., 2. * val(4) * r * r * ratio, val(6) == 2.);
    return area(q, vec2(width));
}

fn turbulent_color(p: vec2<f32>) -> vec4<f32> {
    let high = val(14) == 2. && val(2) != 0.;
    if !high {
        return linear(turbulent(p), false);
    }
    var c = vec4(0.);
    for (var j = 0u; j < 4u; j++) {
        let offset = vec2<f32>(f32(j % 2u), f32(j / 2u)) * 0.5 - 0.25;
        c += linear(turbulent(p + offset), false);
    }
    return c * 0.25;
}

@compute @workgroup_size(8, 8, 1)
fn warp(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = vec2<i32>(id.xy);
    if any(t >= u.dst.zw - u.dst.xy) {
        return;
    }
    let p = vec2<f32>(t + u.dst.xy) + 0.5;
    var c = SAMPLE_COLOR(p);
    if u.hdr == 0u {
        let alpha = clamp(c.a, 0., 1.);
        c = vec4(clamp(c.rgb, vec3(0.), vec3(alpha)), alpha);
    }
    textureStore(dst, t, c);
}
