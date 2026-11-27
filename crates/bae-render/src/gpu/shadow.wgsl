// Shadows of a lit 3D layer (light.rs: record layout, CPU reference), shared
// by the composite pass (`composite.wgsl`) and the receiver-space shadow maps
// (`shadowmap.wgsl`). Both declare `lighting` (the records), `t_shadow` (the
// image casters' atlas) and `s_linear`.

// AE's shadow edge without diffusion, in receiver pixels (light.rs).
const SHADOW_SOFTNESS: f32 = 1.0;

// Fraction of [c - s, c + s] inside [lo, hi].
fn span(c: f32, s: f32, lo: f32, hi: f32) -> f32 {
    return max(min(c + s, hi) - max(c - s, lo), 0.0) / (2.0 * s);
}

// What caster `c` (its block) lets through of a light at `to` from point
// `p` of the receiver: 1 where it casts no shadow there.
fn caster_keep(c: u32, p: vec3<f32>, to: vec3<f32>, l0: vec4<f32>, l4: vec4<f32>, light_dist: f32, pixel: f32) -> vec3<f32> {
    let plane = lighting[c];
    let denom = dot(plane.xyz, to);
    if abs(denom) < 1e-9 { return vec3<f32>(1.0); }
    let t = -(dot(plane.xyz, p) + plane.w) / denom;
    if t <= 1e-5 || t >= 1.0 { return vec3<f32>(1.0); }
    let h = p + to * t;
    let r0 = lighting[c + 1u];
    let r1 = lighting[c + 2u];
    let u = vec2<f32>(dot(r0.xyz, h) + r0.w, dot(r1.xyz, h) + r1.w);
    let rect = lighting[c + 3u];
    let info = lighting[c + 7u];
    // Diffusion: a square light of that half-size seen through the caster;
    // at least AE's shadow-map edge.
    // (Parallel lights: an orthographic shadow map, the edge does not shrink.)
    let frac = clamp(t * length(to) / light_dist, 0.0, 1.0);
    let shrink = select(1.0 - frac, 1.0, u32(l0.w) == 1u);
    let s = max(max(l4.y * frac, SHADOW_SOFTNESS * pixel * shrink) * info.z, 1e-4);
    if u.x + s <= rect.x || u.x - s >= rect.z || u.y + s <= rect.y || u.y - s >= rect.w { return vec3<f32>(1.0); }
    if info.w == 0.0 {
        let col = lighting[c + 4u];
        let a = span(u.x, s, rect.x, rect.z) * span(u.y, s, rect.y, rect.w) * col.a * info.x;
        // What passes: the transmitted colour, else the Shadow Color.
        let through = info.y * col.rgb + (1.0 - info.y) * lighting[c + 8u].rgb;
        return 1.0 - l4.x * a * (1.0 - through);
    } else {
        let map = lighting[c + 5u];
        let clip = lighting[c + 6u];
        let dims = vec2<f32>(textureDimensions(t_shadow));
        let n = u32(clamp(ceil(2.0 * s * map.x * dims.x), 1.0, 6.0));
        let sub = s / f32(n);
        var sum = vec3<f32>(0.0);
        for (var j = 0u; j < n; j++) {
            for (var k = 0u; k < n; k++) {
                let q = u + (vec2<f32>(f32(k), f32(j)) * 2.0 + 1.0) * sub - s;
                let cov = span(q.x, sub, rect.x, rect.z) * span(q.y, sub, rect.y, rect.w);
                let uv = clamp(q * map.xy + map.zw, clip.xy, clip.zw);
                let texel = textureSampleLevel(t_shadow, s_linear, uv, 0.0);
                let col = texel.rgb / max(texel.a, 1e-6);
                let through = info.y * col + (1.0 - info.y) * lighting[c + 8u].rgb;
                sum += 1.0 - l4.x * cov * texel.a * info.x * (1.0 - through);
            }
        }
        return sum / f32(n * n);
    }
}

// How much of light `li` (l0, l1, l4 of its record) reaches point `p` of the
// layer at input texel `u_recv` past the shadow casters of record `o`
// (AE-CMP-437, light.rs `shadow`); `pixel` is the comp-space size of a
// receiver pixel at p.
// Each caster's block is followed by its footprint on the layer per light; an
// 8×8 grid over the layer lists, per light and tile, the casters whose
// footprint reaches the tile (128 at most; more: every caster is tested). The
// loop stops once the point is fully in shadow.
fn shadow(o: u32, p: vec3<f32>, u_recv: vec2<f32>, li: u32, l0: vec4<f32>, l1: vec4<f32>, l4: vec4<f32>, pixel: f32) -> vec3<f32> {
    let head = lighting[o + 6u];
    let count = u32(head.y);
    let block = o + u32(head.z);
    let nl = u32(lighting[o + 3u].w);
    let stride = 9u + nl;
    var to = l0.xyz - p;
    if u32(l0.w) == 1u { to = -l1.xyz * 1e6; }
    let light_dist = max(length(l0.xyz - p), 1e-6);
    var keep = vec3<f32>(1.0);
    let tiles = block + stride * count;
    let grid = lighting[tiles];
    if grid.z <= 0.0 {
        for (var i = 0u; i < count; i++) {
            let c = block + stride * i;
            let foot = lighting[c + 9u + li];
            if any(u_recv < foot.xy) || any(u_recv > foot.zw) { continue; }
            keep *= caster_keep(c, p, to, l0, l4, light_dist, pixel);
            if max(keep.r, max(keep.g, keep.b)) < 1e-3 { break; }
        }
        return keep;
    }
    let cell = clamp(vec2<i32>(floor((u_recv - grid.xy) * grid.zw)), vec2<i32>(0), vec2<i32>(7));
    let mask = bitcast<vec4<u32>>(lighting[tiles + 1u + li * 64u + u32(cell.y * 8 + cell.x)]);
    for (var w = 0u; w < 4u; w++) {
        var bits = mask[w];
        while bits != 0u {
            let i = w * 32u + firstTrailingBit(bits);
            bits &= bits - 1u;
            let c = block + stride * i;
            let foot = lighting[c + 9u + li];
            if any(u_recv < foot.xy) || any(u_recv > foot.zw) { continue; }
            keep *= caster_keep(c, p, to, l0, l4, light_dist, pixel);
            if max(keep.r, max(keep.g, keep.b)) < 1e-3 { return keep; }
        }
    }
    return keep;
}

