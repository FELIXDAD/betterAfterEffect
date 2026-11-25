const luma = vec3f(.2126,.7152,.0722);
fn linear(c:vec3f)->vec3f { return sign(c)*pow(abs(c),vec3f(2.4)); }
fn encoded(c:vec3f)->vec3f { return sign(c)*pow(abs(c),vec3f(1./2.4)); }
fn least(c:vec3f)->f32 { return min(c.r,min(c.g,c.b)); }
fn greatest(c:vec3f)->f32 { return max(c.r,max(c.g,c.b)); }

// Raising saturation compresses chroma as the first channel approaches
// either gamut boundary. Desaturation is a linear-light Rec.709 blend.
fn saturation(c:vec3f,s:f32)->vec3f {
    let y=dot(c,luma);
    var k=s;
    if s>1. {
        let chroma=max((y-least(c))/max(y,1e-8),(greatest(c)-y)/max(1.-y,1e-8));
        let knee=min(.98,.5/s+.13*max(0.,1.8-s));
        if chroma>knee {
            let t=clamp((chroma-knee)/(1.-knee),0.,1.);
            let h=t*t*(3.-2.*t);
            let curved=mix(knee*s,1.,h)+(1.-knee)*t*(1.-t)*(s*(1.-t)-.15*t);
            k=curved/chroma;
        }
    }
    return vec3f(y)+(c-y)*k;
}
