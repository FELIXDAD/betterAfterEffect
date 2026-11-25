// Lumetri Scopes on the GPU: `accumulate` bins the displayed frame (8-bit,
// display-encoded, as AE's 8 Bit scopes read it), `peak` finds the histogram's
// tallest bin, `draw` turns the bins into the scope image: four 256 × 256
// tiles side by side (vectorscope, histogram, RGB parade, luma waveform).

struct Params {
    size: vec2u,
    step: u32,
    // Samples per waveform bin if the frame were spread evenly, over the
    // brightness setting.
    unit: f32,
    // Waveform type: 0 YC No Chroma (luma), 1 RGB.
    wave_rgb: u32,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> bins: array<atomic<u32>>;
@group(0) @binding(3) var dst: texture_storage_2d<rgba8unorm, write>;

const WAVE: u32 = 0u;
const PARADE: u32 = 65536u;
const VECTOR: u32 = 262144u;
const HIST: u32 = 327680u;
const PEAK: u32 = 328704u;

fn luma(c: vec3f) -> f32 {
    return dot(c, vec3f(0.2126, 0.7152, 0.0722));
}

fn level(v: f32) -> u32 {
    return u32(round(saturate(v) * 255.0));
}

@compute @workgroup_size(16, 16)
fn accumulate(@builtin(global_invocation_id) id: vec3u) {
    let pos = id.xy * p.step;
    if pos.x >= p.size.x || pos.y >= p.size.y {
        return;
    }
    let c = textureLoad(src, vec2i(pos), 0).rgb;
    let col = pos.x * 256u / p.size.x;
    let y = luma(c);
    atomicAdd(&bins[WAVE + (255u - level(y)) * 256u + col], 1u);
    let rgb = vec3u(level(c.r), level(c.g), level(c.b));
    for (var ch = 0u; ch < 3u; ch++) {
        atomicAdd(&bins[PARADE + ch * 65536u + (255u - rgb[ch]) * 256u + col], 1u);
        atomicAdd(&bins[HIST + ch * 256u + rgb[ch]], 1u);
    }
    atomicAdd(&bins[HIST + 768u + level(y)], 1u);
    // Rec. 709 Cb / Cr in -0.5 … 0.5: blue to the right, red up.
    let cb = (c.b - y) / 1.8556;
    let cr = (c.r - y) / 1.5748;
    let vx = u32(clamp(round((cb + 0.5) * 255.0), 0.0, 255.0));
    let vy = u32(clamp(round((0.5 - cr) * 255.0), 0.0, 255.0));
    atomicAdd(&bins[VECTOR + vy * 256u + vx], 1u);
}

var<workgroup> tallest: atomic<u32>;

@compute @workgroup_size(256)
fn peak(@builtin(local_invocation_index) lane: u32) {
    if lane == 0u {
        atomicStore(&tallest, 1u);
    }
    workgroupBarrier();
    for (var ch = 0u; ch < 3u; ch++) {
        atomicMax(&tallest, atomicLoad(&bins[HIST + ch * 256u + lane]));
    }
    workgroupBarrier();
    if lane == 0u {
        atomicStore(&bins[PEAK], atomicLoad(&tallest));
    }
}

fn glow(v: u32) -> f32 {
    return min(1.0, sqrt(f32(v) / p.unit) * 0.35);
}

@compute @workgroup_size(16, 16)
fn draw(@builtin(global_invocation_id) id: vec3u) {
    if id.x >= 1024u || id.y >= 256u {
        return;
    }
    let tile = id.x / 256u;
    let x = id.x % 256u;
    let y = id.y;
    var rgb = vec3f(0.0);
    if tile == 0u {
        let g = glow(atomicLoad(&bins[VECTOR + y * 256u + x]));
        rgb = vec3f(g * 0.85, g, g * 0.85);
    } else if tile == 1u {
        // Levels bottom (0) to top (255), counts to the right.
        let lv = 255u - y;
        let peak = f32(atomicLoad(&bins[PEAK]));
        for (var ch = 0u; ch < 3u; ch++) {
            let len = f32(atomicLoad(&bins[HIST + ch * 256u + lv])) / peak * 255.0;
            if f32(x) < len {
                rgb[ch] = 0.8;
            }
        }
    } else if tile == 2u {
        let ch = min(x / 86u, 2u);
        let col = (x - ch * 86u) * 256u / 86u;
        let g = glow(atomicLoad(&bins[PARADE + ch * 65536u + y * 256u + min(col, 255u)]));
        rgb[ch] = g;
    } else if p.wave_rgb == 1u {
        for (var ch = 0u; ch < 3u; ch++) {
            rgb[ch] = glow(atomicLoad(&bins[PARADE + ch * 65536u + y * 256u + x]));
        }
    } else {
        let g = glow(atomicLoad(&bins[WAVE + y * 256u + x]));
        rgb = vec3f(g * 0.85, g, g * 0.85);
    }
    textureStore(dst, vec2i(id.xy), vec4f(rgb, 1.0));
}
