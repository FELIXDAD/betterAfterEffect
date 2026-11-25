struct Stop { rgba: vec4f, pos: f32 }
struct Settings {
    source: FxImage, phase_layer: FxImage, mask_layer: FxImage,
    origin: vec2i, size: vec2u, get_phase:u32, add_phase:u32, add_mode:u32, modify:u32,
    shift:f32, repetitions:f32, tolerance:f32, softness:f32, match_color:vec4f,
    match_mode:u32, has_phase:u32, mask_mode:u32, flags:u32,
    blend:f32, count:u32, stops:array<Stop,64>,
}
@group(0) @binding(0) var<uniform> u: Settings;
@group(0) @binding(1) var dst:texture_storage_2d<WORKING_FORMAT,write>;
@group(0) @binding(2) var src:texture_2d<f32>;
@group(0) @binding(3) var phase_tex:texture_2d<f32>;
@group(0) @binding(4) var mask_tex:texture_2d<f32>;

fn phase(c:vec4f,channel:u32)->f32 {
    switch channel {
        case 1u:{return (c.r + c.g + c.b) / 3.;}
        case 2u,3u,4u:{return c[channel-2u];}
        case 5u:{return fx_hsl(c.rgb).x;}
        case 6u:{return fx_hsl(c.rgb).z;}
        case 7u:{return fx_hsl(c.rgb).y;}
        case 8u:{return max(c.r,max(c.g,c.b));}
        case 9u:{return c.a;}
        default:{return 0.;}
    }
}
fn palette(t:f32)->vec4f {
    var left=u.stops[u.count-1u]; var right=u.stops[0]; left.pos-=1.0; right.pos+=1.0;
    for(var i=0u;i<u.count;i++) {
        let stop=u.stops[i];
        if stop.pos>t {right=stop;break;}
        left=stop;
    }
    let fraction = saturate((t-left.pos) / max(right.pos-left.pos, 1e-7));
    let f = select(step(0.5, fraction), fraction, (u.flags & 1u) != 0u);
    return mix(left.rgba,right.rgba,f);
}
fn modify(rgb:vec3f,color:vec3f)->vec3f {
    if u.modify>=8u && u.modify<=13u {
        let a=fx_hsl(rgb);let b=fx_hsl(color);
        return fx_rgb(select(a,b,vec3<bool>(u.modify==8u || u.modify==11u || u.modify==13u,
            u.modify==10u || u.modify==12u || u.modify==13u,u.modify==9u || u.modify==11u || u.modify==12u)));
    }
    return select(rgb,color,vec3<bool>(u.modify==1u||u.modify==2u||u.modify==5u||u.modify==7u,
        u.modify==1u||u.modify==3u||u.modify==5u||u.modify==6u,u.modify==1u||u.modify==4u||u.modify==6u||u.modify==7u));
}
@compute @workgroup_size(8,8)
fn colorama(@builtin(global_invocation_id) id:vec3u) {
    if any(id.xy>=u.size) {return;}
    let p=vec2f(u.origin)+vec2f(id.xy)+0.5;
    let c=saturate(fx_sample(src,u.source,p,true));
    var t=phase(c,u.get_phase);
    if u.has_phase!=0u {
        let a=phase(fx_sample(phase_tex,u.phase_layer,p,true),u.add_phase);
        switch u.add_mode {case 2u:{t=min(1.,t+a);} case 3u:{t=(t+a)*0.5;} case 4u:{t=t+a-t*a;} default:{t+=a;}}
    }
    // Half-bin endpoints keep white on the last swatch before the cycle wraps.
    let phase_extent = 1.0 - 0.5/256.0;
    let shifted = fract(t*phase_extent + u.shift);
    let mapped = palette(fract(shifted*u.repetitions) * phase_extent);
    let rgb=modify(c.rgb,mapped.rgb);
    var alpha=select(c.a,mapped.a,(u.flags&2u)!=0u);
    if c.a==0. && (u.flags&4u)==0u {alpha=0.;}
    var w=1.0;
    if u.match_mode>1u {
        let key = u.match_color.rgb;
        let delta = c.rgb - key;
        var distance = length(delta) / length(max(key, 1.0-key));
        if u.match_mode == 3u {
            // The measured hue selector repeats after half a turn.
            let difference = 2.0 * abs(fx_hsl(c.rgb).x - fx_hsl(key).x);
            distance = select(difference, difference-1.0, difference > 1.0);
        } else if u.match_mode == 4u {
            let centered = key - dot(key, vec3f(1./3.));
            let edge = max(abs(centered.r), max(abs(centered.g), abs(centered.b)));
            let range = 0.9 * sqrt(dot(centered, centered) + 2./3. + 2.*edge);
            distance = length(delta - dot(delta, vec3f(1./3.))) / range;
        }
        let inner = u.tolerance * u.tolerance;
        let width = (1.0-inner) * u.softness * u.softness;
        w *= 1.0 - saturate((distance-inner) / max(width, 1e-7));
    }
    if u.mask_mode>1u {
        let m=fx_sample(mask_tex,u.mask_layer,p,true);
        let k=select(phase(m,1u),m.a,u.mask_mode==3u||u.mask_mode==5u);
        w*=select(k,1.-k,u.mask_mode>=4u);
    }
    let input=vec4f(c.rgb*c.a,c.a);
    var result=vec4f(rgb*alpha,alpha)*w;
    if (u.flags&8u)!=0u {result=result+input*(1.-result.a);}
    textureStore(dst,id.xy,mix(result,input,u.blend));
}
