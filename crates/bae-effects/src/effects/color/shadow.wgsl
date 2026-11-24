struct Params {
    origin:vec2<i32>, shadow_origin:vec2<i32>, highlight_origin:vec2<i32>,
    automatic:u32, histogram:u32, amount:vec2<f32>, widths:vec2<f32>,
    color:f32, contrast:f32, black:f32, white:f32, blend:f32,
}
@group(0) @binding(0) var<uniform> u:Params;
@group(0) @binding(1) var dst:texture_storage_2d<WORKING_FORMAT,write>;
@group(0) @binding(2) var src:texture_2d<f32>;
@group(0) @binding(3) var shadows:texture_2d<f32>;
@group(0) @binding(4) var highlights:texture_2d<f32>;
@group(0) @binding(5) var stats:texture_2d<f32>;

fn luma(c:vec3<f32>)->f32 { return dot(c,vec3<f32>(0.299,0.587,0.114)); }
fn intensity(c:vec4<f32>)->f32 { return luma(c.rgb)/max(c.a,1e-8); }

var<workgroup> bins:array<atomic<u32>,256>;
@compute @workgroup_size(8,8)
fn histogram(@builtin(global_invocation_id) id:vec3<u32>, @builtin(workgroup_id) group:vec3<u32>, @builtin(local_invocation_index) lane:u32) {
    for(var k=0u;k<4u;k++) { atomicStore(&bins[lane*4u+k],0u); }
    workgroupBarrier();
    let size=textureDimensions(src);
    for(var y=id.y;y<size.y;y+=128u) {
        for(var x=id.x;x<size.x;x+=128u) {
            let c=textureLoad(src,vec2<u32>(x,y),0);
            if c.a>0. { atomicAdd(&bins[u32(round(clamp(intensity(c),0.,1.)*255.))],1u); }
        }
    }
    workgroupBarrier();
    var counts=vec4<f32>(0);
    for(var k=0u;k<4u;k++) { counts[k]=f32(atomicLoad(&bins[lane*4u+k]))/1024.; }
    textureStore(dst,vec2<u32>(lane,group.y*16u+group.x),counts);
}

var<workgroup> sums:array<vec4<f32>,64>;
@compute @workgroup_size(8,8)
fn percentiles(@builtin(local_invocation_index) lane:u32) {
    var sum=vec4<f32>(0);
    for(var row=0u;row<256u;row++) { sum+=textureLoad(src,vec2<u32>(lane,row),0); }
    sums[lane]=sum;
    workgroupBarrier();
    if lane==0u {
        var count=0.; var weight=0.;
        for(var i=0u;i<256u;i++) {
            let n=sums[i/4u][i%4u];
            count+=n; weight+=n*f32(i)/255.;
        }
        var cumulative=0.; var lo=0.; var hi=1.; var first=255u; var last=0u;
        for(var i=0u;i<256u;i++) {
            let n = sums[i/4u][i%4u];
            if n > 0.0 { first = min(first,i); last = max(last,i); }
            cumulative+=n;
            if u.black>0. && cumulative<count*u.black { lo=f32(i+1u)/255.; }
            if u.white>0. && cumulative<count*(1.-0.5*u.white) { hi=f32(i+1u)/255.; }
        }
        if u.black > 0.0 { lo = max(0.0, lo-f32(first)/255.0); }
        if u.white > 0.0 { hi = min(1.0, hi+1.0-f32(last)/255.0); }
        if hi<=lo { lo=0.; hi=1.; }
        let mean=weight/max(count,1e-8);
        textureStore(dst,vec2<i32>(0),vec4<f32>(lo,hi,clamp((0.55-mean)*1.5,0.,1.),clamp(mean-0.36,0.,1.)));
    }
}

// Exposure on neutral ramps at radii zero, fitted to the AE 26 amount and
// tonal width sweeps. Radius changes the local brightness used by the mask.
fn local_stops(amount: f32, width: f32, local: f32) -> f32 {
    let level = clamp(local, 0.0, 1.0);
    let spread = (0.086 + 0.43*width) * (1.0-0.16*(amount-0.5));
    return 2.97*pow(amount, 0.66) * exp(-pow(level/spread, 1.64)) * (1.0 + 2.0*level);
}

@compute @workgroup_size(8,8)
fn shadow_highlight(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=textureDimensions(dst)) { return; }
    let xy=vec2<i32>(id.xy);
    let c=textureLoad(src,xy+u.origin,0);
    let rgb=c.rgb/max(c.a,1e-8);
    let l=luma(rgb);
    let s=intensity(textureLoad(shadows,xy+u.shadow_origin,0));
    let h=intensity(textureLoad(highlights,xy+u.highlight_origin,0));
    let stats_value=select(vec4<f32>(0,1,0,0),textureLoad(stats,vec2<i32>(0),0),u.histogram!=0u);
    let amt=select(u.amount,stats_value.zw,u.automatic!=0u);
    let exposure = local_stops(amt.x, u.widths.x, s) - local_stops(amt.y, u.widths.y, 1.0-h);
    let gain = exp2(exposure);
    let mapped = rgb*gain / max(vec3<f32>(1e-6), 1.0 + (gain-1.0)*rgb);
    let adjusted = luma(mapped);
    var tone = clamp((adjusted-stats_value.x)/(stats_value.y-stats_value.x), 0.0, 1.0);
    let mid = 2.0*tone - 1.0;
    tone += 0.75*u.contrast * mid * pow(1.0-pow(abs(mid), 1.63), 2.8);
    let chroma = (rgb-l) * (1.0 + 7.5*u.color*abs(adjusted-l));
    let low = min(min(chroma.r,chroma.g),chroma.b);
    let high = max(max(chroma.r,chroma.g),chroma.b);
    let limit = min(tone/max(-low,1e-6), (1.0-tone)/max(high,1e-6));
    let correction = tone + chroma*min(1.0,limit);
    textureStore(dst,xy,vec4<f32>(mix(correction,rgb,u.blend)*c.a,c.a));
}
