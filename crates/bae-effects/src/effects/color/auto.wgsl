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
                    atomicAdd(&counts[769], rgb.r);
                    atomicAdd(&counts[770], rgb.g);
                    atomicAdd(&counts[771], rgb.b);
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
        if u.mode == 2u && k >= 1u && k < 12u {
            partial.w /= select(128., 255., k < 4u);
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
    var cdf = vec4f(0);
    for (var k = 0u; k < lane*4u; k++) { cdf += density[k]; }
    for (var k = lane*4u; k < lane*4u+4u; k++) {
        cdf += density[k];
        let x = f32(k);
        var result = select(vec4f(x/255.), saturate((x-lo)/max(hi-lo, vec4f(1))), hi>lo);
        if u.mode == 2u && u.neutral != 0u && density[0].w > 0.0 {
            let sample = vec3f(density[1].w, density[2].w, density[3].w) * (255. / density[0].w);
            let mid = clamp((sample-lo.rgb)/max(hi.rgb-lo.rgb, vec3f(1)), vec3f(0.01), vec3f(0.99));
            let gray = dot(mid, vec3f(0.299, 0.587, 0.114));
            result = vec4f(pow(result.rgb, log(vec3f(gray))/log(mid)), result.a);
        } else if u.mode == 3u {
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
