// Levels (ADBE Easy Levels2, AE-FX-038). The five values apply to the channel
// chosen by Channel (RGB = R, G and B together). Straight colour.

// Clip popups: 1 On, 2 Off, 3 Off for 32 bpc Color (on in 8 / 16 bpc).
fn levels_clips(option: u32) -> bool {
    return option == 1u || (option == 3u && (frame.flags & BAE_FRAME_FLOAT) == 0u);
}

fn levels_map(x: f32, ib: f32, iw: f32, g: f32, ob: f32, ow: f32) -> f32 {
    var t: f32;
    if (iw == ib) {
        t = select(0.0, 1.0, x >= ib);
    } else {
        t = (x - ib) / (iw - ib);
    }
    // Mirrored below input black (AE: with clipping off, values below input
    // black keep falling instead of stopping at output black).
    t = sign(t) * pow(abs(t), 1.0 / max(g, 1e-4));
    var y = ob + (ow - ob) * t;
    if (levels_clips(params.clip_black)) {
        y = max(y, min(ob, ow));
    }
    if (levels_clips(params.clip_white)) {
        y = min(y, max(ob, ow));
    }
    return y;
}

fn levels_ch(x: f32) -> f32 {
    return levels_map(x, params.input_black, params.input_white, params.gamma,
        params.output_black, params.output_white);
}

fn levels(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    var s = bae_unpremultiply(c);
    switch params.channel {
        case 1u: {
            s = vec4<f32>(levels_ch(s.r), levels_ch(s.g), levels_ch(s.b), s.a);
        }
        case 2u: {
            s.r = levels_ch(s.r);
        }
        case 3u: {
            s.g = levels_ch(s.g);
        }
        case 4u: {
            s.b = levels_ch(s.b);
        }
        default: {
            // Alpha: transparent pixels can become visible; their straight
            // colour is black (AE behaviour for premultiplied buffers).
            s.a = clamp(levels_ch(s.a), 0.0, 1.0);
        }
    }
    return vec4<f32>(s.rgb * s.a, s.a);
}
