// Each workgroup samples the disk once; its 64 output pixels share the mean.
var<workgroup> colors: array<vec4f,64>;
var<workgroup> weights: array<f32,64>;

@compute @workgroup_size(8,8)
fn eyedropper_fill(@builtin(global_invocation_id) id:vec3u,@builtin(local_invocation_index) lane:u32) {
    let radius=bae_scale(params.sample_radius);
    let step_size=max(1.0,ceil(radius/16.0));
    let count=i32(floor(radius/step_size));
    let side=u32(2*count+1);
    var sum=vec4f(0);
    var weight=0.0;
    for(var i=lane;i<side*side;i+=64u){
        let xy=vec2f(vec2i(i32(i%side),i32(i/side))-vec2i(count))*step_size;
        if dot(xy,xy)>radius*radius {continue;}
        let point=vec2i(floor(params.sample_point+xy));
        if !bae_rect_contains(frame.layer_bounds,point){continue;}
        let c=bae_input_straight(bae_load_input(point));
        if params.average_pixel_color==1u && c.a==0.0 {continue;}
        sum+=select(c,bae_premultiply(c),params.average_pixel_color==3u);
        weight+=1.0;
    }
    colors[lane]=sum;
    weights[lane]=weight;
    for(var stride=32u;stride>0u;stride/=2u){
        workgroupBarrier();
        if lane<stride{colors[lane]+=colors[lane+stride];weights[lane]+=weights[lane+stride];}
    }
    workgroupBarrier();
    let p=bae_target_pixel(id);
    if !bae_in_target(p){return;}
    let src=bae_premultiply(bae_input_straight(bae_load_input(p)));
    let mean=colors[0]/max(weights[0],1.0);
    let a=select(select(1.0,mean.a,params.average_pixel_color==4u),src.a,params.maintain_original_alpha!=0u);
    bae_store(p,bae_depth_clamp(mix(bae_premultiply(vec4f(mean.rgb,a)),src,params.blend_with_original)));
}
