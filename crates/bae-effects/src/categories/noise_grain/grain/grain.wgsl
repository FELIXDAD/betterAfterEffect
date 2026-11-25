fn auto_point(i:u32)->vec2f {
    let uv=fract((f32(i)+0.5)*vec2f(0.618034,0.414214));
    return mix(vec2f(u.layer.xy),vec2f(u.layer.zw),0.1+0.8*uv);
}
fn difference(rgb:vec3f,reference:vec3f,mode:u32)->f32 {
    let d=abs(rgb-reference);
    switch mode {
        case 4u:{return abs(fract(fx_hsl(rgb).x-fx_hsl(reference).x+0.5)-0.5)*2.;}
        case 5u:{return abs(fx_hsl(rgb).y-fx_hsl(reference).y);}
        case 6u:{return abs(fx_luma(rgb-reference));}
        case 7u,8u,9u:{return d[mode-7u];}
        default:{return max(d.r,max(d.g,d.b));}
    }
}
fn retain_pixel(p:vec2f)->f32 {
    let m=blend_ids();
    let mode=u32(value(m.x));
    var keep=1.;
    if mode>=3u {
        let d=difference(src(p).rgb,u.values[m.y].rgb,mode);
        keep=1.-smoothstep(value(m.z),value(m.z)+max(0.0001,value(m.w)),d);
        if enabled(invert_id()){keep=1.-keep;}
    }
    if has(1u){
        let size=vec2f(u.images[2].size)/u.images[2].scale;let bounds=vec2f(u.layer.zw-u.layer.xy);
        let xy=p-vec2f(u.layer.xy);
        let q=select(xy-0.5*(bounds-size),xy*size/max(bounds,vec2f(1.)),value(placement_id())==2.);
        let c=aux_b(u.images[2].origin+q);
        let channel=u32(value(mask_mode_id()));var mask=select(fx_luma(c.rgb),c.a,channel==2u || channel==4u);
        if channel>=3u{mask=1.-mask;}
        if mode<3u {keep=mask;}else if value(combine_id())==2. {keep=1.-(1.-keep)*(1.-mask);}else{keep*=mask;}
    }
    return keep;
}
fn retain(p:vec2f)->f32 {
    let r=value(blur_id())*u.scale;
    let amount=value(amount_id())*0.01;
    if amount==0.{return 0.;}
    var mask=retain_pixel(p);
    if r>0.{mask=0.5*mask+0.125*(retain_pixel(p+vec2f(r,0.))+retain_pixel(p-vec2f(r,0.))+retain_pixel(p+vec2f(0.,r))+retain_pixel(p-vec2f(0.,r)));}
    return amount*mask;
}
fn sample_boxes(p:vec2f, base:u32)->bool {
    for(var i=0u;i<u32(value(count_id()));i++){
        var center=auto_point(i);
        if value(selection_id())==2.{center=u.values[base+i].xy;}
        let d=abs(p-center)-value(size_id())*u.scale*0.5;
        let edge=max(d.x,d.y);if edge<0. && edge>=-1.{return true;}
    }
    return false;
}
fn evaluate(p:vec2f)->vec4f {
    let c=src(p);
    let view=u32(value(view_id()));
    if view==mask_view(){return gray(retain(p));}
    if view==2u || (mask_view()==4u && view==3u){
        if sample_boxes(p,select(first_point(),70u,view==3u)){return u.values[sample_box_id()];}
        if mask_view()==4u && view==2u && has(2u){
            let uv=(p-vec2f(u.layer.xy))/max(vec2f(u.layer.zw-u.layer.xy),vec2f(1.));
            let ref_pixel=aux_c(u.images[3].origin+uv*vec2f(u.images[3].size)/u.images[3].scale);
            return output(ref_pixel.rgb,ref_pixel.a);
        }
        return output(c.rgb,c.a);
    }
    if view==1u{
        let d=abs(p-u.values[center_id()].xy)-0.5*u.scale*vec2f(value(width_id()),value(height_id()));
        let edge=max(d.x,d.y);if edge>0.{return output(c.rgb,c.a);}
        if edge>=-1. && enabled(box_id()){return u.values[box_color_id()];}
    }
    let sigma=fx_load(map_a,u.images[1],vec2i(0),true).rgb;
    return output(mix(process(p,c,sigma),c.rgb,retain(p)),c.a);
}
