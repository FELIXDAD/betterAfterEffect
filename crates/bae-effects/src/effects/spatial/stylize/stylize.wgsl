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

fn cartoon_brightness(rgb: vec3<f32>) -> f32 {
    return dot(pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(0.625)), LUMA);
}

fn cartoon_signal(p: vec2<i32>, fallback: f32) -> f32 {
    let c = load(aux, f.aux, clamp(p, f.aux.xy, f.aux.zw - 1));
    return select(fallback, cartoon_brightness(straight(c)), c.a > 0.0);
}

@compute @workgroup_size(8, 8)
fn cartoon_fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    if c.a == 0.0 {
        store(p, c);
        return;
    }

    let xy = vec2<f32>(p);
    let width = max(0.5, f.arg[0].z) * 2.5;
    let dx = vec2<i32>(1, 0);
    let dy = vec2<i32>(0, 1);
    let center = cartoon_signal(p, cartoon_brightness(straight(c)));
    let left = cartoon_signal(p - dx, center);
    let right = cartoon_signal(p + dx, center);
    let top = cartoon_signal(p - dy, center);
    let bottom = cartoon_signal(p + dy, center);
    let gradient = vec2<f32>(right - left, bottom - top);
    let laplacian = left + right + top + bottom - 4.0 * center;
    let warp = -sign(laplacian) * gradient * f.arg[0].w * width;
    let fill = bilinear(aux, f.aux, clamp_to_rect(xy + warp, f.aux));
    let rgb = select(straight(c), straight(fill), fill.a > 0.0);

    // AE quantizes Rec.601 luma after encoding each straight channel to
    // the power 0.625, then changes brightness without changing the hue.
    let brightness = cartoon_brightness(rgb);
    let steps = f.arg[0].x - 1.0;
    let bins = brightness * steps;
    let step = max(max(abs(left - center), abs(right - center)),
                   max(abs(top - center), abs(bottom - center)));
    let base_smooth = f.arg[0].y;
    // At the default smoothness, flat regions keep their shade while the
    // pixels immediately next to a contrast boundary still quantize.
    let edge = smoothstep(0.018, 0.068, step) * (1.0 - smoothstep(0.70, 1.0, base_smooth));
    let flat = (1.0 - smoothstep(0.001, 0.005, step)) * smoothstep(0.45, 0.60, base_smooth);
    let smoothness = base_smooth - 0.31 * edge + 0.10 * flat;
    let transition = max(0.001, pow(saturate((smoothness - 0.40) / 0.30), 1.6));
    let q = floor(bins) + smoothstep(0.5 - transition * 0.5, 0.5 + transition * 0.5, fract(bins));
    let shading = mix(q / steps, brightness, smoothstep(0.60, 0.70, smoothness));
    let shade = pow(shading / max(1e-6, brightness), 1.6);
    store(p, vec4<f32>(rgb * shade * c.a, c.a));
}

@compute @workgroup_size(8, 8)
fn cartoon_luma(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    let luma = pow(cartoon_brightness(straight(c)), 0.97);
    store(p, vec4<f32>(luma * c.a, 0.0, 0.0, c.a));
}

@compute @workgroup_size(8, 8)
fn cartoon_stroke(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let xy = vec2<f32>(p);
    let shift = f.arg[1].x;
    var small = straight(load(src, f.src, p)).r;
    var large = straight(load(aux, f.aux, p)).r;
    if shift != 0.0 {
        let dx = vec2<i32>(1, 0);
        let dy = vec2<i32>(0, 1);
        let left = straight(load(src, f.src, clamp(p - dx, f.src.xy, f.src.zw - 1))).r;
        let right = straight(load(src, f.src, clamp(p + dx, f.src.xy, f.src.zw - 1))).r;
        let top = straight(load(src, f.src, clamp(p - dy, f.src.xy, f.src.zw - 1))).r;
        let bottom = straight(load(src, f.src, clamp(p + dy, f.src.xy, f.src.zw - 1))).r;
        let gradient = clamp(vec2<f32>(right - left, bottom - top), vec2<f32>(-1), vec2<f32>(1));
        let laplacian = left + right + top + bottom - 4.0 * small;
        let pos = xy - sign(laplacian) * shift * gradient;
        small = straight(bilinear(src, f.src, clamp_to_rect(pos, f.src))).r;
        large = straight(bilinear(aux, f.aux, clamp_to_rect(pos, f.aux))).r;
    }
    // Edge profiles from gray steps match the relative change between two
    // smoothing scales, with strokes on the darker side of the transition.
    let relative = (large - small) / (small + 0.119);
    let response = max(0.0, relative * f.arg[0].x - f.arg[0].w);
    let strength = pow(response * (1461.0 * f.arg[0].y), 1.31);
    let stroke = (1.0 - pow(1.0 + strength, -1.09)) * f.arg[0].z;
    store(p, vec4<f32>(stroke, 0.0, 0.0, 1.0));
}

@compute @workgroup_size(8, 8)
fn cartoon_finish(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src, f.src, p);
    let rgb = select(c.rgb, vec3<f32>(c.a), f.arg[0].x != 0.0);
    let stroke = load(aux, f.aux, p).r;
    store(p, vec4<f32>(rgb * (1.0 - stroke), c.a));
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
fn tile_resize(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let x_axis = f.arg[0].w != 0.0;
    let origin = i32(f.arg[0].x);
    let footprint = f.arg[0].y / f.arg[0].z;
    let index = select(id.y, id.x, x_axis);
    let lo = f32(index) * footprint;
    let hi = min(f.arg[0].y, f32(index + 1u) * footprint);
    let start = max(origin + i32(floor(lo)), select(f.src.y, f.src.x, x_axis));
    let end = min(origin + i32(ceil(hi)), select(f.src.w, f.src.z, x_axis));
    var sum = vec4<f32>(0.0);
    for (var i = start; i < end; i++) {
        let q = select(vec2<i32>(p.x, i), vec2<i32>(i, p.y), x_axis);
        sum += overlap_1d(i - origin, lo, hi) * load(src, f.src, q);
    }
    store(p, sum / (hi - lo));
}

fn tile_texel(p: vec2<i32>, origin: vec2<i32>, size: vec2<i32>, mirror: bool) -> vec4<f32> {
    let q = select(((p % size) + size) % size, clamp(p, vec2<i32>(0), size - 1), mirror);
    let c = load(src, f.src, origin + q);
    if f.flags.x == 0u {
        return floor(saturate(c) * 255.0 + 0.5) / 255.0;
    }
    return c;
}

@compute @workgroup_size(8, 8)
fn motion_tile(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let xy = vec2<f32>(p);
    let size = f.arg[1].zw;
    let tile = f.arg[0].zw;
    var t = (xy + 0.5 - f.arg[0].xy) / max(tile, vec2<f32>(1e-6)) + 0.5;
    t = select(t, vec2<f32>(0.5), tile == vec2<f32>(0.0));
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
    let point = uv * size - 0.5;
    let low = vec2<i32>(floor(point));
    let weight = fract(point);
    let dims = vec2<i32>(size);
    let a = tile_texel(low, origin, dims, mirror);
    let b = tile_texel(low + vec2<i32>(1, 0), origin, dims, mirror);
    let c = tile_texel(low + vec2<i32>(0, 1), origin, dims, mirror);
    let d = tile_texel(low + 1, origin, dims, mirror);
    let color = mix(mix(a, b, weight.x), mix(c, d, weight.x), weight.y);
    let coverage = saturate(min(xy + 1.0, f.arg[3].zw) - max(xy, f.arg[3].xy));
    store(p, color * coverage.x * coverage.y);
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
