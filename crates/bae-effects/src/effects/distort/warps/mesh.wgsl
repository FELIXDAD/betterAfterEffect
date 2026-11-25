fn node(xy:vec2i)->vec2f {
    let i=u32(xy.y*i32(u.grid_size.x)+xy.x);let pair=u.grid[i/2u];
    return select(pair.xy,pair.zw,i%2u==1u);
}
fn displacement(q:vec2f)->vec2f {
    let last=vec2i(u.grid_size.xy)-1;
    let cell=clamp(q,vec2f(0),vec2f(1))*vec2f(last);
    let base=min(vec2i(cell),last-1);let t=cell-vec2f(base);
    return mix(mix(node(base),node(base+vec2i(1,0)),t.x),mix(node(base+vec2i(0,1)),node(base+1),t.x),t.y);
}
fn warp(p:vec2f)->vec4f {
    let q=uv(p);var source_uv=q;
    if u.grid_size.x>=2. {
        for(var i=0;i<12;i++){
            let delta=q-source_uv-displacement(source_uv);source_uv+=delta*0.85;
            if length(delta*u.layer.zw)<0.01 || i>=i32(v(5)){break;}
        }
    }
    return sample_at(pos(source_uv));
}
