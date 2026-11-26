struct Params {
    input: FxImage,
    roi: vec4i,
    columns: u32,
    style: u32,
    flags: u32,
    opacity: f32,
    gradient: vec4f,
}

struct Mark {
    points: vec4f,
    color_radius: vec4f,
    dynamics: vec4f,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;
@group(0) @binding(3) var<storage, read> marks: array<Mark>;
@group(0) @binding(4) var<storage, read> bins: array<u32>;

fn segment_distance(rel: vec2f, delta: vec2f, inv_len_squared: f32) -> f32 {
    let t = clamp(dot(rel, delta) * inv_len_squared, 0.0, 1.0);
    return length(rel - t * delta);
}

fn scribble_samples(mark: Mark, point: vec2f, already_covered: u32) -> u32 {
    let delta = mark.points.zw - mark.points.xy;
    let rel = point - mark.points.xy;
    let inv_len_squared = 1.0 / max(dot(delta, delta), 1e-9);
    let dist = segment_distance(rel, delta, inv_len_squared);
    let radius = mark.color_radius.w;
    // The four by four sample grid has maximum offset length sqrt(2) * 3/8.
    if dist <= radius - 0.531 { return 65535u; }
    if dist > radius + 0.531 { return already_covered; }
    var covered = already_covered;
    for (var i = 0u; i < 16u; i++) {
        let bit = 1u << i;
        if (covered & bit) != 0u { continue; }
        let offset = vec2f(f32(i % 4u), f32(i / 4u)) * 0.25 - 0.375;
        if segment_distance(rel + offset, delta, inv_len_squared) <= radius {
            covered |= bit;
        }
    }
    return covered;
}

fn brush_sample(p: vec2f, radius: f32, inner: f32) -> f32 {
    let distance = length(p);
    let x = max(distance - inner, 0.0) / (radius - inner);
    return select(exp(-6.25 * x * x), 0.0, distance >= radius);
}

fn brush_coverage(rel: vec2f, radius: f32, hardness: f32) -> f32 {
    // The dab is a pixel-centered brush stamp, translated with bilinear sampling.
    let cell = floor(rel + 0.5) - 0.5;
    let f = rel - cell;
    let inner = max(radius - 1.0, 0.0) * hardness;
    let a = brush_sample(cell, radius, inner);
    if all(f == vec2f(0.0)) { return a; }
    return mix(
        mix(a, brush_sample(cell + vec2f(1.0, 0.0), radius, inner), f.x),
        mix(brush_sample(cell + vec2f(0.0, 1.0), radius, inner),
            brush_sample(cell + vec2f(1.0, 1.0), radius, inner), f.x),
        f.y,
    );
}

fn coverage(mark: Mark, point: vec2f) -> f32 {
    let rel = point - mark.points.xy;
    let radius = mark.color_radius.w;
    let hardness = mark.dynamics.z;
    if (u.flags & 3u) <= 1u {
        return brush_coverage(rel, radius, hardness) * mark.dynamics.x;
    }
    return dash_coverage(point, mark.points.xy, mark.points.zw, radius, hardness,
                         mark.dynamics.xy, mark.dynamics.w, u.gradient);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= vec2u(u.roi.zw - u.roi.xy)) { return; }
    let point = vec2f(u.roi.xy + vec2i(id.xy)) + 0.5;
    let tile = id.xy / 32u;
    let bin = (tile.y * u.columns + tile.x) * 2u;
    let offset = bins[bin];
    let count = bins[bin + 1u];
    var paint = vec4f(0.0);
    var covered = 0u;
    for (var i = 0u; i < count; i++) {
        let mark = marks[bins[offset + i]];
        if (u.flags & 3u) == 2u {
            covered = scribble_samples(mark, point, covered);
            let opacity = min(f32(countOneBits(covered)) * (16.0 / 255.0), 1.0);
            paint = vec4f(mark.color_radius.rgb * opacity, opacity);
            if covered == 65535u { break; }
            continue;
        }
        let a = coverage(mark, point);
        let sample = vec4f(mark.color_radius.rgb * a, a);
        if (u.flags & 3u) == 0u {
            paint = sample + paint * (1.0 - a);
            if paint.a == 1.0 { break; }
        } else if a >= paint.a {
            paint = sample;
        }
    }
    paint *= u.opacity;
    let base = fx_sample(src, u.input, point, false);
    var result: vec4f;
    switch u.style {
        case 1u: { result = paint + base * (1.0 - paint.a); }
        case 3u: { result = base * paint.a; }
        case 4u: { result = base + paint * (1.0 - base.a); }
        // The Paint effect's Eraser.
        case 5u: { result = base * (1.0 - paint.a); }
        default: { result = paint; }
    }
    if (u.flags & 0x80000000u) == 0u {
        result = clamp(result, vec4f(0.0), vec4f(1.0));
    }
    textureStore(dst, vec2i(id.xy), result);
}
