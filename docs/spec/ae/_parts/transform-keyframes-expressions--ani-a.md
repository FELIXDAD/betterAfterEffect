# After Effects inventory: transform, keyframes and expressions (AE-ANI)

**Scope.** This file specifies how After Effects places layers in space and how every animatable value changes over time: 2D/3D layer transforms and their exact matrix order, parenting and auto-orient math, camera and light transforms, the keyframe data model, temporal and spatial interpolation (Linear, Bezier, Continuous/Auto Bezier, Hold), the speed/influence ease model, Easy Ease, roving keyframes, Separate Dimensions, Graph Editor semantics, keyframe velocity, copy/paste of keyframes, keyframe assistants, layer time math (time stretch, time remapping, nested-comp time and source-frame selection), frame blending, motion-blur sampling, and the complete expression language. Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md and ui-preview-render-io.md; the item format, ID scheme, priorities and owners are defined in README.md.

Conventions used in this file (apply to every item unless an item says otherwise):

- Coordinates are AE pixels. Comp space: origin at the top-left corner of the comp frame, +x right, +y **down**, +z **away from the viewer** (into the screen). This is a right-handed system.
- Matrices act on **column vectors** (`p' = M * p`); `A * B` applies `B` first. `T(v)` translation, `S(v)` scale (fractions, i.e. AE percent / 100), `Rx/Ry/Rz(θ)` the standard right-handed rotation matrices with θ in degrees converted to radians. With y down, `Rz(+θ)` turns +x toward +y, which is **clockwise on screen**, matching AE's positive Rotation.
- Time is in seconds in formulas. BAE stores time as integer flicks (705,600,000 per second); "frame" means comp frame unless stated.
- "CTI" = current-time indicator (时间指示器).

## Transform properties

### AE-ANI-001 · Transform property group and per-layer-type defaults (变换)

- **What it does:** Every visual layer has a Transform group whose properties place the layer in its parent's (or the comp's) space; which properties exist depends on layer type and the 3D switch.
- **Exact semantics:**
  - Property set per layer kind (layer kinds themselves: see project-comp-layers.md, Layer types):

    | Layer kind | Properties in the Transform group (AE UI order) |
    |---|---|
    | 2D AV layer (footage, solid, precomp, shape, text, null, adjustment) | Anchor Point (锚点), Position (位置), Scale (缩放), Rotation (旋转), Opacity (不透明度) |
    | 3D AV layer (3D switch on) | Anchor Point, Position, Scale (all 3-D), Orientation (方向), X Rotation (X 轴旋转), Y Rotation (Y 轴旋转), Z Rotation (Z 轴旋转), Opacity |
    | Camera layer | Point of Interest (目标点), Position, Orientation, X/Y/Z Rotation (no Anchor, Scale, Opacity) |
    | Light layer | depends on Light Type, see AE-ANI-010 (no Anchor, Scale, Opacity) |
    | Audio-only layer | no Transform group (only Audio) |

  - Defaults when a layer is created (comp size W×H, layer source size w×h):

    | Property | Footage / solid / precomp / null | Text layer | Shape layer (drawn with a tool) | Units |
    |---|---|---|---|---|
    | Anchor Point | (w/2, h/2[, 0]) | (0, 0[, 0]) (text origin = baseline start of first line) | (0, 0[, 0]) unless the "Center Anchor Point in New Shape Layers" preference is on | layer-space px |
    | Position | (W/2, H/2[, 0]) | where the user clicked | (W/2, H/2[, 0]) | parent-space px |
    | Scale | (100, 100[, 100]) | same | same | % |
    | Rotation / Z Rotation | 0 | 0 | 0 | degrees |
    | Orientation (3D) | (0, 0, 0) | same | same | degrees |
    | X/Y Rotation (3D) | 0 | 0 | 0 | degrees |
    | Opacity | 100 | 100 | 100 | % |

  - Turning the 3D switch on adds a z component (0 for Anchor/Position, 100 for Scale), adds Orientation and X/Y Rotation (plus the Geometry Options and Material Options groups, see compositing-3d-color.md), and renames Rotation to Z Rotation (value and keyframes kept). Turning it off **deletes** Orientation, X Rotation and Y Rotation (and Geometry/Material Options) including all values, keyframes and expressions, which are not restored by switching back; Anchor/Position/Scale keep their keyframes and expressions with their z hidden and ignored, and Z Rotation is renamed back to Rotation (AE help, 3D layers).
  - Reset (重置) next to the group name resets every Transform property to the layer-kind default above. UNVERIFIED: on an animated property Reset does not delete keyframes; it sets/adds a keyframe at the CTI with the default value (observed behaviour); BAE must match this once measured.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting: group matchName `ADBE Transform Group`; children `ADBE Anchor Point`, `ADBE Position` (`ADBE Position_0/_1/_2` when separated), `ADBE Scale`, `ADBE Orientation`, `ADBE Rotate X`, `ADBE Rotate Y`, `ADBE Rotate Z` (the 2D "Rotation" is also `ADBE Rotate Z`), `ADBE Opacity`. Expression accessors: `transform.anchorPoint`, `.position`, `.xPosition`, `.scale`, `.rotation`/`.zRotation`, `.orientation`, `.xRotation`, `.yRotation`, `.opacity`, `.pointOfInterest` (see the expression part of this file). Lottie: transform `ks` with `a`, `p` (or split `p.s:true` + `p.x/p.y/p.z`), `s`, `r` (2D) or `rx/ry/rz/or` (3D, layer `ddd:1`), `o`. Older AE (UNVERIFIED: before CS4) put the Null Object anchor at its top-left (0,0); current AE nulls are 100×100 with anchor (50,50). The "Center Anchor Point in New Shape Layers" preference (UNVERIFIED: added in AE 16.1, 2019) lives in Preferences > General (see ui-preview-render-io.md, Preferences).

### AE-ANI-002 · Anchor Point (锚点)

- **What it does:** The point in layer space about which the layer scales and rotates and which is placed at Position.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Anchor Point (锚点) | 2-D (2D layer) / 3-D (3D layer) float | see AE-ANI-001 | unbounded | layer-space px | spatial property (has spatial interpolation and a motion path in the Layer panel) |

  - Layer space: origin at the top-left of the layer's source rectangle (for text and shape layers: the layer's own origin), +y down.
  - Anchor Point z is meaningful for 3D layers: it offsets the layer plane along its local z (a layer at anchor z = 100 is drawn 100 px *toward the viewer* relative to its Position before rotation, because `T(-anchor)` moves the plane to z = -100).
  - Editing commands that keep the layer visually still (Pan Behind tool, Center Anchor Point in Layer Content) change Anchor **and** compensate Position: `Δposition = M_rs * Δanchor`, where `M_rs` is the layer's own rotation × scale (× orientation for 3D) at the CTI; if Position is animated, AE adds/changes a key at the CTI only (UNVERIFIED: AE offsets all Position keyframes? Observed: Pan Behind on an animated Position modifies only the current value/keyframe, so the layer jumps at other times).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-motion)
- **Notes:** Expression `transform.anchorPoint`; Lottie `ks.a` (Lottie stores the AE value directly; lottie-web applies `translate(-a)`). Pan Behind tool and Quick Set Anchor (Shift+Tab, AE 25.x beta/2025) belong to ui-preview-render-io.md (Tools). Center Anchor Point in Layer Content: AE-ANI-011.

### AE-ANI-003 · Position (位置)

- **What it does:** Places the layer's anchor point in parent space (comp space when unparented).
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Position (位置) | 2-D / 3-D float | (W/2, H/2[, 0]) | unbounded | parent-space px | spatial; motion path in comp viewer |
  | X Position / Y Position / Z Position (X 位置 / Y 位置 / Z 位置) | 1-D float each | components of Position | unbounded | px | only exist while Separate Dimensions is on (AE-ANI-052) |

  - Position z: positive = farther from the viewer (smaller under a perspective camera).
  - For a 2D layer the stored value may still carry a z (kept from a former 3D state); it is ignored for rendering and hidden in the UI.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Lottie `ks.p` (or `ks.p.s=true` with `p.x`, `p.y`, `p.z` for separated dimensions); lottie-web negates z internally (`translate(px, py, -pz)`) because its own matrix uses z toward the viewer, which confirms AE z-away. Arrow-key nudge of Position (1 px, Shift = 10 px, at current magnification) is in ui-preview-render-io.md (shortcuts).

### AE-ANI-004 · Scale (缩放)

- **What it does:** Scales the layer about its anchor point per axis, in percent; negative values mirror the layer.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Scale (缩放) | 2-D / 3-D float | (100, 100[, 100]) | unbounded, negative allowed | % | non-spatial multi-dimensional property: per-dimension temporal ease |
  | Constrain Proportions (约束比例) | bool (UI link state, persisted per property) | on | — | — | link icon left of the values |

  - Matrix factor: `S(sx/100, sy/100, sz/100)`.
  - Constrain Proportions on: editing one component sets the others so the ratios at the moment of the edit are preserved: `k = new_i / old_i`; `other_j = old_j * k`. If `old_i == 0` the ratio is undefined; UNVERIFIED: AE then sets every component to the typed value. Alt/Option-click the link icon turns it on and first sets y (and z) equal to x.
  - Flip Horizontal / Flip Vertical (AE-ANI-011) multiply x or y by -1. `scale = (-100, -100)` is visually identical to Rotation 180°.
  - Scale 0 in any axis produces a singular matrix: the layer renders nothing; children of such a parent collapse onto a line/point; `toComp`/`fromComp` inverse transforms are undefined (BAE: return the anchor-projected point, never NaN).
  - Scale z only matters for 3D layers: it scales the anchor z offset and the positions of 3D children; a flat layer's pixels are unaffected.
  - Edit Value dialog lets the user enter scale as % of source or as pixels (Width/Height), with "Include Pixel Aspect Ratio" (AE-ANI-012).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-ui-motion)
- **Notes:** Lottie `ks.s` in percent. Exponential Scale assistant: AE-ANI-078. Keyboard: Alt/Option + numpad +/- = ±1 %, with Shift ±10 % (ui-preview-render-io.md). Raster softening on downscale depends on layer Quality (compositing-3d-color.md, Quality/sampling).

### AE-ANI-005 · Rotation (2D) (旋转)

- **What it does:** Rotates a 2D layer about its anchor point; positive is clockwise on screen.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Rotation (旋转) | 1-D float | 0 | unbounded (multiple revolutions) | degrees | stored as total degrees |

  - Display format: `<revolutions>x<signed degrees>°`, where `revolutions = trunc(v / 360)` and `degrees = v - 360 * revolutions`, both carrying the sign of `v`: 0 → `0x+0.0°`, 400 → `1x+40.0°`, -30 → `0x-30.0°`, -400 → `-1x-40.0°`. Degrees show one decimal place. The user can edit the revolutions and degrees fields independently; the stored value is `360*rev + deg`.
  - Interpolation is linear in degrees (no shortest-path wrapping): keys 0 → 720 turn the layer twice.
  - Rotation tool drag with Shift snaps to 15° increments; numpad +/- rotates by ±1°, Shift ±10° (UI items in ui-preview-render-io.md).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-ui-motion)
- **Notes:** Scripting matchName `ADBE Rotate Z` (same as 3D Z Rotation). Lottie `ks.r` in degrees. Expression `transform.rotation` returns total degrees.

### AE-ANI-006 · Opacity (不透明度)

- **What it does:** Multiplies the layer's alpha by a uniform factor.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Opacity (不透明度) | 1-D float | 100 | 0 – 100 (values outside are clamped on entry) | % | not inherited by children |

  - Render: `layer_alpha *= opacity / 100` after masks and effects, before the blend mode (compositing-3d-color.md, Alpha model / render order).
  - Opacity is **not** part of the transform matrix and is **not** inherited through parenting.
  - Display: UNVERIFIED: the Timeline shows Opacity as a whole percent (e.g. typing 33.3 shows "33%") while the stored value keeps the fraction; BAE should store full precision and may display one decimal when the value is fractional.
  - Expression results outside 0–100 are clamped at render time.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-render)
- **Notes:** Lottie `ks.o` (0–100). Keyboard: T reveals Opacity (ui file).

### AE-ANI-007 · Orientation (3D) (方向)

- **What it does:** Sets the absolute angular orientation of a 3D layer, camera or light as three angles that animate along the most direct route.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Orientation (方向) | 3-D float (x°, y°, z°) | (0, 0, 0) | each component wraps to [0, 360) on entry | degrees | displayed `0.0°,0.0°,0.0°` |

  - Matrix contribution: `Rx(ox) * Ry(oy) * Rz(oz)` (so z is applied to the point first), applied **outside** (after) X/Y/Z Rotation; see AE-ANI-014.
  - Interpolation between Orientation keyframes is **not** per-component: AE converts each keyframe to a rotation and interpolates along the shortest arc (spherical); see AE-ANI-057. Hence Orientation cannot express more than a half turn between two keys.
  - Orientation keyframes have spatial-like interpolation only in time (no motion path); lottie-web discards the `to/ti` spatial tangents of `or` keyframes.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** matchName `ADBE Orientation`; Lottie `ks.or`. Rotation tool "Set: Orientation / Rotation" menu selects which property the tool edits (ui-preview-render-io.md, Tools). Auto-Orient Along Path for 3D layers writes into the rotation stack on top of Orientation (AE-ANI-020).

### AE-ANI-008 · X, Y and Z Rotation (3D) (X 轴旋转、Y 轴旋转、Z 轴旋转)

- **What it does:** Three independent per-axis rotations of a 3D layer, camera or light that interpolate numerically and can describe multiple revolutions.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | X Rotation (X 轴旋转) | 1-D float | 0 | unbounded | degrees | display `nx+d°` as AE-ANI-005 |
  | Y Rotation (Y 轴旋转) | 1-D float | 0 | unbounded | degrees | " |
  | Z Rotation (Z 轴旋转) | 1-D float | 0 | unbounded | degrees | " |

  - Matrix contribution: `Rx(rx) * Ry(ry) * Rz(rz)` (Z applied to a point first, then Y, then X), inside Orientation: `R = Or * Rot`. See AE-ANI-014.
  - Each property interpolates independently and linearly in degrees (gimbal effects are expected).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** matchNames `ADBE Rotate X`, `ADBE Rotate Y`, `ADBE Rotate Z`; Lottie `ks.rx`, `ks.ry`, `ks.rz` (degrees). AE help: "Orientation values specify an angular destination, Rotation values specify an angular route".

### AE-ANI-009 · Camera transform, Point of Interest, one-node and two-node cameras (摄像机变换、目标点、单节点摄像机/双节点摄像机)

- **What it does:** A camera is placed by Position plus either a Point of Interest it always looks at (two-node) or only its own rotations (one-node).
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Point of Interest (目标点) | 3-D float | (W/2, H/2, 0) | unbounded | comp px | spatial; present only on two-node cameras / auto-orient "Orient Towards Point of Interest" |
  | Position (位置) | 3-D float | (W/2, H/2, -Zoom) | unbounded | comp px | Zoom = the camera's Zoom value (e.g. 2666.7 px for the 50 mm preset at 1920×1080) so a z = 0 layer appears at 100 % |
  | Orientation (方向) | 3-D | (0,0,0) | wraps [0,360) | ° | |
  | X / Y / Z Rotation | 1-D each | 0 | unbounded | ° | |

  - Camera Type (类型) in Camera Settings: One-Node Camera (单节点摄像机) / Two-Node Camera (双节点摄像机). A two-node camera is exactly a camera whose Auto-Orient option is "Orient Towards Point of Interest"; switching to one-node hides POI (value kept).
  - With no camera layer, AE renders 3D layers with a default camera equivalent to a one-node camera at (W/2, H/2, -Zoom_default) looking along +z (compositing-3d-color.md, Cameras).
  - View-matrix math: AE-ANI-022.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-render)
- **Notes:** Scripting: the camera's Point of Interest uses matchName `ADBE Anchor Point` (it occupies the anchor slot); expression `transform.pointOfInterest`. Lottie camera layer `ty: 13`, transform `ks.a` = POI, `ks.p`, `ks.or`, `ks.rx/ry/rz`; perspective `pe` (= Zoom). Camera tools (Orbit/Pan/Dolly, Track XY/Z) move Position and/or POI; see ui-preview-render-io.md (Tools). Camera optics (Zoom, DOF, presets): compositing-3d-color.md, Cameras.

### AE-ANI-010 · Light transform (灯光变换)

- **What it does:** Places and aims a light; the properties present depend on Light Type.
- **Exact semantics:**

  | Light Type (灯光类型) | Transform properties |
  |---|---|
  | Parallel (平行) | Point of Interest, Position, Orientation, X/Y/Z Rotation; direction = from Position toward POI (after rotations) |
  | Spot (聚光) | Point of Interest, Position, Orientation, X/Y/Z Rotation; cone axis = local +z after orientation |
  | Point (点) | Position (UNVERIFIED: AE also shows Orientation/X/Y/Z Rotation for point lights; they have no visible effect except for shadows-casting orientation) |
  | Ambient (环境) | none |
  | Environment (环境光（译名待核）, Advanced 3D only) | Orientation/rotations only (UNVERIFIED), see compositing-3d-color.md |

  - Default Point of Interest = (W/2, H/2, 0). Default Position is offset up-right-toward-viewer from the comp centre (UNVERIFIED: (W/2 + W/36, H/2 − W/36, −Zoom/3), e.g. (1013.3, 486.7, −888.9) at 1920×1080).
  - Auto-Orient default for spot/parallel lights: Orient Towards Point of Interest. Choosing Off makes the light ignore its POI (AE-ANI-022).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-render)
- **Notes:** Lighting/shadow rendering: compositing-3d-color.md (Lights). Lottie light layers are not supported by lottie-web (export ignores them).

### AE-ANI-011 · Layer > Transform menu commands (图层 > 变换)

- **What it does:** Menu commands that set or reset transform values of the selected layers.
- **Exact semantics:** (menu order)

  | Command (中文) | Shortcut (Win / Mac) | Effect |
  |---|---|---|
  | Reset (重置) | — | resets all Transform properties to defaults (AE-ANI-001) |
  | Anchor Point… / Position… / Scale… / Orientation… / Rotation… / Opacity… (锚点… 等) | Position… Ctrl+Shift+P / Cmd+Shift+P; Rotation… Ctrl+Shift+R / Cmd+Shift+R; Opacity… Ctrl+Shift+O / Cmd+Shift+O; Anchor Point…, Scale…, Orientation…: none in the shortcut reference | opens the Edit Value dialog (AE-ANI-012) |
  | Flip Horizontal (水平翻转) | — | `scale.x *= -1` about the anchor point (AE help: "multiply the horizontal or vertical component of its Scale property value by -1"); UNVERIFIED on animated Scale: writes the flipped value at the CTI only (a user feature request reports that existing Scale keyframes are not flipped) |
  | Flip Vertical (垂直翻转) | — | `scale.y *= -1` |
  | Center Anchor Point in Layer Content (在图层内容中居中放置锚点) | Ctrl+Alt+Home / Cmd+Opt+Home | anchor = centre of the layer's content bounds (shape layer: bounds of all shapes; text: text bounds; masked layer: bounds of the masked visible area), Position compensated so the layer does not move |
  | Center In View (在视图中居中（译名待核）) | Ctrl+Home / Cmd+Home | changes Position so the anchor point of each selected layer is at the centre of the current view |
  | Fit to Comp (适合复合) | Ctrl+Alt+F / Cmd+Opt+F | non-uniform scale so the layer bounds exactly fill the comp; anchor/position centred |
  | Fit to Comp Width (适合复合宽度) | Ctrl+Alt+Shift+H / Cmd+Opt+Shift+H | uniform scale so width = comp width, centred |
  | Fit to Comp Height (适合复合高度) | Ctrl+Alt+Shift+G / Cmd+Opt+Shift+G | uniform scale so height = comp height, centred |
  | Auto-Orient… (自动定向…) | Ctrl+Alt+O / Cmd+Opt+O | Auto-Orientation dialog (AE-ANI-018) |

  - All commands are single undoable Commands and respect animated properties by writing the value at the CTI (adding a keyframe if the stopwatch is on).
- **Priority:** P1 (Reset, Fit to Comp, Center Anchor are P0-adjacent but non-blocking)
- **Owner:** bae-motion (+ bae-anim, bae-ui-motion)
- **Notes:** Flip Horizontal/Vertical are menu commands of current AE (documented in the AE 2024+ help); in CS3 flipping was done only by negative Scale (UNVERIFIED: exact version that added the menu commands). Source: https://helpx.adobe.com/after-effects/using/layer-properties.html Content-bounds computation shares code with `sourceRectAtTime()` (expression part of this file). Menu framework: ui-preview-render-io.md (Menus).

### AE-ANI-012 · Edit Value dialogs for transform properties (编辑值)

- **What it does:** Right-click > Edit Value (or Layer > Transform > …) opens a numeric dialog with selectable units.
- **Exact semantics:**

  | Property | Units choices (中文) | Conversion |
  |---|---|---|
  | Position / Anchor Point | pixels (像素), inches, millimeters, % of source (源的 %), % of composition (合成的 %) (UNVERIFIED exact list per property) | % of composition: `px = pct/100 * comp_size`; % of source: `px = pct/100 * layer_size` |
  | Scale | % of source, pixels (Width/Height), + "Include Pixel Aspect Ratio" (包括像素长宽比) | `px = scale/100 * source_px` (× PAR when included) |
  | Rotation | revolutions + degrees | `v = 360*rev + deg` |
  | Opacity | % | clamp 0–100 |

  - Applying the dialog writes the value at the CTI (keyframe if animated) as one undo step.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Dialog layout belongs to ui-preview-render-io.md; listed here because the unit conversion math is property semantics.

## Transform math

### AE-ANI-013 · 2D layer transform matrix (二维图层变换矩阵)

- **What it does:** Defines the exact affine map from a 2D layer's layer space to its parent space.
- **Exact semantics:**

  ```
  // column vectors, y down, angles in degrees, scale in percent
  M_local_2D = T(px, py) * Rz(rotation + autoOrientAngle) * S(sx/100, sy/100) * T(-ax, -ay)
  // Rz(θ) = [[cos θ, -sin θ], [sin θ, cos θ]]   (positive = clockwise on screen because y is down)
  // autoOrientAngle = 0 unless Auto-Orient = Orient Along Path (AE-ANI-019)
  ```

  - Embedded in 4×4 form with z untouched (`z' = z`) when combined with 3D parents/children (AE-ANI-016).
  - No skew exists in AE layer transforms (skew exists only in shape-group transforms, see shapes.md, Group transforms).
  - Test vector: layer 100×100, anchor (50,50), position (960,540), rotation 90, scale (100,100). Layer point (100,50) → `T(-a)` (50,0) → `Rz(90)` (0,50) → (960,590). Scale (200,50), rotation 0: layer point (0,0) → (-50,-50) → (-100,-25) → (860,515).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Verified against lottie-web `TransformProperty.js` (call order translate(-a) → scale → rotate(-r) → translate(p) with a row-vector matrix whose `rotate(θ)` is the inverse of the standard rotation, so `rotate(-r)` = standard `Rz(+r)`), and against AE's documented clockwise-positive rotation. Implementation hint: evaluate in f64 on the CPU per layer per sample time and upload f32 matrices to the GPU; keep anchor/position in f64 because comps can be very large.

### AE-ANI-014 · 3D layer transform matrix and rotation order (三维图层变换矩阵)

- **What it does:** Defines the exact map from a 3D layer's layer space to parent space, including the order of Orientation versus X/Y/Z Rotation.
- **Exact semantics:**

  ```
  // AE coordinates: +x right, +y down, +z away from viewer (right-handed); column vectors
  Rx(a) = [[1,0,0],[0,cos a,-sin a],[0,sin a,cos a]]
  Ry(a) = [[cos a,0,sin a],[0,1,0],[-sin a,0,cos a]]
  Rz(a) = [[cos a,-sin a,0],[sin a,cos a,0],[0,0,1]]

  Or  = Rx(or.x) * Ry(or.y) * Rz(or.z)          // Orientation: z applied first, then y, then x
  Rot = Rx(xRot) * Ry(yRot) * Rz(zRot)          // X/Y/Z Rotation: Z first, then Y, then X
  L   = autoOrientRotation()                    // identity unless Auto-Orient (AE-ANI-019..022)

  M_local_3D = T(px, py, pz) * L * Or * Rot * S(sx/100, sy/100, sz/100) * T(-ax, -ay, -az)
  ```

  - So a point is: moved by -anchor, scaled, rotated by Z Rotation, then Y Rotation, then X Rotation, then Orientation z, y, x, then the auto-orient rotation, then translated to Position.
  - Sign check (derived): X Rotation +90° maps layer point (0,-50,0) (above the anchor) to (0,0,-50): the top edge comes toward the viewer. Y Rotation +90° maps (50,0,0) to (0,0,-50): the right edge comes toward the viewer. Z Rotation +90° maps (50,0,0) to (0,50,0): clockwise on screen.
  - A 3D layer whose Orientation and X/Y Rotation are 0 and z values are 0 gives exactly the 2D matrix of AE-ANI-013.
  - Cameras and lights use the same order without scale and anchor: `C_local = T(pos) * L * Or * Rot`.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Order verified against lottie-web `TransformProperty.js` (`rotateZ(-rz).rotateY(ry).rotateX(rx).rotateZ(-or[2]).rotateY(or[1]).rotateX(or[0])` then `translate(p.x, p.y, -p.z)`, row-vector, lottie z toward viewer) and Skottie's `TransformAdapter3D` (`T(p)·Rx·Ry·Rz·S·T(-a)`). Skottie adds orientation and rotation per axis before building the matrix, which is only an approximation; BAE must use `Or * Rot`. UNVERIFIED for cameras: lottie-web's `HCameraElement` inverts in the order rotation-then-orientation, i.e. `C = T·L·Rot·Or`; for layers it is `Or·Rot`. BAE decision: use `Or * Rot` for cameras too unless an AE measurement shows otherwise (they only differ when both Orientation and Rotation are non-zero). A 2025 Adobe community answer claims AE applies X first about the fixed comp axes (`Rz·Ry·Rx`); this contradicts both lottie-web and Skottie (`Rx·Ry·Rz`, Z first, so Z Rotation always spins the layer in its own plane) and is not adopted. Expression `toWorld()/toComp()` and `lookAt()` must use this same matrix (expression part of this file).

### AE-ANI-015 · Parenting world matrix and inherited attributes (父级变换)

- **What it does:** A child layer's transform is expressed in its parent's layer space; the world (comp-space) matrix is the product of the ancestors' local matrices.
- **Exact semantics:**

  ```
  world(layer, t) = (layer.parent ? world(layer.parent, t) : I) * M_local(layer, t)
  // the parent's matrix is evaluated at the same comp time t (each layer converts t to its own layer time,
  // AE-ANI-087), using the parent's full Anchor, Position, Scale, Rotation/Orientation and auto-orient
  ```

  - The child's Position is in the parent's **layer space** (origin = parent's layer-space origin, i.e. top-left of the parent's source rectangle), not relative to the parent's anchor point.
  - Inherited: everything in the transform matrix (anchor, position, scale incl. negative, rotation, orientation, auto-orient). Not inherited: Opacity, effects, masks, blend mode, switches, time remap, time stretch, visibility. A parent whose Video switch is off or that is outside its in/out range still drives its children (its transform is evaluated for any time).
  - Non-uniform parent scale combined with child rotation produces skew in the child's world matrix (a general affine matrix; no decomposition is needed for rendering).
  - Cycles are forbidden (a layer cannot be parented to itself or a descendant); parent must be in the same comp.
  - Parenting is applied before motion blur sampling: a child of a moving parent is motion-blurred by the parent's motion (AE-ANI-105).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Parent column and pick-whip UI: ui-preview-render-io.md; parent data field and the Alt/Shift pick-whip variants: project-comp-layers.md, Parenting. Expression `parent`, `hasParent`, `toWorld`: expression part of this file. Lottie: layer `parent` = parent `ind`; lottie-web multiplies parent matrices in the same order.

### AE-ANI-016 · Mixed 2D/3D parenting (二维与三维图层混合父级)

- **What it does:** Defines how a 2D layer inherits from a 3D parent and vice versa.
- **Exact semantics:**
  - 3D child of a 2D parent: the parent's 2D matrix is embedded as a 4×4 with `z' = z` (no z scale, z translation 0); the child is placed in 3D normally.
  - 2D child of a 3D parent: the full 3D world matrix of the parent chain is applied to the child's 2D matrix and then **flattened orthographically** (z dropped) to a 2D affine: `A = drop_z(world_parent_3D) * M_local_2D`, where `drop_z` keeps the upper-left 2×2 of the x/y rows and the x/y translation. A parent Y Rotation of 60° therefore squeezes the 2D child horizontally by cos 60° = 0.5 with no perspective, and the parent's z position and the camera have no effect on the 2D child. UNVERIFIED: reconstructed from observed AE behaviour (2D layers never get perspective); confirm with an AE test comp.
  - 2D layers ignore their own z values (AE-ANI-003).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Render grouping of 2D/3D layers (2D layers break 3D groups): compositing-3d-color.md, Classic 3D renderer.

### AE-ANI-017 · Parent assignment compensation (指定父级时的变换补偿)

- **What it does:** When a parent is assigned or removed, AE rewrites the child's transform so it stays visually where it was at the current time.
- **Exact semantics:**

  ```
  on set_parent(child, P_new) at comp time t:          // default (no modifier)
      W  = world(child, t)                              // with the old parent
      L' = inverse(world(P_new, t)) * W                 // desired new local matrix
      decompose L' into position, scale, rotation (2D) or position, scale, orientation/rotation (3D)
      apply the same correction to every keyframe of Position, Scale, Rotation using the parent's
      matrix at time t (a static re-basing, not per-keyframe-time)   // UNVERIFIED: AE re-bases all keys with the CTI matrix
  ```

  - Anchor Point is never changed. Opacity is never changed.
  - If L' contains skew (non-uniform parent scale + rotation), the decomposition cannot be exact; the child's look changes. UNVERIFIED: AE keeps scale/rotation from a polar decomposition and accepts the error.
  - Modifier variant (Alt/Option while choosing the parent): no compensation; the child's existing local values are reinterpreted in the new parent space (it "jumps"). See project-comp-layers.md, Parenting, for the exact modifier mapping.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Must be a single undoable Command. Removing the parent (None) applies the same algorithm with `world(P_new) = I`.

### AE-ANI-018 · Auto-Orientation options (自动定向)

- **What it does:** Per-layer setting that adds an automatic rotation derived from the motion path, the active camera or the point of interest.
- **Exact semantics:** (dialog order)

  | Option (中文) | Available for | Scripting `AutoOrientType` | Lottie |
  |---|---|---|---|
  | Off (关) | all | `AutoOrientType.NO_AUTO_ORIENT` | `ao: 0` |
  | Orient Along Path (沿路径定向) | 2D/3D AV layers, cameras, lights | `AutoOrientType.ALONG_PATH` | `ao: 1` |
  | Orient Towards Camera (定位于摄像机) | 3D AV layers only | `AutoOrientType.CAMERA_OR_POINT_OF_INTEREST` | — |
  | Orient Towards Point of Interest (定向到目标点) | cameras and lights only | `AutoOrientType.CAMERA_OR_POINT_OF_INTEREST` | — |
  | Orient Each Character Independently (独立定向每个字符) | 3D text layers, sub-option of Orient Towards Camera | `AutoOrientType.CHARACTERS_TOWARD_CAMERA` | — |

  - The automatic rotation `L` is applied **outside** the layer's own Orientation and X/Y/Z Rotation (AE-ANI-014): manual rotations act as an offset in the auto-oriented frame (e.g. a camera oriented along a path plus Y Rotation 90° looks out of the side window).
  - Defaults: AV layers Off; two-node cameras and spot/parallel lights Orient Towards Point of Interest; one-node cameras Off.
- **Priority:** P0 (Off/Along Path/Towards POI), P1 (Towards Camera, per-character)
- **Owner:** bae-anim (+ bae-model, bae-ui-motion)
- **Notes:** zh-CN terms from the AE help (cn) page "选择和排列图层": 关 / 沿路径定向 / 定位于摄像机 / 定向到目标点; the shortcut reference calls the dialog "自动方向". Orient Towards Camera is not available for 2D layers, cameras or lights; Orient Towards Point Of Interest only for cameras and lights (AE help). Shortcut Ctrl+Alt+O / Cmd+Opt+O. Per-character orientation math belongs to text.md (Per-character 3D). The setting is stored per layer (`layer.autoOrient`); data item in project-comp-layers.md (Auto-orient data).

### AE-ANI-019 · Orient Along Path for 2D layers (沿路径定向（二维）)

- **What it does:** Rotates a 2D layer so its +x axis follows the tangent of its Position motion path.
- **Exact semantics:**

  ```
  autoOrientAngle(t) = atan2(dp.y, dp.x)   in degrees, where dp = d(position)/dt at layer time t
  // dp is the tangent of the spatial path, i.e. the derivative of the spatial Bezier segment at the
  // arc-length parameter reached at time t (AE-ANI-037); for separated dimensions dp = (dX/dt, dY/dt)
  ```

  - Before the first Position keyframe the tangent at the first keyframe (outgoing) is used; after the last keyframe the incoming tangent at the last keyframe; at a Hold segment or where the speed is 0 the direction is the path tangent direction (Bezier derivative with respect to the curve parameter), not undefined. UNVERIFIED: if the spatial segment is degenerate (both keys equal and zero tangents) AE keeps the previous non-degenerate direction.
  - The angle is added to Rotation inside `Rz(rotation + autoOrientAngle)`; Rotation stays editable as an offset.
  - Position with no keyframes: angle 0. Position driven by an expression: tangent is computed by finite difference of the expression result (UNVERIFIED step size; lottie-web uses 0.01 frame backward difference).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** lottie-web approximates with `-atan2(p(t) - p(t - 0.01 frame))` and, before the first key, `p(t0 + 0.01) - p(t0)`; after the last key, `p(tN) - p(tN - 0.05)` (frames). Implementation hint: use the analytic Bezier derivative; fall back to a central difference of ±1/1000 frame for expressions.

### AE-ANI-020 · Orient Along Path for 3D layers, cameras and lights (沿路径定向（三维）)

- **What it does:** Rotates a 3D layer, camera or light so it points along the 3D Position path.
- **Exact semantics:**

  ```
  d = normalize(dp/dt)                          // 3D path tangent, as AE-ANI-019
  // Cameras and lights: local +z (view / beam axis) points along d, no roll:
  yaw   = atan2(d.x, d.z)
  pitch = atan2(-d.y, hypot(d.x, d.z))
  L_cam = Ry(yaw) * Rx(pitch)
  // 3D AV layers: local +x points along d (reduces to the 2D rule when d.z = 0):   // UNVERIFIED
  alpha = atan2(d.y, d.x)
  beta  = atan2(-d.z, hypot(d.x, d.y))
  L_layer = Rz(alpha) * Ry(beta)
  ```

  - `L` is inserted as in AE-ANI-014. When d is undefined use the rule of AE-ANI-019.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** UNVERIFIED: the camera formula is the no-roll look-along construction that lottie-web uses for POI cameras (`rotateY(atan2(look.x, -look.z))`, `rotateX(-atan2(look.y, lenXZ))` in its z-toward-viewer space); the axis AE aligns for 3D AV layers and its roll convention must be measured in AE.

### AE-ANI-021 · Orient Towards Camera (定位于摄像机)

- **What it does:** Keeps a 3D layer turned so it faces the active camera (billboarding).
- **Exact semantics:**

  ```
  c = world position of the active camera at time t (comp default camera if none)
  d = normalize(c - layer_world_position)      // from layer toward camera
  // layer's local -z (its front normal, which faces the viewer when unrotated) must point along d:
  yaw   = atan2(-d.x, -d.z)
  pitch = atan2(d.y, hypot(d.x, d.z))
  L = Ry(yaw) * Rx(pitch)                       // no roll; layer stays upright
  ```

  - UNVERIFIED: AE faces the camera **position** (point billboarding) rather than the camera plane; with a parent, `L` is computed in world space and converted to the parent's space.
  - Only for 3D AV layers. Orient Each Character Independently applies the same per character about each character's anchor (text.md).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Common expression equivalent used by users: `lookAt(position, thisComp.activeCamera.position)` on Orientation.

### AE-ANI-022 · Orient Towards Point of Interest and camera view matrix (定向到目标点与摄像机视图矩阵)

- **What it does:** Aims a camera or light at its Point of Interest and defines the camera's view matrix used for rendering 3D layers.
- **Exact semantics:**

  ```
  // two-node camera / light with Orient Towards Point of Interest
  d     = normalize(POI - Position)             // both in the camera's parent space
  yaw   = atan2(d.x, d.z)
  pitch = atan2(-d.y, hypot(d.x, d.z))
  L     = Ry(yaw) * Rx(pitch)                   // local +z -> d, no roll (camera up stays toward -y)
  // POI == Position: d undefined -> L = I (UNVERIFIED)
  // straight up/down (hypot == 0): yaw = 0 (UNVERIFIED; AE flips near the poles)

  C_world = world(parent) * T(Position) * L * Or * Rot      // AE-ANI-014, no scale/anchor
  View    = inverse(C_world)
  // camera looks along its local +z; local +y is screen-down
  ```

  - The automatic aim happens before (inside) Orientation and Rotation in the sense that those act as offsets relative to the aimed frame (AE help: "occurs before the Rotation and Orientation transformations are applied").
  - The POI is in the same space as Position (parent space) and is itself transformed by the parent.
  - One-node camera: `L = I`; it looks along +z rotated by Orientation/Rotation.
  - Projection (Zoom, film size, comp centre as principal point) is specified in compositing-3d-color.md, Cameras. Test: default camera at (W/2, H/2, -Zoom), POI (W/2, H/2, 0): `d = (0,0,1)`, `L = I`, a z = 0 layer at 100 % scale projects 1:1.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-render)
- **Notes:** Verified against lottie-web `HCameraElement.js` (look vector, `atan2(look.x, -look.z)` and `atan2(look.y, lenXZ)` in lottie's z-toward-viewer space, order translate(-p) → lookAt inverse → rotation/orientation inverse). Light direction for spot/parallel lights = `C_world * (0,0,1,0)`.

## Keyframe model

### AE-ANI-023 · Keyframe data model (关键帧)

- **What it does:** A keyframe stores a value at a time plus the interpolation settings of the segments that enter and leave it.
- **Exact semantics:** Fields of one keyframe (scripting accessor in parentheses):

  | Field (中文) | Type | Default for a new UI keyframe | Range | Notes |
  |---|---|---|---|---|
  | time (时间) | rational / flicks | CTI | any; unique per property | `keyTime(i)`; see AE-ANI-024 |
  | value (值) | property value type | value at CTI | type-specific | `keyValue(i)` |
  | inInterpolationType / outInterpolationType (传入/传出插值) | enum Linear / Bezier / Hold | Linear / Linear (temporal) | — | `keyInInterpolationType(i)`; scripting `KeyframeInterpolationType.LINEAR` (6612), `.BEZIER` (6613), `.HOLD` (6614) |
  | inTemporalEase / outTemporalEase (传入/传出缓动) | array of {speed, influence}: 2 entries for `TwoD`, 3 for `ThreeD`, 1 entry for every other value type (spatial, `COLOR`, shape, text, 1-D) per the scripting guide (`keyInTemporalEase` returns) | speed = segment slope, influence 16.666667 % (Linear) | influence 0.1–100 %, speed unbounded | AE-ANI-033 |
  | temporalContinuous (时间连续) | bool | false | — | AE-ANI-041 |
  | temporalAutoBezier (时间自动贝塞尔) | bool | false | — | AE-ANI-040 |
  | inSpatialTangent / outSpatialTangent (传入/传出空间切线) | vector, same dimension as value | from auto-bezier computation (AE-ANI-042) or (0,…) when Linear | unbounded | spatial properties only; relative to the key's value |
  | spatialContinuous (空间连续) | bool | true for Auto Bezier keys | — | spatial only |
  | spatialAutoBezier (空间自动贝塞尔) | bool | true unless the "Default Spatial Interpolation to Linear" preference is on (AE-ANI-044) | — | spatial only |
  | roving (漂浮) | bool | false | — | spatial only; first/last key never rove (AE-ANI-051) |
  | label (标签) | int 0–16 | 0 (None) | 0 = none, 1–16 = label colours from Preferences > Labels | AE 22.6 (2022)+; `keyLabel(i)`, `setLabelAtKey(i, n)` |
  | selected (选中) | bool | — | — | `keySelected(i)`; UI state, persisted in the project |

  - A keyframe's "interpolation method" shown to users is derived: Hold = out type Hold; Linear = type Linear; Bezier = Bezier with continuous=false, autoBezier=false; Continuous Bezier = Bezier + continuous; Auto Bezier = Bezier + autoBezier (+continuous). Temporal and spatial methods are independent.
  - Hold is an **outgoing** method: a segment k0→k1 holds iff `out(k0) == Hold`. `in(k1) == Hold` only changes the icon/half-icon state; when a key is created after a Hold key its in type becomes Hold.
  - Segment type: Hold if out(k0)=Hold; Linear if out(k0)=Linear and in(k1)=Linear; otherwise Bezier, where a Linear side contributes an ease of speed = segment average speed and influence 16.666667 % (UNVERIFIED: the value AE reports via `keyOutTemporalEase` for a Linear side; bodymovin exports Linear keys as `o:{x:0.167,y:0.167}` / `i:{x:0.833,y:0.833}`, consistent with this).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Lottie keyframe: `{t, s, h, o:{x,y}, i:{x,y}, to, ti}` (`h:1` = hold; `to`/`ti` spatial tangents; `e` end value in old exports). Keyframe icons per interpolation: ui-preview-render-io.md (Timeline, keyframe icons). BAE decision: store the ease as AE's (speed, influence) pairs, not as normalised Lottie handles, so values survive time stretching and editing exactly as in AE; derive normalised handles on demand.

### AE-ANI-024 · Keyframe time storage and frame alignment (关键帧时间)

- **What it does:** Defines where in time keyframes can sit and how they relate to frames.
- **Exact semantics:**
  - Keyframe times are stored in **layer time** (time relative to the layer's own timeline, so moving the layer moves its keys), not comp time. See AE-ANI-087 for the mapping.
  - Times are exact rationals; keys created in the UI land exactly on frame boundaries of the comp frame rate (as seen through the layer's start time and stretch). Fractional (between-frame) times arise from scripting (`setValueAtTime`), time-stretching a layer, changing the comp frame rate, Alt-drag scaling of keys, and pasting between comps of different frame rates; AE keeps them and evaluates them exactly.
  - Dragging a keyframe in the Timeline snaps to whole frames (of the comp frame rate as displayed); a between-frame key that is nudged (Alt+→) moves by exactly one frame duration and stays between frames.
  - At most one keyframe per property per time. Moving a key onto another key's time replaces that key (UNVERIFIED: AE deletes the overwritten key on drop).
  - Keys may exist before the layer In point or after its Out point; they still affect interpolation.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** BAE stores times as i64 flicks; the NTSC rates are exact in flicks (29.97 = 30000/1001: 705,600,000 × 1001 / 30000 = 23,543,520 flicks per frame; 23.976 = 24000/1001: 29,429,400 flicks per frame), but arbitrary rationals from scripting or stretch factors may not be, so round to the nearest flick on entry. Implementation hint: when comparing a key time with a frame time use an epsilon of 1 flick.

### AE-ANI-025 · Stopwatch and auto-keyframing (秒表)

- **What it does:** The stopwatch switches a property between a static value and keyframed animation; while it is on, every value edit is recorded as a keyframe at the CTI.
- **Exact semantics:**

  ```
  stopwatch_on(prop):   add key(time = CTI, value = prop.static_value); defaults per AE-ANI-023
  stopwatch_off(prop):  v = prop.value_at(CTI) (keyframes only, expression ignored);
                        delete all keys; prop.static_value = v
  edit_value(prop, v):  if prop.animated: if key at CTI: key.value = v else add key(CTI, v)
                                          (new key's interpolation: AE-ANI-027)
                        else prop.static_value = v
  ```

  - Turning the stopwatch off is undoable and is the only bulk "delete all keyframes" that keeps the current value.
  - Alt/Option-click the stopwatch adds/removes an expression instead (expression part of this file).
  - Properties that cannot vary over time (`canVaryOverTime == false`, e.g. Puppet mesh settings, some effect buttons; layer name and switches are not properties) have no stopwatch. Effect menus and checkboxes do have one (they animate with Hold, AE-ANI-056).
  - Editing a value while the CTI is outside the layer's in/out range still creates a keyframe at that time.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `Property.numKeys`, `addKey(time)`, `removeKey(i)`, `setValueAtTime`. The keyframe navigator (◀ ◆ ▶) and the I-beam shown for unanimated properties are UI (ui-preview-render-io.md, Timeline).

### AE-ANI-026 · Spatial vs temporal properties (空间属性与时间属性)

- **What it does:** Classifies properties by whether their values are positions with a motion path, which changes interpolation semantics.
- **Exact semantics:**

  | Class | Properties | Interpolation |
  |---|---|---|
  | Spatial (`isSpatial == true`, value type `TwoD_SPATIAL` / `ThreeD_SPATIAL`) | Position, Anchor Point, Point of Interest, effect Point Controls (2-D/3-D points, e.g. Lens Flare centre), Puppet pin positions, motion-tracker attach points | temporal ease is **one** 1-D curve driving arc length along a spatial Bezier path (AE-ANI-035); spatial interpolation + roving available |
  | Non-spatial multi-dimensional (`TwoD`, `ThreeD`) | Scale, 2-D/3-D values of effects that are not points | one temporal ease **per dimension** (AE-ANI-034) |
  | Colour (`COLOR`, 4 floats RGBA) | colour properties of effects, lights, shapes, text | **one** temporal ease shared by all four components (AE-ANI-034, AE-ANI-053) |
  | 1-D (`OneD`) | Rotation, Opacity, sliders, angles | one ease |
  | Other (`SHAPE`, `TEXT_DOCUMENT`, `MARKER`, `LAYER_INDEX`, `MASK_INDEX`, `CUSTOM_VALUE`, `NO_VALUE`) | mask/shape paths, Source Text, markers, layer/mask pickers, menus/checkboxes (`OneD` but discrete) | AE-ANI-053..058 |

  - Separated Position (X/Y/Z Position) are 1-D non-spatial properties (AE-ANI-052).
  - Orientation is `ThreeD_SPATIAL` in scripting (evidence: bodymovin calls `keyOutSpatialTangent` only for spatial types and does export `to`/`ti` for `ks.or`, which lottie-web then discards) but has no motion path and interpolates by shortest arc (AE-ANI-057); its temporal ease is a single curve.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting `Property.isSpatial`, `Property.propertyValueType`. Bodymovin export treats `TwoD_SPATIAL`, `ThreeD_SPATIAL`, `SHAPE` and `NO_VALUE` as single-ease and `OneD/TwoD/ThreeD/COLOR` as per-dimension, which confirms the split.

### AE-ANI-027 · Interpolation of newly created keyframes (新关键帧的默认插值)

- **What it does:** Decides which interpolation a keyframe gets when it is created.
- **Exact semantics:**
  - First keyframe of a property (stopwatch on): temporal Linear in/out; spatial Auto Bezier (or Linear when the preference AE-ANI-044 is on).
  - Additional keyframe added by editing the value at the CTI or by "Add keyframe": UNVERIFIED: AE copies the temporal and spatial interpolation types of the nearest preceding keyframe (or of the following one when added before the first key). Documented: a key created after a key whose out type is Hold gets incoming Hold (AE help, Keyframe interpolation).
  - Keys created by assistants and expressions-to-keyframes use the types defined in those items.
  - Changing the spatial-interpolation preference affects neither existing keys nor new keys on properties that already have keys (AE help).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Measure in AE before M1: create Bezier keys, add a key between them, read `keyInInterpolationType`.

### AE-ANI-028 · Keyframe colour labels (关键帧颜色标签（译名待核）)

- **What it does:** Each keyframe can carry one of the 16 label colours for organisation; labels are displayed on the keyframe icon and can be used to select keys.
- **Exact semantics:** `label ∈ {0 = None, 1..16}`; colours and names come from Preferences > Labels (shared with layer/item labels, see project-comp-layers.md, Labels). Set via right-click on keyframe > Label (标签) > colour, or "Select Label Group"? UNVERIFIED: AE 22.6 added a keyframe context-menu Label submenu with "Select Label Group" selecting all keys of the same label in the layer/property.
- **Priority:** P2
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Added in AE 22.6 (2022). Scripting `keyLabel`, `setLabelAtKey`. Not exported to Lottie.

### AE-ANI-029 · Keyframe selection model (选择关键帧)

- **What it does:** Keyframes are individually selectable; most keyframe commands act on the selection.
- **Exact semantics:**
  - Selection is a set of (property, key) pairs across layers; persisted in the project (`keySelected`).
  - Click selects one (clears others); Shift-click toggles; marquee-drag selects; clicking a property name selects all its keys; Ctrl+Alt+A / Cmd+Opt+A selects all visible keyframes and properties; Shift+F2 or Ctrl+Alt+Shift+A / Cmd+Opt+Shift+A deselects all keyframes, properties and groups (AE shortcut reference). Alt/Option-clicking two keys shows the duration between them in the Info panel.
  - Right-click > Select Equal Keyframes (选择相同关键帧) selects keys of that property with the same value; Select Previous Keyframes / Select Following Keyframes (选择前面的关键帧 / 选择跟随关键帧（译名待核）) select all keys of the property before/after the clicked key.
  - Selecting a layer's motion path keyframe in the comp viewer selects the same key in the Timeline.
- **Priority:** P0 (basic selection), P1 (Equal/Previous/Following)
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Mouse/shortcut details: ui-preview-render-io.md (Timeline, shortcuts).

## Temporal interpolation

### AE-ANI-030 · Value outside the keyframe range (关键帧范围外的值)

- **What it does:** Defines the keyframed value before the first and after the last keyframe.
- **Exact semantics:**

  ```
  value(t) = keys[0].value       for t <= keys[0].time
  value(t) = keys[n-1].value     for t >= keys[n-1].time
  value(t) = keys[0].value       for every t when n == 1
  // in-range: find k with keys[k].time <= t < keys[k+1].time and evaluate that segment
  // t exactly at a key time returns that key's value (even if the previous segment is Hold)
  ```

  - No extrapolation of the first/last ease (the curve is flat outside the range); looping beyond the range is only via expressions (`loopIn/loopOut`, expression part of this file).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Matches lottie-web `interpolateValue` (clamps to first/last `s`). Implementation hint: binary search the segment, cache the last segment index per property for sequential playback.

### AE-ANI-031 · Linear temporal interpolation (线性插值)

- **What it does:** Changes the value at a constant rate between two keys whose facing sides are both Linear.
- **Exact semantics:**

  ```
  // segment k0 -> k1 with out(k0) == Linear and in(k1) == Linear
  p = (t - t0) / (t1 - t0)
  non-spatial: v(t) = v0 + (v1 - v0) * p                         // per dimension
  spatial:     v(t) = pathPoint(arcLengthToParam(p * L))         // constant speed L/(t1-t0) along the
                                                                 // spatial path, which may be curved (AE-ANI-036)
  ```

  - Displayed speed for a Linear segment: `|v1 - v0| / (t1 - t0)` per dimension (non-spatial) or `L / (t1 - t0)` (spatial) in units per second; the speed jumps at each Linear key.
  - The temporal ease fields of a Linear side report speed = that average speed and influence 16.666667 % (AE-ANI-023).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Temporal Linear does not imply spatial Linear: a Position key can be temporally Linear (constant speed) on a curved Auto Bezier path.

### AE-ANI-032 · Hold interpolation (定格插值)

- **What it does:** Keeps the value of a key unchanged until the next key, then jumps.
- **Exact semantics:**

  ```
  if out(k0) == Hold:  v(t) = v0   for t0 <= t < t1;   v(t1) = v1
  ```

  - Hold is temporal only; the motion path shows the keys but no dots between them.
  - `in(k1)` is irrelevant for the segment's shape. Toggle Hold Keyframe (AE-ANI-050) applies or removes Hold as the **outgoing** interpolation of the selected keys (AE help: "To apply or remove Hold interpolation as outgoing interpolation"); UNVERIFIED: whether it also changes the selected key's incoming type.
  - Sub-frame evaluation (motion blur, frame blending, time remap) sees the jump exactly at t1, not at a frame boundary: a motion-blur sample at t1 − ε shows v0.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Lottie `h: 1` on the outgoing keyframe. Freeze Frame (AE-ANI-092) is built on a Hold Time Remap key.

### AE-ANI-033 · Bezier temporal interpolation: speed/influence to cubic Bezier (贝塞尔曲线插值：速度与影响)

- **What it does:** Maps AE's per-key ease (speed, influence) to a cubic Bezier in the (time, value) plane and evaluates it.
- **Exact semantics:** For one dimension of a segment k0(t0, v0) → k1(t1, v1):

  ```
  dt = t1 - t0
  so, io = k0.outTemporalEase.speed, k0.outTemporalEase.influence / 100   // speed in units/s, signed
  si, ii = k1.inTemporalEase.speed,  k1.inTemporalEase.influence  / 100   // si = incoming slope at k1
  P0 = (t0,            v0)
  P1 = (t0 + io * dt,  v0 + so * io * dt)
  P2 = (t1 - ii * dt,  v1 - si * ii * dt)
  P3 = (t1,            v1)
  // evaluate: find u in [0,1] with Bx(u) = t, return By(u)
  Bx(u) = (1-u)^3 t0 + 3(1-u)^2 u P1.x + 3(1-u) u^2 P2.x + u^3 t1
  ```

  - Because 0 < io, ii ≤ 1, `Bx` is monotonic on [0,1] (its derivative's Bernstein coefficients satisfy (io+ii−1)² ≤ io·ii), so the solve has one root.
  - The slope at k0 is exactly `so` and at k1 exactly `si`; influence sets how far (as a fraction of the segment duration) the speed "persists".
  - Overshoot is allowed (values may go beyond [v0, v1]); for properties with clamped ranges (Opacity, effect sliders with hard limits) the result is clamped after interpolation.
  - v0 == v1 with non-zero speeds produces a bump (non-constant segment).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Verified against bodymovin `keyframeHelper.jsx`: `o.x = inf_out/100`, `o.y = speed_out*inf_out/100*dt/Δv`, `i.x = 1 − inf_in/100`, `i.y = 1 − speed_in*inf_in/100*dt/Δv`, which is the normalisation of the control points above. Implementation hint: Newton iteration (4 steps, slope threshold 0.001) with bisection fallback (precision 1e-7, max 10 iterations), as in lottie-web `BezierEaser.js`; precompute an 11-sample table per segment. For BAE accuracy use f64 and solve to 1e-9 s.

### AE-ANI-034 · Per-dimension temporal ease for multi-dimensional non-spatial properties (多维非空间属性的分维缓动)

- **What it does:** Non-spatial 2-D/3-D properties (`TwoD`, `ThreeD`, e.g. Scale) have an independent temporal curve for each dimension; colour properties have a single shared curve.
- **Exact semantics:**
  - For each dimension d, apply AE-ANI-033 with `v0[d], v1[d]`, `speed[d]`, `influence[d]`. The `x` solve is per dimension (different influences give different u for the same t).
  - Interpolation types (Linear/Bezier/Hold) are shared by all dimensions of the key; eases differ.
  - Colour properties (`COLOR`) are **not** per-dimension: the scripting guide states `keyInTemporalEase`/`keyOutTemporalEase` return 1 KeyframeEase for every value type other than `TwoD`/`ThreeD`, and Lottie colour keyframes carry single-element `o.x/o.y/i.x/i.y` arrays. One ease curve gives a scalar progress `p(t)` in [0,1] and every component is `c = c0 + (c1 − c0) * p(t)`. Components are 0–1 floats in scripting but ease speed is in 8-bit units per second (0–255 scale; bodymovin multiplies colour deltas by 255 when normalising). UNVERIFIED: which scalar "distance" AE uses to relate the colour speed to `p` (bodymovin normalises by component 0's `255*Δ` only, which is probably an exporter shortcut; best guess: Euclidean length of the 255-scaled RGBA difference, by analogy with spatial properties). Interpolation space: AE-ANI-053.
  - Easy Ease and the Keyframe Velocity dialog set all dimensions at once unless the dimensions are edited separately in the Graph Editor.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Scripting: `setTemporalEaseAtKey(i, inEase[, outEase])` takes arrays of 2 KeyframeEase objects for `TwoD`, 3 for `ThreeD`, 1 for all other types including `COLOR` (scripting guide, Property.keyInTemporalEase). Source: https://ae-scripting.docsforadobe.dev/property/property/

### AE-ANI-035 · Temporal ease of spatial properties (arc-length driven) (空间属性的时间缓动)

- **What it does:** For Position, Anchor Point, POI and other spatial properties, one ease curve controls the distance travelled along the spatial path, so speed is in pixels per second along the arc.
- **Exact semantics:**

  ```
  L  = arcLength(segment k0 -> k1)                     // AE-ANI-036/037
  dt = t1 - t0
  // 1-D ease on distance s in [0, L], speeds are path speeds (px/s, >= 0)
  P0 = (t0, 0)
  P1 = (t0 + io*dt, so*io*dt)
  P2 = (t1 - ii*dt, L - si*ii*dt)
  P3 = (t1, L)
  s(t)     = By(u) where Bx(u) = t
  value(t) = spatialBezier(k0,k1).pointAtArcLength(s(t))
  ```

  - Spatial speeds are magnitudes (never negative). If `so*io*dt > L` (or `si*ii*dt > L`), the effective influence is reduced to `L/(so*dt)` (resp. `L/(si*dt)`) so the handle does not pass the path end (UNVERIFIED: AE behaviour inferred from bodymovin, which applies exactly this clamp `inf = min(100*L/(speed*dt), inf)`).
  - L == 0 (keys at the same point, zero tangents): value constant; speeds ignored.
  - Separated dimensions are not spatial: each X/Y/Z Position is a 1-D property (AE-ANI-052).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** This is why a Position segment shows a single speed curve in the Graph Editor's speed graph, and why its value graph shows each dimension only for reference (not editable as separate curves). Lottie stores spatial keys with a single `o/i` pair (scalar `x`,`y`).

### AE-ANI-036 · Spatial path segments and tangents (空间插值和运动路径)

- **What it does:** Defines the geometric path between two spatial keyframes.
- **Exact semantics:**

  ```
  // segment k0 -> k1 of a spatial property (2-D or 3-D)
  P0 = k0.value
  P1 = k0.value + k0.outSpatialTangent
  P2 = k1.value + k1.inSpatialTangent
  P3 = k1.value
  path(u) = cubic Bezier(P0, P1, P2, P3), u in [0,1]
  ```

  - Spatial Linear on a key means that side's tangent is (0, 0[, 0]); both sides Linear gives a straight segment.
  - Tangents are stored relative to the key value and are in the property's units (px); for 2D properties the z tangent is 0.
  - Spatial interpolation methods: Linear, Bezier (independent in/out handles), Continuous Bezier (in = −k·out, collinear, lengths independent), Auto Bezier (computed, AE-ANI-042). No spatial Hold.
  - The motion path displayed in the comp viewer is this curve; for a parented layer it is drawn in the parent's space transformed to the comp.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Lottie `to` (out tangent of the key) and `ti` (in tangent of the **next** key, stored on the earlier keyframe object), both relative. Scripting `keyInSpatialTangent(i)`, `keyOutSpatialTangent(i)`, `setSpatialTangentsAtKey(i, in, out)`.

### AE-ANI-037 · Arc-length parameterisation of spatial segments (空间路径的弧长参数化)

- **What it does:** Converts a distance along the spatial Bezier segment to the curve parameter so motion speed follows the temporal ease exactly.
- **Exact semantics:**

  ```
  L = ∫_0^1 |path'(u)| du
  pointAtArcLength(s): find u with ∫_0^u |path'(w)| dw = s, return path(u)
  ```

  - AE's exact numerical method is not public. UNVERIFIED reconstruction: a polyline approximation of the segment, as bodymovin (200 samples, `getCurveLength`) and lottie-web (`defaultCurveSegments = 150`, linear interpolation between samples) do; AE's results agree with a converged arc-length to within a few hundredths of a pixel in community comparisons.
  - Degenerate segments (all four points equal): L = 0, value = P0.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Implementation hint: adaptive Gauss–Legendre (e.g. 5-point per sub-interval, subdivide until error < 1e-4 px) for L, then Newton on u with bisection fallback; cache per segment and invalidate on key edit. Expressions `valueAtTime` must return the same numbers.

### AE-ANI-038 · Normalised (Lottie) easing equivalence (标准化缓动（Lottie）对应关系)

- **What it does:** Defines the exact conversion between AE (speed, influence) eases and the normalised cubic-bezier easing used by Lottie (`o`, `i`) for import/export.
- **Exact semantics:**

  ```
  AE -> Lottie, non-spatial dimension d:
    Δ = v1[d] - v0[d] (colour: 255*Δ);  if |Δ| < 1e-7: Δ = 1
    o.x = inf_out/100;           o.y = speed_out * inf_out/100 * dt / Δ
    i.x = 1 - inf_in/100;        i.y = 1 - speed_in * inf_in/100 * dt / Δ
    both sides Linear: o = (inf_out/100, same), i = (1-inf_in/100, same)   // straight line
  AE -> Lottie, spatial:
    L = arc length; avg = L/dt; inf clamped as in AE-ANI-035
    o.x = inf_out/100;  o.y = speed_out/avg * inf_out/100
    i.x = 1 - inf_in/100;  i.y = 1 - speed_in/avg * inf_in/100;  if avg == 0: y = x
  Lottie -> AE: invert the formulas (inf = o.x*100, speed = o.y*Δ/(o.x*dt), etc.);
    o.x outside [0.001, 1] is clamped to the AE influence range 0.1–100 %
  Hold: Lottie h:1 <-> out type Hold.
  ```

  - Lossy cases: Δ == 0 with non-zero speeds (AE bump) cannot be expressed in Lottie; Lottie `o.x > 1` or `< 0` cannot be expressed in AE.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-media)
- **Notes:** Bodymovin rounds `o/i` to 3 decimals. Lottie export UI and format list: ui-preview-render-io.md (Lottie export). lottie-web evaluates `v = v0 + (v1−v0)*ease((t−t0)/dt)`.

### AE-ANI-039 · Speed and influence units and ranges (速度与影响的单位和范围)

- **What it does:** Lists the units and limits of the ease parameters as shown in the Keyframe Velocity dialog, Info panel and Graph Editor.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Speed (速度) | float | segment average (Linear) / 0 (Easy Ease) | unbounded; spatial ≥ 0 | property units per second: px/s (Position, Anchor, POI), %/s (Scale, Opacity), °/s (Rotation, angles), 0–255 units/s (colour), "units/sec" for other values | displayed with 2 decimals (UNVERIFIED) |
  | Influence (影响) | float | 16.666667 % (Linear), 33.33 % (Easy Ease) | 0.1 – 100 % (values below 0.1 are clamped) | % of the segment duration | per side |

  - Speed uses seconds regardless of the comp's time display mode.
  - For a layer with time stretch s, speeds are expressed in layer seconds? UNVERIFIED: AE displays speeds per comp second for stretched layers (keyframes are in layer time but the velocity dialog shows comp-time speed).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-ui-motion)
- **Notes:** Scripting `new KeyframeEase(speed, influence)` with influence in [0.1..100.0]. The Info panel shows "Speed" and "Influence" of the selected keyframe's interpolation (UI file).

## Auto Bezier and Continuous Bezier

### AE-ANI-040 · Temporal Auto Bezier (时间自动贝塞尔曲线)

- **What it does:** A temporal Auto Bezier key gets its in/out ease computed from its neighbours so the value changes smoothly through it, and recomputes whenever neighbouring keys change.
- **Exact semantics:** UNVERIFIED reconstruction (AE's algorithm is not documented; must be confirmed by reading `keyInTemporalEase/keyOutTemporalEase` on AE-generated keys):

  ```
  // key k with neighbours k-1, k+1 (per dimension d for non-spatial; arc length for spatial)
  interior key:
     speed_in = speed_out = (v[k+1] - v[k-1]) / (t[k+1] - t[k-1])        // non-spatial
     speed    = (L(k-1,k) + L(k,k+1)) / (t[k+1] - t[k-1])                // spatial, >= 0
     if v[k] is a local extremum of v[k-1], v[k], v[k+1] (non-spatial): speed = 0   // UNVERIFIED (no overshoot)
     influence_in = influence_out = 16.666667 %
  first / last key (one neighbour): speed = average speed of its only segment, influence 16.666667 %
     (so a property whose keys are all Auto Bezier with only two keys is a straight line)
  ```

  - temporalAutoBezier = true implies temporalContinuous = true. Any manual edit of the key's ease (dragging a handle, Keyframe Velocity dialog, Easy Ease) clears autoBezier and leaves Continuous Bezier (AE help: "If you adjust an Auto Bezier direction handle manually, it becomes Continuous Bezier").
  - Recompute the eases of k when k−1, k, or k+1 change value or time.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Ctrl/Cmd-click on a Linear key in the Timeline makes it Auto Bezier (AE-ANI-049). Scripting `setTemporalAutoBezierAtKey(i, bool)`, `keyTemporalAutoBezier(i)`.

### AE-ANI-041 · Temporal Continuous Bezier and broken (independent) handles (时间连续贝塞尔曲线)

- **What it does:** Continuous Bezier keeps the incoming and outgoing speed equal (smooth curve) while the user sets them; plain Bezier lets in and out differ.
- **Exact semantics:**
  - temporalContinuous = true ⇒ `inTemporalEase.speed == outTemporalEase.speed` per dimension (influences may differ). Editing either side's speed moves the other.
  - temporalContinuous = false (Bezier) ⇒ in and out speeds independent ("split handles"). Alt/Option-dragging a handle in the Graph Editor splits it (sets Bezier); Alt/Option-dragging a key draws new handles.
  - Applying Continuous Bezier to keys whose speeds differ sets both sides to the average? UNVERIFIED: AE uses the outgoing speed for both sides.
  - The Keyframe Velocity dialog's "Continuous" checkbox (AE-ANI-047) toggles this flag.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Scripting `setTemporalContinuousAtKey`, `keyTemporalContinuous`.

### AE-ANI-042 · Spatial Auto Bezier tangents (空间自动贝塞尔曲线)

- **What it does:** Computes Position-type tangents so the motion path passes smoothly through each key; the default spatial interpolation.
- **Exact semantics:** UNVERIFIED reconstruction (Catmull–Rom-like, tangent lengths proportional to the adjacent chord lengths):

  ```
  interior key k:
     dir      = normalize(P[k+1] - P[k-1])          // if P[k+1] == P[k-1]: tangents (0,0)  (UNVERIFIED)
     outTan_k =  dir * |P[k+1] - P[k]| / 3
     inTan_k  = -dir * |P[k]   - P[k-1]| / 3
  first key: outTan_0 = (P[1] + inTan_1 - P[0]) / 3     // aims at the neighbour's incoming handle (UNVERIFIED)
  last key:  inTan_n  = (P[n-1] + outTan_{n-1} - P[n]) / 3
  two keys only: both tangents (P[1]-P[0])/3 and (P[0]-P[1])/3 -> straight segment
  ```

  - spatialAutoBezier = true implies spatialContinuous = true. Dragging a tangent handle in the comp viewer converts the key to Continuous Bezier (spatialAutoBezier = false).
  - Known quirk (documented): two consecutive keys with equal values between other keys get non-zero auto tangents, producing a "boomerang" loop; the fix is Hold on the earlier key or Linear on both.
  - Recompute on any value change of k−1, k, k+1 (including roving re-timing, which does not change tangents).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Exported Lottie `to/ti` values from AE give the ground truth for verifying this reconstruction: build a test project, export with bodymovin, compare. Scripting `setSpatialAutoBezierAtKey`, `keySpatialAutoBezier`.

### AE-ANI-043 · Spatial Continuous Bezier and Bezier (空间连续贝塞尔曲线与贝塞尔曲线)

- **What it does:** User-controlled spatial tangents, either collinear (Continuous) or independent (Bezier, "corner").
- **Exact semantics:**
  - Continuous Bezier: `inTan = -λ * outTan` with λ > 0 (collinear, opposite direction, independent lengths); dragging one handle rotates the other to stay collinear and keeps its length.
  - Bezier: in and out tangents independent. In the comp viewer, the Convert Vertex tool (or Ctrl/Cmd+Alt/Opt-drag a handle with the Pen/Selection tool) breaks the handle (sets Bezier); clicking a key with Convert Vertex toggles Linear ⇄ Auto Bezier (AE-ANI-065).
  - Converting Linear → Bezier/Continuous initialises the tangents with the Auto Bezier computation (AE help: initial handle positions use the same method as Auto Bezier).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-ui-motion)
- **Notes:** Scripting `setSpatialContinuousAtKey`, `setSpatialTangentsAtKey`.

### AE-ANI-044 · Default Spatial Interpolation to Linear preference (默认空间插值为线性)

- **What it does:** Preference that makes new spatial keyframes Linear instead of Auto Bezier.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Default Spatial Interpolation to Linear (默认空间插值为线性) | bool | off | — | — | Edit > Preferences > Composition (Win) / After Effects > Settings > Composition (macOS) (current AE help) |

  - Applies only to the first keyframes of properties without keys; does not change existing keys or new keys on already-animated properties.
- **Priority:** P1
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting preferences section "Main Pref Section" key name UNVERIFIED. Preferences dialog layout: ui-preview-render-io.md (Preferences).

### AE-ANI-045 · First and last keyframe behaviour (第一个和最后一个关键帧)

- **What it does:** Collects the special rules that apply to keys with only one neighbour.
- **Exact semantics:**
  - The in-side of the first key and the out-side of the last key have no segment: their interpolation type and ease are stored but unused (the timeline icon shows that half dark grey).
  - Ease values on the unused side are retained and become effective if a key is later added beyond them.
  - First/last keys can never rove (AE-ANI-051).
  - Auto Bezier first/last: AE-ANI-040, AE-ANI-042.
  - Value outside the range: AE-ANI-030.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** —

## Easy Ease, velocity and interpolation commands

### AE-ANI-046 · Easy Ease, Easy Ease In, Easy Ease Out (缓动、缓入、缓出)

- **What it does:** Keyframe assistants that set the speed at the selected keys to 0 with 33.33 % influence, so motion decelerates into and/or accelerates out of the key.
- **Exact semantics:**

  | Command (中文) | Shortcut Win / Mac | In side of each selected key | Out side of each selected key |
  |---|---|---|---|
  | Easy Ease (缓动) | F9 / F9 | type Bezier, speed 0, influence 33.33 % | type Bezier, speed 0, influence 33.33 % |
  | Easy Ease In (缓入) | Shift+F9 / Shift+F9 | type Bezier, speed 0, influence 33.33 % | unchanged (type and ease kept) |
  | Easy Ease Out (缓出) | Ctrl+Shift+F9 / Cmd+Shift+F9 | unchanged | type Bezier, speed 0, influence 33.33 % |

  - Applied to every dimension of non-spatial properties; to the single ease of spatial properties.
  - Clears temporalAutoBezier; temporalContinuous becomes false (UNVERIFIED: Easy Ease keys show the "hourglass" Bezier icon; since both speeds are 0 the curve is smooth anyway).
  - Does **not** change spatial interpolation or spatial tangents: a Position key keeps its Auto Bezier/Linear path; only timing along the path changes.
  - Easy Ease on a Hold-out key: the out type becomes Bezier (removes the hold) (UNVERIFIED).
  - Influence exact value: 33.33 (AE help: "each keyframe has a speed of 0 with an influence of 33.33% on either side"; UNVERIFIED whether scripting reads 33.33 or 33.333333 = 100/3).
  - Also available as buttons at the bottom of the Graph Editor and under Animation > Keyframe Assistant (动画 > 关键帧助手).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-ui-motion)
- **Notes:** On macOS F9 may be taken by the OS (Mission Control); BAE keymap should still bind it. In Lottie, Easy Ease segments export as `o:{x:0.333,y:0}`, `i:{x:0.667,y:1}` (the familiar "ease" values), which confirms speed 0 / influence 33.33 %.

### AE-ANI-047 · Keyframe Velocity dialog (关键帧速度)

- **What it does:** Numeric entry of incoming and outgoing speed and influence for the selected keyframe(s).
- **Exact semantics:** Animation > Keyframe Velocity… (动画 > 关键帧速度), Ctrl+Shift+K / Cmd+Shift+K, or right-click a keyframe > Keyframe Velocity. (Double-clicking a keyframe opens the value dialog instead, AE-ANI-073.)

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Incoming Velocity: Speed (传入速度：速度) | float (per dimension for non-spatial N-D) | current | unbounded (≥ 0 for spatial) | property units/sec (AE-ANI-039) | |
  | Incoming Velocity: Influence (传入速度：影响) | float | current | 0.1–100 | % | |
  | Outgoing Velocity: Speed (传出速度：速度) | float (per dimension) | current | as above | units/sec | |
  | Outgoing Velocity: Influence (传出速度：影响) | float | current | 0.1–100 | % | |
  | Continuous (Lock Outgoing and Incoming) (连续（锁定输入和输出）) | bool | key's temporalContinuous | — | — | when on, editing one speed copies it to the other |

  - Multi-dimensional non-spatial properties show one Speed/Influence row per dimension (Dimension 1, 2, 3 for Scale; UNVERIFIED labels).
  - With several keys selected, fields show the first key's values and OK applies the entered values to all selected keys (UNVERIFIED: fields left blank/mixed keep each key's value).
  - OK sets the affected sides' interpolation type to Bezier (Linear sides become Bezier) and clears autoBezier.
  - The dialog's speed refers to the key's time in comp seconds? see AE-ANI-039 note.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim, bae-motion)
- **Notes:** The applied change must be a bae-motion Command (`SetTemporalEase`). Units depend on property and plug-in (AE help).

### AE-ANI-048 · Keyframe Interpolation dialog (关键帧插值)

- **What it does:** Sets temporal and spatial interpolation methods and roving for the selected keyframes in one step.
- **Exact semantics:** Animation > Keyframe Interpolation… (动画 > 关键帧插值), Ctrl+Alt+K / Cmd+Opt+K.

  | Menu (中文) | Options in order | Notes |
  |---|---|---|
  | Temporal Interpolation (临时插值) | Current Settings (当前设置), Linear (线性), Bezier (贝塞尔曲线), Continuous Bezier (连续贝塞尔曲线), Auto Bezier (自动贝塞尔曲线), Hold (定格) | applies to both in and out sides |
  | Spatial Interpolation (空间插值) | Current Settings, Linear, Bezier, Continuous Bezier, Auto Bezier | only enabled when spatial keys are selected |
  | Roving (漂浮) | Current Settings (当前设置), Rove Across Time (漂浮穿梭时间), Lock To Time (锁定到时间) | spatial keys only |

  - "Current Settings" leaves that aspect unchanged per key. A newly applied method uses default values: Bezier/Continuous initialise from the Auto Bezier computation; Linear clears tangents (spatial) and sets ease to the segment average (temporal).
  - Changing a key to Hold keeps its spatial tangents (still used by neighbours' segments).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** Chinese terms for "Temporal Interpolation" in the zh-CN UI: 临时插值 (UNVERIFIED; help body text uses 时间插值); Rove Across Time = 漂浮穿梭时间 and Lock To Time = 锁定到时间 are from the zh-CN help page.

### AE-ANI-049 · Interpolation toggles by click and Graph Editor buttons (单击切换插值方法)

- **What it does:** Quick interpolation changes without the dialog.
- **Exact semantics:**

  | Action | Win / Mac | Result |
  |---|---|---|
  | Ctrl-click keyframe (layer bar mode) | Ctrl-click / Cmd-click | Linear → Auto Bezier; Bezier, Continuous Bezier or Auto Bezier → Linear (temporal; for spatial properties both temporal and spatial? UNVERIFIED: temporal only in the Timeline, spatial only in the comp viewer) |
  | Ctrl+Alt-click keyframe (layer bar mode) | Ctrl+Alt-click / Cmd+Opt-click | Linear ⇄ Hold |
  | Convert Vertex tool click on a Graph Editor key | G then click | Linear ⇄ Auto Bezier (temporal) |
  | Convert Vertex click on a motion-path key | — | Linear ⇄ Auto Bezier (spatial) |
  | Graph Editor buttons "Convert selected keyframes to Hold / Linear / Auto Bezier" (将选定的关键帧转换为定格/线性/自动贝塞尔曲线（译名待核）) | — | set temporal type for all selected keys |

  - All are undoable Commands applied to the whole selection when the clicked key is selected (UNVERIFIED: Ctrl-click affects only the clicked key).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Keyboard reference: ui-preview-render-io.md (shortcuts).

### AE-ANI-050 · Toggle Hold Keyframe (切换定格关键帧)

- **What it does:** Turns Hold on or off for the selected keyframes.
- **Exact semantics:** Animation > Toggle Hold Keyframe (动画 > 切换定格关键帧), Ctrl+Alt+H / Cmd+Opt+H.
  - If any selected key's out type is not Hold: set the **out** type to Hold on all selected keys (AE help: applies/removes Hold "as outgoing interpolation"; UNVERIFIED whether the in type also changes). Otherwise (all Hold): set them back to the interpolation they had before Hold was applied if remembered, else Auto Bezier? UNVERIFIED: AE's shortcut table describes the toggle as "hold or Auto Bezier"; observed behaviour restores the previous type (Linear stays Linear).
  - Spatial interpolation untouched.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** BAE decision: store the pre-Hold temporal types inside the Hold toggle command's undo data and in a hidden per-key field so toggling back is exact.

## Roving keyframes

### AE-ANI-051 · Roving keyframes (漂浮关键帧)

- **What it does:** Interior spatial keyframes that are not locked to a time; AE re-times them so speed is smooth across the span between the surrounding non-roving keys.
- **Exact semantics:**
  - Only for spatial properties (Position, Anchor Point, POI, effect points); never for the first or last key of the property; not available with separated dimensions.
  - Span: a maximal run of roving keys R1..Rm between non-roving keys A (time tA) and B (time tB).
  - UNVERIFIED reconstruction of the timing rule (AE's algorithm is not documented; help says roving keys get "consistent speed over the range"):

    ```
    Lseg[i]  = arc length of each spatial segment A..R1, R1..R2, ..., Rm..B
    Ltotal   = Σ Lseg
    // one ease over the whole span, from A's out ease to B's in ease (AE-ANI-035 with L = Ltotal, dt = tB - tA)
    s(t)     = spanEase(t)
    time(Rj) = s^-1( Σ_{i<=j} Lseg[i] )          // the time at which the eased distance reaches Rj
    // roving keys' own temporal ease is ignored/derived (continuous speed through them)
    ```

    With A-out and B-in Linear this gives constant speed `Ltotal / (tB − tA)` along the whole span.
  - Roving key times are recomputed whenever any key in the span changes value, tangents or the times of A/B; they are stored (so the Timeline shows them) but marked roving.
  - Dragging a roving key in time, or choosing Lock To Time, makes it non-roving at its current time. Pasting keys with "Rove Across Time" happens when pasting a spatial path into a property (AE-ANI-076).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model, bae-motion)
- **Notes:** Scripting `setRovingAtKey(i, bool)`, `keyRoving(i)`. Roving keys appear as small dots in the Timeline (ui-preview-render-io.md). Lottie has no roving flag: bodymovin bakes the computed times. Verify the reconstruction by exporting a roving animation with bodymovin and reading the baked key times.

## Separate dimensions

### AE-ANI-052 · Separate Dimensions of Position (单独尺寸)

- **What it does:** Splits Position into independent X Position, Y Position (and Z Position for 3D layers) 1-D properties, each with its own keyframes and expressions.
- **Exact semantics:**
  - Commands: Animation > Separate Dimensions (动画 > 单独尺寸), right-click Position > Separate Dimensions, or the Graph Editor button. It is a toggle; doing it again recombines.
  - Only Position (layer transform, incl. cameras/lights) supports it. While separated: no spatial interpolation, no motion-path tangents, no roving; the motion path is still drawn (from the evaluated X/Y values) but its keys cannot be edited as a spatial path (UNVERIFIED: AE draws but does not allow dragging tangents).
  - Separating: AE help states "some information about speed is lost, but the motion path does not change", and the expression reference notes that `numKeys` changes after Separate Dimensions (so AE may add keys while separating). UNVERIFIED reconstruction (simplest form, one key per original key; AE may additionally insert keys to keep the path):

    ```
    for each original key k: create key in X, Y(, Z) at time t_k with value component
    temporal types copied (Linear/Bezier/Hold)
    velocity vector at k (out side) = speed_out * unitTangent_out(k)    // unit tangent of the spatial path at k
    speed_out_d = velocity_d ; influence_out_d = influence_out          // same for the in side
    ```

    This keeps each component's slope at the keys but not the spatial path between keys exactly (a 1-D cubic per component replaces the arc-length mapping), which is why AE presumably inserts extra keys; measure the key count AE produces for a curved Auto Bezier path before implementing.
  - Recombining: keys are created at the union of the components' key times; values = evaluated components at those times; spatial tangents derived from the per-component curves (each component's Bezier handle becomes one component of the tangent; UNVERIFIED); temporal ease recomputed. AE help: "some information about the motion path and speed is lost because the multiple Bezier curves used to represent the individual components are collapsed into a single Bezier curve at each keyframe".
  - UNVERIFIED: an expression on the combined Position is kept on the leader but not evaluated while separated (the per-dimension X/Y/Z Position expressions are evaluated instead).
  - Expressions read `transform.position` as the combined vector even when separated; `transform.xPosition` etc. exist only while separated.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Scripting `dimensionsSeparated` (read/write on the leader), `isSeparationLeader`, `isSeparationFollower`, `getSeparationFollower(dim)`, matchNames `ADBE Position_0/_1/_2`. Lottie `ks.p.s = true` with `p.x`, `p.y`, `p.z`. AE help recommends not toggling back and forth.

## Interpolation of other value types

### AE-ANI-053 · Colour property interpolation (颜色属性插值)

- **What it does:** Defines how keyframed colour values (effect Color controls, shape Fill/Stroke Color, light Color, text fill colour animators) blend between keys.
- **Exact semantics:**
  - Value = RGBA, 4 floats (scripting range 0–1; 32-bpc projects allow > 1 for HDR? UNVERIFIED: colour pickers clamp to 0–1 except in 32-bpc where values above 1 can be typed in some effects).
  - Interpolation is component-wise on the stored numbers (R, G, B, A lerped with the same progress), driven by **one** temporal ease shared by all components (`COLOR` has a single KeyframeEase, AE-ANI-034). No HSL/hue-path interpolation.
  - The stored numbers are in the project's working colour space as encoded values; interpolation is **not** performed in linear light even when "Linearize Working Space" is on (UNVERIFIED; implementers must confirm by keyframing red→green in a linearized project and sampling the midpoint, expected (0.5, 0.5, 0) stored → rendered after linearization).
  - Shape gradient colours (array of stops, see shapes.md, Gradients) interpolate stop-by-stop and component-wise; stop counts must match (shapes.md).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-color)
- **Notes:** Lottie colour arrays are 0–1 (`[r,g,b,a]`); bodymovin multiplies the colour delta by 255 when normalising eases (AE-ANI-034). Colour management: compositing-3d-color.md, Color management.

### AE-ANI-054 · Mask path and shape path interpolation (蒙版路径和形状路径插值)

- **What it does:** Interpolates Bezier paths between keyframes vertex by vertex.
- **Exact semantics:**

  ```
  // path = {closed, vertices[n], inTangents[n], outTangents[n]} (tangents relative to vertex)
  p = ease(t)   // ONE temporal ease per path property (SHAPE type: single KeyframeEase), AE-ANI-033 on a 0..1 progress
  for j in 0..n-1:
      v[j]   = lerp(A.v[j],   B.v[j],   p)
      in[j]  = lerp(A.in[j],  B.in[j],  p)
      out[j] = lerp(A.out[j], B.out[j], p)
  closed = A.closed   // UNVERIFIED: closed state taken from the earlier key; AE keeps it constant across keys in practice
  ```

  - Vertex correspondence by index from the **first vertex** (Layer > Mask and Shape Path > Set First Vertex, 设置第一个顶点); changing the first vertex renumbers and changes the animation. The first vertex at the initial key is the topmost vertex for paths drawn by AE (AE help).
  - With "Preserve Constant Vertex Count and Feather Point Count when Editing Masks" (Preferences > Composition, default on) adding/deleting a vertex adds/deletes it at every keyframe (added vertices do not change the shape at other keys).
  - Vertex count mismatch between keys (pasted paths, scripting): UNVERIFIED: AE inserts extra vertices on the path with fewer vertices (subdividing its last segments without changing its shape) so counts match; lottie-web assumes equal counts.
  - Temporal ease speed for paths is in "units/sec" of normalised progress (UNVERIFIED unit; Easy Ease sets 0).
  - Paths are not spatial: no motion path, no roving. Smart Mask Interpolation (智能蒙版插值) generates intermediate keys (P2, compositing-3d-color.md / shapes.md).
  - Mask Feather points (variable-width feather) interpolate per feather point with the same index rule.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-vector)
- **Notes:** Lottie path keyframes `{s:[{c, v, i, o}]}`; lottie-web lerps `v`, `i`, `o` with one eased percent. Copying a closed path into Position: AE-ANI-076. Shape layer path editing: shapes.md.

### AE-ANI-055 · Source Text and Text Document keyframes (源文本关键帧)

- **What it does:** Text Document values (Source Text) never interpolate; each key holds until the next.
- **Exact semantics:** All Source Text keyframes are Hold (in and out); the Keyframe Interpolation dialog cannot change them; Easy Ease has no effect. Value at t = value of the last key with time ≤ t (first key's value before it).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-text)
- **Notes:** Text Document fields: text.md (TextDocument, Source Text keys).

### AE-ANI-056 · Discrete-valued properties (Hold-only) (离散值属性（仅定格）)

- **What it does:** Properties whose values cannot be blended always behave as Hold.
- **Exact semantics:**

  | Value kind | Examples | Interpolation |
  |---|---|---|
  | Checkbox | effect checkboxes | Hold only |
  | Popup menu / dropdown | effect menus, Dropdown Menu Control, blend-mode-like effect menus | Hold only |
  | Layer picker (`LAYER_INDEX`) | Layer Control, effect layer inputs | Hold only |
  | Mask picker (`MASK_INDEX`) | Stroke effect path menu | Hold only |
  | Marker | layer/comp markers | not interpolated (markers are not keyframes) |

  - Keys of these properties are created with Hold in/out; attempts to set Linear/Bezier are ignored (scripting throws? UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Effects' parameter types: effects.md (effect framework).

### AE-ANI-057 · Orientation versus Rotation interpolation (方向与旋转的插值)

- **What it does:** Orientation interpolates along the shortest 3D arc, X/Y/Z Rotation numerically.
- **Exact semantics:**

  ```
  // Orientation segment k0 -> k1
  q0 = quat(Rx(o0.x) * Ry(o0.y) * Rz(o0.z));  q1 = quat(Rx(o1.x) * Ry(o1.y) * Rz(o1.z))
  if dot(q0, q1) < 0: q1 = -q1
  p  = ease(t)            // single temporal ease of the Orientation property (UNVERIFIED: AE applies the ease; lottie-web uses linear p)
  q  = slerp(q0, q1, p)   // nlerp when 1 - dot < 1e-6
  orientation(t) = euler_xyz(q) wrapped to [0,360)   // as reported to expressions; the renderer can use q directly
  ```

  - A turn larger than 180° between two Orientation keys goes the short way; use X/Y/Z Rotation for multi-turn spins.
  - X/Y/Z Rotation and 2D Rotation interpolate as plain numbers (AE-ANI-033): 0 → 720 makes two turns.
  - Effect Angle controls behave like Rotation (numeric, multi-revolution, `nx+deg` display).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** lottie-web `PropertyFactory.js` uses quaternion slerp with heading/attitude/bank conversion for `or` (`this.sh`) and ignores the ease; BAE must use AE's XYZ order from AE-ANI-014 for the conversion.

### AE-ANI-058 · Custom-value and integer-valued parameters (自定义值与整数参数的插值)

- **What it does:** Interpolation for effect parameters with plug-in-defined data or integer semantics.
- **Exact semantics:**
  - `CUSTOM_VALUE` (e.g. Curves curve, Levels histogram data, Puppet mesh) and arbitrary-data parameters: interpolation is defined by the effect (AE calls the plug-in's interpolate callback with a 0–1 progress after easing); if the effect provides none, Hold. Built-in specifics: effects.md.
  - Integer-semantics sliders (e.g. Mosaic Horizontal Blocks) interpolate as floats; the effect rounds/truncates when rendering (per effect, effects.md).
  - `NO_VALUE` properties (e.g. some effect group placeholders) have no keyframes.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-effects, bae-plugin)
- **Notes:** AE SDK arbitrary-data params use `PF_Arbitrary_INTERP_FUNC` (paraphrased; see ae-plugins docs); BAE plugin API: bae-plugin.

## Graph Editor and motion path semantics

### AE-ANI-059 · Graph Editor graph types (图表编辑器的图表类型)

- **What it does:** The Graph Editor shows each property's animation either as a value graph or a speed graph; the type decides which handles exist and what dragging them means.
- **Exact semantics:** Graph Type and Options menu (Show Graph Editor Set? see UI file), in order:

  | Option (中文) | Effect |
  |---|---|
  | Auto-Select Graph Type (自动选择图表类型) | speed graph for spatial properties (Position, Anchor Point, POI, effect points), value graph for all others (default) |
  | Edit Value Graph (编辑值图表) | value graph for all properties |
  | Edit Speed Graph (编辑速度图表) | speed graph for all properties |
  | Show Reference Graph (显示参考图表) | draws the other graph type in the background, read-only; its values appear as grey numbers on the right |

  - Value graph y-axis: the property value (one curve per dimension, red/green/blue for x/y/z; colour components in channel colours). Speed graph y-axis: units/second.
  - Speed definition: non-spatial dimension d: `speed_d(t) = dv_d/dt` (signed, one curve per dimension for `TwoD`/`ThreeD` such as Scale; UNVERIFIED: AE plots per-dimension speed curves; colour, having one ease, plausibly shows a single speed curve); spatial: `speed(t) = |dP/dt| = ds/dt` (single non-negative curve, px/s along the path).
  - Expressions: the graph shows the post-expression result as a read-only curve when "Show Post-Expression Graph" (显示后表达式图表) is enabled for the property (expression part of this file / ui file).
  - The Separate Dimensions button at the bottom applies AE-ANI-052.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Layout, buttons, zoom/pan (Auto Zoom Height, Fit Selection, Fit All), Show Audio Waveforms, Show Layer In/Out Points, Show Layer Markers, Show Graph Tool Tips, Show Expression Editor: ui-preview-render-io.md (Graph Editor). bae-anim must expose `value(t)`, `speed(t)` and per-key handle geometry for both graph types.

### AE-ANI-060 · Value graph editing semantics (值图表编辑)

- **What it does:** Defines what dragging keys, handles and segments in the value graph changes.
- **Exact semantics:**
  - Key drag: horizontal = time (snaps to frames unless Allow Keyframes Between Frames is on, AE-ANI-062), vertical = value. Shift constrains to one axis (UNVERIFIED).
  - Temporal handle of a Bezier side is drawn at the control point of AE-ANI-033: out handle at `(t0 + io*dt, v0 + so*io*dt)`, in handle at `(t1 − ii*dt, v1 − si*ii*dt)`. Dragging it sets `influence = |Δx|/dt*100` (clamped 0.1–100) and `speed = Δy/Δx`; on a Continuous key the opposite handle rotates to keep the same speed (its influence unchanged); Alt/Option-drag splits.
  - Dragging a handle of a Linear side converts that side to Bezier (UNVERIFIED: the key becomes Bezier/Continuous).
  - Dragging a segment between two keys moves both adjacent handles together (AE help).
  - Spatial properties in the value graph: per-component curves are shown; keys can be moved; temporal handles are not editable there (the ease belongs to the arc-length curve, edit it in the speed graph) — UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim, bae-motion)
- **Notes:** All edits go through bae-motion/bae-model Commands (`SetTemporalEase`, `MoveKeys`).

### AE-ANI-061 · Speed graph editing semantics (速度图表编辑)

- **What it does:** Defines the speed-graph representation of eases and how editing it maps to speed/influence.
- **Exact semantics:**
  - Each key shows its in-speed (left) and out-speed (right) as points at the key time; a vertical jump appears where in ≠ out (split handles).
  - Handles extend horizontally from the key: length = influence × segment duration; vertical position = speed. Dragging a handle up/down changes speed; toward/away from the key changes influence (AE help: drag away to increase influence).
  - Between keys the curve is `dBy/dBx` of the cubic of AE-ANI-033 (non-spatial) or `ds/dt` of AE-ANI-035 (spatial).
  - Linear segments show a horizontal line at the average speed; Hold segments show speed 0? UNVERIFIED (AE draws no curve for hold segments).
  - The area under the speed curve between two keys equals the value change (non-spatial) or arc length (spatial); editing speeds keeps key values fixed, so increasing one side's speed reshapes the rest of the curve to preserve the area.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Implementation hint: compute speed analytically: `dv/dt = (dBy/du)/(dBx/du)`.

### AE-ANI-062 · Allow Keyframes Between Frames and graph snapping (允许帧之间的关键帧)

- **What it does:** Graph Editor option that permits dragging keys to sub-frame times; snapping aligns dragged keys to other time/value features.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Allow Keyframes Between Frames (允许帧之间的关键帧) | bool (per user, Graph Editor options menu) | off | when on, key time drags in the Graph Editor are not quantised to frames |
  | Snap (对齐) | bool (Graph Editor button) | off? UNVERIFIED | snaps to keyframe values, keyframe times, CTI, In/Out points, markers, work-area start/end, comp start/end; an orange line shows the snap target |

  - Off: dragged times round to the nearest comp frame (`round((t - layerStart) * fps)`, see AE-ANI-024). Keys already between frames keep their offset when dragged in whole frames (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Timeline (layer bar mode) snapping with Shift-drag: AE-ANI-070.

### AE-ANI-063 · Graph Editor transform box (图表编辑器变换框)

- **What it does:** With "Show Transform Box When Multiple Keys Are Selected" on, a bounding box around selected keys lets the user move and scale them in time and value together.
- **Exact semantics:**
  - Drag inside the box: move keys in time and value (Shift constrains to horizontal or vertical).
  - Drag a box handle: scale key times and/or values about the opposite edge; Shift keeps the width/height ratio; Ctrl/Cmd-drag scales about the box's anchor point (movable with the Move Anchor Point cursor); Alt/Option-drag a corner moves only that corner.
  - Scaling by a negative amount (dragging past the opposite edge) reverses the keys in time (or flips values vertically).
  - Ctrl+Alt-drag / Cmd+Opt-drag a corner tapers values vertically (scales amplitude progressively across the box); Ctrl+Alt+Shift-drag / Cmd+Opt+Shift-drag moves one side up or down (skew in value).
  - Scaled times are rounded to frames unless Allow Keyframes Between Frames is on (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-ANI-064 · Motion path display in the viewers (运动路径)

- **What it does:** Spatial properties with keyframes are drawn as a path in the Composition panel (Position, POI) or Layer panel (Anchor Point, effect points as layer-space paths).
- **Exact semantics:**
  - Drawn elements: the path curve (AE-ANI-036) for the visible time range; one dot per comp frame at the property's evaluated value (dot spacing shows speed; dots are sampled at frame times through the layer's time mapping); a square box per keyframe (filled when selected); Bezier tangent handles of selected keys.
  - Space: the property's parent space mapped to the viewer (layer's parent world matrix for Position; the layer's own world matrix for Anchor Point in the Layer panel); in 3D it is projected through the active view.
  - Extent preference Preferences > Appearance > Motion Path (运动路径) (AE help): No Motion Path / All Keyframes / No More Than N Keyframes / No More Than <duration> (UNVERIFIED option strings and default, believed No More Than 0:00:15:00). Handle size: Preferences > Appearance > Path Point and Handle Size. Hold segments draw no dots.
  - Visibility: View > View Options > Keyframes, Motion Paths, Motion Handles; a Position path is shown when the Position property (or layer) is selected; Anchor Point and effect-point paths appear in the Layer panel (choose the property in the panel's View menu).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** View > Show Layer Controls toggles it (ui-preview-render-io.md, Viewer). Implementation hint: bae-anim provides a batched `sample_path(prop, times[])` so the UI does not evaluate per dot.

### AE-ANI-065 · Editing the motion path (编辑运动路径)

- **What it does:** Direct manipulation of spatial keys and tangents in the viewer.
- **Exact semantics:**
  - Drag a key box: changes that key's value (all selected keys move by the same delta); Position keys at the CTI also move the layer.
  - Drag a tangent handle: sets the spatial tangent; converts Auto Bezier → Continuous Bezier; Linear sides grow a handle when dragged out with Convert Vertex.
  - Convert Vertex tool (转换"顶点"工具) click on a key: spatial Linear ⇄ Auto Bezier; drag a handle: break into independent Bezier.
  - Pen tool (钢笔工具) or Add Vertex tool (添加"顶点"工具) click on the motion path adds a keyframe at the frame under the click (AE help); the new key's value is the path point there.
  - Moving the layer in the viewer with Position animated creates/updates a key at the CTI (auto-keyframing, AE-ANI-025).
  - Free transform of selected motion-path keys (Ctrl/Cmd+T? UNVERIFIED) scales/rotates the selected key values about a box anchor.
- **Priority:** P0 (drag keys/handles), P1 (Convert Vertex, Pen)
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** Tool definitions: ui-preview-render-io.md (Tools).

## Keyframe editing commands

### AE-ANI-066 · Add and remove keyframes at the current time (在当前时间添加或移除关键帧)

- **What it does:** Creates a keyframe with the current value at the CTI, or removes the one there.
- **Exact semantics:**
  - Ways: keyframe navigator diamond in the Timeline (toggle), Alt+Shift+property shortcut (Win) / Option+property shortcut (Mac) (P = Position, A = Anchor Point, S = Scale, R = Rotation, T = Opacity, M = Mask Path, F = Mask Feather), Animation > Add <Property> Keyframe (动画 > 添加…关键帧), stopwatch on (AE-ANI-025), or editing the value.
  - The new key's value = the property's current **pre-expression** value at the CTI (the keyframe interpolation result), interpolation per AE-ANI-027. Adding a key between existing keys does not change the curve's values at that time but splits the segment: the new key's ease is chosen so the curve stays as close as possible? UNVERIFIED: AE does not preserve the old curve shape exactly; new key gets default (Linear or inherited) interpolation and neighbouring eases are kept.
  - If the property is not yet animated, adding a key turns the stopwatch on.
  - Removing the last remaining key turns the stopwatch off and the static value becomes that key's value (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Shortcut table: ui-preview-render-io.md.

### AE-ANI-067 · Delete keyframes (删除关键帧)

- **What it does:** Removes the selected keyframes.
- **Exact semantics:** Delete/Backspace or Edit > Clear removes selected keys. If all keys of a property are deleted, the stopwatch turns off and the static value = the property value at the CTI before deletion (UNVERIFIED: AE uses the value at the CTI, same as turning the stopwatch off). Neighbouring Auto Bezier keys recompute (AE-ANI-040, AE-ANI-042); roving spans recompute.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** —

### AE-ANI-068 · Keyframe navigation (关键帧导航)

- **What it does:** Moves the CTI to keyframes.
- **Exact semantics:**

  | Command | Win / Mac | Behaviour |
  |---|---|---|
  | Go to next visible item in time ruler (keyframe, marker, in/out, work area) | K / K | next time > CTI among keys of *visible* (revealed) properties of all layers, layer in/out points, markers, work area |
  | Go to previous visible item | J / J | previous |
  | Keyframe navigator arrows ◀ ▶ | click | previous/next key of that property only |
  | Shift-click a navigator arrow | — | also adds the key to the selection (UNVERIFIED) |

  - Keys between frames: CTI moves to the frame containing the key? UNVERIFIED: the CTI snaps to the nearest frame at or after the key time.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** UI shortcuts belong to ui-preview-render-io.md; listed because "visible items" defines which keys count.

### AE-ANI-069 · Nudge keyframes in time (微移关键帧时间)

- **What it does:** Moves the selected keyframes by whole frames with the keyboard.
- **Exact semantics:** Alt+→ / Alt+← (Option on Mac): +1 / −1 frame; with Shift: ±10 frames. Frame = comp frame duration `1/fps`, mapped into layer time through stretch (a 200 % stretched layer moves its layer-time keys by `0.5/fps`). Collisions with unselected keys: the moved key replaces the other (UNVERIFIED). Roving keys become locked. Moving keys before time 0 / beyond comp end is allowed.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Nudging values of selected keys with the keyboard (e.g. arrow keys nudge Position at a key) follows the property's nudge rules in ui-preview-render-io.md.

### AE-ANI-070 · Drag keyframes in time and snapping (拖动关键帧与对齐)

- **What it does:** Moves selected keyframes by dragging in the Timeline, with optional snapping.
- **Exact semantics:**
  - All selected keys (across properties and layers) move by the same time delta; delta is quantised to whole comp frames (AE-ANI-024).
  - Hold Shift **after** starting the drag: snaps to the CTI, other keyframes, markers, in/out points, work-area and comp start/end (snapping targets per ui-preview-render-io.md, snapping). The Info panel shows the key time during the drag.
  - Dragging a roving key makes it non-roving.
  - Keys cannot be dragged past each other within one property? UNVERIFIED: they can; order is re-sorted and an exact collision replaces.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-ANI-071 · Scale keyframe timing by Alt-drag (按比例缩放关键帧时间)

- **What it does:** Stretches or compresses the timing of a keyframe selection proportionally.
- **Exact semantics:**

  ```
  // select >= 2 keys (in one or more properties/layers); Alt/Option-drag the first or last selected key
  anchor = the opposite extreme selected key time (fixed)
  f      = (newExtremeTime - anchor) / (oldExtremeTime - anchor)
  for each selected key: t' = anchor + (t - anchor) * f
  // t' is NOT rounded to frames: keys can land between frames (UNVERIFIED for the Timeline; AE help on keys between frames)
  ```

  - Unselected keys are not moved; scaled keys that collide with unselected keys replace them (UNVERIFIED).
  - Values and eases (speed/influence) are unchanged, so speeds in units/s now produce a different curve shape (AE keeps speed values; UNVERIFIED: AE does not rescale speeds when stretching).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Layer time-stretch keeps key layer-times and thus also produces between-frame keys (AE-ANI-087).

### AE-ANI-072 · Change values of multiple keyframes at once (同时更改多个关键帧的值)

- **What it does:** Edits the values of several selected keys of the same property together.
- **Exact semantics:**
  - Numeric entry (typing a value) → absolute: every selected key gets exactly the typed value.
  - Scrubbing the underlined value → relative: every selected key changes by the same delta.
  - Dragging in the Composition/Layer panel → relative (delta of the dragged key applied to all selected keys).
  - All selected keys must belong to the same property (AE help); keys of other properties are untouched.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** —

### AE-ANI-073 · Keyframe value dialog (关键帧值对话框（译名待核）)

- **What it does:** Double-clicking a keyframe (layer bar mode) opens a dialog to type its value numerically.
- **Exact semantics:** Same unit choices as Edit Value (AE-ANI-012); for effect properties shows that parameter's control; OK writes the value to that key (and to all selected keys of the same property, absolute, AE-ANI-072). The Timeline tooltip on hover shows key time and value; the Info panel shows key time/value/speed for the selected key.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** UI layout in ui-preview-render-io.md.

### AE-ANI-074 · Copy and paste keyframes (复制和粘贴关键帧)

- **What it does:** Copies selected keyframes (with interpolation, eases, tangents and labels) and pastes them at the CTI into the same or a compatible property, possibly on other layers.
- **Exact semantics:**

  ```
  copy:  record for each property: list of (t - t_earliest_over_whole_selection, key data)
  paste: target time origin = CTI (comp time), converted to each target layer's time
         target property:
           if a property is selected in the target layer: that property
             (only allowed when exactly ONE source property was copied)
           else: the property with the same matchName path in each selected layer
                 (multi-property copy maps by identity; layers in the same order? UNVERIFIED)
         for each key: t_target = CTI + offset; existing key at t_target is replaced; other keys kept
         pasted keys become selected; the stopwatch turns on if needed
  ```

  - Compatibility: same property, or different properties with the same value type (Position ⇄ Anchor Point ⇄ POI ⇄ effect point; Scale ⇄ other 2-D values? UNVERIFIED; colour ⇄ colour; 1-D ⇄ 1-D such as Opacity → Rotation, values copied numerically).
  - Dimension mismatch: 2-D spatial keys pasted into a 3-D spatial property get z = 0 (UNVERIFIED); 3-D into 2-D drops z (UNVERIFIED). Pasting into a separated dimension (X Position) from 1-D sources is allowed.
  - Mask path / shape path ⇄ mask path / shape path keys are compatible (paths copied as values). Pasting a whole path (not keys) into Position: AE-ANI-076.
  - Timing is relative: the earliest copied key lands at the CTI across all pasted properties (keeps relative offsets between properties and layers).
  - Copy between comps of different frame rates: offsets are preserved in seconds (keys may land between frames).
  - Pasted keys keep temporal/spatial interpolation, eases, roving flags (UNVERIFIED for roving), labels.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Clipboard format: BAE-internal (bae-model serialization) plus a text representation (AE-ANI-077). AE 2024+ supports copying keys from multiple layers and pasting to multiple layers without creating new layers (help, "spanning multiple layers"); version UNVERIFIED.

### AE-ANI-075 · Reverse paste keyframes (反向粘贴关键帧（译名待核）)

- **What it does:** Pastes copied keyframes with their timing mirrored, so the animation plays backwards from the CTI.
- **Exact semantics:** Ctrl+Shift+V / Cmd+Shift+V. Same targeting as AE-ANI-074, but `t_target = CTI + (t_last − t)` for each copied key (latest key lands at the CTI), and each key's in/out interpolation types, eases and spatial tangents are swapped as in Time-Reverse Keyframes (AE-ANI-079).
- **Priority:** P2
- **Owner:** bae-motion
- **Notes:** Added in a recent AE release (UNVERIFIED: AE 25.x, 2025); listed in the current shortcut reference as "Reverse paste the copied keyframes in the timeline".

### AE-ANI-076 · Paste a path into a spatial property (将路径粘贴为运动路径)

- **What it does:** Pasting a mask, shape or Illustrator path into Position (or another spatial property) creates keyframes that trace the path.
- **Exact semantics:**
  - One keyframe per path vertex, starting at the path's first vertex; closed paths are opened at the first vertex and get a final key equal to the first vertex (motion paths are always open).
  - Values: vertex positions mapped from the source path's space into the target property's space (UNVERIFIED: AE pastes raw path coordinates, so the layer-space path of a mask becomes comp/parent-space positions without transformation).
  - Spatial tangents = the path's vertex tangents.
  - Timing: first key at the CTI, last key at CTI + 2 s (AE help: default duration 2 seconds; adjust by dragging the first/last key), intermediate keys set to Rove Across Time so speed is constant along the path (AE help).
  - Sources: mask path, shape-layer path, paint-stroke path, or a path copied from Illustrator/Photoshop; targets: Position, Anchor Point, effect point controls.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** —

### AE-ANI-077 · Copy keyframes as tab-delimited text (以制表符分隔文本复制关键帧数据)

- **What it does:** Copied keyframe data can be pasted into a spreadsheet and modified data pasted back.
- **Exact semantics:** One layer at a time. Text layout: header lines with comp/units information, then rows with frame number (column B) and value columns C, D, E (by dimension; Opacity only C); pasting such text at the CTI creates keys at `CTI + frame offset` (UNVERIFIED exact header format: "Adobe After Effects 8.0 Keyframe Data", Units Per Second, Source Width/Height, Source Pixel Aspect Ratio, Comp Pixel Aspect Ratio, then a "Transform\tPosition" block with "Frame\tX pixels\tY pixels\tZ pixels"). The same format is produced by mocha AE and the motion tracker.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Interpolation of pasted text keys: Linear (UNVERIFIED).

## Keyframe assistants

### AE-ANI-078 · Exponential Scale (指数比例)

- **What it does:** Replaces linear Scale animation between two selected keys with an exponential curve that looks like an optical zoom.
- **Exact semantics:**

  ```
  // select exactly the start key k0 and end key k1 of Scale (Shift-click), then
  // Animation > Keyframe Assistant > Exponential Scale
  delete all Scale keys strictly between t0 and t1
  for each comp frame time t in (t0, t1):  // one key per frame
      for each dimension d:
          s_d(t) = s0_d * (s1_d / s0_d) ^ ((t - t0) / (t1 - t0))
      add key(t, s(t)), temporal Linear
  ```

  - Requires s0_d and s1_d non-zero with the same sign (a negative/zero ratio has no real exponential; UNVERIFIED: AE shows an error or does nothing).
  - Applies to 2D layers' Scale (AE help: "when working with 2D layers"); for 3D layers z is also processed (UNVERIFIED).
  - Interpolation of k0's out side and k1's in side becomes Linear (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Expression equivalent sometimes preferred: `s0*Math.pow(s1/s0, p)`. BAE decision (suggestion): offer an "exponential" interpolation flag on Scale segments instead of baking per-frame keys, but keep the AE command for fidelity.

### AE-ANI-079 · Time-Reverse Keyframes (时间反向关键帧)

- **What it does:** Reverses the order of the selected keyframes in time within their original span.
- **Exact semantics:**

  ```
  // per property, over the selected keys (>= 2)
  ta = min selected time of this property, tb = max selected time of this property
  for each selected key: t' = ta + tb - t
  swap in/out: interpolation types, temporal eases (speed sign kept for 1-D? see below), spatial tangents
  ```

  - Values stay attached to their keys (the animation plays backwards). For non-spatial 1-D values, the incoming speed of the new key = −(old outgoing speed)? UNVERIFIED: since the curve is mirrored in time, slopes change sign: `newIn.speed = −oldOut.speed`, `newOut.speed = −oldIn.speed`; influences swap. Spatial speeds are magnitudes and only swap.
  - Roving and Auto Bezier flags preserved and recomputed.
  - Keys of each property are reversed only within that property's own selected time range, not the range of other selected properties (AE help); markers are not reversed.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Animation > Keyframe Assistant > Time-Reverse Keyframes. Layer time reversal is a different command (AE-ANI-089).

### AE-ANI-080 · Convert Expression to Keyframes (将表达式转换为关键帧)

- **What it does:** Bakes an expression's result into keyframes and disables the expression.
- **Exact semantics:**
  - Animation > Keyframe Assistant > Convert Expression to Keyframes (动画 > 关键帧助手 > 将表达式转换为关键帧) on selected properties with expressions.
  - Samples the post-expression value at every comp frame across the layer's duration (UNVERIFIED: from layer In point to Out point; AE may use the comp duration for layers that extend beyond it), creating one key per frame with Linear temporal (and Linear spatial) interpolation; existing keys are replaced.
  - The expression is kept but disabled (expression enable switch off), so it can be re-enabled.
  - Consecutive identical values still produce keys (no reduction) (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Expression evaluation: expression part of this file. Also used internally by BAE's Lottie export to bake expressions.

### AE-ANI-081 · Convert Audio to Keyframes (将音频转换为关键帧)

- **What it does:** Analyses the comp's audio amplitude in the work area and creates an "Audio Amplitude" layer with slider keyframes.
- **Exact semantics:**
  - Creates a null layer named "Audio Amplitude" (音频振幅) with three Slider Control effects: Left Channel (左声道), Right Channel (右声道), Both Channels (两个通道), each keyed once per comp frame over the work area.
  - Value = amplitude of the mixed audio of all audible layers in that frame. UNVERIFIED scale: mean absolute amplitude scaled so full scale ≈ 100? Community measurements put typical speech at 5–30; formula must be measured.
  - Keys use Linear interpolation (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-media)
- **Notes:** Audio decode/mix: bae-media; Slider Control: effects.md (Expression Controls).

### AE-ANI-082 · Motion Sketch (运动草图)

- **What it does:** Records the mouse path drawn in the Composition panel as Position keyframes in real time.
- **Exact semantics:** Window > Motion Sketch panel.

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Capture Speed At (捕捉速度为) | float | 100 | > 0 | % | > 100 % plays back slower than recorded |
  | Smoothing (平滑) | float | 1 (UNVERIFIED) | 0–100 (UNVERIFIED) | — | same effect as the Smoother's Tolerance |
  | Show: Wireframe (线框) | bool | on (UNVERIFIED) | — | — | |
  | Show: Background (背景) | bool | on (UNVERIFIED) | — | — | shows the frame at capture start |
  | Start Capture (开始捕捉) | button | — | — | — | captures while the mouse is down, within the work area |

  - One Position key per comp frame of (wall time × capture speed); existing Position keys in the captured range are replaced; capture ends at the work-area end.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-ANI-083 · Smoother (平滑器)

- **What it does:** Refits selected keyframes with fewer Bezier keys within a tolerance.
- **Exact semantics:** Window > Smoother panel.

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Apply To (应用到) | enum: Spatial Path (空间路径) / Temporal Graph (时间图表) | auto from property type | — | — | spatial properties: Spatial Path; others: Temporal Graph |
  | Tolerance (容差) | float | 1 (UNVERIFIED) | ≥ 0 | property units | new keys deviate from the original curve by at most this |

  - Requires all keys or ≥ 3 keys selected. Removes and/or adds keys; applies Bezier interpolation at each key when smoothing the temporal curve. Algorithm: UNVERIFIED (curve fitting, e.g. Schneider's Bezier fitting with the tolerance as max error).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Expression alternative `smooth()` (expression part of this file).

### AE-ANI-084 · Wiggler (摇摆器)

- **What it does:** Adds random deviation keyframes between two or more selected keys.
- **Exact semantics:** Window > Wiggler panel.

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Apply To (应用到) | Spatial Path (空间路径) / Temporal Graph (时间图表) | by property type | — | — | spatial props allow both |
  | Noise Type (杂色类型) | Smooth Noise (平滑杂色) / Jagged Noise (锯齿杂色)（译名待核） | Smooth | — | — | |
  | Dimension (维数) | X / Y / Z (one dimension), All Independently (全部独立), All The Same (全部相同)（译名待核） | All Independently (UNVERIFIED) | — | — | |
  | Frequency (频率) | float | 5 (UNVERIFIED) | > 0 | keys per second | 0.5 = one key every 2 s |
  | Magnitude (数量级) | float | 1 (UNVERIFIED) | ≥ 0 | property units | maximum deviation |

  - Adds keys at `1/frequency` intervals between the first and last selected keys with values = original curve value + random deviation in [−magnitude, +magnitude]; randomises interpolation into/out of existing keys. Exact random distribution: UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Expression alternative `wiggle()` (expression part of this file).

### AE-ANI-085 · Other Keyframe Assistant entries (其他关键帧助手命令)

- **What it does:** Remaining entries of Animation > Keyframe Assistant.
- **Exact semantics:** Menu order in current AE: Convert Audio to Keyframes, Convert Expression to Keyframes, Easy Ease, Easy Ease In, Easy Ease Out, Exponential Scale, RPF Camera Import (RPF 摄像机导入), Sequence Layers… (序列图层), Time-Reverse Keyframes (UNVERIFIED order: alphabetical in English).
  - Sequence Layers: see project-comp-layers.md, Sequence layers.
  - RPF Camera Import: imports camera data from RPF (3ds Max) sequences into a camera layer; P2, out of BAE M1 scope.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** —

## Layer time, time stretch and time remapping

### AE-ANI-086 · Frame sample times (帧的采样时间)

- **What it does:** Defines which instant a rendered comp frame represents.
- **Exact semantics:**

  ```
  t_frame(n) = n / fps_comp            // internal comp time in seconds, n = 0, 1, ...; frame start, not centre
  displayed timecode/frame number = n + displayStartTime * fps_comp   // display only
  ```

  - All property evaluation for frame n happens at `t_frame(n)` unless motion blur or frame blending requests sub-frame times (AE-ANI-099..108).
  - Comp Start Timecode / Start Frame (`displayStartTime`, min −10800 s since AE 17.1) only changes display; internal time 0 is the first frame.
  - Drop-frame timecode is display only (project-comp-layers.md / ui-preview-render-io.md, timecode).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** BAE flicks: `t = n * 705_600_000 * fps_den / fps_num` exactly for common rates (23.976 = 24000/1001, 29.97 = 30000/1001, 59.94 = 60000/1001). Scripting `CompItem.frameDuration`, `displayStartTime`, `displayStartFrame` (AE 17.1+).

### AE-ANI-087 · Layer time from comp time: start time and time stretch (图层时间与时间伸缩)

- **What it does:** Maps comp time to the layer's own (source) time using the layer's Start Time and Stretch.
- **Exact semantics:**

  ```
  // startTime: comp time at which layer/source time 0 occurs (can be negative or beyond comp end)
  // stretch: percent, 100 = normal; |stretch| < 1 is coerced to ±1 (scripting doc); negative = reversed
  layerTime(t_comp) = (t_comp - startTime) * 100 / stretch
  compTime(t_layer) = startTime + t_layer * stretch / 100
  ```

  - Keyframes (all properties, incl. Time Remap and markers? — layer markers are stretched too, UNVERIFIED) are stored in layer time, so stretching moves them proportionally (AE help: "all keyframes that belong to the layer are redistributed").
  - In/Out points are comp times; with stretch s the visible source span is `(outPoint − inPoint) * 100/|s|`.
  - Stretch 200 % plays at half speed (each source frame shown for two comp frames at equal fps).
  - Expressions: `time` is comp time; layer-time conversions via `sourceTime()`/`timeToFrames` etc. (expression part of this file).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Scripting `Layer.startTime`, `Layer.stretch`, `Layer.inPoint`, `Layer.outPoint`. Lottie layer `st` (start frame), `sr` (stretch factor, 1 = 100 %), `ip`, `op`; lottie-web computes the layer frame as `(compFrame − st) / sr`. Expression `toComp`-style time functions must use the same mapping.

### AE-ANI-088 · Time Stretch dialog (时间伸缩)

- **What it does:** Sets a layer's stretch factor or new duration while holding one point in time fixed.
- **Exact semantics:** Layer > Time > Time Stretch… (图层 > 时间 > 时间伸缩).

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Stretch Factor (拉伸因数) | float | current stretch | −10000? .. 10000 % (UNVERIFIED limits), \|v\| ≥ 1 | % | |
  | New Duration (新持续时间) | time | current | > 0 | timecode | `stretch = newDuration / sourceSpan * 100` |
  | Hold in Place (原位定格) | enum: Layer In-point (图层入点), Current Frame (当前帧), Layer Out-point (图层出点) | Layer In-point | — | — | which comp time stays fixed |

  ```
  // hold point h (comp time): In point, CTI, or Out point
  lh = layerTime_old(h)
  newStartTime = h - lh * newStretch / 100       // keeps the layer-time instant lh at comp time h
  inPoint/outPoint scaled about h by newStretch/oldStretch
  ```

  - Ctrl/Cmd-click a time in the In or Out column stretches the layer so its In (Out) point lands at the CTI (AE help).
  - A negative factor reverses the layer (AE-ANI-089).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-ui-motion)
- **Notes:** Stretch column in the Timeline: ui-preview-render-io.md. Time-stretching audio changes its pitch (AE-ANI-098).

### AE-ANI-089 · Time-Reverse Layer (时间反向图层)

- **What it does:** Plays the layer backwards while keeping its In and Out points in the comp.
- **Exact semantics:** Layer > Time > Time-Reverse Layer, Ctrl+Alt+R / Cmd+Opt+R.

  ```
  newStretch   = -oldStretch
  // keep In/Out points: the layer-time instant shown at inPoint becomes the one previously shown at outPoint
  newStartTime = inPoint + outPoint - oldStartTime
  ```

  - All keyframes of the layer reverse as a consequence (they are in layer time) (AE help).
  - Layer markers? UNVERIFIED: reversed with the layer.
  - Stretch displayed as −100 %.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** AE help recommends reversing inside a precomp for best results (effects with temporal state).

### AE-ANI-090 · Time Remap evaluation (时间重映射)

- **What it does:** A keyframable Time Remap property gives, for each layer time, the source time (in seconds) to show.
- **Exact semantics:**

  ```
  if layer.timeRemapEnabled:
      t_layer  = layerTime(t_comp)                 // AE-ANI-087
      t_source = TimeRemap.value(t_layer)          // seconds, keyframed in layer time, any interpolation
  else:
      t_source = layerTime(t_comp)
  frame = sourceFrameAt(t_source)                  // AE-ANI-095 (footage) or nested comp time (AE-ANI-093)
  ```

  - Time Remap is a 1-D property displayed as time (timecode/frames); value graph shows source time vs layer time; slope 1 = normal speed, 0 = freeze, negative = reverse.
  - Audio of the layer follows the remapped time (pitch changes, AE-ANI-098).
  - Values outside [0, sourceDuration]: footage shows the first/last frame (clamped); a nested comp is evaluated at that time (may be empty) (UNVERIFIED for footage clamping).
  - Only for layers with a time-based source (footage with duration, precomps, audio); not for stills, solids, text, shape layers (scripting `canSetTimeRemapEnabled`).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Scripting `AVLayer.timeRemapEnabled`, property matchName `ADBE Time Remapping`. Lottie layer `tm` (keyframed, in **seconds**; lottie-web multiplies by the comp frame rate). Expression `timeRemap` property access works like any property.

### AE-ANI-091 · Enable Time Remapping defaults (启用时间重映射)

- **What it does:** Turns on Time Remap and creates its default keyframes.
- **Exact semantics:** Layer > Time > Enable Time Remapping, Ctrl+Alt+T / Cmd+Opt+T.
  - Creates two Linear keys: at the layer's source start (layer time 0 → value 0) and at the source end (layer time = sourceDuration → value = sourceDuration), i.e. identity mapping (UNVERIFIED: keys sit at the source start/end, not at trimmed In/Out points; the end key is at the time just after the last frame, so the last key's value equals the duration and shows the last frame by clamping).
  - While enabled the layer's Out point can be extended beyond the source duration (the last value holds).
  - Disabling deletes the Time Remap keys and returns to `t_source = layerTime`.
  - Time stretch is unavailable/ignored while Time Remapping is enabled? UNVERIFIED: AE allows stretch values but the stretch column is greyed for remapped layers.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** —

### AE-ANI-092 · Freeze Frame and Freeze On Last Frame (冻结帧、在最后一帧上冻结)

- **What it does:** One-step commands that use Time Remap to hold a frame.
- **Exact semantics:**
  - Freeze Frame (Layer > Time > Freeze Frame): enables Time Remap (deleting any existing Time Remap keys) and places a single Hold key at the CTI with value = the source time currently shown; the whole layer shows that frame.
  - Freeze On Last Frame (在最后一帧上冻结（译名待核）): sets Time Remap keys so the layer plays normally and then holds its last frame, and extends the layer Out point to the comp end (AE help). UNVERIFIED key layout: keys at source start (0) and at the last frame time `(lastFrame)/fps` with Hold out.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Freeze On Last Frame added in AE 17.1? (UNVERIFIED version, 2020).

### AE-ANI-093 · Nested composition time mapping (嵌套合成的时间映射)

- **What it does:** Defines which inner-comp time is rendered for a precomp layer.
- **Exact semantics:**

  ```
  t_inner = t_source (from AE-ANI-090, i.e. after start time, stretch and time remap)
  // the inner comp's displayStartTime does NOT offset t_inner (it is display-only)
  if inner.preserveNestedFrameRate:  t_inner = quantize(t_inner, inner.fps)   // AE-ANI-094
  render inner comp at t_inner (sub-frame times allowed for motion blur / outer frame rates)
  ```

  - Without "Preserve frame rate", the inner comp's keyframes are evaluated at whatever times the outer comp asks for, so a 12 fps precomp inside a 24 fps comp animates at 24 fps.
  - Inner layers' own motion blur uses the inner comp's shutter settings unless collapsed (AE-ANI-106).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-render)
- **Notes:** Expression `comp("x")` time functions: expression part of this file. Nested comp resolution/quality: compositing-3d-color.md (Nested comp res/fps).

### AE-ANI-094 · Preserve frame rate when nested or in render queue (嵌套时或在渲染队列中保留帧速率)

- **What it does:** Comp setting that forces a comp to be sampled only at its own frame times when nested or rendered.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Preserve frame rate when nested or in render queue (嵌套时或在渲染队列中保留帧速率) | bool (Composition Settings > Advanced) | off | `CompItem.preserveNestedFrameRate` |

  ```
  quantize(t, fps) = floor(t * fps + ε) / fps        // ε: UNVERIFIED tolerance (suggest 1e-6 frame) to absorb rounding
  ```

  - Applies when the comp is nested or rendered from the Render Queue at a different frame rate (render settings frame-rate override is ignored for such comps).
  - Motion-blur samples of the outer comp that fall inside one inner frame all see the same inner frame (blur of nested content then comes only from the outer layer's own transform).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Posterize Time effect behaves similarly per layer (AE-ANI-097).

### AE-ANI-095 · Source frame selection for video and image sequences (源帧选择)

- **What it does:** Chooses which decoded frame of footage is shown for a given source time.
- **Exact semantics:**

  ```
  fps_src  = interpreted frame rate of the footage (Interpret Footage, project-comp-layers.md)
  t_src    = t_source  (+ footage start offset for sequences/timecode = 0 internally)
  i        = floor(t_src * fps_src + ε)          // ε UNVERIFIED (suggest 1e-4 frame; AE is believed to use a small tolerance so that
                                                 // 29.97/30 conversions do not drop frames)
  i        = clamp(i, 0, frameCount - 1)          // before start / after end: first / last frame (UNVERIFIED for t < 0)
  ```

  - With frame blending off this is the frame shown for any sub-frame time (nearest-lower frame, "sample and hold").
  - Field-rendered / pulldown footage: fields are separated per Interpret Footage before this step (project-comp-layers.md, Interpret Footage).
  - Looping (Interpret Footage > Loop N times): `t_src = t_src mod originalDuration` within the looped duration.
- **Priority:** P0
- **Owner:** bae-media (+ bae-anim)
- **Notes:** Implementation hint: compute `i` with exact rational arithmetic on flicks and the footage's rational fps (e.g. 30000/1001) — no float epsilon needed when both are rational; BAE decision: use exact rational floor, which matches AE except in degenerate float cases.

### AE-ANI-096 · Stills, generated layers and time beyond the source (静止图像与源范围外的时间)

- **What it does:** Time behaviour of layers without a time-varying source.
- **Exact semantics:**
  - Still images, solids, shape, text, null, camera, light and adjustment layers have no source time; their properties are still evaluated at layer time (keyframes and expressions animate them). Their duration is set by the Still Footage preference / comp duration (project-comp-layers.md).
  - Footage layers cannot be trimmed beyond the source duration unless Time Remap is enabled or the footage is looped; precomp layers likewise up to the inner comp duration.
  - Image sequences with missing frames: missing frames show as missing-footage colour bars? (project-comp-layers.md, Missing footage).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** —

### AE-ANI-097 · Posterize Time (色调分离时间)

- **What it does:** The Posterize Time effect (Time category) makes a layer's evaluation update at a lower frame rate.
- **Exact semantics:**

  ```
  t_eval = floor((t - layerStart?) * fps_effect + ε) / fps_effect   // UNVERIFIED origin: comp time 0 vs layer start
  // the layer's source, masks, effects ABOVE Posterize Time and transform? are evaluated at t_eval
  ```

  - UNVERIFIED: Posterize Time affects the layer's source frame and the effects before it in the stack; whether it affects the layer's transform depends on AE version (it does not affect transforms in current versions; expression `posterizeTime()` is the usual way to step transforms).
  - Parameter: Frame Rate (帧速率), default 12? UNVERIFIED (effects.md, Time effects, is authoritative).
- **Priority:** P1
- **Owner:** bae-effects (+ bae-anim)
- **Notes:** Effect parameters: effects.md (Time: Posterize Time). Expression `posterizeTime(fps)`: expression part of this file.

### AE-ANI-098 · Audio under time stretch and time remap (音频的时间伸缩与重映射)

- **What it does:** Audio follows the layer's time mapping.
- **Exact semantics:** Audio samples are read at `t_source(t_comp)`; stretching or remapping resamples the audio so pitch changes proportionally to playback speed (no time-stretch pitch preservation). Reverse playback plays audio backwards. Freeze (slope 0) produces silence? UNVERIFIED (AE plays nothing while the remap value is constant).
- **Priority:** P1
- **Owner:** bae-media (+ bae-anim)
- **Notes:** AE help notes clicks at edits of remapped audio; fix with Levels. Audio panel/preview: ui-preview-render-io.md.

## Frame blending

### AE-ANI-099 · Frame blending switches and applicability (帧混合)

- **What it does:** Smooths retimed footage by synthesising in-between frames; controlled per layer, per comp and per render.
- **Exact semantics:**

  | Control (中文) | Values | Default | Notes |
  |---|---|---|---|
  | Layer Frame Blending switch / Layer > Frame Blending (图层 > 帧混合) | Off, Frame Mix (帧混合), Pixel Motion (像素运动) | Off | scripting `AVLayer.frameBlendingType`: `FrameBlendingType.NO_FRAME_BLEND`, `.FRAME_MIX`, `.PIXEL_MOTION` |
  | Comp Enable Frame Blending switch (启用帧混合) | on/off | off (UNVERIFIED) | `CompItem.frameBlending`; when off, no layer in the comp blends |
  | Render Settings > Frame Blending | Current Settings, On for Checked Layers, Off for All Layers | Current Settings (UNVERIFIED default for "Best Settings" template: On for Checked Layers) | ui-preview-render-io.md, Render settings |

  - Applies only to layers whose source is motion footage (video, image sequence); not to stills, solids, text, shape layers, and **not to precomp layers** (apply it to the footage layers inside instead).
  - Takes effect whenever the source time for a comp frame does not land exactly on a source frame (time stretch, time remap, or footage frame rate ≠ comp frame rate).
  - Layer Quality Draft always uses Frame Mix even if Pixel Motion is selected (AE help).
- **Priority:** P1
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** Render implementation: compositing-3d-color.md (motion-blur & frame-blend rendering). Timewarp effect is separate (effects.md, Time).

### AE-ANI-100 · Frame Mix weighting (帧混合的加权)

- **What it does:** Frame Mix cross-fades source frames according to the fractional source time.
- **Exact semantics:** UNVERIFIED reconstruction (AE does not document weights):

  ```
  x  = t_source * fps_src            // continuous source frame position
  // slowed down or similar speed (source advances <= 1 frame per output frame):
  i  = floor(x + ε);  f = x - i
  out = (1 - f) * frame[i] + f * frame[i+1]          // premultiplied RGBA, in working space
  // sped up (source advances r > 1 frames per output frame, r = |dx/dt_out|):
  out = average of frames floor(x - r/2) .. floor(x + r/2)   // box average over the frames the output frame spans
  ```

  - At f == 0 the result is exactly frame[i] (no blur for on-frame times).
  - At the last frame, frame[i+1] is clamped to the last frame.
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim, bae-media)
- **Notes:** BAE should validate by rendering a numbered-frame test clip at 50 % speed and reading the pixel mix. Implementation hint: GPU lerp of two decoded textures; the decoder must keep a 2-frame window.

### AE-ANI-101 · Pixel Motion (像素运动)

- **What it does:** Synthesises in-between frames by optical flow (motion vectors between adjacent frames) instead of cross-fading.
- **Exact semantics:** For a fractional source position `x = i + f`, estimate flow between frame[i] and frame[i+1] and warp both toward time f, blending by f. AE's algorithm is proprietary; BAE needs a visually comparable optical-flow interpolator. Draft quality falls back to Frame Mix.
- **Priority:** P2
- **Owner:** bae-render (+ bae-media)
- **Notes:** The Timewarp effect exposes a configurable version of the same technology (effects.md, Time). Implementation hint: GPU block-matching or a compact neural-free flow (e.g. DIS optical flow) in WGSL.

## Motion blur sampling

### AE-ANI-102 · Motion blur switches and render settings (运动模糊开关)

- **What it does:** Determines whether a layer is rendered with motion blur.
- **Exact semantics:**

  ```
  blur(layer) = layer.motionBlur switch (运动模糊)            // per layer, Layer > Switches > Motion Blur
             && comp.enableMotionBlur (启用运动模糊)            // Timeline panel button; affects previews
             && renderSettings.motionBlur != OffForAllLayers   // final render; "Current Settings" uses the comp switch,
                                                              // "On for Checked Layers" ignores the comp switch
  ```

  - Render Settings > Motion Blur: Current Settings (当前设置), On for Checked Layers (对选中图层打开), Off for All Layers (对所有图层关闭); plus Shutter Angle override ("Use comp's shutter angle" / "Override shutter angle" value) (UNVERIFIED wording; ui-preview-render-io.md, Render settings).
  - With the "Switches Affect Nested Comps" preference on, nested comps obey the outer comp's Enable Motion Blur switch (AE help).
  - Motion blur only blurs motion created in AE (transform, masks, shape/text animation, effects that support it); motion inside footage is not blurred (use Pixel Motion Blur effect).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-anim)
- **Notes:** Scripting `AVLayer.motionBlur`, `CompItem.motionBlur`. Lottie: no motion blur (comp-level `mb` exists in some exporters, ignored by lottie-web).

### AE-ANI-103 · Shutter Angle and Shutter Phase sample interval (快门角度与快门相位)

- **What it does:** Defines the time interval integrated for each frame.
- **Exact semantics:** Composition Settings > Advanced > Motion Blur.

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shutter Angle (快门角度) | int | 180 | 0 – 720 | degrees | 360 = whole frame duration |
  | Shutter Phase (快门相位) | int | −90 | −360 – 360 | degrees | offset of shutter open from frame time |

  ```
  T      = 1 / fps_comp                         // comp frame duration (the comp in which the layer is blurred)
  t_open = t_frame + (shutterPhase / 360) * T
  dur    = (shutterAngle / 360) * T
  interval = [t_open, t_open + dur]
  // default 180 / -90: interval = [t - T/4, t + T/4], centred on the frame time
  // N samples (AE-ANI-104):  t_k = t_open + dur * (k + 0.5) / N,  k = 0..N-1     // UNVERIFIED AE distribution
  ```

  - Angle 0 → no blur. Phase = −angle/2 centres the blur on the unblurred position (AE help).
  - Example (AE help): 90° at 24 fps = exposure 1/96 s.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model, bae-render)
- **Notes:** Scripting `CompItem.shutterAngle` (int 0–720), `shutterPhase` (int −360–360). AE SDK: `PF_InData.shutter_angle` (fraction of 360°: 180° → current_time .. current_time + ½ time_step) and `shutter_phase` (offset from frame time to shutter open as a fraction of a frame), which confirms the interval formula. The Timeline shows the shutter interval as a grey band around the CTI when zoomed to frames (ui file).

### AE-ANI-104 · Samples Per Frame and Adaptive Sample Limit (每帧样本数与自适应采样限制)

- **What it does:** Sets how many sub-frame samples are taken, fixed for some layer kinds and adaptive for 2D layers.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | Samples Per Frame (每帧样本) | int | 16 | 2 – 64 | minimum; the fixed count for Classic 3D layers, shape layers and certain effects |
  | Adaptive Sample Limit (自适应采样限制) | int | 128 | 16 – 256 | maximum for 2D layer motion |

  - 2D AV layers: N adapts per layer per frame to the layer's motion: `N = clamp(ceil(maxDisplacementPx / step), samplesPerFrame, adaptiveSampleLimit)` where `maxDisplacementPx` is the largest screen-space movement of the layer's corners over the shutter interval; UNVERIFIED step (believed ≈ 1 px, i.e. one sample per pixel of travel).
  - 3D layers (Classic 3D), shape layers, text layers? (UNVERIFIED) and effects that use the comp shutter: fixed N = Samples Per Frame.
  - When AE cannot determine an adaptive rate (non-affine motion), it uses Samples Per Frame.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-render)
- **Notes:** Scripting `motionBlurSamplesPerFrame` (2–64), `motionBlurAdaptiveSampleLimit` (16–256). BAE decision candidate: allow a per-comp "quality" scale for previews; final render must honour AE values.

### AE-ANI-105 · What motion contributes to motion blur and how samples are combined (运动模糊的贡献来源与样本合成)

- **What it does:** Defines which time-varying inputs are evaluated per sample and how the samples are averaged.
- **Exact semantics:**
  - Per sample time t_k: evaluate the layer's world transform (own transform, auto-orient, all parents, expressions on them) and, for 3D layers, the camera view/projection; 2D layers ignore camera motion.
  - 2D AV layers: AE renders the layer's source + masks + effects once at the frame time and accumulates N copies transformed by the per-sample matrices (transform-only blur) (UNVERIFIED but consistent with "adaptive 2D layer motion" and with effects needing their own blur); animated masks blur per AE-ANI-107; effects that support motion blur (shutter-aware) blur their own animation.
  - Shape layers: the whole layer (paths, shape transforms, Repeaters, Trim Paths) is re-evaluated and rendered at each sample. Text layers: animators evaluated per sample (UNVERIFIED).
  - Combination: equal-weight average (box filter) of the N premultiplied samples, in the working colour space (linear if the project linearizes) (UNVERIFIED weighting; no tent filter documented).
  - Opacity changes over the shutter are included only as far as the layer is re-rendered per sample (UNVERIFIED for 2D layers: opacity is taken at the frame time).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-render)
- **Notes:** Accumulation, bounds growth and GPU strategy: compositing-3d-color.md (motion-blur rendering). Implementation hint: for transform-only blur compute N matrices in bae-anim (`EvaluatedComp` carries `Vec<Mat4>` per layer) and let bae-render accumulate in one pass.

### AE-ANI-106 · Motion blur of nested and collapsed compositions (嵌套与折叠合成的运动模糊)

- **What it does:** Defines whose shutter settings apply when compositions are nested.
- **Exact semantics:**
  - A precomp layer's own transform motion is blurred in the outer comp with the outer comp's shutter settings when the precomp layer's Motion Blur switch is on.
  - Layers inside the precomp are blurred inside the inner comp with the inner comp's shutter angle/phase/samples, gated by the inner layers' switches and the inner comp's Enable Motion Blur switch (or the outer comp's switch when "Switches Affect Nested Comps" is on).
  - Collapse Transformations on the precomp layer: inner layers are rendered as part of the outer comp; their blur uses the outer comp's shutter settings and the precomp layer's Motion Blur switch gates the outer contribution (UNVERIFIED).
  - The inner comp is evaluated at the sub-frame times asked by the outer comp (AE-ANI-093) unless Preserve frame rate is on.
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Collapse semantics: compositing-3d-color.md (collapse/continuous rasterize).

### AE-ANI-107 · Mask Motion Blur (蒙版运动模糊)

- **What it does:** Per-mask control of whether animated mask paths produce motion blur.
- **Exact semantics:** Layer > Masks > Motion Blur (图层 > 蒙版 > 运动模糊).

  | Option (中文) | Behaviour |
  |---|---|
  | Same As Layer (与图层相同) | mask blurs only if the layer's Motion Blur switch is on (default) |
  | On (开) | mask blurs even if the layer switch is off |
  | Off (关) | mask never blurs |

  - In every case the comp's Enable Motion Blur switch must be on (AE help).
  - Mask path, feather, expansion and opacity animation are evaluated per sample (UNVERIFIED for feather/expansion).
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim, bae-model)
- **Notes:** Scripting `MaskPropertyGroup.maskMotionBlur` (`MaskMotionBlur.SAME_AS_LAYER`, `.ON`, `.OFF`). Mask rendering: compositing-3d-color.md (Masks).

### AE-ANI-108 · Effects with their own shutter (自带快门的效果)

- **What it does:** Some effects animate geometry themselves and can render motion blur with either the comp shutter or their own.
- **Exact semantics:**
  - Transform effect (变换): checkbox "Use Composition's Shutter Angle" (使用合成的快门角度) default on, and "Shutter Angle" (快门角度) 0–360? default 0 (UNVERIFIED range/default) used when the checkbox is off; it blurs the effect's own animated transform over the interval of AE-ANI-103 with the comp's sample count.
  - Effects declaring shutter use (SDK `PF_OutFlag_I_USE_SHUTTER_ANGLE`) receive the shutter angle and phase only when motion blur is enabled for the layer and comp; they sample their own parameters within the interval.
  - Pixel Motion Blur and CC Force Motion Blur are effects with their own sample settings (effects.md).
- **Priority:** P1
- **Owner:** bae-effects (+ bae-anim)
- **Notes:** Effect parameters: effects.md (Distort: Transform; Time: Pixel Motion Blur, CC Force Motion Blur). bae-plugin must pass shutter data to plugins (bae-plugin).
