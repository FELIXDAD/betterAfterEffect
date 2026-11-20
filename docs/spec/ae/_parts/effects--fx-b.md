## Blur & Sharpen effects

Conventions for every effect item in this part: "bpc" and "GPU" come from the docsforadobe first-party match-name table (GPU = AE version that added GPU acceleration; "—" = CPU only). Parameter defaults are the values AE 26.0 reports for a freshly applied effect on a 320 × 240 layer (point defaults scale with the layer: "layer centre" = [w/2, h/2]). Some percentage sliders store a 0…255 internal value and display a percentage (noted per parameter); scripting `value` and Lottie export the internal value. Unless stated otherwise every effect processes **premultiplied** RGBA in the layer's working bit depth, never clamps in 32 bpc, scales pixel-distance parameters by the downsample factor (see effects.md [fx-a], Downsample-factor scaling) and follows the generic Repeat Edge Pixels and output-bounds rules (see effects.md [fx-a], Output bounds expansion and Repeat Edge Pixels). Effect-category name: Blur & Sharpen (模糊和锐化).

### AE-FX-300 · Gaussian Blur (高斯模糊)

- **What it does:** Blurs the layer with a separable Gaussian kernel; the most-used blur in AE. GPU accelerated; layer Quality does not change its result.
- **Exact semantics:**
  - matchName `ADBE Gaussian Blur 2`; category Blur & Sharpen; 32 bpc; GPU since AE 13.8 (CC 2015.3). Expands output bounds: yes, unless Repeat Edge Pixels is on.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Blurriness (模糊度) | `ADBE Gaussian Blur 2-0001` | float slider | 25.0 (AE 26.0 dump; UNVERIFIED: earlier versions show 0.0 on apply) | slider 0…50; valid 0…3000 (UNVERIFIED) | pixels | Downsample-scaled. |
  | 2 | Blur Dimensions (模糊方向) | `ADBE Gaussian Blur 2-0002` | popup | 1 = Horizontal and Vertical | 1 Horizontal and Vertical (水平和垂直), 2 Horizontal (水平), 3 Vertical (垂直) | — | — |
  | 3 | Repeat Edge Pixels (重复边缘像素) | `ADBE Gaussian Blur 2-0003` | checkbox | on (1) (AE 26.0 dump; UNVERIFIED: off in older versions) | 0/1 | — | — |

  ```
  sigma = blurriness * 0.3            // UNVERIFIED: empirical; lottie-web (kBlurrinessToSigma)
                                      // and Skottie (kBlurSizeToSigma) both use 0.3
  sx = (dims == Vertical)   ? 0 : sigma * downsample_x
  sy = (dims == Horizontal) ? 0 : sigma * downsample_y
  if sx == 0 and sy == 0: return input            // Blurriness 0 = identity
  r  = ceil(3 * max(sx, sy))                       // kernel half-width (UNVERIFIED: AE truncation)
  addressing = repeat_edge ? CLAMP_TO_EDGE : TRANSPARENT_BLACK
  tmp = convolve_1d_x(premultiplied(input), gauss(sx), addressing)
  out = convolve_1d_y(tmp, gauss(sy), addressing)
  out_rect = repeat_edge ? in_rect : in_rect.expand(rx, ry)   // rx = ceil(3*sx)
  ```
  - Weights are normalised to sum 1 so a flat area keeps its value; with Repeat Edge Pixels off the edges fade to transparent because outside samples are 0.
  - 8/16 bpc: result rounded to the channel depth; 32 bpc: no clamping (super-white values spread).
- **Priority:** P0 — the default blur in nearly every AE project.
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Introduced as the replacement for Gaussian Blur (Legacy) (AE-FX-301), giving "a slightly different rendering result" (Adobe help); it derives from the Premiere Pro Gaussian Blur (secondary source, ProLost). Lottie: `ty: 29` Gaussian Blur effect, values `sigma` (= Blurriness, not a real sigma), `direction` (1/2/3), `wrap` (Repeat Edge Pixels); lottie-web maps it to SVG `feGaussianBlur stdDeviation = 0.3·Blurriness`. Implementation hint: for sigma > ~8 px downsample by 2ⁿ, blur, and upsample (or use 3-pass box approximation, AE-FX-302 formula) to keep cost O(1) in radius; for exact match keep the direct separable kernel for small sigma. Blur is linear, so with a linearised working space it blurs light (see compositing-3d-color.md, Color management).

### AE-FX-301 · Gaussian Blur (Legacy) (高斯模糊（旧版）)

- **What it does:** The pre-2015 AE Gaussian Blur, kept so old projects render unchanged; listed under Obsolete.
- **Exact semantics:**
  - matchName `ADBE Gaussian Blur`; category Obsolete; 32 bpc; GPU —. Expands output bounds: yes unless Repeat Edge Pixels.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Blurriness (模糊度) | `ADBE Gaussian Blur-0001` | float slider | 0.0 | slider 0…50; valid 0…1000 (UNVERIFIED) | pixels | — |
  | 2 | Blur Dimensions (模糊方向) | `ADBE Gaussian Blur-0002` | popup | 1 = Horizontal and Vertical | Horizontal and Vertical / Horizontal / Vertical | — | — |
  | 3 | Repeat Edge Pixels (重复边缘像素) | `ADBE Gaussian Blur-0003` | checkbox | off (UNVERIFIED) | 0/1 | — | UNVERIFIED: present in the legacy version. |

  - Algorithm: same structure as AE-FX-300 (separable Gaussian, premultiplied). UNVERIFIED: the legacy kernel is slightly narrower/wider than the new one for the same Blurriness; BAE uses the AE-FX-300 kernel and accepts the small difference until measured.
- **Priority:** P1 — old projects and many tutorials/presets reference it.
- **Owner:** bae-effects
- **Notes:** Scripting `addProperty("ADBE Gaussian Blur")` still applies it. BAE decision: implement as an alias of AE-FX-300 with its own matchName and default; record the reference renders needed to calibrate the kernel difference.

### AE-FX-302 · Fast Box Blur (快速方框模糊)

- **What it does:** Iterated box blur; 1 iteration gives a boxy blur, ~3 iterations approximate a Gaussian. GPU accelerated and very fast at large radii.
- **Exact semantics:**
  - matchName `ADBE Box Blur2`; category Blur & Sharpen; 32 bpc; GPU since AE 14.2 (CC 2017.1). Expands output bounds: yes unless Repeat Edge Pixels.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Blur Radius (模糊半径) | `ADBE Box Blur2-0001` | float slider | 0.0 (UNVERIFIED) | slider 0…50; valid 0…32767 (UNVERIFIED) | pixels | Downsample-scaled. |
  | 2 | Iterations (迭代) | `ADBE Box Blur2-0002` | integer slider | 3 (UNVERIFIED) | 1…50 (UNVERIFIED) | count | — |
  | 3 | Blur Dimensions (模糊方向) | `ADBE Box Blur2-0003` | popup | 1 = Horizontal and Vertical | Horizontal and Vertical / Horizontal / Vertical | — | — |
  | 4 | Repeat Edge Pixels (重复边缘像素) | `ADBE Box Blur2-0004` | checkbox | off (UNVERIFIED) | 0/1 | — | — |

  ```
  // one iteration = horizontal box then vertical box, each of width w = 2r+1
  // fractional r: the two outermost taps get weight frac(r) (UNVERIFIED: AE handling)
  for i in 1..iterations:
      if dims != Vertical:   img = box_1d_x(img, r, addressing)
      if dims != Horizontal: img = box_1d_y(img, r, addressing)
  // effective Gaussian-equivalent standard deviation:
  //   sigma^2 = iterations * r * (r + 1) / 3      (variance of a (2r+1)-tap box = r(r+1)/3)
  out_rect = repeat_edge ? in_rect : in_rect.expand(iterations * ceil(r))
  ```
  - Radius 0 = identity regardless of Iterations.
- **Priority:** P0 — common fast blur; also the documented equivalent of Fast Blur (Legacy).
- **Owner:** bae-effects
- **Notes:** Naming history: AE originally had "Fast Blur" (`ADBE Fast Blur`) and "Box Blur" (`ADBE Box Blur`); the box blur was rebuilt as "Fast Box Blur" with matchName `ADBE Box Blur2`, while the old Fast Blur became "Fast Blur (Legacy)" (AE-FX-303). UNVERIFIED: the rename shipped in AE 2019 (16.0) or AE 2020 (17.0). Adobe help: "the match name of the Fast Box Blur effect is ADBE Box Blur2. The older match name ADBE Box Blur still works to apply the effect, but with the older effect name Box Blur". Implementation hint: running-sum box filter (O(1) per pixel per pass) in a compute shader, one workgroup per row/column.

### AE-FX-303 · Fast Blur (Legacy) and Box Blur (legacy matchName) (快速模糊（旧版）和方框模糊)

- **What it does:** Two legacy blur identities kept for old projects: Fast Blur (Legacy) and the pre-rename Box Blur.
- **Exact semantics:**
  - **Fast Blur (Legacy) (快速模糊（旧版）):** matchName `ADBE Fast Blur`; category Obsolete; 32 bpc; GPU —. Parameters: 1 Blurriness (模糊度) `ADBE Fast Blur-0001` float slider default 0.0, slider 0…50, valid 0…32767 (UNVERIFIED), pixels; 2 Blur Dimensions (模糊方向) `ADBE Fast Blur-0002` popup Horizontal and Vertical (1) / Horizontal / Vertical; 3 Repeat Edge Pixels (重复边缘像素) `ADBE Fast Blur-0003` checkbox default off. Algorithm: a 3-pass box approximation of a Gaussian (UNVERIFIED); Adobe states Fast Box Blur "can replicate the rendering result of Fast Blur (Legacy)", i.e. Fast Box Blur with Iterations = 3 and a radius derived from Blurriness. UNVERIFIED mapping: `box_radius ≈ blurriness / 2`.
  - **Box Blur (方框模糊):** matchName `ADBE Box Blur`; listed under Obsolete. Same implementation and parameters as Fast Box Blur (AE-FX-302) (params `ADBE Box Blur-0001…0004`: Blur Radius, Iterations, Blur Dimensions, Repeat Edge Pixels), shown with the old display name "Box Blur". Adobe help: Iterations "around 3 gives a blur similar in quality to Fast Blur"; "the default value creates boxy results" (UNVERIFIED: Box Blur default Iterations = 1, Fast Box Blur default 3).
  - Both expand output bounds unless Repeat Edge Pixels.
- **Priority:** P1 — needed to open older projects and presets correctly.
- **Owner:** bae-effects
- **Notes:** BAE decision: map `ADBE Box Blur` and `ADBE Box Blur2` to the same kernel; keep the original matchName on the instance so round-trip is lossless. Calibrate the Fast Blur → box radius mapping with reference renders before claiming bit-exactness.

### AE-FX-304 · Directional Blur (定向模糊)

- **What it does:** Blurs along one direction, symmetric about each pixel, to fake motion.
- **Exact semantics:**
  - matchName `ADBE Motion Blur`; category Blur & Sharpen; 32 bpc; GPU since AE 15.0 (CC 2018). Expands output bounds: yes (no Repeat Edge Pixels option).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Direction (方向) | `ADBE Motion Blur-0001` | angle | 90° (AE 26.0 dump; UNVERIFIED: older versions 0°) | any | degrees | 0° = vertical streak, 90° = horizontal (UNVERIFIED; AE angle 0° points up, clockwise positive). θ and θ+180° are identical. |
  | 2 | Blur Length (模糊长度) | `ADBE Motion Blur-0002` | float slider | 10.0 (AE 26.0 dump) | slider 0…100; valid 0…32767 (UNVERIFIED) | pixels | Downsample-scaled along the blur direction. |

  ```
  d = (sin(dir), -cos(dir))             // AE angle: 0° up, clockwise (UNVERIFIED axis convention)
  L = blur_length                        // total streak length (UNVERIFIED: or half-length)
  n = max(1, ceil(L))
  out(p) = (1/(n+1)) * sum_{k=0..n} sample_bilinear(in, p + d * (k/n - 0.5) * L)   // box kernel, UNVERIFIED shape
  out_rect = in_rect.expand(|d.x|*L/2, |d.y|*L/2)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Despite the matchName (`ADBE Motion Blur`) it is unrelated to layer motion blur (see compositing-3d-color.md, motion-blur rendering). Adobe help: "a setting of 180° and a setting of 0° produce the same results". UNVERIFIED: AE uses a triangular (tent) weighting rather than a box; measure with a single-pixel impulse. Implementation hint: implement as repeated 1D passes along the direction (log-step "line" blur) for O(log L) cost.

### AE-FX-305 · Radial Blur (径向模糊)

- **What it does:** Blurs around a centre point: Spin blurs along concentric arcs (camera rotation), Zoom blurs along rays (camera dolly).
- **Exact semantics:**
  - matchName `ADBE Radial Blur`; category Blur & Sharpen; 32 bpc; GPU —. Expands output bounds: no (UNVERIFIED; Zoom content beyond the layer edge is clipped).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Amount (数量) | `ADBE Radial Blur-0001` | float slider | 10.0 | 0…100 (UNVERIFIED valid range) | Spin: degrees of rotation; Zoom: blur length (relative) | — |
  | 2 | Center (中心) | `ADBE Radial Blur-0002` | point | layer centre | any | layer px | — |
  | 3 | Type (类型) | `ADBE Radial Blur-0003` | popup | 1 = Spin | 1 Spin (旋转), 2 Zoom (缩放) | — | — |
  | 4 | Antialiasing (Best Quality) (消除锯齿（最佳品质）) | `ADBE Radial Blur-0004` | popup | 1 = Low | 1 Low (低), 2 High (高) | — | Ignored at Draft quality. |
  | — | (hidden) Random Seed | `ADBE Radial Blur-0006` | float | 0 | — | — | Hidden parameter seen by scripting (`-0005` is an unnamed hidden group). UNVERIFIED role: seeds the Draft-quality stochastic sampling. |

  ```
  v = p - center; r = |v|; phi = atan2(v)
  Spin:  span = radians(amount)                       // total arc angle (UNVERIFIED: ±amount/2)
         samples along phi + t*span, t in [-0.5, 0.5], radius r
  Zoom:  scale range s in [1 - amount/100 * k, 1]      // UNVERIFIED: k ≈ 1, samples center + v*s
  N = samples ∝ arc/ray length in px (Best: Low or High density; Draft: few jittered samples, grainy)
  out(p) = average of bilinear samples (premultiplied)
  ```
  - At Draft layer quality no antialiasing is applied and the result is grainy (Adobe help); this grain can flicker on interlaced output.
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Lottie: no effect type. Implementation hint: polar-resample to (r, φ) texture, 1D blur along φ (Spin) or r (Zoom), resample back; or multi-pass doubling (each pass samples at ±step, step doubles) for O(log N) cost. BAE decision: Draft-quality grain need not be reproduced bit-exactly.

### AE-FX-306 · Sharpen (锐化)

- **What it does:** Increases contrast where colours change, using a small fixed kernel. Not affected by layer Quality; GPU accelerated.
- **Exact semantics:**
  - matchName `ADBE Sharpen`; category Blur & Sharpen; 32 bpc; GPU since AE 13.8. Expands output bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Sharpen Amount (锐化量) | `ADBE Sharpen-0001` | integer slider | 0 | slider 0…100; valid 0…4000 (UNVERIFIED) | — | 0 = identity. |

  ```
  // UNVERIFIED kernel: 3x3 Laplacian-style unsharp
  k = amount / 100 (UNVERIFIED scale)
  blur = mean of 3x3 neighbourhood (clamp-to-edge)
  out = in + k * (in - blur)       // per channel, unpremultiplied colour, alpha unchanged (UNVERIFIED)
  8/16 bpc: clamp to [0,1]; 32 bpc: no clamp
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** UNVERIFIED: whether Sharpen operates on premultiplied or straight colour (edge halos against transparency differ). Calibrate with reference renders.

### AE-FX-307 · Unsharp Mask (钝化蒙版)

- **What it does:** Classic unsharp masking: adds back the difference between the image and a Gaussian-blurred copy, only where the difference exceeds a threshold.
- **Exact semantics:**
  - matchName `ADBE Unsharp Mask2`; category Blur & Sharpen; 32 bpc; GPU —. Expands output bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Amount (数量) | `ADBE Unsharp Mask2-0001` | float slider | 50.0 (UNVERIFIED) | slider 0…500; valid 0…500 (UNVERIFIED) | % | — |
  | 2 | Radius (半径) | `ADBE Unsharp Mask2-0002` | float slider | 1.0 (UNVERIFIED) | slider 0.1…10; valid 0.1…250 (UNVERIFIED) | pixels | Downsample-scaled. |
  | 3 | Threshold (阈值) | `ADBE Unsharp Mask2-0003` | integer slider | 0 (UNVERIFIED) | 0…255 | levels (8-bit scale) | 0 = sharpen everywhere. |

  ```
  blur = gaussian(in, sigma = radius)            // UNVERIFIED: Photoshop convention radius ≈ sigma
  diff = in - blur                               // per channel
  mask = |diff| * 255 > threshold                // hard threshold (UNVERIFIED: per channel vs luma)
  out  = in + mask * (amount / 100) * diff
  8/16 bpc clamp; 32 bpc no clamp
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Older instance matchName `ADBE Unsharp Mask` (Obsolete list) — BAE maps it to the same implementation. Adobe help: a lower Threshold "produces a greater result"; too low "can generate noise".

### AE-FX-308 · Camera Lens Blur (相机镜头模糊)

- **What it does:** Simulates out-of-focus lens bokeh with a polygonal iris, an optional depth/blur map layer and highlight boosting.
- **Exact semantics:**
  - matchName `ADBE Camera Lens Blur`; category Blur & Sharpen; 32 bpc; GPU —. Expands output bounds: yes unless Repeat Edge Pixels.
  - Parameters (AE order; param numbers per AE 26 enum dump where known):

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Blur Radius (模糊半径) | `ADBE Camera Lens Blur-0001` | float slider | 5.0 (UNVERIFIED) | 0…500 (UNVERIFIED) | pixels | — |
  | 2 | Iris Properties (光圈属性) | group | group | — | — | — | — |
  | 3 | Shape (形状) | `ADBE Camera Lens Blur-0003` | popup | Hexagon (4) (UNVERIFIED) | Triangle, Square, Pentagon, Hexagon, Heptagon, Octagon, Nonagon, Decagon | — | — |
  | 4 | Roundness (圆度) | UNVERIFIED `-0004` | float | 0 % | 0…100 | % | — |
  | 5 | Aspect Ratio (长宽比) | UNVERIFIED `-0005` | float | 1 | 0.01…100 (UNVERIFIED) | ratio | — |
  | 6 | Rotation (旋转) | UNVERIFIED `-0006` | angle | 0° | any | degrees | — |
  | 7 | Diffraction Fringe (衍射条纹) | UNVERIFIED `-0007` | float | 0 | 0…100 | — | — |
  | 8 | Blur Map (模糊图) | group | group | — | — | — | — |
  | 9 | Layer (图层) | UNVERIFIED `-0010` | layer | None | — | — | Depth/blur map. |
  | 10 | Channel (通道) | `ADBE Camera Lens Blur-0011` | popup | Luminance (1) | Luminance, Red, Green, Blue, Alpha | — | — |
  | 11 | Placement (位置) | `ADBE Camera Lens Blur-0012` | popup | Center Map (1) | Center Map, Stretch Map to Fit | — | — |
  | 12 | Blur Focal Distance (模糊焦距) | UNVERIFIED | float | 0 | 0…255 | map value | Map value that stays sharp. |
  | 13 | Invert Blur Map (反转模糊图) | UNVERIFIED | checkbox | off | — | — | — |
  | 14 | Highlight (高光) | group | group | — | — | — | — |
  | 15 | Gain (增益) | UNVERIFIED | float | 0 | 0…100 | — | — |
  | 16 | Threshold (阈值) | UNVERIFIED | float | 255 | 0…255 | levels | — |
  | 17 | Saturation (饱和度) | UNVERIFIED | float | 0 | 0…100 | — | — |
  | 18 | Edge Behavior: Repeat Edge Pixels (边缘行为：重复边缘像素) | UNVERIFIED | checkbox | off | — | — | — |
  | 19 | Use "Linear" Working Space (使用"线性"工作空间) | UNVERIFIED | checkbox | off | — | — | Blurs in linear light. |

  - Algorithm: per pixel, radius = Blur Radius × |map(p) − focal| / 255 (or constant without a map); scatter/gather convolution with a polygon iris kernel (Shape, Roundness, Aspect Ratio, Rotation), highlights above Threshold boosted by Gain. UNVERIFIED in detail.
- **Priority:** P2 — heavy, rarely needed for basic motion graphics.
- **Owner:** bae-effects
- **Notes:** Chinese labels in this table are best-effort（译名待核）. Adobe help notes a "forward and backward compatibility" change (Camera Lens Blur rewritten in AE CC 2014 (13.0), UNVERIFIED version). Cross-ref compositing-3d-color.md, Cameras (DOF) for the 3D camera depth-of-field, which is separate.

### AE-FX-309 · Bilateral Blur (双向模糊)

- **What it does:** Edge-preserving blur (similar to Photoshop Surface Blur); high-contrast edges are blurred less.
- **Exact semantics:**
  - matchName `ADBE Bilateral`; category Blur & Sharpen; 32 bpc; GPU —; expands bounds: no.
  - Parameters: 1 Radius (半径) float default 0 (UNVERIFIED), slider 0…20 px; 2 Threshold (阈值) float default 10 (UNVERIFIED), 0…100; 3 Colorize (着色) checkbox default on (UNVERIFIED). Param matchNames `ADBE Bilateral-0001…0003` (UNVERIFIED order).
  - Algorithm: bilateral filter, spatial weight Gaussian(radius), range weight Gaussian(Threshold) on luminance; Colorize off → operates on luminance only and outputs monochrome; on → per channel (Adobe help).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-310 · Channel Blur (通道模糊)

- **What it does:** Gaussian-blurs R, G, B and A independently by different amounts.
- **Exact semantics:**
  - matchName `ADBE Channel Blur`; 32 bpc; GPU —; expands bounds: yes unless Repeat Edge Pixels.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Red Blurriness (红色模糊度) | `ADBE Channel Blur-0001` | float slider | 0 | slider 0…50 (UNVERIFIED) | pixels | — |
  | 2 | Green Blurriness (绿色模糊度) | `ADBE Channel Blur-0002` | float slider | 0 | same | pixels | — |
  | 3 | Blue Blurriness (蓝色模糊度) | `ADBE Channel Blur-0003` | float slider | 0 | same | pixels | — |
  | 4 | Alpha Blurriness (Alpha 模糊度) | `ADBE Channel Blur-0004` | float slider | 0 | same | pixels | — |
  | 5 | Edge Behavior: Repeat Edge Pixels (边缘特性：重复边缘像素) | `ADBE Channel Blur-0005` | checkbox | on (1) | 0/1 | — | — |
  | 6 | Blur Dimensions (模糊方向) | `ADBE Channel Blur-0006` | popup | Horizontal and Vertical (1) | Horizontal and Vertical / Horizontal / Vertical | — | — |

  - Algorithm: per channel Gaussian (same sigma mapping as AE-FX-300) on straight (unpremultiplied) channels, then re-premultiply (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-311 · Compound Blur (复合模糊)

- **What it does:** Blurs each pixel by an amount taken from the luminance of a control ("blur") layer.
- **Exact semantics:**
  - matchName `ADBE Compound Blur`; 32 bpc; GPU —; expands bounds: no (UNVERIFIED).
  - Parameters: 1 Blur Layer (模糊图层) layer, default = this layer; 2 Maximum Blur (最大模糊) float default 20 px (UNVERIFIED), slider 0…100; 3 If Layer Sizes Differ: Stretch Map to Fit (如果图层大小不同：伸缩对应图以适合) checkbox default on; 4 Invert Blur (反转模糊) checkbox default off. matchNames `ADBE Compound Blur-0001…0004` (UNVERIFIED).
  - Algorithm: `radius(p) = MaxBlur × L(map(p))` (L = luminance 0…1, inverted if Invert Blur); variable-radius blur; map centred or stretched.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** The control layer is read after its masks and effects (see effects.md [fx-a], Layer parameter).

### AE-FX-312 · Smart Blur (智能模糊)

- **What it does:** Blurs while preserving lines and edges; can also output edges only.
- **Exact semantics:**
  - matchName `ADBE Smart Blur`; 16 bpc; GPU —; expands bounds: no.
  - Parameters: 1 Radius (半径) float default 3 (UNVERIFIED), 0.1…100; 2 Threshold (阈值) float default 25 (UNVERIFIED), 0.1…100; 3 Mode (模式) popup `ADBE Smart Blur-0003`: 1 Normal (正常), 2 Edge Only (仅限边缘), 3 Overlay Edge (叠加边缘), default Normal.
  - Algorithm: neighbourhood average restricted to pixels whose value differs from the centre by less than Threshold; Edge Only draws black/white edges, Overlay Edge draws white edges over the image (Adobe help).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Adobe zh-CN help heading says "特殊模糊", body says "智能模糊"（译名待核）.

### AE-FX-313 · Camera-Shake Deblur (摄像机抖动去模糊)

- **What it does:** Reduces motion blur caused by camera shake by borrowing detail from sharper neighbouring frames.
- **Exact semantics:**
  - matchName `ADBE CameraShakeDeblur`; 32 bpc; GPU —; temporal (reads other frames of the layer).
  - Parameters (UNVERIFIED order): Blurriness (模糊度), Detail Quality (细节品质) popup `-0002`: Standard (标准), High Quality (slower) (高品质（较慢）); Blur Duration, Stability Threshold, Detail Threshold, Use Linear, Analysis/Process (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Introduced AE CC 2017 (14.0) (UNVERIFIED).

### AE-FX-314 · CC Cross Blur (CC 交叉模糊)

- **What it does:** Separate horizontal and vertical box blurs with a transfer mode, giving a cross-shaped streak.
- **Exact semantics:**
  - matchName `CS CrossBlur`; 32 bpc; GPU —; expands bounds: UNVERIFIED.
  - Parameters (UNVERIFIED): Radius X (半径 X), Radius Y (半径 Y), Transfer Mode (传输模式), Repeat Edge Pixels (重复边缘像素).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** CycoreFX (third-party bundled). Names（译名待核）.

### AE-FX-315 · CC Radial Blur (CC 径向模糊)

- **What it does:** Cycore radial/zoom/rotate blur variants.
- **Exact semantics:**
  - matchName `CC Radial Blur`; 32 bpc; GPU —.
  - Type (类型) popup `CC Radial Blur-0001`: 1 Straight Zoom, 2 Fading Zoom, 3 Centered Zoom, (separator), 5 Rotate, 6 Scratch, 7 Rotate Fading (separators occupy an index in AE popups). Other params (UNVERIFIED): Amount, Quality, Center.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Names（译名待核）.

### AE-FX-316 · CC Radial Fast Blur (CC 径向快速模糊)

- **What it does:** Fast zoom blur from a centre point, with brightest/darkest zoom modes.
- **Exact semantics:**
  - matchName `CC Radial Fast Blur`; 16 bpc; GPU —.
  - Parameters (UNVERIFIED order): Center (中心), Amount (数量), Zoom (缩放) popup `CC Radial Fast Blur-0003`: 1 Standard, 2 Brightest, 3 Darkest.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-317 · CC Vector Blur (CC 矢量模糊)

- **What it does:** Blurs along vectors derived from a property map (a layer channel), giving flowing streaks.
- **Exact semantics:**
  - matchName `CC Vector Blur`; 16 bpc; GPU —.
  - Type (类型) popup `CC Vector Blur-0001`: Natural, Constant Length, Perpendicular, Direction Center, Direction Fading. Property (属性) popup `CC Vector Blur-0006`: Red, Green, Blue, Alpha, Luminance, Lightness, Hue, Saturation. Other params (UNVERIFIED): Amount, Angle Offset, Ridge Smoothness, Vector Map (layer), Map Softness.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-318 · Reduce Interlace Flicker (减少交错闪烁)

- **What it does:** Vertical-only blur that softens thin horizontal lines that flicker on interlaced displays.
- **Exact semantics:**
  - matchName `ADBE Reduce Interlace Flicker`; category Obsolete in the match-name table (still shipped); 32 bpc; GPU —.
  - Parameter: 1 Softness (柔和度) float default 0 (UNVERIFIED), 0…3 px (UNVERIFIED).
  - Algorithm: vertical Gaussian/tent blur with sigma ∝ Softness, no horizontal blur.
- **Priority:** P2 — interlaced delivery is rare.
- **Owner:** bae-effects
- **Notes:** —

## Stylize effects

Effect-category name: Stylize (风格化). Conventions as stated at the start of the Blur & Sharpen section.

### AE-FX-319 · Glow (发光)

- **What it does:** Finds the bright (or opaque-edge) parts of the layer, blurs them into a diffuse aura, optionally recolours the aura with a two-colour gradient or an arbitrary map, and composites it with the original using a chosen blend mode.
- **Exact semantics:**
  - matchName `ADBE Glo2`; category Stylize; 32 bpc; GPU since AE 14.1 (CC 2017). Expands output bounds: yes (by the glow blur radius).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Glow Based On (发光基于) | `ADBE Glo2-0001` | popup | 2 = Color Channels | 1 Alpha Channel (Alpha 通道), 2 Color Channels (颜色通道) | — | — |
  | 2 | Glow Threshold (发光阈值) | `ADBE Glo2-0002` | float slider (percent display) | 60.0 % (internal 153 of 255) | 0…100 % | % brightness | Scripting/Lottie value is the 0…255 internal number. |
  | 3 | Glow Radius (发光半径) | `ADBE Glo2-0003` | float slider | 10.0 | slider 0…100; valid 0…1000 (UNVERIFIED) | pixels | Downsample-scaled. |
  | 4 | Glow Intensity (发光强度) | `ADBE Glo2-0004` | float slider | 1.0 | slider 0…4; valid 0…255 (UNVERIFIED) | multiplier | — |
  | 5 | Composite Original (合成原始项目) | `ADBE Glo2-0005` | popup | 2 = Behind | 1 On Top (顶端), 2 Behind (后面), 3 None (无) | — | — |
  | 6 | Glow Operation (发光操作) | `ADBE Glo2-0006` | popup | 3 = Add | 1 None, 2 Normal, 3 Add, 4 Multiply, 5 Dissolve, 6 Screen, 7 Overlay, 8 Soft Light, 9 Hard Light, 10 Darken, 11 Lighten, 12 Difference, 13 Hue, 14 Saturation, 15 Color, 16 Luminosity, 17 Color Dodge, 18 Color Burn, 19 Exclusion, 20 Stencil Alpha, 21 Stencil Luma, 22 Silhouette Alpha, 23 Silhouette Luma, 24 Luminescent Premultiply, 25 Alpha Add | — | Blend-mode formulas: see compositing-3d-color.md, Blend modes (Chinese names as there). |
  | 7 | Glow Colors (发光颜色) | `ADBE Glo2-0007` | popup | 1 = Original Colors | 1 Original Colors (原始颜色), 2 A & B Colors (A 和 B 颜色), 3 Arbitrary Map (任意映射) | — | Arbitrary Map is loaded from an .amp file via the effect's Options… link. |
  | 8 | Color Looping (颜色循环) | `ADBE Glo2-0008` | popup | 3 = Triangle A>B>A | 1 Sawtooth A>B (锯齿 A>B), 2 Sawtooth B>A (锯齿 B>A), 3 Triangle A>B>A (三角形 A>B>A), 4 Triangle B>A>B (三角形 B>A>B) | — | Only with A & B Colors / Arbitrary Map. |
  | 9 | Color Loops (颜色循环数)（译名待核） | `ADBE Glo2-0009` | float slider | 1.0 | slider 1…10; valid 1…127 (UNVERIFIED) | loops | — |
  | 10 | Color Phase (色彩相位) | `ADBE Glo2-0010` | angle | 0° | any | degrees (1 rev = 1 loop) | — |
  | 11 | A & B Midpoint (A 和 B 中点) | `ADBE Glo2-0011` | float slider (percent) | 50 % (internal 0.5) | 0…100 % | % | — |
  | 12 | Color A (颜色 A) | `ADBE Glo2-0012` | color | white [1,1,1] | — | — | — |
  | 13 | Color B (颜色 B) | `ADBE Glo2-0013` | color | black [0,0,0] | — | — | — |
  | 14 | Glow Dimensions (发光维度) | `ADBE Glo2-0014` | popup | 1 = Horizontal and Vertical | Horizontal and Vertical / Horizontal / Vertical | — | — |

  ```
  t = threshold_internal / 255
  // 1. Threshold (bright-pass), straight colour
  if based_on == ColorChannels:
      src_g = per channel: max(c - t, 0) / (1 - t)       // UNVERIFIED: soft bright-pass; AE may use luma
  else:  // AlphaChannel
      src_g = alpha-based: g = smoothstep around alpha == t  // glow only at alpha edges (UNVERIFIED)
  // 2. Blur
  g = gaussian_blur(premultiply(src_g), radius, glow_dimensions)   // UNVERIFIED: AE uses a fast box approximation
  g *= intensity
  // 3. Colour
  if glow_colors != Original:
      v = luminance(g) (Color Channels) or alpha(g) (Alpha Channel), 0…1
      u = fract(v * loops + phase/360)
      switch looping:
        SawtoothAB:  s = u
        SawtoothBA:  s = 1 - u
        TriangleABA: s = 1 - |2u - 1|
        TriangleBAB: s = |2u - 1|
      s = bias(s, midpoint)                                // midpoint 0.5 = linear
      g.rgb = (A&B) ? lerp(colorA, colorB, s) * v : amp_lookup(s) * v   // UNVERIFIED
  // 4. Composite (Composite Original = where the ORIGINAL goes relative to the glow)
  switch composite_original:
      Behind: out = blend(glow_operation, top = g, bottom = src)   // glow over the image (default: Add)
      On Top: out = src OVER blend(glow_operation, top = g, bottom = src)  // original stays crisp, glow shows around it
      None:   out = (glow_operation == None) ? g : blend(glow_operation, g, transparent)
  ```
  - 8/16 bpc: glow clips at white; 32 bpc: glow values above 1.0 are kept (Adobe: glows look brighter and more realistic in 32 bpc).
  - Radius 0 or Intensity 0 → glow layer is empty and the result equals the original (for Behind/On Top).
  - Best vs Draft quality can differ (Adobe help: preview at Best before rendering).
- **Priority:** P0 — one of the most-used stylize effects in motion graphics.
- **Owner:** bae-effects (+ bae-render)
- **Notes:** UNVERIFIED: Adobe help describes "On Top" as placing the glow on top and "Behind" as backlighting, i.e. the reverse of the menu wording; the menu label refers to the original. BAE must confirm with a reference render (bright text on dark: default Behind + Add brightens the text interior). Recipes from Adobe help: glow-only = Composite Original None + Glow Operation None; knock-out glow = Glow Operation Silhouette Alpha. Lottie: no effect type (exported as generic `ty: 5`, not rendered by lottie-web). Implementation hint: bright-pass + downsampled blur pyramid (dual-Kawase style) + one composite pass; keep the bright-pass and composite in one shader each.

### AE-FX-320 · Find Edges (查找边缘)

- **What it does:** Replaces the image with its edges: dark lines on white, or bright lines on black when inverted; looks like a sketch.
- **Exact semantics:**
  - matchName `ADBE Find Edges`; category Stylize; 8 bpc; GPU since AE 14.1. Expands bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Invert (反转) | `ADBE Find Edges-0001` | checkbox | off | 0/1 | — | Off: dark edges on white; on: bright edges on black. |
  | 2 | Blend With Original (与原始图像混合) | `ADBE Find Edges-0002` | float slider | 0 % | 0…100 | % | 100 % = original unchanged. |

  ```
  for each colour channel c (straight colour):
      gx, gy = sobel_3x3(c)            // UNVERIFIED operator; clamp-to-edge addressing
      e = clamp(sqrt(gx^2 + gy^2) * k, 0, 1)   // k UNVERIFIED normalisation
      fx = invert ? e : 1 - e
  out = lerp(fx, original, blend/100); alpha = original alpha
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** 8 bpc only in AE (32-bpc projects convert down and up; see effects.md [fx-a], Effect bit-depth support). BAE decision: implement in float but quantisation-compatible.

### AE-FX-321 · Mosaic (马赛克)

- **What it does:** Pixelates the layer into a grid of solid-colour rectangles.
- **Exact semantics:**
  - matchName `ADBE Mosaic`; category Stylize; 16 bpc; GPU (per Adobe help: GPU accelerated; not listed in the match-name table GPU column, UNVERIFIED version). Expands bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Horizontal Blocks (水平块) | `ADBE Mosaic-0001` | integer slider | 10 | slider 1…200; valid 1…4000 (UNVERIFIED) | blocks per row | — |
  | 2 | Vertical Blocks (垂直块) | `ADBE Mosaic-0002` | integer slider | 10 | same | blocks per column | — |
  | 3 | Sharp Colors (锐化颜色) | `ADBE Mosaic-0003` | checkbox | off | 0/1 | — | On: colour of the block's centre pixel; off: block average. |

  ```
  bw = layer_w / H; bh = layer_h / V          // fractional block sizes allowed
  i = floor(x / bw); j = floor(y / bh)
  colour = sharp ? in(center of block i,j) : mean(premultiplied in over block i,j)   // includes alpha
  Best quality: block borders are antialiased (coverage-weighted at fractional boundaries)
  ```
- **Priority:** P1 — used for censoring and pixel transitions.
- **Owner:** bae-effects
- **Notes:** Blocks are anchored at the layer's top-left corner, not the centre. Implementation hint: box-downsample to H×V with exact area weights, then nearest-neighbour upsample.

### AE-FX-322 · Posterize (色调分离)

- **What it does:** Reduces each colour channel to a fixed number of tonal levels.
- **Exact semantics:**
  - matchName `ADBE Posterize`; category Stylize; 32 bpc; GPU —. Expands bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Level (级别) | `ADBE Posterize-0001` | integer slider | 7 | 2…255 (Adobe help) | levels per channel | — |

  ```
  // per R,G,B on straight colour; alpha unchanged (UNVERIFIED)
  L = level
  v' = floor(v * L) / (L - 1)  clamped to [0,1]     // UNVERIFIED exact rounding (Photoshop-style)
  32 bpc: values outside 0…1 are clamped before quantising (UNVERIFIED)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Not to be confused with Posterize Time (see effects.md [fx-c], Time effects).

### AE-FX-323 · Threshold (阈值)

- **What it does:** Converts the image to pure black and white: pixels lighter than the level become white, darker become black.
- **Exact semantics:**
  - matchName `ADBE Threshold2`; category Stylize; 32 bpc; GPU —. Expands bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Level (级别) | `ADBE Threshold2-0001` | integer slider | 128 (UNVERIFIED; the obsolete `ADBE Threshold` reports 127.5) | 0…255 | 8-bit level | — |

  ```
  y = luma(straight rgb)            // UNVERIFIED weights; Rec.601 0.299/0.587/0.114 assumed
  rgb' = (y * 255 >= level) ? 1 : 0  // boundary inclusive side UNVERIFIED
  alpha unchanged
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** The obsolete matchName `ADBE Threshold` (8/16 bpc, Level default 127.5) loads in old projects; BAE maps it to the same implementation.

### AE-FX-324 · Roughen Edges (毛边)

- **What it does:** Roughens the alpha edge with fractal noise and optionally colours the edge band (rust, photocopy looks).
- **Exact semantics:**
  - matchName `ADBE Roughen Edges`; category Stylize; 8 bpc; GPU —. Expands bounds: no (works inward from the alpha edge).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Edge Type (边缘类型) | `ADBE Roughen Edges-0001` | popup | 1 = Roughen | 1 Roughen (粗糙化), 2 Roughen Color (粗糙化颜色), 3 Cut (剪切), 4 Spiky (尖刺), 5 Rusty (生锈), 6 Rusty Color (生锈颜色), 7 Photocopy (影印), 8 Photocopy Color (影印颜色)（译名待核） | — | — |
  | 2 | Edge Color (边缘颜色) | `ADBE Roughen Edges-0010` | color | [0.6, 0.2, 0] (rust brown) | — | — | Used by Roughen Color, Rusty Color (edge) and Photocopy Color (fill). |
  | 3 | Border (边框) | `ADBE Roughen Edges-0002` | float slider | 8.0 | slider 0…500 (UNVERIFIED) | pixels | Inward distance from the alpha edge. |
  | 4 | Edge Sharpness (边缘锐度) | `ADBE Roughen Edges-0003` | float slider | 1.0 | 0…10 (UNVERIFIED) | — | — |
  | 5 | Fractal Influence (分形影响) | `ADBE Roughen Edges-0004` | float slider | 1.0 | 0…1 | — | — |
  | 6 | Scale (缩放) | `ADBE Roughen Edges-0005` | float slider | 100 % | slider 10…400; valid 10…2000 (UNVERIFIED) | % | — |
  | 7 | Stretch Width or Height (伸缩宽度或高度) | `ADBE Roughen Edges-0006` | float slider | 0.0 | −10…10 (UNVERIFIED) | — | Positive stretches the fractal horizontally (UNVERIFIED sign). |
  | 8 | Offset (Turbulence) (偏移（湍流）) | `ADBE Roughen Edges-0007` | point | [0, 0] | any | layer px | — |
  | 9 | Complexity (复杂度) | `ADBE Roughen Edges-0008` | integer slider | 2 | 1…10 | octaves | — |
  | 10 | Evolution (演化) | `ADBE Roughen Edges-0009` | angle | 0° | any | revolutions + degrees | — |
  | 11 | Evolution Options (演化选项) | `ADBE Roughen Edges-0011` | group | — | — | — | Contains 12–14. |
  | 12 | Cycle Evolution (循环演化) | `ADBE Roughen Edges-0012` | checkbox | off | — | — | — |
  | 13 | Cycle (in Revolutions) (循环（旋转次数）) | `ADBE Roughen Edges-0013` | integer slider | 1 | 1…1000 (UNVERIFIED) | revolutions | — |
  | 14 | Random Seed (随机植入) | `ADBE Roughen Edges-0014` | integer slider | 0 | 0…10000 (UNVERIFIED) | — | `-0015` is the group end marker. |

  ```
  d = distance inside the alpha edge (0 at edge, grows inward), from alpha (e.g. via blur/distance field)
  n = fractal_noise(p / (scale/100) with stretch, offset, octaves = complexity,
                    evolution (cycled if Cycle Evolution), seed)   // same family as Fractal Noise, see effects.md [fx-c]
  rough = clamp((d / border) - fractal_influence * n, ...)  // UNVERIFIED shape
  alpha' = alpha * sharpen(rough, edge_sharpness)
  Roughen Color / Rusty Color: band near the edge tinted with Edge Color
  Cut: binary cut; Spiky: sharp spikes; Photocopy: outline-ish; exact per-type math UNVERIFIED
  ```
- **Priority:** P1 — popular for hand-made/grunge text looks.
- **Owner:** bae-effects
- **Notes:** Evolution semantics (revolutions progress forever; Cycle Evolution loops) are shared with Turbulent Displace (AE-FX-355) and Fractal Noise (effects.md [fx-c]). Implementation hint: share one WGSL fractal-noise module across Roughen Edges, Turbulent Displace and Fractal Noise.

### AE-FX-325 · Motion Tile (动态拼贴)

- **What it does:** Repeats the layer image as tiles around a main tile, with optional mirrored tiles and row/column phase offset; can enlarge the output.
- **Exact semantics:**
  - matchName `ADBE Tile`; category Stylize; 8 bpc; GPU —. Expands bounds: yes when Output Width/Height > 100 %.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Tile Center (拼贴中心) | `ADBE Tile-0001` | point | layer centre | any | layer px | Centre of the main tile. |
  | 2 | Tile Width (拼贴宽度) | `ADBE Tile-0002` | float slider | 100.0 | slider 1…100; valid 1…3000 (UNVERIFIED) | % of layer width | — |
  | 3 | Tile Height (拼贴高度) | `ADBE Tile-0003` | float slider | 100.0 | same | % of layer height | — |
  | 4 | Output Width (输出宽度) | `ADBE Tile-0004` | float slider | 100.0 | slider 0…1000; valid 0…3000 (UNVERIFIED) | % of layer width | Output rect centred on the layer. |
  | 5 | Output Height (输出高度) | `ADBE Tile-0005` | float slider | 100.0 | same | % of layer height | — |
  | 6 | Mirror Edges (镜像边缘) | `ADBE Tile-0006` | checkbox | off | — | — | Adjacent tiles are flipped. |
  | 7 | Phase (相位) | `ADBE Tile-0007` | angle | 0° | any | degrees (360° = 1 tile) | Offsets alternate columns vertically (or rows horizontally). |
  | 8 | Horizontal Phase Shift (水平位移)（译名待核） | `ADBE Tile-0008` | checkbox | off | — | — | On: phase shifts rows horizontally. |

  ```
  tw = W * tile_w/100; th = H * tile_h/100
  out_rect = centred rect of size (W*out_w/100, H*out_h/100) around layer centre
  for output pixel p:
      q = p - tile_center
      col = floor((q.x + tw/2) / tw); row = floor((q.y + th/2) / th)
      if !h_phase: q.y += (col odd ? phase/360 * th : 0)      // UNVERIFIED: odd columns only
      else:        q.x += (row odd ? phase/360 * tw : 0)
      u = (q.x + tw/2) mod tw; v = (q.y + th/2) mod th          // recompute col/row after shift
      if mirror and col odd: u = tw - u;  if mirror and row odd: v = th - v
      src = (u/tw * W, v/th * H)    // tile = whole layer scaled to tile size
      out(p) = sample(in, src)       // Best quality: bilinear
  ```
- **Priority:** P1 — standard for kaleidoscopic/background patterns.
- **Owner:** bae-effects
- **Notes:** Adobe help: when tile placement changes, layer motion blur (if enabled) applies to the movement (UNVERIFIED mechanism: the effect is sampled at motion-blur sub-frame times like any effect). Commonly paired with Offset (AE-FX-362). BAE decision: process in float at any project depth.

### AE-FX-326 · Brush Strokes (画笔描边)

- **What it does:** Paints the image with randomised brush strokes at a given angle.
- **Exact semantics:**
  - matchName `ADBE Brush Strokes`; 8 bpc; GPU —; clips to layer bounds (Adobe help suggests Grow Bounds first).
  - Parameters (AE order; UNVERIFIED defaults): Stroke Angle (描边角度) angle 135°; Brush Size (画笔大小) 2 px (1…5); Stroke Length (描边长度) 12 px; Stroke Density (描边浓度) 1.0; Stroke Randomness (描边随机性) 1.0; Paint Surface (绘画表面) popup `ADBE Brush Strokes-0006`: 1 Paint On Original Image (在原始图像上绘画), 2 Paint On Transparent (在透明背景上绘画), 3 Paint On White (在白色上绘画), 4 Paint On Black (在黑色上绘画), default 1; Blend With Original (与原始图像混合) 0 %.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Chinese labels per zh-CN help (笔刷描边 heading in help; AE UI name 画笔描边（译名待核）).

### AE-FX-327 · Cartoon (卡通)

- **What it does:** Smooths the image (bilateral-type blur), posterises its shading and draws edge strokes for a cel-shaded look.
- **Exact semantics:**
  - matchName `ADBE Cartoonify`; 32 bpc; GPU —.
  - Parameters (AE order): Render (渲染) popup `ADBE Cartoonify-0001`: 1 Fill (填充), 2 Edges (边缘), 3 Fill & Edges (填充及边缘), default 3 (UNVERIFIED); Detail Radius (细节半径); Detail Threshold (细节阈值); Fill group: Shading Steps (阴影步骤), Shading Smoothness (阴影平滑度); Edge group: Threshold (阈值), Width (宽度), Softness (柔和度), Opacity (不透明度); Advanced group: Edge Enhancement (边缘增强), Edge Black Level (边缘黑色阶), Edge Contrast (边缘对比度), Performance. Defaults UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Added AE CS6 (11.0).

### AE-FX-328 · Color Emboss (彩色浮雕)

- **What it does:** Emboss that keeps the original colours.
- **Exact semantics:**
  - matchName `ADBE Color Emboss`; 16 bpc; GPU —.
  - Parameters (same as Emboss): Direction (方向) angle 45°; Relief (起伏) 1.0 px; Contrast (对比度) 100; Blend With Original (与原始图像混合) 0 % (defaults UNVERIFIED, mirrored from Emboss).
  - Algorithm: directional derivative along Direction, scaled by Contrast, added to the original colour.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-329 · Emboss (浮雕)

- **What it does:** Grey relief image from directional edges, lit from a direction.
- **Exact semantics:**
  - matchName `ADBE Emboss`; 16 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Direction (方向) | `ADBE Emboss-0001` | angle | 45° | any | degrees | Light direction. |
  | 2 | Relief (起伏) | `ADBE Emboss-0002` | float slider | 1.0 | 1…10 (UNVERIFIED) | pixels | Max highlight edge width. |
  | 3 | Contrast (对比度) | `ADBE Emboss-0003` | integer slider | 100 | 0…500 (UNVERIFIED) | % | — |
  | 4 | Blend With Original (与原始图像混合) | `ADBE Emboss-0004` | float slider | 0 % | 0…100 | % | — |

  ```
  d = (cos θ, -sin θ) * relief
  e = luma(in(p + d)) - luma(in(p - d))     // UNVERIFIED sampling
  out.rgb = 0.5 + e * contrast/100; alpha unchanged; then lerp toward original by blend
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-330 · Scatter (散布)

- **What it does:** Randomly displaces pixels within a radius, giving a grainy smear.
- **Exact semantics:**
  - matchName `ADBE Scatter`; 16 bpc; GPU —.
  - Parameters: 1 Scatter Amount (散布数量) float default 20 (UNVERIFIED), 0…127 px; 2 Grain (颗粒) popup `ADBE Scatter-0002`: 1 Both (两者), 2 Horizontal (水平), 3 Vertical (垂直); 3 Scatter Randomness: Randomize Every Frame (散布随机性：随机分布每个帧) checkbox default off.
  - Algorithm: `out(p) = in(p + rand2(p, seed) * amount)` with the random restricted to the Grain axis; seed changes each frame when Randomize Every Frame is on.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Adobe help describes the Grain menu as "None" for all directions; AE 26 menu label is "Both".

### AE-FX-331 · Strobe Light (闪光灯)

- **What it does:** At periodic or random intervals applies a colour operation to the layer or makes it transparent.
- **Exact semantics:**
  - matchName `ADBE Strobe`; 8 bpc; GPU —.
  - Parameters (AE order): Strobe Color (闪光颜色) white; Blend With Original (与原始图像混合) 0 %; Strobe Duration (secs) (闪光持续时间（秒）) 0.1 (UNVERIFIED); Strobe Period (secs) (闪光周期（秒）) 1.0 (UNVERIFIED); Random Strobe Probability (随机闪光概率) 0 %; Strobe (闪光) popup `ADBE Strobe-0006`: 1 Operates On Color Only (仅对颜色操作), 2 Makes Layer Transparent (使图层透明); Strobe Operator (闪光运算符) popup `ADBE Strobe-0007`: Copy, Add, Subtract, Multiply, Difference, And, Or, Xor, Lighter, Darker, Minimum, Maximum, Screen (default Copy); Random Seed (随机植入) (UNVERIFIED).
  - Timing: strobe active when `(t − layer_in) mod period < duration`, or on frames chosen with probability p (UNVERIFIED reference time).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-332 · Texturize (纹理化)

- **What it does:** Uses another layer's luminance as a bump texture lit from a direction.
- **Exact semantics:**
  - matchName `ADBE Texturize`; 8 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Texture Layer (纹理图层) | `ADBE Texturize-0001` | layer | None | — | — | — |
  | 2 | Light Direction (光照方向) | `ADBE Texturize-0002` | angle | 135° | any | degrees | — |
  | 3 | Texture Contrast (纹理对比度) | `ADBE Texturize-0003` | float slider | 1.0 | 0…2 (UNVERIFIED) | — | — |
  | 4 | Texture Placement (纹理位置) | `ADBE Texturize-0004` | popup | 1 = Tile Texture | 1 Tile Texture (拼贴纹理), 2 Center Texture (居中纹理), 3 Stretch Texture to Fit (拉伸纹理以适合) | — | — |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Best quality positions the texture with subpixel precision (Adobe help).

### AE-FX-333 · Contour Paint (轮廓绘制)

- **What it does:** Detects contour flow in each frame, smears pixels along it and optionally overlays posterised flat colour fills for a painted look.
- **Exact semantics:**
  - matchName UNVERIFIED (effect is newer than the AE 26.0 match-name dump); 8/16/32 bpc (Adobe help); GPU UNVERIFIED.
  - Main controls (Adobe help): edge source (Get Edges From Layer (从图层获取边缘), Edge Layer Behavior: Center Map / Stretch Map to Fit / Tile Map), Offset Flow Angles, Quantize Flow Angles (0/15/30/45/60/90° or custom), Edge Strength Adjustment, Freehand Flow Adjustment (brush tools), Stylization (风格化), Cleanliness (整洁度), Smear Iterations, Apply to Alpha, Draw Color Fills (None / On Top of Smeared Image / Only), Reduction Method (Posterize K-Means, Posterize Median, Match Closest Custom Color), Posterize Levels, Custom Color Set, Fill Rendering (Feather, Opacity), Limit Fills by Edge Strength (Min/Max Strength).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Added in a 2025/2026 AE release (UNVERIFIED version).

### AE-FX-334 · CC Block Load (CC 块加载)

- **What it does:** Simulates progressive (interlaced-JPEG-style) image loading in blocks.
- **Exact semantics:** matchName `CS BlockLoad`; 32 bpc; GPU —. Params (UNVERIFIED): Completion, Scanlining, Start Cleared, Bilinear.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** CycoreFX. Names（译名待核）.

### AE-FX-335 · CC Burn Film (CC 胶片灼烧)

- **What it does:** Animated burning-film holes eating through the layer.
- **Exact semantics:** matchName `CC Burn Film`; 32 bpc; GPU —. Params (UNVERIFIED): Burn, Center, Random Seed.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Names（译名待核）.

### AE-FX-336 · CC Glass (CC 玻璃)

- **What it does:** Bump-mapped glass distortion with lighting and shading from a property map.
- **Exact semantics:** matchName `CC Glass`; 16 bpc; GPU —. Surface group: Bump Map (layer), Property popup `CC Glass-0003` (Red, Green, Blue, Alpha, Luminance, Lightness), Height, Displacement; Light group: Using popup `CC Glass-0023` (Effect Light, AE Lights), Light Type popup `CC Glass-0011` (Distant Light, Point Light), Light Intensity, Light Color, Light Height, Light Position/Direction; Shading group: Ambient, Diffuse, Specular, Roughness, Metal (UNVERIFIED order/defaults).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** "AE Lights" reads comp light layers (see compositing-3d-color.md, Lights).

### AE-FX-337 · CC HexTile (CC 六边形拼贴)

- **What it does:** Tiles the image in a hexagonal grid.
- **Exact semantics:** matchName `CS HexTile`; 32 bpc; GPU —. Params (UNVERIFIED): Render (popup), Radius, Center, Rotate, Smearing.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Names（译名待核）.

### AE-FX-338 · CC Kaleida (CC 万花筒)

- **What it does:** Kaleidoscope mirroring around a centre.
- **Exact semantics:** matchName `CC Kaleida`; 32 bpc; GPU —. Params: Center, Size, Mirroring popup `CC Kaleida-0003` (Unfold, Wheel, Fish Head, Can Meas, Flip Flop, Flower, Dia Cross, Flipper, Starlish), Rotation, Floating Center (UNVERIFIED order/defaults).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-339 · CC Mr. Smoothie (CC 平滑先生)

- **What it does:** Smears colours along a flow derived from a property, producing liquid colour patterns.
- **Exact semantics:** matchName `CC Mr. Smoothie`; 16 bpc; GPU —. Params: Flow Layer, Property popup `CC Mr. Smoothie-0002` (Red, Green, Blue, Alpha, Luminance, Lightness, Hue, Saturation), Smoothness, Sample A, Sample B, Phase, Color Loop popup `CC Mr. Smoothie-0007` (AB, BA, ABA, BAB) (UNVERIFIED order).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Names（译名待核）.

### AE-FX-340 · CC Plastic (CC 塑料)

- **What it does:** Bump-mapped plastic/metal shading from a property map with lighting.
- **Exact semantics:** matchName `CC Plastic`; 16 bpc; GPU —. Same structure as CC Glass (Surface Bump with Property popup `CC Plastic-0003`, Light Using `-0026` Effect Light / AE Lights, Light Type `-0012` Distant / Point, Shading group) (UNVERIFIED details).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-341 · CC RepeTile (CC 重复平铺)

- **What it does:** Expands the layer by repeating it outward with tiling patterns.
- **Exact semantics:** matchName `CC RepeTile`; 32 bpc; GPU —; expands bounds: yes. Params: Expand Right, Expand Left, Expand Down, Expand Up (px), Tiling popup `CC RepeTile-0005` (Repeat, Checker Flip H, Checker Flip V, Unfold, Checker 180 deg, Checker Flip 45 deg, Checker 90 deg CCW, Checker 90 deg CW, Rosette, Random, None, (separator), Turn CW, Turn CCW, Twist, Slide, Brick), Blend Borders (UNVERIFIED order/defaults).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Frequently used instead of Motion Tile for seamless backgrounds; consider P1 if usage data shows it.

### AE-FX-342 · CC Threshold (CC 阈值)

- **What it does:** Threshold with selectable channel and blend.
- **Exact semantics:** matchName `CC Threshold`; 32 bpc; GPU —. Params: Threshold (0…255), Channel popup `CC Threshold-0002` (Luminance, RGB, Saturation, Alpha), Invert, Blend w. Original (UNVERIFIED order/defaults).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-343 · CC Threshold RGB (CC 阈值 RGB)

- **What it does:** Independent thresholds per R, G, B channel.
- **Exact semantics:** matchName `CC Threshold RGB`; 32 bpc; GPU —. Params (UNVERIFIED): Red Threshold, Green Threshold, Blue Threshold, Invert Red/Green/Blue Channel, Blend w. Original.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-344 · CC Vignette (CC 暗角)

- **What it does:** Darkens the layer towards its edges.
- **Exact semantics:** matchName `CS Vignette`; 32 bpc; GPU —. Params (UNVERIFIED): Amount, Angle of View, Center, Pin Highlights.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Names（译名待核）.

## Perspective effects

Effect-category name: Perspective (透视). Conventions as stated at the start of the Blur & Sharpen section. Layer styles (Drop Shadow / Bevel and Emboss layer styles) are a separate feature: see project-comp-layers.md, Layer styles.

### AE-FX-345 · Drop Shadow (投影)

- **What it does:** Draws a soft, offset, single-colour copy of the layer's alpha behind the layer; can output the shadow alone.
- **Exact semantics:**
  - matchName `ADBE Drop Shadow`; category Perspective; 32 bpc; GPU since AE 14.2. Expands output bounds: yes (by Distance + blur extent); the shadow can extend past the layer rect.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Shadow Color (阴影颜色) | `ADBE Drop Shadow-0001` | color | black [0,0,0] | — | — | — |
  | 2 | Opacity (不透明度) | `ADBE Drop Shadow-0002` | float slider (percent display) | 50 % (internal 127.5 of 255) | 0…100 % | % | Scripting `value` and Lottie return the 0…255 internal number (127.5). |
  | 3 | Direction (方向) | `ADBE Drop Shadow-0003` | angle | 135° | any | degrees | 0° = shadow straight up, 90° = right, 135° = down-right (clockwise from up). |
  | 4 | Distance (距离) | `ADBE Drop Shadow-0004` | float slider | 5.0 | slider 0…120; valid 0…32000 (UNVERIFIED) | pixels | Downsample-scaled. |
  | 5 | Softness (柔和度) | `ADBE Drop Shadow-0005` | float slider | 0.0 | slider 0…250; valid 0…1000 (UNVERIFIED) | pixels | — |
  | 6 | Shadow Only (仅阴影) | `ADBE Drop Shadow-0006` | checkbox | off | 0/1 | — | — |

  ```
  rad = radians(direction)
  off = (distance * sin(rad), -distance * cos(rad))      // layer space, y down; 135° → (+0.707d, +0.707d)
  a_s = gaussian_blur(alpha(in), sigma = softness * k)    // k UNVERIFIED: Skottie 0.3, lottie-web 0.25
  a_s = shift(a_s, off)                                   // subpixel (bilinear) at Best quality, integer at Draft
  shadow = premul(shadow_color, a_s * opacity/255)
  out = shadow_only ? shadow : in OVER shadow             // original composited on top
  out_rect = in_rect ∪ (in_rect + off).expand(blur extent)
  ```
  - The shadow is computed in layer space before the layer transform, so it rotates with the layer (Adobe: rotate with the Transform effect first, or precompose/adjustment layer, to keep a fixed light direction).
  - Semi-transparent pixels cast proportionally lighter shadow; the colour of the layer does not tint the shadow.
- **Priority:** P0 — ubiquitous on text and graphics.
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Lottie: `ty: 25` Drop Shadow effect, values in order color, opacity (0…255), angle (direction), distance, blur (softness) — lottie-web uses offset angle `(direction − 90)°`, `flood-opacity = opacity/255`, `stdDeviation = softness/4`; Skottie uses `sigma = softness·0.3`. Shadow Only is not part of the Lottie schema (UNVERIFIED whether bodymovin exports it as a 6th value). Layer quality affects subpixel positioning and the smoothness of the soft edge (Adobe help). Implementation hint: blur only the alpha channel (one R16F/R32F texture), then one composite pass.

### AE-FX-346 · Bevel Alpha (斜面 Alpha)

- **What it does:** Gives the alpha boundary a lit, chiselled bevel so flat graphics look 3D; softer than Bevel Edges. A fully opaque layer is bevelled along its bounding box.
- **Exact semantics:**
  - matchName `ADBE Bevel Alpha`; category Perspective; 16 bpc (Adobe help: 8 and 16 bpc); GPU —. Expands bounds: no.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Edge Thickness (边缘厚度) | `ADBE Bevel Alpha-0001` | float slider | 2.00 (UNVERIFIED) | slider 0…10; valid 0…200 (UNVERIFIED) | pixels | — |
  | 2 | Light Angle (灯光角度) | `ADBE Bevel Alpha-0002` | angle | −60° (UNVERIFIED) | any | degrees | Direction the light comes from (0° = up). |
  | 3 | Light Color (灯光颜色) | `ADBE Bevel Alpha-0003` | color | white (UNVERIFIED) | — | — | — |
  | 4 | Light Intensity (灯光强度) | `ADBE Bevel Alpha-0004` | float slider | 0.40 (UNVERIFIED) | 0…1 | — | — |

  ```
  h = gaussian_blur(alpha, sigma ≈ edge_thickness / 2)     // height field (UNVERIFIED kernel)
  n = normalize(-dh/dx, -dh/dy, 1/edge_thickness)
  L = direction from light_angle (in-plane) with fixed elevation (UNVERIFIED, ~45°)
  s = dot(n, L) - dot(n_flat, L)                             // signed shading: >0 highlight, <0 shadow
  rgb' = rgb + intensity * (s > 0 ? s * light_color : s)     // highlight tinted by light colour, shadow darkens
  only where 0 < h < 1 (edge band); alpha unchanged
  ```
- **Priority:** P1 — common on titles.
- **Owner:** bae-effects
- **Notes:** Adobe recommends the Bevel and Emboss layer style when highlights and shadows need different blend modes. Implementation hint: compute the height field once per frame with a separable blur of alpha, then a single shading pass.

### AE-FX-347 · 3D Camera Tracker (3D 摄像机跟踪器)

- **What it does:** Analyses 2D footage, solves a 3D camera and track points, and creates a camera layer and nulls/solids/text placed on tracked planes.
- **Exact semantics:**
  - matchName `ADBE 3D Tracker`; 32 bpc; GPU —; temporal analysis effect (results stored in the effect's arbitrary data).
  - Controls (Adobe help): Analyze / Cancel buttons, Shot Type (拍摄类型): Fixed Angle of View (固定视角), Variable Zoom (变量缩放), Specify Angle of View; Horizontal Angle of View; Show Track Points (显示轨迹点): 2D Source / 3D Solved; Render Track Points (渲染跟踪点); Track Point Size (跟踪点大小); Target Size (目标大小); Create Camera (创建摄像机); Advanced group (Solve Method, Method Used, Average Error, Detailed Analysis, Auto-delete Points Across Time, Hide Warning Banner).
- **Priority:** P2 — computer-vision feature, out of scope for M1.
- **Owner:** bae-effects (+ bae-motion)
- **Notes:** Creating the camera/null layers is a bae-motion command. See compositing-3d-color.md, Cameras.

### AE-FX-348 · 3D Glasses (3D 眼镜)

- **What it does:** Combines left/right view layers into a stereoscopic image (anaglyph, side-by-side, over-under, interlaced, difference).
- **Exact semantics:**
  - matchName `ADBE 3D Glasses2` (obsolete predecessor `ADBE 3D Glasses`); 32 bpc; GPU —.
  - Parameters (AE order, UNVERIFIED defaults): Left View (左视图) layer; Right View (右视图) layer; Scene Convergence (场景融合) px; Vertical Alignment (垂直对齐) px; Units (单位) popup `ADBE 3D Glasses2-0008`: 1 Pixels, 2 % of Source; Swap Left-Right (左右互换) checkbox; 3D View (3D 视图) popup `ADBE 3D Glasses2-0005`: Stereo Pair (Side by Side), Over Under, Interlace Upper L Lower R, Difference, (separator), Red Green LR, Red Blue LR, (separator), Balanced Red Green LR, Balanced Red Blue LR, (separator), Balanced Colored Red Blue; Balance (平衡) float.
  - The obsolete `ADBE 3D Glasses` uses a different 3D View menu order (Stereo Pair, Interlace, Red Green, Red Blue, Balanced…, Balanced Colored Red Blue, Over Under, Difference); BAE must remap stored indices by matchName.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Stereo rigs: see compositing-3d-color.md, Cameras.

### AE-FX-349 · Bevel Edges (边缘斜面)

- **What it does:** Rectangular lit bevel along the layer's edges; always rectangular regardless of alpha shape.
- **Exact semantics:**
  - matchName `ADBE Bevel Edges`; 8 bpc; GPU —.
  - Parameters (UNVERIFIED defaults): Edge Thickness (边缘厚度) 0.10 (fraction of the shorter layer side, 0…0.5); Light Angle (灯光角度) −60°; Light Color (灯光颜色) white; Light Intensity (灯光强度) 0.40 (0…1).
  - Algorithm: four trapezoidal border bands of equal thickness; each band shaded by `intensity * dot(band_normal, light_dir)`; Best quality interpolates the thickness for smooth edges.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-350 · Radial Shadow (径向阴影)

- **What it does:** Shadow cast from a point light onto a surface behind the layer; shadow size grows with projection distance; Glass Edge lets layer colour tint the shadow.
- **Exact semantics:**
  - matchName `ADBE Radial Shadow`; 8 bpc; GPU —; expands bounds: only with Resize Layer.
  - Parameters (AE order; UNVERIFIED defaults): Shadow Color (阴影颜色) black; Opacity (不透明度) 50 %; Light Source (光源) point (layer centre-top); Projection Distance (投影距离) 10; Softness (柔和度) 0; Render (渲染) popup `ADBE Radial Shadow-0006`: 1 Regular (常规), 2 Glass Edge (玻璃边缘); Color Influence (颜色影响) 100 %; Shadow Only (仅阴影) off; Resize Layer (调整图层大小) off.
  - Algorithm: shadow = alpha scaled about the light source by `1 + projection_distance / k` (UNVERIFIED k), blurred by Softness, coloured; Glass Edge uses layer colour weighted by Color Influence through semi-transparent areas.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-351 · CC Cylinder (CC 圆柱体)

- **What it does:** Wraps the layer onto a lit 3D cylinder.
- **Exact semantics:** matchName `CC Cylinder`; 16 bpc; GPU —. Params (UNVERIFIED order): Radius, Position (X/Y/Z), Rotation X/Y/Z, Rotation Order popup `CC Cylinder-0027` (XYZ, XZY, YXZ, YZX, ZXY, ZYX), Render popup `CC Cylinder-0013` (Full, Outside, Inside), Light group (Intensity, Color, Height, Direction), Shading group (Ambient, Diffuse, Specular, Roughness, Metal).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-352 · CC Environment (CC 环境)

- **What it does:** Maps an environment image onto the layer as a reflection, driven by the active camera.
- **Exact semantics:** matchName `CC Environment`; 32 bpc; GPU —. Params: Environment (layer), Mapping popup `CC Environment-0004` (Spherical, Probe, Vertical Cross), Horizontal Offset, Filter Environment (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-353 · CC Sphere (CC 球体)

- **What it does:** Wraps the layer onto a lit 3D sphere.
- **Exact semantics:** matchName `CC Sphere`; 32 bpc; GPU —. Params (UNVERIFIED order): Rotation X/Y/Z, Rotation Order popup `CC Sphere-0026` (XYZ…ZYX), Radius, Offset, Render popup `CC Sphere-0008` (Full, Outside, Inside), Light group, Shading group (Ambient, Diffuse, Specular, Roughness, Metal, Reflective, Global Illumination, Transparency).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Popular for planet animations; candidate for P1 later.

### AE-FX-354 · CC Spotlight (CC 聚光灯)

- **What it does:** Simulated spotlight (optionally projecting a gel layer) over the layer.
- **Exact semantics:** matchName `CC Spotlight`; 16 bpc; GPU —. Params (UNVERIFIED order): From, To, Height, Cone Angle, Edge Softness, Color, Intensity, Render popup `CC Spotlight-0008` (Light Only, Light Add, Light Add+, Light Shadow, (separator), Gel Only, Gel Add, Gel Add+, Gel Shadow), Gel Layer.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —
