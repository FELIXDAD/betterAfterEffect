fn difference_levels(v: f32, black: f32, white: f32, gamma: f32) -> f32 {
    let x = saturate((v-black/255.0)/max(1e-6,(white-black)/255.0));
    return pow(x,1.0/max(gamma,1e-4));
}

fn difference_view(c: vec4<f32>, view: u32) -> vec4<f32> {
    if view == 1u { return c; }
    let rgb = bae_unpremultiply(c).rgb;
    let key = params.key_color.rgb;
    // The two partial mattes use adjacent axes on the RGB colour wheel.
    let hi = max(key.r,max(key.g,key.b));
    var order = rgb.brg;
    var primary = key.brg;
    if hi == key.g { order=rgb.gbr; primary=key.gbr; }
    else if hi == key.r { order=rgb.rgb; primary=key.rgb; }
    var a = saturate(abs(order.y-primary.y));
    var b = saturate(abs(order.x-primary.x));
    if params.color_matching_accuracy == 2u {
        let span = max(hi-min(key.r,min(key.g,key.b)),1e-6);
        a = saturate(max(a,abs(order.z-primary.z))/span);
        b = saturate(max(b,abs((order.x-order.z)-(primary.x-primary.z))));
    }
    let ca = mix(params.black_out_part_a,params.white_out_part_a,
        difference_levels(a,params.black_in_part_a,params.white_in_part_a,params.gamma_part_a))/255.0;
    let cb = mix(params.black_out_part_b,params.white_out_part_b,
        difference_levels(b,params.black_in_part_b,params.white_in_part_b,params.gamma_part_b))/255.0;
    let raw = ca + (1.0-ca)*cb;
    let matte = difference_levels(raw,params.black_in_matte,params.white_in_matte,params.gamma_matte);
    if view == 8u { return c*matte; }
    var preview = matte;
    if view==2u { preview=a; }
    if view==3u { preview=ca; }
    if view==4u { preview=b; }
    if view==5u { preview=cb; }
    if view==6u { preview=raw; }
    return vec4<f32>(vec3<f32>(preview),1.0);
}

fn result(pos: vec2<i32>) -> vec4<f32> {
    if params.view != 10u { return difference_view(bae_load_input(pos),params.view); }
    let bounds = frame.layer_bounds;
    let size = bounds.max-bounds.min;
    let xy = (pos-bounds.min)*2;
    let right = xy.x >= size.x;
    let bottom = xy.y >= size.y;
    let view = select(select(3u,5u,right),select(7u,8u,right),bottom);
    let p = bounds.min+(xy%size);
    var sum = vec4<f32>(0);
    for (var y=0;y<2;y++) {
        for (var x=0;x<2;x++) {
            sum += difference_view(bae_load_input(min(p+vec2<i32>(x,y),bounds.max-1)),view);
        }
    }
    return sum*0.25;
}

@compute @workgroup_size(8,8)
fn color_difference_key(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if bae_in_target(p) { bae_store(p,bae_depth_clamp(result(p))); }
}
