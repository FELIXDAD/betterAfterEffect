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

// Select the middle sample independently in each channel. Counts include
// duplicates. The adjacent real samples also narrow the next search interval.
fn rank_pixel(p: vec2<i32>, threshold: f32, clip: bool) {
    if !bae_in_target(p) { return; }
    let rect = bae_rect_input();
    let r = i32(round(bae_scale(f32(params.radius))));
    let start = select(p - r, max(rect.min, p - r), clip);
    let end = select(p + r + 1, min(rect.max, p + r + 1), clip);
    let size = end - start;
    if r == 1 && all(size == vec2<i32>(3)) {
        store_rank(p, median3x3(p), threshold);
        return;
    }
    let rank = vec4<u32>(u32(size.x * size.y) / 2u);
    var lo = rank_load(p);
    var hi = lo;
    for (var y = start.y; y < end.y; y++) {
        for (var x = start.x; x < end.x; x++) {
            let value = rank_load(vec2<i32>(x, y));
            lo = min(lo, value);
            hi = max(hi, value);
        }
    }
    while any(lo < hi) {
        // If two adjacent floats round up at the midpoint, test the lower one.
        let mid = 0.5 * lo + 0.5 * hi;
        let pivot = select(mid, lo, mid >= hi);
        var count = vec4<u32>(0);
        var below = lo;
        var above = hi;
        for (var y = start.y; y < end.y; y++) {
            for (var x = start.x; x < end.x; x++) {
                let value = rank_load(vec2<i32>(x, y));
                let lower = value <= pivot;
                count += select(vec4<u32>(0), vec4<u32>(1), lower);
                below = select(below, max(below, value), lower);
                above = select(min(above, value), above, lower);
            }
        }
        let upper = count <= rank;
        lo = select(lo, above, upper);
        hi = select(below, hi, upper);
    }
    store_rank(p, lo, threshold);
}
