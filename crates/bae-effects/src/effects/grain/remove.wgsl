fn blend_ids()->vec4u {return vec4u(23,9,10,11);}
fn selection_id()->u32 {return 21u;}
fn first_point()->u32 {return 30u;}
fn count_id()->u32 {return 3u;}
fn size_id()->u32 {return 4u;}
fn invert_id()->u32 {return 12u;}
fn placement_id()->u32 {return 70u;}
fn mask_mode_id()->u32 {return 71u;}
fn combine_id()->u32 {return 72u;}
fn blur_id()->u32 {return 68u;}
fn amount_id()->u32 {return 8u;}
fn view_id()->u32 {return 15u;}
fn mask_view()->u32 {return 3u;}
fn sample_box_id()->u32 {return 54u;}
fn center_id()->u32 {return 16u;}
fn width_id()->u32 {return 18u;}
fn height_id()->u32 {return 19u;}
fn box_id()->u32 {return 56u;}
fn box_color_id()->u32 {return 57u;}

fn candidate(q:vec2f,history:bool)->vec3f {
    let bounds=vec2f(u.layer.xy);let hi=vec2f(u.layer.zw)-0.5;
    let p=clamp(q,bounds+0.5,hi);
    if history{return aux_c(p).rgb;}return src(p).rgb;
}
fn process(p:vec2f,c:vec4f,sigma:vec3f)->vec3f {
    let noise=max(sigma,vec3f(0.002));
    let channels=vec3f(value(65),value(66),value(67));
    let strength=value(1)*noise*channels;
    if all(strength==vec3f(0.)){return c.rgb;}
    var sum=c.rgb;var weights=vec3f(1.);
    let step_size=max(0.5,(1.+0.5*value(60))*u.scale);
    let halfwidth=i32(clamp(value(20),1.,6.));
    let temporal=has(2u) && enabled(22);
    let count=select(1,2,temporal);
    let offsets=array<vec2f,5>(vec2f(0.),vec2f(1.,0.),vec2f(-1.,0.),vec2f(0.,1.),vec2f(0.,-1.));
    var center:array<vec3f,5>;
    for(var i=0;i<5;i++){center[i]=candidate(p+offsets[i]*step_size,false);}
    for(var t=0;t<count;t++){for(var y=-halfwidth;y<=halfwidth;y++){for(var x=-halfwidth;x<=halfwidth;x++){
        if x==0 && y==0 && t==0{continue;}
        let q=p+vec2f(f32(x),f32(y))*step_size;
        var dist=vec3f(0.);
        for(var i=0;i<5;i++){
            let delta=center[i]-candidate(q+offsets[i]*step_size,t!=0);
            dist+=delta*delta;
        }
        dist=max(vec3f(0.),dist*0.2-2.*noise*noise);
        let h=max(vec3f(1e-5),strength*select(1.,1.5,enabled(63)));
        var error=dist/(h*h);
        if value(59)==1. {error=vec3f(fx_luma(error));}
        var w=exp(-min(error,vec3f(30.)));
        let rgb=candidate(q,t!=0);
        if t!=0{w*=value(14)*0.01*exp(-value(61)*length(rgb-c.rgb)/max(0.001,length(noise)));}
        weights+=w;sum+=w*rgb;
    }}}
    var result=sum/weights;
    let removed=c.rgb-result;
    result+=mix(removed,vec3f(fx_luma(removed)),saturate(value(64)))*value(62);
    if value(2)>0. && value(50)>0. {
        let offset=vec2f(value(50)*u.scale,0.);
        let detail=result-0.25*(src(p+offset).rgb+src(p-offset).rgb+src(p+offset.yx).rgb+src(p-offset.yx).rgb);
        result+=select(vec3f(0.),detail,abs(detail)>vec3f(value(51)))*value(2);
    }
    return result;
}
