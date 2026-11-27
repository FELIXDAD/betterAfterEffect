// Accumulate spatial and colour likelihoods at the stroke's source time.
// A current-to-source flow field carries the seed and colour model together.
struct Segment { ends: vec4f, settings: vec4f }
struct Control {
    current: FxImage, base: FxImage, flow: FxImage,
    roi: vec4i, flags: vec4u, tuning: vec4f,
    segments: array<Segment, 32>
}
@group(0) @binding(0) var<uniform> u: Control;
@group(0) @binding(1) var current: texture_2d<f32>;
@group(0) @binding(2) var base: texture_2d<f32>;
@group(0) @binding(3) var flow: texture_2d<f32>;
@group(0) @binding(4) var scores: texture_2d<f32>;
@group(0) @binding(5) var output: texture_storage_2d<WORKING_FORMAT, write>;

@compute @workgroup_size(8,8)
fn accumulate(@builtin(global_invocation_id) id: vec3u) {
    let pixel = u.roi.xy + vec2i(id.xy);
    if any(pixel >= u.roi.zw) { return; }
    let p = vec2f(pixel) + 0.5;
    var q = p;
    if u.flags.y != 0u { q += fx_bilinear(flow,u.flow,p,true).xy; }
    let rgb = fx_sample(current,u.current,p,true).rgb;
    var score = vec4f(0.0,0.34,0.0,0.0);
    if u.flags.z != 0u { score = textureLoad(scores,id.xy,0); }
    for (var i=0u; i<u.flags.x; i++) {
        let seed = u.segments[i];
        let v = seed.ends.zw - seed.ends.xy;
        let center = seed.ends.xy + v * clamp(dot(q-seed.ends.xy,v)/max(dot(v,v),0.001),0.0,1.0);
        let radius = abs(seed.settings.x);
        let distance_px = distance(center,q);
        let sample_color = fx_sample(base,u.base,center,true).rgb;
        let halfway = fx_sample(base,u.base,mix(center,q,0.5),true).rgb;
        let color_error = distance(sample_color,rgb);
        let barrier = max(distance(sample_color,halfway),distance(halfway,rgb));
        let cost = (color_error * 3.0 + barrier * u.tuning.y)
             + max(0.0,distance_px-radius)/(radius*5.0+u.tuning.x);
        let likelihood = select(exp(-cost), 2.0 + seed.settings.y, distance_px<=radius);
        if seed.settings.x > 0.0 { score.x = max(score.x, likelihood); }
        else { score.y = max(score.y, likelihood); }
    }
    textureStore(output,id.xy,score);
}

@compute @workgroup_size(8,8)
fn resolve(@builtin(global_invocation_id) id: vec3u) {
    let pixel = u.roi.xy + vec2i(id.xy);
    if any(pixel >= u.roi.zw) { return; }
    let sample = fx_sample(current,u.current,vec2f(pixel)+0.5,false);
    var confidence: f32;
    if u.flags.y != 0u {
        confidence = fx_bilinear(scores,u.base,vec2f(pixel)+0.5,true).a - 0.5;
    } else {
        let s = textureLoad(scores,id.xy,0);
        confidence = clamp(s.x-s.y,-0.5,0.5);
    }
    if u.flags.z != 0u {
        textureStore(output,id.xy,vec4f(confidence+0.5)); return;
    }
    var a = smoothstep(-u.tuning.z,u.tuning.z,confidence+u.tuning.w);
    if u.flags.w != 0u { a = 1.0-a; }
    textureStore(output,id.xy,sample*a);
}
