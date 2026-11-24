# After Effects inventory: compositing, 3D and color (AE-CMP)

**Scope.** This file specifies how After Effects turns evaluated layers into pixels: the pixel and alpha model and bit-depth clamping, per-layer and per-comp render order, every blend mode with exact formulas, opacity and Preserve Underlying Transparency, track mattes, masks, adjustment layers, Collapse Transformations / Continuously Rasterize, nested-comp resolution and frame rate, quality / sampling / subpixel positioning, layer bounds, motion-blur and frame-blend rendering, the Classic 3D renderer (depth sort, intersections, 2D/3D grouping), cameras, lights, material options, color management, and preview resolution / region of interest / fast previews / transparency grid / channel views. Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md, ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

**Conventions used in this file.** Colour values are written normalised: 0.0 = black / transparent, 1.0 = white / opaque, whatever the project bit depth. `Cb`, `ab` = backdrop (composite of the layers below) colour and alpha; `Cs`, `as` = source (the layer being composited) colour and alpha; colours are *straight* (unmultiplied) unless the name ends in `p` (`cbp = Cb*ab`, `csp = Cs*as`). `Lum(C) = 0.30*R + 0.59*G + 0.11*B` unless stated otherwise. `clamp01(x) = min(max(x, 0), 1)`.

## Pixel and alpha model

### AE-CMP-001 · Internal pixel format and straight alpha (像素格式和直接 Alpha（译名待核）)

- **What it does:** Defines how AE stores the pixels of every layer buffer, effect buffer and comp buffer: four channels A, R, G, B with colour stored *straight* (not multiplied by alpha).
- **Exact semantics:**
  - Channel order in AE memory is ARGB (alpha first). 8 bpc: `PF_Pixel8 {u8 alpha, red, green, blue}`; 16 bpc: `PF_Pixel16 {u16 alpha, red, green, blue}`; 32 bpc: `PF_PixelFloat {f32 alpha, red, green, blue}` (plug-in SDK, Color space conversion / PF_EffectWorld).
  - Colour in layer and effect buffers is straight (unmultiplied): a pixel with alpha 0 may carry arbitrary RGB, and that RGB is preserved through the pipeline when an effect asks for it (SDK: SmartFX `preserve_rgb_of_zero_alpha`, `PF_OutFlag2_REVEALS_ZERO_ALPHA` — effects such as Set Channels can make zero-alpha pixels visible again, which is only possible with straight storage).
  - Footage with a premultiplied alpha is unmultiplied on import according to Interpret Footage (Premultiplied with colour X): `C = (Cp - (1 - a) * X) / a` for `a > 0`; see project-comp-layers.md, Interpret Footage. Output modules re-premultiply on export when Channels = RGB+Alpha and Color = Premultiplied (with black or the comp background colour); see ui-preview-render-io.md, Render Queue.
  - All compositing formulas in this file are stated on straight colour; the generic compositing equation (AE-CMP-012) gives the premultiplied form used for implementation.
  - Edge case: unmultiplying a pixel with `a == 0` yields RGB = 0 unless the RGB was preserved; implementations must never divide by zero.
- **Priority:** P0 — every compositing formula depends on it.
- **Owner:** bae-render (+ bae-color, bae-effects)
- **Notes:** UNVERIFIED: the AE plug-in SDK guide does not state in one sentence that "AE pixels are straight"; the straight-alpha conclusion is derived from `preserve_rgb_of_zero_alpha`, `PF_OutFlag2_REVEALS_ZERO_ALPHA`, `AEGP_CopyBits_HQ_Straight` vs `_Premul`, and long-standing third-party knowledge (AE internally works unmultiplied). Implementation hint: BAE should store GPU working buffers premultiplied (better filtering, cheaper Normal blend, correct bilinear sampling) and unmultiply only inside shaders that need straight colour (non-Normal blend modes, effects declared as straight-input, `sampleImage()`); the visible result must equal AE's straight pipeline. BAE decision: zero-alpha RGB preservation is needed only for the few effects that reveal zero alpha (Set Channels, Channel Combiner, Shift Channels, Remove Color Matting inputs); such effects receive a side "straight RGB" buffer only when flagged, not by default.

### AE-CMP-002 · 8 bits per channel pixels (8 bpc（每通道 8 位）)

- **What it does:** In an 8-bpc project every buffer stores integer channel values 0-255 and every operation result is clamped and rounded to that range.
- **Exact semantics:**
  - Code value `v8 ∈ {0..255}`; normalised `x = v8 / 255`. White = 255.
  - After every pixel-producing step (footage decode, effect output, mask application, layer transform resample, blend with the layers below, comp output) the result is clamped to [0, 255] and rounded to integer. Overbright (>1.0) and negative values cannot exist.
  - Rounding: UNVERIFIED: round-half-up (`floor(x*255 + 0.5)`) for stored results; AE's internal fixed-point maths (e.g. 8-bit multiply `(a*b + 128) / 255` style) can differ by ±1 code value from float maths.
  - 8-bpc effects in a 16- or 32-bpc project: the effect's input is converted down to 8 bpc (clamped and quantised) and its output converted back; the effect icon in Effect Controls shows a warning in higher-depth projects. See effects.md, effect framework.
  - Conversion to 8-bpc output when the project is 16 or 32 bpc applies dithering at render/export time, not in previews (Adobe help, Color basics).
- **Priority:** P0 — default project depth.
- **Owner:** bae-render (+ bae-color)
- **Notes:** Project bit depth itself (Project Settings > Color > Bit Depth, Alt/Option-click the bpc button in the Project panel) is specified in compositing-3d-color.md, Color management. Scripting: `app.project.bitsPerChannel` (8, 16, 32). Lottie has no bit-depth concept (players render 8 bpc).

### AE-CMP-003 · 16 bits per channel pixels (16 bpc（每通道 16 位）)

- **What it does:** In a 16-bpc project buffers store integer channel values 0-32768 ("15+1 bit") with the same clamping points as 8 bpc but much finer quantisation.
- **Exact semantics:**
  - Code value `v16 ∈ {0..32768}`; normalised `x = v16 / 32768`; white = 32768 (not 65535). (Adobe help, Color basics; SDK `PF_MAX_CHAN16` = 32768, slider display "0-32768".)
  - Clamping points identical to 8 bpc (AE-CMP-002): every step's result clamped to [0, 32768] and quantised.
  - Conversion 8 → 16: `v16 = round(v8 * 32768 / 255)`; 16 → 8: `v8 = round(v16 * 255 / 32768)` (UNVERIFIED exact rounding).
  - Files with 16-bit channels (0-65535) are rescaled on import: `v16 = round(vfile * 32768 / 65535)` (UNVERIFIED rounding).
- **Priority:** P1 — common for gradients/banding avoidance; BAE gets it almost for free from float buffers.
- **Owner:** bae-render (+ bae-color)
- **Notes:** The extra code value means 16-bpc midpoint grey = 16384. Info panel can display values as 0-1, 0-255, 0-32768 or percent; see ui-preview-render-io.md, Info panel.

### AE-CMP-004 · 32 bits per channel floating-point pixels (32 bpc 浮点（每通道 32 位）)

- **What it does:** In a 32-bpc project buffers store IEEE-754 single floats; values above 1.0 (overbright / over-range) and below 0.0 are kept through layers, effects, blend modes and nested comps.
- **Exact semantics:**
  - `x ∈ (-∞, +∞)` per colour channel, white = 1.0. Alpha is stored as float too; UNVERIFIED: AE keeps alpha within [0, 1] at compositing steps (opacity, masks and mattes multiply alpha by values in [0, 1]), but individual 32-bpc effects may output alpha outside [0, 1].
  - No clamp is applied between steps; clamping happens only where an operation is defined that way (e.g. some blend modes clamp, see AE-CMP-012 onward), at conversion to 8/16 bpc effects, at display (viewer shows values clipped to the display range unless the HDR/exposure tools are used) and at output to an integer format.
  - Blend modes, glows and blurs can look different from 8/16 bpc because inputs are not clamped (Adobe help, Color basics: "some blending modes behave differently in 32-bpc mode").
  - NaN / Inf: UNVERIFIED: AE does not sanitise NaN produced by effects; they propagate.
- **Priority:** P0 — BAE's float-only GPU pipeline makes this the natural native mode, and HDR is a core BAE goal.
- **Owner:** bae-render (+ bae-color)
- **Notes:** Linear-light blending (Blend Colors Using 1.0 Gamma / Linearize Working Space) is specified in compositing-3d-color.md, Color management; it is independent of bit depth but most useful at 32 bpc. Implementation hint: GPU half floats (RGBA16F) have max 65504 and ~3 decimal digits of precision; this is enough for display but differs from AE 32 bpc for very large or tiny values — offer RGBA32F as the "32 bpc" mode and RGBA16F as the default.

### AE-CMP-005 · Emulating 8/16 bpc clamping in BAE float buffers (在浮点缓冲中模拟 8/16 bpc 截断（译名待核）)

- **What it does:** Makes BAE's float GPU pipeline produce the same numbers as AE when the project is set to 8 or 16 bpc.
- **Exact semantics:**
  - BAE working buffers are RGBA16F by default or RGBA32F (ARCHITECTURE 5). When the project depth is 8 or 16 bpc, every place listed in AE-CMP-002 as a clamping point applies `x = clamp01(x)` to all four channels (straight colour) before the result is stored.
  - Optional quantisation (fidelity mode): `x = round(x * N) / N` with `N = 255` (8 bpc) or `N = 32768` (16 bpc), applied at the same points. Default: clamp only, no quantisation (UNVERIFIED that quantisation differences are visible enough to matter; they cause ±1 code value differences).
  - Clamping is applied to straight colour; when buffers are premultiplied, clamp as `cp = min(max(cp, 0), a)` and `a = clamp01(a)`, which is equivalent.
  - Clamping points (minimum set): footage decode output; each effect output; output of mask/matte/opacity stage; each blend with the layers below; nested comp output; the final comp output.
- **Priority:** P0 — without it 8-bpc projects (the AE default) render differently, e.g. Add/Screen/Color Dodge chains and Levels after a Glow.
- **Owner:** bae-render (+ bae-color, bae-effects)
- **Notes:** BAE decision: clamping is a pipeline mode, not a separate buffer format, so 8/16/32 bpc share one shader set with a `clamp_mode` uniform. Implementation hint: fuse the clamp into the blend shader's final store; effects compiled with a `CLAMP` specialization constant.

## Comp render order

### AE-CMP-006 · Composition stacking and layer participation (合成中的图层堆积顺序（译名待核）)

- **What it does:** A comp frame is built by rendering its layers from the bottom of the Timeline stack to the top, each layer compositing onto the result of the layers below it.
- **Exact semantics:**
  ```
  render_comp(comp, t):
    acc = transparent (RGBA 0,0,0,0), size = comp size * resolution factor
    for layer in comp.layers ordered from highest index (bottom) to index 1 (top):
      if !participates(layer, t): continue
      if layer is adjustment layer:  acc = apply_adjustment(layer, acc, t)   # AE-CMP-049 Adjustment layers
      elif layer is 3D and the renderer groups it: consecutive 3D layers form one 3D group,
           rendered together (depth sorted / intersected) and composited as a unit (see compositing-3d-color.md, Classic 3D renderer)
      else: acc = composite(layer_pixels(layer, t), acc, layer.blend_mode, layer.preserve_transparency)
    return acc            # comp background colour is NOT part of acc (AE-CMP-010)
  participates(layer, t) =
      layer.video_enabled (eye switch)                       # matte layers: see Track mattes
      and layer.in_point <= t < layer.out_point
      and (no layer in comp is soloed, or layer.solo)
      and layer has visual content (not audio-only, not camera/light/null-without-source)
      and not (layer.guide_layer and this render is a nested-comp render or a final render)
  ```
  - Layer index 1 is the top of the stack. A layer whose in/out excludes `t` contributes nothing (also no adjustment, no matte).
  - Solo affects only the comp being viewed/rendered at that level; the soloed state of layers in a nested comp applies inside that nested comp. UNVERIFIED: solo in a nested comp is honoured when rendering the outer comp.
  - Shy switch and "hide shy layers" never affect rendering.
  - Guide layers render in the comp's own viewer, but are excluded from final render output and from the nested-comp render when the comp is used as a layer in another comp (Adobe help, Creating layers: guide layers). Render Settings has "Guide Layers: Current Settings / All Off" (see ui-preview-render-io.md, Render Queue).
  - Nested comps (precomp layers) are rendered before they are composited (AE renders the nested comp, at the time mapped through the precomp layer's time stretch/remap, then treats it as footage). See AE-CMP-054 Nested comps.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Stacking order editing, layer switches and solo semantics as data are in project-comp-layers.md, Layer switches / Stacking. Lottie: `layers[0]` is the top layer, players render from the last array element to the first; layers with `hd: true` are hidden; `ip`/`op` are in frames. Implementation hint: build the per-frame render graph from the EvaluatedComp once, skipping non-participating layers before any GPU work; group consecutive 2D layers with Normal mode and no matte into one batched draw pass.

### AE-CMP-007 · Per-layer render pipeline for raster layers (栅格图层的渲染顺序（译名待核）)

- **What it does:** Fixes the order in which a non-vector (footage, solid, precomp without collapse) layer's own operations are applied before it is composited.
- **Exact semantics:**
  ```
  layer_pixels(layer, t):                      # all in layer (source) pixel space until step 4
    1. src  = source frame at layer-local time (time stretch / time remap / frame blending,
              Interpret Footage, proxy); precomp = nested comp render (AE-CMP-054)
    2. src  = apply masks top-to-bottom (AE-CMP-038 Masks) -> alpha multiplied by mask result
    3. src  = apply effects top-to-bottom (Effect Controls order), each effect sees the previous
              effect's output; effects may enlarge the buffer (Grow Bounds etc.)
    4. img  = transform into comp space: anchor point, position, scale, rotation / orientation,
              opacity (alpha *= opacity/100), with motion-blur sub-samples if MB is on
              (AE-CMP-064 Motion blur); resampling per Quality/Sampling (AE-CMP-059..061)
    5. img  = layer styles (drop shadow, strokes, ... ; see project-comp-layers.md, Layer styles)
    6. img  = track matte: img.alpha *= matte value (AE-CMP-034 Track mattes)
    7. acc  = blend img onto acc with blend mode and Preserve Underlying Transparency
  ```
  - Adobe help (Precomposing > Render order): "Within each raster layer, elements are applied in the following order: masks, effects, transformations, and layer styles." Masks within the Masks group and effects within the Effects group are processed top to bottom.
  - Steps 6-7: UNVERIFIED exact position of the track matte relative to layer styles; observed AE behaviour is that a track matte also cuts the layer's styles (e.g. a drop shadow outside the matte disappears), i.e. matte after styles.
  - Opacity is part of the Transform group but acts as an alpha multiplier at step 4; it is applied before the blend mode (AE-CMP-031 Opacity).
  - What the Layer viewer shows is the result before step 4 (its View menu can show the source, the masks stage, or any effect stage).
  - The Transform effect (Distort) lets the user insert an extra transform among effects; it does not move the layer's own Transform step.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim, bae-effects)
- **Notes:** Some effects ignore the layer's masks (they read the unmasked source, e.g. effects that take a "layer" parameter read that layer's source or masks-and-effects per their own setting); Adobe help recommends precomposing in that case. Lottie: masks (`masksProperties`) are applied before effects (`ef`) in lottie-web; layer styles (`sy`) are mostly unsupported by players. Implementation hint: steps 1-3 run at layer resolution in a layer-sized texture; step 4 is one resampling draw into a comp-sized (or ROI-sized) target, fused with opacity, matte and blend when no layer style is present.

### AE-CMP-008 · Per-layer render pipeline for continuously rasterized layers (连续栅格化图层的渲染顺序（译名待核）)

- **What it does:** For vector layers that are continuously rasterized (shape, text, and AI/PDF/EPS footage with the switch on), the transform is applied before effects so vectors are rasterised sharp at their final size.
- **Exact semantics:**
  ```
  layer_pixels_cr(layer, t):
    1. vector content (shape contents / text / vector file) evaluated in layer space
    2. masks top-to-bottom (mask paths are in layer space; they move with the layer)
    3. rasterise vectors + masks directly into comp space using the full layer transform
       (with motion-blur sub-samples) and opacity; anti-aliased in Best quality
    4. effects top-to-bottom, operating on the comp-space raster
    5. layer styles, 6. track matte, 7. blend (as AE-CMP-007)
  ```
  - Adobe help (Layers > Continuous rasterization): default order is "masks, followed by transformations, and then effects" for continuously rasterized layers versus "masks, effects, transformations" for other layers, so effect results can differ (e.g. a blur radius is not scaled by layer scale; directional parameters do not rotate with the layer).
  - UNVERIFIED: effect parameters that are points in layer space (e.g. a Point Control used by an effect) are mapped through the layer transform so that they still land on the same layer-space location.
  - Shape and text layers are always continuously rasterized; vector footage layers are only when the switch is on (otherwise they are rasterised once at 100% of their document size, then treated as raster layers).
- **Priority:** P0 — shape and text layers are always on this path.
- **Owner:** bae-render (+ bae-vector, bae-text, bae-effects)
- **Notes:** Full Collapse Transformations / Continuously Rasterize rules (when effects force a rasterisation, buffer bounds, 3D interaction) are in this file, section Collapse Transformations and Continuous Rasterization. Implementation hint: Vello renders the layer's vector scene with the final layer-to-comp matrix straight into an ROI-sized texture; effects then run on that texture.

### AE-CMP-009 · Layer viewer render stage selection (图层面板"视图"渲染阶段（译名待核）)

- **What it does:** The Layer viewer can display the layer at any stage of its pre-transform pipeline: Masks, or after any effect; this is the same pipeline as AE-CMP-007 truncated.
- **Exact semantics:**
  - View menu entries in order: Masks, then one entry per effect in stack order (UNVERIFIED exact first entry naming; when "Render" is unchecked the viewer shows the untouched source with mask outlines).
  - Selecting an effect renders steps 1-3 of AE-CMP-007 up to and including that effect; transforms, styles, mattes and blend are never applied in the Layer viewer.
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** The SDK (`AEGP_RenderOptionsSuite`, "what's new" notes) exposes the same choice for third-party renders ("render the layer from its source, from its masks, or from its effects"). Viewer UI is in ui-preview-render-io.md, Layer viewer.

### AE-CMP-010 · Composition background colour in rendering (合成背景颜色)

- **What it does:** The comp Background Color is shown behind the comp in the Composition viewer but is not a layer: it is not blended with, not seen by adjustment layers or blend modes, and becomes transparent when the comp is nested or rendered with alpha.
- **Exact semantics:**
  - The comp is always composited over transparent black (AE-CMP-006). Display: viewer draws `out = comp_over(bg)` = `Cp + (1 - a) * bg` unless the transparency grid is on (see compositing-3d-color.md, transparency grid).
  - Nesting: the nested comp's background colour is discarded (transparent); the containing comp's background is used (Adobe help, Composition settings).
  - Output: with Channels = RGB+Alpha the background is transparent. With Channels = RGB, UNVERIFIED: the frame is matted against the comp background colour (best understanding of "RGB" output; to get black use a black background or a solid).
  - Blend modes on the bottom-most layer therefore blend against transparency, not the background colour: e.g. a Multiply layer alone in a comp looks like Normal.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** The Background Color setting (default black `[0,0,0]`) is persisted in Composition Settings, see project-comp-layers.md, Composition Settings; scripting `CompItem.bgColor`. Lottie has no comp background (players draw transparent). Gotcha: users often expect blend modes to interact with the background colour — they do not; the standard workaround is a solid layer.

## Blend modes

### AE-CMP-011 · Blending mode menu, data and enums (混合模式)

- **What it does:** Every AV layer has exactly one blending mode (default Normal) chosen from a fixed 38-entry menu in eight separator-delimited groups; the mode is not animatable.
- **Exact semantics:** Menu order (group separators shown as `—`), with official zh-CN UI names, ExtendScript `BlendingMode` enum, and Lottie layer `bm` value:

  | # | Mode (中文) | Group | ExtendScript `BlendingMode.` | Lottie `bm` |
  |---|---|---|---|---|
  | 1 | Normal (正常) | Normal | `NORMAL` | 0 |
  | 2 | Dissolve (溶解) | Normal | `DISSOLVE` | — |
  | 3 | Dancing Dissolve (动态抖动溶解) | Normal | `DANCING_DISSOLVE` | — |
  | — | | | | |
  | 4 | Darken (变暗) | Subtractive | `DARKEN` | 4 |
  | 5 | Multiply (相乘) | Subtractive | `MULTIPLY` | 1 |
  | 6 | Color Burn (颜色加深) | Subtractive | `COLOR_BURN` | 7 |
  | 7 | Classic Color Burn (经典颜色加深) | Subtractive | `CLASSIC_COLOR_BURN` | — |
  | 8 | Linear Burn (线性加深) | Subtractive | `LINEAR_BURN` | — |
  | 9 | Darker Color (深色) | Subtractive | `DARKER_COLOR` | — |
  | — | | | | |
  | 10 | Add (相加) | Additive | `ADD` | 16 |
  | 11 | Lighten (变亮) | Additive | `LIGHTEN` | 5 |
  | 12 | Screen (滤色) | Additive | `SCREEN` | 2 |
  | 13 | Color Dodge (颜色减淡) | Additive | `COLOR_DODGE` | 6 |
  | 14 | Classic Color Dodge (经典颜色减淡) | Additive | `CLASSIC_COLOR_DODGE` | — |
  | 15 | Linear Dodge (线性减淡) | Additive | `LINEAR_DODGE` | — |
  | 16 | Lighter Color (浅色) | Additive | `LIGHTER_COLOR` | — |
  | — | | | | |
  | 17 | Overlay (叠加) | Complex | `OVERLAY` | 3 |
  | 18 | Soft Light (柔光) | Complex | `SOFT_LIGHT` | 9 |
  | 19 | Hard Light (强光) | Complex | `HARD_LIGHT` | 8 |
  | 20 | Linear Light (线性光) | Complex | `LINEAR_LIGHT` | — |
  | 21 | Vivid Light (亮光) | Complex | `VIVID_LIGHT` | — |
  | 22 | Pin Light (点光) | Complex | `PIN_LIGHT` | — |
  | 23 | Hard Mix (实色混合) | Complex | `HARD_MIX` | 17 |
  | — | | | | |
  | 24 | Difference (差值) | Difference | `DIFFERENCE` | 10 |
  | 25 | Classic Difference (经典差值) | Difference | `CLASSIC_DIFFERENCE` | — |
  | 26 | Exclusion (排除) | Difference | `EXCLUSION` | 11 |
  | 27 | Subtract (相减) | Difference | `SUBTRACT` | — |
  | 28 | Divide (相除) | Difference | `DIVIDE` | — |
  | — | | | | |
  | 29 | Hue (色相) | HSL | `HUE` | 12 |
  | 30 | Saturation (饱和度) | HSL | `SATURATION` | 13 |
  | 31 | Color (颜色) | HSL | `COLOR` | 14 |
  | 32 | Luminosity (明度) | HSL | `LUMINOSITY` | 15 |
  | — | | | | |
  | 33 | Stencil Alpha (模板 Alpha) | Matte | `STENCIL_ALPHA` | — |
  | 34 | Stencil Luma (模板亮度) | Matte | `STENCIL_LUMA` | — |
  | 35 | Silhouette Alpha (轮廓 Alpha) | Matte | `SILHOUETE_ALPHA` (sic, one T) | — |
  | 36 | Silhouette Luma (轮廓亮度) | Matte | `SILHOUETTE_LUMA` | — |
  | — | | | | |
  | 37 | Alpha Add (Alpha 添加) | Utility | `ALPHA_ADD` | — |
  | 38 | Luminescent Premul (冷光预乘) | Utility | `LUMINESCENT_PREMUL` | — |

  - Scripting attribute `AVLayer.blendingMode` (read/write). The mode is a layer attribute, not a property: no keyframes, no expressions (Adobe help suggests splitting the layer, or the Compound Arithmetic effect, to change it over time).
  - The same mode list (subsets) is reused by paint strokes, layer styles, some effects (e.g. Blend, Set Matte "composite" options) and shape-layer Blend Mode; those are specified with their features.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** zh-CN names are from the zh-CN help category lists (Darker Color appears as 深色 in the category list and 较深的颜色 in a heading; Lighter Color 浅色 / 较浅的颜色; Luminosity 明度 / 发光度; Hard Mix 实色混合) — UNVERIFIED which form the AE 2026 zh-CN menu uses（译名待核）. Lottie: `bm` 0-17 per the Lottie spec; lottie-web implements only 0-15 (CSS `mix-blend-mode` names, `player/js/utils/blendModes.js`), so Add (16) and Hard Mix (17) are dropped there; Skottie implements all 18 (`modules/skottie/src/BlendModes.cpp`: 16 → `SkBlendMode::kPlus`, 17 → a runtime Hard Mix blender); Lottie has no Dissolve, Classic*, Linear Burn/Dodge/Light, Vivid/Pin Light, Darker/Lighter Color, Subtract, Divide, Stencil/Silhouette, Alpha Add, Luminescent Premul — BAE Lottie export must flatten or warn (see ui-preview-render-io.md, Lottie export). Keyboard: Shift+- / Shift+= cycle modes of selected layers (ui-preview-render-io.md, shortcuts).

### AE-CMP-012 · Generic blend compositing equation (混合模式合成公式（译名待核）)

- **What it does:** Defines how a separable or non-separable blend function `B(Cb, Cs)` is combined with source and backdrop alpha, so that each mode item only has to specify `B`.
- **Exact semantics:**
  - Adobe help states the AE modes are "identical to blending modes in Adobe Photoshop" and refers to the PDF Reference 1.7 §7.2.4 for the algorithms. The PDF/W3C model, written premultiplied:
  ```
  as' = as                                   # source alpha after masks, effects, opacity, matte
  ao  = as' + ab * (1 - as')                 # union alpha (all modes in this item)
  cop = (1 - ab) * csp + (1 - as') * cbp + as' * ab * B(Cb, Cs)
  Co  = (ao > 0) ? cop / ao : 0              # straight result colour
  # equivalently (straight):  Cmix = (1 - ab) * Cs + ab * B(Cb, Cs);  then Normal "over" with Cmix
  ```
  - Normal is `B(Cb, Cs) = Cs`, giving ordinary Porter-Duff source-over.
  - `B` is evaluated on straight colour of both inputs, per channel for separable modes; non-separable modes (Darker/Lighter Color, Hue, Saturation, Color, Luminosity) take whole RGB triples.
  - Bit-depth behaviour:
    - 8/16 bpc: `Cb`, `Cs` are already in [0, 1]; `B` results are clamped to [0, 1]; then the output is stored with clamping (AE-CMP-005).
    - 32 bpc: inputs may be < 0 or > 1; `B` is evaluated unclamped where the formula allows (Add, Subtract, Divide, Multiply, Screen, Linear modes produce over-range / negative values); modes that are only meaningful in [0, 1] (Color Burn/Dodge, Vivid/Pin Light, Hard Mix, Overlay, Soft Light, HSL modes) — UNVERIFIED: AE evaluates the same formula without clamping inputs, so results for over-range inputs follow the formula literally (e.g. Screen with `Cs = 2` gives `Cb + 2 - 2*Cb`).
  - Colour space: blending happens in the project working space's encoded values, or in linear light when "Blend Colors Using 1.0 Gamma" / "Linearize Working Space" is on (see compositing-3d-color.md, Color management).
  - Modes that change alpha differently (Dissolve, Stencil/Silhouette, Alpha Add, Luminescent Premul) override this equation; see their items.
  - Edge cases: `ao == 0` → output transparent black; `ab == 0` → result equals the source (no blend visible); `as' == 0` → backdrop unchanged.
- **Priority:** P0
- **Owner:** bae-render (+ bae-color)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/blending-modes-layer-styles.html ("identical to blending modes in Adobe Photoshop"; "see section 7.2.4 of version 1.7 of the PDF reference"). UNVERIFIED: that AE's implementation matches the PDF/W3C alpha equation exactly for every mode (especially the Classic modes, which predate it, and Add / Luminescent Premul). Behaviour visible in AE consistent with it: a non-Normal layer over transparency looks like Normal. Implementation hint: one WGSL "blend" fragment shader with a `mode` switch reading premultiplied inputs, unmultiplying with `max(a, 1e-6)` guard, applying `B`, recombining with the equation above; for Normal use fixed-function hardware blending (`One, OneMinusSrcAlpha`) and batch consecutive Normal layers. Adobe help lists eight modes (Screen, Multiply, Add, Overlay, Hard Light, Classic Difference, Lighten, Darken) as GPU-accelerated at 32 bpc when the layer has motion blur — irrelevant to results, but indicates AE's own GPU path.

### AE-CMP-013 · Normal blend mode (正常)

- **What it does:** Composites the source over the backdrop with ordinary alpha compositing; the default mode.
- **Exact semantics:**
  ```
  B(Cb, Cs) = Cs
  ao  = as + ab*(1 - as)
  cop = csp + cbp*(1 - as)
  ```
  - With `as == 1` the backdrop is fully replaced; with opacity < 100% the backdrop shows through proportionally.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Lottie `bm: 0` (or `bm` absent). ExtendScript `BlendingMode.NORMAL`.

### AE-CMP-014 · Dissolve and Dancing Dissolve (溶解和动态抖动溶解)

- **What it does:** Replaces partial transparency with a random per-pixel choice: each pixel is either the fully opaque source colour or the untouched backdrop, with probability equal to the source alpha; Dancing Dissolve re-randomises every frame.
- **Exact semantics:**
  ```
  r = rand01(seed, pixel_x, pixel_y [, frame_time for Dancing Dissolve])   # uniform in [0,1)
  if r < as':   ao = 1; Co = Cs                                  # pixel fully opaque source
  else:         ao = ab;  Co = Cb                                # pixel untouched
  ```
  - `as'` includes opacity, masks/feather, effects and track matte, so a soft mask edge becomes a speckled edge.
  - Dissolve: noise pattern is constant over time for a given layer (changes only if the layer moves, because the pattern is in comp pixel space — UNVERIFIED whether it is in comp space or layer space). Dancing Dissolve: pattern changes on each rendered frame.
  - Dissolve and Dancing Dissolve do not work on 3D layers (Adobe help: "Dissolve and Dancing Dissolve do not work on 3D layers"); UNVERIFIED: on a 3D layer they then behave like Normal.
  - Seeding: UNVERIFIED (AE's hash is not documented); BAE needs a deterministic hash of (layer id, x, y[, frame]) so renders are reproducible.
- **Priority:** P2 — rarely used.
- **Owner:** bae-render
- **Notes:** Not in Lottie. At preview resolution < Full the dither grain is per downsampled pixel (UNVERIFIED). Implementation hint: integer hash (e.g. PCG) of pixel coordinates in the shader; no texture needed.

### AE-CMP-015 · Darken, Multiply and Linear Burn (变暗、相乘和线性加深)

- **What it does:** Simple per-channel darkening modes.
- **Exact semantics:**
  ```
  Darken:      B = min(Cb, Cs)
  Multiply:    B = Cb * Cs                 # 8 bpc: (b*s)/255 ; 16 bpc: (b*s)/32768
  Linear Burn: B = Cb + Cs - 1             # 8/16 bpc: clamped at 0
  ```
  - 32 bpc: Multiply and Darken are unclamped (over-range × over-range grows); Linear Burn may go negative (UNVERIFIED whether AE clamps Linear Burn at 0 in 32 bpc).
  - White source is neutral for Multiply and Linear Burn; black source gives black.
- **Priority:** P0 — Multiply is among the most used modes; Darken and Linear Burn ride along.
- **Owner:** bae-render
- **Notes:** Lottie: Multiply `bm: 1`, Darken `bm: 4`; Linear Burn has no Lottie value.

### AE-CMP-016 · Color Burn and Classic Color Burn (颜色加深和经典颜色加深)

- **What it does:** Darkens the backdrop by increasing contrast according to the source.
- **Exact semantics:**
  ```
  Color Burn (PDF/W3C):
    if Cb >= 1:      B = 1
    elif Cs <= 0:    B = 0
    else:            B = 1 - min(1, (1 - Cb) / Cs)
  Classic Color Burn (AE 5.0 and earlier):
    UNVERIFIED: B = 1 - (1 - Cb) / Cs clamped to [0, 1], computed on premultiplied colour
    (without the PDF/W3C alpha handling), with Cs == 0 -> 0
  ```
  - Pure white source leaves the backdrop unchanged.
  - 32 bpc: UNVERIFIED handling of `Cb > 1` or negative values; the formula above yields 1 for `Cb >= 1`.
- **Priority:** P1 (Classic variant P2-level importance, kept in the same item because it shares the code path).
- **Owner:** bae-render
- **Notes:** Lottie Color Burn `bm: 7`. Adobe help: Classic Color Burn is the AE 5.0-and-earlier Color Burn, kept for compatibility with old projects. BAE decision: until verified against AE renders, Classic variants may map to the modern formula with a golden-image test flagged as known difference.

### AE-CMP-017 · Darker Color and Lighter Color (深色和浅色)

- **What it does:** Non-separable modes that pick the whole source or whole backdrop colour, whichever is darker (or lighter).
- **Exact semantics:**
  ```
  sum(C) = C.r + C.g + C.b            # Photoshop: "compares the total of all channel values"
  Darker Color:  B = (sum(Cs) < sum(Cb)) ? Cs : Cb
  Lighter Color: B = (sum(Cs) > sum(Cb)) ? Cs : Cb
  ```
  - UNVERIFIED: AE compares channel sums (as documented for Photoshop) rather than luminosity; tie-break (equal sums) keeps the backdrop (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** Not in Lottie. Unlike Darken/Lighten no new colours are created.

### AE-CMP-018 · Add and Linear Dodge (相加和线性减淡)

- **What it does:** Adds source and backdrop channel values; the result is never darker than either input.
- **Exact semantics:**
  ```
  Add:          B = Cb + Cs            # 8/16 bpc clamped to 1; 32 bpc unclamped (overbright)
  Linear Dodge: B = Cb + Cs            # Photoshop "Linear Dodge (Add)"
  ```
  - UNVERIFIED: the AE difference between Add and Linear Dodge. Best understanding: Add is AE's legacy additive mode (added to premultiplied colour so that an edge's partial alpha contributes `Cs*as` light without darkening, i.e. `cop = cbp + csp`, `ao = as + ab*(1-as)`), while Linear Dodge follows the PDF/W3C equation (AE-CMP-012) and clamps at 1 even at 32 bpc. Needs golden tests against AE.
  - Black source leaves the backdrop unchanged.
- **Priority:** P0 — Add is the standard mode for glows, light leaks, particles.
- **Owner:** bae-render
- **Notes:** Lottie Add `bm: 16` (not implemented by lottie-web; Skottie maps it to `SkBlendMode::kPlus`). Add is one of the eight modes AE accelerates on the GPU at 32 bpc.

### AE-CMP-019 · Lighten and Screen (变亮和滤色)

- **What it does:** Per-channel lightening modes.
- **Exact semantics:**
  ```
  Lighten: B = max(Cb, Cs)
  Screen:  B = 1 - (1 - Cb) * (1 - Cs)  = Cb + Cs - Cb*Cs
  ```
  - 32 bpc: Screen with an input > 1 can produce results lower than `max(Cb, Cs)` or negative-derived artefacts (e.g. `Cb = 2, Cs = 0.5` → `2 + 0.5 - 1.0 = 1.5`, darker than the backdrop), because the formula is used literally (UNVERIFIED; this is the well-known "Screen breaks in 32 bpc" behaviour that leads users to Add in HDR).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Lottie Screen `bm: 2`, Lighten `bm: 5`.

### AE-CMP-020 · Color Dodge and Classic Color Dodge (颜色减淡和经典颜色减淡)

- **What it does:** Brightens the backdrop by decreasing contrast according to the source.
- **Exact semantics:**
  ```
  Color Dodge (PDF/W3C):
    if Cb <= 0:      B = 0
    elif Cs >= 1:    B = 1
    else:            B = min(1, Cb / (1 - Cs))
  Classic Color Dodge (AE 5.0 and earlier):
    UNVERIFIED: B = Cb / (1 - Cs) clamped to [0, 1], on premultiplied colour, Cs == 1 -> 1
  ```
  - Black source leaves the backdrop unchanged.
  - 32 bpc: UNVERIFIED whether the `min(1, ...)` clamp is applied (likely not for over-range `Cb`).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Lottie Color Dodge `bm: 6`. Classic variant: see AE-CMP-016 Notes.

### AE-CMP-021 · Overlay and Hard Light (叠加和强光)

- **What it does:** Multiply-or-screen modes: Overlay switches on the backdrop value, Hard Light on the source value (Hard Light = Overlay with inputs swapped).
- **Exact semantics:**
  ```
  HardLight(Cb, Cs) = (Cs <= 0.5) ? Cb * 2*Cs                      # multiply
                                   : 1 - (1 - Cb) * (1 - (2*Cs - 1)) # screen(Cb, 2Cs-1)
  Overlay(Cb, Cs)   = HardLight(Cs, Cb)                             # switch on Cb <= 0.5
  ```
  - A 50% grey (0.5) source is neutral for both modes (result = `Cb`); a 50% grey backdrop under Overlay yields the source colour (`Overlay(0.5, Cs) = Cs`).
  - 32 bpc: formula used literally for over-range values (UNVERIFIED).
- **Priority:** P0 — Overlay is a staple; Hard Light shares the code.
- **Owner:** bae-render
- **Notes:** Adobe help's Hard Light text says the switch depends on "the underlying color"; the Photoshop/PDF definition (and the same help page's first sentence, "depending on the original source color") switch on the source — BAE follows the PDF/W3C definition above; UNVERIFIED against AE renders. Lottie Overlay `bm: 3`, Hard Light `bm: 8`. Both are GPU-accelerated in AE 32 bpc.

### AE-CMP-022 · Soft Light (柔光)

- **What it does:** A gentle burn/dodge of the backdrop driven by the source; never produces pure black or white from non-pure inputs.
- **Exact semantics:**
  ```
  # PDF 1.7 / W3C definition:
  if Cs <= 0.5:  B = Cb - (1 - 2*Cs) * Cb * (1 - Cb)
  else:          B = Cb + (2*Cs - 1) * (D(Cb) - Cb)
  D(x) = (x <= 0.25) ? ((16*x - 12)*x + 4)*x : sqrt(x)
  ```
  - UNVERIFIED: AE may use the Photoshop variant where `D(x) = sqrt(x)` for all x (differs slightly for dark backdrops); verify with a golden test (backdrop ramp × source ramp).
  - 32 bpc: `sqrt` of negative backdrop is undefined → treat `Cb < 0` as 0 for `D` (UNVERIFIED AE behaviour).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Lottie `bm: 9` (lottie-web uses CSS `soft-light`, which is the W3C formula).

### AE-CMP-023 · Linear Light, Vivid Light, Pin Light and Hard Mix (线性光、亮光、点光和实色混合)

- **What it does:** Photoshop "light" family: burn or dodge the backdrop depending on whether the source is above or below 50% grey; Hard Mix posterises the Vivid Light result to 0 or 1 per channel.
- **Exact semantics:**
  ```
  LinearLight: B = Cb + 2*Cs - 1                                # 8/16 bpc clamp [0,1]
  VividLight:  B = (Cs <= 0.5) ? ColorBurn(Cb, 2*Cs)           # AE-CMP-016 modern formula
                               : ColorDodge(Cb, 2*Cs - 1)       # AE-CMP-020 modern formula
  PinLight:    B = (Cs <= 0.5) ? min(Cb, 2*Cs) : max(Cb, 2*Cs - 1)
  HardMix:     B = (Cb + Cs >= 1) ? 1 : 0                       # per channel
  ```
  - Hard Mix threshold equality: in 8 bpc, `b + s >= 255` → 255 (UNVERIFIED tie handling; Photoshop's documented rule is "sum ≥ 255 → 255").
  - Hard Mix ignores source opacity in Photoshop's "fill" sense; in AE opacity still mixes via AE-CMP-012 (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Adobe help describes these as switching on "the underlying color"; Photoshop and the PDF-style definitions switch on the source (blend) colour — BAE uses the source switch above, UNVERIFIED against AE. Lottie: only Hard Mix exists (`bm: 17`, not implemented by lottie-web). Divisions inside Vivid Light: `2*Cs == 0` → 0 (burn), `2*Cs - 1 == 1` → 1 (dodge) with the guards of AE-CMP-016/020.

### AE-CMP-024 · Difference, Classic Difference and Exclusion (差值、经典差值和排除)

- **What it does:** Per-channel difference modes; white source inverts the backdrop, black leaves it unchanged.
- **Exact semantics:**
  ```
  Difference:         B = |Cb - Cs|
  Exclusion:          B = Cb + Cs - 2*Cb*Cs
  Classic Difference: UNVERIFIED: |cbp - csp| on premultiplied colour with alpha
                      handled the pre-AE-6 way (result alpha = union), giving different
                      edges on partially transparent pixels than Difference
  ```
  - 32 bpc: Difference of over-range values is unclamped (e.g. `|3 - 1| = 2`). Adobe help: in Difference "painting with white inverts the backdrop" — at 32 bpc that is only true for backdrop ≤ 1.
- **Priority:** P0 — Difference is used daily for alignment checks.
- **Owner:** bae-render
- **Notes:** Lottie Difference `bm: 10`, Exclusion `bm: 11`; Classic Difference has no Lottie value. Classic Difference is GPU-accelerated in AE 32 bpc (Adobe help).

### AE-CMP-025 · Subtract and Divide (相减和相除)

- **What it does:** Arithmetic modes: backdrop minus source; backdrop divided by source.
- **Exact semantics:**
  ```
  Subtract: B = Cb - Cs                    # 8/16 bpc: clamp at 0; 32 bpc: may be < 0 (Adobe help)
  Divide:   B = (Cs > 0) ? Cb / Cs : (Cb > 0 ? 1 : 0)   # 8/16 bpc: clamp at 1; 32 bpc: may be > 1
  ```
  - Divide by zero: UNVERIFIED. Photoshop yields white for `Cs = 0` (unless `Cb = 0`); at 32 bpc AE may return a large or infinite value — BAE must not emit Inf/NaN; BAE decision: return `Cb / max(Cs, 1e-6)` capped at 65504 in 32 bpc (UNVERIFIED vs AE) and the Photoshop rule at 8/16 bpc.
  - White source is neutral for Divide; black source is neutral for Subtract.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Not in Lottie.

### AE-CMP-026 · Hue, Saturation, Color and Luminosity (色相、饱和度、颜色和明度)

- **What it does:** Non-separable modes that combine the hue, saturation and luminosity of source and backdrop.
- **Exact semantics:**
  ```
  Lum(C)     = 0.30*C.r + 0.59*C.g + 0.11*C.b
  Sat(C)     = max(C.r, C.g, C.b) - min(C.r, C.g, C.b)
  ClipColor(C): l = Lum(C); n = min(C); x = max(C)
                if n < 0: C = l + (C - l) * l / (l - n)
                if x > 1: C = l + (C - l) * (1 - l) / (x - l)
                return C
  SetLum(C, l): d = l - Lum(C); return ClipColor(C + d)
  SetSat(C, s): (Cmax, Cmid, Cmin) = sort channels
                if Cmax > Cmin: Cmid = (Cmid - Cmin) * s / (Cmax - Cmin); Cmax = s
                else: Cmid = Cmax = 0
                Cmin = 0; return C (channels in original positions)
  Hue:        B = SetLum(SetSat(Cs, Sat(Cb)), Lum(Cb))
  Saturation: B = SetLum(SetSat(Cb, Sat(Cs)), Lum(Cb))
  Color:      B = SetLum(Cs, Lum(Cb))
  Luminosity: B = SetLum(Cb, Lum(Cs))
  ```
  - Luma weights 0.30 / 0.59 / 0.11 per PDF 1.7 and Photoshop; UNVERIFIED that AE uses exactly these (not Rec.709 0.2126/0.7152/0.0722).
  - 32 bpc: `ClipColor` forces results into [0, 1] in the PDF model; UNVERIFIED whether AE skips `ClipColor` for 32 bpc (if it does, overbright luminosity survives Color/Luminosity modes).
- **AE 26.2.1 实测（8 bpc）：** 上述四个函数在半透明像素上的底色取自“底图覆盖源图”的结果。令 `ao = ab + as*(1-ab)`，若 `ao > 0`，则 `D = (cbp + csp*(1-ab))/ao`，最终预乘颜色为 `ao * lerp(D, B(D, Cs), as)`。整数位深在 alpha、预乘底色和直通底色处取整。用 6 对 RGB × 前后景各 4 档不透明度（25/50/75/100%）逐像素对比，色相、饱和度、颜色、明度的最大通道差分别为 2、3、1、2（满值 255）。这些数值只针对该 96 格网格。饱和度的整数取整仍待校准；16/32 bpc 已有 CPU/GPU 对照，尚无对应的 AE 位深实测。[校准记录 #257](https://github.com/FELIXDAD/betterAfterEffect/issues/257)
- **Priority:** P1 — Color and Luminosity are commonly used for tinting/grading.
- **Owner:** bae-render (+ bae-color)
- **Notes:** Lottie `bm` 12 Hue, 13 Saturation, 14 Color, 15 Luminosity (lottie-web: CSS `hue`/`saturation`/`color`/`luminosity`, which are this W3C definition). Implementation hint: all four share one shader function set; avoid branches by sorting with `min/max`.

### AE-CMP-027 · Stencil Alpha and Stencil Luma (模板 Alpha 和模板亮度)

- **What it does:** The stencil layer is not drawn; instead it multiplies the alpha of the entire composite of all layers below it in the same comp, revealing them only where the stencil is opaque (or bright).
- **Exact semantics:**
  ```
  Stencil Alpha:  m = as'                       # stencil alpha incl. opacity, masks, effects
  Stencil Luma:   m = Lum601(Cs) * as'          # UNVERIFIED weights and whether as' multiplies
  acc.alpha = acc.alpha * m      # acc straight colour unchanged; premultiplied: acc *= m (all 4 channels)
  # nothing of the stencil layer's colour is added
  ```
  - Affects every layer below it in the same comp (the accumulated buffer), including layers whose own blend mode is Normal; layers above the stencil are composited normally afterwards.
  - To limit the effect to a subset of layers, those layers must be precomposed (Adobe help).
  - Outside the stencil layer's bounds `m = 0` (the composite below becomes transparent there).
  - Preserve Underlying Transparency and track mattes on the stencil layer itself modify `as'` normally (UNVERIFIED).
  - In 8/16 bpc `m` is in [0, 1]; in 32 bpc a luma > 1 is clamped to 1 (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Luma for "Luma" modes: UNVERIFIED — AE track-matte luma is commonly reported as Rec.601 (0.299, 0.587, 0.114); assume the same here. Not in Lottie. Interaction with 3D: a stencil layer in a 3D group — UNVERIFIED (stencil/silhouette modes on 3D layers act on the composite rendered so far in depth order; see compositing-3d-color.md, Classic 3D renderer). Implementation hint: fixed-function blend `src=Zero, dst=SrcAlpha` (after writing `m` to the shader's alpha) on premultiplied buffers.

### AE-CMP-028 · Silhouette Alpha and Silhouette Luma (轮廓 Alpha 和轮廓亮度)

- **What it does:** The inverse of Stencil: the silhouette layer is not drawn and cuts a hole through all layers below it where it is opaque (or bright).
- **Exact semantics:**
  ```
  Silhouette Alpha: m = 1 - as'
  Silhouette Luma:  m = 1 - Lum601(Cs) * as'     # white -> fully transparent hole; black -> no change
  acc.alpha = acc.alpha * m        (premultiplied: acc *= m)
  ```
  - Outside the silhouette layer's bounds `m = 1` (no change).
  - Same scope rules as Stencil (AE-CMP-027): all layers below in the same comp.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** ExtendScript enum is misspelled `BlendingMode.SILHOUETE_ALPHA`. Adobe help: "Painting with pure white creates 0% opacity" (Silhouette Luma). Not in Lottie.

### AE-CMP-029 · Alpha Add (Alpha 添加)

- **What it does:** Composites colour like Normal but adds the alphas instead of combining them multiplicatively, so complementary soft edges (e.g. two layers with inverted feathered mattes, or abutting anti-aliased 3D faces) join without a seam.
- **Exact semantics:**
  ```
  ao  = min(1, ab + as')                     # instead of as' + ab*(1 - as')
  cop = csp + cbp * (1 - as')                # UNVERIFIED: colour as Normal
  Co  = (ao > 0) ? cop / ao : 0
  ```
  - Example from Adobe help: two 50% alpha pixels give 75% with Normal, 100% with Alpha Add.
  - UNVERIFIED: exact colour formula; alternative plausible form `cop = csp + cbp` (pure additive premultiplied) clamped by `ao`. Verify with two complementary gradient-alpha solids of different colours.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** Not in Lottie. Alpha Add affects the alpha of the source result (Adobe help: "The Alpha Add blending mode affects the alpha channel of the source layer").

### AE-CMP-030 · Luminescent Premul (冷光预乘)

- **What it does:** Treats the source as premultiplied light: colour values that exceed the source alpha are not clipped but added to the composite, for compositing lens flares and light effects rendered with premultiplied alpha.
- **Exact semantics:**
  ```
  # UNVERIFIED best understanding:
  csp_lum = Cs (straight colour interpreted as if already premultiplied, i.e. not multiplied by as')
  cop = csp_lum + cbp * (1 - as')
  ao  = as' + ab * (1 - as')
  Co  = (ao > 0) ? cop / ao : 0               # may exceed 1; clamped at 8/16 bpc
  ```
  - Adobe help recommends interpreting such premultiplied footage as Straight when using this mode, which is consistent with the formula: the footage's stored (premultiplied) RGB is used directly as additive light, so colour in pixels with alpha < colour value is not lost.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** Not in Lottie. Golden test: lens-flare footage with premultiplied black edges, interpreted Straight, over a mid-grey solid.

## Opacity and Preserve Underlying Transparency

### AE-CMP-031 · Layer opacity application (图层不透明度的应用（译名待核）)

- **What it does:** The layer's Opacity transform property scales the layer's alpha after masks, effects and transform, before the track matte and blend mode.
- **Exact semantics:**
  ```
  as' = clamp01(as_after_transform * opacity / 100)     # opacity in percent, 0..100
  ```
  - Opacity range 0-100 % (values set by expression outside the range are clamped, UNVERIFIED for > 100).
  - Opacity multiplies alpha only (straight colour unchanged); with premultiplied buffers multiply all four channels.
  - Opacity 0 % layers are still evaluated (expressions, effects referencing them) but contribute nothing; BAE may skip rasterising them unless they are a track matte, a stencil/silhouette source or referenced by an effect layer parameter (UNVERIFIED whether an effect that samples another layer sees that layer's opacity: AE effect layer parameters ignore the referenced layer's transforms and opacity by default — see effects.md, effect framework).
  - Opacity of a track-matte layer scales the matte value (AE-CMP-034); opacity of a stencil/silhouette layer scales `m` (AE-CMP-027/028); opacity of an adjustment layer scales the effect mix (AE-CMP-049 Adjustment layers).
  - Opacity is applied per motion-blur sub-sample (it can be animated within the shutter).
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** The Opacity property itself (keyframes, `ks.o` in Lottie, matchName `ADBE Opacity`) is defined in transform-keyframes-expressions.md, Transform properties. Implementation hint: fold opacity into the final resample/blend shader as a uniform; never allocate a buffer for it.

### AE-CMP-032 · Preserve Underlying Transparency (保留底层透明度)

- **What it does:** With the "T" switch on, the layer is visible only where the composite of the layers below is opaque, and it never changes that composite's alpha (like a Photoshop clipping mask).
- **Exact semantics:**
  ```
  # source-atop with blend mode; ab, cbp from the accumulated composite below
  ao  = ab                                   # alpha of the result = alpha below, unchanged
  cop = as' * ab * B(Cb, Cs) + (1 - as') * cbp
  Co  = (ao > 0) ? cop / ao : 0
  ```
  - "Underlying" means the full accumulated composite of all participating layers below in the same comp (not just the next layer).
  - Over fully transparent areas the layer contributes nothing; partially transparent underlying pixels receive the layer weighted by `ab`.
  - Combined with a track matte: the matte first reduces `as'` (AE-CMP-034), then the atop composite applies.
  - Ignored on adjustment layers (UNVERIFIED) and has no meaning on the bottom layer (result would be empty).
  - UNVERIFIED: exact formula; AE help describes the behaviour qualitatively only ("the layer gets its transparency from the composite of the layers below").
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Switch lives in the Modes column ("T"); scripting `AVLayer.preserveTransparency` (Boolean). No Lottie equivalent (BAE Lottie export: approximate with an alpha track matte only when the underlying composite is a single layer, else warn). Implementation hint: fixed-function blend for Normal mode: `color: src=DstAlpha, dst=OneMinusSrcAlpha; alpha: src=Zero, dst=One` on premultiplied buffers.

## Track mattes

### AE-CMP-033 · Track matte assignment: legacy and AE 23.0 models (轨道遮罩)

- **What it does:** A layer can take its transparency from another layer (the matte) in the same comp; since AE 23.0 (2023) any layer can be chosen as the matte, earlier only the layer directly above.
- **Exact semantics:**
  - **AE 23.0+ model (current):** each AV layer has `trackMatteLayer` (any other AV layer in the same comp, or none) plus two toggles: Alpha / Luma and Normal / Inverted. Chosen via the Track Matte column (in the Modes column area) with a layer list dropdown or pick whip. Many layers may share one matte layer. When a layer is first picked as a matte its Video switch is turned off automatically, but the user may turn it back on and the matte then also renders normally.
  - **Legacy model (AE ≤ 22.x):** the matte is always the layer immediately above (index − 1); choosing a Track Matte mode turns off that layer's Video switch; reordering layers changes or breaks the pairing; one matte serves one layer.
  - Opening a legacy project maps each matted layer to `trackMatteLayer = layer above` with the same mode.
  - Modes (UI order; scripting `TrackMatteType`; Lottie `tt`):

  | Mode (中文) | ExtendScript | Lottie `tt` | Opaque where |
  |---|---|---|---|
  | No Track Matte (没有轨道遮罩（译名待核）) | `NO_TRACK_MATTE` | absent / 0 | — |
  | Alpha Matte (Alpha 遮罩) | `ALPHA` | 1 | matte alpha = 100 % |
  | Alpha Inverted Matte (Alpha 反转遮罩) | `ALPHA_INVERTED` | 2 | matte alpha = 0 % |
  | Luma Matte (亮度遮罩) | `LUMA` | 3 | matte luminance = 100 % |
  | Luma Inverted Matte (亮度反转遮罩) | `LUMA_INVERTED` | 4 | matte luminance = 0 % |

  - Constraints: a layer cannot be its own matte; cycles (A mattes B, B mattes A) are refused (UNVERIFIED exact UI behaviour). Camera, light and audio-only layers cannot be mattes (no pixels). UNVERIFIED: whether adjustment layers can be picked as mattes (if allowed, only their source alpha would count).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-ui-motion, bae-motion)
- **Notes:** Scripting (23.0+): `AVLayer.setTrackMatte(layer, TrackMatteType)`, `AVLayer.removeTrackMatte()` (keeps the type), `AVLayer.trackMatteLayer` (read-only), `hasTrackMatte`, `isTrackMatte`; `trackMatteType` is the legacy read/write API. Lottie: matted layer has `tt` (1-4), matte layer has `td: 1`, and `tp` (matte parent layer index) — when `tp` is absent the matte is the previous layer in the array (the layer above), which is how bodymovin exported pre-23 projects. Parenting the matte to the matted layer makes a matte that moves with it (Adobe help tip). Source: https://helpx.adobe.com/after-effects/using/track-mattes-and-traveling-mattes.html ("Any layer in your composition, no matter where it sits in the layer stack, can be used as a Track Matte source"; multiple layers may reference one matte; the matte's visibility is switched off by default when chosen and can be turned back on).

### AE-CMP-034 · Track matte compositing math (轨道遮罩合成计算（译名待核）)

- **What it does:** Multiplies the matted layer's alpha by a per-pixel matte value derived from the matte layer's rendered image in comp space.
- **Exact semantics:**
  ```
  M = render matte layer at comp resolution (its own masks, effects, transform, opacity,
      layer styles, motion blur, and its own track matte if any), as straight (Cm, am)
      — the matte layer's blend mode and Preserve Transparency are ignored (UNVERIFIED)
  v = Alpha:          am
      Alpha Inverted: 1 - am
      Luma:           Luma(Cm) * am              # transparent matte areas count as black
      Luma Inverted:  1 - Luma(Cm) * am          # transparent matte areas become opaque
  as' = as' * clamp01(v)                         # matted layer, after its opacity
  Luma(C) = 0.299*R + 0.587*G + 0.114*B          # UNVERIFIED weights (Rec.601 assumed)
  ```
  - Order: applied after the matted layer's transform, opacity and layer styles, before its blend mode and Preserve Underlying Transparency (AE-CMP-007).
  - Both layers are evaluated at the same comp time; each uses its own in/out points. Outside the matte layer's in/out range (or when the matte is otherwise empty) `M` is transparent: Alpha/Luma mattes hide the layer completely, inverted mattes show it fully (UNVERIFIED).
  - Pixels outside the matte layer's bounds are transparent (`am = 0`).
  - The matte layer's Video switch does not affect its use as a matte; Solo state of the matte is irrelevant (UNVERIFIED).
  - 32 bpc: luma > 1 is clamped to 1 for the matte value (UNVERIFIED); 8/16 bpc already in range.
  - Linear blending setting: UNVERIFIED whether luma is computed on encoded or linearised values when "Blend Colors Using 1.0 Gamma" is on.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** lottie-web implements Alpha/Luma via SVG masks with `mask-type` alpha/luminance (CSS luminance coefficients 0.2125/0.7154/0.0721 in linearRGB), so lottie-web luma mattes do not match AE exactly. Implementation hint: render the matte layer once per frame into an R16F/R8 "matte value" texture at ROI size, shared by all layers that use it; fuse the multiply into the matted layer's resample/blend shader.

### AE-CMP-035 · Nested, shared and 3D track mattes (嵌套、共享和 3D 轨道遮罩（译名待核）)

- **What it does:** Covers mattes that themselves have mattes, one matte used by several layers, and mattes or matted layers that are 3D.
- **Exact semantics:**
  - Nested mattes: a matte layer may itself have a track matte; it is resolved recursively (matte of matte applied to the matte's alpha before the matte value is derived). UNVERIFIED depth limit; cycles must be rejected at assignment.
  - Shared mattes (23.0+): the matte is rendered once per frame and reused; each user may choose a different mode (Alpha / Luma / inverted).
  - 3D matte layer: the matte is rendered through the comp's active camera (and lights, if it accepts lights) into a flat 2D comp-space image, then used as in AE-CMP-034.
  - 3D matted layer: the matte value multiplies the matted layer's rendered (camera-projected) image in comp space; the matted layer still takes part in its 3D group's depth sort / intersection (UNVERIFIED: AE renders a matted 3D layer as its own 2D image, breaking intersections with other 3D layers, similar to layers with blend modes in the Classic 3D renderer; see compositing-3d-color.md, Classic 3D renderer).
  - Interplay with blend modes: the matted layer's blend mode applies after the matte (matte reduces `as'`, then `B` and AE-CMP-012).
  - Matte layer on an adjustment layer: an adjustment layer can have a track matte, limiting where its effects apply (see Adjustment layers section).
  - Collapsed precomp as matte or matted layer: the precomp is flattened for the matte step (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Legacy model limitation: since a matte had to be directly above, sharing a matte required duplicates or precomps; BAE implements only the 23.0+ model internally and converts legacy pairs on import.

## Masks

### AE-CMP-036 · Mask data model and mask path (蒙版和蒙版路径)

- **What it does:** Each AV layer owns an ordered list of masks; each mask is a Bezier path in the layer's own coordinate space plus mode, inversion, feather, opacity and expansion properties. Closed masks modify the layer's alpha; open masks only serve as path inputs for effects.
- **Exact semantics:**
  - Property tree (top to bottom in the Timeline under Masks (蒙版), each mask named "Mask 1", "Mask 2", ...):

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Mask Path (蒙版路径) | Bezier path (shape) | set when drawn | — | layer pixels | closed or open; animatable; matchName `ADBE Mask Shape` |
  | Mask Feather (蒙版羽化) | 2D [x, y] | [0, 0] | 0 … 32,000 by scrubbing (Adobe help); UNVERIFIED max for typed values | pixels | constrain-proportions link; matchName `ADBE Mask Feather` |
  | Mask Opacity (蒙版不透明度) | scalar | 100 | 0 … 100 | % | matchName `ADBE Mask Opacity` |
  | Mask Expansion (蒙版扩展) | scalar | 0 | −32,000 … 32,000 (scrub range) | pixels | matchName `ADBE Mask Offset` |

  - Attributes (not animatable): mode (default Add; AE-CMP-038), Inverted (反转, default off), Mask Feather Falloff (Smooth default), Motion Blur (Same As Layer default), RotoBezier (off), colour, locked, name.
  - Coordinates: vertices are in layer space (the source's pixel grid, origin top-left of the layer), so masks move, scale and rotate with the layer; to move a mask independently the user animates the path (or uses a separate layer as a track matte).
  - Closed vs open: a path with `closed = true` affects alpha (unless mode None). An open path never affects alpha and has no mode; effects such as Stroke, Path Text, Audio Waveform, Audio Spectrum and Vegas accept open or closed masks; Fill, Smear, Reshape, Particle Playground and Inner/Outer Key need closed masks (Adobe help).
  - Empty mask list, or all masks None/open → layer alpha unchanged.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-vector)
- **Notes:** Scripting: `layer.mask` (`ADBE Mask Parade`) → `MaskPropertyGroup` (`ADBE Mask Atom`) with `maskMode`, `inverted`, `rotoBezier`, `maskMotionBlur`, `maskFeatherFalloff`, `locked`, `color`. Expression access: `mask("Mask 1").maskPath / maskFeather / maskOpacity / maskExpansion / invert` (see transform-keyframes-expressions.md, Expressions mask object). Lottie: layer `hasMask`, `masksProperties[]` with `mode` (see AE-CMP-038), `inv` (bool), `pt` (animated Bezier), `o` (opacity %), `x` (expansion px), `nm`; the official Lottie schema (lottie-docs, Mask) lists only `mode`, `o`, `pt`, `x`, `inv` (+ `nm`); feather is a de-facto extension: Skottie reads a 2D `f` (`modules/skottie/src/Layer.cpp`), lottie-web ignores it. Bezier path model and editing: shapes.md, Bezier model; drawing tools: ui-preview-render-io.md, tools.

### AE-CMP-037 · Mask display attributes: colour, lock, name (蒙版颜色、锁定和名称（译名待核）)

- **What it does:** Per-mask UI data that does not affect rendering: outline colour in viewers and Timeline, lock (prevents selection/editing), and name.
- **Exact semantics:**
  - Colour: RGB in [0, 1]; default yellow for every new mask unless the Appearance preference "Cycle Mask Colors" is on (then it cycles through label colours); "Use Contrasting Color For Mask Path" picks a colour that contrasts with the pixels where drawing starts and differs from the previous mask.
  - Locked: boolean; locked masks cannot be selected or edited in viewers; still render.
  - Name: string, default "Mask N" (N = index at creation), renamable, unique not required (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `MaskPropertyGroup.color` (array of 3 floats), `.locked`. Preferences: ui-preview-render-io.md, Preferences (Appearance).

### AE-CMP-038 · Mask modes and accumulation (蒙版模式)

- **What it does:** Combines the coverage of all closed masks on a layer, top to bottom, into one alpha multiplier using per-mask modes.
- **Exact semantics:**
  - Modes in menu order (zh-CN; ExtendScript `MaskMode.`; Lottie `mode`; drawing shortcut key): None (无; `NONE`; `"n"`; N), Add (相加; `ADD`; `"a"`; A), Subtract (相减; `SUBTRACT`; `"s"`; S), Intersect (相交; `INTERSECT`; `"i"`; I), Lighten (变亮; `LIGHTEN`; `"l"`; L), Darken (变暗; `DARKEN`; `"d"`; D), Difference (差值; `DIFFERENCE`; `"f"`; F). Default Add. Not animatable.
  ```
  # m_i = per-mask value in [0,1] = Inverted? / feather / expansion / opacity applied (AE-CMP-039..043)
  active = [mask for mask in masks (top to bottom) if mask.closed and mask.mode != None]
  if active is empty: M = 1
  else:
    first = active[0]
    acc = (first.mode in {Subtract, Intersect, Darken}) ? 1 : 0      # Darken: UNVERIFIED
    for mask in active:
      m = mask_value(mask)
      Add:        acc = acc + m - acc*m            # union (source-over); UNVERIFIED vs min(1, acc+m)
      Subtract:   acc = acc * (1 - m)
      Intersect:  acc = acc * m
      Lighten:    acc = max(acc, m)
      Darken:     acc = min(acc, m)
      Difference: acc = acc + m - 2*acc*m          # XOR-like; UNVERIFIED vs |acc - m|
    M = acc
  layer.alpha = layer.alpha * M                     # masks stage of AE-CMP-007
  ```
  - With a single Subtract mask the layer is visible everywhere except inside the mask (because the start value is 1); a single Intersect mask behaves like Add.
  - Mask modes operate only between masks of the same layer; they never touch other layers.
  - The first mask "interacts with the alpha channel of the layer" (Adobe help): `M` multiplies the source alpha, it does not replace it, so a mask cannot make transparent source pixels opaque.
  - Drawing/editing shortcuts that set the mode of the closed mask being drawn or modified (Adobe help, Mask modes): N None, A Add, S Subtract, I Intersect, L Lighten, D Darken, F Difference.
  - Coverage outside the layer's bounds does not exist (masks are evaluated inside the layer frame only).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model, bae-vector)
- **Notes:** Scripting enum order as documented: `MaskMode.NONE, ADD, SUBTRACT, INTERSECT, LIGHTEN, DARKEN, DIFFERENCE`. Source: https://helpx.adobe.com/after-effects/using/alpha-channels-masks-mattes.html (Mask modes). Adobe help's Intersect text says non-overlapping areas become "complete opacity"; actual AE behaviour (and lottie-web/Skottie) is complete transparency outside the intersection — the help text is wrong. Reference implementations: Skottie maps Add→SrcOver, Intersect→SrcIn, Subtract→DstOut (first Subtract = inverted geometry), Difference→Xor and does not support Lighten/Darken; lottie-web starts from a full white rect when the first mask is Subtract or Intersect. Golden tests needed for the partial-coverage formulas (overlap of two 50 % opacity masks in each mode). Implementation hint: rasterise each mask's coverage into an R16F texture at layer resolution (or ROI), accumulate in one compute pass over all masks, multiply into alpha in the same pass.

### AE-CMP-039 · Mask Inverted (反转)

- **What it does:** Swaps inside and outside of one mask before it is combined with the other masks.
- **Exact semantics:**
  ```
  cov = antialiased coverage of the (expanded, feathered) closed path in [0,1]
  m   = (inverted ? 1 - cov : cov) * maskOpacity/100      # UNVERIFIED: opacity after inversion
  ```
  - Inverted applies to the expanded and feathered coverage (so the feather ramp is mirrored as well).
  - Shortcut Ctrl/Cmd+Shift+I inverts selected masks (see ui-preview-render-io.md, shortcuts).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** Scripting `MaskPropertyGroup.inverted`; expression `mask(...).invert`; Lottie `inv`. lottie-web implements inversion by adding a comp-sized rectangle to the path (even-odd), Skottie by inverse fill type — both equivalent for coverage.

### AE-CMP-040 · Mask Opacity (蒙版不透明度)

- **What it does:** Scales the influence of one closed mask on the layer's alpha.
- **Exact semantics:**
  - `m = coverage' * opacity / 100`, where `coverage'` is after expansion, feather and inversion (AE-CMP-039).
  - Mask Opacity 100 % → interior fully opaque; outside of a (non-inverted) mask is always transparent regardless of opacity (Adobe help).
  - Range 0-100 %; 0 % makes an Add mask contribute nothing (and a lone Add mask with 0 % hides the layer completely).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Lottie mask `o` (0-100). Shortcut TT shows Mask Opacity (ui shortcuts).

### AE-CMP-041 · Mask Feather (蒙版羽化)

- **What it does:** Softens the mask edge with a Gaussian-like falloff, separately in x and y, straddling the path half inside and half outside.
- **Exact semantics:**
  - Value `[fx, fy]` in layer pixels; both ≥ 0. Feather width `f` is centred on the (expanded) edge: e.g. f = 25 → ~12.5 px inside and ~12.5 px outside (Adobe help).
  - Profile: opacity varies "according to a Gaussian distribution", so the influence extends slightly beyond the nominal width (Adobe help). Best implementation: blur the hard (anti-aliased) coverage with a separable Gaussian, `sigma_x = k * fx`, `sigma_y = k * fy`. UNVERIFIED `k` for AE: Skottie uses `k = 0.38` (`kFeatherToSigma` in `modules/skottie/src/Layer.cpp`, its empirical match to AE); a pure "99 % within f/2" Gaussian would be `k ≈ 0.19`. BAE default: `k = 0.38` until golden-tested.
  - Feathering is computed only within the layer's dimensions: coverage outside the layer frame is not available, so a feather that reaches the layer edge is cut abruptly (Adobe help). The blur uses transparent (decal) borders, not edge clamping.
  - Feather is in layer space, so it scales with the layer's Scale when the layer is transformed afterwards (raster pipeline); downsampled previews scale it by the resolution factor (AE-CMP-062 Resolution downsampling).
  - 0 feather → hard anti-aliased edge (Best quality) or aliased (Draft, UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Shortcut F reveals Mask Feather. Lottie: not in the official mask schema; Skottie reads `f`. Implementation hint: separable two-pass Gaussian on the R16F coverage texture with kernel radius `3*sigma`; for large feathers downsample (mip) first — the result is smooth, so a 1/4-resolution blur and bilinear upsample is visually identical. Feather Falloff and variable-width feathers modify this profile (AE-CMP-042, AE-CMP-043).

### AE-CMP-042 · Mask Feather Falloff (羽化衰减)

- **What it does:** Chooses the shape of the feather ramp: Smooth (default) or Linear.
- **Exact semantics:**
  - Values in menu order (Layer > Mask > Feather Falloff / 图层 > 蒙版 > 羽化衰减; scripting `MaskPropertyGroup.maskFeatherFalloff`, `MaskFeatherFalloff.`): Smooth (平滑, `FFO_SMOOTH`, default), Linear (线性, `FFO_LINEAR`).
  - Smooth: Gaussian / smoothstep-like ramp (AE-CMP-041). Linear: alpha changes linearly with distance across the feather band from the inner to the outer feather boundary. UNVERIFIED exact Smooth curve when variable-width feather points exist (likely smoothstep over the band rather than a true Gaussian blur).
  - Per-mask attribute, not animatable.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Introduced with variable-width mask feather in AE CS6 (11.0). Implementation hint: when Falloff = Linear or feather points exist, compute coverage from a signed-distance field of the path (distance to inner/outer boundary) instead of blurring.

### AE-CMP-043 · Variable-width mask feather points (可变宽度蒙版羽化)

- **What it does:** Feather points placed along a closed mask path with the Mask Feather tool define a feather band whose width varies along the path, inside or outside.
- **Exact semantics:**
  - Each feather point: position along the path (segment index + parameter), radius (signed: outward positive, inward negative — UNVERIFIED sign), tension (curvature of the boundary through the point, 0-100 %, UNVERIFIED range), corner angle, and Hold flag (constant radius up to the next feather point).
  - Feather points define the outer and inner feather boundaries; if no feather point is inside the mask, the inner boundary is the mask path itself (Adobe help). The feather ramps from the inner to the outer boundary (shape per Feather Falloff).
  - Feather points are stored with the Mask Path keyframes; counts are kept constant across keyframes when the preference "Preserve Constant Vertex and Feather Point Count when Editing Masks" is on (Preferences > Composition in current AE; General in older versions).
  - Combined with the global Mask Feather value: UNVERIFIED (assume the global Gaussian feather is applied on top).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model, bae-ui-motion)
- **Notes:** SDK: `AEGP_MaskFeatherSuite` (feather points with `radiusF`, negative allowed for inner feathers, `AEGP_MaskFeatherType_INNER/OUTER`, interpolation, tension, corner angle). Not in Lottie. Tool UI in ui-preview-render-io.md, tools (Mask Feather tool, G toggles Pen/Mask Feather).

### AE-CMP-044 · Mask Expansion (蒙版扩展)

- **What it does:** Grows (positive) or shrinks (negative) the area a mask influences, as an offset of its coverage in pixels, without changing the mask path.
- **Exact semantics:**
  - `e` in layer pixels; `e > 0` expands outward, `e < 0` contracts; scrub range −32,000 … 32,000 (Adobe help; earlier versions scrubbed only to ±1,000, larger values had to be typed).
  - Applied before feather: the feather is centred on the expanded edge.
  - Offset geometry: UNVERIFIED — expansion behaves like a morphological dilation/erosion of the filled path (rounded outer corners on expansion, sharp inner corners), not a miter offset of the path.
  - Inverted masks: expansion is applied to the path's interior first, then inverted (UNVERIFIED).
  - Large negative values may remove the mask area completely (coverage 0 → for Add mode the layer becomes invisible).
- **Priority:** P0
- **Owner:** bae-render (+ bae-vector)
- **Notes:** Lottie `x` (pixels); lottie-web emulates positive expansion with a stroke of width `2x` and negative with an SVG `feMorphology erode` of radius `-x`. Implementation hint: compute from a signed distance field: `cov = smoothstep(-0.5, 0.5, -sd(p) + e)`; one SDF serves expansion, feather and linear falloff.

### AE-CMP-045 · Mask rasterisation and anti-aliasing (蒙版栅格化和消除锯齿（译名待核）)

- **What it does:** Converts each closed mask path into a per-pixel coverage value.
- **Exact semantics:**
  - Coverage is the area fraction of the pixel inside the path (anti-aliased) in Best quality; in Draft quality UNVERIFIED (AE still anti-aliases masks in Draft but with lower precision).
  - Fill rule for self-intersecting mask paths: UNVERIFIED (nonzero winding assumed — Skottie and lottie-web use nonzero for masks).
  - Coverage is computed in layer space at the layer's render resolution (source pixels × preview resolution factor); for continuously rasterized and collapsed layers it is computed in comp space with the layer transform applied (AE-CMP-008), so mask edges stay sharp when scaled up.
  - Paths are flattened adaptively so that the error is below ~0.1 px at render resolution (implementation requirement, not AE-documented).
- **Priority:** P0
- **Owner:** bae-render (+ bae-vector)
- **Notes:** Implementation hint: use Vello's path coverage (or kurbo flattening + analytic coverage compute shader) into an R16F mask atlas; share the code path with shape-layer fills.

### AE-CMP-046 · Mask motion blur (蒙版运动模糊)

- **What it does:** Controls whether a mask's own path animation is motion-blurred.
- **Exact semantics:**
  - Values (scripting `MaskMotionBlur`): Same As Layer (与图层相同, `SAME_AS_LAYER`, default), On (打开（译名待核）, `ON`), Off (关闭（译名待核）, `OFF`).
  - Same As Layer: the mask is evaluated at each motion-blur sub-sample time when the layer's Motion Blur switch and the comp's Motion Blur enable are on.
  - On: mask path is sampled at the sub-sample times even if the layer's Motion Blur switch is off (Adobe help: "regardless of the setting of the Enable Motion Blur switch for the layer"); the comp-level Enable Motion Blur / Render Settings motion blur still gate it (UNVERIFIED for the comp-level gate).
  - Off: mask evaluated at the frame time only, while the layer's transform is still blurred.
  - Sub-sample times and weighting come from the comp's Shutter Angle / Phase / Samples Per Frame (see transform-keyframes-expressions.md, motion-blur sampling; AE-CMP-064 Motion blur rendering).
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Layer > Mask > Motion Blur menu; scripting `MaskPropertyGroup.maskMotionBlur`. Source: https://helpx.adobe.com/after-effects/using/assorted-animation-tools.html (Motion blur). Implementation hint: for raster layers the masked source is rendered per sub-sample only when the mask path actually changes within the shutter interval; otherwise reuse one masked source for all sub-samples.

### AE-CMP-047 · RotoBezier masks (RotoBezier 蒙版（译名待核）)

- **What it does:** A mask whose curve tangents are computed automatically from vertex positions and per-vertex tension, instead of manual Bezier handles.
- **Exact semantics:**
  - Boolean per mask (`MaskPropertyGroup.rotoBezier`); converting toggles between manual Bezier and RotoBezier (Layer > Mask and Shape Path > RotoBezier).
  - Each vertex has a tension (adjusted with the Convert Vertex tool); the renderer receives ordinary cubic Beziers computed from vertices + tensions. UNVERIFIED: exact tangent formula (Catmull-Rom-like with tension scaling).
  - Rendering of a RotoBezier mask is identical to rendering the derived Bezier path.
- **Priority:** P2
- **Owner:** bae-vector (+ bae-model)
- **Notes:** Also exists for shape-layer paths; shapes.md, Bezier model. Lottie exports the derived Bezier.

### AE-CMP-048 · Masks on special layer types (特殊图层上的蒙版（译名待核）)

- **What it does:** Defines what masks do on adjustment, text, shape, precomp and 3D layers.
- **Exact semantics:**
  - Adjustment layer: the mask result limits where the adjustment layer's effects are applied (mix factor, see Adjustment layers section).
  - Text and shape layers: masks clip the rendered vector content; mask paths are in layer space; evaluated in the continuously rasterized order (masks → transform → effects, AE-CMP-008). Text-on-path and shape effects can use the masks as path input.
  - Solid layers: a mask on a solid is the standard way to draw a soft shape. Solids are raster layers, so the mask is rasterised at the solid's pixel size and its edge softens when the solid is scaled above 100 % (UNVERIFIED: AE may render solids resolution-independently).
  - Precomp layers: masks apply to the nested comp's rendered frame; with Collapse Transformations on, a closed mask forces the nested layers to be flattened first (Collapse Transformations section).
  - 3D layers: masks are in the layer's own plane and are projected with it.
  - Camera, light and audio-only layers cannot have masks (no pixels). Null objects: UNVERIFIED whether AE allows masks on them; they would have no visible effect (a null is never rendered).
- **Priority:** P0
- **Owner:** bae-render (+ bae-text, bae-vector)
- **Notes:** Effects that read a mask path (Stroke, Fill, Path Text, Vegas...) are specified in effects.md; text "Path Options" in text.md.

## Adjustment layers

### AE-CMP-049 · Adjustment layer rendering (调整图层)

- **What it does:** An adjustment layer applies its effect stack to the composite of all layers below it in the same comp; its own alpha (source, masks, transform, opacity, track matte) defines where and how strongly the effects apply.
- **Exact semantics:**
  ```
  apply_adjustment(L, acc, t):                 # acc = premultiplied composite of layers below
    if !L.video_enabled or no enabled effects: return acc        # no visible result
    E = run L.effects top-to-bottom on acc (comp-sized input, comp coordinates)
    A = alpha of L's own layer pipeline:  source alpha (AE-created adjustment = opaque comp-size solid)
        * masks (AE-CMP-038) -> transform (incl. motion blur) -> * opacity/100 -> * track matte value
    if L.blend_mode == Normal:
        out = lerp(acc, E, A)                  # premultiplied, all four channels
    else:
        out = lerp(acc, blend(src = E, dst = acc, L.blend_mode), A)   # AE-CMP-012; UNVERIFIED
    return out
  ```
  - Effects can change alpha: e.g. Linear Wipe on an adjustment layer makes the layers below transparent where it wipes (follows from the lerp).
  - An adjustment layer at the bottom of the stack (nothing below) has no visible result; layers above it are unaffected.
  - Opacity acts as an effect-strength control (`A` scales the mix); Preserve Underlying Transparency on an adjustment layer: UNVERIFIED (no visible meaning).
  - Effects' spatial parameters (points, layer-space coordinates) are interpreted in the adjustment layer's layer space, which for an untransformed comp-sized adjustment solid equals comp space (UNVERIFIED for transformed adjustment layers).
  - Any AV layer can be switched to an adjustment layer (Adjustment Layer switch); its pixel colours are then ignored and only its alpha is used as `A`. Switching it off makes it a normal layer again.
  - Layer > New > Adjustment Layer (Ctrl+Alt+Y / Cmd+Option+Y) creates a white comp-size solid named "Adjustment Layer N" with the switch on (solid stored in the Solids folder).
  - Blend modes on adjustment layers are honoured (classic "glow" trick: adjustment layer with a blur set to Screen/Add).
- **Priority:** P0
- **Owner:** bae-render (+ bae-effects, bae-model)
- **Notes:** Scripting `AVLayer.adjustmentLayer` (Boolean). Not representable in Lottie (BAE export: precompose the layers below and apply the effects to the precomp, or warn). Implementation hint: when `A` is a full-frame constant 1 (common case), skip the lerp and pass the effect output through; the ROI of `E` can be limited to the bounding box of `A`.

### AE-CMP-050 · Adjustment layers with 3D, precomps and mattes (调整图层与 3D、预合成和遮罩的交互（译名待核）)

- **What it does:** Specifies the boundary cases of adjustment layers.
- **Exact semantics:**
  - 3D adjustment layer: its footprint `A` is the layer's projected (camera-transformed) alpha, but the effects still process the 2D composite of everything below it in stacking order. UNVERIFIED: whether 3D layers in front of the adjustment layer in depth but below it in stacking order are adjusted (stacking order wins in AE's Classic 3D: adjustment layers act on the stacking-order composite). An adjustment layer splits 3D layers above and below it into separate 3D groups (no intersections or shadows across it) — see compositing-3d-color.md, Classic 3D renderer.
  - Inside a precomp: it affects only layers below it inside that precomp. If that precomp layer has Collapse Transformations on, the adjustment layer forces the nested layers to be flattened and cropped (Adobe help: an adjustment layer "will force the flattening and cropping that collapsing transformations would usually prevent").
  - Track matte on an adjustment layer: limits the adjustment to the matte (multiplies `A`). Using an adjustment layer as a matte source for another layer: UNVERIFIED (its source alpha is used).
  - Masks on an adjustment layer: the standard way to restrict effects to a region (Adobe help); animated masks may follow subjects.
  - Motion blur on an adjustment layer blurs only its footprint `A`, not the content below (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** —

## Collapse Transformations and Continuous Rasterization

### AE-CMP-051 · Collapse Transformations for precomp layers (折叠变换)

- **What it does:** With the switch on, a precomp layer is not flattened into a comp-sized image: its nested layers are transformed straight into the containing comp (transforms concatenated), so resolution is preserved, content outside the nested comp frame is kept, nested 3D layers join the outer 3D scene, and nested blend modes act on the outer comp's layers.
- **Exact semantics:**
  ```
  render_collapsed(P, acc, t):                  # P = precomp layer with Collapse on
    if the nested comp contains an (active) adjustment layer:
        # Adobe help: the adjustment layer "will force the flattening and cropping that
        # collapsing transformations would usually prevent" -> behave as not collapsed
        return composite(nested_frame(P, t) transformed by P (AE-CMP-054), acc, ...)
    if P has (a closed mask with mode != None) or (an enabled effect) or (a layer style):
        # forced rasterisation ("partial collapse"; Adobe help: nested layers "are first
        # rendered on their own, then masks and effects are applied")
        img = render nested layers with transforms = outer(P) ∘ nested(layer), into a
              comp-space buffer of the OUTER comp size (UNVERIFIED: not cropped to the
              nested frame; transforms still concatenated, so resolution is preserved)
        img = apply P's masks (mapped to comp space), effects, layer styles
        acc = composite(img, acc, P.blend_mode, P.preserve_transparency)
        # nested blend modes affect only nested layers; 3D layers cannot intersect
        # or shadow across this boundary
    else:
        for each nested layer N bottom-to-top:
            N.transform = outer_transform(P) ∘ N.transform            # concatenated
            N.opacity  *= P.opacity/100                                # UNVERIFIED: per layer
            composite N directly onto acc with N's own blend mode      # nested modes pass through
        nested 3D layers are depth-sorted/intersected with the outer 3D group and use the
        outer comp's active camera and lights (nested cameras/lights ignored, UNVERIFIED)
  ```
  - Nested comp background colour is never rendered (always transparent).
  - The precomp layer's own blend mode: in the full-collapse case the nested layers keep their own modes and P's mode is ignored (UNVERIFIED; in practice AE greys out / ignores P's mode when collapsed without masks/effects).
  - Without the switch: the nested comp is rendered to an image of the nested comp's size (cropped at its frame), using the nested comp's own camera (or default camera) for its 3D layers, and treated as a 2D or 3D footage layer.
  - Quality switch and motion blur of P propagate to the nested layers as per "Switches Affect Nested Comps" (AE-CMP-057).
  - Collapse Transformations is also the Continuously Rasterize switch position for vector layers (AE-CMP-052) — same switch column, different icon/meaning.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Scripting `AVLayer.collapseTransformation` (Boolean; for vector layers it is the Continuously Rasterize state). Adobe help (Precomposing > Collapsing transformations) is the source of the 3D, cropping, adjustment-layer and forced-rasterisation rules. Lottie: precomp layers are always "collapsed" in players (no cropping by `w`/`h` unless the player clips; lottie-web clips precomps to `w`×`h`) — UNVERIFIED cross-player. Cost: full collapse avoids an intermediate buffer (cheaper and sharper) but prevents caching the nested comp as one image. Implementation hint: represent collapsed precomps in the render graph as an inlined sub-graph with a pushed transform; forced rasterisation inserts an offscreen node in comp space.

### AE-CMP-052 · Continuously Rasterize for vector layers (连续栅格化)

- **What it does:** For vector footage layers (Illustrator AI, PDF, EPS) the switch makes AE re-rasterise the vectors every frame at their transformed size so they stay sharp at any scale; shape and text layers are always continuously rasterized.
- **Exact semantics:**
  - Off (default for vector footage): the vector file is rasterised once at its document size (100 %) into a bitmap, then follows the raster pipeline (AE-CMP-007); scaling above 100 % blurs/pixelates.
  - On: render order becomes masks → transformations → effects (AE-CMP-008); the vector is rasterised in comp space with the full layer transform at the current preview resolution.
  - Effects on a continuously rasterized layer receive a comp-space raster, so their results differ from the switch-off case (e.g. a Drop Shadow's distance is in comp pixels and does not scale with the layer).
  - Best quality anti-aliases vector edges whether or not continuous rasterization is on (Adobe help, Layers > Continuous rasterization); Draft renders without anti-aliasing (Adobe help, Layer image quality: Draft "displays and renders a layer without anti-aliasing and subpixel positioning").
  - Shape and text layers: switch shown but always on; scripting state UNVERIFIED for these layer types.
  - Cost: re-rasterisation each frame (slower but sharper).
- **Priority:** P0 for shape/text (always on); P1 for AI/PDF/EPS footage switch.
- **Owner:** bae-render (+ bae-vector, bae-media)
- **Notes:** BAE decision: vector footage import (AI/PDF/EPS) is a bae-media concern (see project-comp-layers.md, Import); BAE converts it to a Vello scene so both switch states can be honoured. Implementation hint: when off, cache the 100 % raster once per source; when on, render Vello each frame into the ROI.

### AE-CMP-053 · Collapsed or continuously rasterized layers: bounds, 3D and nested-switch effects (折叠/连续栅格化图层的边界与 3D 交互（译名待核）)

- **What it does:** Summarises the side effects users rely on when the switch is on.
- **Exact semantics:**
  - Bounds: collapsed/continuously rasterized content is not cropped to the nested comp frame or vector artboard; layer bounds become the transformed content bounds, clipped only by the outer comp's render region (AE-CMP-063 Layer bounds).
  - 3D: the precomp layer must itself have its 3D switch on for its collapsed nested 3D layers to join the outer 3D scene (outer camera, lights, shadows, intersections). UNVERIFIED: with the precomp layer 2D, AE renders the nested 3D layers through the outer comp's camera but composites them as a closed 2D group at the precomp layer's stacking position.
  - The 3D switch of a continuously rasterized vector layer works as for any layer.
  - Motion blur, quality and frame blending switches on a collapsed layer: propagate into the nested layers (AE-CMP-057 Switches Affect Nested Comps).
  - Track matte with a collapsed precomp (either side): forces flattening (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** —

## Nested compositions

### AE-CMP-054 · Nested composition rendering (嵌套合成的渲染（译名待核）)

- **What it does:** A precomp layer renders its source comp at the mapped time into an image the size of the nested comp, then treats it like footage in the outer comp (unless collapsed, AE-CMP-051).
- **Exact semantics:**
  ```
  nested_frame(P, t_outer):
    t_n = layer_time(P, t_outer)            # start time, stretch, time remap
                                            # (see transform-keyframes-expressions.md, Time remapping)
    if nested.preserve_frame_rate: t_n = floor_to_frame(t_n, nested.frame_rate)   # AE-CMP-056
    res = nested.preserve_resolution ? nested.own_resolution : outer render resolution factor (AE-CMP-055)
    img = render_comp(nested, t_n) at nested width × height × res     # AE-CMP-006
    # background transparent; content outside the nested frame is cropped
  ```
  - Nested comps are rendered before the outer layers that use them (Adobe help, render order).
  - Image size = nested comp frame size (scaled by the resolution factor); layer bounds of the precomp layer = that rectangle (AE-CMP-063 Layer bounds).
  - Nested comp's pixel aspect ratio: the image is treated as footage with the nested comp's PAR and resampled to the outer comp's PAR (UNVERIFIED exact rule; see project-comp-layers.md, Composition Settings).
  - Nested comp's 3D layers: rendered with the nested comp's active camera (or the default camera) into a flat image, unless collapsed.
  - Guide layers inside the nested comp are excluded (AE-CMP-006).
  - Recursion: nesting depth is unbounded; a comp cannot contain itself directly or indirectly (AE refuses the operation).
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Lottie: precomp layer `ty: 0`, `refId` → `assets[].layers`, `w`/`h` (clip size), `tm` (time remap, seconds); `sr` stretch, `st` start time. Implementation hint: a nested comp is a sub-graph rendered into a cached texture keyed by (comp id, nested time, resolution, switch-propagation state, content hash) — see AE-CMP-058.

### AE-CMP-055 · Preserve Resolution When Nested (嵌套时保留分辨率)

- **What it does:** By default a nested comp renders at the containing comp's resolution factor (Full, Half, Third, Quarter, Custom); with this Composition Settings > Advanced option it keeps its own resolution setting.
- **Exact semantics:**
  - Off (default): nested resolution factor := factor of the comp being rendered/previewed (propagates recursively down the nesting chain).
  - On: the nested comp renders at its own Resolution setting (e.g. a comp set to Quarter stays Quarter even when the outer comp is Full); the result is upsampled into the outer comp.
  - Not inherited by new comps (Adobe help: new comps do not inherit Preserve frame rate / resolution settings from the previous Composition Settings).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Scripting `CompItem.preserveNestedResolution`. Resolution factor semantics: AE-CMP-062 (Resolution downsampling) and compositing-3d-color.md, Preview resolution.

### AE-CMP-056 · Preserve Frame Rate When Nested or in Render Queue (嵌套时或在渲染队列中保留帧速率)

- **What it does:** Makes a nested comp (or a comp rendered in the Render Queue with a different frame rate) sample its animation only at its own frame times, instead of at the outer comp's times.
- **Exact semantics:**
  - Off (default): the nested comp is evaluated at the exact nested time `t_n` derived from the outer frame (animation is resampled at the outer frame rate; e.g. a 12 fps comp nested in 24 fps shows 24 distinct positions per second).
  - On: `t_n` is quantised down to the nested comp's frame grid: `t_n' = floor(t_n * fps_n + eps) / fps_n` (UNVERIFIED rounding: floor vs nearest); same when the comp is rendered with a Render Settings frame-rate override.
  - Footage frames inside the nested comp are always chosen by the footage's own frame rate regardless of this switch.
  - Recommended by Adobe instead of Posterize Time for intentionally choppy animation.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-render, bae-model)
- **Notes:** Scripting `CompItem.preserveNestedFrameRate`. SDK: with PFR on, effects' `current_time` is restricted to multiples of the time step. Implementation hint: in flicks the quantisation is exact integer division (705,600,000 / fps for integer rates; NTSC rates use the comp's flick frame duration).

### AE-CMP-057 · Switches Affect Nested Comps (开关影响嵌套合成)

- **What it does:** A preference that makes the Quality, Effects (fx), Frame Blending and Motion Blur switches of a precomp layer propagate recursively into the layers of the nested comp.
- **Exact semantics:**
  - Preference location: Preferences > Composition in current AE (Adobe help, Precomposing and Preferences pages: "Quality, effects, frame blending, and motion blur switches affect nested compositions recursively"); older help pages place it under General / Appearance. Default on (UNVERIFIED: default not stated by Adobe; long-standing AE default is checked).
  - When on, for every layer N rendered inside a nested comp reached through precomp layer P (recursively):
    - Quality: `N.quality = min(N.quality, P.quality)` in order Wireframe < Draft < Best (UNVERIFIED: P = Draft forces nested layers to Draft; P = Best leaves them as set).
    - Effects switch: P's fx switch off disables effects on nested layers (the preference text lists "effects" among the propagated switches; exact rule UNVERIFIED: assumed `N.effects_on = N.effects_on && P.effects_on`).
    - Motion Blur: nested comps obey the containing comp's Enable Motion Blur comp switch (Adobe help, Motion blur: "nested compositions obey the setting for the compositions in which they're contained"); nested layers blur only if their own MB switch is on and P's MB switch is on (UNVERIFIED for the layer-switch AND); shutter settings come from the outer (rendering) comp (UNVERIFIED).
    - Frame Blending: frame blending cannot be applied to a precomp layer itself (Adobe help, Frame blending), so propagation concerns the comp-level Enable Frame Blending switch: nested footage layers frame-blend only if the containing (rendering) comp's switch is on (UNVERIFIED by analogy with motion blur).
  - When off, nested layers use only their own switches and the nested comp's own comp switches.
  - Draft 3D and comp-level switches: UNVERIFIED propagation.
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Preferences dialog: ui-preview-render-io.md, Preferences. Golden test: nested comp with a fast-moving MB-enabled layer, toggle the precomp layer's MB switch with the preference on/off.

### AE-CMP-058 · Nested comp frame caching (嵌套合成帧缓存（译名待核）)

- **What it does:** A nested comp frame that is needed several times for the same outer frame (or several outer frames mapping to the same nested time) is rendered once and reused.
- **Exact semantics:**
  - Cache key: (nested comp id, nested time `t_n` in flicks, resolution factor, propagated switch state (quality, MB, frame blending, fx), downstream ROI, project bit depth/colour settings, content version of the nested sub-tree).
  - Reuse cases: the same precomp used by several layers at the same time; held frames (time remap holds, PFR quantisation, stretch < 1 producing repeated nested frames); motion-blur sub-samples of the precomp layer's own transform (the nested content is rendered once when only the outer transform moves within the shutter — UNVERIFIED that AE does this; AE re-renders nested comps per sub-sample when the nested content itself animates).
  - Invalidation: any change inside the nested sub-tree (via StageEvents) invalidates entries with that comp id.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** BAE decision: cache keys per ARCHITECTURE 5 are (Prim, time, parameter hash); the nested comp's parameter hash includes the propagated switches. Implementation hint: GPU texture cache with LRU eviction under a VRAM budget; RAM-preview cache sits on top (see compositing-3d-color.md, Preview).

## Quality and sampling

### AE-CMP-059 · Layer quality: Best, Draft, Wireframe (图层品质：最佳、草图、线框)

- **What it does:** A per-layer switch that trades rendering precision for speed, both in previews and final renders.
- **Exact semantics:**

  | Value (中文) | ExtendScript `LayerQuality.` | Rendering |
  |---|---|---|
  | Best (最佳) | `BEST` (default) | subpixel positioning, anti-aliasing, 3D shading, effects at full precision (`PF_Quality_HI`); sampling per AE-CMP-060 |
  | Draft (草图) | `DRAFT` | no anti-aliasing, no subpixel positioning (positions snapped to whole pixels), nearest-neighbour resampling, effects told `PF_Quality_LO` and may compute approximately |
  | Wireframe (线框) | `WIREFRAME` | layer drawn only as its bounding-box outline; contents not rendered |

  - Clicking the Quality switch cycles Best (Bilinear) → Draft → Best (Bicubic) (Adobe help); Layer > Quality offers all options including Wireframe.
  - Duplicated or split layers keep the setting. Quality also affects the precision of the layer when used as a control layer for compound effects and in motion tracking.
  - Draft nearest-neighbour: UNVERIFIED exact rounding (pixel centre lands on `floor(x + 0.5)`).
  - Wireframe in final render: UNVERIFIED (AE renders the box outline in the layer's label colour? Best understanding: wireframe layers render nothing in final output).
  - Propagation into nested comps: AE-CMP-057.
- **Priority:** P0 (Best and Draft); Wireframe P2.
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Scripting `AVLayer.quality`. Render Settings can override all layers' quality (Current Settings / Best / Draft / Wireframe; ui-preview-render-io.md, Render Queue). Lottie has no quality concept. Implementation hint: Draft = `textureSample` with a nearest sampler and integer-snapped translation; effect shaders get a `quality` uniform so they can drop sample counts.

### AE-CMP-060 · Sampling: Bilinear and Bicubic (双线性和双立方采样)

- **What it does:** For Best-quality layers, chooses the resampling filter used for the layer transform (scaling, rotation, subpixel translation) and the Transform effect.
- **Exact semantics:**
  - Values (scripting `AVLayer.samplingQuality`, `LayerSamplingQuality.`): Bilinear (双线性, `BILINEAR`, default), Bicubic (双立方, `BICUBIC`). Only meaningful when quality = Best. Shortcuts Alt/Option+B (Best/Bilinear), Alt/Option+Shift+B (Best/Bicubic).
  - Bilinear: standard 2×2 tap interpolation on premultiplied (alpha-weighted) colour; for downscaling AE uses area sampling / mip-like prefiltering (UNVERIFIED: AE Best quality downscales with a box-area filter rather than plain bilinear, avoiding aliasing).
  - Bicubic: 4×4 tap cubic; kernel UNVERIFIED (Keys a = −0.5 assumed); can ring/overshoot at hard edges (Adobe help), helps more for upscaling; results clamped to [0, 1] at 8/16 bpc, and overshoot may produce < 0 or > 1 in 32 bpc (UNVERIFIED).
  - Not used by (Adobe help): textures in the Ray-traced 3D renderer (always bilinear); transformations inside effects (bilinear unless the effect implements its own method). UNVERIFIED for the Advanced 3D renderer.
  - Samples outside the layer are transparent (zero alpha), not edge-clamped.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** UNVERIFIED version of introduction (a CC-era addition, around AE CC 2014.1-2015, 13.1-13.5). Source: https://helpx.adobe.com/after-effects/using/layers.html (Bilinear and bicubic sampling; shortcuts Alt/Option+B and Alt/Option+Shift+B; Quality switch cycles Best (bilinear) → Draft → Best (bicubic)). Implementation hint: bilinear = hardware sampler on premultiplied textures with mipmaps for strong minification (trilinear when scale < 0.5); bicubic = 4-tap optimised B-spline/Catmull-Rom shader using bilinear fetches.

### AE-CMP-061 · Subpixel positioning and precision (子像素定位)

- **What it does:** Layer positions and other spatial values are fractional; in Best quality a layer placed between pixel boundaries is resampled (slight softening), in Draft it snaps to whole pixels.
- **Exact semantics:**
  - All spatial values (Position, Anchor Point, effect points) are computed to a precision of 1/65,536 pixel (Adobe help).
  - Best: the transform matrix is used unrounded; a fractional offset produces interpolation blur similar to anti-aliasing.
  - Draft: translation is rounded to whole pixels (UNVERIFIED rounding rule), giving visible stepping in slow moves.
  - A layer at integer position, 100 % scale, 0° rotation and matching PAR is copied pixel-exactly in Best quality (no filtering), which users rely on for crisp UI/graphics.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Implementation hint: detect identity-up-to-integer-translation transforms and use a copy/blit path (exactness and speed); keep layer matrices in f64 on the CPU and convert to f32 only after subtracting the ROI origin to avoid precision loss on large comps.

### AE-CMP-062 · Resolution downsampling factor (分辨率/降采样因子（译名待核）)

- **What it does:** When a comp is previewed or rendered at reduced resolution (Half, Third, Quarter, Custom), every layer and effect is rendered with a downsample factor so the image looks like a scaled-down full-resolution render.
- **Exact semantics:**
  - Factor `(dx, dy)` with `dx, dy ∈ {1, 2, 3, 4, custom}`; render buffers are `ceil(width / dx) × ceil(height / dy)` (UNVERIFIED rounding).
  - All pixel-distance parameters are scaled by `1/dx`, `1/dy`: effect sliders measured in pixels (blur radius, glow radius, stroke width...), mask feather and expansion, layer-style sizes, motion-blur distances (automatic via transforms), shape stroke widths and text sizes (automatic via transforms). Effects receive `downsample_x/y` (SDK `PF_InData`) and must scale scalar pixel parameters themselves; point parameters and layer-parameter dimensions are pre-scaled by AE.
  - Non-pixel parameters (angles, percentages, colours, counts, random seeds) are not scaled.
  - Effects that cannot match full resolution exactly (e.g. integer-radius kernels, noise patterns) may look different at reduced resolution; this is accepted AE behaviour.
  - Preview resolution UI (Composition panel menu, Adaptive Resolution, Auto) is in compositing-3d-color.md, Preview resolution.
- **Priority:** P0 — without consistent downsampling, half-resolution previews look wrong.
- **Owner:** bae-render (+ bae-effects, bae-vector, bae-text)
- **Notes:** Scripting `CompItem.resolutionFactor` ([x, y]). BAE decision: effects declare which parameters are pixel distances in their schema so the host can pre-scale them; plug-ins via the AE-compat layer still receive `downsample_x/y` (ARCHITECTURE 15).

## Layer bounds and buffers

### AE-CMP-063 · Layer pixel bounds, comp-edge clipping and buffer limits (图层边界与合成边缘裁剪（译名待核）)

- **What it does:** Defines the extent of each layer's pixel buffer through the pipeline and where content is cropped.
- **Exact semantics:**
  - Source bounds: footage frame size; solids/adjustment solids their size; text and shape layers the bounds of their rendered content (plus stroke/effects); precomp layers the nested comp frame (AE-CMP-054) unless collapsed.
  - Masks never enlarge bounds (they only reduce alpha); feather is computed only inside the layer frame (AE-CMP-041).
  - Effects see the whole layer, including parts outside the comp frame (a blur near the comp edge uses pixels outside it). Effects may expand the buffer (Drop Shadow, Glow, blurs without "Repeat Edge Pixels", Grow Bounds), so the effect output can extend beyond the source frame; effects that do not expand clip their output to the input buffer (a classic AE artefact: a blur on a layer cut off at the layer edge). See effects.md, effect framework (output extent) and Grow Bounds (Utility).
  - After the transform, everything is clipped to the comp frame (plus the render ROI). Content outside a nested (non-collapsed) comp's frame is cropped when that comp is used as a layer.
  - Maximum comp and layer size 30,000 × 30,000 px (SDK: effect output buffers are clamped to 30,000 × 30,000).
  - Empty bounds (zero-area content, fully transparent after masks) → layer skipped downstream, but still evaluated for expressions.
- **Priority:** P0
- **Owner:** bae-render (+ bae-effects)
- **Notes:** SDK `extent_hint` = smallest rectangle containing all non-zero-alpha pixels; BAE keeps an equivalent per-buffer "content rect" to shrink work. Implementation hint: every render-graph node propagates (input rect → output rect) and (requested ROI → needed input ROI) like AE SmartFX PreRender, so off-screen work is skipped.

## Motion blur and frame blending rendering

### AE-CMP-064 · Motion blur rendering (运动模糊)

- **What it does:** For layers with the Motion Blur switch on (and the comp's Enable Motion Blur switch on for previews, or Render Settings motion blur on for output), the layer is rendered at several sub-frame times within the shutter interval and the results are averaged.
- **Exact semantics:**
  ```
  if comp.mb_enabled and layer.mb_switch (with nested-switch rules AE-CMP-057):
      times = motion_blur_sample_times(layer, t)       # shutter angle/phase, samples per frame,
                                                       # adaptive limit: see transform-keyframes-
                                                       # expressions.md, motion-blur sampling
      img = (1/N) * Σ_i  render_layer_at(layer, times[i])        # equal (box) weights, premultiplied
  else:
      img = render_layer_at(layer, t)
  ```
  - What is re-evaluated per sub-sample: the layer transform (incl. parent chain, camera for 3D), opacity, masks set to Same As Layer/On (AE-CMP-046), shape and text content of continuously rasterized layers (their path animation is blurred — shape layers always use Samples Per Frame), and nested comps when the precomp layer is collapsed or nested layers have MB (each nested layer blurs according to its own switch).
  - Not re-evaluated: effects on raster layers (rendered once at frame time, then transformed per sample) unless the effect supports shutter-angle motion blur itself (SDK `PF_OutFlag_I_USE_SHUTTER_ANGLE`; e.g. Transform effect, CC Force Motion Blur family); motion inside footage (Adobe help: the MB switches cannot blur motion that already exists within a layer; use frame blending or effects such as Pixel Motion Blur instead).
  - Averaging is in premultiplied space so edges blur correctly; 8/16 bpc clamping applies after averaging (UNVERIFIED).
  - 2D layers: number of samples adapts to the layer's motion between Samples Per Frame (minimum; default 16) and Adaptive Sample Limit (maximum; default 128); 3D layers and shape layers always use Samples Per Frame (Adobe help, Composition Settings > Advanced). Defaults and ranges are specified in project-comp-layers.md, Composition Settings.
  - Track mattes: the matte layer is blurred according to its own MB switch; the matted layer's blurred image is then multiplied by the (separately blurred) matte value (UNVERIFIED: AE applies the matte per sample vs after averaging).
  - Adjustment layers: blur only their footprint (AE-CMP-050).
  - 3D layers: each sample renders the layer through the camera at the sample time; camera motion itself blurs every 3D layer with MB on.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Scripting `AVLayer.motionBlur`, `CompItem.motionBlur`, `CompItem.shutterAngle` (0-720), `shutterPhase` (−360…360), `motionBlurSamplesPerFrame`, `motionBlurAdaptiveSampleLimit`. Lottie has no motion blur (some players offer a global MB option). Cost: up to N× the per-layer transform/resample cost. Implementation hint: for raster layers render the pre-transform image once and accumulate N transformed draws of the same texture into an RGBA16F/32F accumulator with additive blending and weight 1/N (one pass, no extra buffers); for rigid 2D transforms an analytic line-integral sampler (k taps along the motion vector per pixel) is cheaper and visually equivalent; only re-rasterise per sample when content (masks, shapes, text, nested comps) changes within the shutter.

### AE-CMP-065 · Frame blending rendering (帧混合)

- **What it does:** For footage layers (video or image sequences) whose source frames do not line up with the comp's frames (time stretch, time remap, conformed footage), creates intermediate frames by blending source frames instead of repeating them. Not applicable to precomp layers or single stills (Adobe help: "You can't apply frame blending to a precomposition layer"; apply it to the footage layers inside the nested comp instead).
- **Exact semantics:**
  - Layer switch values (scripting `AVLayer.frameBlendingType`, `FrameBlendingType.`): Off (`NO_FRAME_BLEND`), Frame Mix (帧混合, `FRAME_MIX`), Pixel Motion (像素运动, `PIXEL_MOTION`). Only applied when the comp's Enable Frame Blending switch is on (previews) / Render Settings frame blending is on (output).
  ```
  Frame Mix:
    s  = source time for this comp frame (in source frames, fractional)   # see transform-
                                                                          # keyframes-expressions.md
    f0 = floor(s); w = s - f0
    img = (1 - w) * frame(f0) + w * frame(f0 + 1)     # premultiplied blend; UNVERIFIED
    # when several source frames fall within one comp frame (speed-up), AE averages them
    # (UNVERIFIED: average of all source frames inside the comp frame duration)
  Pixel Motion:
    img = optical-flow interpolation between frame(f0) and frame(f0+1) at phase w
          (motion vectors estimated per pixel; AE's algorithm is proprietary)
  ```
  - Integer `s` (no fractional phase) → the frame itself, no blending.
  - Frame blending applies before masks/effects (it produces the layer's source frame).
  - Quality interaction: a frame-blended layer in Draft quality always uses Frame Mix, even if Pixel Motion is selected (Adobe help).
  - Gate: the comp's Enable Frame Blending switch disables frame blending for all layers in that comp regardless of layer switches (Adobe help); Render Settings can enable it for output.
- **Priority:** P1 (Frame Mix); Pixel Motion P2.
- **Owner:** bae-render (+ bae-media, bae-anim)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/time-stretching-time-remapping.html (Frame blending). zh-CN: menu 图层 > 帧混合 > 帧混合 / 像素运动. Which source frames and weights are used is defined in transform-keyframes-expressions.md, frame blending; this item covers only the pixel combination. Cost: Frame Mix decodes two frames per output frame (cache-friendly); Pixel Motion needs a flow pass (GPU compute, e.g. pyramidal Lucas-Kanade/DIS) — BAE decision: Pixel Motion may ship later behind Frame Mix.

## Other compositing behaviour

### AE-CMP-066 · Effects switch and per-effect enable in rendering (效果开关和单个效果启用（译名待核）)

- **What it does:** The layer's Effects (fx) switch bypasses the layer's whole effect stack; each effect's own enable checkbox bypasses that effect only.
- **Exact semantics:**
  - Layer fx switch off: step 3 of AE-CMP-007 (and step 4 of AE-CMP-008) is skipped entirely; masks, transform, styles, matte and blend still apply. For an adjustment layer it disables the adjustment (result = composite below, unchanged).
  - Effect enable off: that effect is removed from the chain; the next effect receives the previous effect's output.
  - Disabled effects are not evaluated (no rendering cost); their expressions are still evaluated when referenced elsewhere (UNVERIFIED).
  - Render Settings "Effects: Current Settings / All On / All Off" overrides for output (see ui-preview-render-io.md, Render Queue).
  - Nested propagation of the fx switch: AE-CMP-057.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** Scripting `AVLayer.effectsActive`; per effect `PropertyBase.enabled`. Lottie: effects carry `en` (enabled) flags; lottie-web supports only a few effect types.

### AE-CMP-067 · Pixel aspect ratio conversion during compositing (合成时的像素长宽比转换（译名待核）)

- **What it does:** Layers whose source pixel aspect ratio (PAR) differs from the comp's PAR are horizontally rescaled as part of the layer transform so they look correct in the comp.
- **Exact semantics:**
  ```
  par_scale_x = source.PAR / comp.PAR          # applied in layer space before Scale/Rotation
  M_layer = Translate(Position) * Rotate * Scale * Scale(par_scale_x, 1) * Translate(-AnchorPoint)
  ```
  - Anchor Point and mask coordinates stay in source pixels (unscaled); the comp viewer can additionally apply "Pixel Aspect Ratio Correction" for display only (ui).
  - Nested comps use the nested comp's PAR as their source PAR (AE-CMP-054).
  - The conversion is a real resample of raster layers (filtered per Quality/Sampling); continuously rasterized and collapsed layers include it in their transform without an extra resample.
  - Effects on a layer operate in source pixels (non-square); AE adjusts effect point parameters for PAR (SDK pixel-aspect-ratio notes: effects receive `pixel_aspect_ratio` and must compensate for circular kernels).
- **Priority:** P1 — matters for anamorphic/DV footage; square-pixel projects are unaffected.
- **Owner:** bae-render (+ bae-anim, bae-media)
- **Notes:** PAR values come from Interpret Footage and Composition Settings (project-comp-layers.md). UNVERIFIED: exact position of the PAR scale in the matrix chain (assumed innermost, i.e. applied to source pixels first). Lottie assumes square pixels.

## 3D layers and the Classic 3D renderer

### AE-CMP-400 · 3D Layer switch (3D 图层)

- **What it does:** Turns a 2D AV layer (footage, solid, precomp, shape, text, null) into a planar 3D object that can be positioned in comp space, seen through cameras and lit by lights. The layer itself stays a flat plane.
- **Exact semantics:**
  - Toggled by the 3D Layer switch in the Timeline or Layer > 3D Layer. Camera and light layers are always 3D and have no switch; audio-only layers cannot be 3D.
  - On enable: Position, Anchor Point and Scale gain a z component (Position z = 0, Anchor z = 0, Scale z = 100 %), and the layer gains Orientation [0,0,0], X Rotation 0°, Y Rotation 0°, Geometry Options and Material Options (AE-CMP-440, AE-CMP-441). The 2D Rotation property is renamed Z Rotation and keeps its value/keys.
  - On disable: Orientation, X/Y Rotation, Geometry Options and Material Options are deleted together with values, keyframes and expressions (not restored if 3D is re-enabled). Position/Anchor/Scale keep their keys and expressions; their z values are kept in data but hidden and ignored while 2D.
  - Only 3D layers interact with cameras, lights and shadows. Effects and masks on a 3D layer are computed in the layer's own 2D space before projection (a Bulge seen edge-on shows no relief).
  - Coordinate system: comp origin top-left, +x right, +y down, +z away from the viewer (into the screen). Matrix order and Orientation/Rotation composition: see transform-keyframes-expressions.md, Transform properties & matrix order.
  - Edge cases: a 3D layer in a comp with no camera layer is seen through the default comp camera (AE-CMP-403); with that camera a 3D layer at z = 0 with zero rotations renders pixel-identically to the same layer in 2D.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model, bae-anim)
- **Notes:** Scripting `AVLayer.threeDLayer` (Boolean), `AVLayer.threeDPerChar` for text; property matchNames `ADBE Orientation`, `ADBE Rotate X`, `ADBE Rotate Y`, `ADBE Rotate Z`, groups `ADBE Plane Options Group` (Geometry Options), `ADBE Material Options Group`. Lottie: layer `ddd: 1` (and comp/animation-level `ddd: 1` when any layer is 3D); transform `ks.or`, `ks.rx`, `ks.ry`, `ks.rz`. Per-character 3D text: see text.md. Switch storage: see project-comp-layers.md, Layer switches. The Geometry Options group (bevel/extrusion, curvature) is only active under CINEMA 4D / Advanced 3D renderers (AE-CMP-413, AE-CMP-414, AE-CMP-442).

### AE-CMP-401 · 3D renderer selection (3D 渲染器)

- **What it does:** Each composition chooses which plug-in renderer draws its 3D layers; the choice decides which 3D features (intersections, shadows, extrusion, models, environment light) exist.
- **Exact semantics:**
  - Per-composition setting (Composition Settings > 3D Renderer tab, also the renderer menu at the top-right of the Composition panel).
  - | Renderer (中文) | Since / status | Scripting `CompItem.renderer` string | BAE priority |
    |---|---|---|---|
    | Classic 3D (经典 3D) | default; the internal id keeps the pre-CS6 name "Advanced 3D" (UI name Classic 3D since CS6) | `ADBE Advanced 3d` | P0 |
    | CINEMA 4D (CINEMA 4D) | AE CC 2015 (13.5) – still listed in current AE (the Advanced 3D help calls Advanced 3D "an additional rendering option to Classic 3D and Cinema 4D"); since AE 23.0 it needs the Cinema 4D add-on installed via Creative Cloud. UNVERIFIED: any formal deprecation | `ADBE Calder` | P2 |
    | Ray-traced 3D (光线追踪 3D) | AE CS6 (11.0); labelled deprecated after CINEMA 4D arrived (13.5); removed in AE 17.0 (AE 2020, 2019-11) | `ADBE Ernst` | P2 (not implemented; load as Classic 3D) |
    | Advanced 3D (高级 3D) | AE 24.0 (2023-10) | UNVERIFIED: `ADBE Mercury 3d` | P2 |
  - The default for new comps is Classic 3D; adding a 3D model layer switches the comp to Advanced 3D automatically. `CompItem.renderers` lists the installed renderers (strings above; `CompItem.renderer` is read/write).
  - Rendering contract: the renderer is called once per **3D group ("bin")** of contiguous 3D layers (AE-CMP-406); 2D layers are composited by the normal 2D pipeline.
- **Priority:** P0 (Classic 3D only; others P2)
- **Owner:** bae-render (+ bae-model)
- **Notes:** Source for Ray-traced removal: Adobe community thread "AE 17.0 no 3D ray-traced option" (https://community.adobe.com/t5/after-effects-discussions/ae-17-0-no-3d-ray-traced-option/m-p/10736255). BAE decision: implement Classic 3D semantics first; when loading an AE project whose comp uses CINEMA 4D / Ray-traced / Advanced 3D, keep the renderer string in data, render with the Classic 3D path and show a "renderer substituted" warning. Composition Settings dialog layout: see project-comp-layers.md, Composition Settings.

### AE-CMP-402 · Classic 3D perspective projection pipeline (经典 3D 渲染器)

- **What it does:** Projects each 3D layer plane through the active camera onto the comp frame with a pinhole perspective (or orthographic for side views) and resamples the layer texture into comp pixels.
- **Exact semantics:**
  - Each 3D layer is a rectangle in its own layer space `[0, w] × [0, h]` (source pixels; for collapsed/continuously rasterized layers, the rasterized bounds) placed in comp space by its full 4×4 matrix `M_layer` (anchor, scale, rotations, orientation, position, parent chain; see transform-keyframes-expressions.md).
  - The camera supplies a view matrix `V` (inverse of camera world matrix, camera looking down its local +z) and a projection with principal point at the comp centre:
    ```
    // camera space point (xc, yc, zc), zc > 0 in front of the camera
    x_comp = W/2 + zoom * xc / zc
    y_comp = H/2 + zoom * yc / zc
    // zoom in comp pixels (horizontal), W, H = comp size in pixels
    // a point at distance zc == zoom maps at 1:1 scale
    ```
  - Pixel aspect ratio: the projection is done in comp pixel units; comp PAR does not change the 3D math (it only affects display/output interpretation). UNVERIFIED: AE scales x by 1/PAR for non-square comps in camera projection; best evidence says no extra PAR term.
  - Near clipping: geometry with `zc <= near` is clipped. UNVERIFIED: AE's near plane is small (≈1 px in front of the camera); layers behind the camera are not drawn (no wrap-around).
  - Far clipping: none for Classic 3D.
  - Texture resampling uses the layer's Quality/Sampling switch (bilinear / bicubic, see compositing-3d-color.md, Quality and sampling) with perspective-correct interpolation; Draft quality = nearest neighbour.
  - Layers seen exactly edge-on render nothing (zero area); back faces render the mirrored front (layers are double-sided in Classic 3D).
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Implementation hint: each layer is one quad → draw as two triangles with a homogeneous-coordinate vertex shader; texture sampled with perspective-correct UVs; use mip-maps for minification (AE's Best quality mips when downscaling). Orthographic views (Front/Left/Top/…, AE-CMP-410) use `x_comp = W/2 + s*xc`, no divide.

### AE-CMP-403 · Default composition camera (默认摄像机)

- **What it does:** When a comp contains no enabled camera layer at the current time, 3D layers are seen through a hidden default camera that makes z = 0 map 1:1 onto the comp frame.
- **Exact semantics:**
  ```
  // comp W x H pixels
  zoom_default    = W * 50 / 36            // 50 mm lens on 36 mm horizontal film
                                           // e.g. 1920x1080 -> 2666.67 px; 640x480 -> 888.89 px
  position        = (W/2, H/2, -zoom_default)
  point_of_interest = (W/2, H/2, 0)
  orientation/rotation = 0; depth of field = Off
  ```
  UNVERIFIED: the default camera zoom equals the 50 mm preset (strongly supported by matching a new 50 mm camera layer, which renders identically to the no-camera view); exact value may differ in older versions.
  - Because a point at depth `zc = zoom` maps 1:1, every 3D layer at comp z = 0 with no rotation renders identically to 2D; changing the default camera zoom only changes perspective for z ≠ 0.
  - Selection rule: the default camera is used if no camera layer is active at time t (AE-CMP-423), and in the "Default Camera" view even when cameras exist.
  - Default Camera Settings (AE 25.x, 2025): View > Default Camera Settings / Layer > Camera > Default Camera Settings lets the user edit the default camera and custom views with the same Camera Settings dialog; orthographic views (Front, Left, Top, Back, Right, Bottom) are not editable. Stored per comp. UNVERIFIED: exact version (help page dated 2025).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** Nested comps that are not collapsed render their 3D layers with their own active camera or this default camera (AE-CMP-424).

### AE-CMP-404 · 3D depth ordering (3D 图层深度排序)

- **What it does:** Decides which 3D layer appears in front in a 3D group: by depth from the camera, not by Timeline stacking order.
- **Exact semantics:**
  - Inside one 3D group (AE-CMP-406) the Classic 3D renderer composites layers by camera-space depth per pixel (it resolves intersections, AE-CMP-405), so the visible result equals: for every comp pixel, collect each layer fragment covering it with its camera-space depth `zc`, sort far→near, composite with each layer's opacity and blend mode.
  - Ties (exactly coplanar layers, or equal depth at a pixel): the layer higher in the Timeline stacking order (lower index) is in front. Measured in AE 26.2.1 by 04: "equal" means within about 1 px of depth along the view ray — the seam of a layer crossing a camera-facing one at 45° sits one pixel into the lower layer's half.
  - Non-intersecting whole-layer ordering, where AE needs one (e.g. for the CPU legacy path and for shadows): sort by camera-space distance of the layer's centre. UNVERIFIED: AE documentation never states whether layer centre or anchor point is used; community reports favour the layer bounding-box centre for Classic 3D. BAE decision: use per-pixel depth (exact) so the question only affects AE-incompatible corner cases.
  - Semi-transparent layers composite correctly in depth order (no order-dependent artefacts) because sorting is per fragment.
  - 2D layers inside the comp are never depth-sorted; they composite by stacking order and split 3D groups.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Implementation hint: layers are planar quads, so an exact visibility order can be computed by a BSP split of the quads (each quad splits others at intersection lines), then drawn back-to-front with normal 2D blending; alternative: depth peeling or per-pixel fragment lists (WebGPU storage buffers). BSP is exact, antialias-friendly and cheap for typical layer counts (< 100 per group).

### AE-CMP-405 · Pixel-accurate 3D layer intersections (3D 图层相交)

- **What it does:** Two 3D layers that pass through each other are cut along their intersection line, each half visible where it is nearer.
- **Exact semantics:**
  - Occurs only between 3D layers rendered in the same 3D group with the Classic 3D (or C4D/Advanced 3D) renderer.
  - Does **not** occur between layers separated by any group-breaking layer (AE-CMP-406), or between a 3D layer and a layer inside a non-collapsed precomp.
  - The intersection edge is antialiased per the layer's Quality setting (Best = antialiased).
  - Shadows cast across intersections leave a gap near the intersection line whose width depends on the shadow-map resolution (AE-CMP-408).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Falls out of AE-CMP-404's per-pixel ordering. Very old AE versions offered a "Standard 3D" renderer that sorted whole layers and did not intersect (UNVERIFIED: the AE version in which the intersecting "Advanced 3D" renderer, today Classic 3D, became the default); BAE does not reproduce Standard 3D.

### AE-CMP-406 · 3D group splitting by 2D and special layers (3D 图层分组 / 3D 箱)

- **What it does:** Contiguous runs of 3D layers in the Timeline stack are rendered together as one 3D scene ("3D bin"); certain layers between them split the stack into separately rendered groups that are then composited in stacking order like 2D layers.
- **Exact semantics:**
  ```
  groups = []
  current = []
  for layer in comp.layers bottom..top (only layers active and visible at t):
      if is_3d_participant(layer):            // 3D AV layer, or collapsed 3D precomp
          current.push(layer)                 // whose own layers are all 3D (see below)
      else if layer is camera or light:
          continue                            // lights/cameras never split groups
      else:                                   // group breaker
          flush(current) -> groups (rendered by 3D renderer to one 2D image)
          groups.push(2D layer)
  flush(current)
  composite groups bottom..top with normal 2D rules (blend modes, mattes, adjustment layers...)
  ```
  Group breakers (layers that are not 3D participants):
  - any 2D layer;
  - an adjustment layer (2D or 3D);
  - a 3D layer with a layer style applied;
  - a 3D precomp layer with an effect, a closed mask whose mode is not None, or a track matte;
  - ~~a 3D precomp layer without Collapse Transformations~~ — not a breaker: AE 26.2.1 depth-sorts a plain (non-collapsed, no effect/mask/matte) 3D precomp layer with the other 3D layers of its group (measured by 04, `threed_precomp` in crates/bae-render/tests/ae);
  - a collapsed 3D precomp that contains any 2D layer (its content becomes a flat image).
  - Collapsed precomp with all-3D content: its 3D layers join the outer group individually (they intersect and shadow outer layers); in exchange the precomp layer's own blend mode, quality and motion blur switches are ignored.
  - Lights are not limited by groups: a light lights 3D layers in every group of the comp (subject to the light's own Adjustment Layer scope, AE-CMP-435). Shadows and intersections never cross a group boundary.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** "3D bins" is the term used in the AE 3D Design Space help (2023+). Collapse Transformations semantics: see compositing-3d-color.md, Collapse transformations / continuous rasterize. Shadows on a 3D layer with a track matte only stay correct if both are precomposed (non-collapsed); AE quirk, not a BAE requirement.

### AE-CMP-407 · Opacity, blend modes and track mattes on 3D layers (3D 图层的不透明度、混合模式和轨道遮罩)

- **What it does:** Defines how 2D compositing attributes apply to layers inside a 3D group.
- **Exact semantics:**
  - Opacity multiplies the layer's coverage before depth compositing; a 0 % 3D layer still casts shadows if Casts Shadows is On (shadow strength uses layer alpha × opacity, UNVERIFIED: AE uses alpha only, ignoring Opacity, for shadow maps).
  - Blend mode: each fragment of a 3D layer is blended (with its blend mode) over the already composited, farther fragments of the same group at that pixel; content of other groups or 2D layers below is reached only through the group's composite with the layers beneath, i.e. a 3D group image is first composited onto transparent and then the group (with Normal mode) onto the layers below. UNVERIFIED: whether AE renders each group onto the background below (letting a 3D layer's Multiply mode see a 2D layer below the group). Best-known behaviour: blend modes of 3D layers do see the 2D layers below the group (AE passes the underlying comp buffer to the 3D renderer).
  - Track matte on a 3D layer: the matte layer is rendered from the camera's view (if 3D) into comp space and applied to the matted layer's projected image in comp space. Track matte formulas: see compositing-3d-color.md, Track mattes.
  - Preserve Underlying Transparency on a 3D layer: applied against the composite below as in 2D (see compositing-3d-color.md, Preserve transparency).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** BAE decision: implement the "group renders over the underlying buffer" model, which matches the visible result of AE in common cases; flag any divergence found by golden-image tests.

### AE-CMP-408 · Classic 3D Renderer Options: Shadow Map Resolution (经典 3D 渲染器选项：阴影图分辨率)

- **What it does:** Sets the pixel resolution of the shadow maps the Classic 3D renderer renders from each shadow-casting light.
- **Exact semantics:**
  - | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Shadow Map Resolution (阴影图分辨率) | enum | Comp Size | Comp Size, 256, 512, 1000, 2000, 4000, 8000 UNVERIFIED: exact list | px (square map side) | "Comp Size" = automatic from comp resolution and layer Quality |
  - Opened from Composition Settings > 3D Renderer > Options…, or Cmd/Ctrl-click on the renderer name in the Composition panel.
  - Lower values → blurrier shadows and wider gaps at shadow/intersection contact lines; higher → sharper, slower.
  - Automatic size: shadow map side = max(comp W, comp H) at Full resolution, scaled by the current preview Resolution/Down Sample factor. UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Persisted per comp (renderer options blob in AE). Shadow algorithm: AE-CMP-437.

### AE-CMP-409 · Draft 3D mode (草图 3D)

- **What it does:** A comp-level preview switch that makes 3D interaction fast; it never affects final renders.
- **Exact semantics:**
  - Toggle: Draft 3D button (Composition panel toolbar in current AE; historically at the top of the Timeline panel); applies to all views of the comp. Scripting `CompItem.draft3d` (Boolean, read/write).
  - Classic semantics (documented in the Previews help, still valid as the minimum contract): disables all lights (3D layers render unlit, as if no light existed), all shadows, and camera depth-of-field blur.
  - Since AE 18.0 (2021-03, "3D Design Space") Draft 3D additionally switches the viewer to the GPU real-time draft engine (today the Advanced 3D engine, "only intended for draft quality") and enables the 3D Ground Plane and Extended Viewer buttons (AE-CMP-411). This real-time draft preview replaced the old OpenGL "Fast Draft" Fast Previews mode (AE-CMP-467). Help note: with Draft 3D on, some effects may not render on precomps. UNVERIFIED: whether the real-time draft engine still omits lights/shadows/DOF or now previews them approximately.
  - Render Queue / headless output always renders at full quality regardless of Draft 3D.
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion, bae-model)
- **Notes:** Stored per comp. Sources: https://helpx.adobe.com/after-effects/using/previewing.html (Preview modes: "Draft 3D … Disables lights, shadows, and depth-of-field blur for cameras"), real-time-engine help page "What is Draft 3D Preview", CG Channel on AE 18.0 (real-time 3D Draft preview replaces the OpenGL Fast Draft mode). BAE decision: implement the classic contract (no lights/shadows/DOF) on the normal renderer; BAE's renderer is already GPU real-time. Fast Previews menu: AE-CMP-467.

### AE-CMP-410 · 3D views (3D 视图)

- **What it does:** Lets a viewer show the 3D scene through the active camera, any camera, the default camera, orthographic side views or free custom views, without changing the render output.
- **Exact semantics:**
  - Menu order (3D View popup): Active Camera (活动摄像机), Default Camera (AE 25.x+), each camera layer by name, Front (正面), Left (左侧), Top (顶部), Back (背面), Right (右侧), Bottom (底部), Custom View 1, Custom View 2, Custom View 3 (自定义视图 1/2/3). UNVERIFIED: exact current order and the AE version that lists individual cameras.
  - Orthographic views: parallel projection along ±x, ±y, ±z with the comp's z = 0 plane framed; the view can be panned/zoomed by camera tools (orbit disabled).
  - Custom Views: perspective cameras stored per comp; editable by camera tools and (since 2025) by Default Camera Settings.
  - Only the Active Camera view is used for final output and for any non-viewer render. Viewer layouts (1/2/4 views), "Share View Options": see ui-preview-render-io.md, Composition viewer.
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** View-only camera state is not undoable in AE for orthographic/custom views (view changes are UI state). BAE decision: store custom view cameras in per-user UI state, not in the shared CRDT document.

### AE-CMP-411 · 3D Ground Plane and Extended Viewer (3D 地平面和扩展查看器（译名待核）)

- **What it does:** Viewer aids available with Draft 3D: a grid ground plane at comp y-floor and a viewer that shows 3D content outside the comp frame.
- **Exact semantics:**
  - Both buttons appear in the Composition panel bottom toolbar only when Draft 3D is on. Ground Plane shipped with the real-time Draft 3D preview in AE 18.0 (2021-03); Extended Viewer shortly after (UNVERIFIED: exact 18.x version). Ground Plane is on by default when Draft 3D is enabled; its grid scales progressively while dollying, and a scene-centre locator shows the X/Y/Z axes at the comp centre.
  - Extended Viewer renders the 3D scene beyond the comp bounds (pasteboard), with an opacity slider (press and hold the button) for the out-of-frame region.
  - Neither affects output.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** UI details: see ui-preview-render-io.md, Composition viewer. Implementation hint: Extended Viewer = render the active camera with an enlarged viewport and a shifted principal point.

### AE-CMP-412 · Create Camera from 3D View (从 3D 视图创建摄像机（译名待核）)

- **What it does:** Creates a camera layer whose transform and zoom reproduce the current viewer (default camera or custom view) perspective.
- **Exact semantics:** View > Create Camera from 3D View: new two-node camera layer at the top of the stack, Position/POI/Zoom copied from the current view camera at the current time; becomes the active camera. UNVERIFIED: one-node vs two-node type of the created camera.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Part of the 3D Design Space features (AE 18.x, UNVERIFIED exact version).

### AE-CMP-413 · Advanced 3D renderer (高级 3D 渲染器)

- **What it does:** GPU physically-based composition renderer (AE 24.0, 2023-10) used for imported 3D models (GLTF, GLB, OBJ, FBX per current help), environment lights with HDRI image-based lighting, extruded text/shape layers, and Adobe Standard Material on 3D layers. It is the only renderer that can render 3D model layers.
- **Exact semantics:**
  - With 3D layers but no light layer, Advanced 3D lights the scene with a hidden default Environment light (also used for reflections); adding any light layer disables it.
  - Renders 3D groups like Classic 3D (same grouping rules), but layers are shaded with a PBR material (Adobe Standard Material) using Material Options extensions (AE-CMP-441, AE-CMP-442); lights: Parallel/Spot/Point/Ambient and Environment (AE-CMP-439).
  - Shadows: Environment light shadows first; current versions allow up to eight shadow-casting Spot and Parallel lights plus one Environment light (Point lights are not listed as shadow casters). UNVERIFIED: version that added Spot/Parallel shadows (25.x).
  - Advanced 3D Render Options dialog (3D Renderer menu > Rendering Options): Render Quality slider (interactive vs final quality trade-off); Shadows: Resolution (Half 2 MB, Full 16 MB, Double 128 MB), Smoothness (1–32; follows Render Quality unless set), Casting Box Size, Casting Box Center, Fit to Scene button. Resolution/Smoothness apply to Environment-light shadows only.
  - System requirements (help): macOS 12.0+ with Metal, ≥ 16 GB RAM, Intel Macs need a discrete GPU with ≥ 4 GB VRAM; Windows: discrete GPU with ≥ 4 GB VRAM from the supported list.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** BAE decision: out of scope for M1; keep the comp setting and material data round-tripping. 3D model layers: see project-comp-layers.md, Layer types.

### AE-CMP-414 · CINEMA 4D composition renderer (CINEMA 4D 合成渲染器)

- **What it does:** CPU renderer from Maxon (AE CC 2015, 13.5 onward) that extrudes and bevels text/shape layers and bends footage layers, with a Quality slider.
- **Exact semantics:** Options dialog: Quality slider (adjusts Ray Threshold, Ray Depth, Reflection Depth, Shadow Depth), Anti-Aliasing (default Geometry, 16×16 sub-pixels), Reflectance (default Layer Sampling). Quality slider range Draft – Typical – Extreme. Geometry Options (Bevel Style, Bevel Depth, Hole Bevel Depth, Extrusion Depth, Curvature, Segments) become active. Renderer Settings can point at a full retail Cinema 4D installation (Choose Installation / Defaults). Since AE 23.0 the Cinema 4D add-on must be installed via Creative Cloud. UNVERIFIED: Anti-Aliasing/Reflectance defaults quoted above.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** BAE decision: not implemented; Geometry Options data is preserved and later honoured by BAE's extrusion path (shared with Advanced 3D).

### AE-CMP-415 · Ray-traced 3D renderer (removed) (光线追踪 3D 渲染器)

- **What it does:** Historical NVIDIA OptiX-based renderer (AE CS6 11.0) supporting reflections, transparency/refraction, environment layers, bending and extrusion.
- **Exact semantics:** Material options specific to it: Reflection Intensity/Sharpness/Rolloff, Transparency, Transparency Rolloff, Index of Refraction; comp options Ray-tracing Quality, Anti-aliasing Filter (Box/Tent/Cubic). Removed in AE 17.0 (AE 2020); projects that used it open with Classic 3D.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** BAE decision: never implemented; on import map `ADBE Ernst` to Classic 3D and preserve the extra material properties as opaque data.

## Cameras

### AE-CMP-416 · Camera layer (摄像机图层)

- **What it does:** A non-rendering layer that defines the viewpoint, lens and depth of field through which the comp's 3D layers are projected.
- **Exact semantics:**
  - Create: Layer > New > Camera (Cmd+Opt+Shift+C / Ctrl+Alt+Shift+C) opens Camera Settings (AE-CMP-417). New camera goes on top of the stack; starts at comp start (or at the CTI if the "Create Layers at Composition Start Time" preference is off).
  - Initial transform: Position = (W/2, H/2, −zoom), Point of Interest = (W/2, H/2, 0), Orientation (0,0,0), X/Y/Z Rotation 0°. Changing the preset of an existing camera does not move it.
  - Transform group (in order): Point of Interest (目标点; two-node only), Position (位置), Orientation (方向), X Rotation, Y Rotation, Z Rotation. No Anchor Point, Scale or Opacity. Camera layers have no source, no blend mode, no masks/effects, no 3D switch (always 3D). Camera motion (position/POI/rotation/zoom animation) produces motion blur on 3D layers whose Motion Blur switch is on (see compositing-3d-color.md, Motion-blur rendering).
  - Type (类型):
    | Enum (中文) | Behaviour | Scripting |
    |---|---|---|
    | One-Node Camera (单节点摄像机) | orientation from Orientation/Rotation only; POI property hidden | `autoOrient = AutoOrientType.NO_AUTO_ORIENT` |
    | Two-Node Camera (双节点摄像机) | additionally aims at Point of Interest (look-at), = Auto-Orient "Orient Towards Point of Interest" | `AutoOrientType.CAMERA_OR_POINT_OF_INTEREST` |
    Default: Two-Node Camera (factory). UNVERIFIED: whether the dialog remembers the last-used type.
    Changing Type later is equivalent to Layer > Transform > Auto-Orient (Off ↔ Orient Towards Point of Interest); help states the two are the same thing.
  - Parenting: cameras can be parented / be parents like any layer (see project-comp-layers.md, Parenting).
  - Camera layer Video switch off ⇒ the camera is ignored for active-camera selection.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model, bae-anim)
- **Notes:** matchName `ADBE Camera Layer`; POI uses property matchName `ADBE Anchor Point` with display name "Point of Interest". Scripting `LayerCollection.addCamera(name, centerPoint)`, `CameraLayer`. Look-at math: see transform-keyframes-expressions.md, Auto-orient math. Lottie: camera layer `ty: 13` (AE-CMP-428).

### AE-CMP-417 · Camera Settings dialog (摄像机设置)

- **What it does:** Modal dialog for creating/editing a camera: type, name, lens preset and the linked zoom/angle/film/focal/aperture values.
- **Exact semantics:** Fields in dialog order:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Type (类型) | enum | Two-Node Camera | One-Node / Two-Node | — | AE-CMP-416 |
  | Name (名称) | string | "Camera N" (N = 1 + count of existing cameras) | — | — | localized "摄像机 N" |
  | Preset (预设) | enum | 50mm (help: "The default preset is 50mm") | 15mm, 20mm, 24mm, 28mm, 35mm, 50mm, 80mm, 135mm, 200mm, Custom | — | sets Zoom, AOV, Focal Length, Focus Distance, Aperture; any manual edit switches to Custom; user presets can be saved/deleted (disk icons) |
  | Zoom (缩放 / 变焦) | float | from preset (50mm: W·50/36) | > 0 UNVERIFIED: min 1 px | px (or selected Units) | AE-CMP-418 |
  | Angle of View (视角) | float | from preset (50mm horizontal: 39.60°) | (0, 180) | degrees | measured per Measure Film Size |
  | Enable Depth of Field (启用景深 / 景深) | bool | off | — | — | |
  | Focus Distance (焦点距离) | float | = Zoom | > 0 | px | |
  | Lock to Zoom (锁定到缩放) | bool | on UNVERIFIED | — | — | makes Focus Distance = Zoom; the lock is dialog-only: editing Zoom or Focus Distance in the Timeline unlocks them (help) |
  | Aperture (光圈) | float | preset-derived (each preset sets Aperture) UNVERIFIED: exact per-preset values; community recollection is "25.3" for 50mm but whether that number is px or mm is unconfirmed | ≥ 0 | px (or selected Units) | AE-CMP-419 |
  | F-Stop (光圈大小) | float | derived: focal_length_mm / aperture_mm (AE-CMP-419) | > 0 | — | editing F-Stop changes Aperture and vice versa |
  | Blur Level (模糊层次) | float | 100 | 0–∞ UNVERIFIED: dialog allows > 100 | % | scales DOF blur |
  | Film Size (胶片大小) | float | 36 | > 0 | mm (or Units) | |
  | Focal Length (焦距) | float | 50 | > 0 | mm (or Units) | |
  | Units (单位) | enum | pixels | pixels, inches, millimeters | — | display units for Zoom, Focus Distance, Aperture, Film Size, Focal Length |
  | Measure Film Size (测量胶片大小) | enum | Horizontally | Horizontally, Vertically, Diagonally | — | AE-CMP-418 |
  | Preview (预览) | bool | on | — | — | live preview in viewer while dialog is open |
  - Selecting a preset always resets Film Size to 36 mm (35 mm-film presets), even after a custom Film Size was entered.
  - OK on a new camera creates the layer; OK on an existing camera writes Zoom/Focus Distance/Aperture/Blur Level/Depth of Field into Camera Options at the current time (adds a keyframe if the property is keyframed). Type change toggles the layer's auto-orient.
  - Default Camera Settings and Custom View settings reuse this dialog (AE-CMP-403).
- **Priority:** P0 (P1 for Units and Measure Film Size options)
- **Owner:** bae-ui-motion (+ bae-motion, bae-model)
- **Notes:** Dialog is UI; the formulas it applies are AE-CMP-418/419 and must live in a non-UI crate (bae-motion command `SetCameraSettings`). Scripting cannot open the dialog; values are set through Camera Options properties.

### AE-CMP-418 · Lens math: zoom, film size, focal length, angle of view (镜头换算：缩放、胶片大小、焦距、视角)

- **What it does:** Defines how the dialog's lens values relate; only Zoom is stored and rendered, the others are derived for display.
- **Exact semantics:**
  ```
  // dim = comp size in px along the measure direction
  dim = W                 if Measure Film Size == Horizontally
      = H                 if Vertically
      = sqrt(W^2 + H^2)   if Diagonally
  zoom_px      = focal_length * dim / film_size          // film_size, focal_length same units
  AOV_degrees  = 2 * atan( dim / (2 * zoom_px) ) * 180/pi
               = 2 * atan( film_size / (2 * focal_length) ) * 180/pi
  // editing rules in the dialog:
  //  - change Zoom       -> AOV and Focal Length recomputed (film size fixed)
  //  - change AOV        -> Zoom and Focal Length recomputed
  //  - change Focal Len  -> Zoom, AOV, Aperture px recomputed; Preset updated to match if equal to a preset
  //  - change Film Size  -> Zoom recomputed (focal length fixed)
  //  - change Measure    -> Zoom unchanged; Film Size and AOV re-expressed for the new dim
  //                         (presets are defined on a 36 mm *horizontal* film, so in Diagonal mode
  //                          the 50mm preset still gives zoom = W*50/36 and shows AOV 44.9° at 1920x1080)
  ```
  UNVERIFIED: the exact behaviour when Measure Film Size changes (whether Film Size or Zoom is held). Supporting evidence for "Zoom held": ProVideo Coalition (Chris Zwar, "Inside the After Effects camera") reports the default 50mm camera at 1920×1080 as Zoom 2666.7 px with a diagonal Angle of View of 44.9° (entering 44.885° reproduces 2666.7), i.e. the diagonal AOV of zoom = W·50/36.
  Preset table (Measure = Horizontally, film 36 mm; zoom shown for W = 1920, H = 1080):
  | Preset | Focal (mm) | Zoom (px) | AOV h (°) | AOV v (°) | AOV diag (°) |
  |---|---|---|---|---|---|
  | 15mm | 15 | 800.00 | 100.39 | 68.04 | 108.02 |
  | 20mm | 20 | 1066.67 | 83.97 | 53.70 | 91.84 |
  | 24mm | 24 | 1280.00 | 73.74 | 45.75 | 81.42 |
  | 28mm | 28 | 1493.33 | 65.47 | 39.76 | 72.82 |
  | 35mm | 35 | 1866.67 | 54.43 | 32.27 | 61.09 |
  | 50mm | 50 | 2666.67 | 39.60 | 22.90 | 44.89 |
  | 80mm | 80 | 4266.67 | 25.36 | 14.43 | 28.95 |
  | 135mm | 135 | 7200.00 | 15.19 | 8.58 | 17.40 |
  | 200mm | 200 | 10666.67 | 10.29 | 5.80 | 11.79 |
  - Units conversion: pixels ↔ mm uses the film-size scale (`mm = px * film_size / dim`); inches = mm / 25.4.
  - Comp PAR is ignored by these formulas (dim is in comp pixels). UNVERIFIED for non-square-pixel comps.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Expressions commonly rely on `zoom = W*focal/36` and on AOV ↔ zoom conversions; BAE must produce identical zoom numbers to 0.01 px so imported expressions and Lottie `pe` match.

### AE-CMP-419 · Aperture and F-Stop relation (光圈与光圈大小)

- **What it does:** Links the stored Aperture (px) to the photographic F-Stop shown in the dialog.
- **Exact semantics:**
  ```
  aperture_mm = aperture_px * film_size_mm / dim          // same scale as AE-CMP-418
  f_stop      = focal_length_mm / aperture_mm
  // edit F-Stop  -> aperture_px = (focal_length_mm / f_stop) * dim / film_size_mm
  // edit Focal   -> aperture_px kept? UNVERIFIED: help says Aperture changes with Focal Length,
  //                 i.e. F-Stop is held and aperture_px recomputed
  ```
  - Aperture 0 ⇒ no DOF blur (pinhole) even with Depth of Field on; F-Stop shows ∞ / blank. UNVERIFIED display.
  - Changing aperture does not change exposure/brightness (AE ignores light gathering).
- **Priority:** P1
- **Owner:** bae-motion
- **Notes:** —

### AE-CMP-420 · Camera Options property group (摄像机选项)

- **What it does:** The animatable camera properties shown in the Timeline under the camera layer.
- **Exact semantics:** Properties in Timeline order:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Zoom (缩放) | 1D float | from dialog | > 0 UNVERIFIED: min 1 | px | `ADBE Camera Zoom` |
  | Depth of Field (景深) | bool (On/Off, hold-only keys) | Off | — | — | `ADBE Camera Depth of Field` |
  | Focus Distance (焦点距离) | 1D float | = Zoom | > 0 UNVERIFIED: min 1 | px | `ADBE Camera Focus Distance` |
  | Aperture (光圈) | 1D float | from dialog | ≥ 0 | px | `ADBE Camera Aperture` |
  | Blur Level (模糊层次) | 1D float | 100 | ≥ 0 | % | `ADBE Camera Blur Level` |
  | Iris Shape (光圈形状) | enum | Fast Rectangle | AE-CMP-422 | — | `ADBE Iris Shape` (AE CS6+) |
  | Iris Rotation (光圈旋转) | angle | 0 | any | ° | `ADBE Iris Rotation` |
  | Iris Roundness (光圈圆度) | float | 0 | 0–100 | % | `ADBE Iris Roundness` |
  | Iris Aspect Ratio (光圈长宽比) | float | 1 | UNVERIFIED: 0.01–100 | ratio | `ADBE Iris Aspect Ratio` |
  | Iris Diffraction Fringe (光圈衍射条纹) | float | 0 | UNVERIFIED: 0–500 | — | `ADBE Iris Diffraction Fringe` |
  | Highlight Gain (高光增益) | float | 0 | 0–100 | — | `ADBE Iris Highlight Gain` |
  | Highlight Threshold (高光阈值) | float | 255 | 0–255 (8-bpc scale; 0–1 shown in 32 bpc UNVERIFIED) | level | `ADBE Iris Highlight Threshold` |
  | Highlight Saturation (高光饱和度) | float | 0 | 0–100 | — | `ADBE Iris Hightlight Saturation` (sic, misspelled matchName; confirmed in docsforadobe Camera Layer Match Names) |
  - Group matchName `ADBE Camera Options Group`. Editing Zoom or Focus Distance in the Timeline breaks Lock to Zoom (it is not a persisted link).
  - All Iris/Highlight Chinese names（译名待核） except 光圈 (Aperture). Zoom: zh-CN help uses 缩放. Iris/Highlight defaults mirror the Camera Lens Blur effect defaults (Fast Rectangle, Gain 0, Threshold 255, Saturation 0) UNVERIFIED for the camera group.
- **Priority:** P0 (Zoom, Depth of Field, Focus Distance, Aperture, Blur Level); P2 (Iris/Highlight)
- **Owner:** bae-model (+ bae-render)
- **Notes:** Expression access: `thisComp.activeCamera.zoom`, camera object attributes `pointOfInterest`, `zoom`, `depthOfField`, `focusDistance`, `aperture`, `blurLevel`, `irisShape` etc. — see transform-keyframes-expressions.md, Camera object. Lottie exports only Zoom (`pe`).

### AE-CMP-421 · Camera depth-of-field blur (摄像机景深模糊)

- **What it does:** When Depth of Field is On, 3D layers away from the focus distance are blurred according to aperture and blur level.
- **Exact semantics:** Best-known model (thin-lens circle of confusion in comp pixels). UNVERIFIED: Adobe does not publish the formula; this matches observed behaviour (no blur at focus distance, blur grows with aperture, symmetric in 1/z).
  ```
  // zc = camera-space depth of the layer point, s = Focus Distance, A = Aperture (px),
  // k = Blur Level / 100, zoom = camera zoom (image-plane distance in px)
  coc_diameter_px = k * A * zoom * abs(1/s - 1/zc)
  blur_radius_px  = coc_diameter_px / 2
  // applied as a disc (iris-shaped, AE-CMP-422) blur of the projected layer image
  ```
  - Applied per layer, after projection, only to 3D layers; 2D layers are never blurred. Blur varies across a layer that recedes in depth (per-pixel depth). UNVERIFIED: whether Classic 3D uses per-pixel or per-layer-centre depth; per-pixel is believed.
  - Not affected by layer Quality; scaled by preview Resolution (radius / downsample factor).
  - Draft 3D disables DOF (AE-CMP-409).
  - Edge cases: Aperture 0 or Blur Level 0 ⇒ no blur; zc ≤ 0 ⇒ not drawn.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Implementation hint: since each layer is a plane, compute per-pixel radius analytically from the plane equation; use a gather blur with variable radius (separable approximation for Fast Rectangle, disc/polygon kernel for other iris shapes); cap radius for previews.

### AE-CMP-422 · Iris shape and highlight (bokeh) controls (光圈形状和高光（译名待核）)

- **What it does:** Shapes the DOF blur kernel (bokeh) and boosts bright highlights inside the blur, mimicking real lens blur. UNVERIFIED: added in AE CS6 (11.0).
- **Exact semantics:**
  - Iris Shape enum (menu order, scripting value 1-based UNVERIFIED): Fast Rectangle (快速矩形) 1, Triangle (三角形) 2, Square (正方形) 3, Pentagon (五边形) 4, Hexagon (六边形) 5, Heptagon (七边形) 6, Octagon (八边形) 7, Nonagon (九边形) 8, Decagon (十边形) 9. Fast Rectangle uses a fast separable box approximation; others use a polygon kernel.
  - Iris Rotation rotates the polygon; Iris Roundness blends polygon → circle (100 % = circle); Iris Aspect Ratio stretches the kernel horizontally (>1) / vertically (<1) UNVERIFIED direction; Diffraction Fringe brightens the kernel rim.
  - Highlight Threshold: pixels with luminance ≥ threshold are treated as highlights; Highlight Gain boosts them before blurring (brighter bokeh discs); Highlight Saturation keeps their colour (0 = desaturated to white?) UNVERIFIED exact curves; same algorithm as the Camera Lens Blur effect (see effects.md, Blur & Sharpen).
- **Priority:** P2
- **Owner:** bae-render (+ bae-effects)
- **Notes:** Implementation hint: share the kernel code with the Camera Lens Blur effect.

### AE-CMP-423 · Active camera selection (活动摄像机)

- **What it does:** Chooses the one camera through which the comp is rendered at time t.
- **Exact semantics:**
  ```
  active_camera(comp, t) =
      first layer L in stacking order top->bottom such that
          L is a camera layer
          and L.enabled (Video switch on)
          and L.inPoint <= t < L.outPoint
      else default comp camera (AE-CMP-403)
  ```
  - Cameras can be cut between by trimming/sequencing camera layers; the switch is instantaneous at the frame boundary (no blending). With motion blur, sub-frame samples use the camera active at each sample time UNVERIFIED.
  - Shy, lock and label do not affect selection. UNVERIFIED: a camera layer is still used when other layers are soloed (solo only hides AV layers).
  - The Active Camera 3D view and every non-viewer render (nesting, Render Queue) use this camera.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-render)
- **Notes:** Expression `thisComp.activeCamera` returns this camera (or errors if none, see transform-keyframes-expressions.md, Comp object). Lottie players use the first camera layer.

### AE-CMP-424 · Cameras and lights in nested compositions (嵌套合成中的摄像机和光照)

- **What it does:** Determines which camera/lights render 3D layers inside a precomp.
- **Exact semantics:**
  - Precomp layer **without** Collapse Transformations: its 3D layers are rendered inside the precomp with the precomp's own active camera (or its default camera) and the precomp's lights; the result is a flat 2D image. If the precomp layer itself is 3D, that flat image is then placed as a single plane in the outer comp.
  - Precomp layer **with** Collapse Transformations and 3D switch on: its 3D layers are transformed by the precomp layer's transform and rendered with the **outer** comp's active camera and lights; cameras inside the precomp are ignored. UNVERIFIED: lights inside a collapsed precomp are also ignored (best-known behaviour).
  - Collapsed precomp whose 3D switch is off: the inner 3D layers are rendered with the inner comp's camera and flattened; the precomp acts as a 2D layer (breaks 3D groups).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Collapse semantics: see compositing-3d-color.md, Collapse transformations. Group splitting: AE-CMP-406.

### AE-CMP-425 · Focus distance commands (焦距命令)

- **What it does:** Layer > Camera menu commands that set or link the camera's Focus Distance.
- **Exact semantics:**
  | Command (中文) | Effect |
  |---|---|
  | Link Focus Distance to Point of Interest (将焦距链接到目标点) | adds expression `length(position, pointOfInterest)` UNVERIFIED exact text to Focus Distance of the selected camera |
  | Link Focus Distance to Layer (将焦距链接到图层) | needs one camera + one other layer selected; adds an expression computing the distance from the camera position to the other layer's world anchor point, e.g. `length(toWorld([0,0,0]), thisComp.layer("X").toWorld(thisComp.layer("X").anchorPoint))` UNVERIFIED exact text |
  | Set Focus Distance to Layer (将焦距设置为图层) | sets Focus Distance at the current time to that distance (adds a key if keyframed) |
- **Priority:** P1
- **Owner:** bae-motion
- **Notes:** Undoable as one command each.

### AE-CMP-426 · Comp Camera attribute for effects (合成摄像机属性（译名待核）)

- **What it does:** Certain effects (e.g. Card Dance, Shatter, Caustics, CC Sphere, Particle-type plug-ins) can read the comp's active camera and lights to render pseudo-3D on 2D layers.
- **Exact semantics:** The effect's "Camera System / Comp Camera" option makes the effect query the active camera (AE-CMP-423) and lights at the current time (position, orientation, zoom, light type/colour/intensity) in comp space. Plug-in SDK: `AEGP_GetEffectCameraMatrix` / camera-light access functions.
- **Priority:** P2
- **Owner:** bae-effects (+ bae-plugin)
- **Notes:** Effect-specific behaviour: see effects.md (Perspective, Simulation). BAE: expose `EvaluatedComp.active_camera()` and `lights()` to effects and plug-ins.

### AE-CMP-427 · Create Stereo 3D Rig (创建立体 3D 设备)

- **What it does:** Layer > Camera > Create Stereo 3D Rig (UNVERIFIED: since AE CS5.5) builds a stereo rig: a master camera, left/right eye cameras in precomps, a 3D Glasses output comp and a control null with Stereo Scene Depth, Convergence etc.
- **Exact semantics:** Generates comps/layers/expressions only (no new renderer feature). UNVERIFIED: exact controls (Stereo Scene Depth, Interaxial Separation, Convergence Of, Z Offset, 3D View, Enable Depth of Field, Swap Left-Right).
- **Priority:** P2
- **Owner:** bae-motion
- **Notes:** 3D Glasses effect: see effects.md, Perspective.

### AE-CMP-428 · Lottie camera layer (Lottie 摄像机图层)

- **What it does:** Export/import mapping of an AE camera to Lottie.
- **Exact semantics:**
  - Layer `ty: 13`; `pe` = perspective = Zoom (px), animatable; transform `ks.p` position, `ks.a` = Point of Interest (two-node; absent for one-node), `ks.or`, `ks.rx`, `ks.ry`, `ks.rz`. No DOF/iris data.
  - Players: lottie-web supports cameras only in the HTML renderer (CSS `perspective: pe px`, view matrix = inverse camera transform with y/z sign conventions), SVG/Canvas renderers ignore cameras; Skottie supports `ty 13` with the same projection.
  - Bodymovin exports the camera only when 3D layers exist (`ddd: 1`) UNVERIFIED. lottie-web `HCameraElement` reads `pe`, `ks.p` (or split `ks.p.x/y/z`), optional `ks.a`, `ks.or`, `ks.rx`, `ks.ry`, `ks.rz`; Lottie docs: "small values of `pe` yield a stronger perspective".
- **Priority:** P1
- **Owner:** bae-media (+ bae-anim)
- **Notes:** Lottie export settings in general: see ui-preview-render-io.md, Lottie export.

## Lights

### AE-CMP-429 · Light layer (光照图层)

- **What it does:** A non-rendering layer that illuminates 3D layers (with Accepts Lights on) and can make them cast shadows.
- **Exact semantics:**
  - Create: Layer > New > Light (Cmd+Opt+Shift+L / Ctrl+Alt+Shift+L) opens Light Settings (AE-CMP-430). Default name = light type name + index ("Spot Light 1", "聚光 1"); if the user never renamed it, changing the light type renames it ("Point Light 1").
  - Transform group: Point of Interest (目标点), Position (位置), Orientation, X/Y/Z Rotation. Default POI = (W/2, H/2, 0); default Position is offset from the comp centre up-left and toward the viewer (negative z). UNVERIFIED: exact default offsets (they scale with comp size). Ambient lights have no transform (no position or direction). Parallel and Spot lights orient towards the POI by default (Auto-Orient "Orient Towards Point of Interest"); Point lights ignore orientation.
  - All Light Options properties are animatable except Light Type and Casts Shadows.
  - Lights affect only 3D layers (and effects that read comp lights, AE-CMP-426). A comp with no enabled light renders 3D layers unlit at full colour; as soon as one light is enabled, lit layers receive only the light computed by AE-CMP-436 (dark where no light reaches, unless an Ambient light exists).
  - A light is enabled when its Video switch is on and t is within its in/out range. All enabled lights act together (no "active light" selection).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** matchName `ADBE Light Layer`; scripting `LayerCollection.addLight(name, centerPoint)`, `LightLayer.lightType` (`LightType.PARALLEL`, `SPOT`, `POINT`, `AMBIENT`, `ENVIRONMENT`), `LightLayer.lightSource` (AE 24.3+; any 2D layer since 25.2). Lottie: the Lottie docs define no light layer and lottie-web/Skottie render no lights. BAE Lottie export: bake nothing, warn. zh-CN help terms: 光照 (light), 聚光 1 (default name example).

### AE-CMP-430 · Light Settings dialog and Light Options (光照设置 / 光照选项)

- **What it does:** Dialog and Timeline property group defining light type, colour, intensity, cone, falloff and shadow parameters.
- **Exact semantics:** Fields in dialog order (Timeline group `ADBE Light Options Group` shows the same properties except Light Type):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Name (名称) | string | "<Type> Light N" | — | — | |
  | Light Type (光照类型) | enum | Spot UNVERIFIED (dialog remembers last used) | Parallel, Spot, Point, Ambient, Environment | — | AE-CMP-431; Environment: Advanced 3D only (scripting `LightType.ENVIRONMENT` since AE 24.3; UNVERIFIED: feature itself shipped with Advanced 3D in 24.0) |
  | Color (颜色) | color | white (1,1,1) | 0–1 per channel (overbright allowed in 32 bpc UNVERIFIED) | — | `ADBE Light Color` |
  | Intensity (强度) | float | 100 | unbounded; negative allowed | % | `ADBE Light Intensity`; negative = "nonlight", subtracts |
  | Cone Angle (锥形角度) | angle | 90 | 0–180 | ° (full cone angle) | Spot only; `ADBE Light Cone Angle` |
  | Cone Feather (锥形羽化) | float | 50 | 0–100 | % | Spot only; `ADBE Light Cone Feather 2` |
  | Falloff (衰减) | enum | None | None (无), Smooth (平滑), Inverse Square Clamped (反向平方限制) | — | Parallel/Spot/Point; `ADBE Light Falloff Type`; added in AE CS5.5 (10.5) |
  | Radius (半径) | float | 500 | ≥ 0 | px | `ADBE Light Falloff Start` |
  | Falloff Distance (衰减距离) | float | 500 | ≥ 0 | px | `ADBE Light Falloff Distance`; Smooth only |
  | Casts Shadows (投影) | bool (not animatable) | off | — | — | not for Ambient |
  | Shadow Darkness (阴影深度) | float | 100 | 0–100 | % | `ADBE Light Shadow Darkness` |
  | Shadow Diffusion (阴影扩散) | float | 0 | ≥ 0 UNVERIFIED max | px | `ADBE Light Shadow Diffusion` |
  - Inactive fields for the chosen type are greyed out in the dialog and hidden in the Timeline.
  - Changing Light Type in the dialog keeps all other values.
- **Priority:** P0 (P1: Falloff, shadows)
- **Owner:** bae-model (+ bae-ui-motion, bae-render)
- **Notes:** Expression light object: `intensity`, `color`, `coneAngle`, `coneFeather`, `shadowDarkness`, `shadowDiffusion`, `pointOfInterest` (see transform-keyframes-expressions.md, Light object). Default values marked here are the AE factory values; the dialog persists the last-used values in preferences.

### AE-CMP-431 · Light types (光照类型)

- **What it does:** Defines the direction and position dependence of each light type.
- **Exact semantics:** For a surface point P (comp world space) with unit normal N:
  ```
  Parallel (平行):  L = normalize(light.position - light.POI)    // direction only; position-independent
                    (with auto-orient off: L = -light local +z axis in world)
                    distance used for falloff = distance from P to the plane through light.position
                    perpendicular to L  UNVERIFIED
  Spot (聚光):      L = normalize(light.position - P); spot factor from cone (AE-CMP-432)
  Point (点):       L = normalize(light.position - P); omnidirectional
  Ambient (周围光):  no direction; adds color*intensity uniformly; never casts shadows; no viewer icon
  Environment (环境光, Advanced 3D only): image-based light from a 2D layer source
                    (lightSource: HDR/EXR in 24.3; any 2D layer since 25.2); P2
  ```
  Chinese UI (zh-CN help light-type list): Parallel 平行光, Spot 聚光, Point 点光, Ambient 周围光, Environment 环境光; the Light Settings description text uses the short forms 平行/聚光/点/环境 (译名待核: which form the dialog menu shows).
- **Priority:** P0 (Environment P2)
- **Owner:** bae-render
- **Notes:** —

### AE-CMP-432 · Spot light cone and feather (聚光锥形和羽化)

- **What it does:** Restricts a spot light to a cone around its axis with a soft edge.
- **Exact semantics:** measured in AE 26.2.1 by 04 (feather 0 / 50 / 100 %, exact to 0/255; crates/bae-render/tests/ae/README.md):
  ```
  axis   = normalize(light.POI - light.position)       // or local +z when not auto-oriented
  theta  = angle between axis and (P - light.position)
  outer  = ConeAngle / 2
  inner  = outer * (1 - ConeFeather/100)
  spot   = smoothstep(cos(outer), cos(inner), cos(theta))   // smoothstep in the cosine, not the angle
  ```
  - Feather 0 % ⇒ hard edge; 100 % ⇒ soft from the axis to the edge.
  - In AE CS6+ the selected spot light's cone wireframe extends to the POI (viewer only).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** "Control Light with Camera" links cone angle to camera zoom (AE-CMP-438).

### AE-CMP-433 · Light falloff (光照衰减)

- **What it does:** Attenuates Parallel, Spot and Point lights with distance.
- **Exact semantics:**
  ```
  d = distance(P, light.position)            // Parallel: see AE-CMP-431
  R = Radius, F = Falloff Distance
  None:                     atten = 1
  Smooth:                   atten = 1 - smoothstep(R, R + F, d)   // measured in AE 26.2.1 by 04 (exact; a linear ramp is off by 9/255)
  Inverse Square Clamped:   atten = 1                         if d <= R
                                  = (R / d)^2                 otherwise   (F unused)  // measured, exact
  ```
  - F = 0 with Smooth ⇒ hard cut at R. R = 0 with Inverse Square Clamped ⇒ atten = 0 everywhere except d = 0 UNVERIFIED (AE likely clamps R to a small minimum).
  - Falloff does not affect Ambient lights.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Falloff (Radius, Falloff Distance) was added in AE CS5.5 (10.5) (CS5.5 user manual, Light settings: "Radius … After Effects CS5.5 and later"); earlier projects load with Falloff = None. Help wording: Smooth = "smooth linear falloff starting at the Falloff Start radius and extending the length specified by Falloff Distance"; Inverse Square Clamped = "physically accurate falloff starting at the Falloff Start radius and decreasing proportionally to the inverse square of the distance". The Advanced 3D help recommends Inverse Square Clamped for physically plausible results with Adobe Standard Material.

### AE-CMP-434 · Light intensity, colour and negative light (光照强度、颜色和负光)

- **What it does:** Scales the light's contribution; negative intensity subtracts light ("nonlight").
- **Exact semantics:**
  - `light_rgb = Color.rgb * Intensity / 100`. Intensity > 100 % can push lit pixels above 1.0: clipped to 1.0 in 8/16 bpc after lighting; kept overbright in 32 bpc.
  - Negative intensity contributes negative diffuse/specular/ambient terms; the per-layer lit result is clamped at 0 (no negative colour), in all bit depths UNVERIFIED for 32 bpc.
  - Light colour multiplies the layer colour channel-wise in the diffuse/ambient terms (AE-CMP-436).
- **Priority:** P0
- **Owner:** bae-render (+ bae-color)
- **Notes:** In a linearized working space (AE-CMP-451) lighting is computed on linear values, giving physically plausible falloff; otherwise on gamma-encoded values (AE default).

### AE-CMP-435 · Light scope: adjustment-layer lights and Accepts Lights (光照作用范围)

- **What it does:** Limits which 3D layers a light affects.
- **Exact semantics:**
  - Per layer: a 3D layer is lit only if its Material Option Accepts Lights = On (AE-CMP-440). Accepts Lights does not affect shadows.
  - Per light: turning on the light layer's Adjustment Layer switch makes it affect only 3D layers **below** it in the Timeline stack; layers above it receive no light from it, regardless of 3D position. Without the switch, the light affects all 3D layers in the comp (in every 3D group, AE-CMP-406).
  - Lights do not illuminate 2D layers, and a light in comp A does not affect layers in a non-collapsed precomp (AE-CMP-424).
  - If every light is excluded by scope, the layer renders **unlit** (its own colour), measured in AE 26.2.1 by 04 (`light_scope`): the "no lights ⇒ unlit" test is per layer.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** —

### AE-CMP-436 · Classic 3D lighting model (经典 3D 光照模型)

- **What it does:** Computes the lit colour of each 3D layer pixel from layer colour, material options and all lights.
- **Exact semantics:** Adobe does not document the equation. Measured in AE 26.2.1 by 04 (crates/bae-render/tests/ae/README.md, errors 0–2/255): Lambert diffuse, **Blinn-Phong** specular with the half vector `H = normalize(L + V)`, exponent `n` = Specular Shininess in percent (5 % → 5), specular only where `N·L > 0`, `specColor = mix(white, C, Metal/100)`. AE's highlight peak is slightly below pure Blinn-Phong at high shininess (249/255 at 100 %). The block below is the original estimate; read `spec = … pow(max(0, dot(N, H)), n)` for the Phong term:
  ```
  // C = layer pixel colour (straight, un-premultiplied RGB), alpha unchanged by lighting
  // N = layer normal facing the camera (layers are double-sided: N flipped toward viewer)
  // V = unit vector from P to camera
  amb  = sum over Ambient lights a: a.rgb                                   // AE-CMP-434
  diff = sum over directional lights l: l.rgb * atten_l * spot_l * shadow_l * max(0, dot(N, L_l))
  spec = sum over directional lights l: l.rgb * atten_l * spot_l * shadow_l
                                        * pow(max(0, dot(R_l, V)), n)       // R_l = reflect(-L_l, N)
  n    = shininess exponent from Specular Shininess s (0..100 %):
         UNVERIFIED: n ≈ 2^(s/100 * 7) (1..128); 100 % = small tight highlight, 0 % = broad
  specColor = mix(white, C, Metal/100)                                      // Metal 100 % => highlight takes layer colour
  out.rgb = C * (Ambient/100 * amb + Diffuse/100 * diff)
          + Specular/100 * specColor * spec
  out.rgb = max(out.rgb, 0); clip to [0,1] unless 32 bpc
  ```
  - Unlit cases: Accepts Lights Off, or no enabled light in the comp ⇒ `out = C`.
  - Lighting happens after the layer's masks/effects (on the layer's 2D image) and before depth compositing; opacity and blend modes apply to the lit result.
  - Shadow factor `shadow_l` ∈ [0,1] from AE-CMP-437.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Implementation hint: evaluate per fragment in the 3D pass shader with a small uniform array of lights (AE projects rarely exceed 8 lights); BAE decision: verify n mapping and specular colour against AE golden renders before M1 and replace UNVERIFIED values with measured ones.

### AE-CMP-437 · Classic 3D shadows (经典 3D 阴影)

- **What it does:** Layers with Casts Shadows On block (or tint) light from shadow-casting lights onto layers with Accepts Shadows On in the same 3D group.
- **Exact semantics:**
  - Requires: light Casts Shadows = on (Parallel, Spot, Point; never Ambient); caster Material Casts Shadows = On or Only; receiver Accepts Shadows = On or Only; caster and receiver in the same 3D group (AE-CMP-406).
  - Algorithm: shadow maps rendered from each shadow light (perspective for Spot, cube/omni for Point, orthographic for Parallel UNVERIFIED projection details) at the Shadow Map Resolution (AE-CMP-408); the map stores caster depth and transmitted colour.
  - Measured in AE 26.2.1 by 04 (crates/bae-render/tests/ae/README.md): darkness, opacity and transmission act exactly as below with `casterCol` = the caster's own (unlit) colour and `blocked = alpha × opacity`; Shadow Diffusion behaves as a square area light of half-size = diffusion (linear penumbra of width `2 · diffusion · d(caster, receiver) / d(light, caster)`); at diffusion 0 the shadow-map edge is about one receiver pixel wide. Shadows remove the light's diffuse and specular terms, not ambient.
  - Shadow value per light at receiver point:
    ```
    occluded   = caster depth < receiver depth (with bias)
    casterCol  = caster colour * caster alpha
    trans      = LightTransmission/100                 // AE-CMP-440
    filter     = mix(black, casterCol, trans)           // coloured shadow (stained glass)
    blocked    = caster alpha * opacity  (0..1)        UNVERIFIED: opacity factor
    shadow_l   = 1 - (ShadowDarkness/100) * blocked * (1 - filter)   // per channel
    ```
  - Shadow Diffusion: softens the shadow edge with a blur whose radius grows with distance between caster and receiver (`radius ≈ ShadowDiffusion * dist(caster, receiver) / dist(light, caster)` UNVERIFIED); 0 px = hard shadow.
  - Continuously rasterized layers (text, shapes, collapsed comps) cast shadows from their source **before** effects (effects not seen in the shadow); precompose to include effects.
  - Shadows are not cast onto 2D layers or across groups; lights ignore groups only for illumination.
  - Draft 3D disables shadows.
  - Advanced 3D differs (P2): shadows from up to eight Spot/Parallel lights plus one Environment light; Shadow Diffusion is available only for Spot lights there (help "Enable lights to cast shadows").
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Implementation hint: since casters are planar quads, an exact analytic shadow (project caster quad onto receiver plane per light) is possible for hard shadows and avoids shadow-map gaps at intersections; BAE decision: use analytic projection where possible, shadow maps for many casters. Advanced 3D (AE 25.x) supports up to eight shadow-casting Spot/Parallel lights plus one Environment light — P2.

### AE-CMP-438 · Control Light with Camera (使用相机控制光线)

- **What it does:** Layer > Create > Control Light with Camera creates a camera linked to the selected light so the light follows the camera view (AE 25.x, 2025 UNVERIFIED version).
- **Exact semantics:** Creates a camera whose Position/POI drive the light by expressions (light = camera transform); for a Spot light the camera zoom is linked to the cone angle: `zoom = (W/2) / tan(ConeAngle/2)` UNVERIFIED. Looking through the camera shows the light's view.
- **Priority:** P2
- **Owner:** bae-motion
- **Notes:** —

### AE-CMP-439 · Environment light (环境光)

- **What it does:** Advanced-3D-only image-based light using a 2D layer (HDRI equirectangular) as the light source, providing lighting, reflections and optionally shadows.
- **Exact semantics:** `LightLayer.lightSource` = a 2D video/still/precomp layer in the same comp (24.3: HDR/EXR only; 25.2: any 2D layer); assigning a 3D layer errors "Invalid light source specified". Properties: Intensity, Color, Source, rotation via the light's Orientation/Rotation, Casts Shadows (one environment shadow), shadow settings UNVERIFIED full list. Ignored by Classic 3D.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** BAE decision: post-M1, together with Advanced 3D.

## Material Options

### AE-CMP-440 · Material Options: shadow and light switches (材质选项：投影、透光率、接受阴影、接受光照)

- **What it does:** Per-3D-layer switches deciding whether a layer casts/receives shadows, how much coloured light passes through it, and whether lights change its colour.
- **Exact semantics:** Group `ADBE Material Options Group` (Timeline shortcut AA), shown only on 3D layers. Properties in Timeline order:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Casts Shadows (投影) | enum, animatable (hold) | Off | Off (关), On (开), Only (仅限) | — | matchName `ADBE Casts Shadows` UNVERIFIED; toggle Off↔On: Opt+Shift+C / Alt+Shift+C |
  | Light Transmission (透光率) | float | 0 | 0–100 | % | `ADBE Light Transmission` |
  | Accepts Shadows (接受阴影) | enum, animatable (hold) | On | Off, On, Only | — | `ADBE Accepts Shadows` UNVERIFIED |
  | Accepts Lights (接受光照) | enum, animatable (hold) | On | Off, On | — | `ADBE Accepts Lights` UNVERIFIED |
  - Casts Shadows: Off = no shadow; On = layer visible and casts; Only = layer invisible (contributes nothing to the colour image) but still casts shadows (and transmits colour per Light Transmission).
  - Light Transmission: 0 % = opaque caster, shadow is black (scaled by Shadow Darkness); 100 % = the caster's colours are projected fully onto receivers (stained-glass). Formula in AE-CMP-437.
  - Accepts Shadows: Off = ignores shadows; On = normal; Only = the layer renders **only** the shadows falling on it (layer pixels transparent except where shadowed; shadow colour/darkness from the light), for shadow-catcher use. UNVERIFIED: exact alpha of the "Only" result = shadow strength × layer alpha.
  - Accepts Lights: Off = layer rendered unlit at its own colour even when lights exist (still receives shadows if Accepts Shadows On, UNVERIFIED for interaction). Help: Accepts Lights does not affect shadows.
  - Scripting enum values: Casts Shadows 0 = Off, 1 = On, 2 = Only (property value) UNVERIFIED numbering; Accepts Shadows 0 = Off, 1 = On, 2 = Only UNVERIFIED.
- **Priority:** P1 (Accepts Lights P0)
- **Owner:** bae-model (+ bae-render)
- **Notes:** In the Advanced 3D renderer the same switches appear for 3D model layers under "Compositing Options"; Accepts Shadows "Only" is disabled there (shadow catchers are a separate feature, P2). AE 25.1 (2024-12) added the Accepts Lights switch for Advanced 3D / 3D model layers (there it also removes shadows and highlights from lights); Classic 3D has had Accepts Lights all along. zh-CN help translates Material Options as 素材选项; the AE zh-CN UI string is believed to be 材质选项（译名待核）.

### AE-CMP-441 · Material Options: surface reflectance (材质选项：环境、漫射、镜面强度、镜面反光度、金属质感)

- **What it does:** Coefficients that weight the ambient, diffuse and specular terms of the lighting model.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Ambient (环境) | float | 100 | 0–100 | % | `ADBE Ambient Coefficient`; scales response to Ambient lights |
  | Diffuse (漫射) | float | 50 | 0–100 | % | `ADBE Diffuse Coefficient`; Lambert term |
  | Specular Intensity (镜面强度) | float | 50 | 0–100 | % | `ADBE Specular Coefficient` |
  | Specular Shininess (镜面反光度) | float | 5 | 0–100 | % | `ADBE Shininess Coefficient`; 100 % = small tight highlight, 0 % = large; active only if Specular > 0 |
  | Metal (金属质感) | float | 100 | 0–100 | % | `ADBE Metal Coefficient`; 100 % highlight = layer colour, 0 % = light colour |
  - Defaults 100/50/50/5/100 are the long-standing AE factory values (documented in ProVideo Coalition's lighting series; the current Adobe help page lists no defaults).
  - The three lighting terms add, so Diffuse + Specular + Ambient contributions can exceed 100 % and blow out (clipped in 8/16 bpc).
  - Use in the lighting equation: AE-CMP-436. Out-of-range values are clamped by the UI; expressions can produce values outside 0–100, which AE clamps at render UNVERIFIED.
  - Adobe tip (Advanced 3D materials help): to make a 3D layer match its 2D look as closely as possible, set Ambient and Diffuse to 100 and Specular Intensity, Specular Shininess and Metal to 0.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Expression attributes `ambient`, `diffuse`, `specular`, `shininess`, `metal`, `lightTransmission`, `castsShadows`, `acceptsShadows`, `acceptsLights` on 3D layers (see transform-keyframes-expressions.md, Layer 3D attributes). In Advanced 3D these map to Adobe Standard Material: Ambient ≈ emissive-like (does not light others), Diffuse → Base Color weight, Specular Intensity → specular level, Specular Shininess → 1 − roughness (non-linear), Metal → metallic-like colour tint.

### AE-CMP-442 · Renderer-specific material and geometry options (渲染器专属材质和几何选项)

- **What it does:** Extra 3D layer properties that only some renderers use.
- **Exact semantics:**
  - Ray-traced 3D only (removed renderer): Appears in Reflections (Off/On/Only), Reflection Intensity (`ADBE Reflection Coefficient`), Reflection Sharpness (`ADBE Glossiness Coefficient`), Reflection Rolloff (`ADBE Fresnel Coefficient`), Transparency (`ADBE Transparency Coefficient`), Transparency Rolloff (`ADBE Transp Rolloff`), Index of Refraction (`ADBE Index of Refraction`).
  - CINEMA 4D / Advanced 3D: Geometry Options group — for footage/solids: Curvature (`ADBE Plane Curvature`), Segments (`ADBE Plane Subdivision`); for text/shape: Bevel Style, Bevel Depth (`ADBE Bevel Depth`), Hole Bevel Depth (`ADBE Hole Bevel Depth`), Extrusion Depth (`ADBE Extrsn Depth`); Advanced 3D adds Substance materials, 3D model compositing options and shadow catchers.
  - Classic 3D ignores all of the above (stored, not rendered).
- **Priority:** P2
- **Owner:** bae-model (+ bae-render)
- **Notes:** BAE decision: persist all of these for round-trip; implement extrusion/bevel with Advanced 3D work (post-M1).

## Color depth and color management

### AE-CMP-443 · Project color depth (项目颜色深度 / 每通道位数)

- **What it does:** Sets the per-channel precision of every intermediate image in the project: 8, 16 or 32 bits per channel (bpc).
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Depth (位深度 / 颜色深度) | enum | 8 bits per channel | 8 bits per channel, 16 bits per channel, 32 bits per channel (float) | bpc | scripting `Project.bitsPerChannel` ∈ {8,16,32} |
  - Project-wide (one value for all comps); changed in File > Project Settings > Color, or by Alt/Opt-clicking the bpc indicator at the bottom of the Project panel (cycles 8 → 16 → 32).
  - Render Settings can override per render item ("Color Depth: Current Settings / 8 / 16 / 32 bpc", see ui-preview-render-io.md, Render settings).
  - Changing depth never alters stored property values; it changes rendering results (clamping, precision, blend-mode maths in 32 bpc).
- **Priority:** P0
- **Owner:** bae-color (+ bae-render, bae-model)
- **Notes:** Storage of the setting: see project-comp-layers.md, Project settings. BAE decision: BAE may internally always render in f16/f32 for speed and correctness, but must emulate the 8/16-bpc clamping points (AE-CMP-444) when the project depth is 8 or 16 so results match AE.

### AE-CMP-444 · Pixel encoding and clamping per bit depth (各位深度的像素编码与钳制)

- **What it does:** Defines numeric pixel representation and where values are clamped.
- **Exact semantics:**
  ```
  8 bpc  : integer 0..255 per channel; white = 255
  16 bpc : integer 0..32768 per channel; white = 32768  (15 bits + 1, "16-bit" in AE/PS sense)
  32 bpc : IEEE float per channel; white = 1.0; values < 0 and > 1 allowed (HDR / over-range)
  normalised value v = int / 255 (8), int / 32768 (16), float (32)
  premultiplied alpha internally in all depths (see compositing-3d-color.md, Alpha model)
  ```
  - 8 and 16 bpc: every layer render, effect output, blend result, light result, colour property value at use time is clamped to [0, 1] (rounded to the integer grid). Over-range values from footage (EXR, HDR) are clipped at import conversion.
  - 32 bpc: no clamping in 32-bit-capable effects, blend modes, lighting, motion blur, transforms; clamping happens only at an 8/16-bpc effect (AE-CMP-445), at output to an integer format, and at display (unless HDR display, AE-CMP-460).
  - Alpha: clamped to [0,1] in all depths, including 32 bpc. UNVERIFIED: AE clamps alpha in 32 bpc after every blend.
  - Rounding when quantising to 8 bpc: round-to-nearest (`round(v*255)`); 16 bpc: `round(v*32768)`.
- **Priority:** P0
- **Owner:** bae-color (+ bae-render)
- **Notes:** Plug-in SDK: 16-bpc channel max = `PF_MAX_CHAN16` = 32768, 8-bpc = 255 (see ARCHITECTURE §15). Implementation hint: render in rgba16float/rgba32float and insert `saturate()` at the AE clamp points when project depth < 32; quantise to 8/16 at the same points only when bit-exact matching is requested.

### AE-CMP-445 · Mixed bit-depth effects and conversion warnings (效果位深度转换与警告)

- **What it does:** Handles effects that do not support the project's bit depth.
- **Exact semantics:**
  - Each effect declares supported depths (8, 16, 32); see effects.md, Effect framework, and the bpc column of the matchName table.
  - In a 16-bpc project an 8-bpc-only effect receives its input quantised to 8 bpc; output is converted back to 16 bpc (precision loss, banding).
  - In a 32-bpc project an effect without 32-bpc support receives input converted to 16 bpc if it supports 16, else 8 bpc: values clamped to [0,1] (over-range and negative values lost) and quantised; output converted back to float.
  - UI warning: the Effect Controls panel shows a yellow warning triangle next to the effect name when the effect's max depth is below the project depth (tooltip states the supported depth); the Effects & Presets panel can be set to list only the effects that support the current project depth (help); UNVERIFIED: exact menu strings of those filters.
  - Effects later in the chain after a clamping effect cannot recover over-range data.
- **Priority:** P1
- **Owner:** bae-effects (+ bae-color, bae-ui-motion)
- **Notes:** BAE decision: all built-in BAE effects run in float; the clamp/quantise is emulated only for effects whose AE counterpart is not 32-bpc capable, behind a per-project "AE bit-depth fidelity" switch (on by default for imported AE projects). Plug-in host must perform the conversion for third-party 8/16-bpc plug-ins (ARCHITECTURE §15).

### AE-CMP-446 · Dithering on conversion to 8 bpc (转换到 8 bpc 时的仿色)

- **What it does:** Adds dither noise when higher-depth colour is converted to 8 bpc to reduce banding.
- **Exact semantics:** Applied when rendering/exporting from a 16/32-bpc project to an 8-bpc output format and when an 8-bpc-only effect receives higher-depth input. Not applied for previews/viewers. Dither pattern: UNVERIFIED (ordered/blue-noise, amplitude ±0.5 LSB of 8 bpc).
- **Priority:** P2
- **Owner:** bae-color (+ bae-media)
- **Notes:** Help tip: forcing dither in previews is possible by adding a do-nothing 8-bpc effect (e.g. Arithmetic). BAE decision: also dither previews optionally (better than AE); off by default for parity.

### AE-CMP-447 · Colour values in 32 bpc: colour properties, picker and Info readouts (32 bpc 下的颜色值与拾色器)

- **What it does:** Defines how colour parameters store and edit values, including over-range.
- **Exact semantics:**
  - Colour properties (effect colour, solid colour, fill/stroke, light colour) store RGBA as floats 0..1 nominal (`[r,g,b,a]` in scripting/expressions). Expressions may return values outside 0..1; they are kept and clamped only per AE-CMP-444.
  - Colour Picker in 8/16 bpc: RGB edited as 0–255 (8) or 0–32768 (16) integers, plus HSB and hex. In a 32-bpc project the RGB fields show floats (0.0–1.0 nominal) and accept values > 1.0 (over-range) UNVERIFIED: whether values < 0 are accepted.
  - Picked colours are interpreted in the working colour space (switching to a linear/different working space keeps numbers, so appearance changes).
  - Info panel readout formats (Decimal, Percent, 8-bpc, 10-bpc, 16-bpc, Web; 32-bpc as float), see ui-preview-render-io.md, Info panel.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-color, bae-model)
- **Notes:** —

### AE-CMP-448 · Color engine and enabling color management (颜色引擎)

- **What it does:** Turns colour management on and selects the engine: ICC-based Adobe engine or OpenColorIO.
- **Exact semantics:** Project Settings > Color > Color Management:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Color Engine (颜色引擎) | enum | Adobe color managed | Adobe color managed, OCIO color managed | — | OCIO engine shipped in AE 23.2 (2023-02; beta from 23.1); OCIO library 2.5.1 with ACES 2.0 configs in AE 26.5 |
  | Working Color Space (工作色彩空间) | enum/profile | None (unmanaged) UNVERIFIED: factory default for new projects | AE-CMP-449 / AE-CMP-459 | — | `Project.workingSpace` ("" = None) |
  - Adobe engine: colour management is ON iff Working Color Space ≠ None. With None, footage RGB numbers enter compositing unchanged (except Interpret As Linear Light, AE-CMP-455, and Working Gamma handling of colour-managed video formats, AE-CMP-450) and output is unchanged.
  - Selecting OCIO always manages colour (working space chosen from the config).
- **Priority:** P0 (Adobe ICC with None/sRGB/Rec.709), P2 (OCIO)
- **Owner:** bae-color (+ bae-model)
- **Notes:** Before the OCIO engine (≤ AE 23.1) there was no engine menu; Working Space alone controlled CM (choosing a Color Engine "turns color management on", help). Sources: https://helpx.adobe.com/after-effects/using/color-management.html; CG Channel "Adobe releases After Effects 23.2" (2023-02-08). Project settings storage: see project-comp-layers.md, Project settings.

### AE-CMP-449 · Working color space (Adobe engine) (工作色彩空间)

- **What it does:** The common RGB space (primaries + white point + tone curve) in which all footage is converted and all compositing happens.
- **Exact semantics:**
  - Menu lists ICC profiles installed on the system plus AE's bundled ones. Common entries: None; sRGB IEC61966-2.1; HDTV (Rec. 709); HDTV (Rec. 709) 16-235; Rec.709 Gamma 2.4 (AE 22.x+ UNVERIFIED version); SDTV NTSC; SDTV PAL; Adobe RGB (1998); ProPhoto RGB; DCI-P3 / P3-D65 variants; Rec.2020 / Rec.2100 PQ / Rec.2100 HLG (HDR, AE 24.x+ UNVERIFIED); Kodak/film printing density profiles (Cineon only unless "Show All Available Profiles").
  - Conversion pipeline (Adobe engine, per pixel, 32-bit float internally):
    ```
    footage RGB --(input profile -> PCS (XYZ D50), rendering intent: relative colorimetric + BPC)--> working RGB
    working RGB --(composite)--> working RGB
    working RGB --(output profile)--> file RGB        // AE-CMP-456
    working RGB --(monitor profile [+ simulation])--> display RGB   // AE-CMP-457/458
    ```
  - If footage has no embedded profile and no rule, it is assumed to already be in the working space (no conversion).
  - All profiles used in a project are embedded in the project file.
- **Priority:** P0 (sRGB, Rec.709, None), P1 (others)
- **Owner:** bae-color
- **Notes:** BAE decision: implement matrix/curve ICC profiles (RGB, parametric or LUT-1D TRC + 3×3) natively in bae-color; full LUT-based ICC (CMYK, film printing density) P2. No OpenColorIO runtime dependency (AGENTS.md forbids large runtimes): implement an "OCIO subset" (built-in ACES/sRGB/Rec.709/Rec.2020 transforms) instead.

### AE-CMP-450 · Working gamma when unmanaged (工作伽玛（译名待核）)

- **What it does:** With Working Color Space = None, chooses whether colour-managed video formats are interpreted for a display gamma of 2.2 or 2.4.
- **Exact semantics:** `Project.workingGamma` ∈ {2.2, 2.4} (any other value is a scripting error); ignored when a working space is set (scripting guide). UNVERIFIED semantics: media with embedded colour info (QuickTime/H.264/HEVC/ProRes tagged Rec.709) is converted from its tagged space to a pure gamma 2.2 or 2.4 display space with Rec.709 primaries; 2.4 keeps Rec.709 video numbers unchanged, 2.2 brightens shadows slightly. Default UNVERIFIED (2.4 for new projects since AE 22.x).
- **Priority:** P2
- **Owner:** bae-color
- **Notes:** Related legacy option "Match Legacy After Effects QuickTime Gamma Adjustments" (AE-CMP-462).

### AE-CMP-451 · Linearize Working Space (线性化工作色彩空间)

- **What it does:** Composites in linear light: the working space keeps its primaries and white point but uses a gamma-1.0 tone curve, so all operations (blending, resampling, blur, motion blur, antialiasing, lighting) act on linear values.
- **Exact semantics:**
  - Available only when colour management is on. `Project.linearizeWorkingSpace` (scripting since AE 16.0). Help recommends combining it with 16 or 32 bpc.
  - Footage converted to the linearised working space: `v_lin = TRC_working^-1(v)` (AE-CMP-461 formulas) after profile conversion.
  - Colour property values (picker colours, solid colours) are interpreted as already linear-space values (numbers unchanged ⇒ appearance changes when toggling). UNVERIFIED: AE may instead convert picker colours so they keep their appearance; verify against AE.
  - Display/output convert from linear working space through the monitor / output profile.
  - Recommended 16 or 32 bpc; in 8 bpc linear storage bands dark tones (AE allows it).
  - Mutually exclusive in effect with Blend Colors Using 1.0 Gamma (that option becomes irrelevant/disabled when linearized UNVERIFIED).
  - Output simulation presets "Legacy Macintosh RGB" and "Internet Standard RGB" are unavailable when linearized.
- **Priority:** P1
- **Owner:** bae-color (+ bae-render)
- **Notes:** Implementation hint: BAE renders internally in linear float when linearize is on; with linearize off, keep gamma-encoded working values so effects and blend modes match AE numerically.

### AE-CMP-452 · Blend Colors Using 1.0 Gamma (使用 1.0 灰度系数混合颜色)

- **What it does:** Performs only layer-to-layer blending (blend modes, opacity, track-matte/motion-blur/frame-blend accumulation) in linear light, while effects and footage stay in the gamma-encoded working space.
- **Exact semantics:**
  ```
  // per blend of layer src over dst (both in working space, gamma-encoded)
  g = working space gamma (CM on: working TRC; CM off: gamma 2.2  UNVERIFIED for 2.4 working gamma)
  src_l = linearize(src.rgb); dst_l = linearize(dst.rgb)
  out_l = blend_mode(src_l, dst_l, opacity, alpha)        // see compositing-3d-color.md, Blend modes
  out.rgb = encode(out_l)
  ```
  - Works with colour management off (uses gamma 2.2) and on (uses the working space's curve). `Project.linearBlending`.
  - Affects opacity fades, motion blur, frame blending (Pixel Motion/Frame Mix), antialiased edges between layers, and all blend modes; does not affect effects, masks' feather inside a layer, or resampling within a layer.
  - Ignored when Linearize Working Space is on (already linear).
- **Priority:** P1
- **Owner:** bae-color (+ bae-render)
- **Notes:** —

### AE-CMP-453 · Compensate for Scene-referred Profiles (补偿场景引用的配置文件)

- **What it does:** Applies an automatic contrast (image-state) adjustment when converting between scene-referred profiles (e.g. HDTV Rec.709, SDTV) and output-referred profiles (e.g. sRGB, monitor).
- **Exact semantics:** Project Settings > Color checkbox, default On. `Project.compensateForSceneReferredProfiles` (AE 16.0+). Adjustment is based on HDTV-standard gamma values: UNVERIFIED model — scene-referred Rec.709 OETF (≈1/1.9) to output-referred display (2.2) implies a system gamma ≈ 1.16; with compensation on, AE adds that end-to-end gamma so that Rec.709-tagged video converted to sRGB keeps its expected appearance (RGB numbers almost unchanged). Off = strict colorimetric conversion (Rec.709 video looks lighter/flatter on sRGB).
- **Priority:** P1
- **Owner:** bae-color
- **Notes:** BAE decision: implement as an extra 1D power on luminance-preserving RGB per channel between scene- and output-referred transfer functions; calibrate exponent against AE renders (currently UNVERIFIED).

### AE-CMP-454 · Input color: media color space and embedded profiles (输入颜色：媒体色彩空间)

- **What it does:** Determines how each footage item's pixels convert into the working space.
- **Exact semantics:** Interpret Footage > Main > Color tab (fields stored per footage item; UI: see project-comp-layers.md, Interpret Footage):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Media Color Space (媒体色彩空间) | read-only | from file (embedded ICC / CICP / codec metadata) | — | — | shown "None" if untagged |
  | Override Media Color Space (覆盖媒体色彩空间) | profile/enum | (none: use embedded) | any profile / OCIO colour space | — | assigns input profile |
  | Preserve RGB (保留 RGB) | bool | off | — | — | AE-CMP-455 |
  | Interpret As Linear Light (解释为线性光) | enum | Off UNVERIFIED (On for 32-bpc float files) | Off, On, On for 32 bpc | — | AE-CMP-455 |
  - Profile precedence: Override > interpretation-rules file (`interpretation rules.txt`) > embedded profile/metadata > assume working space.
  - Embedded profiles are read for PSD, TIFF, PNG, JPEG; CICP (ITU-T H.273) codes for PNG, MOV, and in ICC; colour-managed video formats: QuickTime ProRes, H.264/HEVC, MXF (XAVC/AVC), camera RAW (ARRI, Sony, RED, ProRes RAW, Canon).
  - Non-RGB sources (CMYK, Y'CbCr, camera raw) are converted to RGB by the decoder; no profile assignable before that.
  - Cineon/DPX log: use Cineon Settings or film profiles (see project-comp-layers.md, Interpret Footage).
- **Priority:** P0 (sRGB/Rec.709 embedded/override), P1 (rules file, CICP)
- **Owner:** bae-color (+ bae-media, bae-model)
- **Notes:** —

### AE-CMP-455 · Preserve RGB and Interpret As Linear Light (保留 RGB 与解释为线性光)

- **What it does:** Per-footage escape hatches: skip profile conversion, or treat the data as linear.
- **Exact semantics:**
  - Preserve RGB: pixel numbers are passed into the working space unchanged (no conversion), appearance not preserved; intended for data images (displacement maps, normal maps, mattes).
  - Interpret As Linear Light: treats the assigned input profile as gamma 1.0. With CM on: footage converted from linear(profile) to working (or linear working). With CM off: footage values are encoded with gamma 2.2 (or working gamma) before compositing: `v_out = v^(1/2.2)` UNVERIFIED exponent so linear EXRs look correct in an unmanaged 2.2 project. Option "On for 32 bpc" applies it only in 32-bpc projects.
  - Both work per footage item; Interpret As Linear Light also works with CM off.
- **Priority:** P1
- **Owner:** bae-color
- **Notes:** —

### AE-CMP-456 · Output module color management (输出模块色彩管理)

- **What it does:** Converts the rendered comp from the working space into the output file's colour space.
- **Exact semantics:** Output Module Settings > Color tab:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Output Color Space / Output Profile (输出色彩空间) | profile | Working Color Space | any profile / OCIO space | — | requires CM on |
  | Preserve RGB (保留 RGB) | bool | off | — | — | no conversion; numbers written as-is |
  | Convert To Linear Light (转换为线性光) | enum | On for 32 bpc (help: default because linear output is seldom a good idea at 8/16 bpc) | Off, On, On for 32 bpc | — | output encoded with gamma 1.0 of the output profile |
  | Embed Profile / Embed color space (嵌入色彩空间) | bool | on for formats that support it UNVERIFIED | — | — | PSD, PNG, TIFF, JPEG (ICC); CICP for PNG/MOV HDR |
  - Rendering intent chosen automatically: relative colorimetric with black-point compensation; absolute colorimetric for film-negative output profiles.
  - Multiple output modules on one render item may each use a different profile.
  - HDR10 metadata for PNG (Mastering Display Color Volume primaries/luminance, content light level) when output space is PQ (AE 25.x UNVERIFIED).
- **Priority:** P0 (profile conversion), P1 (linear light, embed), P2 (HDR10 metadata)
- **Owner:** bae-color (+ bae-media)
- **Notes:** Output module UI and formats: see ui-preview-render-io.md, Render Queue / Output modules.

### AE-CMP-457 · Display color management (显示色彩管理)

- **What it does:** Converts viewer pixels from the working space to the monitor's profile so colours look correct; per viewer, preview only.
- **Exact semantics:**
  - Toggle per viewer (Composition, Layer, Footage): View > Use Display Color Management or Show Channel and Color Management Settings button > Use Display Color Management; shortcut Shift + / on the numeric keypad (Win), Control+Option+/ (Mac) (help). Default On when CM is enabled. A yellow plus sign on the button indicates it is on.
  - On: `working → monitor profile` (OS display profile; HDR displays get extended range, AE-CMP-460). Off: working RGB numbers sent directly to the display (RGB numbers preserved; appearance not).
  - Never changes rendered data or cached frames' working-space content (BAE: apply in the final viewer shader).
  - Turning it off also disables output simulation (whose setting is remembered).
- **Priority:** P0
- **Owner:** bae-color (+ bae-ui-motion)
- **Notes:** Implementation hint: final present pass = (optional exposure) → (optional simulation) → working→display 3×3 + TRC in one shader; use the OS colour profile via a platform crate (macOS ColorSync / Windows WCS) or the swap-chain's colour space (EDR on macOS).

### AE-CMP-458 · Simulate Output (模拟输出)

- **What it does:** Previews how the comp will look after output to a different device or after a non-colour-managed display; per viewer, preview only.
- **Exact semantics:**
  - View > Simulate Output menu: No Output Simulation (无输出模拟), Legacy Macintosh RGB (Gamma 1.8), Internet Standard RGB (sRGB), Kodak 5218 To Kodak 2383, additional HDTV/SDTV presets UNVERIFIED, Custom… The selection is per viewer; Custom settings are shared by all views.
  - Pipeline:
    ```
    1. working -> output profile                     (as the real render would)
    2. if Preserve RGB: reinterpret numbers as simulation profile (no conversion)
       else: output profile -> simulation profile    (colorimetric)
    3. simulation profile -> monitor profile
    ```
  - Requires display colour management on. "Legacy Macintosh RGB" and "Internet Standard RGB" (non-CM app simulations, gamma 1.8 / 2.2 with Preserve RGB) are unavailable with Linearize Working Space.
  - Toggle between none and last-used simulation from the Show Channel and Color Management Settings menu.
- **Priority:** P2
- **Owner:** bae-color (+ bae-ui-motion)
- **Notes:** To bake a simulation into a render, Adobe recommends two Color Profile Converter effects on an adjustment layer (see effects.md, Utility).

### AE-CMP-459 · OCIO color management (OCIO 色彩管理)

- **What it does:** Uses OpenColorIO configs (ACES) for working, media, display and output colour spaces.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | OCIO Configuration (OCIO 配置) | enum/file | bundled ACES config (ACES 1.x configs since 23.2 — trade press reports ACES 1.2 and 1.3 presets; the current help names the default "ACE 2" UNVERIFIED meaning; ACES 2.0 CG v4.0 / Studio v4.0 added in 26.5 with OCIO 2.5.1) | bundled configs, Custom (load .ocio) | — | |
  | Working Color Space (工作色彩空间) | OCIO colour space | config default (ACEScg UNVERIFIED) | spaces of the config | — | linear scene-referred |
  | Display Color Space (显示色彩空间) | display/view | config default (sRGB – ACES SDR view UNVERIFIED) | displays × views of the config | — | also switchable from a menu at the bottom of the Composition panel (OCIO mode only) |
  - Media colour space defaults from the config's file rules; override per footage in Interpret Footage (AE-CMP-454); output colour space per output module (AE-CMP-456); "Show all" lists all spaces and display/view transforms.
  - OCIO effects (OCIO File Transform, OCIO Color Space Transform, OCIO Display Transform) work without OCIO project mode: see effects.md, Utility.
- **Priority:** P2
- **Owner:** bae-color
- **Notes:** BAE decision: no OpenColorIO library (size/runtime rule). Ship built-in analytic transforms for an "ACES subset" (ACEScg, ACES2065-1, ACEScct, sRGB/Rec.709/Rec.2020/P3-D65 encodings, ACES 1.x/2.0 output transforms as baked 3D LUTs generated at build time) and read simple custom configs later (P2).

### AE-CMP-460 · HDR working spaces and HDR display preview (HDR 工作空间与 HDR 预览)

- **What it does:** Supports HDR (PQ/HLG) footage, compositing and preview on HDR displays.
- **Exact semantics:**
  - Requires 32 bpc and an HDR-capable working space (e.g. Rec.2100 PQ, Rec.2100 HLG) with the Adobe engine; footage tagged via CICP/profiles converts in; output to Rec.2100 PQ writes CICP metadata (PNG, MOV).
  - Viewer: with Use Display Color Management on and an OS-level HDR display, values above SDR white are shown with extended range (AE 25.x, 2025; beta 25.2x72); without HDR display they are clipped/tone-mapped at display. Mercury Transmit to an HDR monitor sends an HDR signal.
  - Graphics white for PQ/HLG variants: selectable variants of the colour spaces with specific graphics white levels (e.g. 203 nits) UNVERIFIED list.
- **Priority:** P2
- **Owner:** bae-color (+ bae-media, bae-ui-motion)
- **Notes:** Implementation hint: wgpu surface with `Rgba16Float` + extended-sRGB/PQ colour space (macOS EDR, Windows scRGB).

### AE-CMP-461 · Transfer functions and primaries bae-color must implement (传递函数与基色)

- **What it does:** Exact formulas for the tone curves and matrices used by the profiles above.
- **Exact semantics:** (`V` = encoded 0..1, `L` = linear 0..1 relative to reference white)
  ```
  sRGB IEC61966-2.1:
    decode: L = V / 12.92                       if V <= 0.04045
            L = ((V + 0.055) / 1.055) ^ 2.4     otherwise
    encode: V = 12.92 * L                       if L <= 0.0031308
            V = 1.055 * L^(1/2.4) - 0.055       otherwise
  Rec.709 / Rec.2020 OETF (scene-referred, HDTV (Rec. 709) profile):
    a = 1.09929682680944, b = 0.018053968510807
    encode: V = 4.5 * L                         if L < b
            V = a * L^0.45 - (a - 1)            otherwise
    decode: inverse (L = V/4.5 if V < 4.5*b else ((V + a - 1)/a)^(1/0.45))
  Rec.709 Gamma 2.4 (BT.1886, black = 0):  L = V^2.4 ; V = L^(1/2.4)
  Pure gamma 2.2 (unmanaged blending, Internet Standard RGB sim): L = V^2.2
  Gamma 1.8 (Legacy Macintosh RGB): L = V^1.8
  PQ (SMPTE ST 2084): m1=2610/16384, m2=2523/4096*128, c1=3424/4096, c2=2413/4096*32, c3=2392/4096*32
    L_abs(cd/m^2) = 10000 * ( max(V^(1/m2) - c1, 0) / (c2 - c3*V^(1/m2)) )^(1/m1)
  HLG (BT.2100): a=0.17883277, b=1-4a, c=0.5-a*ln(4a)
    OETF: V = sqrt(3E) if E <= 1/12 else a*ln(12E - b) + c
  16-235 ("video range") variants: V_full = (V_code*255 - 16) / 219 (8-bit), applied before the curve
  Primaries / white (xy):
    sRGB = Rec.709: R(0.640,0.330) G(0.300,0.600) B(0.150,0.060) W D65 (0.3127,0.3290)
    Rec.2020/2100: R(0.708,0.292) G(0.170,0.797) B(0.131,0.046) W D65
    DCI-P3: R(0.680,0.320) G(0.265,0.690) B(0.150,0.060) W (0.314,0.351); P3-D65 same primaries, W D65
    Adobe RGB (1998): R(0.640,0.330) G(0.210,0.710) B(0.150,0.060) W D65, gamma 563/256 ≈ 2.19921875
    ACES AP0 (ACES2065-1): R(0.7347,0.2653) G(0,1) B(0.0001,-0.0770) W (0.32168,0.33767)
    ACES AP1 (ACEScg): R(0.713,0.293) G(0.165,0.830) B(0.128,0.044) W (0.32168,0.33767)
  ICC PCS = XYZ D50; chromatic adaptation via Bradford (as in ICC v4 profiles).
  ```
  - Negative / > 1 values in 32 bpc: curves are extended by odd symmetry (`f(-x) = -f(x)`) for negatives and by continuing the power segment above 1. UNVERIFIED: AE's ACE handling of out-of-range values in float mode (believed unbounded for matrix/TRC profiles).
- **Priority:** P0 (sRGB, Rec.709 variants, gamma 2.2/2.4), P2 (PQ, HLG, ACES)
- **Owner:** bae-color
- **Notes:** These are the published standard formulas; Adobe's ICC profiles may approximate curves with sampled LUTs (e.g. AE's "HDTV (Rec. 709)" profile) — validate against AE renders and record any deviation.

### AE-CMP-462 · Match Legacy After Effects QuickTime Gamma Adjustments (匹配旧版 After Effects QuickTime 灰度系数调整（译名待核）)

- **What it does:** Legacy project option reproducing the gamma shifts AE applied to QuickTime footage/output before CS5.5-era fixes.
- **Exact semantics:** Project Settings > Color checkbox (default off for new projects, on for projects from AE ≤ CS5 UNVERIFIED). When on, QuickTime footage with certain codecs is interpreted/rendered with a 1.8 ↔ 2.2 gamma adjustment. UNVERIFIED whether still present in AE 24+.
- **Priority:** P2
- **Owner:** bae-color (+ bae-media)
- **Notes:** BAE decision: read the flag from imported projects; warn instead of emulating.

### AE-CMP-463 · Color management for nested comps, Dynamic Link and video preview (嵌套合成、动态链接与视频预览的色彩管理)

- **What it does:** Clarifies where colour conversions do *not* happen.
- **Exact semantics:**
  - Nested comps: no conversion between comps — every comp is in the single project working space.
  - Dynamic Link / Media Encoder: when CM is on, frames are converted working → Rec.709 as the last step (Dynamic Link assumes Rec.709 input).
  - Mercury Transmit (external video preview): colour-managed, treating the device as HDTV Rec.709 (HDR displays per AE-CMP-460).
  - Color Profile Converter effect performs explicit conversions inside a comp (see effects.md, Utility).
- **Priority:** P2
- **Owner:** bae-color
- **Notes:** —

## Preview rendering

### AE-CMP-464 · Resolution / Down Sample Factor (分辨率/向下采样系数)

- **What it does:** Renders a comp at a fraction of its pixels (every Nth column and Mth row) for speed; stored per comp, used by previews and (by default) by final renders.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Resolution (分辨率) | enum | Full (Auto in the viewer menu for new comps UNVERIFIED) | Auto (自动, previews only), Full (完全), Half (二分之一), Third (三分之一), Quarter (四分之一), Custom (自定义) | — | Composition Settings and viewer menu; scripting `CompItem.resolutionFactor` = [x, y] |
  | Custom horizontal / vertical (自定义) | int, int | 1, 1 | 1–99 UNVERIFIED max | "render every N pixels" | x and y independent |
  ```
  (fx, fy) = (1,1) Full | (2,2) Half | (3,3) Third | (4,4) Quarter | (N,M) Custom
  buffer size  = (ceil(W / fx), ceil(H / fy))          // UNVERIFIED rounding (ceil vs floor)
  render transform: comp space -> buffer space = scale(1/fx, 1/fy)
  every layer, mask, effect and 3D projection renders directly at the reduced size
    (not render-then-downscale); effects receive downsample_x = 1/fx, downsample_y = 1/fy
    and must scale their spatial parameters (blur radius, offsets) by it
  viewer displays the buffer scaled by (fx, fy) * zoom, nearest-neighbour UNVERIFIED (blocky look)
  ```
  - Nested comps render at the parent's factor unless the nested comp has "Preserve Resolution When Nested" (see compositing-3d-color.md, Nested comp resolution and frame rate).
  - Render Settings > Resolution can override for output (Current Settings / Full / Half / Third / Quarter / Custom), see ui-preview-render-io.md, Render settings.
  - Layer viewer resolution follows its comp's viewer.
  - Shortcuts: Full Cmd/Ctrl+J, Half Cmd/Ctrl+Shift+J, Quarter Cmd/Ctrl+Opt/Alt+Shift+J, Custom Cmd/Ctrl+Opt/Alt+J (see ui-preview-render-io.md, Shortcuts).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** Plug-in SDK `in_data->downsample_x/y` (rational) are the effect-side contract (ARCHITECTURE §15). Cache keys must include the factor. Implementation hint: GPU render targets allocated at reduced size; all shaders get a `px_scale` uniform.

### AE-CMP-465 · Auto resolution (自动分辨率)

- **What it does:** Picks the down-sample factor from the viewer zoom so no more pixels than visible are rendered (viewer only).
- **Exact semantics:**
  ```
  z = max zoom among the comp's open views (1.0 = 100 %)
  if z >= 1:            factor = 1 (Full)
  else:                 factor = largest integer n with 1/n >= z   // 50% -> 2 (Half), 33.3% -> 3, 25% -> 4
                        UNVERIFIED: whether n is limited to {1,2,3,4} or any integer (e.g. 12.5% -> 8 as Custom)
  ```
  - The menu shows the effective value, e.g. "Auto (Quarter)".
  - Ignored (treated as Full) for comps with Preserve Resolution When Nested.
  - Never used by Render Queue (Auto ⇒ Full for output).
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** BAE decision: allow non-integer factors internally for HiDPI viewers (render at exactly displayed size) as an optional "faster than AE" mode, defaulting to AE's integer behaviour.

### AE-CMP-466 · Region of Interest (目标区域)

- **What it does:** Restricts preview rendering to a user-drawn rectangle of the comp/layer/footage; optionally crops the comp or output to it.
- **Exact semantics:**
  - ROI rectangle in comp (or layer/footage) pixel coordinates: left, top, right, bottom (Info panel shows T, L, B, R from the upper-left corner). Stored per viewer/comp UNVERIFIED (per comp, shared by its viewers; scripting none).
  - Toggle Region of Interest button: on = render only pixels inside the ROI (everything outside is not rendered, shown as pasteboard/empty); off = full frame; Alt/Opt-click the button to draw a new ROI.
  - Effects still sample source pixels outside the ROI as needed (ROI limits the output rect, not inputs). Cached frames are keyed by ROI.
  - Composition > Crop Comp to Region of Interest: comp Width/Height become the ROI size and all layers are shifted by (−left, −top) so content stays in place (one undoable command). UNVERIFIED: whether it shifts layer positions or anchor via comp-origin offset (AE shifts layer positions of top-level layers and keyframes).
  - Output Module > Crop > Use Region of Interest crops the rendered output to the ROI (see ui-preview-render-io.md, Output modules).
  - Render Queue ignores the ROI otherwise.
- **Priority:** P1
- **Owner:** bae-render (+ bae-motion, bae-ui-motion)
- **Notes:** bae-motion owns the Crop Comp to Region of Interest command. Implementation hint: ROI = scissor rect + render-graph request rect; combined with Resolution it gives the minimal pixel set.

### AE-CMP-467 · Fast Previews modes (快速预览)

- **What it does:** Chooses a speed/quality trade-off used while interacting (dragging, scrubbing) and for viewer playback.
- **Exact semantics:** Fast Previews menu (Composition panel), shortcuts Cmd/Ctrl+Opt/Alt+1…5:
  | Mode (中文) | Behaviour |
  |---|---|
  | Off (Final Quality) (关（最终品质）) | always full quality at the comp's Resolution |
  | Adaptive Resolution (自适应分辨率) | during interaction, renders at a lower factor (down to the Adaptive Resolution Limit preference: 1/2, 1/4, 1/8, 1/16; default 1/8 UNVERIFIED) to keep interactive speed; full quality when interaction stops |
  | Draft (草图) | Ray-traced 3D comps only: ray-tracing quality reduced to 1 ray. Still documented in the current help, but meaningless since Ray-traced 3D was removed (AE 17.0) UNVERIFIED: whether the menu item still exists |
  | Fast Draft (快速绘图) | OpenGL/GPU layout preview: scene downsampled to 1/4 resolution for texture upload, effects and track mattes on; in Ray-traced comps it rendered bevels/extrusion/curvature. Still documented in the help, but CG Channel reports the AE 18.0 real-time Draft 3D preview (AE-CMP-409) replaced it UNVERIFIED: whether the menu item remains |
  | Wireframe (线框) | draws layer bounding boxes (and 3D layer outlines through the camera) instead of pixels |
  - If an interaction frame takes too long in Off/Adaptive modes, AE temporarily shows wireframes until the mouse stops.
  - The renderer button's lightning bolt turns orange in Draft/Fast Draft/Wireframe, and in Adaptive Resolution while downsampled; the mode name appears at the viewer's top-right.
  - Live Update (实时更新) off ⇒ wireframes during interaction; Alt/Opt while dragging toggles it temporarily; Caps Lock suspends viewer refresh (red bar). These are UI behaviours: see ui-preview-render-io.md, Preview & cache behaviour.
  - Adaptive Resolution limit: Preferences > Previews (help). Newer AE replaces it with an "Adaptive Resolution Priority" slider (minimum resolution; lower values allow more aggressive down-sampling) plus a 3D renderer reduction choice UNVERIFIED: version and exact option names; see ui-preview-render-io.md, Preferences. Adaptive Resolution also applies to 3D (Advanced 3D) rendering in current AE (help).
  - Ray-traced legacy: Adaptive Resolution at 1/2 halved ray-tracing quality, at 1/4 capped it at 4, at 1/8 or 1/16 capped it at 2 (historical, P2).
  - Never affects Render Queue output.
- **Priority:** P1 (Off, Adaptive Resolution, Wireframe); P2 (others)
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** BAE decision: BAE should rarely need Adaptive Resolution; keep the mode for parity and as a fallback on weak GPUs.

### AE-CMP-468 · Transparency grid and composition background color (透明网格与合成背景颜色)

- **What it does:** Controls what the viewer shows where the comp is transparent, and what fills transparency in alpha-less output.
- **Exact semantics:**
  - Background Color is a comp setting (default black (0,0,0); see project-comp-layers.md, Composition Settings). Viewer with Toggle Transparency Grid off: `display = comp_premul + (1 − alpha) * background`. On: same with a checkerboard instead of the background colour (grid square size and two colours: preferences, AE 24.x+; default light/dark grey 16-px squares UNVERIFIED).
  - Background colour is never part of the comp's alpha: when the comp is nested, the background is transparent; Render Queue output with an alpha channel (RGB+Alpha) leaves it transparent.
  - Output with RGB only: transparent areas are composited over the comp background colour UNVERIFIED (best-known: premultiplied colour shown over background colour, i.e. same formula as the viewer).
  - Layer and Footage viewers have their own grid toggle.
- **Priority:** P0
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** Scripting `CompItem.bgColor`; `Project.transparencyGridThumbnails` for Project-panel thumbnails.

### AE-CMP-469 · Show Channel views (显示通道)

- **What it does:** Shows a single channel, alpha, or un-premultiplied colour in a viewer for inspection (viewer only).
- **Exact semantics:** Show Channel and Color Management Settings menu (显示通道及色彩管理设置), channel part, in order:
  | Item (中文) | Display formula (comp pixel premultiplied `c`, alpha `a`) |
  |---|---|
  | RGB | normal (over grid/background per AE-CMP-468) |
  | Red (红色) / Green (绿色) / Blue (蓝色) | grey = straight channel value? UNVERIFIED: AE shows the premultiplied channel `c.r` as grey, over black |
  | Alpha (Alpha) | grey = `a` (black transparent, white opaque) |
  | RGB Straight (RGB 直通) | `c.rgb / a` for a > 0, opaque; undefined (whatever is stored, typically black) where a = 0 |
  | Alpha Boundary (Alpha 边界) / Alpha Overlay (Alpha 叠加) | Layer panel only (Roto Brush) — outline / tinted overlay of the alpha |
  | Colorize (彩色化（译名待核）) | toggle: single-channel views shown in that channel's colour instead of grey |
  - Then the colour-management items: Use Display Color Management, Simulate Output (AE-CMP-457/458).
  - Alt/Opt-click the button toggles between Alpha and RGB. Shortcuts Alt/Opt+1..4 (R, G, B, Alpha; Shift adds Colorize) UNVERIFIED.
  - Channel views are applied after exposure and before display colour management? UNVERIFIED order; best-known: channel extraction on working values, then display transform. Straight-colour views (RGB Straight, Alpha Overlay, Alpha Boundary) ignore the Viewer Quality preference and render as if it were "Faster" (help).
  - 32 bpc: channel values > 1 are clipped at display unless HDR.
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** Implementation hint: a final viewer shader with a channel-select uniform; never touches cached frames.

### AE-CMP-470 · Viewer exposure (调整曝光度)

- **What it does:** Per-viewer preview-only exposure adjustment in f-stops for inspecting HDR/overbright and shadow detail.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Adjust Exposure (调整曝光度) | float | 0 | UNVERIFIED: −40 … +40 | stops (f-stops) | Reset Exposure button (blue when ≠ 0) toggles 0 ↔ last non-zero value |
  ```
  // applied in linear light, like the Exposure effect's Exposure parameter  (UNVERIFIED for CM-off projects)
  lin  = to_linear(working_rgb)      // working TRC; CM off: gamma 2.2 UNVERIFIED; linearized WS: identity
  lin *= 2 ^ exposure
  out  = from_linear(lin)            // then channel view, display CM
  ```
  - Each viewer has its own value (help); never affects output; persisted with the project per viewer UNVERIFIED. Help usage: drag far left/right ("gamma slamming") to check composite matching and find black/white points. zh-CN help section title: 调整预览曝光度.
  - Useful at 8/16 bpc too (shows clipping), but only 32 bpc preserves above-white detail.
- **Priority:** P1
- **Owner:** bae-render (+ bae-color, bae-ui-motion)
- **Notes:** Exposure effect (final output): see effects.md, Color Correction.

### AE-CMP-471 · Viewer pixel aspect ratio correction (像素长宽比校正)

- **What it does:** Displays non-square-pixel comps/footage stretched to square-pixel proportions on the computer monitor (preview only).
- **Exact semantics:** Toggle Pixel Aspect Ratio Correction button per viewer; when on, display width = buffer width × PAR (e.g. PAR 2.0 anamorphic shows twice as wide); scaling quality per Zoom Quality preference (Faster / More Accurate Except Cached Preview / More Accurate). Rendering and output are unaffected.
- **Priority:** P2
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** PAR in compositing math: see compositing-3d-color.md, Quality and sampling; comp PAR setting: project-comp-layers.md, Composition Settings.

### AE-CMP-472 · Viewer Quality preferences: Zoom Quality and Color Management Quality (查看器质量：缩放品质与色彩管理品质)

- **What it does:** Preferences > Previews choose the speed/accuracy of the two display-only operations a viewer applies on top of rendered frames: zoom/PAR scaling and display colour conversion.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Zoom Quality (缩放品质) | enum | UNVERIFIED: More Accurate Except Cached Preview | Faster (更快), More Accurate Except Cached Preview (除缓存预览之外更准确), More Accurate (更准确) | — | quality of viewer magnification and of Pixel Aspect Ratio Correction scaling (AE-CMP-471) |
  | Color Management Quality (色彩管理品质) | enum | UNVERIFIED: More Accurate Except Cached Preview | same three values | — | precision of the working → display (and simulation) conversion (AE-CMP-457, AE-CMP-458) |
  - "Except Cached Preview": the accurate path is used for still frames/interaction, the faster path during cached (RAM) preview playback.
  - Straight-colour channel views (RGB Straight, Alpha Overlay, Alpha Boundary; AE-CMP-469) ignore both preferences and behave as Faster.
  - Never affects rendered frames, caches or output.
- **Priority:** P2
- **Owner:** bae-render (+ bae-color, bae-ui-motion)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/previewing.html (Viewer Quality preferences). BAE decision: BAE's display transform runs in a GPU shader at full precision always, so "Faster" and "More Accurate" may be identical in BAE; keep the stored preference for UI parity. Preferences dialog layout: see ui-preview-render-io.md, Preferences.
