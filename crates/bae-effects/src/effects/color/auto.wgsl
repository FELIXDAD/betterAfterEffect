struct Settings {
    image: FxImage, origin: vec2i, size: vec2u, mode: u32, equalize: u32,
    neutral: u32, amount: f32, black: f32, white: f32,
}
@group(0) @binding(0) var<uniform> u: Settings;
@group(0) @binding(1) var dst: texture_storage_2d<WORKING_FORMAT, write>;
@group(0) @binding(2) var src: texture_2d<f32>;
@group(0) @binding(3) var mapping: texture_2d<f32>;

fn lightness_bin(rgb: vec3u) -> u32 {
    return (max(rgb.r, max(rgb.g, rgb.b)) + min(rgb.r, min(rgb.g, rgb.b))) / 2u;
}

var<workgroup> counts: array<atomic<u32>, 1024>;
// Sum a reference color in the unused fourth histogram channel. The component
// sums have the same range as the weight sum, including on a 32K layer.
fn add_reference(bin: u32, rgb: vec3u, weight: u32) {
    if weight == 0u { return; }
    let v = (rgb * weight + 127u) / 255u;
    atomicAdd(&counts[bin], weight);
    atomicAdd(&counts[bin + 1u], v.r);
    atomicAdd(&counts[bin + 2u], v.g);
    atomicAdd(&counts[bin + 3u], v.b);
}
@compute @workgroup_size(8, 8)
fn auto_histogram(@builtin(global_invocation_id) id: vec3u, @builtin(workgroup_id) group: vec3u,
                  @builtin(local_invocation_index) lane: u32) {
    for (var k = lane; k < 1024u; k += 64u) { atomicStore(&counts[k], 0u); }
    workgroupBarrier();
    // Each workgroup accumulates its part of the full layer in a private histogram.
    for (var y = id.y; y < u.image.size.y; y += 128u) {
        for (var x = id.x; x < u.image.size.x; x += 128u) {
            let c = fx_load(src, u.image, vec2i(vec2u(x,y)), true);
            let rgb = vec3u(round(saturate(c.rgb) * 255.0));
            atomicAdd(&counts[rgb.r], 1u);
            atomicAdd(&counts[256u + rgb.g], 1u);
            atomicAdd(&counts[512u + rgb.b], 1u);
            let y8 = u32(round(dot(vec3f(rgb), vec3f(0.299, 0.587, 0.114))));
            if u.mode == 2u {
                let low = min(rgb.r, min(rgb.g, rgb.b));
                let high = max(rgb.r, max(rgb.g, rgb.b));
                add_reference(772u, rgb, 92u - min(high, 92u));
                add_reference(776u, rgb, max(low, 127u) - 127u);
                let spread = high - low;
                if spread <= 10u && y8 >= 64u && y8 <= 192u {
                    atomicAdd(&counts[768], 1u);
                }
            } else {
                let bin = select(y8, lightness_bin(rgb), u.mode == 3u);
                atomicAdd(&counts[768u + bin], 1u);
            }
        }
    }
    workgroupBarrier();
    for (var k = lane; k < 256u; k += 64u) {
        let v = vec4u(atomicLoad(&counts[k]), atomicLoad(&counts[256u + k]),
                      atomicLoad(&counts[512u + k]), atomicLoad(&counts[768u + k]));
        var partial = vec4f(v) / 1024.0;
        if u.mode == 2u && k >= 4u && k < 12u {
            partial.w /= 128.;
        }
        textureStore(dst, vec2u(k, group.y*16u + group.x), partial);
    }
}

var<workgroup> density: array<vec4f, 256>;
@compute @workgroup_size(8, 8)
fn auto_mapping(@builtin(local_invocation_index) lane: u32) {
    for (var k = lane; k < 256u; k += 64u) {
        var sum = vec4f(0);
        for (var row = 0u; row < 256u; row++) { sum += textureLoad(src, vec2u(k, row), 0); }
        density[k] = sum;
    }
    workgroupBarrier();
    var total = vec4f(0);
    for (var k = 0u; k < 256u; k++) { total += density[k]; }
    var lo = vec4f(255);
    var hi = vec4f(0);
    var cumulative = vec4f(0);
    for (var k = 0u; k < 256u; k++) {
        let next = cumulative + density[k];
        lo = select(lo, min(lo, vec4f(f32(k))), next > total*u.black);
        hi = select(hi, max(hi, vec4f(f32(k))), cumulative < total*(1.0-u.white));
        cumulative = next;
    }
    if u.mode == 1u {
        lo = lo.wwww;
        hi = hi.wwww;
    } else if u.mode == 2u {
        let white = max(hi.r, max(hi.g, hi.b));
        var light = vec3f(white);
        var dark = lo.rgb - (white - hi.rgb) * lo.rgb / max(hi.rgb, vec3f(1));
        if density[0].w > 0. && density[4].w > 0. {
            dark = 255. * vec3f(density[5].w, density[6].w, density[7].w) / density[4].w;
            if density[8].w > 0. {
                light = 255. * vec3f(density[9].w, density[10].w, density[11].w) / density[8].w;
            }
        }
        // Extend the neutral axis until the first channel hits each clipped endpoint.
        let width = max(light - dark, vec3f(1));
        let start = (lo.rgb - dark) / width;
        let end = (hi.rgb - dark) / width;
        lo = vec4f(dark + width * min(start.r, min(start.g, start.b)), 0.);
        hi = vec4f(dark + width * max(end.r, max(end.g, end.b)), 255.);
    }
    if u.neutral != 0u && lane == 0u {
        textureStore(dst, vec2u(256, 0), lo / 255.);
        textureStore(dst, vec2u(257, 0), hi / 255.);
    }
    var cdf = vec4f(0);
    for (var k = 0u; k < lane*4u; k++) { cdf += density[k]; }
    for (var k = lane*4u; k < lane*4u+4u; k++) {
        cdf += density[k];
        let x = f32(k);
        var result = select(vec4f(x/255.), saturate((x-lo)/max(hi-lo, vec4f(1))), hi>lo);
        if u.mode == 3u {
            let combined = dot(cdf.rgb, vec3f(1));
            let norm = max(dot(total.rgb, vec3f(1)), 1e-8);
            var mapped = ceil(combined/norm*256.) - 1.;
            if u.equalize == 3u {
                // Photoshop Style maps the centers of occupied bins and anchors both endpoints.
                let first = u32(min(lo.r, min(lo.g, lo.b)));
                let last = u32(max(hi.r, max(hi.g, hi.b)));
                let start = 0.5 * dot(density[first].rgb, vec3f(1));
                let end = norm - 0.5 * dot(density[last].rgb, vec3f(1));
                let center = combined - 0.5 * dot(density[k].rgb, vec3f(1));
                mapped = select(x, floor(255.*(center-start)/max(end-start, 1e-8)-0.5), end > start);
            } else if combined == dot(density[k].rgb, vec3f(1)) { mapped = 0.; }
            let lightness = select(floor(cdf.w / max(total.w, 1e-8) * 256.) - 1., 0., cdf.w == density[k].w);
            result = saturate(vec4f(vec3f(mapped), lightness)/255.);
        }
        textureStore(dst, vec2u(k, 0), result);
    }
}

fn corrected_rgb(p: vec2i, low: vec3f, high: vec3f) -> vec3f {
    let rgb = saturate(fx_load(src, u.image, p, true).rgb);
    return round(255. * select(rgb, saturate((rgb-low)/max(high-low, vec3f(1./255.))), high>low));
}

fn neutral_distance(rgb: vec3f) -> f32 {
    let high = max(rgb.r, max(rgb.g, rgb.b));
    let low = min(rgb.r, min(rgb.g, rgb.b));
    let sat = (high-low) / max(1., 255.-abs(high+low-255.));
    return (abs((high+low)*0.5-127.5) + 255.*sat) / 64.;
}

@compute @workgroup_size(8, 8)
fn auto_midtones(@builtin(global_invocation_id) id: vec3u, @builtin(workgroup_id) group: vec3u,
                 @builtin(local_invocation_index) lane: u32) {
    let low = textureLoad(mapping, vec2u(256, 0), 0).rgb;
    let high = textureLoad(mapping, vec2u(257, 0), 0).rgb;
    for (var k = lane; k < 128u; k += 64u) { atomicStore(&counts[k], 0u); }
    workgroupBarrier();
    for (var y = id.y+1u; y < u.image.size.y; y += 128u) {
        for (var x = id.x+1u; x < u.image.size.x; x += 128u) {
            let p = vec2i(vec2u(x,y));
            let rgb = corrected_rgb(p, low, high);
            // AE only admits a complete 2x2 neighborhood and takes its bottom-right color.
            let d = max(max(neutral_distance(rgb), neutral_distance(corrected_rgb(p-vec2i(1,0), low, high))),
                        max(neutral_distance(corrected_rgb(p-vec2i(0,1), low, high)),
                            neutral_distance(corrected_rgb(p-vec2i(1,1), low, high))));
            if d <= 1. {
                let b = 4u * min(u32(d*32.), 31u);
                atomicAdd(&counts[b], u32(rgb.r));
                atomicAdd(&counts[b+1u], u32(rgb.g));
                atomicAdd(&counts[b+2u], u32(rgb.b));
                atomicAdd(&counts[b+3u], 1u);
            }
        }
    }
    workgroupBarrier();
    if lane < 32u {
        let b = lane*4u;
        let v = vec4u(atomicLoad(&counts[b]), atomicLoad(&counts[b+1u]),
                      atomicLoad(&counts[b+2u]), atomicLoad(&counts[b+3u]));
        textureStore(dst, vec2u(lane, group.y*16u+group.x), vec4f(v)/vec4f(261120.,261120.,261120.,1024.));
    }
}

@compute @workgroup_size(8, 8)
fn auto_snap(@builtin(local_invocation_index) lane: u32) {
    if lane < 32u {
        var band = vec4f(0);
        for (var row = 0u; row < 256u; row++) { band += textureLoad(mapping, vec2u(lane,row), 0); }
        density[lane] = band;
    }
    workgroupBarrier();
    let population = f32(u.image.size.x)*f32(u.image.size.y)/1024.;
    var count = 0.;
    var last = 0u;
    // A full tenth of the image establishes the search radius. Fewer than
    // one percent qualified even at the widest radius leaves the curve alone.
    for (var i=0u; i<32u; i++) {
        count += density[i].a;
        last = i;
        if count >= 0.1*population { break; }
    }
    var exponent = vec3f(1);
    if count >= 0.01*population {
        var sum = vec4f(0);
        for (var i=0u; i<=last; i++) {
            sum += density[i] * f32(last-i+1u);
        }
        let mid = sum.rgb / sum.a;
        let gray = dot(mid, vec3f(1./3.));
        exponent = log(vec3f(gray)) / log(mid);
    }
    for (var k = lane; k < 256u; k += 64u) {
        let value = textureLoad(src, vec2u(k, 0), 0);
        textureStore(dst, vec2u(k, 0), vec4f(pow(value.rgb, exponent), value.a));
    }
}

fn mapped(x: f32, ch: u32) -> f32 {
    let level = saturate(x)*255.;
    let i = u32(level);
    return mix(textureLoad(mapping, vec2u(i, 0), 0)[ch],
               textureLoad(mapping, vec2u(min(i+1u, 255u), 0), 0)[ch], fract(level));
}
@compute @workgroup_size(8, 8)
fn auto_apply(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= u.size) { return; }
    let c = saturate(fx_sample(src, u.image, vec2f(u.origin) + vec2f(id.xy) + 0.5, true));
    var rgb = vec3f(mapped(c.r, 0u), mapped(c.g, 1u), mapped(c.b, 2u));
    if u.mode == 3u && u.equalize == 2u {
        let bin = lightness_bin(vec3u(round(saturate(c.rgb)*255.)));
        let new_l = textureLoad(mapping, vec2u(bin, 0), 0).w;
        let hsl = fx_hsl(c.rgb);
        rgb = fx_rgb(vec3f(hsl.xy, new_l));
    }
    textureStore(dst, id.xy, vec4f(mix(c.rgb, rgb, u.amount)*c.a, c.a));
}
