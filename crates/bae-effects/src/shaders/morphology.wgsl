struct Morphology {
    source: FxImage,
    original: FxImage,
    read_roi: vec4i,
    write_roi: vec4i,
    domain: vec4i,
    step: vec2i,
    target_origin: vec2i,
    sigma: f32,
    channels: u32,
    flags: u32,
    padding: u32,
    remap: vec4f,
}

const MINIMUM = 1u;
const FIRST = 2u;
const LAST = 4u;
const REPEAT_EDGE = 8u;
const CHOKER = 16u;
const VIEW_MATTE = 32u;
const FLOAT = 64u;

@group(0) @binding(0) var<uniform> u: Morphology;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var original: texture_2d<f32>;
@group(0) @binding(3) var dst: texture_storage_2d<WORKING_FORMAT, write>;

fn flag(f: u32) -> bool { return (u.flags & f) != 0u; }

fn sample(p: vec2i) -> vec4f {
    var q = p;
    if flag(REPEAT_EDGE) {
        q = clamp(q, u.domain.xy, u.domain.zw - 1);
    }
    if any(q < u.read_roi.xy) || any(q >= u.read_roi.zw) {
        return vec4f(0.0);
    }
    return fx_sample(src, u.source, vec2f(q) + 0.5, flag(FIRST) && !flag(CHOKER));
}

fn extremum(a: vec4f, b: vec4f) -> vec4f {
    return select(max(a, b), min(a, b), flag(MINIMUM));
}

// Separable Gaussian with the adjacent weight computed from its ratio to
// the preceding tap. The radius can be varied without a shader recompilation.
fn blur(p: vec2i, center: vec4f) -> vec4f {
    let r = u.step.x + u.step.y;
    if r == 0 { return center; }
    let axis = u.step / r;
    let exponent = -0.5 / max(u.sigma * u.sigma, 1e-6);
    let ratio_step = exp(2.0 * exponent);
    var ratio = exp(exponent);
    var weight = 1.0;
    var sum = 1.0;
    var color = center;
    for (var i = 1; i <= r; i += 1) {
        weight *= ratio;
        ratio *= ratio_step;
        sum += 2.0 * weight;
        color += (sample(p - i * axis) + sample(p + i * axis)) * weight;
    }
    return color / sum;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let p = u.write_roi.xy + vec2i(id.xy);
    if any(p >= u.write_roi.zw) { return; }
    let center = sample(p);
    var v: vec4f;
    if flag(CHOKER) {
        v = blur(p, center);
    } else {
        v = extremum(extremum(center, sample(p - u.step)), sample(p + u.step));
    }
    if flag(LAST) {
        let base = fx_sample(original, u.original, vec2f(p) + 0.5, true);
        if flag(CHOKER) {
            let a = clamp(v.a * u.remap.x + u.remap.y, 0.0, 1.0);
            var rgb = base.rgb;
            if base.a == 0.0 && u.original.alpha_mode == 0u {
                rgb = fx_unpremult(v).rgb;
            }
            if flag(VIEW_MATTE) {
                v = vec4f(vec3f(a), 1.0);
            } else {
                v = fx_output(rgb, a, flag(FLOAT));
            }
        } else {
            let selected = (vec4u(u.channels) & vec4u(1u, 2u, 4u, 8u)) != vec4u(0u);
            let out = select(base, v, selected);
            v = fx_output(out.rgb, out.a, flag(FLOAT));
        }
    }
    textureStore(dst, p - u.target_origin, v);
}
