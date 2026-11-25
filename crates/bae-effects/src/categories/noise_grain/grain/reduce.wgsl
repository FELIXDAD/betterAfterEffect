fn evaluate(p:vec2f)->vec4f {
    let count=u.index;
    let start=i32(p.x)*i32(count);
    var sum=vec3f(0.);
    for(var y=0;y<i32(count);y++){
        for(var x=0;x<16;x++){sum+=textureLoad(map_a,vec2i(x,start+y),0).rgb;}
    }
    return vec4f(sqrt(sum/(20.*f32(count))),1.);
}
