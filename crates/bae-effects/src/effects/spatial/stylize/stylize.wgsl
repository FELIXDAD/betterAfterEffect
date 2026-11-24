const LUMA = vec3<f32>(0.299, 0.587, 0.114);

fn clamp_to_rect(p: vec2<f32>, rect: vec4<i32>) -> vec2<f32> {
    return clamp(p, vec2<f32>(rect.xy), vec2<f32>(rect.zw - 1));
}

@compute @workgroup_size(8, 8)
fn emboss(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    let xy = vec2<f32>(p);
    let delta = f.arg[0].xy;
    let front = bilinear(src, f.src, clamp_to_rect(xy + delta, f.src));
    let back = bilinear(src, f.src, clamp_to_rect(xy - delta, f.src));
    let base = straight(c);
    let relief = (front.rgb - back.rgb) * f.arg[0].z;
    let tone = select(vec3<f32>(128.0 / 255.0), base, f.arg[1].x != 0.0);
    let colored = saturate(tone + relief);
    store(p, vec4<f32>(mix(colored, base, f.arg[0].w) * c.a, c.a));
}

@compute @workgroup_size(8, 8)
fn cartoon_smooth(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let guide = load(src, f.src, p);
    if guide.a == 0.0 {
        store(p, guide);
        return;
    }
    let range = max(0.001, f.arg[0].w);
    let radius = f.arg[0].z;
    let taps = min(i32(ceil(radius)), 8);
    let step = radius / f32(taps);
    let axis = f.arg[0].xy;
    let integral = fract(step) == 0.0;
    var sum = load(aux, f.aux, p);
    var norm = 1.0;
    for (var i = -taps; i <= taps; i++) {
        if i == 0 { continue; }
        let q = clamp_to_rect(vec2<f32>(p) + axis * (step * f32(i)), f.src);
        var sample_guide: vec4<f32>;
        var sample_pixel: vec4<f32>;
        if integral {
            sample_guide = load(src, f.src, vec2<i32>(q));
            sample_pixel = load(aux, f.aux, vec2<i32>(q));
        } else {
            sample_guide = bilinear(src, f.src, q);
            sample_pixel = bilinear(aux, f.aux, q);
        }
        let delta = straight(sample_guide) - straight(guide);
        let spatial = f32(i) / f32(taps);
        let weight = exp(-2.0 * spatial * spatial - dot(delta, delta) / (range * range)) * sample_guide.a;
        sum += weight * sample_pixel;
        norm += weight;
    }
    store(p, sum / norm);
}

fn cartoon_signal(p: vec2<f32>, fallback: f32) -> f32 {
    let c = bilinear(aux, f.aux, clamp_to_rect(p, f.aux));
    return select(fallback, dot(straight(c), LUMA), c.a > 0.0);
}

@compute @workgroup_size(8, 8)
fn cartoon_output(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    if c.a == 0.0 {
        store(p, c);
        return;
    }
    let xy = vec2<f32>(p);
    let mode = f.arg[0].x;
    let width = max(0.5, f.arg[1].x) * 2.5;
    let dx = vec2<f32>(width, 0.0);
    let dy = vec2<f32>(0.0, width);
    let source_tone = dot(straight(c), LUMA);
    let center = cartoon_signal(xy, source_tone);
    let left = cartoon_signal(xy - dx, center);
    let right = cartoon_signal(xy + dx, center);
    let top = cartoon_signal(xy - dy, center);
    let bottom = cartoon_signal(xy + dy, center);
    let gradient = vec2<f32>(right - left, bottom - top);
    let detail = max(max(left, right), max(top, bottom)) - center;
    let softness = f.arg[1].y;
    let cutoff = 0.20 / f.arg[0].w;
    let edge = smoothstep(cutoff * (1.0 - softness * 0.35), cutoff * (1.0 + softness), detail);
    let contrast = 0.5 + f.arg[2].y;
    let stroke = min(0.995, saturate((edge * contrast + detail * f.arg[2].x * 0.1)
        * min(1.0, (1.0 - softness) * 2.5))) * f.arg[1].z;
    if mode == 2.0 {
        store(p, vec4<f32>(vec3<f32>(c.a * (1.0 - stroke)), c.a));
        return;
    }

    let laplacian = left + right + top + bottom - 4.0 * center;
    let warp = -sign(laplacian) * gradient * f.arg[1].w * width;
    let fill_sample = bilinear(aux, f.aux, clamp_to_rect(xy + warp, f.aux));
    let rgb = select(straight(c), straight(fill_sample), fill_sample.a > 0.0);
    let tone = dot(rgb, LUMA);
    let steps = clamp(f.arg[0].y, 2.0, 30.0) - 1.0;
    let brightness = pow(max(0.0, tone), 0.625);
    let bins = brightness * steps;
    let softness_fill = clamp(f.arg[0].z, 0.001, 1.0);
    let quantized = (floor(bins) + smoothstep(0.5 - softness_fill * 0.5, 0.5 + softness_fill * 0.5, fract(bins))) / steps;
    let shaded = pow(quantized, 1.6);
    let weight = 1.0 - smoothstep(0.0, 0.70, softness_fill);
    let shade = clamp(mix(shaded, tone, 1.0 - weight) / max(1e-6, tone), 0.0, 16.0);
    store(p, vec4<f32>(rgb * shade * c.a * select(1.0, 1.0 - stroke, mode == 3.0), c.a));
}

fn noise_slice(p: vec2<f32>, seed: u32) -> f32 {
    let i = vec2<i32>(floor(p));
    let t = fract(p);
    let s = t * t * (3.0 - 2.0 * t);
    let a = random(i, seed);
    let b = random(i + vec2<i32>(1, 0), seed);
    let c = random(i + vec2<i32>(0, 1), seed);
    let d = random(i + 1, seed);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn rough_fractal(p: vec2<f32>) -> f32 {
    // Four lattice seeds per revolution also let a one-revolution cycle evolve.
    let evolution = f.arg[2].y * 4.0;
    let cycle = f.arg[2].z * 4.0;
    let z0 = floor(evolution);
    var z1 = z0 + 1.0;
    if cycle > 0.0 && z1 >= cycle { z1 = 0.0; }
    let t = fract(evolution);
    let fade = t * t * (3.0 - 2.0 * t);
    let seed = u32(f.arg[2].w);
    let seed_a = seed ^ (bitcast<u32>(i32(z0)) * 0x9e3779b9u);
    let seed_b = seed ^ (bitcast<u32>(i32(z1)) * 0x9e3779b9u);
    let count = clamp(f.arg[2].x, 1.0, 10.0);
    var sum = 0.0;
    var weight = 0.0;
    var amp = 1.0;
    var q = p;
    for (var i = 0; i < i32(ceil(count)); i++) {
        let w = amp * min(1.0, count - f32(i));
        sum += w * mix(noise_slice(q, seed_a), noise_slice(q, seed_b), fade);
        weight += w;
        q = q * 2.0 + vec2<f32>(9.13, 4.42);
        amp *= 0.5;
    }
    return sum / weight;
}

@compute @workgroup_size(8, 8)
fn rough_output(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    let blur = load(aux, f.aux, p);
    if blur.a == 0.0 {
        store(p, c);
        return;
    }
    let sharpness = sqrt(0.1 + 0.9 * max(0.0, f.arg[0].y));
    let influence = f.arg[0].z;
    let mode = i32(f.arg[0].w);
    var noise = 0.5;
    if influence > 0.0 && blur.a < 1.0 {
        let q = (vec2<f32>(p) + 0.5 - f.arg[1].xy) / f.arg[1].zw;
        noise = rough_fractal(q);
    }
    let modulation = (noise - 0.5) * influence;
    let strength = select(0.70, 1.2, mode == 4);
    let contour = blur.a + strength * modulation;
    let edge = 0.5 + (contour - 0.5368) * (4.99 * sharpness);
    var coverage = smoothstep(0.0, 1.0, edge);
    var rgb = straight(select(blur, c, c.a > 0.0));
    if mode == 2 {
        let rim = 1.0 - smoothstep(0.59, 0.77, contour);
        rgb = mix(rgb, f.arg[3].rgb, rim);
    }
    if mode == 3 {
        coverage = c.a * saturate(0.76 + (blur.a - abs(modulation) - 0.53) * (4.0 * sharpness));
    }
    if mode == 5 || mode == 6 {
        let strip = smoothstep(0.63, 0.93, blur.a) * (1.0 - smoothstep(0.97, 1.0, blur.a));
        let pits = influence * strip * smoothstep(0.42, 0.88, noise);
        coverage = c.a * (1.0 - pits * 0.85);
        if mode == 6 { rgb = mix(rgb, f.arg[3].rgb, pits); }
    }
    if mode == 7 || mode == 8 {
        let ink = 1.0 - 0.953 * smoothstep(0.64, 0.99, contour);
        coverage = c.a;
        if mode == 7 { coverage *= ink; }
        if mode == 8 { rgb = mix(f.arg[3].rgb, rgb, ink); }
    }
    store(p, vec4<f32>(rgb * coverage, coverage));
}

fn odd(n: f32) -> bool {
    return (i32(n) & 1) != 0;
}

@compute @workgroup_size(8, 8)
fn tile_collapse(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let x_axis = f.arg[0].x != 0.0;
    let origin = i32(f.arg[0].y);
    let size = i32(f.arg[0].z);
    let lo = max(origin, select(f.src.y, f.src.x, x_axis));
    let hi = min(origin + size, select(f.src.w, f.src.z, x_axis));
    var sum = vec4<f32>(0.0);
    for (var i = lo; i < hi; i++) {
        sum += load(src, f.src, select(vec2<i32>(p.x, i), vec2<i32>(i, p.y), x_axis));
    }
    store(p, sum / f32(size));
}

fn tile_radius(scale: f32) -> f32 {
    if scale == 0.0 || scale == 1.0 { return 0.5; }
    if scale == 0.5 { return 1.0; }
    return min(4.0, select(0.5, 0.9, scale < 0.75) / scale);
}

fn tile_texel(p: vec2<i32>, size: vec2<i32>, mirror: bool) -> vec2<i32> {
    if mirror {
        let q = ((p % (2 * size)) + 2 * size) % (2 * size);
        return min(q, 2 * size - 1 - q);
    }
    return ((p % size) + size) % size;
}

@compute @workgroup_size(8, 8)
fn motion_tile(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let xy = vec2<f32>(p);
    let size = f.arg[1].zw;
    let tile = f.arg[0].zw;
    let scale = f.arg[4].xy;
    var t = (xy + 0.5 - f.arg[0].xy) / max(tile, vec2<f32>(1e-6)) + 0.5;
    t = select(t, vec2<f32>(0.5), scale == vec2<f32>(0.0));
    let phase = f.arg[2].x;
    if f.arg[2].z != 0.0 {
        if odd(floor(t.y)) { t.x -= phase; }
    } else {
        if odd(floor(t.x)) { t.y -= phase; }
    }
    let mirror = f.arg[2].y != 0.0;
    var uv = fract(t);
    if mirror {
        uv = select(uv, 1.0 - uv, vec2<bool>(odd(floor(t.x)), odd(floor(t.y))));
    }
    let origin = vec2<i32>(f.arg[1].xy);
    let point = uv * size;
    let radius = vec2<f32>(tile_radius(scale.x), tile_radius(scale.y));
    let lo = point - radius;
    let hi = point + radius;
    let end = vec2<i32>(ceil(hi));
    var sum = vec4<f32>(0.0);
    for (var y = i32(floor(lo.y)); y < end.y; y++) {
        let wy = overlap_1d(y, lo.y, hi.y);
        for (var x = i32(floor(lo.x)); x < end.x; x++) {
            let wx = overlap_1d(x, lo.x, hi.x);
            let q = tile_texel(vec2<i32>(x, y), vec2<i32>(size), mirror);
            sum += wx * wy * load(src, f.src, origin + q);
        }
    }
    let coverage = saturate(min(xy + 1.0, f.arg[3].zw) - max(xy, f.arg[3].xy));
    store(p, sum * coverage.x * coverage.y / (4.0 * radius.x * radius.y));
}

@compute @workgroup_size(8, 8)
fn radial_shadow_project(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let light = f.arg[0].xy;
    let q = light + (vec2<f32>(p) - light) / f.arg[0].z;
    let c = bilinear(src, f.src, q);
    let shadow = f.arg[1];
    let influence = f.arg[2].x;
    let tint = saturate(2.1 * pow(influence, 0.74) * pow(saturate(1.0 - c.a), 0.78));
    let color = mix(shadow.rgb, straight(c), tint);
    let alpha = c.a * floor(f.arg[0].w * 255.0) / 255.0;
    store(p, vec4<f32>(saturate(color) * alpha, alpha));
}

@compute @workgroup_size(8, 8)
fn radial_shadow_over(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    store(p, c + load(aux, f.aux, p) * (1.0 - c.a));
}
