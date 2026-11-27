struct Params {
    input: FxImage,
    // Render-grid position of cell (0, 0) and the cell size.
    origin: vec2f,
    step: f32,
    // Samples per cell side.
    samples: u32,
    cols: u32,
    rows: u32,
    pad: vec2u,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
// Per cell, row by row: the most opaque alpha in it.
@group(0) @binding(2) var<storage, read_write> cells: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if id.x >= u.cols || id.y >= u.rows { return; }
    let base = u.origin + vec2f(id.xy) * u.step;
    let d = u.step / f32(u.samples);
    var most = 0.0;
    for (var j = 0u; j < u.samples; j++) {
        for (var i = 0u; i < u.samples; i++) {
            let p = base + (vec2f(f32(i), f32(j)) + 0.5) * d;
            most = max(most, fx_sample(src, u.input, p, false).a);
        }
    }
    cells[id.y * u.cols + id.x] = most;
}
