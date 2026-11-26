struct Grading {
    image:FxImage, origin:vec2i, floating:u32, flags:u32, bounds:vec4i,
    creative:vec4f, shadows:vec3f, balance:f32, highlights:vec3f, curve_mask:u32,
    lift:vec4f, midtone:vec3f, curve_white:f32, gain:vec3f, wheel_white:f32,
    key_color:vec3f, has_key:u32, range:vec4f,
    white_balance:vec3f, sat:f32, contrast:f32, invert:u32, mask_view:u32, spatial:u32,
    vignette:vec4f, blur0:vec4f,
    curves:array<vec4u,288>, film:array<vec4u,32>, vibrance:array<vec4u,264>, edge:array<vec4u,32>, tint_curve:array<vec4f,66>, secondary_curve:array<vec4f,16>, correction:vec4f, chroma:array<vec4u,256>,
}
@group(0) @binding(0) var<uniform> u:Grading;
@group(0) @binding(1) var src:texture_2d<f32>;
@group(0) @binding(2) var matte:texture_2d<f32>;
@group(0) @binding(3) var dst:texture_storage_2d<WORKING_FORMAT,write>;

fn film_sample(row:u32,i:u32)->vec2f {
    let j=row*64u+i;
    return unpack2x16float(u.film[j/4u][j%4u]);
}
fn faded_film(rgb:vec3f)->vec3f {
    let x=clamp(pow(max(dot(rgb,luma),0.),1./2.4),0.,1.)*63.;
    let i=min(62u,u32(x));
    let a=mix(film_sample(0u,i),film_sample(0u,i+1u),x-f32(i));
    let b=mix(film_sample(1u,i),film_sample(1u,i+1u),x-f32(i));
    let k=mix(a,b,min(1.,u.creative.x*10.-min(14.,floor(u.creative.x*10.))));
    return k.x*rgb+k.y;
}
fn extrema(row:u32,hi:u32,lo:u32)->vec2f {
    let i=528u*row+hi*(hi+1u)/2u+lo;
    let j=i/2u;
    let word=u.vibrance[j/4u][j%4u]>>(16u*(i%2u));
    return vec2f(f32(word&255u),f32((word>>8u)&255u))/255.;
}
fn vibrance_row(row:u32,p:vec2f)->vec2f {
    let i=min(vec2u(p),vec2u(30u));
    let f=p-vec2f(i);
    let a=extrema(row,i.y,i.x);
    let b=extrema(row,i.y+1u,i.x+1u);
    if f.y>=f.x {
        return a*(1.-f.y)+extrema(row,i.y+1u,i.x)*(f.y-f.x)+b*f.x;
    }
    return a*(1.-f.x)+extrema(row,i.y,min(i.x+1u,i.y))*(f.x-f.y)+b*f.y;
}
fn vibrance(rgb:vec3f)->vec3f {
    let c=encoded(rgb);
    let p=clamp(vec2f(least(c),greatest(c)),vec2f(0.),vec2f(1.))*31.;
    let pos=u.creative.z*4.+4.;
    let t=pos-min(7.,floor(pos));
    let base=mix(vibrance_row(0u,p),vibrance_row(2u,p),t);
    let red=mix(vibrance_row(1u,p),vibrance_row(3u,p),t);
    let hue=fx_hsl(c).x;
    let skin=1.-smoothstep(.06,.17,abs(fract(hue+.46)-.5));
    let ends=mix(base,red,skin);
    let lo=least(rgb); let hi=greatest(rgb);
    if hi==lo {return rgb;}
    let values=pow(ends,vec2f(2.4));
    return values.x+(rgb-lo)*((values.y-values.x)/(hi-lo));
}

fn sample_lin(pos:vec2f)->vec3f {
    let xy=clamp(pos,vec2f(u.bounds.xy)+.5,vec2f(u.bounds.zw)-.5);
    var rgb=linear(fx_sample(src,u.image,xy,true).rgb);
    if u.creative.x>0. {rgb=faded_film(rgb);}
    if u.creative.z!=0. {rgb=vibrance(rgb);}
    if u.creative.w!=1. {rgb=saturation(rgb,u.creative.w);}
    return rgb;
}
fn sharpen(rgb:vec3f,pos:vec2f)->vec3f {
    let amount=u.creative.y;
    if amount>0. {
        var sum=vec3f(0.);
        for(var y=-1;y<=1;y++) {
            for(var x=-1;x<=1;x++) {
                let w=select(.2,.4,x==0 || y==0);
                sum+=w*(rgb-sample_lin(pos+vec2f(f32(x),f32(y))));
            }
        }
        return rgb+amount*sum;
    }
    let n=i32(u.spatial);
    var sum=vec3f(0.); var weight=0.;
    for(var y=-n;y<=n;y++) {
        for(var x=-n;x<=n;x++) {
            let offset=vec2f(f32(x),f32(y));
            let weights=array<f32,5>(u.blur0.x,u.blur0.y,u.blur0.z,u.blur0.w,u.lift.w);
            let w=weights[abs(x)]*weights[abs(y)];
            sum+=w*sample_lin(pos+offset); weight+=w;
        }
    }
    return sum/weight;
}

fn tint(c:f32,ch:u32)->f32 {
    let p=clamp(c,0.,1.)*255.;
    let q=pow(p/255.,exp2(u.balance))*255.;
    let i=min(63u,u32(q)/4u)+u32(q>=254.);
    let a=min(254.,f32(i)*4.);
    let b=min(255.,select(a+4.,a+2.,i==63u));
    let delta=mix(u.tint_curve[i][ch],u.tint_curve[i+1u][ch],(q-a)/(b-a));
    return c+delta*p/max(.001,q);
}
fn contrast_response(x:f32)->f32 {
    let p=clamp(pow(max(0.,x),1./2.4),0.,1.)*63.;
    let i=min(62u,u32(p));
    let a=u.secondary_curve[i/4u][i%4u];
    let b=u.secondary_curve[(i+1u)/4u][(i+1u)%4u];
    return pow(mix(a,b,p-f32(i)),2.4);
}
fn chroma_sample(p:vec3u)->f32 {
    let i=p.x*256u+p.y*16u+p.z;
    let word=u.chroma[i/16u][(i/4u)%4u];
    return f32(extractBits(bitcast<i32>(word),8u*(i%4u),8u))/255.;
}
fn contrast_residual(p:vec3f)->f32 {
    let q=clamp(p,vec3f(0.),vec3f(1.))*15.;
    let i=min(vec3u(q),vec3u(14u));
    let f=q-vec3f(i);
    var sum=0.;
    for(var a=0u;a<8u;a++) {
        let bit=vec3u(a>>2u,(a>>1u)&1u,a&1u);
        let w=select(1.-f,f,bit!=vec3u(0u));
        sum+=w.x*w.y*w.z*chroma_sample(i+bit);
    }
    return sum;
}
fn secondary_contrast(rgb:vec3f)->vec3f {
    if u.contrast==0. {return rgb;}
    let y=dot(rgb,luma);
    let lo=least(rgb);
    let target_y=contrast_response(y);
    if y-lo<1e-8 {return vec3f(target_y);}
    let c=encoded(rgb);
    let level=pow(max(0.,y),1./2.4);
    let ratio=least(c)/max(1e-8,level);
    let headroom=(greatest(c)-level)/max(1e-8,1.-level);
    let offset=contrast_residual(vec3f(level,ratio,headroom))*select(1.,2.,u.contrast<0.);
    let low=select(0.,max(0.,ratio*pow(target_y,1./2.4)+offset),lo>0. || u.contrast<0.);
    let slope=(target_y-pow(low,2.4))/(y-lo);
    let out=target_y+(rgb-y)*slope;
    return select(out,rgb,((rgb>=vec3f(1.)) | (rgb<=vec3f(0.))) & vec3<bool>(u.contrast>0.));
}
fn curve_sample(ch:u32,i:u32)->f32 {
    let j=256u*ch+i;
    return unpack2x16float(u.curves[j/8u][(j/2u)%4u])[j%2u];
}
fn curve(ch:u32,x:f32)->f32 {
    if (u.curve_mask & (1u<<ch))==0u {return x;}
    let pos=clamp(x,0.,1.)*255.;
    let i=min(u32(pos),254u);
    return mix(curve_sample(ch,i),curve_sample(ch,i+1u),pos-f32(i));
}
fn key_weight(c:vec3f)->f32 {
    let hsl=fx_hsl(c);
    let delta=abs(hsl-fx_hsl(u.key_color));
    let distance=vec3f(min(delta.x,1.-delta.x),delta.yz)-u.range.xyz;
    let fade=max(.00001,u.range.w);
    let w=clamp(1.-distance/fade,vec3f(0.),vec3f(1.));
    let key=select(0.,w.x*w.y*w.z,u.has_key!=0u);
    return select(key,1.-key,u.invert!=0u);
}

fn edge_sample(row:u32,i:u32)->vec2f {
    let j=row*64u+i;
    return unpack2x16float(u.edge[j/4u][j%4u]);
}
fn gain(c:vec3f,g:vec3f)->vec3f {
    let knee=2./max(vec3f(.001),g)-1.;
    let shoulder=1.-g*g*(1.-c)*(1.-c)/max(vec3f(1e-6),4.*(g-1.));
    return select(c*g,shoulder,(g>vec3f(1.))&(c>knee)&(c<=vec3f(1.)));
}
fn vignette(rgb:vec3f,pos:vec2f)->vec3f {
    let size=vec2f(u.bounds.zw-u.bounds.xy);
    let half_size=.5*size;
    let midpoint=.625+.75*u.vignette.y;
    let center=abs(pos-vec2f(u.bounds.xy)-half_size+vec2f(-.5,.5));
    let axes=mix(half_size,vec2f(.5*(half_size.x+half_size.y)),max(0.,u.vignette.z))*midpoint;
    // Negative roundness makes the ellipse approach a rounded rectangle.
    let n=2./(1.-.88*max(0.,-u.vignette.z));
    let xy=center/axes;
    let power=pow(xy,vec2f(n));
    let radius=pow(power.x+power.y,1./n);
    let normal=pow(xy/max(radius,.00001),vec2f(n-1.))/axes;
    let distance=(radius-1.)/max(.00001,length(normal));
    let width=1.+1.72*min(axes.x,axes.y)*u.vignette.w*min(1.,1.2+u.vignette.z);
    let mask=smoothstep(-.5*width,.5*width,distance);
    let c=linear(rgb);
    let y=dot(c,luma);
    let x=clamp(pow(max(y,0.),1./2.4),0.,1.)*63.;
    let i=min(62u,u32(x));
    let t=x-f32(i);
    let a=mix(edge_sample(0u,i),edge_sample(0u,i+1u),t);
    let b=mix(edge_sample(1u,i),edge_sample(1u,i+1u),t);
    let slider=2.*u.vignette.x+10.;
    let fit=mix(a,b,slider-min(19.,floor(slider)));
    let target_y=pow(fit.x,2.4);
    var slope=fit.y;
    if u.vignette.x<0. {
        slope=min(slope,target_y/max(1e-12,y-least(c)));
    }
    var graded=max(vec3f(0.),target_y+slope*(c-y));
    if u.vignette.x<0. {
        let white=mix(edge_sample(0u,63u).x,edge_sample(1u,63u).x,slider-min(19.,floor(slider)));
        graded=select(graded,vec3f(pow(white,2.4)),c>=vec3f(1.));
    }
    let protect=select(1.,1.-u.creative.x*smoothstep(.25,1.,y),(u.flags&256u)!=0u);
    return encoded(mix(c,graded,mask*protect));
}

@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
    if any(id.xy>=textureDimensions(dst)) {return;}
    let pos=vec2f(u.origin)+vec2f(id.xy)+.5;
    let src_color=fx_sample(src,u.image,pos,true);
    var c=src_color.rgb;
    if (u.flags&64u)!=0u {
        var w=key_weight(c);
        let strength=bitcast<f32>(u.spatial);
        if strength>0. {
            var taps:array<f32,9>;
            for(var i=0u;i<9u;i++) {
                let d=vec2f(f32(i%3u)-1.,f32(i/3u)-1.);
                let xy=clamp(pos+d,vec2f(u.bounds.xy)+.5,vec2f(u.bounds.zw)-.5);
                taps[i]=key_weight(fx_sample(src,u.image,xy,true).rgb);
                for(var j=i;j>0u;j--) {
                    if taps[j]>=taps[j-1u] {break;}
                    let tmp=taps[j];taps[j]=taps[j-1u];taps[j-1u]=tmp;
                }
            }
            w=mix(w,taps[4],strength);
        }
        textureStore(dst,id.xy,vec4f(vec3f(w),1.));
        return;
    }
    if (u.flags&1u)!=0u {
        var rgb=sample_lin(pos);
        if u.creative.y!=0. {rgb=sharpen(rgb,pos);}
        c=encoded(rgb);
        if (u.flags&16u)!=0u {c=vec3f(tint(c.r,0u),tint(c.g,1u),tint(c.b,2u));}
    }
    if (u.curve_mask&15u)!=0u {
        c/=u.curve_white;
        if (u.curve_mask&1u)!=0u {
            let light=linear(c);
            let lo=least(light); let hi=greatest(light);
            let a=linear(vec3f(curve(0u,least(c)))).r;
            let b=linear(vec3f(curve(0u,greatest(c)))).r;
            c=encoded(a+(light-lo)*(b-a)/max(1e-12,hi-lo));
        }
        c=u.curve_white*vec3f(curve(1u,c.r),curve(2u,c.g),curve(3u,c.b));
    }
    if (u.curve_mask & 496u) != 0u {
        // Every secondary curve selects from the same post-RGB input.
        let input_light = linear(c);
        let input_hsl = fx_hsl(input_light);
        let input_luma = dot(input_light,luma);
        var light = input_light;
        if (u.curve_mask & 16u) != 0u {
            light = input_luma + (light-input_luma) * max(0.,1.+curve(4u,input_hsl.x));
        }
        if (u.curve_mask & 32u) != 0u {
            let hsl = fx_hsl(encoded(light));
            light = linear(fx_rgb(vec3f(fract(hsl.x + curve(5u,input_hsl.x)),hsl.yz)));
        }
        if (u.curve_mask & 64u) != 0u {
            light *= exp2(curve(6u,input_hsl.x));
        }
        let y = dot(light,luma);
        var factor = 1.;
        if (u.curve_mask & 128u) != 0u { factor *= max(0.,1.+curve(7u,input_luma)); }
        if (u.curve_mask & 256u) != 0u { factor *= max(0.,1.+curve(8u,input_hsl.y)); }
        c = encoded(select(y + (light-y)*factor, light, factor == 1.));
    }
    if (u.flags&2u)!=0u {
        c/=u.wheel_white;
        c+=.5*u.lift.rgb*(1.-c);
        c=sign(c)*pow(abs(c),max(vec3f(.01),1.-u.midtone));
        c=u.wheel_white*gain(c,1.+.5*u.gain);
    }
    if (u.flags&4u)!=0u {
        var weight=key_weight(c);
        if (u.flags&128u)!=0u {weight=textureLoad(matte,id.xy,0).r;}
        if u.mask_view!=0u {
            var background=vec3f(0.);
            if u.mask_view==1u {background=vec3f(184./255.);}
            c=encoded(mix(linear(background),select(linear(c),vec3f(1.),u.mask_view==3u),weight));
        } else {
            var base=c;
            if (u.flags&32u)!=0u {base=encoded(sharpen(linear(c),pos));}
            base=sign(base)*pow(abs(base),1.-u.correction.rgb);
            let corr=secondary_contrast(linear(base*u.white_balance));
            c=encoded(mix(linear(c),saturation(corr,u.sat),weight));
        }
    }
    if (u.flags&8u)!=0u {c=vignette(c,pos);}
    textureStore(dst,id.xy,fx_output(c,src_color.a,u.floating!=0u));
}
