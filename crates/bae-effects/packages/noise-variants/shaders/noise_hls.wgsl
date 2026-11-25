fn noise_hls(c:vec4f, px:BaePixel)->vec4f {
    return hls_noise(c,px, params.phase / 360.0);
}
