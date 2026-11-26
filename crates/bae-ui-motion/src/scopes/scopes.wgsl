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
    // Waveform type: 0 luma, 1 RGB, 2 YC.
    wave_mode: u32,
    parade: u32, // RGB, RGB white, YUV, YUV white.
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
const CB: u32 = PEAK + 1u;
const CR: u32 = CB + 65536u;
const CHROMA: u32 = CR + 65536u;

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
    let r = level(c.r);
    let g = level(c.g);
    let b = level(c.b);
    atomicAdd(&bins[PARADE + (255u - r) * 256u + col], 1u);
    atomicAdd(&bins[PARADE + 65536u + (255u - g) * 256u + col], 1u);
    atomicAdd(&bins[PARADE + 131072u + (255u - b) * 256u + col], 1u);
    atomicAdd(&bins[HIST + r], 1u);
    atomicAdd(&bins[HIST + 256u + g], 1u);
    atomicAdd(&bins[HIST + 512u + b], 1u);
    atomicAdd(&bins[HIST + 768u + level(y)], 1u);
    // Rec. 709 Cb / Cr in -0.5 … 0.5: blue to the right, red up.
    let cb = (c.b - y) / 1.8556;
    let cr = (c.r - y) / 1.5748;
    if p.wave_mode == 2u {
        let chroma = length(vec2f(cb, cr));
        atomicAdd(&bins[CHROMA + (255u - level(y - chroma)) * 256u + col], 1u);
        atomicAdd(&bins[CHROMA + (255u - level(y + chroma)) * 256u + col], 1u);
    }
    if p.parade >= 2u {
        atomicAdd(&bins[CB + (255u - level(cb + 0.5)) * 256u + col], 1u);
        atomicAdd(&bins[CR + (255u - level(cr + 0.5)) * 256u + col], 1u);
    }
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
    atomicMax(&tallest, atomicLoad(&bins[HIST + lane]));
    atomicMax(&tallest, atomicLoad(&bins[HIST + 256u + lane]));
    atomicMax(&tallest, atomicLoad(&bins[HIST + 512u + lane]));
    workgroupBarrier();
    if lane == 0u {
        atomicStore(&bins[PEAK], atomicLoad(&tallest));
    }
}

// Histogram bar of channel `ch` at level `lv`: lit left of its length.
// (No writes through a runtime vector index: D3D's FXC rejects them.)
fn bar(ch: u32, lv: u32, x: u32) -> f32 {
    let peak = f32(atomicLoad(&bins[PEAK]));
    let len = f32(atomicLoad(&bins[HIST + ch * 256u + lv])) / peak * 255.0;
    return select(0.0, 0.8, f32(x) < len);
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
        rgb = vec3f(bar(0u, lv, x), bar(1u, lv, x), bar(2u, lv, x));
    } else if tile == 2u {
        let ch = min(x / 86u, 2u);
        let col = min((x - ch * 86u) * 256u / 86u, 255u);
        let bases = vec3u(WAVE, CB, CR);
        let yuv = p.parade >= 2u;
        let start = select(PARADE + ch * 65536u, bases[ch], yuv);
        let g = glow(atomicLoad(&bins[start + y * 256u + col]));
        if (p.parade & 1u) != 0u || (yuv && ch == 0u) {
            rgb = vec3f(g);
        } else if yuv {
            rgb = select(vec3f(0.0, 0.0, g), vec3f(g, 0.0, 0.0), ch == 2u);
        } else {
            rgb = vec3f(select(0.0, g, ch == 0u), select(0.0, g, ch == 1u), select(0.0, g, ch == 2u));
        }
    } else if p.wave_mode == 1u {
        let at = y * 256u + x;
        rgb = vec3f(
            glow(atomicLoad(&bins[PARADE + at])),
            glow(atomicLoad(&bins[PARADE + 65536u + at])),
            glow(atomicLoad(&bins[PARADE + 131072u + at])),
        );
    } else {
        let at = y * 256u + x;
        let g = glow(atomicLoad(&bins[WAVE + at]));
        if p.wave_mode == 2u {
            rgb = vec3f(0.0, g, glow(atomicLoad(&bins[CHROMA + at])));
        } else {
            rgb = vec3f(g * 0.85, g, g * 0.85);
        }
    }
    textureStore(dst, vec2i(id.xy), vec4f(rgb, 1.0));
}
