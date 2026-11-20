# After Effects inventory: effects (AE-FX)

**Scope.** This file specifies the AE effect framework and every built-in effect BAE must provide. The framework covers the effect stack and its render order, enable switches, effect instance data, Compositing Options (Effect Opacity, Mask Reference), parameter types and how each one animates, parameter IDs and match names, bit-depth support (8/16/32 bpc), GPU flags, output-bound expansion, downsample scaling, layer parameters, Reset / Remove All, Animation Presets and legacy effect versions. For each built-in effect it gives the exact matchName, category, bpc and GPU support, every parameter in AE order (match name, type, default, range, units), an implementable algorithm, whether it expands bounds, and the Lottie effect `ty` where one exists. Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md, ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

**Conventions used in this file.**

- Colours are normalised: 0.0 = black, 1.0 = white in every bit depth (8 bpc 255, 16 bpc 32768 = 1.0; see compositing-3d-color.md, Pixel and alpha model). `R, G, B, A` are straight (unpremultiplied) channel values of the effect's input pixel; `R', G', B', A'` the output.
- `lerp(a, b, t) = a + (b - a) * t`; `clamp01(x) = min(max(x, 0), 1)`; `luma709(c) = 0.2126 R + 0.7152 G + 0.0722 B`; `luma601(c) = 0.299 R + 0.587 G + 0.114 B`.
- "bpc" and "GPU" values for each effect come from the docsforadobe first-party effect match-name table (columns "BPC" and "GPU": AE version in which GPU acceleration was added). An empty GPU column means CPU only.
- Parameter tables use the columns `| # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |`. `#` is the parameter's 1-based property index inside the effect group (`effect.property(#)`), which is also the numeric suffix of the parameter match name: AE gives every built-in effect parameter the match name `"<effect matchName>-NNNN"` (4 digits, 1-based, counted over the effect's parameter list including group start markers, see AE-FX-007). Where a parameter match name is written with "UNVERIFIED", the index was derived from the documented UI order and not checked against an AE project.
- Range is written "slider a…b; valid c…d". The slider range is what dragging in Effect Controls covers; the valid range is what typing or expressions can reach (values outside valid are clamped).
- "Expands bounds: yes/no" says whether the effect can draw outside the input layer rectangle (PF_OutFlag_I_EXPAND_BUFFER, AE-FX-017).

## Effect framework

### AE-FX-001 · Effect stack and effect render order (效果堆栈和渲染顺序)

- **What it does:** Every AV layer (footage, solid, comp, shape, text, adjustment, null) owns an ordered list of effect instances, the Effects group; effects process the layer's image one after another, top to bottom.
- **Exact semantics:**
  - Container: layer property group `"ADBE Effect Parade"` (display name "Effects", 效果). Effect instances are its children, index 1 = top. Camera, light and audio-only layers have no Effects group (audio layers get audio effects only; see effects.md, Audio effects).
  - Per-layer render order (full pipeline in compositing-3d-color.md, Render order): layer source (footage frame / rasterised vector / nested comp) → masks (mask paths, feather, expansion, modes; applied to the layer's alpha) → effects in stack order, index 1 first → layer styles → transform (with motion blur) → track matte → blend with layers below.
  ```
  img = render_source(layer, t)
  img = apply_masks(img, layer.masks, t)          // unless an effect references masks via Compositing Options
  for fx in layer.effects (index 1..n):
      if layer.fx_switch && fx.enabled:
          out = fx.render(img, params_at(fx, t), ctx)
          img = composite_options(img, out, fx, t)  // AE-FX-005/006
  img = apply_layer_styles(img)
  ```
  - Reordering an effect (drag in Effect Controls or Timeline) changes its index and therefore the result; there is no automatic sorting.
  - Each effect receives the output of the effect above as its input layer `param[0]` (SDK: "All effects automatically have at least 1 layer parameter, param[0], the layer to which they are applied").
  - Adding an effect inserts it at the bottom of the stack when nothing is selected; when an effect is selected in Effect Controls the new effect is inserted below it (UNVERIFIED: AE 2015+ behaviour; older versions always appended at the bottom).
- **Priority:** P0 — the basis of every effect.
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Scripting: `layer.property("ADBE Effect Parade")` (= `layer.Effects`, `layer.effect`), `addProperty(matchName)` appends an effect; `PropertyBase.moveTo(newIndex)` reorders. Expressions: `effect("Name")` or `effect(index)` (see transform-keyframes-expressions.md, effect object). Lottie: layer `ef` array, each entry `{ty, nm, mn, ix, en, ef:[values]}`; lottie-web only renders a few effect types (Tint 20, Fill 21, Stroke 22, Tritone 23, Pro Levels 24, Drop Shadow 25, Set Matte 28, Gaussian Blur 29, ...) and treats others (including `ty: 5` custom/expression-control effects) as data only. Implementation hint: build each layer's effect chain as a linear sub-graph of the render graph; adjacent pixel-independent effects (PF_OutFlag_PIX_INDEPENDENT, e.g. Levels → Curves → Tint) can be fused into one WGSL pass.

### AE-FX-002 · Apply, duplicate, copy/paste and delete effects (应用、复制、粘贴和删除效果)

- **What it does:** Basic editing commands on effect instances: apply from Effects & Presets / Effect menu, duplicate, copy/paste between layers, delete, rename.
- **Exact semantics:**
  - Apply: drag from Effects & Presets onto a layer (Timeline, Composition viewer or Effect Controls), double-click in Effects & Presets with layers selected (applies to every selected layer), or choose from the Effect menu (category submenus). If no layer is selected, double-clicking an animation preset creates a new layer; double-clicking an effect does nothing (UNVERIFIED for effects; documented for presets).
  - Effect > [last applied effect] (Ctrl+Alt+Shift+E / Cmd+Opt+Shift+E) re-applies the most recently applied effect to the selected layers.
  - Duplicate (Ctrl/Cmd+D) on a selected effect creates a copy directly below it with all keyframes, expressions and Compositing Options; the copy gets a numbered name ("Blur 2").
  - Paste of an effect onto a layer: if the target layer already has exactly one instance of that effect (same matchName) and the clipboard holds one instance, the paste replaces the existing instance's values instead of adding a new one; once a layer has two or more instances of that effect, paste adds a new instance (Adobe help, Effects and animation presets overview). Pasted keyframes start at the current time (keyframe paste rules: see transform-keyframes-expressions.md, copy/paste keys).
  - Delete: select effect, press Delete/Backspace; keyframes and expressions of that effect are removed with it. Expressions elsewhere that referenced the effect by name then error (see transform-keyframes-expressions.md, expression errors).
  - Rename: select effect, press Enter/Return, type the new name. The name must be unique among the layer's effects; AE appends " 2", " 3"... to automatically created duplicates. Default name = the effect's display name in the UI language.
  - Effect instance names default to the localised display name, so expression `effect("Slider Control")` written in an English AE fails in a localised AE unless the instance was renamed or the expression uses the match name / index (UNVERIFIED: AE's expression engine also accepts the English display name in localised versions; behaviour differs between versions).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** All of these are bae-model Commands (undoable, one history step each). "Undo Delete Effect" / "Undo Remove All Effects" restore keyframes too. Effects & Presets panel and Effect Controls UI: see ui-preview-render-io.md, Effect Controls and Effects & Presets panels.

### AE-FX-003 · Effect switch and layer Effects switch (效果开关和图层效果开关)

- **What it does:** Each effect instance has an on/off switch; each layer has an "fx" switch that disables all of its effects at once.
- **Exact semantics:**
  - Per-effect switch (Effect Controls / Timeline, left of the effect name; drawn as "fx" icon, an eyeball icon in the AE 2025+ modernised Effect Controls): when off, the effect is skipped entirely (its input passes through unchanged), its parameters still evaluate and its keyframes are kept. Scripting: `PropertyBase.enabled` on the effect group; Lottie `en: 0|1`.
  - Layer Effects switch (Switches column, "fx", 效果): when off, every effect of that layer is skipped for preview and render. Scripting: `AVLayer.effectsActive` (boolean). Not animatable.
  - Render Settings > Effects (Current Settings / All On / All Off) overrides the switches at render time (see ui-preview-render-io.md, Render Queue).
  - Expression Controls effects are never rendered regardless of the switch, but their values still evaluate for expressions even when the effect switch is off (UNVERIFIED: AE returns the parameter value from a disabled effect; it does).
  - Neither switch is keyframeable; to fade an effect in time animate Effect Opacity (AE-FX-005) or effect parameters.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** Disabling an effect on an adjustment layer leaves the adjustment layer acting as an empty adjustment (no change below). BAE decision: a disabled effect must be removed from the render-graph node chain (not rendered as identity) so it costs nothing.

### AE-FX-004 · Effect instance data (效果实例数据)

- **What it does:** Defines what is persisted for each effect instance in a layer's effect stack.
- **Exact semantics:**
  | Field | Type | Notes |
  |---|---|---|
  | matchName | string | Locale-independent identifier of the effect type, e.g. `"ADBE Easy Levels2"`. Scripting `PropertyBase.matchName`. Lottie `mn`. |
  | name | string | Instance name, user-editable; default = localised display name (`PropertyBase.name`, Lottie `nm`). |
  | enabled | bool | AE-FX-003. |
  | version | (major, minor, bug, stage, build) | The effect plug-in version that created the instance (SDK `PF_VERSION`, stored in the project). AE compares it on load and may run the effect's parameter upgrade path. |
  | parameters | ordered list | One property per parameter, keyed by parameter ID (AE-FX-007), each with static value or keyframes and an optional expression. |
  | compositing options | group | AE-FX-005, AE-FX-006. |
  | sequence data | opaque bytes | Per-instance private state of the effect (flattened sequence data); built-in BAE effects normally keep none. |
  | label / selection | UI only | Not persisted per effect except selection state. |
  - Effects have no in/out times of their own; an effect is active for the whole layer duration.
  - Scripting: `effect.isEffect === true`, `effect.propertyIndex`, `effect.canSetEnabled`. `app.effects` lists installed effects with `displayName`, `matchName`, `category`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-plugin)
- **Notes:** BAE decision: store `matchName` verbatim for AE effects so AE projects and Lottie round-trip; BAE-native effects use reverse-DNS style IDs and may map to an AE matchName. Unknown matchNames are kept as placeholders (AE-FX-026). Cross-ref ARCHITECTURE §14 (plugin manifest/registry) and §15 (AE plug-in compatibility, sequence data slot).

### AE-FX-005 · Compositing Options: Effect Opacity (合成选项：效果不透明度)

- **What it does:** Every effect (AE CS6 11.0 and later) has a Compositing Options group whose Effect Opacity blends the effect's output with its input, like a per-effect "Blend With Original".
- **Exact semantics:**
  - Group `"ADBE Effect Built In Params"` (合成选项, Compositing Options), appended after the effect's own parameters. Child `"ADBE Effect Mask Opacity"` = Effect Opacity (效果不透明度): percent, default 100%, valid 0…100%, keyframeable, expression-capable.
  ```
  // in = input of this effect, fx = effect output, o = effectOpacity/100, m = mask coverage (AE-FX-006; 1 when no references)
  w = o * m
  out = lerp(in, fx, w)       // per channel including alpha, computed on premultiplied values
  ```
  - UNVERIFIED: the blend is done on premultiplied colour (equivalent to a "normal" crossfade of the two images); for effects that change alpha (e.g. Set Matte) this yields alpha = lerp(in.a, fx.a, w).
  - At o = 100% and no mask references the effect output is used unchanged and no extra pass is needed. At o = 0 the effect is effectively bypassed (but still rendered by AE).
  - Expression Controls effects have no Compositing Options (they produce no pixels). Audio effects have none.
- **Priority:** P1 — used regularly for quick effect fades; trivial to implement.
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Scripting path: `effect.property("ADBE Effect Built In Params").property("ADBE Effect Mask Opacity")` (confirmed by an Adobe community answer, 2025). Many older effects additionally have their own "Blend With Original" parameter (与原始图像混合), which is applied inside the effect before Compositing Options. Implementation hint: fold `lerp` into the effect's own shader as a uniform `mix` when the effect is a single pass; skip it when w == 1.

### AE-FX-006 · Compositing Options: Mask References (合成选项：蒙版引用)

- **What it does:** Restricts an effect to the area of one or more masks on the same layer, without those masks cutting the layer's alpha.
- **Exact semantics:**
  - Under Compositing Options, "+" adds a mask reference, "−" removes the last one. Each reference has:
    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Mask Reference N (蒙版引用 N) | popup | None (无) | None, then every mask on this layer by name (Mask 1, Mask 2, ...) | — | Dynamic list; renaming a mask updates the menu. |
    | Inverted (反转) | checkbox | off | — | — | UNVERIFIED: per-reference invert checkbox next to the menu. |
  - Coverage: each referenced mask is rasterised with its own path, feather (incl. per-vertex feather), expansion, opacity and mode; references are combined in their order using the masks' modes (Add, Subtract, Intersect, Lighten, Darken, Difference; None contributes nothing), exactly as layer masks combine (compositing-3d-color.md, Masks). Result `m(x,y) ∈ [0,1]`, used in AE-FX-005: `out = lerp(in, fx, effectOpacity * m)`.
  - A mask that is referenced by any effect's Compositing Options does not cut the layer's alpha (UNVERIFIED: AE treats referenced masks as effect-only when their mode is None; with any other mode the mask also applies to the layer — users set the mask mode to None to use it purely as an effect mask). Adobe help: "An effect mask cannot be used to modify the alpha channel of a layer."
  - Mask Opacity scales the effect intensity inside that mask. Only masks on the same layer can be referenced; references to a deleted mask revert to None.
  - Version: Compositing Options and Mask Reference added in AE CS6 (11.0, 2012).
- **Priority:** P1
- **Owner:** bae-render (+ bae-model, bae-vector)
- **Notes:** UNVERIFIED match names: references group `"ADBE Effect Mask Parade"`, each reference `"ADBE Effect Mask"` with `"ADBE Effect Path Stream Ref"` (menu) and `"ADBE Effect Mask Invert"` (checkbox). Scripting cannot set the referenced mask directly in AE (known limitation). Implementation hint: reuse the mask coverage rasteriser of the layer-mask pass and feed it as a texture into the per-effect mix shader.

### AE-FX-007 · Parameter identity: indices, disk IDs and parameter match names (参数标识：索引、磁盘 ID 和匹配名称)

- **What it does:** Defines how each effect parameter is identified in projects, scripts, expressions and plug-ins.
- **Exact semantics:**
  - Index: parameters are numbered 1..n in UI order (`effect.property(1)` = first visible parameter). The SDK numbers them 0..n with param[0] = input layer; SDK index k ↔ property index k.
  - Group start markers (topics) occupy an index; the matching group end does not appear as a property (UNVERIFIED: scripting sees the topic as a PropertyGroup; its children continue the effect-level numbering in match names).
  - Parameter match name: `"<effect matchName>-NNNN"`, NNNN = 4-digit zero-padded disk ID, e.g. Slider Control's slider = `"ADBE Slider Control-0001"`, Levels (Individual Controls) Red Gamma = `"ADBE Pro Levels2-0013"` (UNVERIFIED index). For built-in effects the disk ID equals the 1-based index at the time the effect was designed; later-added parameters get new IDs appended even if they are displayed earlier (so disk ID ≠ display index for some effects, e.g. parameters added in newer versions). Expressions and scripts that address by match name survive localisation; by display name they do not.
  - Disk ID (SDK `PF_ParamDef.uu.id`) is the stable key under which AE stores the parameter value in the project; a plug-in can reorder parameters in the UI while keeping disk IDs (SDK "Changing parameter orders").
  - Display name = localised UI string (e.g. "Gamma" / "灰度系数"); `PropertyBase.name` returns it.
- **Priority:** P0 — required for AE project import, Lottie (`mn`) and expressions.
- **Owner:** bae-model (+ bae-plugin, bae-anim)
- **Notes:** Lottie effect values carry `mn` (parameter match name), `nm`, `ix` (= index) and `ty` (value type, AE-FX-008..013). BAE decision: every BAE effect parameter has a permanent numeric ID (= AE disk ID for AE-compatible effects) and a match name derived from it; renaming or reordering parameters never changes the ID. See ARCHITECTURE §15 ("参数有稳定的数字 ID（对应 AE 的 disk ID）").

### AE-FX-008 · Numeric parameters: slider, float slider and angle (数值参数：滑块、浮点滑块和角度)

- **What it does:** One-dimensional numeric effect parameters shown as an underlined scrubbable value with an optional slider.
- **Exact semantics:**
  | Type | SDK | Value | Animation | Notes |
  |---|---|---|---|---|
  | Slider (integer, legacy) | `PF_Param_SLIDER` | long | linear/bezier/hold | "No longer used"; values rounded to integers. |
  | Fixed slider | `PF_Param_FIX_SLIDER` | 16.16 fixed | linear/bezier/hold | Deprecated; precision = 1/65536. |
  | Float slider | `PF_Param_FLOAT_SLIDER` | double | linear/bezier/hold | `valid_min/valid_max`, `slider_min/slider_max`, `precision` (decimal places shown), display flags (percent, pixel), optional exponential curve tolerance. |
  | Angle | `PF_Param_ANGLE` | degrees (16.16 fixed; float in expressions) | linear/bezier/hold | Dial UI shown as `Nx +D°` (revolutions + degrees); multiple revolutions allowed (value may exceed 360 or be negative). |
  - Slider range vs valid range: dragging the slider covers [slider_min, slider_max]; typing/scrubbing the number accepts [valid_min, valid_max]; expression results are clamped to the valid range (UNVERIFIED: AE clamps expression results to valid range for effect sliders; it does for most).
  - Right-click > Edit Value... opens a dialog where the user can change the slider range for that instance (stored per instance) within the valid range.
  - Units flags: pixels (value scaled by downsample factor at render time, AE-FX-018), percent (displayed with "%", stored as the displayed number, e.g. 50 not 0.5), degrees.
  - Graph editor and velocity work like any 1D property (transform-keyframes-expressions.md, Keyframes and interpolation).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Lottie value `ty: 0` (Slider) and `ty: 1` (Angle), value in `v` (animatable property with `a`, `k`). Expression `effect("X")(n).value` returns a Number.

### AE-FX-009 · Checkbox and popup parameters (复选框和弹出菜单参数)

- **What it does:** Discrete parameters: on/off checkbox and enumerated popup menu.
- **Exact semantics:**
  - Checkbox (`PF_Param_CHECKBOX`): value 0/1 (expressions return `0`/`1`; scripting returns `true/false`... UNVERIFIED: scripting `value` returns 1/0 for effect checkboxes). `PF_ParamFlag_CANNOT_INTERP` is forced on: keyframes are always Hold. Expression results are thresholded: non-zero → on (UNVERIFIED rounding rule: AE treats value ≥ 0.5 as on? report: any non-zero is on).
  - Popup (`PF_Param_POPUP`): value = 1-based item index (A_long). Items are fixed at effect definition ("Entry1|Entry2|..."; "(-" draws a separator, which still consumes an index). Keyframes are Hold only. Expression results are rounded to the nearest integer and clamped to [1, n] (UNVERIFIED rounding).
  - Lottie: checkbox `ty: 4`, popup/dropdown `ty: 7`; values stored as numbers (popup 1-based).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Popup menu text is localised; projects store only the index, so BAE must keep AE's item order exactly as given in each effect item below. Dropdown Menu Control (AE-FX-035) is the only popup whose items the user edits.

### AE-FX-010 · Color parameter (颜色参数)

- **What it does:** RGB colour chosen with the colour picker or eyedropper.
- **Exact semantics:**
  - `PF_Param_COLOR`: stored as RGB (alpha unused; AE always reports alpha = 1). Plug-ins read 8-bit `PF_Pixel` or float values through `PF_ColorParamSuite1`.
  - Expressions/scripting see a 4-D array `[r, g, b, a]` in 0…1 (a = 1). Float colours may exceed 1.0 when set by expression in 32 bpc projects (UNVERIFIED: the picker itself caps at 1.0 unless HDR picker; AE 2022+ allows values > 1 in 32 bpc).
  - Interpolates linearly per channel in the stored (non-linear, working-space) RGB values; no hue-space interpolation. Supports linear/bezier/hold (1D-style temporal ease shared by all channels).
  - Eyedropper samples the composition viewer at full precision of the comp (in 8 bpc projects 1/255 steps).
- **Priority:** P0
- **Owner:** bae-model (+ bae-color, bae-ui-motion)
- **Notes:** Lottie `ty: 2`, value `[r,g,b,a]` in 0…1. With a linearised working space, AE converts colour parameters from the picker's display space to working space (see compositing-3d-color.md, Color management).

### AE-FX-011 · Point and 3D Point parameters and effect control points (点和三维点参数及效果控制点)

- **What it does:** 2D (x, y) or 3D (x, y, z) positions, shown in Effect Controls and as draggable effect control points in the Composition/Layer viewer.
- **Exact semantics:**
  - `PF_Param_POINT`: value in pixels in the effect's input layer space, origin top-left, y down. Default is specified by the effect as a percentage of layer size (e.g. (50%, 50%) = layer centre) and converted to pixels when the effect is applied.
  - `PF_Param_POINT_3D` (AE CS5.5 / 10.5+): (x, y, z) pixels; default similarly relative to layer size (z in pixels).
  - Coordinate space: layer space for normal layers; comp space for continuously-rasterised layers and collapsed precomps (shape and text layers always comp space) (Adobe help, Effect control points).
  - Origin offset: if an upstream effect expanded the buffer, AE adds the offset so the point still refers to the same layer pixel (SDK "Point Parameter Origin").
  - Animation: both dimensions animate together; point parameters are spatial (motion path shown in the viewer with spatial bezier/auto-bezier handles) (UNVERIFIED: effect points use spatial interpolation like Position; they draw motion paths in the Layer panel). Separate Dimensions is not available on effect points.
  - Downsampling: pixel values are multiplied by the downsample factor before rendering (AE-FX-018).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Lottie `ty: 3` (Point; 3D points are exported as 3-element point values UNVERIFIED). Expressions: `effect("X")("Point")` returns [x, y] in layer space; use `toComp()` to convert.

### AE-FX-012 · Layer parameter (图层参数)

- **What it does:** A popup that selects another layer of the same comp as an input image (control layer / layer map), with a second menu choosing which stage of that layer to use.
- **Exact semantics:**
  - `PF_Param_LAYER`: value = layer reference (stored by layer ID; menu shows "None" (无) then layers as "N. Name"). Choosing the layer itself is allowed for most effects (the effect then receives the layer's own source stage, not its own output).
  - Second menu (AE CC 2015 13.5+ UNVERIFIED version): `Source` (源) / `Masks` (蒙版) / `Effects & Masks` (效果和蒙版). Source = layer's source footage/frame without masks, effects or transforms; Masks = source with its masks applied; Effects & Masks = after masks and effects. Default = Source. Transforms are never included (Adobe help: "The compound effect ignores effects, masks, and transformations of a control layer" — precompose to include transforms).
  - Time: the referenced layer is sampled at the same comp time as the current frame, mapped through that layer's own time stretch/remap (UNVERIFIED: AE checks out the layer at the comp time, converted to the layer's own time).
  - Size: the control layer arrives at its own pixel size; effects with "Stretch Map To Fit"-type options scale it to the destination size, otherwise it is centred on the destination layer.
  - The control layer's video switch does not matter (hidden layers are usable). Track-matte layers can be referenced. Not animatable (no keyframes; UNVERIFIED: layer params cannot be keyframed but can take an expression in AE 2019+? — Layer Control accepts expressions? no: layer params are not expression-capable except Layer Control? keep: not keyframeable, not expression-capable).
  - Scripting cannot read or set the Source/Masks/Effects & Masks menu (known AE limitation); value via `property.value` = layer index (0 = None).
- **Priority:** P0 — needed by Set Matte, Displacement Map, Layer Control and many compound effects.
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 10` (Effect Value Layer), value = layer `ind`. Implementation hint: the render graph adds a dependency edge to the referenced layer's stage node (source / after masks / after effects), which must not create cycles; a cycle (A references B whose effects reference A) renders the referenced layer as empty. Cross-ref ARCHITECTURE §15 (layer checkout at other times).

### AE-FX-013 · Group, button, no-value, arbitrary-data and path parameters (参数组、按钮、无值、任意数据和路径参数)

- **What it does:** Structural and special parameter types.
- **Exact semantics:**
  | Type | SDK | Value / behaviour | Animatable |
  |---|---|---|---|
  | Group (topic) | `PF_Param_GROUP_START` / `_END` | Twirl-down group in Effect Controls and Timeline; can nest; may start collapsed (`PF_ParamFlag_START_COLLAPSED`). | n/a |
  | Button | `PF_Param_BUTTON` (CS5.5+) | Push button; triggers a callback; no value. | no |
  | No value / label | Lottie `Effect No Value`; SDK param with `PF_PUI_ECW_SEPARATOR` / custom UI | Placeholder rows (e.g. histogram rows of Levels, labels). | no |
  | Arbitrary data | `PF_Param_ARBITRARY_DATA` | Opaque blob with an ID and default; interpolation provided by the effect (e.g. Curves' curve data, Levels' histogram). Effect supplies copy/compare/interpolate/flatten callbacks. | if the effect implements interpolation (Curves: yes, UNVERIFIED) |
  | Path | `PF_Param_PATH` | Reference to a mask on the same layer (index of mask; "None" = 0). The effect reads the mask's path at render time. | no (the referenced mask animates) |
  - Parameter flags BAE must model (SDK `PF_ParamFlag_*`, `PF_PUI_*`): CANNOT_TIME_VARY (no keyframes), CANNOT_INTERP (hold only), SUPERVISE (UI callbacks), START_COLLAPSED, COLLAPSE_TWIRLY, INVISIBLE, DISABLED, NO_ECW_UI (hidden in Effect Controls), STD_CONTROL_ONLY.
- **Priority:** P0 for groups and no-value rows; P1 for path; P2 for button and arbitrary data (internal use by built-in effects such as Curves).
- **Owner:** bae-model (+ bae-plugin, bae-ui-motion)
- **Notes:** Lottie: Ignored value `ty: 6`, No Value has no `ty`; groups appear as `ty: 5` custom-effect children (UNVERIFIED). Cross-ref ARCHITECTURE §15 (BAE parameter types must be a superset of AE's).

### AE-FX-014 · Animating effect parameters (效果参数动画)

- **What it does:** Effect parameters are ordinary animatable properties: stopwatch, keyframes, expressions.
- **Exact semantics:**
  - Every parameter with a value and without CANNOT_TIME_VARY shows a stopwatch in Effect Controls and Timeline; keyframing, interpolation, Graph Editor, expressions and pick-whip work exactly as for layer properties (transform-keyframes-expressions.md).
  - Interpolation by type: float/angle/colour/point/3D point → linear, bezier, auto-bezier, continuous, hold (point also spatial); checkbox, popup, layer, path → hold only; arbitrary data → effect-defined.
  - Effect parameters are evaluated at the layer's local time after time stretch/remap (the effect sees the same time as the layer's other properties).
  - Keyboard: `E` reveals effects in the Timeline; `EE` reveals expressions; `U` animated properties.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Expressions reference effect parameters as `effect("Name")("Param name")`, `effect("Name")(index)` or by match name `effect("Name")("ADBE Slider Control-0001")` (see transform-keyframes-expressions.md, effect object).

### AE-FX-015 · Effect bit-depth support and conversion (效果位深度支持和转换)

- **What it does:** Each effect declares the maximum bit depth it processes (8, 16 or 32 bpc); AE converts pixels when the project depth is higher.
- **Exact semantics:**
  - Flags: 8 bpc only (no flag); 16 bpc = `PF_OutFlag_DEEP_COLOR_AWARE`; 32 bpc float = `PF_OutFlag2_FLOAT_COLOR_AWARE` (+ SmartFX). The Effects & Presets panel badges each effect with 8/16/32 and can filter to effects supporting the project depth.
  - Effect depth < project depth: the input is converted down (32 → 16 or 8: values clamped to [0, 1] and quantised; 16 → 8: quantised), the effect runs, its output is converted up (exact). Overbright values above 1.0 are lost at that point. Effect Controls shows a warning triangle next to the effect name ("this effect only supports 8 bpc").
  - Effect depth ≥ project depth: runs at project depth.
  - Per-effect bpc values for built-in effects are given in each effect item (from the docsforadobe match-name table).
- **Priority:** P0 — needed to match AE numbers in 8/16/32 projects.
- **Owner:** bae-render (+ bae-effects, bae-color)
- **Notes:** BAE decision: all BAE effects run in float on the GPU. To match AE in 16/32 bpc projects, a node whose AE counterpart is 8 or 16 bpc applies the same clamp+quantise on its input and output (configurable "AE fidelity" switch; off = keep float). In 8 bpc projects every node output is clamped and quantised to 1/255 anyway (compositing-3d-color.md, Emulating 8/16 bpc clamping).

### AE-FX-016 · GPU acceleration flag (GPU 加速标志)

- **What it does:** Marks effects that AE can render on the GPU (Mercury GPU Acceleration).
- **Exact semantics:**
  - Flag `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32` (SDK, AE 16.0+ for third-party). The effect is badged GPU in Effects & Presets; "Show GPU Capable Effects Only" filters by it. GPU rendering runs at 32-bit float regardless of project depth (output then converted to project depth).
  - When Project Settings > Video Rendering and Effects = Mercury Software Only, all effects render on the CPU; results should match within rounding.
  - The AE version that first GPU-accelerated each built-in effect is listed per effect below (e.g. Invert 14.1, Levels 14.2, Lumetri 13.8).
- **Priority:** P2 — informational for AE fidelity; BAE effects are GPU-first anyway.
- **Owner:** bae-effects (+ bae-plugin)
- **Notes:** BAE decision: every built-in BAE effect is implemented in WGSL and runs on the GPU; the AE GPU column is only used to decide which AE effects have a "GPU-path" numeric behaviour (float, unclamped) worth matching. Cross-ref ARCHITECTURE §14 (plug-ins declare GPU support).

### AE-FX-017 · Output bounds expansion and Repeat Edge Pixels (输出边界扩展和重复边缘像素)

- **What it does:** Some effects draw outside the input layer's rectangle (blurs, glows, drop shadow, distortions) and enlarge the layer buffer; others are clipped to the layer bounds.
- **Exact semantics:**
  - Effects with `PF_OutFlag_I_EXPAND_BUFFER` (or SmartFX result rect larger than the request) may return a larger output rect; the layer's bounds grow accordingly for the rest of the effect stack, layer styles and transform. AE limits a layer buffer to 30,000 × 30,000 pixels (UNVERIFIED limit applies to effect buffers too).
  - Effects without the flag output exactly the input rect; content outside is clipped (Adobe help: use Grow Bounds (增长边界) to extend legacy effects).
  - Effects that sample outside the input (blurs, distortions) treat outside pixels as transparent black, unless a "Repeat Edge Pixels" (重复边缘像素) checkbox is on: then outside samples clamp to the nearest edge pixel (clamp-to-edge addressing) and the output is NOT expanded (the result stays the layer size with hard edges).
  ```
  sample(img, x, y):
      if repeat_edge: return img[clamp(x, 0, w-1), clamp(y, 0, h-1)]
      else: return (x,y) inside ? img[x,y] : transparent_black
  out_rect = repeat_edge ? in_rect : in_rect.expand(effect_radius)
  ```
  - Each effect item states "Expands bounds: yes/no".
  - Adjustment layers and collapsed/continuously rasterised layers render effects at comp size, so expansion is limited to the comp rect (see compositing-3d-color.md, Adjustment layers; Collapse transformations).
- **Priority:** P0 — blur/glow fidelity depends on it.
- **Owner:** bae-render (+ bae-effects)
- **Notes:** Implementation hint: every BAE effect node declares `fn output_rect(input_rect, params) -> Rect` and `fn input_roi(output_roi, params) -> Rect` (ARCHITECTURE §14 ROI negotiation); the render graph allocates textures from those rects. Pixel-independent colour effects (this part's Color Correction and most Channel effects) never expand bounds and need ROI = output ROI.

### AE-FX-018 · Downsample-factor scaling of pixel parameters (像素参数的降采样缩放)

- **What it does:** When previewing at reduced resolution (Half, Third, Quarter, Custom) or with proxies, pixel-unit parameters are scaled so the result looks like a scaled-down full-resolution render.
- **Exact semantics:**
  - Render request carries `downsample_x`, `downsample_y` (rational, e.g. 1/2). Effects multiply every pixel-distance parameter (blur radius, offsets, point positions, stroke widths) by the factor in the corresponding axis: "a blur of 4 pixels should be interpreted as a blur of 2 pixels if the downsample factor is 1/2" (SDK PF_InData).
  - Point parameters are delivered already scaled by AE (UNVERIFIED: AE scales point param values by downsample before the effect sees them; effects scale scalar pixel params themselves).
  - Non-square pixel aspect ratio: effects receive `pixel_aspect_ratio` and must scale horizontal distances so circles stay round in square-pixel display (SDK Pixel aspect ratio).
  - Colour-only effects (this part) are unaffected, except where they compute statistics over the image (Auto Levels/Contrast/Color, Equalize, Color Stabilizer), which then see a lower-resolution image and may give slightly different results.
- **Priority:** P0
- **Owner:** bae-effects (+ bae-render)
- **Notes:** See compositing-3d-color.md, Preview resolution. BAE decision: parameters tagged `unit = pixels` in the BAE parameter schema are scaled by the host before the shader runs, so individual WGSL shaders never see downsample factors.

### AE-FX-019 · Effects on adjustment layers (调整图层上的效果)

- **What it does:** An effect on an adjustment layer processes the composite of all layers below it within the adjustment layer's area and time.
- **Exact semantics:** Specified in compositing-3d-color.md, Adjustment layers (input = composite below, cropped to the adjustment layer's bounds and masks; result blended with Opacity and blend mode; adjustment layer's own source pixels are ignored). Effect-level rules that apply here: the effect's input layer `param[0]` is that composite, at comp size; point parameters are in the adjustment layer's layer space; Compositing Options (AE-FX-005/006) apply before the adjustment layer's own opacity.
- **Priority:** P0
- **Owner:** bae-render (+ bae-effects)
- **Notes:** —

### AE-FX-020 · Layer and time inputs of effects (效果的图层和时间输入)

- **What it does:** Defines which images and times an effect may read besides its own input at the current time.
- **Exact semantics:**
  - Layer parameters (AE-FX-012) read another layer's Source / Masks / Effects & Masks stage at the current comp time (in that layer's time).
  - Effects with `PF_OutFlag_WIDE_TIME_INPUT` (e.g. Echo, Posterize Time, Time Displacement, Color Stabilizer, Pixel Motion Blur) check out their input or parameters at other times; AE then renders the input layer (after masks and effects above this effect) at those times. The input is the same stage of the same layer at time t' (UNVERIFIED: AE renders the layer's own effects above at t' too).
  - Effects with `PF_OutFlag_I_USE_SHUTTER_ANGLE` (e.g. Transform effect, CC Force Motion Blur) read the comp shutter angle/phase.
  - Effects that use the comp camera and lights (`PF_OutFlag2_I_USE_3D_CAMERA`, `_I_USE_3D_LIGHTS`; e.g. Card Dance, Card Wipe, Shatter, CC Sphere) read the active camera/lights at the current time; the layer stays 2D (no intersections or shadows with 3D layers).
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim, bae-effects)
- **Notes:** Implementation hint: an effect node declares its extra dependencies (`(layer, stage, time)` tuples) during graph construction, so the render scheduler can render and cache them first; caching must key on (layer, stage, time). Cross-ref ARCHITECTURE §15 ("效果可以声明对其他图层、其他时间的依赖").

### AE-FX-021 · Reset effect (重置效果)

- **What it does:** Resets all parameters of one effect instance to their defaults.
- **Exact semantics:**
  - Effect Controls: "Reset" link (AE ≤ 24) / Reset Effect icon (AE 25.x modernised Effect Controls) at the top of the effect. Removes all keyframes of the effect's parameters and sets static defaults; expressions are UNVERIFIED (AE keeps expressions? — Reset removes keyframes and sets values; expressions remain attached UNVERIFIED).
  - Also resets Compositing Options (Effect Opacity 100%, mask references removed? UNVERIFIED).
  - Point defaults are recomputed from the current layer size (e.g. layer centre).
  - Right-click a single parameter > Reset resets only that parameter.
  - One undo step.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Expression Controls: Reset sets the Dropdown Menu Control value to 1 but keeps the edited menu items (UNVERIFIED).

### AE-FX-022 · Effect > Remove All (效果 > 全部移除)

- **What it does:** Deletes every effect from the selected layers.
- **Exact semantics:** Menu Effect > Remove All, shortcut Ctrl+Shift+E (Win) / Cmd+Shift+E (Mac). Applies to all selected layers (selected in Timeline or Composition panel); deletes all effect instances including Expression Controls, with their keyframes and expressions. One undo step named "Remove All Effects". Expressions on other properties that referenced removed effects then error.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** —

### AE-FX-023 · Animation Presets (.ffx) (动画预设)

- **What it does:** Saves and applies a reusable set of effects, properties, keyframes and expressions as a `.ffx` file.
- **Exact semantics:**
  - Save: select effects and/or properties (any layer properties, including transform, text animators, masks), Animation > Save Animation Preset or Effects & Presets panel menu; saved to the user Presets folder by default.
  - Apply: drag/double-click from Effects & Presets, Animation > Apply Animation Preset..., Animation > Recent Animation Presets, or Ctrl+Alt+Shift+F / Cmd+Opt+Shift+F (apply most recent preset). Applied at the current time (keyframe times are relative to the first keyframe). Effects in the preset are added to the layer (with the paste-replace rule of AE-FX-002 for single instances, UNVERIFIED); non-effect properties overwrite existing values.
  - Effect Controls per-effect "Animation Presets" menu (shown when "Show Animation Presets" is enabled) applies a preset's settings to the existing effect instance.
  - Presets folders scanned at startup and when Effects & Presets is shown; folders named in parentheses are ignored. "Refresh List" rescans.
  - A preset cannot be disabled/removed as a unit after application.
  - `.ffx` is a binary RIFX container (big-endian RIFF, as in .aep) (UNVERIFIED structure).
- **Priority:** P1 — commonly used, but BAE can first ship its own preset format.
- **Owner:** bae-model (+ bae-motion, bae-ui-motion)
- **Notes:** BAE decision: BAE presets are a stored subtree of the project data model (JSON/USD-style fragment); importing AE `.ffx` is P2 and depends on the .aep reader. Bundled AE presets are Adobe content and must not be copied.

### AE-FX-024 · Legacy and obsolete effect versions (旧版和过时效果)

- **What it does:** When AE replaces an effect with a new implementation, the old one keeps its matchName and stays loadable (in the Obsolete category or hidden) so old projects render identically.
- **Exact semantics:**
  - New versions get a new matchName (usually with a numeric suffix): e.g. `ADBE Easy Levels` → `ADBE Easy Levels2` (Levels), `ADBE Pro Levels` → `ADBE Pro Levels2`, `ADBE Brightness & Contrast` → `ADBE Brightness & Contrast 2`, `ADBE Color Balance` → `ADBE Color Balance 2`, `ADBE Exposure` → `ADBE Exposure2`, `ADBE Photo Filter` → `ADBE PhotoFilterPS`, `ADBE Set Matte2` → `ADBE Set Matte3`, `ADBE Gaussian Blur` → `ADBE Gaussian Blur 2` (Gaussian Blur (Legacy) stays in Obsolete), `ADBE Fast Blur` = Fast Blur (Legacy).
  - Obsolete category effects (still applicable from the Effect > Obsolete menu): Basic 3D, Basic Text, Color Key, Fast Blur (Legacy), Gaussian Blur (Legacy), Lightning, Luma Key, Path Text, Reduce Interlace Flicker, Spill Suppressor (docsforadobe table, "Obsolete").
  - Hidden obsolete effects (load only from old projects): Alpha Levels, Apply Color LUT, Box Blur, Cineon Converter, old Color Balance / Exposure / Levels / Photo Filter / Set Matte / Brightness & Contrast, Three-Way Color Corrector, Threshold, Unsharp Mask, Photoshop layer-style effects (`ADBE PSL *`), etc. (docsforadobe "_Obsolete").
  - An effect can also keep its matchName and change behaviour behind a parameter (e.g. Brightness & Contrast "Use Legacy", Hue/Saturation "Use legacy"? no; see each effect).
- **Priority:** P1 for the legacy versions of effects specified in this file (old projects and templates use them); P2 for the rest.
- **Owner:** bae-effects (+ bae-model)
- **Notes:** BAE decision: map old matchNames to the same BAE implementation with a `legacy` mode flag when the maths differs, otherwise alias them. Each effect item below names its legacy matchName where one exists.

### AE-FX-025 · Effect categories and the Effect menu (效果类别和效果菜单)

- **What it does:** Built-in effects are grouped into categories that form the Effect menu submenus and the Effects & Presets tree.
- **Exact semantics:** Categories (AE 2025, English / zh-CN): 3D Channel (3D 通道), Audio (音频), Blur & Sharpen (模糊和锐化), Channel (通道), CINEMA 4D, Color Correction (颜色校正), Distort (扭曲), Expression Controls (表达式控制), Generate (生成), Immersive Video (沉浸式视频), Keying (抠像), Matte (遮罩), Noise & Grain (杂色和颗粒), Obsolete (过时), Perspective (透视), Simulation (模拟), Stylize (风格化), Text (文本), Time (时间), Transition (过渡), Utility (实用工具). Within a category effects are sorted alphabetically by localised display name. Third-party effects add categories by their PiPL category string. Scripting `app.effects[i].category` gives the category string ("" for hidden effects).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-plugin)
- **Notes:** UNVERIFIED zh-CN strings for Immersive Video and Utility (译名待核). The Effects & Presets panel itself is specified in ui-preview-render-io.md.

### AE-FX-026 · Missing effects (缺失的效果)

- **What it does:** When a project references an effect whose plug-in is not installed, AE keeps the instance and its parameter data and shows it as missing.
- **Exact semantics:** On open, AE reports missing effects (dialog listing them; File > Dependencies > Find Missing Effects selects layers using them). The effect stays in the stack with its name in red/italic UNVERIFIED and a "missing" message in Effect Controls; it renders as pass-through (input unchanged). Parameter values and keyframes are preserved and saved back.
- **Priority:** P1
- **Owner:** bae-model (+ bae-plugin, bae-render)
- **Notes:** BAE decision (ARCHITECTURE §15): resolve an unknown matchName in order BAE native implementation → installed BAE plug-in → AE-plug-in compatibility layer → "missing effect" placeholder that keeps all parameter data and round-trips to AE unchanged.
