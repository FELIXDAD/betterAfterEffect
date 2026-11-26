// CC Glass Wipe, CC Twister and CC Warpomatic share one pipeline (u.stage).
fn evaluate(p:vec2f)->vec4f {
    switch u.stage {
        case 0u: { return glass(p,value(1u)*0.01); }
        case 1u: { return twist(p,value(2u)); }
        default: { return warp(p,value(1u)*0.01); }
    }
}
