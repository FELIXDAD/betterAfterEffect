// The measured cell centers lie at 1/6, 1/2 and 5/6 in layer coordinates.
fn mesh(base:u32, uv:vec2f)->vec2f{
    let q=uv*3.0-0.5;
    let cell=clamp(vec2i(floor(q)),vec2i(0),vec2i(1));
    let w=q-vec2f(cell);
    let i=base+u32(cell.x+3*cell.y);
    return mix(mix(u.values[i].xy,u.values[i+1u].xy,w.x),mix(u.values[i+3u].xy,u.values[i+4u].xy,w.x),w.y);
}
fn cross2(a:vec2f,b:vec2f)->f32{return a.x*b.y-a.y*b.x;}
fn displacement(uv:vec2f,size:vec2f)->vec2f{
    var mid=vec2f(0); var rot=0.0; var zoom=0.0;var moment=0.0;
    for(var i=0u;i<9u;i++){
        let offset=u.values[100u+i].xy;
        let r=(vec2f(f32(i%3u),f32(i/3u))-1.0)*size/3.0;
        mid+=offset/9.0;
        rot+=cross2(r,offset);zoom+=dot(r,offset);moment+=dot(r,r);
    }
    let r=(uv-0.5)*size;
    let scale=zoom/max(1.0,moment);
    let method=u32(value(28u));
    if method==1u{return mid;}
    if method==2u{return mid+(vec2f(-r.y,r.x)*rot/max(1.0,moment))+select(r*scale,vec2f(0),enabled(44u));}
    var d=mesh(100u,uv);
    if method==3u {
        let a=mesh(100u,vec2f(0))/size;
        let b=vec2f(1,0)+mesh(100u,vec2f(1,0))/size;
        let c=vec2f(1)+mesh(100u,vec2f(1))/size;
        let e=vec2f(0,1)+mesh(100u,vec2f(0,1))/size;
        let det=cross2(b-c,e-c);
        if abs(det)>0.0001 {
            let sum=a-b+c-e;
            let g=vec2f(cross2(sum,e-c),cross2(b-c,sum))/det;
            let den=1.0+dot(g,uv);
            if abs(den)>0.01{d=((a+uv.x*(b-a+g.x*b)+uv.y*(e-a+g.y*e))/den-uv)*size;}
        }
    }else if value(37u)==2.0{
        d=mix(d,mesh(100u,vec2f(0.5,uv.y)),0.30);
    }
    return d-select(vec2f(0),r*scale,enabled(44u));
}
@compute @workgroup_size(8,8)
fn border(@builtin(global_invocation_id) id:vec3u){
    if any(id.xy!=vec2u(0)){return;}
    let size=max(vec2f(1),vec2f(u.layer.zw-u.layer.xy));
    var inset=vec2f(0);
        for(var j=0;j<9;j++){
            let uv=vec2f(f32(j%3),f32(j/3))*0.5;
            let d=displacement(uv,size)/size;
            inset=max(inset,max((uv+d)-1.0,-(uv+d)));
        }
    textureStore(out_tex,vec2i(0),vec4f(inset,0,1));
}
fn evaluate(p:vec2f)->vec4f{
    let lo=vec2f(u.layer.xy);let size=max(vec2f(1),vec2f(u.layer.zw-u.layer.xy));
    let frame=u32(value(22u));var zoom=value(16u)/100.0;
    var inset=vec2f(0);
    if frame==2u || frame==3u {
        inset=textureLoad(map_d,vec2i(0),0).xy;
        if frame==3u{zoom*=max(1.0,min(value(4u)/100.0,max(0.01,1.0-value(3u)/100.0)/max(0.01,1.0-2.0*max(inset.x,inset.y))));}
    }
    if zoom<=0.0{return vec4f(0);}
    let uv=((p-lo)/size-0.5)/zoom+0.5;
    if (frame==2u || frame==3u) && any(abs(uv-0.5)>0.5-inset){return vec4f(0);}
    let q=lo+uv*size+displacement(uv,size);
    let rgba=src(q);
    if value(47u)>=5.0 && has(0u){
        let overlay=aux_a(q);var out=output(overlay.rgb,overlay.a);
        if value(47u)==6.0{let bg=src(p);out+=output(bg.rgb,bg.a)*(1.0-out.a);}
        return out;
    }
    if frame!=4u {return output(rgba.rgb,rgba.a);}
    let crop=u.scale*vec4f(value(31u),value(32u),value(33u),value(34u));
    let edge=min(q-(lo+crop.xy),lo+size-crop.zw-q);
    let keep=clamp((min(edge.x,edge.y)+0.5)/max(1.0,u.scale*value(35u)),0.0,1.0);
    var out=output(rgba.rgb,rgba.a)*keep;
    for(var i=0u;i<4u;i++){
        if has(i) && out.a<1.0{
            let t=q+mesh(110u+i*9u,uv);var c=vec4f(0);
            switch i{case 0u:{c=aux_a(t);}case 1u:{c=aux_b(t);}case 2u:{c=aux_c(t);}default:{c=fx_bilinear(map_d,u.images[4],t,true);}}
            out+=output(c.rgb,c.a)*(1.0-out.a);
        }
    }
    return out;
}
