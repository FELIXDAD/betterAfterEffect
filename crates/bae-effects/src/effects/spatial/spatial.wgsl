struct Frame {
    src: vec4<i32>,
    aux: vec4<i32>,
    out: vec4<i32>,
    flags: vec4<u32>,
    arg: array<vec4<f32>, 6>,
}
@group(0) @binding(0) var<uniform> f: Frame;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var aux: texture_2d<f32>;
@group(0) @binding(3) var dst: texture_storage_2d<WORKING_FORMAT, write>;

fn load(tex: texture_2d<f32>, rect: vec4<i32>, p: vec2<i32>) -> vec4<f32> {
    if any(p < rect.xy) || any(p >= rect.zw) { return vec4<f32>(0); }
    return textureLoad(tex, p - rect.xy, 0);
}

fn bilinear(tex: texture_2d<f32>, rect: vec4<i32>, pos: vec2<f32>) -> vec4<f32> {
    let p = vec2<i32>(floor(pos));
    let t = fract(pos);
    let top = mix(load(tex,rect,p),load(tex,rect,p+vec2<i32>(1,0)),t.x);
    let bottom = mix(load(tex,rect,p+vec2<i32>(0,1)),load(tex,rect,p+1),t.x);
    return mix(top,bottom,t.y);
}

fn store(p: vec2<i32>, color: vec4<f32>) {
    var c = color;
    if f.flags.x == 0u {
        let a = saturate(c.a);
        c = vec4<f32>(clamp(c.rgb,vec3<f32>(0),vec3<f32>(a)),a);
    }
    textureStore(dst,p-f.out.xy,c);
}

fn straight(c: vec4<f32>) -> vec3<f32> {
    return c.rgb / select(1.0,c.a,c.a>0.0);
}

fn random(p:vec2<i32>, seed:u32) -> f32 {
    var h = u32(p.x)*0x9e3779b9u ^ u32(p.y)*0x85ebca6bu ^ seed;
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    return f32(h >> 8u) / 16777216.0;
}

// The cardinal large-radius path runs in the shared Gaussian pyramid.
@compute @workgroup_size(8,8)
fn directional(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let sigma = max(0.001, f.arg[0].z);
    let radius = ceil(3.0 * sigma);
    let taps = min(i32(radius), 72);
    let step_px = radius / f32(taps);
    var sum = load(src,f.src,p);
    var norm = 1.0;
    for (var i = 1; i <= taps; i++) {
        let x = f32(i) * step_px;
        let w = exp(-0.5 * x * x / (sigma * sigma));
        let d = x * f.arg[0].xy;
        sum += w * (bilinear(src,f.src,vec2<f32>(p)-d) + bilinear(src,f.src,vec2<f32>(p)+d));
        norm += 2.0 * w;
    }
    store(p,sum/norm);
}

@compute @workgroup_size(8,8)
fn radial(@builtin(global_invocation_id) id:vec3<u32>) {
    let p=f.out.xy+vec2<i32>(id.xy);
    if any(p>=f.out.zw){return;}
    let center=f.arg[0].xy-0.5;
    let v=vec2<f32>(p)-center;
    let spin=f.arg[0].w==1.0;
    let span=select(f.arg[0].z/200.0,radians(f.arg[0].z),spin);
    let density=select(0.5,1.0,f.arg[1].x==2.0);
    let n=clamp(i32(ceil(length(v)*span*density)),1,256);
    let phase=select(0.5,random(p,u32(f.arg[1].y)),f.arg[1].y!=0.0);
    var sum=vec4<f32>(0);
    for(var i=0;i<n;i++){
        let t=(f32(i)+phase)/f32(n);
        var ray=v*(1.0+span*(t-0.5));
        if spin{
            let a=(t-0.5)*span;
            ray=vec2<f32>(v.x*cos(a)-v.y*sin(a),v.x*sin(a)+v.y*cos(a));
        }
        sum+=bilinear(src,f.src,clamp(center+ray, vec2<f32>(f.src.xy), vec2<f32>(f.src.zw-1)));
    }
    store(p,sum/f32(n));
}

// Integrate fractional coverage on each axis before expanding the block grid.
fn mosaic_input(p:vec2<i32>) -> vec4<f32> {
    // AE Mosaic has an integer (up to 16 bpc) working range.
    let c = textureLoad(src,p,0);
    let a = saturate(c.a);
    return vec4<f32>(clamp(c.rgb,vec3<f32>(0),vec3<f32>(a)),a);
}
fn overlap_1d(i: i32, lo: f32, hi: f32) -> f32 {
    return min(f32(i+1), hi) - max(f32(i), lo);
}

@compute @workgroup_size(8, 8)
fn mosaic_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let width = f32(f.src.z - f.src.x) / f.arg[0].x;
    let lo = f32(p.x) * width;
    let hi = min(f32(f.src.z - f.src.x), f32(p.x+1) * width);
    var sum = vec4<f32>(0);
    for (var x = i32(floor(lo)); x < i32(ceil(hi)); x++) {
        sum += overlap_1d(x, lo, hi) * mosaic_input(vec2<i32>(x,p.y));
    }
    store(p, sum / (hi-lo));
}

@compute @workgroup_size(8, 8)
fn mosaic_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let height = f32(f.src.w) / f.arg[0].y;
    let lo = f32(p.y) * height;
    let hi = min(f32(f.src.w), f32(p.y+1) * height);
    var sum = vec4<f32>(0);
    for (var y = i32(floor(lo)); y < i32(ceil(hi)); y++) {
        sum += overlap_1d(y, lo, hi) * textureLoad(src, vec2<i32>(p.x, y), 0);
    }
    store(p, sum / (hi-lo));
}

fn cell_color(cell:vec2<i32>, grid:vec2<i32>, size:vec2<i32>) -> vec4<f32> {
    if any(cell < vec2(0)) || any(cell >= grid) { return vec4(0.); }
    if f.arg[0].z != 0.0 {
        // AE Sharp Colors picks the last texel covered by the cell.
        let last = ((cell+1)*size-1)/grid;
        return mosaic_input(last);
    }
    return textureLoad(aux,cell,0);
}

@compute @workgroup_size(8,8)
fn mosaic_output(@builtin(global_invocation_id) id:vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let size = f.src.zw-f.src.xy;
    let grid = vec2<i32>(f.arg[0].xy);
    let pos = p-f.src.xy;
    // Best filters one destination pixel using the integer block width. The
    // resulting footprint can cross the outside edge as well as an inner seam.
    let block = vec2<f32>(size) / vec2<f32>(grid);
    let center = (vec2<f32>(pos) + 0.5) / block;
    let width = 1. / floor(block);
    let cell = vec2<i32>(floor(center - 0.5 * width));
    let next = cell + 1;
    let frac = saturate((center + 0.5 * width - vec2<f32>(next)) / width);
    let a = cell_color(cell,grid,size);
    if all(frac == vec2(0.)) { store(p, a); return; }
    let b = cell_color(vec2<i32>(next.x,cell.y),grid,size);
    let c = cell_color(vec2<i32>(cell.x,next.y),grid,size);
    let d = cell_color(next,grid,size);
    store(p,mix(mix(a,b,frac.x),mix(c,d,frac.x),frac.y));
}

@compute @workgroup_size(8,8)
fn unsharp(@builtin(global_invocation_id) id:vec3<u32>) {
    let p=f.out.xy+vec2<i32>(id.xy);
    if any(p>=f.out.zw){return;}
    let c=load(src,f.src,p);
    let detail=straight(c)-straight(load(aux,f.aux,p));
    let luma=dot(detail,vec3<f32>(0.299,0.587,0.114));
    let threshold = f.arg[0].y;
    let channel_detail = sign(detail) * max(abs(detail)-threshold, vec3<f32>(0));
    let color_detail = detail * max(0.0,1.0-threshold/max(abs(luma),1e-6));
    let correction = select(channel_detail,color_detail,f.arg[0].z!=0.0);
    store(p,vec4<f32>(c.rgb+correction*f.arg[0].x*c.a,c.a));
}

@compute @workgroup_size(8,8)
fn shadow(@builtin(global_invocation_id) id:vec3<u32>) {
    let p=f.out.xy+vec2<i32>(id.xy);
    if any(p>=f.out.zw){return;}
    let a=bilinear(aux,f.aux,vec2<f32>(p)-f.arg[0].xy).a*f.arg[0].z;
    let shadow_color=vec4<f32>(f.arg[1].rgb*a,a);
    let c=select(load(src,f.src,p),vec4<f32>(0),f.arg[0].w!=0.0);
    store(p,c+shadow_color*(1.0-c.a));
}

fn glow_ramp(x: f32) -> vec3<f32> {
    let cycle = fract(x * f.arg[1].w + f.arg[2].x);
    var t = cycle;
    if f.arg[1].z >= 3.0 { t = 1.0 - abs(2.0 * cycle - 1.0); }
    if f.arg[1].z == 2.0 || f.arg[1].z == 4.0 { t = 1.0 - t; }
    let mid = clamp(f.arg[2].y,0.009995,0.990005);
    t = select(0.5 + 0.5*(t-mid)/(1.0-mid), 0.5*t/mid, t<mid);
    return mix(f.arg[4].rgb, f.arg[3].rgb, t);
}

@compute @workgroup_size(8,8)
fn glow_extract(@builtin(global_invocation_id) id:vec3<u32>) {
    let p = f.out.xy + vec2<i32>(id.xy);
    if any(p >= f.out.zw) { return; }
    let c = load(src,f.src,p);
    let alpha_channel = f.arg[0].x == 1.0;
    let signal = select(straight(c),vec3<f32>(c.a),alpha_channel);
    let threshold = f.arg[0].y;
    // AE's radius-zero ramp opens 16 code values either side of the
    // threshold (the transition narrows at the ends of the 8-bit range).
    let low = max(0.0, threshold - 16.0/255.0);
    let high = min(1.0, threshold + 16.0/255.0);
    let bright = saturate((signal - low)/(high-low));
    let a = select(c.a, 1.0, alpha_channel);
    store(p,vec4<f32>(bright*a,select(c.a,bright.r,alpha_channel)));
}

fn luma(c: vec3<f32>) -> f32 { return dot(c,vec3<f32>(0.299,0.587,0.114)); }
fn chroma(c: vec3<f32>) -> f32 { return max(c.r,max(c.g,c.b))-min(c.r,min(c.g,c.b)); }
fn with_chroma(c: vec3<f32>, s: f32) -> vec3<f32> {
    return (c-min(c.r,min(c.g,c.b)))*(s/max(1e-6,chroma(c)));
}
fn with_luma(c: vec3<f32>, y: f32) -> vec3<f32> {
    var v = c+y-luma(c);
    let lo = min(v.r,min(v.g,v.b));
    let hi = max(v.r,max(v.g,v.b));
    if lo<0.0 && lo<y { v=y+(v-y)*(y/(y-lo)); }
    if hi>1.0 && hi>y { v=y+(v-y)*((1.0-y)/(hi-y)); }
    return v;
}
fn hard_light(b:vec3<f32>,s:vec3<f32>) -> vec3<f32> {
    return select(1.0-2.0*(1.0-b)*(1.0-s),2.0*b*s,s<=vec3<f32>(0.5));
}
fn burn_color(b: vec3<f32>,s: vec3<f32>)->vec3<f32> {
    return select(saturate(1.0-(1.0-b)/max(s,vec3<f32>(1e-6))),vec3<f32>(1),b>=vec3<f32>(1));
}
fn overlap(op:u32,b:vec3<f32>,s:vec3<f32>)->vec3<f32>{
    switch op {
        case 4u: { return b*s; }
        case 6u: { return b+s-b*s; }
        case 7u: { return hard_light(s,b); }
        case 8u: {
            let d=select(sqrt(max(b,vec3<f32>(0))),((16.0*b-12.0)*b+4.0)*b,b<=vec3<f32>(0.25));
            return select(b+(2.0*s-1.0)*(d-b),b-(1.0-2.0*s)*b*(1.0-b),s<=vec3<f32>(0.5));
        }
        case 9u: { return hard_light(b,s); }
        case 10u: { return min(b,s); }
        case 11u: { return max(b,s); }
        case 12u: { return abs(b-s); }
        case 13u: { return with_luma(with_chroma(s,chroma(b)),luma(b)); }
        case 14u: { return with_luma(with_chroma(b,chroma(s)),luma(b)); }
        case 15u: { return with_luma(s,luma(b)); }
        case 16u: { return with_luma(b,luma(s)); }
        case 17u: { return 1.0-burn_color(1.0-b,1.0-s); }
        case 18u: { return burn_color(b,s); }
        case 19u: { return b+s-2.0*b*s; }
        default: { return s; }
    }
}

fn place_original(p:vec2<i32>,result:vec4<f32>,orig:vec4<f32>,placement:u32) {
    var color = result;
    if placement == 1u { color = orig + result * (1.0-orig.a); }
    if placement == 2u { color = result + orig * (1.0-result.a); }
    store(p,color);
}

@compute @workgroup_size(8,8)
fn glow_composite(@builtin(global_invocation_id) id:vec3<u32>) {
    let p = f.out.xy+vec2<i32>(id.xy);
    if any(p>=f.out.zw) { return; }
    let orig = load(src,f.src,p);
    let soft = load(aux,f.aux,p)*f.arg[0].z;
    var glow = vec4<f32>(soft.rgb,saturate(soft.a));
    if f.arg[1].y == 2.0 {
        glow = vec4<f32>(glow_ramp(soft.a) * soft.a,glow.a);
    }
    let placement = u32(f.arg[0].w);
    let op = u32(f.arg[1].x);
    if op == 1u { place_original(p,glow,orig,placement); return; }
    var s = glow;
    let b = orig;
    if op>=20u && op<=23u {
        var mask=saturate(select(s.a,luma(s.rgb),op==21u || op==23u));
        if op>=22u {mask=1.0-mask;}
        place_original(p,b*mask,orig,placement);return;
    }
    if op==5u {
        if random(p,0u)>=s.a { place_original(p,b,orig,placement); return; }
        s=vec4<f32>(straight(s),1.0);
    }
    var a=s.a+b.a*(1.0-s.a);
    let blended=overlap(op,straight(b),straight(s));
    var rgb=s.rgb*(1.0-b.a)+b.rgb*(1.0-s.a)+s.a*b.a*blended;
    if op==3u {rgb=s.rgb+b.rgb;}
    if op==24u {rgb=s.rgb+b.rgb*(1.0-s.a);}
    if op==25u {a=min(1.0,s.a+b.a);rgb=s.rgb+straight(b)*(a-s.a);}
    place_original(p,vec4<f32>(rgb,a),orig,placement);
}

@compute @workgroup_size(8,8)
fn matte_choke(@builtin(global_invocation_id) id:vec3<u32>) {
    let p=f.out.xy+vec2<i32>(id.xy);
    if any(p>=f.out.zw) { return; }
    let args=f.arg[0];
    let vertical=args.w>0.0;
    let axis=select(vec2<i32>(1,0),vec2<i32>(0,1),vertical);
    let r=select(0,i32(ceil(args.x+2.0)),args.x>0.0);
    let sigma=max(0.45,select(0.35*args.x+0.2,0.25*args.x+0.98,args.x>8.0));
    var sum=load(src,f.src,p);
    var norm=1.0;
    for(var i=1;i<=r;i++) {
        var w=exp(-0.5*f32(i*i)/(sigma*sigma));
        let a=p-axis*i;
        let b=p+axis*i;
        // Measured impulse weights at the default radius.
        if args.x==4.0 {
            let weights=array<f32,7>(63.0,52.0,31.0,11.0,2.0,0.0,0.0);
            w=weights[u32(i)]/weights[0];
        }
        sum+=w*(load(src,f.src,a)+load(src,f.src,b));
        norm+=w*(f32(all(a>=f.src.xy))+f32(all(b<f.src.zw)));

    }
    let b=sum/norm;
    if !vertical {store(p,b); return;}
    let width=clamp(2.0*floor(255.0*args.z),1.0,255.0);
    let alpha=saturate((b.a-args.y-0.5)*(255.0/width)+0.5);
    let original=load(aux,f.aux,p);
    store(p,vec4<f32>(select(straight(b),straight(original),original.a>0.0)*alpha,alpha));
}
