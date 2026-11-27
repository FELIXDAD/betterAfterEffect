// Accumulate spatial and colour likelihoods at the stroke's source time.
// A current-to-source flow field carries the seed and colour model together.
struct Segment { ends: vec4f, settings: vec4f }
struct Control {
    current: FxImage, base: FxImage, flow: FxImage,
    roi: vec4i, flags: vec4u, tuning: vec4f,
    edge: vec4f, soften: vec4f, cleaning: vec4f, padding: vec4f,
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
        if seed.settings.z != 0.0 {
            let coverage = 1.0-smoothstep(max(0.0,radius-1.0),radius+1.0,distance_px);
            // Alt subtracts from the refine region.
            score.z = select(max(score.z,coverage),min(score.z,1.0-coverage),seed.settings.x<0.0);
            continue;
        }
        let sample_color = fx_sample(base,u.base,center,true).rgb;
        let halfway = fx_sample(base,u.base,mix(center,q,0.5),true).rgb;
        let reference = fx_sample(base,u.base,q,true).rgb;
        let temporal_error = distance(sample_color, reference);
        let color_error = mix(distance(sample_color,rgb),temporal_error,u.edge.z);
        let anchor_cost = (3.0*temporal_error + max(0.0,distance_px-radius)/(radius*5.0+u.tuning.x));
        let anchor = exp(-anchor_cost);
        // Signed likelihood in the motion-compensated reference frame.
        if seed.settings.x > 0.0 {score.w=max(score.w,anchor);}
        let barrier = max(distance(sample_color,halfway),distance(halfway,rgb));
        let cost = (color_error * 3.0 + barrier * u.tuning.y)
             + max(0.0,distance_px-radius)/(radius*5.0+u.tuning.x);
        let likelihood = select(exp(-cost), 2.0 + seed.settings.y, distance_px<=radius);
        if seed.settings.x > 0.0 { score.x = max(score.x, likelihood); }
        else { score.y = max(score.y, likelihood); }
    }
    textureStore(output,id.xy,score);
}

// (signed confidence, painted trimap coverage, reference-frame confidence).
fn matte_at(p: vec2f) -> vec3f {
    if u.flags.y != 0u {
        let c = fx_bilinear(scores,u.base,p,true);
        // 2: frozen before the RGB channels, the confidence is in alpha.
        if u.flags.y == 2u { return vec3f(c.a-0.5,0.0,c.a); }
        return vec3f(c.r-0.5,c.g,c.b);
    }
    let coord=clamp(vec2i(floor(p))-u.roi.xy,vec2i(0),u.roi.zw-u.roi.xy-1);
    let s=textureLoad(scores,coord,0);
    return vec3f(clamp(s.x-s.y,-0.5,0.5),s.z,clamp(s.w-s.y+0.5,0.0,1.0));
}

@compute @workgroup_size(8,8)
fn resolve(@builtin(global_invocation_id) id:vec3u) {
    let pixel=u.roi.xy+vec2i(id.xy);
    if any(pixel>=u.roi.zw){return;}
    let p=vec2f(pixel)+0.5;
    let m=matte_at(p);
    if u.flags.z!=0u {
        let a=m.x+0.5;
        textureStore(output,id.xy,vec4f(a,m.y,m.z,1.0)); return;
    }
    let src=fx_sample(current,u.current,p,true);
    var a=smoothstep(-u.tuning.z,u.tuning.z,m.x+u.tuning.w);
    var color=src.rgb;
    let region=select(0.0,max(m.y,select(0.0,1.0,u.edge.x>0.0)),u.edge.y!=0.0);
    var cleaned=0.0;
    if region>0.0 {
        // Locate samples on opposite sides of the matte. The minimum and
        // maximum confidences choose local background and foreground models.
        var low=m.x; var high=m.x;
        var bg=color; var fg=color;
        var sum=0.0; var weight=0.0;
        let reach=max(2.0, max(u.edge.x,5.0)+u.soften.x+u.soften.y+u.cleaning.y*u.cleaning.z);
        for(var ring=1;ring<=3;ring++){
            let r=f32(ring)*reach/3.0;
            for(var k=0;k<8;k++){
                let angle=f32(k)*0.785398163;
                let q=p+r*vec2f(cos(angle),sin(angle));
                let v=matte_at(q).x;
                let c=fx_sample(current,u.current,q,true).rgb;
                if v<low {low=v;bg=c;}
                if v>high {high=v;fg=c;}
                let w=1.0/(1.0+distance(color,c)*8.0);
                sum+=w*smoothstep(-u.tuning.z,u.tuning.z,v+u.tuning.w);weight+=w;
            }
        }
        if low<0.0 && high>0.0 {
            let axis=fg-bg;
            let power=dot(axis,axis);
            let projected=clamp(dot(color-bg,axis)/max(power,0.00001),0.0,1.0);
            var edge_alpha=mix(a,projected,smoothstep(0.0001,0.01,power));
            edge_alpha=mix(edge_alpha,m.z,u.edge.w*0.65);
            edge_alpha=mix(edge_alpha,sum/max(weight,0.001),1.0-exp(-0.08*(u.soften.x+u.soften.y)));
            edge_alpha=clamp((edge_alpha-0.5)*(1.0+u.soften.z*5.0)+0.5+u.soften.w,0.0,1.0);
            a=mix(a,edge_alpha,region);
            let clean=region*u.cleaning.x*(1.0-a);
            let estimated=clamp((color-(1.0-a)*bg)/max(a,0.05),min(fg,color)-0.1,max(fg,color)+0.1);
            color=mix(color,estimated,clean);
            cleaned=clean;
        }
    }
    if u.cleaning.w>0.0 && u.cleaning.x>0.0 {textureStore(output,id.xy,vec4f(vec3f(cleaned),1.0));return;}
    if u.flags.w!=0u {a=1.0-a;}
    textureStore(output,id.xy,vec4f(color*src.a*a,src.a*a));
}
