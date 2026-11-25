fn effect(p:vec2f)->vec4f{
    let base=fx_unpremult(source(p));var c=base;
    if v(2)!=0. && u.images[1].size.x!=0u{c=fx_unpremult(layer_sample(map1,u.images[1],p,false));}
    let input_mode=i32(v(5));let output_mode=i32(v(6));let hsl=fx_hsl(c.rgb);
    let y=fx_luma(c.rgb);var rgb=base.rgb;var a=base.a;
    if input_mode<=5 {
        switch input_mode {
            case 1:{rgb=hsl.xzy;}
            case 2:{rgb=fx_rgb(c.rbg);}
            case 3:{rgb=vec3f(y,.5+(c.b-y)/1.772,.5+(c.r-y)/1.402);}
            case 4:{let cb=c.g-.5;let cr=c.b-.5;rgb=vec3f(c.r+1.402*cr,c.r-.344136*cb-.714136*cr,c.r+1.772*cb);}
            default:{rgb=c.rgb*c.a;}
        }
        if v(7)!=0.{rgb=1.-rgb;}
    }else{
        var val=0.;
        switch input_mode{
            case 7:{val=c.r;} case 8:{val=c.g;} case 9:{val=c.b;} case 10:{val=c.a;}case 12:{val=hsl.x;}
            case 13:{val=hsl.z;}case 14:{val=y;}case 15:{val=hsl.y;}
            case 16:{val=fx_channel(c,8u);}case 18:{val=min(c.r,min(c.g,c.b));}case 19:{val=max(c.r,max(c.g,c.b));}
            default:{}
        }
        if v(7)!=0.{val=1.-val;}
        let dest=select(output_mode,output_mode-9,output_mode>=10);
        if output_mode>=10 {rgb=vec3f(0);a=1.;}
        switch dest{
            case 1:{rgb.r=val;} case 2:{rgb.g=val;} case 3:{rgb.b=val;}
            case 4:{a=val;if output_mode>=10{rgb=vec3f(1);}}
            case 6,7,8:{
                var h=fx_hsl(rgb);
                if output_mode>=10 {h=vec3f(0.,1.,.5);}
                if dest==6 {h.x=val;} else if dest==7 {h.z=val;} else {h.y=val;} rgb=fx_rgb(h);
            }
            default:{}
        }
    }
    if v(8)!=0.{a=1.;}
    return vec4f(rgb*a,a);
}