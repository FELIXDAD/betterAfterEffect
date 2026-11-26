// A wide disk blur in three passes (input -> a -> b -> output), each wider
// than the last and each pass's tap spacing covered by the one before:
// sparse wide taps no longer show discrete copies of small highlights, and
// a pixel costs ~100 taps at any radius instead of thousands. `a` and `b`
// cover the whole panorama and are sampled on the sphere like the input.
const CASCADE_TAPS = 36u;
// Each disk is half as wide as the next, so its (Gaussian-weighted) spread
// covers the next one's tap spacing; together their widths add up (in
// variance) to about the asked radius.
fn cascade_radius(radius: f32, step: u32) -> f32 {
    return 0.87*radius*select(select(1.0, 0.5, step == 1u), 0.25, step == 0u);
}
// The fine pass gets more taps as it grows (it is the one sampling detail).
fn cascade_fine_taps(radius: f32) -> u32 {
    return clamp(u32(3.0*radius), 24u, 64u);
}
// Pole folding and seam wrap of an eye's texel (as `sphere_tap`): its
// position and whether it straddles two texels (odd widths over a pole).
fn sphere_fold(p: vec2i, rect: BaeRect, fov: vec2f) -> vec3i {
    let span = max(rect.max - rect.min, vec2i(1));
    var q = p;
    var half_texel = 0;
    if (q.y < 0 || q.y >= span.y) && fov.y >= PI - 0.00001 {
        q.y = select(-1-q.y, 2*span.y-1-q.y, q.y >= span.y);
        if fov.x >= TAU - 0.00001 {
            q.x += span.x / 2;
            half_texel = select(0, 1, (span.x % 2) != 0);
        }
    }
    q.y = clamp(q.y, 0, span.y-1);
    if fov.x >= TAU - 0.00001 {
        q.x = wrap_index(q.x, span.x);
    } else { q.x = clamp(q.x,0,span.x-1); }
    return vec3i(rect.min + q, half_texel);
}
fn fold_next(f: vec3i, rect: BaeRect) -> vec2i {
    let span = max(rect.max.x - rect.min.x, 1);
    return vec2i(rect.min.x + (f.x - rect.min.x + 1) % span, f.y);
}
// The bilinear position of a tap in the eye (as `sample_uv`); z = 0 outside
// a partial panorama.
fn bilinear_at(n: Neighborhood, offset: vec2f) -> vec3f {
    let uv = sphere_uv(nearby_ray(n,offset), n.eye.fov);
    let inside = all(uv >= vec2f(-0.00001)) && all(uv <= vec2f(1.00001));
    return vec3f(uv * rect_size(n.eye.rect) - 0.5, select(0.0, 1.0, inside));
}
fn tap_a(p: vec2i, n: Neighborhood) -> vec4f {
    let f = sphere_fold(p,n.eye.rect,n.eye.fov);
    let c = bae_load_a(f.xy);
    if f.z == 0 { return c; }
    return 0.5*(c+bae_load_a(fold_next(f,n.eye.rect)));
}
fn nearby_a(n: Neighborhood, offset: vec2f) -> vec4f {
    let at = bilinear_at(n,offset);
    if at.z == 0.0 { return vec4f(0.0); }
    let q = vec2i(floor(at.xy));
    let f = fract(at.xy);
    return mix(mix(tap_a(q,n),tap_a(q+vec2i(1,0),n),f.x),
               mix(tap_a(q+vec2i(0,1),n),tap_a(q+1,n),f.x),f.y);
}
fn tap_b(p: vec2i, n: Neighborhood) -> vec4f {
    let f = sphere_fold(p,n.eye.rect,n.eye.fov);
    let c = bae_load_b(f.xy);
    if f.z == 0 { return c; }
    return 0.5*(c+bae_load_b(fold_next(f,n.eye.rect)));
}
fn nearby_b(n: Neighborhood, offset: vec2f) -> vec4f {
    let at = bilinear_at(n,offset);
    if at.z == 0.0 { return vec4f(0.0); }
    let q = vec2i(floor(at.xy));
    let f = fract(at.xy);
    return mix(mix(tap_b(q,n),tap_b(q+vec2i(1,0),n),f.x),
               mix(tap_b(q+vec2i(0,1),n),tap_b(q+1,n),f.x),f.y);
}
