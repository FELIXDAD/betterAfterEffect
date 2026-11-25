struct Wipe {
    src:FxImage,back:FxImage,map:FxImage,roi:vec4i,rect:vec4i,frame:vec4f,
    inverse:mat3x3f,light:vec4f,light_color:vec4f,available:vec4u,params:array<vec4f,108>,
}
@group(0) @binding(0) var<uniform> u:Wipe;
@group(0) @binding(1) var image:texture_2d<f32>;
@group(0) @binding(2) var back:texture_2d<f32>;
@group(0) @binding(3) var gradient:texture_2d<f32>;
@group(0) @binding(4) var out:texture_storage_2d<WORKING_FORMAT,write>;

fn val(id:u32)->f32{return u.params[id].x;}
fn source_uv(img:FxImage,uv:vec2f)->vec2f{return img.origin+uv*vec2f(img.size)/img.scale;}
fn gradient_sample(uv:vec2f)->vec4f{
    if u.available.y==0u{return vec4f(0);}
    return fx_bilinear(gradient,u.map,source_uv(u.map,uv),true);
}
fn image_wipe(pos:vec2f,size:vec2f)->vec4f{
    let c=fx_sample(image,u.src,pos,false);
    let t=val(1);
    if t<=0.0{return c;}if t>=1.0{return vec4f(0);}
    let uv=(pos-vec2f(u.rect.xy))/size;
    var sample=gradient_sample(uv);
    let blur=val(4)*u.frame.x;
    if blur>0.0{
        var total=vec4f(0);var weights=0.0;
        for(var y=-2;y<=2;y++){for(var x=-2;x<=2;x++){
            let d=vec2f(f32(x),f32(y));let w=exp(-dot(d,d)*0.5);
            total+=w*gradient_sample(uv+d*0.5*blur/size);weights+=w;
        }}
        sample=total/weights;
    }
    let hsl=fx_hsl(sample.rgb);
    var level=fx_luma(sample.rgb);
    switch u32(val(6)){
        case 1u:{level=sample.r;}case 2u:{level=sample.g;}case 3u:{level=sample.b;}case 4u:{level=sample.a;}
        case 6u:{level=hsl.z;}case 7u:{level=hsl.x;}case 8u:{level=hsl.y;}default:{}
    }
    if val(7)>0.5{level=1.0-level;}
    let feather=max(0.00001,val(2)*select(1.0,sin(3.14159*t),val(3)>0.5));
    let threshold=mix(-feather,1.0+feather,t);
    return c*smoothstep(threshold-feather,threshold+feather,level);
}

fn random_value(cell:vec2i,seed:u32)->f32{
    var v=u32(cell.x)*747796405u+u32(cell.y)*2891336453u+seed*277803737u;
    v=((v>>((v>>28u)+4u))^v)*277803737u;
    return f32((v>>22u)^v)/4294967296.0;
}
fn card_phase(cell:vec2i,tiles:vec2f,seed:u32)->f32{
    let uv=(vec2f(cell)+0.5)/tiles; var order=uv.x;
    switch u32(val(20)){
        case 2u:{order=1.0-uv.x;}case 3u:{order=uv.y;}case 4u:{order=1.0-uv.y;}
        case 5u:{order=(uv.x+uv.y)*0.5;}case 6u:{order=(1.0-uv.x+uv.y)*0.5;}
        case 7u:{order=(uv.x+1.0-uv.y)*0.5;}case 8u:{order=1.0-(uv.x+uv.y)*0.5;}
        case 9u:{order=fx_luma(gradient_sample(uv).rgb);}default:{}
    }
    order=mix(order,random_value(cell,seed),val(24));
    let cycle=max(0.00001,val(4)*0.01);
    return saturate((val(2)*0.01-order*(1.0-cycle))/cycle);
}
fn oscillation(cell:vec2i,seed:u32,speed:f32)->f32{
    return sin(6.2831853*(random_value(cell,seed)+u.frame.y*speed));
}
fn card_wipe(pos:vec2f,size:vec2f)->vec4f{
    let homogeneous=u.inverse*vec3f(pos,1);
    if abs(homogeneous.z)<1e-9{return vec4f(0);}
    let uv=homogeneous.xy/homogeneous.z;
    let rows=max(1.0,round(val(10)));
    let cols=select(max(1.0,round(val(12))),max(1.0,round(rows*size.x/size.y)),val(8)==2.0);
    let tiles=vec2f(cols,rows);let pitch=size/tiles;
    let seed=u32(val(26));
    let home=vec2i(floor(uv*tiles));
    let jitter=vec3f(val(74),val(78),val(82));
    let rotation=vec3f(val(86),val(90),val(94));
    let reach=vec2i(ceil(jitter.xy+max(0.0,(val(14)-1.0)*0.5)+min(1.0,jitter.z)));
    let lo=max(vec2i(0),home-reach);let hi=min(vec2i(tiles)-1,home+reach);
    var nearest=-1e20;var result=vec4f(0);
    for(var y=lo.y;y<=hi.y;y++){for(var x=lo.x;x<=hi.x;x++){
        let cell=vec2i(x,y);
        let phase=card_phase(cell,tiles,seed);
        var angle=phase*3.14159265;
        if val(18)==2.0 || (val(18)==3.0 && random_value(cell,seed+1u)>0.5){angle=-angle;}
        let around_x=val(16)==1.0 || (val(16)==3.0 && random_value(cell,seed+2u)>0.5);
        let shift=jitter*vec3f(oscillation(cell,seed+3u,val(76)),oscillation(cell,seed+4u,val(80)),oscillation(cell,seed+5u,val(84)));
        let r=radians(rotation)*vec3f(oscillation(cell,seed+6u,val(88)),oscillation(cell,seed+7u,val(92)),oscillation(cell,seed+8u,val(96)));
        let a=r.x+select(0.0,angle,around_x);let b=r.y+select(angle,0.0,around_x);
        let z=r.z;let org=(vec2f(cell)+0.5+shift.xy)*pitch;
        let axis_x=vec3f(cos(b)*cos(z),cos(b)*sin(z),-sin(b));
        let axis_y=vec3f(sin(a)*sin(b)*cos(z)-cos(a)*sin(z),sin(a)*sin(b)*sin(z)+cos(a)*cos(z),sin(a)*cos(b));
        let normal=cross(axis_x,axis_y);
        let length_mm=select(val(40),select(val(54),35.0,val(52)>0.5),val(28)==2.0);
        let focal=max(1.0,size.x*length_mm/35.0);
        let q=uv*size-org;
        let projection=mat2x2f(axis_x.xy-q*axis_x.z/focal,axis_y.xy-q*axis_y.z/focal);
        let det=determinant(projection);if abs(det)<0.001{continue;}
        let inv=mat2x2f(vec2f(projection[1].y,-projection[0].y),vec2f(-projection[1].x,projection[0].x));
        let local=(inv*q/det)*(1.0+shift.z*pitch.y/focal);
        let actual=pitch*val(14);
        if any(abs(local)*2.0>=actual){continue;}
        let depth=-(dot(vec2f(axis_x.z,axis_y.z),local)+shift.z*pitch.y);
        if depth<=nearest{continue;}nearest=depth;
        let is_back=normal.z<0.0;
        var coord=local/max(actual,vec2f(0.0001));
        if is_back {coord*=select(vec2f(-1,1),vec2f(1,-1),around_x);}
        let tex=(vec2f(cell)+0.5+coord)/tiles;
        var c=fx_bilinear(image,u.src,vec2f(u.rect.xy)+tex*size,false);
        if is_back {c=select(vec4f(0),fx_bilinear(back,u.back,source_uv(u.back,tex),false),u.available.x!=0u);}
        let lit_normal=select(normal,-normal,is_back);
        var light=vec3f(u.params[60].xy-select(size*0.5,org,val(64)==1.0),max(0.001,val(62)*size.y));
        var light_rgb=u.params[58].rgb*val(56);
        if val(64)==3.0 && u.light.w!=0.0{light=u.light.xyz-vec3f(org,-depth);light.z=-light.z;light_rgb=u.light_color.rgb;}
        let dir=light/max(length(light),0.0001);
        let diffuse=max(0.0,dot(dir,lit_normal))*val(68);
        let half_vec=normalize(dir+vec3f(0,0,1.00001));
        let spec=pow(max(0.00001,dot(lit_normal,half_vec)),max(1.0,val(72)))*val(70);
        result=vec4f(c.rgb*(vec3f(val(66))+light_rgb*diffuse)+c.a*light_rgb*spec,c.a);
    }}
    return result;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u){
    let p=vec2i(id.xy)+u.roi.xy;if any(p>=u.roi.zw){return;}
    let size=max(vec2f(1),vec2f(u.rect.zw-u.rect.xy));let pos=vec2f(p)+0.5;
    var c=vec4f(0);
    if u.frame.w>0.5{c=card_wipe(pos,size);}else{c=image_wipe(pos,size);}
    if u.frame.z==0.0{c=vec4f(clamp(c.rgb,vec3f(0),vec3f(saturate(c.a))),saturate(c.a));}
    textureStore(out,vec2i(id.xy),c);
}
