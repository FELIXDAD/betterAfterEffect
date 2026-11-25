fn noise_hls_auto(c:vec4f, px:BaePixel)->vec4f {
    return hls_noise(c,px, frame.time * params.speed);
}
