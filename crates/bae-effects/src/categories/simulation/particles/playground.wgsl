struct PlaygroundParticle {
    pos:vec2f, vel:vec2f, size:vec2f, color:vec4f, angle:f32, mass:f32, life:f32,
}
fn source_matches(source:u32,selection:u32)->bool{
    let masks=array<u32,16>(15,0,1,2,4,8,3,5,9,6,10,12,7,11,13,14);
    return (masks[clamp(selection,1u,16u)-1u] & (1u<<source))!=0u;
}
fn age_weight(age:f32, threshold:f32, feather:f32)->f32{
    if threshold==0.{return 1.;}
    let delta=select(abs(threshold)-age,age-threshold,threshold>0.);
    if feather<=0.{return select(0.,1.,delta>=0.);}
    return clamp(.5+delta/feather,0.,1.);
}
fn selection(tex:texture_2d<f32>,present:f32,p:vec2f)->f32{
    if present==0.{return 1.;}
    let xy=vec2i(clamp(p/cfg.size,vec2f(0.),vec2f(.999999))*vec2f(textureDimensions(tex)));
    let c=textureLoad(tex,xy,0);return lum(c.rgb);
}
fn op(a:f32,b:f32,operation:u32)->f32{
    switch operation {case 2u:{return a+b;}case 3u:{return abs(a-b);}case 4u:{return a-b;}
        case 5u:{return a*b;}case 6u:{return min(a,b);}case 7u:{return max(a,b);}default:{return b;}}
}
// Apply one RGB channel to the selected particle property in place.
fn map_channel(p:ptr<function,PlaygroundParticle>,base:u32,channel:f32,w:f32,transient:bool,age:f32){
    let key=u32(f(base));
    if key<=1u{return;}
    let low=base+select(1u,2u,transient);
    let v=mix(f(low),f(low+1u),channel)*w;
    let operation=select(1u,u32(f(base+1u)),transient);
    // Select the scalar once so every operation uses the same arithmetic.
    var current=0.;
    switch key{
        case 2u:{current=(*p).color.r;}case 3u:{current=(*p).color.g;}case 4u:{current=(*p).color.b;}
        case 7u:{current=degrees((*p).angle);}
        case 13u:{current=(*p).pos.x;}case 14u:{current=(*p).pos.y;}
        case 15u:{current=length((*p).vel);}
        case 16u:{current=(*p).vel.x;}case 17u:{current=(*p).vel.y;}
        case 21u:{current=(*p).color.a;}case 22u:{current=(*p).mass;}case 23u:{current=(*p).life;}
        default:{}
    }
    let n=op(current,v,operation);
    switch key{
        case 2u:{(*p).color.r=n;} case 3u:{(*p).color.g=n;}case 4u:{(*p).color.b=n;}
        case 5u,6u:{(*p).vel*=exp(-max(0.,v)*age);}
        case 7u:{(*p).angle=radians(n);}case 8u,9u:{(*p).angle+=radians(v)*age;}
        case 10u:{(*p).size*=v;}case 11u:{(*p).size.x*=v;}case 12u:{(*p).size.y*=v;}
        case 13u:{(*p).pos.x=n;}case 14u:{(*p).pos.y=n;}
        case 15u:{(*p).vel=normalize((*p).vel+.00001)*n;}
        case 16u:{(*p).vel.x=n;}case 17u:{(*p).vel.y=n;}
        case 18u:{(*p).vel+=normalize((*p).vel+.00001)*v*age;}
        case 19u:{(*p).vel.x+=v*age;}case 20u:{(*p).vel.y+=v*age;}
        case 21u:{(*p).color.a=clamp(n,0.,1.);}case 22u:{(*p).mass=max(.01,n);}
        case 23u:{(*p).life=n;}
        case 25u,27u:{(*p).size*=max(0.,1.+v*age);}
        default:{}
    }
}
fn apply_play_map(p:ptr<function,PlaygroundParticle>,source:u32,age:f32,transient:bool){
    let present=select(f(P501),f(P551),transient);
    if present<=0. || !source_matches(source,select(u32(f(P517)),u32(f(P567)),transient)){return;}
    let w=age_weight(age,select(f(P520),f(P570),transient),select(f(P521),f(P571),transient));
    if w<=0.{return;}
    var color=vec4f(0.);
    if transient{color=map_at(map3,3u,(*p).pos/cfg.size);}else{color=map_at(map2,2u,(*p).pos/cfg.size);}
    // Only traverse channels up to the last mapped property.
    for(var i=0u;i<select(cfg.history.y,cfg.history.z,transient);i++){
        let base=select(P502+3u*i,P552+4u*i,transient);
        map_channel(p,base,color[i],w,transient,age);
    }
}
fn playground(id:u32,t:f32)->Sprite{
    let children=select(1u,5u,f(P231)!=0.);
    let parent=id/children;
    let child=id%children;
    let cols=u32(round(f(P134)));let rows=u32(round(f(P135)));
    let grid_count=cols*rows;
    let spacing=max(2.,2.*f(P202));
    let pieces=vec2u(ceil(cfg.size/spacing));
    let fragment_count=select(0u,pieces.x*pieces.y,f(P201)>0.);
    let q=noise3(hash(parent))*2.-1.;
    var kind=0u; var age=t;
    var p=PlaygroundParticle(pt(P101),vec2f(0.),vec2f(f(P110)),vec4f(rgb(P104),1.),0.,1.,1e10);
    if parent<grid_count{
        kind=1u;
        let uv=(vec2f(f32(parent%max(cols,1u)),f32(parent/max(cols,1u)))+.5)/vec2f(max(vec2u(cols,rows),vec2u(1)));
        p.pos=pt(P131)+(uv-.5)*vec2f(f(P132),f(P133));
        p.size=vec2f(f(P136));p.color=vec4f(rgb(P137),1.);
    }else if parent<grid_count+fragment_count{
        kind=2u;let k=parent-grid_count;
        p.pos=(vec2f(f32(k%pieces.x),f32(k/pieces.x))+.5)*spacing;
        p.color=straight(map_at(map0,0u,p.pos/cfg.size));p.size=vec2f(f(P202));p.vel=q.xy*f(P204);
    }else{
        age=t-(f32(parent-grid_count-fragment_count)+.5)/max(.00001,f(P103));
        if age<0. || f(P103)<=0.{return Sprite();}
        p.pos+=vec2f(q.x*f(P102),0.);
        p.vel=dir(f(P105)+q.y*f(P106))*(f(P107)+q.z*f(P108));
    }
    if child!=0u{
        if !source_matches(kind,u32(f(P252))) || age_weight(age,f(P255),f(P256))*selection(select_explosion,f(P253),p.pos)<=0.{return Sprite();}
        kind=3u;p.size=vec2f(abs(f(P231)));p.vel+=(noise3(hash(id+13u)).xy*2.-1.)*f(P233);
    }
    let birth=p.pos;
    apply_play_map(&p,kind,age,false);
    var accel=vec2f(0.);
    if source_matches(kind,u32(f(P352))){
        accel+=dir(f(P333))*(f(P331)+q.z*f(P332))*age_weight(age,f(P355),f(P356))*selection(select_gravity,f(P353),p.pos)/p.mass;
    }
    if source_matches(kind,u32(f(P422))) && source_matches(kind,u32(f(P402))){
        let delta=p.pos-pt(P101);
        let reach=max(.001,abs(f(P381)));
        accel+=normalize(delta+.00001)*f(P382)*exp(-length(delta)/reach)*age_weight(age,f(P425),f(P426))
            *selection(select_repeller,f(P403),p.pos)*selection(select_repelled,f(P423),p.pos);
    }
    p.pos+=age*(p.vel+.5*accel*age);
    apply_play_map(&p,kind,age,true);
    if f(P453)>0. && source_matches(kind,u32(f(P472))) && age_weight(age,f(P475),0.)>0.{
        let uv=clamp(p.pos/cfg.size,vec2f(0.),vec2f(.999999));
        if textureLoad(side_map,vec2i(uv*frame.side_size.xy),0).a>.5{
            var lo=birth;var hi=p.pos;
            for(var k=0u;k<8u;k++){
                let mid=(lo+hi)*.5;
                if textureLoad(side_map,vec2i(clamp(mid/cfg.size,vec2f(0.),vec2f(.999999))*frame.side_size.xy),0).a>.5 {hi=mid;}else{lo=mid;}
            }
            p.pos=lo;
        }
    }
    if age>p.life {return Sprite();}
    var s=sprite(p.pos,p.size,p.angle,DISC,p.color);
    if f(P281)>0. && source_matches(kind,u32(f(P302))) && age_weight(age,f(P305),f(P306))*selection(select_sprite,f(P303),p.pos)>0.{
        s.y=vec4f(s.y.xy,.5,.5);s.brush=vec4f(.5,.5,TILE,3.);
    }
    return s;
}
