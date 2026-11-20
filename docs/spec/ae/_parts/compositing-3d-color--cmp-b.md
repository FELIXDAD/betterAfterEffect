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
  - Ties (exactly coplanar layers, or equal depth at a pixel): the layer higher in the Timeline stacking order (lower index) is in front.
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
  - a 3D precomp layer without Collapse Transformations;
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
  - Since AE 18.0 (2021-03, "3D Design Space") Draft 3D additionally switches the viewer to the GPU real-time draft engine (today the Advanced 3D engine, "only intended for draft quality") and enables the 3D Ground Plane and Extended Viewer buttons (AE-CMP-411). This real-time draft preview replaced the old OpenGL "Fast Draft" Fast Previews mode (AE-CMP-467). Help note: with Draft 3D on, some effects may not render on precomps.
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
  - Applied per layer, after projection, only to 3D layers; 2D layers are never blurred. Blur varies across a layer that recedes in depth (per-pixel depth). UNVERIFIED: whether Classic 3D uses per-pixel or per-layer-centre depth; per-pixel is believed (since AE 7.0).
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

- **What it does:** Layer > Camera > Create Stereo 3D Rig (AE CS5.5+) builds a stereo rig: a master camera, left/right eye cameras in precomps, a 3D Glasses output comp and a control null with Stereo Scene Depth, Convergence etc.
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
- **Notes:** matchName `ADBE Light Layer`; scripting `LayerCollection.addLight(name, centerPoint)`, `LightLayer.lightType` (`LightType.PARALLEL`, `SPOT`, `POINT`, `AMBIENT`, `ENVIRONMENT`), `LightLayer.lightSource` (AE 24.3+). Lottie has no light layer type in common players (bodymovin skips lights; lottie-web ignores them). BAE Lottie export: bake nothing, warn.

### AE-CMP-430 · Light Settings dialog and Light Options (光照设置 / 灯光选项)

- **What it does:** Dialog and Timeline property group defining light type, colour, intensity, cone, falloff and shadow parameters.
- **Exact semantics:** Fields in dialog order (Timeline group `ADBE Light Options Group` shows the same properties except Light Type):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Name (名称) | string | "<Type> Light N" | — | — | |
  | Light Type (光照类型) | enum | Spot UNVERIFIED (dialog remembers last used) | Parallel, Spot, Point, Ambient, Environment | — | AE-CMP-431; Environment AE 24.3+ (Advanced 3D only) |
  | Color (颜色) | color | white (1,1,1) | 0–1 per channel (overbright allowed in 32 bpc UNVERIFIED) | — | `ADBE Light Color` |
  | Intensity (强度) | float | 100 | unbounded; negative allowed | % | `ADBE Light Intensity`; negative = "nonlight", subtracts |
  | Cone Angle (锥形角度) | angle | 90 | 0–180 | ° (full cone angle) | Spot only; `ADBE Light Cone Angle` |
  | Cone Feather (锥形羽化) | float | 50 | 0–100 | % | Spot only; `ADBE Light Cone Feather 2` |
  | Falloff (衰减) | enum | None | None (无), Smooth (平滑), Inverse Square Clamped (反向平方限制) | — | Parallel/Spot/Point; `ADBE Light Falloff Type`; AE CC 2014 (13.0) UNVERIFIED |
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
  Ambient (环境):   no direction; adds color*intensity uniformly; never casts shadows
  Environment (环境光, Advanced 3D only, AE 24.3+): image-based light from a 2D layer source
                    (HDR/EXR in 24.3; any 2D layer since 25.2); P2
  ```
  Chinese UI (zh-CN help light-type list): Parallel 平行光, Spot 聚光, Point 点光, Ambient 周围光, Environment 环境光; the Light Settings description text uses the short forms 平行/聚光/点/环境 (译名待核: which form the dialog menu shows).
- **Priority:** P0 (Environment P2)
- **Owner:** bae-render
- **Notes:** —

### AE-CMP-432 · Spot light cone and feather (聚光锥形和羽化)

- **What it does:** Restricts a spot light to a cone around its axis with a soft edge.
- **Exact semantics:** UNVERIFIED model (Adobe does not publish it):
  ```
  axis   = normalize(light.POI - light.position)       // or local +z when not auto-oriented
  theta  = angle between axis and (P - light.position)
  outer  = ConeAngle / 2
  inner  = outer * (1 - ConeFeather/100)
  spot   = 1                              if theta <= inner
         = smoothstep(outer, inner, theta) if inner < theta < outer   // falls 1 -> 0
         = 0                              if theta >= outer
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
  Smooth:                   atten = 1                         if d <= R
                                  = 1 - smoothstep(0, 1, (d - R) / F)   UNVERIFIED (help: "smooth linear")
                                  = 0                         if d >= R + F
  Inverse Square Clamped:   atten = 1                         if d <= R
                                  = (R / d)^2                 otherwise   (F unused)  UNVERIFIED
  ```
  - F = 0 with Smooth ⇒ hard cut at R. R = 0 with Inverse Square Clamped ⇒ atten = 0 everywhere except d = 0 UNVERIFIED (AE likely clamps R to a small minimum).
  - Falloff does not affect Ambient lights.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Before falloff existed (≤ AE CC 12.x), lights never attenuated; old projects load with Falloff = None.

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
  - If every light reaching a layer is excluded by scope, that layer renders as if lights exist but none reach it (dark, ambient only), not as unlit. UNVERIFIED: AE's "no lights at all ⇒ unlit" test is comp-wide, not per layer.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** —

### AE-CMP-436 · Classic 3D lighting model (经典 3D 光照模型)

- **What it does:** Computes the lit colour of each 3D layer pixel from layer colour, material options and all lights.
- **Exact semantics:** Adobe does not document the equation; best-known Phong-style model, all values 0–1, material percentages /100. UNVERIFIED (overall form well established; exact shininess mapping and specular colour mixing unverified):
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
  | Casts Shadows (投影) | enum, animatable (hold) | Off | Off (关), On (开), Only (仅) | — | matchName `ADBE Casts Shadows` UNVERIFIED; toggle Off↔On: Opt+Shift+C / Alt+Shift+C |
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
- **Notes:** In the Advanced 3D renderer the same switches appear for 3D model layers under "Compositing Options"; Accepts Shadows "Only" is disabled there (shadow catchers are a separate feature, P2).

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
  - To make a lit 3D layer match its 2D look under a single 100 % light: Ambient 100, Diffuse 100, Specular 0 (Adobe tip).
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
  - UI warning: the Effect Controls panel shows a yellow warning triangle next to the effect name when the effect's max depth is below the project depth (tooltip states the supported depth); the Effects & Presets panel can filter "Show 32-bpc Only / 16-bpc / 8-bpc" effects.
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
  | Color Engine (颜色引擎) | enum | Adobe color managed | Adobe color managed, OCIO color managed | — | OCIO engine AE 24.x UNVERIFIED (24.0, 2023-10 or 24.1); ACES 2.0 configs AE 26.5 |
  | Working Color Space (工作色彩空间) | enum/profile | None (unmanaged) UNVERIFIED for new 2024+ projects | AE-CMP-449 / AE-CMP-459 | — | `Project.workingSpace` ("" = None) |
  - Adobe engine: colour management is ON iff Working Color Space ≠ None. With None, footage RGB numbers enter compositing unchanged (except Interpret-as-Linear and media colour-space handling, AE-CMP-450) and output is unchanged.
  - Selecting OCIO always manages colour (working space chosen from the config).
- **Priority:** P0 (Adobe ICC with None/sRGB/Rec.709), P2 (OCIO)
- **Owner:** bae-color (+ bae-model)
- **Notes:** Older AE (≤ 23.x) had no engine menu; Working Space alone controlled CM. Project settings storage: see project-comp-layers.md, Project settings.

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
- **Exact semantics:** `Project.workingGamma` ∈ {2.2, 2.4}; ignored when a working space is set. UNVERIFIED semantics: media with embedded colour info (QuickTime/H.264/HEVC/ProRes tagged Rec.709) is converted from its tagged space to a pure gamma 2.2 or 2.4 display space with Rec.709 primaries; 2.4 keeps Rec.709 video numbers unchanged, 2.2 brightens shadows slightly. Default UNVERIFIED (2.4 for new projects since AE 22.x).
- **Priority:** P2
- **Owner:** bae-color
- **Notes:** Related legacy option "Match Legacy After Effects QuickTime Gamma Adjustments" (AE-CMP-462).

### AE-CMP-451 · Linearize Working Space (线性化工作空间)

- **What it does:** Composites in linear light: the working space keeps its primaries and white point but uses a gamma-1.0 tone curve, so all operations (blending, resampling, blur, motion blur, antialiasing, lighting) act on linear values.
- **Exact semantics:**
  - Available only when colour management is on. `Project.linearizeWorkingSpace` (scripting since AE 16.0).
  - Footage converted to the linearised working space: `v_lin = TRC_working^-1(v)` (AE-CMP-461 formulas) after profile conversion.
  - Colour property values (picker colours, solid colours) are interpreted as already linear-space values (numbers unchanged ⇒ appearance changes when toggling).
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
  | Convert To Linear Light (转换为线性光) | enum | On for 32 bpc | Off, On, On for 32 bpc | — | output encoded with gamma 1.0 of the output profile |
  | Embed Profile / Embed color space (嵌入配置文件) | bool | on for formats that support it UNVERIFIED | — | — | PSD, PNG, TIFF, JPEG (ICC); CICP for PNG/MOV HDR |
  - Rendering intent chosen automatically: relative colorimetric with black-point compensation; absolute colorimetric for film-negative output profiles.
  - Multiple output modules on one render item may each use a different profile.
  - Export paths outside the Render Queue that bypass output modules (legacy SWF) convert to sRGB.
  - HDR10 metadata for PNG (Mastering Display Color Volume primaries/luminance, content light level) when output space is PQ (AE 25.x UNVERIFIED).
- **Priority:** P0 (profile conversion), P1 (linear light, embed), P2 (HDR10 metadata)
- **Owner:** bae-color (+ bae-media)
- **Notes:** Output module UI and formats: see ui-preview-render-io.md, Render Queue / Output modules.

### AE-CMP-457 · Display color management (显示色彩管理)

- **What it does:** Converts viewer pixels from the working space to the monitor's profile so colours look correct; per viewer, preview only.
- **Exact semantics:**
  - Toggle per viewer (Composition, Layer, Footage): View > Use Display Color Management or Show Channel and Color Management Settings button > Use Display Color Management; shortcut Shift + numpad / (Win), Ctrl+Opt+/ UNVERIFIED (Mac). Default On when CM is enabled. A yellow plus sign on the button indicates it is on.
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
  | OCIO Configuration (OCIO 配置) | enum/file | bundled ACES config (ACES 1.x "ACE 2" in 24.x; ACES 2.0 CG v4.0 / Studio v4.0 added in 26.5 with OCIO 2.5.1) | bundled configs, Custom (load .ocio) | — | |
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
  | Resolution (分辨率) | enum | Full (Auto in the viewer menu for new comps UNVERIFIED) | Auto (自动, previews only), Full (完整), Half (二分之一), Third (三分之一), Quarter (四分之一), Custom (自定义) | — | Composition Settings and viewer menu; scripting `CompItem.resolutionFactor` = [x, y] |
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

### AE-CMP-466 · Region of Interest (感兴趣区域)

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
  | Off (Final Quality) (关闭（最终品质）) | always full quality at the comp's Resolution |
  | Adaptive Resolution (自适应分辨率) | during interaction, renders at a lower factor (down to the Adaptive Resolution Limit preference: 1/2, 1/4, 1/8, 1/16; default 1/8 UNVERIFIED) to keep interactive speed; full quality when interaction stops |
  | Draft (草稿) | Ray-traced 3D only (removed in AE 24.0 UNVERIFIED) |
  | Fast Draft (快速绘图) | (≤ AE 23.x) 3D layout mode: 1/4 downsample, effects & mattes on, no lights/shadows/DOF; replaced by Draft 3D (AE-CMP-409) |
  | Wireframe (线框) | draws layer bounding boxes (and 3D layer outlines through the camera) instead of pixels |
  - If an interaction frame takes too long in Off/Adaptive modes, AE temporarily shows wireframes until the mouse stops.
  - The renderer button's lightning bolt turns orange in Draft/Fast Draft/Wireframe, and in Adaptive Resolution while downsampled; the mode name appears at the viewer's top-right.
  - Live Update (实时更新) off ⇒ wireframes during interaction; Alt/Opt while dragging toggles it temporarily; Caps Lock suspends viewer refresh (red bar). These are UI behaviours: see ui-preview-render-io.md, Preview & cache behaviour.
  - Adaptive Resolution preferences (AE 25.x): Adaptive Resolution Priority slider (minimum resolution) and 3D Renderer Reduction (Resolution and Quality / Render Quality Only) UNVERIFIED version; see ui-preview-render-io.md, Preferences.
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
  | Alpha Boundary / Alpha Overlay | Layer panel only (Roto Brush) — outline / tinted overlay of the alpha |
  | Colorize (彩色化) | toggle: single-channel views shown in that channel's colour instead of grey |
  - Then the colour-management items: Use Display Color Management, Simulate Output (AE-CMP-457/458).
  - Alt/Opt-click the button toggles between Alpha and RGB. Shortcuts Alt/Opt+1..4 (R, G, B, Alpha; Shift adds Colorize) UNVERIFIED.
  - Channel views are applied after exposure and before display colour management? UNVERIFIED order; best-known: channel extraction on working values, then display transform. Straight-colour views force "Faster" viewer quality.
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
  - Each viewer has its own value; not saved to output; persisted with the project per viewer UNVERIFIED.
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
