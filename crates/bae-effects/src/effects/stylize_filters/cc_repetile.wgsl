fn effect(p:vec2f)->vec4f {
    let q=uv(p);let tile=floor(q);let inside=all(tile==vec2f(0));
    if inside{return source(p);}
    let mode=i32(v(5));if mode==11{return vec4f(0);}
    let parity=vec2i(tile)&vec2i(1);let odd=(parity.x^parity.y)!=0;
    var local=fract(q)-0.5;
    switch mode{
        case 2:{if odd {local.x=-local.x;}}
        case 3:{if odd {local.y=-local.y;}}
        case 4:{local*=vec2f(1.-2.*f32(parity.x),1.-2.*f32(parity.y));}
        case 5:{if odd {local=-local;}}
        case 6:{if odd {local=local.yx;}}
        case 7,8:{if odd {local=angle2(local,select(-1.,1.,mode==8)*1.57079633);}}
        case 9,13,14,15:{local=angle2(local,(tile.x+tile.y)*select(1.,-1.,mode==14)*1.57079633);}
        case 10:{local=angle2(local,floor(fract(sin(dot(tile,vec2f(91.7,17.3)))*43567.)*4.)*1.57079633);}
        case 16:{local.y=fract(local.y+0.5+tile.x*0.25)-0.5;}
        case 17:{local.x=fract(local.x+0.5+f32(parity.y)*0.5)-0.5;}
        default:{}
    }
    let st=fract(local+0.5);let c=source(pos(st));let w=v(6)*0.005;
    if w<=0. {return c;}
    let edge=min(st,1.-st);let other=select(vec2f(st.x,1.-st.y),vec2f(1.-st.x,st.y),edge.x<edge.y);
    return mix(c,source(pos(other)),0.5*(1.-smoothstep(0.,w,min(edge.x,edge.y))));
}
