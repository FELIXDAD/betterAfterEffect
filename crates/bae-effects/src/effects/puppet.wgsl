struct Params {
    input: FxImage,
    roi: vec4i,
    // Tiles per row of `bins`.
    columns: u32,
    count: u32,
    pad: vec2u,
}

// A triangle: per corner, where it is (xy) and where it rested (zw), in
// render-grid pixels; `info.x` is its In Front priority.
struct Tri {
    a: vec4f,
    b: vec4f,
    c: vec4f,
    info: vec4f,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;
@group(0) @binding(3) var<storage, read> tris: array<Tri>;
// Per tile (start, count) into the same array, then triangle indices.
@group(0) @binding(4) var<storage, read> bins: array<u32>;

const TILE: u32 = 16u;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= vec2u(u.roi.zw - u.roi.xy)) { return; }
    let p = vec2f(u.roi.xy + vec2i(id.xy)) + 0.5;
    let tile = (id.y / TILE) * u.columns + id.x / TILE;
    let start = bins[2u * tile];
    let count = bins[2u * tile + 1u];
    var found = false;
    var best = 0.0;
    var read_at = vec2f(0.0);
    for (var k = 0u; k < count; k++) {
        let t = tris[bins[start + k]];
        let v0 = t.b.xy - t.a.xy;
        let v1 = t.c.xy - t.a.xy;
        let v2 = p - t.a.xy;
        let den = v0.x * v1.y - v1.x * v0.y;
        if abs(den) < 1e-9 { continue; }
        let s = (v2.x * v1.y - v1.x * v2.y) / den;
        let r = (v0.x * v2.y - v2.x * v0.y) / den;
        let w = 1.0 - s - r;
        if min(min(s, r), w) < -1e-4 { continue; }
        // The first triangle wins unless one further on is more in front.
        if found && t.info.x <= best { continue; }
        found = true;
        best = t.info.x;
        read_at = t.a.zw * w + t.b.zw * s + t.c.zw * r;
    }
    if !found {
        textureStore(dst, vec2i(id.xy), vec4f(0.0));
        return;
    }
    textureStore(dst, vec2i(id.xy), fx_bilinear(src, u.input, read_at, false));
}
