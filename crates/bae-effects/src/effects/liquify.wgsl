struct Params {
    input: FxImage,
    roi: vec4i,
    origin: vec2f,
    cell: f32,
    // Distortion Percentage over the grid scale (mesh offsets are layer pixels).
    percent: f32,
    cols: u32,
    rows: u32,
    stride: u32,
    color_index: u32,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;
@group(0) @binding(3) var mesh_tex: texture_2d<f32>;

fn mesh_at(x: u32, y: u32) -> vec2f {
    return textureLoad(mesh_tex, vec2i(i32(x), i32(y)), 0).xy;
}

// The mesh's offset at `p` (bilinear between vertices; none outside it).
fn offset_at(p: vec2f) -> vec2f {
    let g = (p - u.origin) / u.cell;
    let last = vec2f(f32(u.cols) - 1.0, f32(u.rows) - 1.0);
    if any(g < vec2f(0.0)) || any(g > last) || u.cols < 2u || u.rows < 2u {
        return vec2f(0.0);
    }
    let c = min(floor(g), last - 1.0);
    let f = g - c;
    let x = u32(c.x);
    let y = u32(c.y);
    let top = mix(mesh_at(x, y), mesh_at(x + 1u, y), f.x);
    let bottom = mix(mesh_at(x, y + 1u), mesh_at(x + 1u, y + 1u), f.x);
    return mix(top, bottom, f.y);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= vec2u(u.roi.zw - u.roi.xy)) { return; }
    let point = vec2f(u.roi.xy + vec2i(id.xy)) + 0.5;
    var read_at = point;
    if u.percent != 0. {
        read_at += offset_at(point) * u.percent;
    }
    var color = fx_bilinear(src, u.input, read_at, false);
    if u.stride > 0u {
        let g = (read_at - u.origin) / u.cell;
        if all(g >= vec2f(0.)) && all(g <= vec2f(vec2u(u.cols,u.rows) - 1u)) {
            let lines = fract(g / f32(u.stride) + 0.5);
            let dist = abs(lines - 0.5) * u.cell * f32(u.stride);
            let a = 1. - smoothstep(0.3, 1., min(dist.x,dist.y));
            let palette = array(vec3f(1,0,0), vec3f(1,1,0), vec3f(0,1,0), vec3f(0,1,1),
                vec3f(0,0,1), vec3f(1,0,1), vec3f(.5));
            color = mix(color, vec4f(palette[u.color_index],1.),a);
        }
    }
    textureStore(dst, vec2i(id.xy), color);
}
