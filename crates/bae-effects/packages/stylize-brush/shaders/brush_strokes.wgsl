fn brush_noise(p:vec2i)->vec2f{
    let i=vec2u(p);let n=(i.x*73856093u)^(i.y*19349663u);let h=(n^(n>>15u))*2246822519u;
    return vec2f(vec2u(h&65535u,((h^(h>>13u))*3266489917u)&65535u))/65535.;
}
@compute @workgroup_size(8,8)
fn brush_strokes(@builtin(global_invocation_id) id:vec3u){
    let p=bae_target_pixel(id);if !bae_in_target(p){return;}
    let pos=vec2f(p)+0.5;let orig=bae_load_input(p);
    var base=orig;
    switch params.paint_surface{case 2u:{base=vec4f(0);}case 3u:{base=vec4f(1);}case 4u:{base=vec4f(0,0,0,1);}default:{}}
    let width=max(.5,bae_scale(params.brush_size));
    let stroke=max(.5,bae_scale(params.stroke_length));
    let angle=radians(params.stroke_angle);
    let dir=vec2f(cos(angle),sin(angle));let normal=vec2f(-dir.y,dir.x);
    let grid=vec2f(max(1.,stroke),max(1.,2.*width))/max(.1,params.stroke_density);
    let local=vec2f(dot(pos,dir),dot(pos,normal))/grid;
    let cell=vec2i(floor(local));
    var coverage=0.;var ink=vec4f(0);
    for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
        let k=cell+vec2i(x,y);let noise=brush_noise(k);
        let center=(vec2f(k)+.5+(noise-.5)*params.stroke_randomness*.7)*grid;
        let delta=center-local*grid;
        let d=length(vec2f(delta.x/max(.5,stroke),delta.y/max(.5,width)));
        let w=1.-smoothstep(.72,1.0,d);
        if w>coverage{coverage=w;ink=bae_sample_input(center.x*dir+center.y*normal);}
    }}
    let color=mix(base,ink,coverage);
    bae_store(p,bae_depth_clamp(mix(color,orig,params.blend_with_original)));
}