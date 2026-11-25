fn blend_ids()->vec4u {return vec4u(147,144,145,146);}
fn selection_id()->u32 {return 18u;}
fn first_point()->u32 {return 50u;}
fn count_id()->u32 {return 10u;}
fn size_id()->u32 {return 11u;}
fn invert_id()->u32 {return 148u;}
fn placement_id()->u32 {return 151u;}
fn mask_mode_id()->u32 {return 152u;}
fn combine_id()->u32 {return 153u;}
fn blur_id()->u32 {return 149u;}
fn amount_id()->u32 {return 143u;}
fn view_id()->u32 {return 2u;}
fn mask_view()->u32 {return 4u;}
fn sample_box_id()->u32 {return 142u;}
fn center_id()->u32 {return 14u;}
fn width_id()->u32 {return 15u;}
fn height_id()->u32 {return 16u;}
fn box_id()->u32 {return 117u;}
fn box_color_id()->u32 {return 115u;}

fn animation(p:vec2f,channel:u32,size:f32)->f32 {
    let frame=u.time*u.fps*value(155);
    let seed=u32(value(17))+channel*41u+bitcast<u32>(i32(floor(frame)))*11u;
    let uv=p/max(vec2f(0.01),size*value(119)*vec2f(value(120),1.)*u.scale);
    let a=grain_lattice(uv,seed,value(232));
    if !enabled(154){return a;}
    return mix(a,grain_lattice(uv,seed+11u,value(232)),smoothstep(0.,1.,fract(frame)));
}
fn process(p:vec2f,c:vec4f,measured:vec3f)->vec3f {
    var sigma=measured;
    if value(118)>0. {
        let existing=fx_load(map_a,u.images[1],vec2i(1,0),true).rgb;
        sigma=sqrt(max(vec3f(0.),sigma*sigma-existing*existing*value(118)*0.01));
    }
    var grain=vec3f(animation(p,0u,value(122)),animation(p,1u,value(123)),animation(p,2u,value(124)));
    grain=mix(vec3f(fx_luma(grain)),grain,select(value(20),0.,enabled(31)));
    let l=fx_luma(c.rgb);let mid=value(131);
    let low=saturate(1.-l/max(mid,1e-5));let high=saturate((l-mid)/max(1.-mid,1e-5));
    let response=low*value(128)*vec3f(value(132),value(135),value(138))+
        (1.-low-high)*value(129)*vec3f(value(133),value(136),value(139))+
        high*value(130)*vec3f(value(134),value(137),value(140));
    grain*=2.*sigma*value(21)*response*vec3f(value(22),value(23),value(24))*mix(vec3f(1.),u.values[25].rgb,value(114)*0.01);
    switch u32(value(29)){
        case 1u:{grain*=sqrt(max(vec3f(0.01),4.*c.rgb*(1.-c.rgb)));}
        case 2u:{grain*=c.rgb;}
        case 4u:{grain*=1.-c.rgb;}
        case 5u:{grain*=2.*min(c.rgb,1.-c.rgb);}
        default:{}
    }
    return c.rgb+grain;
}
