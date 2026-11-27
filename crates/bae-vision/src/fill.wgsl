// Jump flooding carries the nearest unmasked texel in two byte pairs.
// Zero encodes a missing donor. Integer bytes are exact in both working formats.
struct Control {
    size: vec2i, jump: i32, method: u32,
    expansion: i32, lighting: f32, pad: vec2u,
}
@group(0) @binding(0) var<uniform> u: Control;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var field: texture_2d<f32>;
@group(0) @binding(3) var previous: texture_2d<f32>;
@group(0) @binding(4) var following: texture_2d<f32>;
@group(0) @binding(5) var dst: texture_storage_2d<WORKING_FORMAT, write>;

fn bounded(p: vec2i) -> vec2i {
    return clamp(p, vec2i(0), u.size - 1);
}

fn pack_seed(p: vec2i) -> vec4f {
    let q = vec2u(p + 1);
    return vec4f(vec4u(q & vec2u(255), q >> vec2u(8)));
}

fn nearest_seed(p: vec2i) -> vec2i {
    let bytes = vec4u(textureLoad(field, bounded(p), 0));
    return vec2i(bytes.xy + (bytes.zw << vec2u(8))) - 1;
}

// Two separable minimum filters expand the hole before choosing donors.
@compute @workgroup_size(8, 8)
fn horizontal(@builtin(global_invocation_id) id: vec3u) {
    let p = vec2i(id.xy);
    if any(p >= u.size) { return; }
    var alpha = 1.;
    for (var x = -u.expansion; x <= u.expansion; x++) {
        alpha = min(alpha, textureLoad(src, bounded(p + vec2i(x, 0)), 0).a);
    }
    textureStore(dst, p, vec4f(alpha));
}

@compute @workgroup_size(8, 8)
fn seed(@builtin(global_invocation_id) id: vec3u) {
    let p = vec2i(id.xy);
    if any(p >= u.size) { return; }
    var alpha = 1.;
    for (var y = -u.expansion; y <= u.expansion; y++) {
        alpha = min(alpha, textureLoad(field, bounded(p + vec2i(0, y)), 0).a);
    }
    textureStore(dst, p, select(vec4f(0), pack_seed(p), alpha >= 0.999));
}

@compute @workgroup_size(8, 8)
fn propagate(@builtin(global_invocation_id) id: vec3u) {
    let p = vec2i(id.xy);
    if any(p >= u.size) { return; }
    var best = nearest_seed(p);
    var delta = vec2f(best - p);
    var closest = select(1e20, dot(delta, delta), best.x >= 0);
    if closest > 0. {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let candidate = nearest_seed(p + vec2i(x, y) * u.jump);
                delta = vec2f(candidate - p);
                let squared = dot(delta, delta);
                if candidate.x >= 0 && squared < closest {
                    best = candidate;
                    closest = squared;
                }
            }
        }
    }
    textureStore(dst, p, pack_seed(best));
}

fn solid(p: vec2i) -> bool {
    return all(p == bounded(p)) && all(nearest_seed(p) == p);
}

fn straight(tex: texture_2d<f32>, p: vec2i) -> vec3f {
    let color = textureLoad(tex, bounded(p), 0);
    return color.rgb / max(color.a, 0.00001);
}

// Estimate translation and exposure from known pixels beside the missing patch.
// Alpha of the return value holds the mean squared colour residual.
fn temporal(tex: texture_2d<f32>, p: vec2i, boundary: vec2i) -> vec4f {
    let taps = array<vec2i, 5>(vec2i(0), vec2i(3,0), vec2i(-3,0), vec2i(0,3), vec2i(0,-3));
    var best = vec4f(0, 0, 0, 1e10);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2i(x, y) * 3;
            let donor = p + offset;
            if any(donor != bounded(donor)) || textureLoad(tex, donor, 0).a < 0.999 { continue; }
            var error = 0.;
            var count = 0.;
            var delta = vec3f(0);
            for (var i = 0; i < 5; i++) {
                let q = bounded(boundary + taps[i]);
                if !solid(q) || textureLoad(tex, bounded(q + offset), 0).a < 0.999 { continue; }
                let diff = straight(src, q) - straight(tex, q + offset);
                delta += diff;
                error += dot(diff, diff);
                count += 1.;
            }
            if count > 0. {
                let residual = error / count + 0.0001 * length(vec2f(offset));
                if residual < best.a {
                    let corrected = straight(tex, donor) + u.lighting * delta / count;
                    best = vec4f(max(vec3f(0), corrected), residual);
                }
            }
        }
    }
    return best;
}

// Inverse-distance interpolation from visible boundaries on eight rays.
fn edge_blend(p: vec2i, boundary: vec2i) -> vec3f {
    let radius = max(1., distance(vec2f(boundary), vec2f(p)));
    var sum = straight(src, boundary) / radius;
    var weights = 1. / radius;
    for (var i = 0; i < 8; i++) {
        let angle = f32(i) * 0.78539816;
        let dir = vec2f(cos(angle), sin(angle));
        var step = radius;
        for (var k = 0; k < 12; k++) {
            let q = p + vec2i(round(dir * step));
            if any(q != bounded(q)) { break; }
            if solid(q) {
                let edge = nearest_seed(p + vec2i(round(dir * max(0., step / 1.6))));
                let weight = 1. / max(1., distance(vec2f(edge), vec2f(p)));
                sum += straight(src, edge) * weight;
                weights += weight;
                break;
            }
            step = step * 1.6 + 1.;
        }
    }
    return sum / weights;
}

@compute @workgroup_size(8, 8)
fn fill(@builtin(global_invocation_id) id: vec3u) {
    let p = vec2i(id.xy);
    if any(p >= u.size) { return; }
    let boundary = nearest_seed(p);
    if all(boundary == p) {
        textureStore(dst, p, vec4f(0));
        return;
    }
    if boundary.x < 0 {
        var color = vec4f(0);
        if u.method == 0u {
            let a = textureLoad(previous, p, 0);
            let b = textureLoad(following, p, 0);
            if a.a >= 0.999 { color = a; } else if b.a >= 0.999 { color = b; }
        }
        textureStore(dst, p, color);
        return;
    }
    var rgb = straight(src, boundary);
    if u.method == 2u {
        rgb = edge_blend(p, boundary);
    } else {
        // Reflect the visible patch across the boundary to preserve local texture.
        let reflected = bounded(boundary + (boundary - p));
        if solid(reflected) { rgb = straight(src, reflected); }
        if u.method == 0u {
            let before = temporal(previous, p, boundary);
            let after = temporal(following, p, boundary);
            let best = select(after, before, before.a <= after.a);
            if best.a < 0.3 { rgb = best.rgb; }
        }
    }
    let retain = select(textureLoad(src, p, 0).a, 0., u.expansion > 0);
    textureStore(dst, p, vec4f(mix(rgb, straight(src, p), retain), 1.));
}
