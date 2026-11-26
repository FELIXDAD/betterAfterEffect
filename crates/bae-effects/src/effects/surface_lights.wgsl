// Scene light records are supplied in layer coordinates. The surface normal
// points towards the viewer, opposite to AE's positive camera-space Z.
fn surface_lights(p:vec2f,n:vec3f,color:vec3f,ambient:f32,diffuse:f32,specular:f32,roughness:f32,metal:f32)->vec3f {
    var result=vec3f(0);
    for(var i=0u;i<u32(value(64u));i++) {
        let slot=65u+3u*i;
        let position=u.values[slot];
        let direction=u.values[slot+1u];
        let light=u.values[slot+2u];
        if position.w==0.0 { result+=color*light.rgb*ambient; continue; }
        var l=-direction.xyz;
        var cone=1.0;
        if position.w>=2.0 {
            l=normalize(position.xyz-vec3f(p/u.scale,0)+vec3f(0,0,0.00001));
            if position.w==3.0 {
                let outer=direction.w;
                cone=smoothstep(outer,min(1.0,outer+max(0.0001,(1.0-outer)*light.w)),dot(-l,direction.xyz));
            }
        }
        l.z=-l.z;
        let shine=pow(max(0.0,reflect(-l,n).z),1.0/max(0.001,roughness))*specular;
        result+=light.rgb*cone*(color*max(0.0,dot(n,l))*diffuse+mix(vec3f(1),color,metal)*shine);
    }
    return result;
}
