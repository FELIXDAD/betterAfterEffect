// Both classic emitters use the particle engine's birth intervals, animated
// producer trajectory and sprite rasterizer. The two AE menus order their
// animation presets differently. These arrays index the same motion presets.
const CLASSIC = 18u;
const CLASSIC_LE = 19u;
const CLASSIC_MODES = array(1u,2u,3u,4u,5u,6u,7u,8u,9u,11u,0u,14u,15u,17u,18u,16u,20u,21u);
const LE_MODES = array(1u,2u,3u,4u,5u,6u,7u,8u,9u,10u,11u,12u,13u,0u,15u,16u,17u,18u,19u,20u,21u,22u);

fn classic_particle(birth: Birth, age: f32) -> Sprite {
    let le = cfg.kind == CLASSIC_LE;
    let base = select(0u,3u,le);
    let choice = u32(f(base+9u));
    var mode = CLASSIC_MODES[clamp(choice,1u,18u)-1u];
    if le { mode = LE_MODES[clamp(choice,1u,22u)-1u]; }
    if mode == 0u { return Sprite(); }
    let q = noise3(hash(birth.id));
    let r = noise3(hash(birth.id)+4u)*2.-1.;
    let theta = TAU*q.x;
    let radial = vec2f(cos(theta),sin(theta));
    let w = cfg.size.x;
    var origin = cfg.size*.5;
    if le { origin = emitter_at(birth.time,pt(3)) + r.xy*vec2f(f(1),f(2))*cfg.size*.01; }
    let normalized_age = clamp(age/max(.00001,birth.life),0.,1.);
    let speed = f(base+1u)*w*.22;
    var v = radial*(.15+q.y);
    var gravity = f(base+4u)*w*.36;
    let turbulence = f(base+5u)*w*.012;
    var size = max(.65,w*.002)*(.6+q.z);
    var style = DISC;
    var opacity = f(base+6u)*.01*(1.-normalized_age);
    var spread = age;
    var angle = theta;
    switch mode {
        case 2u: { v = vec2f(.3+q.y,r.y*.15); }
        case 3u,4u: { // Fire and bonfire
            v = vec2f(r.x*select(.16,.55,mode==4u),-.5-q.y);
            gravity *= -.3;
            style = SOFT;
            size *= (2.-normalized_age)*select(1.,1.8,mode==4u);
        }
        case 5u,11u: { // Twirl and inward vortex
            v = rot(radial,age*select(4.,-6.,mode==11u))*(.25+q.y);
            if mode==11u { spread = (1.-normalized_age)*birth.life*.4; }
        }
        case 6u: { v = vec2f(r.x*.26,-.65-q.y); }
        case 7u: { spread = (1.-exp(-3.*age))/3.; size *= 2.; }
        case 8u: { origin += r.xy*cfg.size*.4; }
        case 9u,15u,19u: { // Sparkle, star light and firefly
            style = STAR;
            size *= select(1.7,2.8,mode==15u);
            opacity *= .4+.6*abs(sin(theta+age*select(13.,4.,mode==19u)));
            if mode==19u { style=SOFT; v*=.3; gravity*=.05; }
        }
        case 10u: { v = radial*(.25+q.y+sin(5.*theta)*.4); }
        case 12u,13u: {
            v = vec2f(.08*r.x,-1.-q.y);
            if mode==13u { v=vec2f(-v.y,v.x); }
        }
        case 14u: {
            origin += vec2f(r.x*cfg.size.x*.5,-cfg.size.y*.4);
            v=vec2f(.08*r.x,1.+q.y);
            style=LINE;
            angle=1.57;
        }
        case 16u,20u: { style=SPHERE; size*=select(2.8,1.7,mode==20u); }
        case 17u,18u: { // Smoke and bubbles rise and grow.
            style=select(SOFT,BUBBLE,mode==18u);
            size *= 2.+4.*normalized_age;
            v=vec2f(r.x*.3,-.3-q.y);
            gravity*=-.2;
        }
        case 21u,22u: {
            v=rot(radial,(1.+q.z)*age*5.)*sin(theta+age*select(3.,7.,mode==22u));
            style=select(STAR,BUBBLE,mode==22u);
        }
        default: {}
    }
    let phase = vec2f(theta,theta+r.z);
    let drift = turbulence*(sin(phase+age*vec2f(3.,5.))-sin(phase));
    let pos = origin+v*speed*spread+drift+vec2f(0.,.5*gravity*age*age);
    let color = mix(rgb(base+7u),rgb(base+8u),normalized_age);
    let half_size = select(vec2f(size),vec2f(size*3.,size*.5),style==LINE);
    return sprite(pos,half_size,angle,style,vec4f(color,opacity));
}
