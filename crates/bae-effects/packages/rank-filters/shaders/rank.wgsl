// 32 adjacent outputs share a moving histogram. Each step exchanges the
// two ends of each scanline in the square window instead of rebuilding its area.
var<workgroup> bins: array<atomic<u32>,2048>;
var<workgroup> coarse: array<atomic<u32>,128>;
var<workgroup> counts: array<vec4<u32>,32>;
var<workgroup> chosen: array<atomic<u32>,4>;

fn rank_load(p: vec2<i32>) -> vec4<f32> {
    let rect = bae_rect_input();
    return bae_input_straight(bae_load_input(clamp(p,rect.min,rect.max-1)));
}

fn middle(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>) -> vec4<f32> {
    return max(min(a,b),min(max(a,b),c));
}

fn median3x3(p: vec2<i32>) -> vec4<f32> {
    var low = vec4<f32>(-1e30);
    var high = vec4<f32>(1e30);
    var mid: array<vec4<f32>,3>;
    for (var row=0; row<3; row++) {
        let q = p+vec2<i32>(0,row-1);
        let a=rank_load(q-vec2<i32>(1,0));
        let b=rank_load(q);
        let c=rank_load(q+vec2<i32>(1,0));
        low=max(low,min(a,min(b,c)));
        high=min(high,max(a,max(b,c)));
        mid[u32(row)]=middle(a,b,c);
    }
    return middle(low,middle(mid[0],mid[1],mid[2]),high);
}

fn store_rank(p: vec2<i32>, filtered: vec4<f32>, threshold: f32) {
    let src = rank_load(p);
    let delta = abs(filtered-src);
    let rgb = max(delta.r,max(delta.g,delta.b));
    var c = select(src,filtered,max(rgb,select(0.0,delta.a,params.operate_on_alpha_channel!=0u))>threshold);
    if params.operate_on_alpha_channel == 0u { c.a = src.a; }
    bae_store(p,bae_depth_clamp(bae_premultiply(c)));
}

fn rank_update(p: vec2<i32>, increment: u32, clip: bool) {
    let rect = bae_rect_input();
    if clip && (any(p<rect.min) || any(p>=rect.max)) { return; }
    // Multiples of 1/255 have exact bins. Finer input has half an 8-bit level
    // between neighbouring bins, independent of the requested radius.
    let v = vec4<u32>(round(saturate(rank_load(p))*510.0));
    for (var ch=0u; ch<4u; ch++) {
        atomicAdd(&bins[512u*ch+v[ch]],increment);
        atomicAdd(&coarse[ch*32u+(v[ch]>>4u)],increment);
    }
}

fn rank_row(group: vec3<u32>, lane: u32, threshold: f32, clip: bool) {
    let start = frame.target_rect.min+vec2<i32>(i32(group.x)*32,i32(group.y));
    let radius = i32(round(bae_scale(f32(params.radius))));
    let rect = bae_rect_input();
    let interior = all(start-1>=rect.min) && all(start+vec2<i32>(33,2)<=rect.max);
    if radius==0 || (radius==1 && (!clip || interior)) {
        let p = start+vec2<i32>(i32(lane),0);
        if bae_in_target(p) {
            if radius==0 { bae_store(p,bae_premultiply(rank_load(p))); }
            else { store_rank(p,median3x3(p),threshold); }
        }
        return;
    }
    for (var i=lane; i<2048u; i+=32u) { atomicStore(&bins[i],0u); }
    for (var i=lane; i<128u; i+=32u) { atomicStore(&coarse[i],0u); }
    workgroupBarrier();
    for (var y=-radius; y<=radius; y++) {
        for (var x=i32(lane)-radius; x<=radius; x+=32) { rank_update(start+vec2<i32>(x,y),1u,clip); }
    }

    let width=min(32,frame.target_rect.max.x-start.x);
    for (var x=0; x<width; x++) {
        workgroupBarrier();
        let p=start+vec2<i32>(x,0);
        let size=select(vec2<i32>(2*radius+1),min(rect.max,p+radius+1)-max(rect.min,p-radius),clip);
        let half=u32(size.x*size.y)/2u;
        var block=vec4<u32>(0);
        for (var ch=0u;ch<4u;ch++) { block[ch]=atomicLoad(&coarse[32u*ch+lane]); }
        counts[lane]=block;
        workgroupBarrier();
        var before=vec4<u32>(0);
        for (var i=0u;i<lane;i++) { before+=counts[i]; }
        for (var ch=0u;ch<4u;ch++) {
            if before[ch]<=half && before[ch]+block[ch]>half {
                var total=before[ch];
                for (var i=0u;i<16u;i++) {
                    total+=atomicLoad(&bins[512u*ch+lane*16u+i]);
                    if total>half { atomicStore(&chosen[ch],lane*16u+i); break; }
                }
            }
        }
        workgroupBarrier();
        if lane==0u {
            var result=vec4<f32>(0);
            for (var ch=0u;ch<4u;ch++) { result[ch]=f32(atomicLoad(&chosen[ch]))/510.0; }
            store_rank(p,result,threshold);
        }
        if x+1<width {
            for (var y=i32(lane)-radius;y<=radius;y+=32) {
                rank_update(p+vec2<i32>(-radius,y),0xffffffffu,clip);
                rank_update(p+vec2<i32>(radius+1,y),1u,clip);
            }
        }
    }
}
