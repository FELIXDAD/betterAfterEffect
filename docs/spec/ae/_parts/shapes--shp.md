# After Effects inventory: shape layers (AE-SHP)

**Scope.** This file specifies everything about After Effects shape layers (形状图层): the structure of a shape layer's Contents and its exact order of operations (paths → path operators → paint, group isolation, Composite, Merge Paths suppression), the parametric primitives (Rectangle, Ellipse, Polystar) with the exact Bezier geometry they generate, freeform Bezier paths and path direction, fills, strokes (line caps/joins, dashes, taper, wave), gradient fills and strokes (editor data, storage, evaluation), every path operator (Merge Paths, Offset Paths, Pucker & Bloat, Repeater, Round Corners, Trim Paths, Twist, Wiggle Paths, Wiggle Transform, Zig Zag), group transforms, masks on shape layers, shape conversion commands (Create Shapes from Vector Layer, Create Shapes from Text, Convert to Bezier Path), the shape and pen tools, and the Bezier path model as it appears in AE scripting/expressions and in Lottie (`ty: 4` shape layers). Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md, ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

Conventions used in this file: coordinates are AE layer space (x right, y **down**, angles in degrees, positive = clockwise on screen). "Above" / "below" refer to the Timeline stacking order inside a property group (item 1 is the top). Lottie keys refer to the bodymovin/Lottie JSON schema; lottie-web and Skottie are cited as behaviour references where Adobe does not document a behaviour.

## Shape layer structure and order of operations

### AE-SHP-001 · Shape layer and Contents (形状图层与内容)

- **What it does:** A shape layer is a synthetic (non-footage) layer whose image is produced by rendering a tree of vector "shape attributes" held in its Contents property group. It has the normal layer Transform group, masks, effects and layer styles like any AV layer.
- **Exact semantics:**
  - Property tree (matchNames): layer `ADBE Vector Layer` → `ADBE Root Vectors Group` ("Contents", 内容) → ordered list of shape attributes. Each attribute is exactly one of:
    | Kind | Members | Notes |
    |---|---|---|
    | Path (路径) | Rectangle, Ellipse, Polystar, Path | Generate geometry only; invisible on their own. |
    | Paint (绘画操作) | Fill, Stroke, Gradient Fill, Gradient Stroke | Paint the paths above them. |
    | Path operation (路径操作) | Merge Paths, Offset Paths, Pucker & Bloat, Repeater, Round Corners, Trim Paths, Twist, Wiggle Paths, Wiggle Transform, Zig Zag | Modify paths above them (Repeater also duplicates paint). |
    | Group (组) | Group | Container with its own Contents, Blend Mode and Transform (AE-SHP-002). |
  - The root Contents behaves as an implicit group with identity transform and Normal blend mode, except that it has no Transform/Blend Mode properties; the layer Transform positions it.
  - Shape coordinates are in layer space; the layer has no intrinsic size: layer anchor point default is (0,0) and layer space origin (0,0) is the reference for all path coordinates. The layer's width/height reported to scripting/expressions equals the comp size (UNVERIFIED: AVLayer.width/height of a shape layer returns the composition width/height).
  - Bit depth follows the project (8/16/32 bpc), but fill/stroke colours are clipped to [0,1] (Adobe-documented; see AE-SHP-016).
  - A shape layer cannot be opened in the Layer panel (same as text layers).
  - Every attribute and group has a visibility toggle (eyeball, scripting `PropertyBase.enabled`); a disabled attribute is skipped entirely in evaluation as if absent (Lottie `hd: true`).
  - Names: each attribute has a user-editable name (Lottie `nm`) and fixed matchName (Lottie `mn`); default names are "<Kind> <n>" numbered per kind within the layer (e.g. "Rectangle 1", "Fill 1", "Group 1"), UNVERIFIED: numbering counts existing siblings of the same kind in the target group.
- **Priority:** P0
- **Owner:** bae-model (+ bae-vector)
- **Notes:** Lottie: layer `ty: 4`, attributes in `shapes[]`, each with `ty` code (see AE-SHP-060 for the full table). Shape layers are always rasterized from their vectors at the final transformed resolution (no blurring when scaled up). UNVERIFIED: the effect of the Collapse Transformations/Continuously Rasterize switch on shape layers (best understanding: it changes where effects/masks are applied relative to the layer transform, as for continuously rasterized vector footage). Rendering of the layer as a whole (quality, motion blur, bounds) is in compositing-3d-color.md, "Collapse/continuous rasterize" and "Layer bounds". Material Options group (`ADBE Vector Materials Group`) exists on groups for the Advanced 3D/C4D renderer (AE-SHP-062). Implementation hint: evaluate the attribute tree into a flat display list of (geometry, paint, transform, opacity, blend) draw commands per frame, cache by property-change hash, and hand the list to Vello.

### AE-SHP-002 · Group (组)

- **What it does:** A Group is a container shape attribute holding its own ordered Contents plus a Blend Mode and a Transform; it scopes path operations and paints and transforms everything inside it like a parent.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Blend Mode (混合模式) | enum | Normal | AE layer blend modes | — | `ADBE Vector Blend Mode`; how the group's rendered result composites onto the content below it in the parent group. |
  | Contents (内容) | group | empty | — | — | `ADBE Vectors Group`; Lottie `it[]`. |
  | Transform: <name> (变换) | group | identity | — | — | `ADBE Vector Transform Group`; Lottie: last element of `it[]` with `ty: "tr"`; see AE-SHP-029. |
  | Material Options (材质选项) | group | — | — | — | Only used by 3D renderers; see AE-SHP-062. |
  - A group's paths, after the group's own path operators, are transformed by the group transform and then contributed to the parent group's path list (so paints/operators below the group in the parent also act on them) — see AE-SHP-003.
  - A group's paint output is rendered as a unit at the group's stacking position in the parent (like a nested comp); Group opacity is defined in AE-SHP-030.
  - Group (Empty) (组（空）) from the Add menu creates a group with no contents and identity transform.
- **Priority:** P0
- **Owner:** bae-vector (+ bae-model)
- **Notes:** Lottie `ty: "gr"`, `np` = number of properties (informational), `cix` (property index) are exporter details; BAE must accept a group whose `tr` is missing (identity). The dashed bounding box shown for a selected group in the Composition panel is UI (bae-ui-motion). Expression access: `content("Group 1").transform.position`, see transform-keyframes-expressions.md, expression property objects.

### AE-SHP-003 · Shape render order and order of operations (形状的渲染顺序)

- **What it does:** Defines exactly which paths each path operator and each paint acts on, the order operators run, and the back-to-front order in which paints and groups are drawn.
- **Exact semantics:**
  - Rules (Adobe-documented): (1) path operations and paint operations apply to all paths **above** them in the same group, including paths inside nested groups above them; (2) all path operations act before paints use the geometry; path operations run top to bottom; (3) paints and groups are drawn bottom to top, so an item higher in the list is in front (unless a paint's Composite says otherwise, AE-SHP-004); (4) a group's transform applies to the group's paths before they are passed to the parent's operators/paints.
  - Reference algorithm (matches lottie-web `searchShapes` and Skottie `attachShape`; evaluate per frame):
    ```
    // returns (draws: list of draw nodes, top-first), and pushes this group's
    // surviving geometry into parentGeos
    fn eval_group(items, pendingOps /* operators from enclosing groups, innermost last */,
                  parentGeos, suppressed /* inherited Merge suppression */) -> Draws
      // pass 1, bottom -> top: compute Merge Paths suppression for every item
      suppress_flag_at = array(len)
      for i in (len-1 down to 0):
          suppress_flag_at[i] = suppressed
          if items[i] is MergePaths: suppressed = true     // paints ABOVE a Merge are not drawn
      // pass 2, top -> bottom; opsBelow(i) = this group's path operators at index > i (innermost last)
      geos = []; draws = []
      for i in 0..len:
        match items[i]:
          Path p        => geos.push(p.generate())                       // AE-SHP-010..015
          PathOperator o=> geos = o.apply(geos)                         // consumes current list
                           // (Repeater additionally repeats `draws` so far, AE-SHP-034)
          Group g       => draws.push(eval_group(g.items, pendingOps ++ opsBelow(i),
                                                 &mut geos, suppress_flag_at[i]))
          Paint s       => if !suppress_flag_at[i] and !geos.empty():
                             g = geos
                             for op in pendingOps (innermost first): g = op.apply(g)
                             draws.push(Draw(g as one compound path, s))   // AE-SHP-016..028
      // group transform and opacity wrap this group's draws; geometry goes to parent transformed
      for g in geos: parentGeos.push(g.transformed(group.transform))
      return draws drawn in reverse list order (bottom item first, top item last = in front)
    ```
  - Consequences an implementation must reproduce:
    - A paint draws the union of all paths above it in its group (including transformed sub-group paths) as **one compound path** with the paint's fill rule; paths are not painted separately (matters for gradients, dashes across paths, Even-Odd holes and stroke self-overlap).
    - A paint placed above all paths draws nothing. Paths below the last paint are invisible.
    - Two fills on the same paths: the upper one is in front (by default).
    - A path operator applies to paths in nested groups above it, and those nested groups' own paints see the operator's result too (pending operators from enclosing groups are applied when the nested paint draws). UNVERIFIED: whether AE applies an enclosing operator before or after the nested group's transform for nested paints; Skottie applies it in the nested group's local space (before its transform), which differs from AE for transform-sensitive operators (Trim lengths, Offset amount, Wiggle size, Zig Zag size) when the nested group is scaled.
    - Merge Paths consumes all paths above it and suppresses every paint above it in the same group and in nested groups above it (AE-SHP-031).
  - Order of evaluation of animated values is irrelevant (all properties are sampled at the same layer time first).
- **Priority:** P0 — every shape layer depends on it.
- **Owner:** bae-vector
- **Notes:** Adobe help "Render order for shapes within a shape layer" (overview-shape-layers-paths-vector). lottie-web registers each path with every operator that is still "open" (not yet closed by finishing its group) and runs operators in document order (`renderModifiers` iterates the reversed list); Skottie keeps a geometry-effect stack. Both treat the Repeater specially (AE-SHP-034). Implementation hint: build a static "evaluation plan" from the tree once (rebuild only when the structure changes), then each frame just evaluates property values and runs the plan; keep geometry in kurbo `BezPath` in f64 and transform to f32 only for Vello.

### AE-SHP-004 · Paint Composite option (合成)

- **What it does:** Per fill/stroke/gradient fill/gradient stroke, chooses whether the paint is drawn behind or in front of the paint that precedes it (is above it) in the same group.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Composite (合成) | enum | Below Previous in Same Group | 1–2 | — | matchName `ADBE Vector Composite Order` |
  - Enum (menu order, scripting value): 1 = Below Previous in Same Group (在同组中前一个之下（译名待核）), 2 = Above Previous in Same Group (在同组中前一个之上).
  - With all paints at the default, draw order within a group is bottom-to-top (top item in front). A paint set to "Above Previous" is drawn after (in front of) the draw item immediately above it in the list. Reference:
    ```
    // draws: list in Timeline order (index 0 = top). Build back-to-front order.
    order = []
    for d in draws reversed:                 // bottom first
        order.push(d)
    // then for each paint with composite == Above, move it so it is drawn
    // immediately after the item directly above it (its "previous").
    ```
    UNVERIFIED: exact semantics when several consecutive paints are set to Above (chain behaviour) and whether "previous" includes groups; best understanding: "previous" = the immediately preceding paint or group in the same group, and a chain of Above paints yields top-to-bottom drawing for that run.
- **Priority:** P1
- **Owner:** bae-vector (+ bae-model)
- **Notes:** Not exported by bodymovin/Lottie (no key); Lottie players always use the default order. Official zh-CN help string "在同组中前一个之上" is used for the Above option.

### AE-SHP-005 · Blend modes inside a shape layer (形状内部的混合模式)

- **What it does:** Each Group, Fill, Stroke, Gradient Fill and Gradient Stroke has its own Blend Mode that controls how it composites onto what has already been drawn inside the shape layer.
- **Exact semantics:**
  - Property `ADBE Vector Blend Mode` on groups and paints; enum = the AE layer blend-mode list (Normal … Luminosity, Dissolve and Dancing Dissolve excluded — UNVERIFIED which modes are offered; Lottie `bm` uses the layer blend-mode numbering 0–15: 0 Normal, 1 Multiply, 2 Screen, 3 Overlay, 4 Darken, 5 Lighten, 6 Color Dodge, 7 Color Burn, 8 Hard Light, 9 Soft Light, 10 Difference, 11 Exclusion, 12 Hue, 13 Saturation, 14 Color, 15 Luminosity).
  - Paint blending happens inside the layer's own buffer: a paint blends only with content drawn earlier in the same shape layer (lower paints/groups), never with layers below in the comp. The finished layer then composites with the layer blend mode (compositing-3d-color.md, Blend modes & formulas).
  - Each group renders into an isolated transparent buffer and is composited onto its parent's buffer with the group blend mode (group opacity handling: AE-SHP-030) (Adobe: rendering a shape layer is "similar to rendering a composition that contains nested compositions"; "each stroke and fill has its own blending mode, which determines how it interacts with other paint operations in the same group"). UNVERIFIED: whether a group set to Normal is still isolated (observable only when a paint inside it uses a non-Normal mode); best understanding: yes, isolated.
  - Blend formulas are the layer formulas (compositing-3d-color.md).
- **Priority:** P1
- **Owner:** bae-render (+ bae-vector)
- **Notes:** lottie-web canvas renderer ignores per-paint blend modes; SVG renderer applies `mix-blend-mode` per element; Skottie wraps each draw with a blend node. Implementation hint: only allocate an offscreen layer (Vello `push_layer`) for a group/paint when its blend mode is non-Normal or group opacity < 1 and isolation is required; otherwise draw directly.

### AE-SHP-006 · Add menu and insertion rules (添加菜单)

- **What it does:** The Add menu (in the Tools panel when a shape layer is selected or a shape tool is active, and in the Timeline next to Contents) inserts a new shape attribute into the selected group(s).
- **Exact semantics:**
  - Menu items in order: Group (Empty) (组（空）); Rectangle (矩形); Ellipse (椭圆); Polystar (多边星形); Path (路径); Fill (填充); Stroke (描边); Gradient Fill (渐变填充); Gradient Stroke (渐变描边); Merge Paths (合并路径); Offset Paths (位移路径); Pucker & Bloat (收缩和膨胀); Repeater (中继器); Round Corners (圆角); Trim Paths (修剪路径); Twist (扭转); Wiggle Paths (摆动路径); Wiggle Transform (摆动变换); Zig Zag (Z 字形). UNVERIFIED: exact separator placement.
  - Target: the selected group(s), or Contents if nothing inside is selected; one copy is inserted into each selected group.
  - Insertion position (Adobe-documented):
    - New paths: below existing paths and groups.
    - New path operations: below existing path operations; if none exist, below existing paths.
    - New paints: below existing paths and above existing paints.
    - Repeater: always at the end of the group.
    - Holding Alt/Option while choosing: insert at the end of the group regardless of type.
  - Merge Paths additionally inserts a Fill and a Stroke after it if none exist below it (AE-SHP-031).
  - Added primitives get Add-menu defaults (AE-SHP-010..012). UNVERIFIED: added paints take the Tools-panel settings (Add > Fill uses the Tools panel fill colour; Add > Stroke uses the Tools panel stroke colour and width).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-ui-motion, bae-model)
- **Notes:** The command (insert attribute at rule-determined index) lives in bae-motion; the menu UI in bae-ui-motion. Scripting: `PropertyGroup.addProperty(matchName)` appends at the end (no rule-based placement); `canAddProperty()`.

### AE-SHP-007 · Group Shapes and Ungroup Shapes (组合形状 / 取消组合形状)

- **What it does:** Wraps the selected shape attributes in a new group, or dissolves a group into its parent.
- **Exact semantics:**
  - Group Shapes: Layer > Mask and Shape Path (蒙版和形状路径) > Group Shapes; Ctrl+G / Cmd+G. Selected attributes must be at the same grouping level; they are moved, preserving relative order, into a new Group inserted at the position of the topmost selected item. The new group's Transform has Anchor Point = Position = centre of the bounding box of the grouped content (so the visual result is unchanged), all other transform values identity.
  - Ungroup Shapes: Layer > Mask and Shape Path > Ungroup Shapes; Ctrl+Shift+G / Cmd+Shift+G. Single group selected; its children replace it in the parent at its position. UNVERIFIED: how a non-identity group transform is preserved (best understanding: the transform is baked into the children's own transform/path values where possible; otherwise visual result may change).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Bounding-box computation uses the same geometry bounds as AE-SHP-009 (paths without stroke extents, UNVERIFIED).

### AE-SHP-008 · New shape layer creation and default transform (新建形状图层)

- **What it does:** Shape layers are created empty via Layer > New > Shape Layer, or by drawing with a shape tool or the Pen tool with no layer selected (AE-SHP-051).
- **Exact semantics:**
  - Layer > New > Shape Layer (图层 > 新建 > 形状图层): empty Contents; layer Anchor Point (0,0); Position = comp centre (w/2, h/2); duration/in-point per comp defaults (project-comp-layers.md, layer timing); name "Shape Layer <n>" (形状图层 <n>).
  - Drawing a parametric shape with no layer selected: new shape layer, layer Anchor Point (0,0), layer Position = comp centre; inside, a new Group "<Kind> 1" containing the primitive (path Position (0,0), drawn size), Stroke 1, Fill 1 (per Tools panel settings), and the group Transform Position = drawn centre − comp centre (layer space). UNVERIFIED: exact split of the offset between group Position and primitive Position; best understanding as stated.
  - Drawing with the Pen tool: Bezier vertices stored in layer space (relative to layer origin = comp centre); group Transform identity. UNVERIFIED.
  - Preference "Center Anchor Point in New Shape Layers" (在新形状图层中居中放置锚点（译名待核）, Preferences > General): when on, a new shape layer created by drawing a parametric shape (Rectangle/Rounded Rectangle/Ellipse/Polygon/Star tool) gets its layer Anchor Point at the centre of the drawn shape's bounds (layer space) and Position at that same point in comp space, so the shape stays put. It does not apply to Pen-tool paths (Adobe community expert, 2018). The preference exists at least since AE CC 2018 (15.x). UNVERIFIED: default value (best understanding: on in current versions).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Layer > Transform > Center Anchor Point in Layer Content (Ctrl+Alt+Home / Cmd+Option+Home) uses the shape content bounds (project-comp-layers.md, layer editing ops). Source for the preference: https://community.adobe.com/t5/after-effects-discussions/center-anchor-point-in-new-shape-layers/td-p/10103192

### AE-SHP-009 · Shape layer bounds and sourceRectAtTime (形状图层边界)

- **What it does:** Defines the rectangle that encloses a shape layer's content, used by sourceRectAtTime(), Center Anchor Point in Layer Content, the layer bounding box in the viewer, Fit to comp and effects' input bounds.
- **Exact semantics:**
  - Bounds are computed in layer space from the final geometry of every visible draw (after all path operators, Repeater copies and group transforms), as the tight bounds of the Bezier curves (not just vertices).
  - `sourceRectAtTime(t = time, includeExtents = false)` returns `{top, left, width, height}` in layer space. For shape layers, `includeExtents = true` expands the rectangle by stroke extents (half stroke width plus caps/miter joins); `false` returns path bounds only. UNVERIFIED: whether the miter/cap extension is included exactly or approximated by half width; best understanding: exact stroke outline bounds.
  - Empty content → `{top:0, left:0, width:0, height:0}`. UNVERIFIED.
  - Masks and effects do not change sourceRectAtTime; layer styles do not either (UNVERIFIED).
- **Priority:** P0 — expression rigs (auto-sizing boxes) rely on it.
- **Owner:** bae-vector (+ bae-anim)
- **Notes:** The expression API itself is in transform-keyframes-expressions.md, "sourceRectAtTime". The render-time layer bounds (for effects, motion blur, buffer allocation) are in compositing-3d-color.md, "Layer bounds"; shape layers are not limited to comp size. Implementation hint: kurbo `BezPath::bounding_box()` gives exact curve bounds; stroke extents via the stroked outline bounds or a conservative `w/2 * max(1, miterLimit)` expansion for miter joins.

## Parametric primitives and paths

### AE-SHP-010 · Rectangle path (矩形路径)

- **What it does:** Parametric axis-aligned rectangle with optional rounded corners, generating a closed Bezier path.
- **Exact semantics:**
  | Parameter (中文) | Type | Default (Add menu) | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shape Direction (反转路径方向) | enum | Normal | see AE-SHP-014 | — | `ADBE Vector Shape Direction`, Lottie `d` |
  | Size (大小) | 2D vector, linkable (constrain proportions toggle) | (100, 100) | ≥ 0 (UNVERIFIED: negative values not accepted) | px | `ADBE Vector Rect Size`, Lottie `s` |
  | Position (位置) | 2D point | (0, 0) | any | px | centre of the rectangle in group space; `ADBE Vector Rect Position`, Lottie `p` |
  | Roundness (圆度) | scalar | 0 | ≥ 0 | px | corner radius; `ADBE Vector Rect Roundness`, Lottie `r` |
  - Tool-drawn rectangles get Size = dragged size, Position (0,0) inside a new group whose Position is the drag centre (AE-SHP-008, AE-SHP-052); Rounded Rectangle tool sets Roundness to the tool value (UNVERIFIED default for the tool: 10% of the smaller side? best understanding: Roundness starts at the last-used value, initially 10 px).
  - Generated path (lottie-web `RectShapeProperty`, which reproduces AE's vertex order; `K = 0.5519`):
    ```
    hw = Size.x/2; hh = Size.y/2; (cx,cy) = Position
    R  = min(hw, hh, Roundness)          // clamp: radius never exceeds half the shorter side
    c  = R * (1 - K)                     // handle tip distance from the corner apex
    L=cx-hw; Rt=cx+hw; T=cy-hh; B=cy+hh
    if direction == Normal (clockwise on screen, y-down):
      if R == 0: vertices (corner, no tangents): (Rt,T) (Rt,B) (L,B) (L,T)      // 4 vertices, start top-right
      else 8 vertices, each with one zero-length tangent:
        v0=(Rt,T+R) in=(Rt,T+c)  out=0      v1=(Rt,B-R) in=0  out=(Rt,B-c)
        v2=(Rt-R,B) in=(Rt-c,B)  out=0      v3=(L+R,B)  in=0  out=(L+c,B)
        v4=(L,B-R)  in=(L,B-c)   out=0      v5=(L,T+R)  in=0  out=(L,T+c)
        v6=(L+R,T)  in=(L+c,T)   out=0      v7=(Rt-R,T) in=0  out=(Rt-c,T)
        (tangent points given absolute here; stored relative to the vertex)
    if direction == Reversed (counter-clockwise): same vertex set, start vertex is
      v0=(Rt,T+R) (R==0: (Rt,T)), then proceeds along the top edge leftwards:
      R==0: (Rt,T) (L,T) (L,B) (Rt,B)
    closed = true
    ```
    Each corner arc is one cubic segment with handle length `R*K`; straight edges are cubic segments with zero-length handles (degenerate to lines). When `R == hw` (or `hh`) the two vertices on that edge coincide (zero-length segment kept).
  - Edge cases: Size 0 in one axis → degenerate (zero-area) path, still strokable; Size (0,0) → all vertices coincide.
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "rc"`. The start vertex/direction is what Trim Paths and Offset/Round Corners act on, so it must match exactly for Lottie parity. UNVERIFIED: whether AE's own internal corner constant is 0.5519 (lottie-web) or 0.55228 (4/3·(√2−1)); difference < 0.07 % of R, visually negligible, but affects Trim Paths positions by a hair. BAE decision: use lottie-web's construction (0.5519) for both render and Lottie export so imports round-trip identically. Implementation hint: generate directly into kurbo `BezPath` (MoveTo, CurveTo…, ClosePath) and keep the vertex/tangent form only when needed (Convert to Bezier, Trim).

### AE-SHP-011 · Ellipse path (椭圆路径)

- **What it does:** Parametric axis-aligned ellipse, generating a closed 4-vertex Bezier path.
- **Exact semantics:**
  | Parameter (中文) | Type | Default (Add menu) | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shape Direction (反转路径方向) | enum | Normal | AE-SHP-014 | — | Lottie `d` |
  | Size (大小) | 2D vector, linkable | (100, 100) | ≥ 0 (UNVERIFIED) | px | full width/height; `ADBE Vector Ellipse Size`, Lottie `s` |
  | Position (位置) | 2D point | (0, 0) | any | px | centre; `ADBE Vector Ellipse Position`, Lottie `p` |
  - Generated path (lottie-web `EllShapeProperty`; `K = 0.5519`):
    ```
    rx = Size.x/2; ry = Size.y/2; (cx,cy) = Position
    Normal (clockwise, start at 12 o'clock):
      v0=(cx, cy-ry)  in=(-rx*K, 0)  out=(+rx*K, 0)
      v1=(cx+rx, cy)  in=(0, -ry*K)  out=(0, +ry*K)
      v2=(cx, cy+ry)  in=(+rx*K, 0)  out=(-rx*K, 0)
      v3=(cx-rx, cy)  in=(0, +ry*K)  out=(0, -ry*K)
    Reversed: v0 top, v1 = left (cx-rx,cy), v2 bottom, v3 right, tangents mirrored in x.
    closed = true      (tangents relative to the vertex)
    ```
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "el"`. UNVERIFIED: AE's internal kappa (0.5519 per lottie-web vs 0.55228475 = 4(√2−1)/3, which some third-party exporters assume); to settle it, Convert to Bezier Path on a 100 px circle in AE and read the tangent length: 27.595 means 0.5519, 27.614 means 0.55228 (the two differ by only 0.02 px here, so a rounded display of 27.6 does not decide it). Verified: lottie-web `common.js` defines `roundCorner = 0.5519` and uses it for both ellipses and rounded-rectangle corners. BAE decision: keep the constant in one place (`ELLIPSE_KAPPA`) so it can be switched after verification against AE.

### AE-SHP-012 · Polystar path (多边星形路径)

- **What it does:** Parametric regular star or polygon with per-vertex roundness, generating a closed Bezier path.
- **Exact semantics:**
  | Parameter (中文) | Type | Default (Add menu) | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shape Direction (反转路径方向) | enum | Normal | AE-SHP-014 | — | Lottie `d` |
  | Type (类型) | enum | Star | Star=1, Polygon=2 | — | `ADBE Vector Star Type`, Lottie `sy` |
  | Points (点) | scalar | 5 | 3–100 (UNVERIFIED) | count | `ADBE Vector Star Points`, Lottie `pt` |
  | Position (位置) | 2D point | (0, 0) | any | px | centre; Lottie `p` |
  | Rotation (旋转) | angle | 0 | any | degrees, clockwise | Lottie `r` |
  | Inner Radius (内径) | scalar | 50 | ≥ 0 | px | Star only (disabled for Polygon); Lottie `ir` |
  | Outer Radius (外径) | scalar | 100 | ≥ 0 | px | Lottie `or` |
  | Inner Roundness (内圆度) | scalar | 0 | UNVERIFIED (negative allowed) | % | Star only; Lottie `is` |
  | Outer Roundness (外圆度) | scalar | 0 | UNVERIFIED (negative allowed) | % | Lottie `os` |
  - Generated path (lottie-web `StarShapeProperty`):
    ```
    dir = (Shape Direction == Reversed) ? -1 : +1
    n   = floor(Points)                      // lottie-web; see Notes for AE fractional points
    a   = -90° + Rotation                    // first vertex straight up (12 o'clock) at Rotation 0
    if Type == Star:
        count = 2n; step = 360°/(2n)
        for i in 0..count:
            outer = (i % 2 == 0)
            r     = outer ? OuterRadius : InnerRadius
            rnd   = (outer ? OuterRoundness : InnerRoundness) / 100
            seg   = 2π·r / (4n)               // = (2π r)/(2·count)
            emit_vertex(r, a, rnd, seg); a += step*dir
    else Polygon:
        count = n; step = 360°/n
        for i in 0..count:
            seg = 2π·OuterRadius / (4n)
            emit_vertex(OuterRadius, a, OuterRoundness/100, seg); a += step*dir
    emit_vertex(r, a, rnd, seg):
        p = Position + r·(cos a, sin a)
        t = (sin a, -cos a)                    // unit vector, = (y,-x)/r; (0,0) if r == 0
        out_tangent = -t · seg·rnd·dir          // absolute tip = p + out_tangent
        in_tangent  = +t · seg·rnd·dir
    closed = true
    ```
    With roundness 100 % on a polygon the handle length is `2πr/(4n)` (one quarter of the arc per vertex), giving a near-circle; negative roundness flips handles (loops/cusps).
  - Edge cases: Inner Radius > Outer Radius is allowed (star inverts); radius 0 collapses vertices to Position with zero tangents.
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "sr"`. UNVERIFIED: AE accepts fractional Points and renders a partially grown extra point (smooth animation of Points); lottie-web and Skottie floor the value. BAE decision: implement AE behaviour once verified; until then floor (Lottie parity). Polygon tool and Star tool create Polystar with Type preset (AE-SHP-053); the Polygon tool's default is 5 points (UNVERIFIED tool-drawn radii: drag distance = Outer Radius, star Inner Radius = Outer/2).

### AE-SHP-013 · Bezier shape path (路径)

- **What it does:** Freeform Bezier path ("Path" attribute) whose geometry is stored directly as an animatable Path value (vertices + tangents + closed flag).
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shape Direction (反转路径方向) | enum | Normal | AE-SHP-014 | — | |
  | Path (路径) | Shape value (AE-SHP-041) | Add menu: empty path (0 vertices) | — | px, group space | `ADBE Vector Shape`; Lottie `ks` |
  - Attribute matchName `ADBE Vector Shape - Group` (display "Path <n>"), Lottie `ty: "sh"`.
  - Path coordinates are relative to the containing group's coordinate space (i.e. before the group transform), not the layer; Adobe notes that Bezier shape paths are relative to the local group anchor, so expression-driven points can look offset unless the group transform is identity.
  - Keyframe interpolation and vertex-count changes: AE-SHP-042. An empty Path draws nothing.
- **Priority:** P0
- **Owner:** bae-vector (+ bae-model, bae-anim)
- **Notes:** Created by the Pen tool (AE-SHP-055), by Alt-dragging a shape tool (Bezier option, AE-SHP-058), by Convert to Bezier Path (AE-SHP-015), by pasting a mask/Illustrator path (AE-SHP-046), by Create Shapes from Text/Vector Layer (AE-SHP-048/049). Path property expressions (`points()`, `createPath()`…) are in transform-keyframes-expressions.md, "path methods".

### AE-SHP-014 · Shape direction and Reverse Path Direction (反转路径方向)

- **What it does:** Per path attribute, flips the drawing direction of the generated path; affects Trim Paths, dashes/taper start, Non-Zero winding holes, Offset direction semantics, and Wiggle/Zig Zag phase.
- **Exact semantics:**
  - Shown in the Timeline as a toggle button on the path attribute row ("Reverse Path Direction On/Off", 反转路径方向（开）/（关）), scripting property `ADBE Vector Shape Direction` (enum).
  - Values: scripting UNVERIFIED (best understanding: 1 = Default, 2 = Clockwise, 3 = Counter-clockwise, with the toggle switching between 1 and 3); Lottie `d`: 1 = Normal, 3 = Reversed (lottie-web also treats 2 as normal).
  - Parametric shapes: Normal = clockwise on screen, Reversed = counter-clockwise, same start vertex (AE-SHP-010..012).
  - Bezier Path: Reversed renders the stored path traversed backwards: vertex order reversed and each vertex's in/out tangents swapped. UNVERIFIED: which vertex becomes the start of a reversed closed path; best understanding: vertex 0 stays first, followed by n−1, n−2, …, 1.
  - Reversing does not change the stored Path value.
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** "Reverse path direction" also exists on masks (compositing-3d-color.md, masks). Lottie exporters write `d` for every path; BAE must honour `d` on `sh` too (lottie-web ignores it for `sh`, UNVERIFIED).

### AE-SHP-015 · Convert to Bezier Path (转换为贝塞尔曲线路径)

- **What it does:** Replaces a parametric Rectangle/Ellipse/Polystar attribute with an equivalent Bezier Path attribute.
- **Exact semantics:**
  - Invoked from the context menu of the parametric path in the Timeline or Composition panel (Convert To Bezier Path).
  - Result: a Path attribute with the same name, inserted at the same position, whose Path value is the generated geometry (AE-SHP-010..012) at the current time, including Shape Direction applied (UNVERIFIED: whether direction is baked into vertex order or carried on the new attribute's Shape Direction).
  - Keyframes/expressions on the parametric properties are not converted to path keyframes (UNVERIFIED; best understanding: the conversion is static at the current time and animation is lost).
  - Adobe notes that conversion can adjust the shape group's Position (UNVERIFIED how: best understanding — the primitive's own Position offset is folded into vertex coordinates, which may look like a position change to expression rigs).
  - Undoable single command.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-vector, bae-ui-motion)
- **Notes:** Needed before Create Nulls from Paths (AE-SHP-050) or vertex editing. The geometry must be the exact generator output so that Trim Paths gives the same result before and after conversion.

## Fills, strokes and gradients

### AE-SHP-016 · Fill (填充)

- **What it does:** Paints the interior of all paths above it in the same group (and nested groups above it) with a solid colour.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Blend Mode (混合模式) | enum | Normal | AE-SHP-005 | — | `ADBE Vector Blend Mode`, Lottie `bm` |
  | Composite (合成) | enum | Below Previous in Same Group | AE-SHP-004 | — | `ADBE Vector Composite Order` |
  | Fill Rule (填充规则) | enum | Non-Zero Winding | AE-SHP-017 | — | `ADBE Vector Fill Rule`, Lottie `r` |
  | Color (颜色) | colour RGB | Tools panel fill colour (AE-SHP-057) | each channel 0–1 | — | `ADBE Vector Fill Color`, Lottie `c` (RGB 0–1; a 4th component may be present and is ignored) |
  | Opacity (不透明度) | scalar | 100 | 0–100 | % | `ADBE Vector Fill Opacity`, Lottie `o` |
  - Rendering: the fill's geometry (AE-SHP-003) is filled as one compound path in the fill's group coordinate space, then transformed by enclosing group transforms and the layer transform; coverage is anti-aliased (layer Quality Best) or aliased (Draft; UNVERIFIED for shape layers).
  - Effective alpha = Opacity/100 × enclosing group opacities (exactly so for non-overlapping draws; overlap inside a faded group: AE-SHP-030).
  - Colour values are clipped to [0, 1] even in 32 bpc projects (Adobe: "the colors of strokes and fills for shape layers are not rendered as high-dynamic range colors"; values under 0.0 or over 1.0 are clipped) — this applies to solid and gradient fills/strokes, including values produced by expressions.
  - Open paths are filled as if closed by a straight segment from last to first vertex.
- **Priority:** P0
- **Owner:** bae-vector (+ bae-color)
- **Notes:** Lottie `ty: "fl"`. Colour is in the project working space (compositing-3d-color.md, color management); BAE must apply the same input-to-working conversion as AE for shape colours. Expressions on Color return [r,g,b,a] (transform-keyframes-expressions.md).

### AE-SHP-017 · Fill Rule (填充规则)

- **What it does:** Selects how "inside" is determined for self-intersecting or compound paths for Fill and Gradient Fill.
- **Exact semantics:**
  - Enum (menu order, Lottie `r`): 1 = Non-Zero Winding (非零环绕), default; 2 = Even-Odd (奇偶).
  - Non-zero: point inside iff the signed crossing count of a ray (left-to-right crossings minus right-to-left crossings) ≠ 0. Even-odd: inside iff crossings are odd.
  - Because Non-Zero depends on direction, reversing one sub-path (AE-SHP-014) in a compound path creates a hole.
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Strokes have no fill rule. Merge Paths "Merge" mode output relies on the fill rule of the paint below it (AE-SHP-031). Vello supports both (`Fill::NonZero`, `Fill::EvenOdd`).

### AE-SHP-018 · Stroke (描边)

- **What it does:** Paints a band of the given width centred on all paths above it in the same group (and nested groups above it).
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Blend Mode (混合模式) | enum | Normal | AE-SHP-005 | — | |
  | Composite (合成) | enum | Below Previous in Same Group | AE-SHP-004 | — | |
  | Color (颜色) | colour | Tools panel stroke colour (AE-SHP-057) | 0–1 | — | `ADBE Vector Stroke Color`, Lottie `c` |
  | Opacity (不透明度) | scalar | 100 | 0–100 | % | `ADBE Vector Stroke Opacity`, Lottie `o` |
  | Stroke Width (描边宽度) | scalar | Tools panel width (initially 2, UNVERIFIED) | ≥ 0 (UNVERIFIED upper limit) | px | `ADBE Vector Stroke Width`, Lottie `w` |
  | Line Cap (线段端点) | enum | Butt Cap | AE-SHP-019 | — | Lottie `lc` |
  | Line Join (线段连接) | enum | Miter Join | AE-SHP-019 | — | Lottie `lj` |
  | Miter Limit (尖角限制) | scalar | 4 | ≥ 1 (UNVERIFIED max) | × width | Lottie `ml` (static) / `ml2` (animated) |
  | Dashes (虚线) | group | empty | AE-SHP-020 | — | Lottie `d[]` |
  | Taper (锥度（译名待核；帮助页作“椎体化”）) | group | no taper (lengths 0) | AE-SHP-021 | — | AE 17.1+ |
  | Wave (波形) | group | no wave (Amount 0) | AE-SHP-022 | — | AE 17.1+ |
  - The stroke outline is computed in the stroke's own group space with the given width and then transformed by enclosing transforms (so group/layer scale scales the stroke width, non-uniform scale makes it anisotropic). A path's own group transform scales the path but not a stroke that sits outside that group.
  - Open paths get caps at both ends; closed paths get a join at the start vertex and no caps. Each sub-path of a compound path is stroked independently (no joins between sub-paths).
  - Stroke Width 0 → nothing drawn. Overlapping parts of one stroke do not double-cover (one coverage mask per stroke).
  - Effective alpha and colour clamping as for Fill (AE-SHP-016).
- **Priority:** P0
- **Owner:** bae-vector (+ bae-color)
- **Notes:** Lottie `ty: "st"`. Implementation hint: kurbo `stroke()` produces an outline path that Vello fills, or use Vello's native stroking; for dashes/taper/wave BAE must build its own outline (Vello strokes have constant width).

### AE-SHP-019 · Line Cap, Line Join and Miter Limit (线段端点、线段连接、尖角限制)

- **What it does:** Shape of stroke ends and corners.
- **Exact semantics:**
  - Line Cap enum (menu order; scripting value = Lottie `lc`): 1 Butt Cap (平头端点), 2 Round Cap (圆头端点), 3 Projecting Cap (矩形端点). Butt ends exactly at the path end; Round adds a semicircle of radius width/2; Projecting extends a square by width/2 beyond the end. Caps apply to open-path ends and to each dash end (AE-SHP-020). (Adobe's help text says round/projecting caps extend "a number of pixels equal to the stroke width"; that wording is loose — the cap extends by half the stroke width on the path's end, i.e. the cap is a semicircle / half-square of the stroke's own width, as in SVG/PostScript stroking and lottie-web's mapping of `lc` to SVG `stroke-linecap`.)
  - Line Join enum (`lj`): 1 Miter Join (斜接连接), 2 Round Join (圆角连接), 3 Bevel Join (斜面连接). Joins apply only at segment boundaries (vertices) where the incoming and outgoing directions differ, including the closing vertex of closed paths; curve interiors are offset smoothly with no join.
  - Miter Limit (`ml`): a miter join whose miter length / stroke width exceeds the limit is drawn as a bevel (Adobe: "if the miter limit is 4, then when the length of the point reaches four times the stroke weight, a bevel join is used"; "a miter limit of 1 causes a bevel join"). Miter length ratio = 1/sin(θ/2) for interior angle θ. Default 4.
  - zh-CN names are from the zh-CN help page (shape-attributes-paint-operations-path).
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `lc`: 1 butt, 2 round, 3 square; `lj`: 1 miter, 2 round, 3 bevel. Offset Paths has its own Line Join/Miter Limit (AE-SHP-032).

### AE-SHP-020 · Stroke dashes (虚线)

- **What it does:** Turns a stroke into a repeating pattern of up to three dash/gap pairs with an animatable offset.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Dash (虚线) | scalar | UNVERIFIED (10) | ≥ 0 | px | `ADBE Vector Stroke Dash 1`, Lottie `{n:"d"}` |
  | Gap (间隙) | scalar | UNVERIFIED (= Dash when added) | ≥ 0 | px | `ADBE Vector Stroke Gap 1`, Lottie `{n:"g"}` |
  | Dash 2 / Gap 2 / Dash 3 / Gap 3 | scalar | — | ≥ 0 | px | `… Dash 2`, `… Gap 2`, `… Dash 3`, `… Gap 3` |
  | Offset (偏移) | scalar | 0 | any | px | `ADBE Vector Stroke Offset`, Lottie `{n:"o"}` |
  - UI: the Dashes group has "+" (Add a Dash or Gap, 添加虚线或间隙) and "−" buttons; each "+" adds the next element in the sequence Dash(+Offset on first click), Gap, Dash 2, Gap 2, Dash 3, Gap 3; maximum 3 dashes and 3 gaps. "−" removes the last one.
  - Pattern = [Dash, Gap, Dash 2, Gap 2, …] in the order present; if the pattern has an odd number of entries it is repeated twice to make it even (SVG semantics; UNVERIFIED for AE, best understanding: missing Gap behaves as Gap = Dash).
  - Distance is measured along the path's arc length in the stroke's group space, starting at vertex 0 of each sub-path (each sub-path restarts the pattern; UNVERIFIED — lottie-web/canvas restart per sub-path).
    ```
    phase(s) = (s + Offset) mod patternLength        // SVG stroke-dashoffset semantics (lottie-web)
    // a point at arc length s is "on" if phase(s) falls in a dash interval
    ```
    So increasing Offset moves dashes toward the path start (against path direction). UNVERIFIED for AE's visual direction (lottie-web passes Offset to stroke-dashoffset unchanged).
  - Each dash is an independent open sub-stroke with the stroke's Line Cap; a zero-length dash with Round or Projecting cap still draws a dot/square (used for dotted lines).
  - Closed paths: the pattern runs from vertex 0 around and is cut at the end (no wrap-around continuity unless the length is an exact multiple).
  - All-zero pattern → stroke drawn solid (UNVERIFIED; SVG treats an all-zero dasharray as solid).
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie stroke `d: [{n,nm,v}]`. Gradient Stroke has the same Dashes group. Dashes are computed after Trim Paths (trim is a path operator, dashing is part of paint), so trimming does not shift the dash phase relative to the original start? — UNVERIFIED: in AE the dash pattern is anchored to the start of the trimmed path (dashes "crawl" while trimming); Skottie applies dashing to the already-trimmed geometry (`AdjustStrokeGeometry`), which implies anchoring at the trimmed start.

### AE-SHP-021 · Stroke taper (锥度)

- **What it does:** Varies stroke width along the path so the stroke narrows toward its start and/or end, with easing.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Length Units (长度单位) | enum | UNVERIFIED (best understanding: Percentage) | Percentage (百分比) / Pixels (像素) (help-page labels; UI may read "Percent") | — | `ADBE Vector Taper Length Units` |
  | Start Length (起始长度) | scalar | 0 | 0–100 % or ≥ 0 px | % of path length or px | `ADBE Vector Taper Start Length` |
  | End Length (末端长度（译名待核）) | scalar | 0 | same | same | `ADBE Vector Taper End Length` |
  | Start Width (起始宽度) | scalar | 0 (UNVERIFIED) | 0–100 (UNVERIFIED) | % of Stroke Width | `ADBE Vector Taper Start Width` |
  | End Width (末端宽度（译名待核）) | scalar | 0 (UNVERIFIED) | 0–100 (UNVERIFIED) | % of Stroke Width | `ADBE Vector Taper End Width` |
  | Start Ease (起始缓动) | scalar | 0 | UNVERIFIED (−100–100) | % | negative = pointy, 0 = linear, positive = rounder |
  | End Ease (末端缓动（译名待核）) | scalar | 0 | same | % | |
  - Width profile along arc length s of each open path (total length Ltot), UNVERIFIED reconstruction (Adobe documents only the qualitative behaviour):
    ```
    Ls = StartLength (Percent: StartLength/100*Ltot);  Le likewise
    if Ls+Le > Ltot: scale both by Ltot/(Ls+Le)        // UNVERIFIED overlap handling
    f(s) = 1
    if s < Ls:          u = s/Ls;          f = lerp(StartWidth/100, 1, easeCurve(u, StartEase))
    if s > Ltot - Le:   u = (Ltot-s)/Le;   f = lerp(EndWidth/100, 1, easeCurve(u, EndEase))
    width(s) = StrokeWidth * f(s) * waveFactor(s)        // AE-SHP-021 × AE-SHP-022
    easeCurve(u, 0) = u;  ease > 0 bows outward (rounder), ease < 0 bows inward (pointier)
    ```
  - Pixels units keep taper length constant while the path length changes (e.g. with Trim Paths); Percent is relative to the (trimmed) path length.
  - Caps are applied at the tapered ends (a taper to 0 % with Round Cap ends in a point). Closed paths: taper starts/ends at vertex 0 (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** Introduced in AE 17.1 (May 2020) as "tapered shape strokes" (public beta from April 2020: Adobe community "Feature Focus: Tapered Shape Strokes"). Also present on Gradient Stroke (UNVERIFIED). Adobe: tapered strokes cannot be imported from Illustrator. Source: https://helpx.adobe.com/after-effects/using/taper-shape-strokes.html Not representable in Lottie/bodymovin (taper is dropped on export); Illustrator variable-width strokes are not imported as taper. Implementation hint: build the outline by offsetting the path's arc-length-parameterised normal by ±width(s)/2 at adaptive samples, then fill; cache per (path hash, params).

### AE-SHP-022 · Stroke wave (波形)

- **What it does:** Modulates the stroke width periodically along the path, producing wavy or bead-like strokes.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Amount (数量) | scalar | 0 | 0–100 (UNVERIFIED) | % | `ADBE Vector Taper Wave Amount` |
  | Units (单位) | enum | Pixels (UNVERIFIED) | Pixels (像素) / Cycles (周期) | — | `ADBE Vector Taper Wave Units` |
  | Wavelength (波长) / Cycles (周期) | scalar | UNVERIFIED | > 0 | px, or count per path | label switches with Units; `ADBE Vector Taper Wavelength` |
  | Phase (相位) | angle | 0 | any | degrees (UNVERIFIED) | `ADBE Vector Taper Wave Phase` |
  - Width factor, UNVERIFIED reconstruction:
    ```
    λ = (Units == Pixels) ? Wavelength : Ltot / Cycles
    waveFactor(s) = 1 - (Amount/100) * (0.5 - 0.5*cos(2π*s/λ + Phase))
    ```
    Cycles makes an integer number of waves fit a closed path seamlessly; Phase animates the wave along the path (Adobe example: Phase 0→360 over 4 s gives one full travel per 4 s, implying degrees).
- **Priority:** P2
- **Owner:** bae-vector
- **Notes:** Introduced together with Taper in AE 17.1 (May 2020). Adobe: with Units = Cycles the Wavelength property is relabelled Cycles and a value of 1 gives a single wave; Cycles suits closed paths (seamless), Pixels suits open paths. Not exported to Lottie. Combines multiplicatively with Taper (UNVERIFIED).

### AE-SHP-023 · Gradient Fill (渐变填充)

- **What it does:** Fill whose colour/opacity is a linear or radial gradient mapped through Start/End points in the fill's group space.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Blend Mode (混合模式) | enum | Normal | — | — | |
  | Composite (合成) | enum | Below Previous in Same Group | — | — | |
  | Fill Rule (填充规则) | enum | Non-Zero Winding | — | — | Lottie `r` |
  | Type (类型) | enum | Linear | 1 Linear (线性), 2 Radial (径向) | — | `ADBE Vector Grad Type`, Lottie `t` |
  | Start Point (起始点) | 2D point | (0,0) (UNVERIFIED) | any | px | `ADBE Vector Grad Start Pt`, Lottie `s` |
  | End Point (结束点) | 2D point | (100,0) (UNVERIFIED) | any | px | `ADBE Vector Grad End Pt`, Lottie `e` |
  | Highlight Length (高光长度) | scalar | 0 | −100–100 (UNVERIFIED) | % | Radial only; Lottie `h` |
  | Highlight Angle (高光角度) | angle | 0 | any | degrees | Radial only; Lottie `a` |
  | Colors (颜色) | gradient | white→black (UNVERIFIED) | — | — | `ADBE Vector Grad Colors`, Lottie `g`; AE-SHP-025 |
  | Opacity (不透明度) | scalar | 100 | 0–100 | % | `ADBE Vector Fill Opacity` |
  - Geometry and stacking exactly as Fill (AE-SHP-016); colour per pixel from AE-SHP-027/028 evaluated in the fill's group space (gradient moves/scales with group transforms).
  - Shape tools: when a shape is drawn with gradient fill, Start/End are placed relative to the drawn shape (UNVERIFIED: start at shape centre, end at its right edge); with the Pen tool both points start at the layer centre (Adobe).
- **Priority:** P0
- **Owner:** bae-vector (+ bae-color)
- **Notes:** Lottie `ty: "gf"`. Gradient colours are clamped to [0,1] like solid colours. The Colors property cannot be read or set by expressions or scripting in AE (no value type; UNVERIFIED for AE 2024+). Lottie also defines Type 3 Conic, which AE does not produce.

### AE-SHP-024 · Gradient Stroke (渐变描边)

- **What it does:** Stroke whose paint is a linear or radial gradient.
- **Exact semantics:**
  - Properties (AE order): Blend Mode, Composite, Type, Start Point, End Point, Highlight Length, Highlight Angle, Colors, Opacity, Stroke Width, Line Cap, Line Join, Miter Limit, Dashes (+ Taper, Wave, UNVERIFIED). Defaults/ranges as Stroke (AE-SHP-018) and Gradient Fill (AE-SHP-023).
  - The gradient is evaluated in group space over the stroke outline, i.e. it is not distributed along the path length (no "gradient along stroke" mode in AE).
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "gs"` with stroke keys `w`, `lc`, `lj`, `ml`, `d` plus gradient keys `t`, `s`, `e`, `h`, `a`, `g`.

### AE-SHP-025 · Gradient colour data and storage (渐变颜色数据)

- **What it does:** Defines the gradient value: colour stops, opacity stops and midpoints, and how it is keyframed and serialised.
- **Exact semantics:**
  - Value = ordered colour stops `(location 0–1, RGB)` + ordered opacity stops `(location 0–1, alpha 0–1)`; each adjacent pair of stops has a midpoint (location between them, default 50 %). Minimum 1 colour stop and 1 opacity stop (UNVERIFIED; editor requires ≥ 1… best understanding ≥ 2 is not required).
  - Colour and opacity ramps are independent; RGBA at t = (colourRamp(t), opacityRamp(t)).
  - Keyframing: the whole gradient is one animatable property; interpolation is per stop (location, colour, alpha) between keyframes with the same stop counts. UNVERIFIED: behaviour when stop counts differ (best understanding: AE inserts/removes stops by resampling, otherwise holds).
  - Lottie storage `g: {p: N, k: {a, k}}` where the flat array is `[loc0,r0,g0,b0, loc1,r1,g1,b1, … (N colour stops), oloc0,a0, oloc1,a1, …]`, all 0–1. Midpoints are not stored; bodymovin bakes them (UNVERIFIED: by inserting extra stops). lottie-web treats opacity stops as absent when array length = 4N.
- **Priority:** P0
- **Owner:** bae-model (+ bae-vector)
- **Notes:** In AE's project file the gradient is stored as an XML-like "GradientColorData" string with explicit midpoints (UNVERIFIED). BAE decision: store stops and midpoints natively (value type `Gradient`), export to Lottie by baking midpoints into extra stops (e.g. 8 samples per segment when midpoint ≠ 50 %).

### AE-SHP-026 · Gradient Editor (渐变编辑器)

- **What it does:** Dialog for editing gradient colour/opacity stops and midpoints, opened from the Colors property ("Edit Gradient…") or the Tools panel swatch.
- **Exact semantics:**
  - Controls: type buttons (None, Solid, Linear, Radial) when opened from the Tools panel; a ramp with opacity stops above and colour stops below; midpoint diamonds between stops; fields for selected stop Location (%), Opacity (%) or Color; Delete stop; colour picker (HSB/RGB/hex); OK/Cancel.
  - Click below/above the ramp adds a stop interpolated at that location; drag a stop off the ramp to delete it (UNVERIFIED); drag a midpoint to change its location (range limited between neighbouring stops; UNVERIFIED 5–95 %).
  - Edits apply live to the property (one undo step on OK). With keyframes and the CTI on a keyframe the edit changes that keyframe; otherwise it adds a keyframe if the stopwatch is on.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** UI layout and widgets: ui-preview-render-io.md (panels). Gradients can be saved in animation presets.

### AE-SHP-027 · Gradient evaluation: linear (线性渐变求值)

- **What it does:** Maps each pixel to a position on the gradient for Linear type.
- **Exact semantics:**
  ```
  // p in the paint's group space; S = Start Point, E = End Point
  d = E - S
  t = dot(p - S, d) / dot(d, d)          // S==E: t = 0 for all p (UNVERIFIED: AE draws last stop?)
  t = clamp(t, 0, 1)                     // pad: colours beyond the ends extend flat
  rgb   = colourRamp(t); alpha = opacityRamp(t)
  // ramp between stops i,i+1 with midpoint m (0..1 relative inside the segment):
  u = (t - loc_i) / (loc_{i+1} - loc_i)
  u' = (m == 0.5) ? u : pow(u, log(0.5)/log(m))     // UNVERIFIED midpoint curve (Photoshop-style)
  value = lerp(v_i, v_{i+1}, u')
  ```
  - Interpolation is component-wise in the project working colour space values (gamma-encoded unless the project is linearized; UNVERIFIED for 32 bpc linear).
  - Before the first stop / after the last stop the end values extend.
- **Priority:** P0
- **Owner:** bae-vector (+ bae-color)
- **Notes:** lottie-web builds an SVG/Canvas linear gradient with the stops (no midpoint support; pad spread). Implementation hint: Vello `Gradient::new_linear` with stops; midpoints baked as extra stops; for 8 bpc banding, AE does not dither shape gradients (UNVERIFIED).

### AE-SHP-028 · Gradient evaluation: radial and highlight (径向渐变与高光)

- **What it does:** Maps pixels for Radial type, with an optional off-centre focal point set by Highlight Length/Angle.
- **Exact semantics:**
  ```
  r     = |E - S|                         // outer circle centred at S through E, t = 1 on it
  base  = atan2(E.y - S.y, E.x - S.x)      // direction from start to end
  h     = clamp(HighlightLength/100, -0.99, 0.99)   // lottie-web clamp
  F     = S + h*r*(cos(base + HighlightAngle°), sin(base + HighlightAngle°))  // focal point, t = 0
  // two-point conical gradient: focal circle (F, 0) -> outer circle (S, r)
  t(p)  = smallest λ >= 0 such that |p - lerp(F, S, λ)| = λ*r ; clamp(t, 0, 1)
  ```
  With Highlight Length 0 this reduces to `t = |p − S| / r`. Pixels beyond the outer circle use the last stop (pad).
  - Highlight Angle 0 places the focal point on the S→E line toward E; positive angles rotate it clockwise.
- **Priority:** P0 (plain radial) / P1 (highlight)
- **Owner:** bae-vector
- **Notes:** Lottie `h` is percent (lottie-web multiplies by 0.01), `a` in degrees. lottie-web canvas uses `createRadialGradient(F, 0, S, r)`, SVG uses `fx/fy`. UNVERIFIED: AE clamps highlight to ±100 % and how AE renders |h| → 100 % (lottie-web clamps to 0.99 to avoid the degenerate cone). Vello `Gradient::new_two_point_radial(F, 0, S, r)`.

## Group transform

### AE-SHP-029 · Group Transform (变换)

- **What it does:** Each group's Transform property group positions, scales, skews and rotates everything in the group relative to the parent group.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Anchor Point (锚点) | 2D point | (0,0) | any | px | `ADBE Vector Anchor`, Lottie `a` |
  | Position (位置) | 2D point | (0,0) | any | px | `ADBE Vector Position`, Lottie `p` |
  | Scale (缩放) | 2D % (linked) | (100,100) | any (negative flips) | % | `ADBE Vector Scale`, Lottie `s` |
  | Skew (倾斜) | scalar | 0 | −85–85 (UNVERIFIED) | degrees | `ADBE Vector Skew`, Lottie `sk` |
  | Skew Axis (倾斜轴) | angle | 0 | any | degrees | `ADBE Vector Skew Axis`, Lottie `sa` |
  | Rotation (旋转) | angle | 0 | any | degrees, clockwise | `ADBE Vector Rotation`, Lottie `r` |
  | Opacity (不透明度) | scalar | 100 | 0–100 | % | `ADBE Vector Group Opacity`, Lottie `o`; AE-SHP-030 |
  - Matrix (column vectors, y-down; applied to group-local points; lottie-web `TransformProperty` order):
    ```
    M = T(Position) · R(Rotation) · K(Skew, SkewAxis) · S(Scale/100) · T(-AnchorPoint)
    R(θ) = [[cos θ, -sin θ], [sin θ, cos θ]]                // θ>0 clockwise on screen
    K(sk, ax) = R(-ax) · [[1, tan(-sk)], [0, 1]] · R(ax)    // shear along the skew axis
    S(sx, sy) = diag(sx, sy)
    ```
    (lottie-web: `translate(-a) → scale → skewFromAxis(-sk, sa) → rotate(-r) → translate(p)` in its row-vector convention.) Skew 0 → K = identity; |Skew| → 90° is singular.
  - The group matrix composes with ancestors: `world = layerMatrix · M_root… · M_parentGroup · M_group`.
  - The Repeater's Transform and the Wiggle Transform's Transform use the same parameters minus Skew (AE-SHP-034, AE-SHP-039).
- **Priority:** P0
- **Owner:** bae-vector (+ bae-anim)
- **Notes:** Shape group transforms are 2D even on 3D layers. Timeline label is "Transform: <group name>". Interactive editing via the group's dashed bounding box in the viewer: AE-SHP-059. UNVERIFIED: the visual direction of positive Skew (lottie-web: positive Skew with Axis 0 shears the top of the shape to the right).

### AE-SHP-030 · Group opacity (组不透明度)

- **What it does:** Scales the opacity of everything inside the group.
- **Exact semantics:**
  - For a single paint (no overlap with other content of the group), effective opacity = paint Opacity/100 × Π(enclosing group Opacity/100), then × layer opacity at layer composite time. Where draws inside a faded group overlap, the result depends on isolation (next bullet).
  - UNVERIFIED (important for parity): whether AE applies group opacity per paint (overlapping fills inside a 50 % group show their overlap) or to the group as an isolated unit (the group is composited first, then faded; no visible overlap). Reference players disagree: lottie-web's canvas renderer multiplies group opacity into each paint (`coOp = o * groupTransform.opacity`), while lottie-web's SVG renderer sets `opacity` on the group's `<g>` (isolated) and Skottie also isolates. Best understanding: isolated, consistent with Adobe's statement that shape-layer rendering follows the rules of nested compositions and with AE-SHP-005.
  - Repeater Start/End Opacity multiply in the same way per copy (AE-SHP-034).
- **Priority:** P0
- **Owner:** bae-vector (+ bae-render)
- **Notes:** BAE decision: render a group with opacity < 100 % as an isolated layer (Vello `push_layer` with alpha); when the group produces a single draw with Normal blending, multiply the opacity into that paint instead (identical result, no offscreen). Switch to per-paint multiplication only if an AE render shows overlap inside a faded group.

## Path operations

### AE-SHP-031 · Merge Paths (合并路径)

- **What it does:** Combines all paths above it in the same group (including nested groups) into one output path, either as a plain compound path or via a Boolean operation.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Mode (模式) | enum | Add (UNVERIFIED) | 1–5 | — | `ADBE Vector Merge Type`, Lottie `mm` |
  - Enum (menu order = scripting value = Lottie `mm`): 1 Merge (合并), 2 Add (相加), 3 Subtract (相减), 4 Intersect (相交), 5 Exclude Intersections (排除交集).
    - Merge: concatenate all input sub-paths, unchanged, into one compound path (no geometry change; the fill rule of the paint below decides holes). Adobe: this is the mode used for characters made of several paths (e.g. "e") by Create Shapes from Text.
    - Add: union of the areas of all inputs.
    - Subtract: area of the **topmost** input minus the union of all inputs below it.
    - Intersect: area common to all inputs (iterated pairwise in stacking order).
    - Exclude Intersections: XOR, iterated pairwise in stacking order (UNVERIFIED for ≥ 3 inputs: area covered an odd number of times).
  - Inputs are taken in Timeline order (top first), after their own group transforms; each input's area is its non-zero interior (UNVERIFIED: AE uses the input path's winding; open paths are implicitly closed).
  - Output: one compound path placed in the operator's group at the operator's position; downstream paints/operators see only this path.
  - Paint suppression: every paint above the Merge Paths in the same group (and inside nested groups above it) is not rendered (AE-SHP-003). When added from the Add menu, a Fill and a Stroke are inserted after it if none exist below it.
  - Boolean output for curves: the result is again cubic Bezier (curve segments are split at intersections, not flattened). UNVERIFIED: AE's exact vertex placement/ordering of the result (affects Trim Paths after a Merge).
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "mm"`; lottie-web does **not** implement Merge Paths (renders inputs as-is, Merge-mode equivalent); Skottie implements it with Skia PathOps and suppresses paints above it. Implementation hint: kurbo has no robust Boolean ops yet; candidates are `linesweeper` or a flattened-polygon clipper with curve refitting — choose one that keeps curves and is deterministic; cache results by input hash because Booleans are expensive.

### AE-SHP-032 · Offset Paths (位移路径)

- **What it does:** Offsets the input paths by a distance (expanding/contracting closed shapes), optionally producing concentric copies.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Amount (数量) | scalar | 10 (UNVERIFIED) | any | px | `ADBE Vector Offset Amount`, Lottie `a` |
  | Line Join (线段连接) | enum | Miter Join | 1 Miter, 2 Round, 3 Bevel | — | `ADBE Vector Offset Line Join`, Lottie `lj` |
  | Miter Limit (尖角限制) | scalar | 4 | ≥ 1 | × amount | `ADBE Vector Offset Miter Limit`, Lottie `ml` |
  | Copies (副本) | scalar | 1 | ≥ 1 (UNVERIFIED upper limit; fractional?) | count | `ADBE Vector Offset Copies`; not in Lottie |
  | Copy Offset (副本偏移) | scalar | 0 | any | copies | `ADBE Vector Offset Copy Offset`; not in Lottie |
  - For each input sub-path, the output is the curve offset by `d` along its normal: on closed paths positive `d` expands the area and negative contracts it (independent of path direction per Adobe; UNVERIFIED for counter-clockwise paths — lottie-web offsets along the direction-dependent normal). Open paths: the result is an outline around the path (forward offset then backward offset joined at the ends) — UNVERIFIED (lottie-web behaviour).
  - Corners between offset segments use Line Join; a miter longer than Miter Limit (× |Amount|) becomes a bevel; round joins are circular arcs (K = 0.5519 handles).
  - Self-intersections created by contraction are pruned (lottie-web `pruneIntersections`); UNVERIFIED how fully AE cleans loops on strongly concave shapes.
  - Copies (default 1, Adobe-documented; added in AE 17.1, May 2020, as the "concentric shape repeater"): output contains `ceil(Copies)` concentric paths. UNVERIFIED formula, best understanding:
    ```
    for i in 0 .. ceil(Copies)-1:
        d_i = Amount * (i + 1 + CopyOffset)      // Copy Offset shifts all copies by whole/fractional steps
        emit offset(path, d_i)
    ```
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "op"` (`a`, `lj`, `ml`); lottie-web compares the miter distance to `ml` in pixels (deviation). Curves are split at inflection points before offsetting (lottie-web), then each piece approximated by one offset cubic. Implementation hint: kurbo `offset::CubicOffset` + fit gives accurate offsets; cache per frame by input hash.

### AE-SHP-033 · Pucker & Bloat (收缩和膨胀)

- **What it does:** Moves vertices toward/away from the path centre while moving tangent handles the opposite way, giving star-like (pucker) or flower-like (bloat) shapes.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Amount (数量) | scalar | 0 | −100–100 (UNVERIFIED; slider range) | % | `ADBE Vector PuckerBloat Amount`, Lottie `a` |
  - Per sub-path (lottie-web `PuckerAndBloatModifier`):
    ```
    p = Amount/100
    C = average of all vertex positions of this sub-path   // not the bbox centre
    for each vertex:  v' = v + (C - v)*p
                      outTip' = outTip + (C - outTip)*(-p)  // tips = absolute handle points
                      inTip'  = inTip  + (C - inTip )*(-p)
    ```
    Positive Amount = Bloat (vertices in, handles out); negative = Pucker (vertices out, handles in). Vertices with zero-length handles get handles after the operation (handle tip = vertex position moved by −p).
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "pb"`.

### AE-SHP-034 · Repeater (中继器)

- **What it does:** Creates virtual copies of everything above it in the same group (paths, paints, nested groups), each transformed incrementally, with interpolated opacity.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Copies (副本) | scalar | 3 | ≥ 0 (UNVERIFIED max) | count | `ADBE Vector Repeater Copies`, Lottie `c` |
  | Offset (位移) | scalar | 0 | any | copies | `ADBE Vector Repeater Offset`, Lottie `o` |
  | Composite (合成) | enum | Below | 1 Below (之下（译名待核）), 2 Above (之上（译名待核）) | — | `ADBE Vector Repeater Order`, Lottie `m` (Lottie docs: 1 = Below, 2 = Above) |
  | Transform: Repeater (变换: 中继器) › Anchor Point (锚点) | 2D point | (0,0) | any | px | Lottie `tr.a` |
  | › Position (位置) | 2D point | (100,0) | any | px | Lottie `tr.p` |
  | › Scale (缩放) | 2D % | (100,100) | any | % | Lottie `tr.s` |
  | › Rotation (旋转) | angle | 0 | any | degrees | Lottie `tr.r` |
  | › Start Opacity (起始点不透明度) | scalar | 100 | 0–100 | % | Lottie `tr.so` |
  | › End Opacity (结束点不透明度) | scalar | 100 | 0–100 | % | Lottie `tr.eo` |
  - Copy `i` (i = 0 … N−1, N = ceil(Copies); copy 0 is the "original") is drawn with the transform applied `n = i + Offset` times (Adobe: copy n gets each transformation n times; Offset 3 with 10 copies → original transformed 3×, last 12×). Per-step accumulation (lottie-web `applyTransforms`), with fractional steps for fractional Offset:
    ```
    // A = anchor, P = position, s = scale/100 (per axis), r = rotation
    step(k) for real k:  R_k = rotate about A by k*r
                          S_k = scale about A by (s^floor(k)) * lerp(1, s, frac(k))   // per axis
                          P_k = translate by k*P
    copy_matrix(n) = P_n · S_n · R_n      // column-vector order: rotate first, then scale, then translate
    // negative n: inverse steps (rotate by n*r, scale by 1/s per step, translate by n*P)
    opacity_i = (N == 1) ? StartOp : StartOp + (EndOp - StartOp) * i/(N-1)   // uses copy index i, not n
    ```
    UNVERIFIED: AE's per-copy composition order for non-uniform scale combined with rotation (lottie-web applies rotation before scale about the anchor, unlike a group Transform which scales first).
  - Fractional Copies: UNVERIFIED — best understanding: AE draws `ceil(Copies)` copies and multiplies the last copy's opacity by `frac(Copies)` so animating Copies fades copies in; lottie-web draws `ceil(Copies)` at full opacity. Copies ≤ 0 draws nothing (not even the original).
  - Composite: Below (default) = each subsequent copy is drawn underneath the previous one (copy 0 in front); Above = each subsequent copy on top.
  - Placement semantics (Adobe): if the Repeater is above a fill/stroke (paths → Repeater → paints), the copies are geometry: the paint below draws all copies as one compound path (one gradient across all, overlaps not double-painted). If paints are above the Repeater, each copy is painted individually (gradients repeat per copy). Both happen if paints exist on both sides. Multiple Repeaters stack (grid = two Repeaters).
  - The Repeater always goes to the end of the group when added from the Add menu (AE-SHP-006). Operators below the Repeater (e.g. Wiggle Transform) see each copy separately (AE-SHP-039).
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "rp"`; lottie-web implements it by cloning the items above into hidden groups; Skottie treats it as a draw effect on paints above it. Match names of the Transform children are `ADBE Vector Repeater Transform` › `ADBE Vector Repeater Anchor`, `… Position`, `… Scale`, `… Rotation`, `… Opacity 1` (Start Opacity), `… Opacity 2` (End Opacity) (confirmed in third-party importer tables; not listed on docsforadobe). Implementation hint: never materialise copies as model data; emit N draw instances with per-instance matrices (instanced drawing for large N).

### AE-SHP-035 · Round Corners (圆角)

- **What it does:** Replaces sharp corner vertices of the input paths with circular-looking arcs of the given radius.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Radius (半径) | scalar | 10 | ≥ 0 | px | `ADBE Vector RoundCorner Radius`, Lottie `r` |
  - Only vertices whose in- and out-tangents are both zero-length are rounded; smooth/curved vertices and the end vertices of open paths are left unchanged (lottie-web `RoundCornersModifier`):
    ```
    K = 0.5519
    for vertex V with prev vertex Pp and next vertex Pn (wrapping on closed paths):
        dp = |Pp - V|;  a = V + (Pp - V) * (min(dp/2, Radius)/dp)    // dp == 0 -> a = V
        dn = |Pn - V|;  b = V + (Pn - V) * (min(dn/2, Radius)/dn)
        emit vertex a: in-tangent = 0 (tip at a), out-tip = a + (V - a)*K
        emit vertex b: in-tip = b + (V - b)*K, out-tangent = 0
    ```
    The cut distance is limited to half of each adjacent chord (distance between vertices, even when that segment is curved), so radii larger than half an edge do not overlap.
  - Radius 0 → unchanged path.
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "rd"`. Because Rectangle vertices already carry tangents only on one side, Round Corners does not affect a rectangle with Roundness > 0, but does round a Roundness-0 rectangle or a Polystar with 0 % roundness (all tangents zero).

### AE-SHP-036 · Trim Paths (修剪路径)

- **What it does:** Keeps only a portion of the input paths between Start and End (as fractions of arc length), rotated by Offset; the basis of write-on and line-drawing animation.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Start (开始) | scalar | 0 | 0–100 (values outside are clamped) | % | `ADBE Vector Trim Start`, Lottie `s` |
  | End (结束) | scalar | 100 | 0–100 (clamped) | % | `ADBE Vector Trim End`, Lottie `e` |
  | Offset (偏移) | angle | 0 | any | degrees; 360° = one full path length | `ADBE Vector Trim Offset`, Lottie `o` |
  | Trim Multiple Shapes (修剪多重形状) | enum | Simultaneously | 1 Simultaneously (同时), 2 Individually (单独) | — | `ADBE Vector Trim Type`, Lottie `m` |
  - Range computation (lottie-web `TrimModifier`, matches AE):
    ```
    o = (Offset mod 360)/360; if o < 0: o += 1
    s = clamp(Start/100, 0, 1) + o;  e = clamp(End/100, 0, 1) + o
    if s > e: swap(s, e)
    s = round(s*10000)/10000;  e = round(e*10000)/10000
    if s == e: output empty (nothing drawn)
    if (s,e) == (0,1): output = input unchanged (closed paths stay closed)
    else: segments = (e <= 1) ? [(s,e)] : (s >= 1) ? [(s-1,e-1)] : [(s,1),(0,e-1)]
    ```
  - Simultaneously: each input **shape** (the output of one path attribute, which may contain several sub-paths) is trimmed with the same `segments`, measured on that shape's own total length (its sub-paths concatenated in order).
  - Individually: all input shapes are concatenated into one sequence in Timeline order (topmost path first) and `segments` are applied to the total length; each shape receives the part of the range that falls in its own span (lottie-web `m: 2`, which Lottie docs call "Sequential"; `m: 1` = "Parallel" = Simultaneously). Adobe's help describes Individually loosely as treating the paths "as a compound path".
  - Wrap-around (`e > 1`): the piece `[s,1]` and the piece `[0,e−1]` are joined into one continuous open path through vertex 0 when the (last) sub-path is closed; otherwise two separate open paths.
  - Any trim other than the full range produces open paths (a closed path loses its closing join; fills close it with a straight chord). Trim boundaries split Bezier segments exactly (de Casteljau at the parameter that matches the arc length).
  - Arc length is measured in the path's own coordinate space (before the enclosing group transforms of the Trim's group; UNVERIFIED for paths inside scaled nested groups, AE-SHP-003).
- **Priority:** P0
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "tm"`. lottie-web approximates segment length by 150 samples per cubic (`defaultCurveSegments`); BAE should use kurbo `arclen` / `inv_arclen` (accuracy ~1e‑3 px) — differences vs lottie-web are sub-pixel. Path direction (AE-SHP-014) and first vertex (AE-SHP-043) decide where trimming starts. Trim before dashes: dash pattern is laid out on the trimmed path (AE-SHP-020).

### AE-SHP-037 · Twist (扭转)

- **What it does:** Rotates the path around a centre, more strongly near the centre than at the edges; positive angles twist clockwise.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Angle (角度) | angle | 0 | any | degrees | `ADBE Vector Twist Angle`, Lottie `a` |
  | Center (中心) | 2D point | (0,0) | any | px | `ADBE Vector Twist Center`, Lottie `c` |
  - UNVERIFIED algorithm (Adobe documents only the qualitative behaviour; no open-source player implements it). Best understanding:
    ```
    Rmax = max distance from Center to any vertex of the input shapes
    for each vertex and handle tip p:
        d = |p - Center|;  θ = Angle * (1 - d/Rmax)      // full twist at centre, none at Rmax
        p' = Center + rotate(p - Center, θ)
    ```
    AE probably subdivides long segments before twisting so straight edges curve smoothly (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "tw"` is defined but not rendered by lottie-web or Skottie. Needs empirical verification against AE renders before implementation.

### AE-SHP-038 · Wiggle Paths (摆动路径)

- **What it does:** Auto-animated random distortion of the paths: subdivides them and displaces vertices with smooth temporal noise.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Size (大小) | scalar | 10 (UNVERIFIED) | ≥ 0 | px | max displacement; `ADBE Vector Roughen Size` |
  | Detail (细节) | scalar | 10 (UNVERIFIED) | ≥ 0 | segments per… UNVERIFIED | subdivision density; `ADBE Vector Roughen Detail` |
  | Points (点) | enum | Smooth (UNVERIFIED) | Corner (边角), Smooth (平滑) | — | `ADBE Vector Roughen Points` |
  | Wiggles/Second (摇摆/秒) | scalar | 2 (UNVERIFIED) | ≥ 0 | Hz | `ADBE Vector Temporal Freq` |
  | Correlation (关联) | scalar | 50 | 0–100 | % | neighbour-vertex similarity; `ADBE Vector Correlation` |
  | Temporal Phase (时间相位) | angle | 0 | any | revolutions + degrees | `ADBE Vector Temporal Phase` |
  | Spatial Phase (空间相位) | angle | 0 | any | revolutions + degrees | `ADBE Vector Spatial Phase` |
  | Random Seed (随机植入) | integer | 0 | ≥ 0 | — | `ADBE Vector Random Seed` |
  - Behaviour (Adobe): output = series of irregular peaks/valleys; changes over time without keyframes at Wiggles/Second; Correlation low → neighbouring vertices move independently (jagged); Corner = sharp vertices (no tangents), Smooth = smooth vertices; with Wiggles/Second 0, animating Temporal Phase drives the wiggle manually; same seed and parameters → identical result.
  - UNVERIFIED reconstruction for implementation:
    ```
    subdivide each segment into k = max(1, round(Detail * segLen / Lref)) pieces   // Lref unknown
    for vertex j at time t:
        τ = t*WigglesPerSecond + TemporalPhase/360
        σ = j*(1 - Correlation/100) + SpatialPhase/360
        offset_j = Size * noise2D(seed, τ, σ)            // smooth noise in [-1,1], per x and y or along normal
    Smooth: tangents = Catmull-Rom style from neighbours; Corner: zero tangents
    ```
- **Priority:** P1 — widely used for organic motion; exact AE numeric match not required
- **Owner:** bae-vector (+ bae-anim)
- **Notes:** Not exported to Lottie (no `ty`); lottie-web/Skottie do not support it. The timing parameters behave like the text Wiggly selector (text.md, Wiggly selector). BAE decision: implement with a deterministic seeded noise so renders are reproducible across machines; exact AE numeric match is not expected (document as a known deviation until reverse-engineered).

### AE-SHP-039 · Wiggle Transform (摆动变换)

- **What it does:** Auto-animated random offsets of anchor point, position, scale and rotation applied to the paths above it; after a Repeater, each copy wiggles separately.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Wiggles/Second (摇摆/秒) | scalar | 2 (UNVERIFIED) | ≥ 0 | Hz | `ADBE Vector Xform Temporal Freq` |
  | Correlation (关联) | scalar | 50 (UNVERIFIED) | 0–100 | % | similarity between repeated copies |
  | Temporal Phase (时间相位) | angle | 0 | any | rev + deg | |
  | Spatial Phase (空间相位) | angle | 0 | any | rev + deg | |
  | Random Seed (随机植入) | integer | 0 | ≥ 0 | — | |
  | Transform (变换) › Anchor Point (锚点) | 2D | (0,0) | any | px | wiggle magnitude |
  | › Position (位置) | 2D | (0,0) | any | px | wiggle magnitude |
  | › Scale (缩放) | 2D % | (0,0) (UNVERIFIED) | any | % | wiggle magnitude |
  | › Rotation (旋转) | angle | 0 | any | degrees | wiggle magnitude |
  - Each frame a random transform is generated with each component in [−magnitude, +magnitude] (smooth over time at Wiggles/Second) and applied as a group-like transform (AE-SHP-029 order) to the paths above it in the group.
  - Ordering semantics (Adobe): Wiggle Transform above a Repeater → all copies wiggle identically; below a Repeater → each copy gets its own random transform, with Correlation 100 % = all copies the same, 0 % = independent.
  - All random components share one noise channel per instance, which is why Adobe recommends separate Wiggle Transform instances to wiggle properties independently (UNVERIFIED interpretation).
- **Priority:** P2
- **Owner:** bae-vector (+ bae-anim)
- **Notes:** Matchname `ADBE Vector Filter - Wiggler`, Transform group `ADBE Vector Wiggler Transform`. Not in Lottie. Same determinism decision as AE-SHP-038.

### AE-SHP-040 · Zig Zag (Z 字形)

- **What it does:** Converts each path segment into a regular zig-zag or wave with a given amplitude and number of ridges.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Size (大小) | scalar | 10 (UNVERIFIED) | any | px | amplitude; `ADBE Vector Zigzag Size`, Lottie `s` |
  | Ridges per segment (每段的背脊（译名待核）) | scalar | 5 (UNVERIFIED) | ≥ 0, rounded to integer | count | `ADBE Vector Zigzag Detail`, Lottie `r` |
  | Points (点) | enum | Corner (UNVERIFIED) | 1 Corner (边角), 2 Smooth (平滑) | — | `ADBE Vector Zigzag Points`, Lottie `pt` |
  - Algorithm (lottie-web `ZigZagModifier`):
    ```
    f = max(0, round(Ridges)); A = Size; dir = -1 initially
    for each sub-path (count = #segments):
      emit corner(vertex 0, dir)
      for each segment k:
         dir = -dir
         for i in 1..f:  t = i/(f+1)                         // Bezier parameter, not arc length
              p = segment.point(t); n = unit normal at t
              emit p + n*dir*A, with tangents along the segment of length chord/(2(f+1)) if Smooth else 0
              dir = -dir
         emit corner(vertex k+1, dir)
    corner(v, dir): displaced by A*dir along the perpendicular of (next - prev) vertex chord,
                    Smooth handle lengths = |v-prev|/(2(f+1)) and |v-next|/(2(f+1))
    Size 0 → path unchanged.
    ```
    Original vertices are also displaced (they are ridges).
- **Priority:** P1
- **Owner:** bae-vector
- **Notes:** Lottie `ty: "zz"`. UNVERIFIED: whether AE spaces ridges by arc length (likely) rather than Bezier parameter (lottie-web); on curved segments the ridges differ. Adobe's help mentions "absolute or relative size", which does not match the current property list (legacy wording).

## Bezier path model

### AE-SHP-041 · Path value data model (路径数据)

- **What it does:** Defines the animatable "Path" value type shared by shape Path attributes and mask paths: an ordered list of vertices with relative tangents and a closed flag.
- **Exact semantics:**
  - Value (AE scripting `Shape` object, expressions path methods, Lottie bezier):
    | Field | Type | Meaning |
    |---|---|---|
    | `vertices` (Lottie `v`) | array of [x,y] | anchor points, in the owning group's (or, for masks, layer's) space, px |
    | `inTangents` (Lottie `i`) | array of [x,y] | incoming handle, **relative to its vertex** |
    | `outTangents` (Lottie `o`) | array of [x,y] | outgoing handle, relative to its vertex |
    | `closed` (Lottie `c`) | bool | if true, a final segment joins the last vertex to vertex 0 |
    | `featherSegLocs`, `featherRelSegLocs`, `featherRadii`, `featherInterps`, `featherTensions`, `featherTypes`, `featherRelCornerAngles` | arrays | mask-only variable-feather points (compositing-3d-color.md, masks); ignored/absent on shape paths |
  - Segment k (from vertex k to k+1, or n−1 → 0 when closed) is the cubic `P0 = v[k], P1 = v[k] + o[k], P2 = v[k+1] + i[k+1], P3 = v[k+1]`. Zero tangents ⇒ straight line. An open path with n vertices has n−1 segments; closed has n.
  - A vertex is "smooth" when its in- and out-tangents are collinear and opposite (editing keeps them so), "corner" otherwise; this is an editing attribute, not stored separately (UNVERIFIED: AE may store a per-vertex smooth flag internally).
  - Edge cases: 0 vertices = empty (draws nothing); 1 vertex = a point (no visible stroke except caps? UNVERIFIED: AE draws nothing); arrays must have equal length (scripting throws otherwise).
  - Coordinate convention: y down; a path that runs clockwise on screen has positive shoelace area in these coordinates. Parametric shapes are clockwise by default (AE-SHP-010..012).
- **Priority:** P0
- **Owner:** bae-model (+ bae-vector, bae-anim)
- **Notes:** Lottie: `sh.ks` = `{a:0, k:{i,o,v,c}}` or animated `{a:1, k:[{t, s:[{i,o,v,c}], i:{x,y}, o:{x,y}, h?}]}` (older files also carry `e`). Scripting: `new Shape()`, `Property.setValue(shape)` (project-comp-layers.md, property value types). Expressions: `points()`, `inTangents()`, `outTangents()`, `isClosed()`, `pointOnPath()`, `tangentOnPath()`, `normalOnPath()`, `createPath()` — transform-keyframes-expressions.md. BAE decision: store as `Vec<PathVertex{pos, in_rel, out_rel}>` + `closed` in f64, convert to kurbo `BezPath` lazily.

### AE-SHP-042 · Path keyframe interpolation (路径插值)

- **What it does:** Defines how a Path property animates between keyframes.
- **Exact semantics:**
  - Interpolation is vertex-wise: vertex k (and its two tangents) at keyframe A moves to vertex k at keyframe B along a straight line; the temporal ease (keyframe velocity/influence, Hold) controls progress `u(t)` exactly as for 1-D properties (transform-keyframes-expressions.md, keyframes/ease). There is no spatial (curved) interpolation for paths.
    ```
    v_k(t) = lerp(vA_k, vB_k, u(t)); in_k(t) = lerp(inA_k, inB_k, u); out_k(t) = lerp(outA_k, outB_k, u)
    closed(t) = closedA      // UNVERIFIED: which keyframe's closed flag wins when they differ
    ```
  - Matching uses vertex indices starting at the first vertex (AE-SHP-043), so the first vertex choice changes the animation.
  - Mismatched vertex counts between keyframes: with the preference "Preserve Constant Vertex Count and Feather Point Count when Editing Masks" (on by default), vertex edits are propagated to all keyframes so counts stay equal. UNVERIFIED for pasted/scripted keyframes with different counts: best understanding — AE adds vertices to the path with fewer vertices (subdividing its longest segments) for interpolation purposes.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-vector)
- **Notes:** lottie-web's `interpolateShape` assumes equal counts. Smart Mask Interpolation (Window > Mask Interpolation) generates intermediate keyframes for masks only (compositing-3d-color.md, masks) — UNVERIFIED whether it works on shape paths (best understanding: masks only). BAE decision: when counts differ, resample the shorter path by inserting vertices at arc-length midpoints of its longest segments (deterministic) and flag it in the UI.

### AE-SHP-043 · First vertex (第一个顶点)

- **What it does:** Marks which vertex is index 0 of a path; it anchors path interpolation, Trim Paths, dashes and paste-as-motion-path.
- **Exact semantics:**
  - Command: Layer > Mask and Shape Path > Set First Vertex (设置第一个顶点), with exactly one vertex selected. It renumbers the path's vertices (cyclic rotation for closed paths) at the current keyframe; other keyframes keep their numbering, so the animation changes (Adobe). UNVERIFIED: whether it applies to all keyframes when the CTI is not on a keyframe.
  - On open paths the first vertex is always an end point (UNVERIFIED: command unavailable / reverses the path if the other end is selected).
  - Display: the first vertex is drawn slightly larger in the viewer.
  - When a closed path is pasted into a Position property (motion path), the first vertex becomes the start of the motion path.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion, bae-model)
- **Notes:** Same command for masks and shape paths.

### AE-SHP-044 · Path vertex editing (编辑路径顶点)

- **What it does:** Interactive editing operations on Bezier shape/mask paths: move vertices/segments/handles, add/delete vertices, convert corner/smooth, free-transform selected points.
- **Exact semantics:**
  - Selection tool: drag vertex; drag direction handle (smooth vertex: opposite handle stays collinear, length independent; corner: only that handle); drag a curved segment (adjusts both adjacent handles; on RotoBezier also moves vertices). Shift-click extends selection; marquee selects vertices.
  - Add Vertex tool (添加"顶点"工具): click a segment → inserts a vertex by splitting the cubic at that parameter (de Casteljau, path shape unchanged). With "Preserve Constant Vertex Count…" on, the vertex is added at the same relative position in every keyframe and reshapes only the current one.
  - Delete Vertex tool (删除"顶点"工具): click a vertex → removes it (from all keyframes). Delete key with vertices selected also deletes them (UNVERIFIED for shape paths: deleting all vertices removes the path value).
  - Convert Vertex tool (转换"顶点"工具): click a vertex toggles corner (tangents zeroed) ↔ smooth (tangents created along the bisector, length UNVERIFIED ~1/3 of the neighbouring chords); drag a handle breaks/re-aligns it.
  - Pen tool context switching: over a segment → Add Vertex; over a vertex → Delete Vertex; Alt/Option over a vertex → Convert Vertex; Ctrl/Cmd → temporary Selection tool; G cycles pen tools.
  - Free Transform Points (自由变换点, Layer > Mask and Shape Path > Free Transform Points, Ctrl/Cmd+T, or double-click the path): bounding box with anchor; scale (Shift = proportional), rotate (outside the box), move anchor; applies the 2D affine transform to the selected vertices and their tangents; Esc/Enter exits. Changes are written into the Path value (keyframe at current time if animated).
  - Arrow keys nudge selected vertices 1 px (Shift: 10 px) at current magnification (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion, bae-vector)
- **Notes:** All edits are Commands in bae-motion (`InsertVertex`, `DeleteVertex`, `SetVertexTangents`, `TransformVertices`) so agents can use them; the viewer gestures are bae-ui-motion. Shortcut tables: ui-preview-render-io.md, tools and shortcuts.

### AE-SHP-045 · RotoBezier paths (RotoBezier 路径)

- **What it does:** Path mode in which tangents are computed automatically from neighbouring vertices with per-vertex tension, used for quick smooth drawing.
- **Exact semantics:**
  - Toggle: Layer > Mask and Shape Path > RotoBezier, or the RotoBezier checkbox in the Tools panel when the Pen tool is active (applies to new paths).
  - Manual → RotoBezier changes the shape if handles had been adjusted (tangents recomputed); RotoBezier → manual keeps the shape.
  - Tension (Adobe-documented): drag a vertex with the Convert Vertex tool (Alt/Option with the Pen tool active) to change the tension of the selected vertices; the Info panel shows the value. Clicking a vertex without dragging sets it to a corner point (100 % tension); clicking again sets it to a smooth point (33 % tension, the default for new RotoBezier vertices). Dragging up or right decreases tension (more curvature in the adjacent segments); dragging down or left increases it. Range 0–100 % (UNVERIFIED lower bound).
  - Tangent formula UNVERIFIED (best understanding: Catmull-Rom-like, tangent ∝ (next − prev) scaled by (1 − tension) and adjacent chord lengths, so 100 % tension gives zero-length tangents).
- **Priority:** P2
- **Owner:** bae-vector (+ bae-ui-motion)
- **Notes:** RotoBezier state is exposed in scripting for masks (`MaskPropertyGroup.rotoBezier`) but not for shape paths (UNVERIFIED). Lottie export bakes the computed tangents.

### AE-SHP-046 · Copy and paste paths between masks, shapes, motion paths and Illustrator (路径复制粘贴)

- **What it does:** Path values can be copied between mask paths, shape Path properties, paint stroke paths, Position/anchor motion paths and Illustrator/Photoshop clipboard paths.
- **Exact semantics:**
  - Mask Path ↔ shape Path: copy the property (or its keyframes) and paste onto a selected Path/Mask Path property; vertices/tangents copied verbatim in numbers (coordinate space differs: mask = layer space, shape = group space, so position may shift).
  - Paste into a layer with a shape layer selected but no Path property selected: creates a new Path (in a new group with stroke/fill? UNVERIFIED) — for non-shape layers, creates a new mask.
  - Illustrator/Photoshop: copy a path in Illustrator (AICB clipboard option required) or Photoshop (72 dpi for 1:1 scale) and paste into AE as mask path or shape path; compound paths become multiple paths (UNVERIFIED).
  - Motion path → path: copy Position/Anchor Point (or effect point) keyframes from a single property and paste into a selected Path property: each keyframe becomes a vertex with its spatial tangents (open path).
  - Path → motion path: paste a path into Position: creates Position keyframes along the path (roving except first/last, total duration 2 s by default; UNVERIFIED), starting at the first vertex.
  - Shape group duplicate-while-transforming: Alt/Option-drag a group's transform box duplicates the group.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Motion-path keyframe semantics belong to transform-keyframes-expressions.md (keyframes, roving). Clipboard formats: BAE decision — support SVG path data on the clipboard as the interchange format (Illustrator AICB is proprietary).

## Masks on shape layers

### AE-SHP-047 · Masks on shape layers (形状图层上的蒙版)

- **What it does:** Shape layers can carry layer masks like any AV layer; masks clip the rendered (rasterised) shape layer and do not alter the vector contents.
- **Exact semantics:**
  - Creating masks: with a shape layer selected, the shape tools/Pen create **shapes** by default; enable "Tool Creates Mask" (工具创建蒙版) in the Tools panel to draw masks instead (AE-SHP-051). Masks can also be pasted (AE-SHP-046).
  - Rendering order: contents rendered → masks applied (mask modes, feather, expansion, opacity) → effects → layer styles/transform per the normal layer pipeline (compositing-3d-color.md, render order and masks).
  - Mask coordinates are in layer space (origin = shape layer origin, not comp corner), unlike footage layers whose origin is the top-left of the source.
  - Since a shape layer is unbounded, masks are the usual way to crop it.
- **Priority:** P0
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** Mask features themselves are fully specified in compositing-3d-color.md, "Masks"; this item only states the shape-layer specifics.

## Shape conversion commands

### AE-SHP-048 · Create Shapes from Vector Layer (从矢量图层创建形状)

- **What it does:** Converts an Illustrator/PDF/EPS (vector footage) layer into a new, editable shape layer.
- **Exact semantics:**
  - Menu: Layer > Create > Create Shapes from Vector Layer (also in the layer context menu). Exactly one selected vector footage layer.
  - Result: a new shape layer named "<source name> Outlines" (UNVERIFIED name) placed directly above the source; the source layer's Video switch is turned off. Transform values of the source are carried over so the result aligns (UNVERIFIED: anchor/position mapping).
  - Structure: one group per Illustrator path/object, nested groups for Illustrator groups/layers (UNVERIFIED depth handling), each with Path(s), Fill and/or Stroke reproducing colour, opacity, stroke width, caps, joins, miter limit and dashes; compound paths become one group with several paths plus Merge Paths? (UNVERIFIED; best understanding: several Path attributes in one group with Even-Odd/Non-Zero fill per Illustrator fill rule).
  - Gradients: since AE 26.x (public beta 26.0x8, October 2025; Adobe help updated January 2026; listed in What's New with the 2026 releases) gradient fills, gradient strokes and transparency are retained as Gradient Fill/Gradient Stroke attributes (with the new gradient Scale/Rotation, AE-SHP-063); earlier versions (and still-unsupported types) produce 50 % grey shapes. Freeform gradients and Illustrator blend modes are not retained (Adobe); effects/appearance stacks, live text, images and meshes are not converted (UNVERIFIED list).
  - Pixel Aspect Ratio overrides from Interpret Footage are ignored. Files with thousands of paths convert slowly without progress feedback.
  - Works on one layer at a time; for an AI imported as Composition, each layer must be converted individually (Adobe). SVG import/paste as shape layers: AE-SHP-064.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-media, bae-vector)
- **Notes:** Requires a PDF/AI vector parser (bae-media) producing BAE path/paint data; the command itself is bae-motion. Introduced in AE CS6 (11.0). BAE decision: also accept SVG layers as input (BAE-native SVG import), which is the main Lottie/Figma path.

### AE-SHP-049 · Create Shapes from Text (从文字创建形状)

- **What it does:** Creates a shape layer with the outlines of a text layer's characters.
- **Exact semantics:** (shape-side only)
  - Result shape layer is created at the top of the stack (above all layers), named "<text layer name> Outlines"; the text layer's Video switch is turned off.
  - One group per selected character (named by the character), containing one Path per contour; multi-contour glyphs (e.g. "e", "i") get a Merge Paths in Merge mode; Fill and Stroke matching the text fill/stroke colours and stroke width; effects, masks, layer styles and Transform keyframes/expressions of the text layer are copied.
  - Text animators are not baked; the outlines reflect the text at the current time (UNVERIFIED).
  - Create Masks from Text (从文字创建蒙版) is the mask variant on a new solid.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-text, bae-vector)
- **Notes:** Text-side behaviour (glyph outlines, selection of characters, per-character transforms) is specified in text.md, "Create Shapes from Text". Glyph contours are emitted with the font's winding so Non-Zero filling produces holes.

### AE-SHP-050 · Create Nulls from Paths (从路径创建空对象)

- **What it does:** Bundled ScriptUI panel (Window > Create Nulls From Paths.jsx) that rigs path vertices to null layers with expressions.
- **Exact semantics:**
  - Buttons: Points Follow Nulls (点跟随空对象: creates one null per vertex and writes a `createPath()` expression on the Path so vertices follow nulls); Nulls Follow Points (空对象跟随点: nulls get position expressions reading `path.points()`); Trace Path (追踪路径: one null animated along the path via `pointOnPath()` with a Progress slider and optional loop).
  - Works on Bezier paths only (parametric shapes must be converted first, AE-SHP-015). Group transforms must be identity for exact alignment (Adobe note).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Script (CC 2018 / 15.0+), not core AE. BAE can ship an equivalent command; it depends on expression path methods (transform-keyframes-expressions.md).

## Shape tools

### AE-SHP-051 · Shape tool target and Tool Creates Shape / Mask (工具创建形状 / 工具创建蒙版)

- **What it does:** Decides whether dragging with a shape tool or clicking with the Pen tool creates a new shape layer, adds a shape to an existing shape layer, or creates a mask.
- **Exact semantics:**
  | Context when drawing starts | Result |
  |---|---|
  | No layer selected (Composition panel) | New shape layer (AE-SHP-008) containing the new shape |
  | One shape layer selected, Tool Creates Shape on (default) | New shape group added to that layer's Contents (into the selected group if a group is selected; UNVERIFIED) |
  | Shape layer selected, Tool Creates Mask on | New mask on the shape layer |
  | Any non-shape layer selected (Composition or Layer panel) | New mask on that layer (the Shape/Mask toggle is not shown) |
  | Layer panel | Always a mask |
  - The two toggle buttons appear in the Tools panel options area only while a shape tool or the Pen tool is active and a shape layer is selected.
  - New shapes start at the current time with no keyframes; group name "<Kind> <n>" (Rectangle 1, Ellipse 1, Polystar 1, Shape 1 for Pen).
  - Shape tools cycle with Q (Rectangle → Rounded Rectangle → Ellipse → Polygon → Star); Pen tools with G (ui-preview-render-io.md, shortcuts).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** The creation itself is a bae-motion Command (`AddShape{layer, group, primitive, params}` / `AddMask`), so agents get identical results.

### AE-SHP-052 · Drawing rectangles, rounded rectangles and ellipses (绘制矩形、圆角矩形和椭圆)

- **What it does:** Drag interaction of the Rectangle (矩形工具), Rounded Rectangle (圆角矩形工具) and Ellipse (椭圆工具) tools.
- **Exact semantics:**
  - Drag diagonally from corner to corner; the result is a parametric primitive (Size = |drag|, centred at the drag rectangle centre) unless the Bezier Path option is on or Alt/Option is held before the drag starts (then a Bezier Path with the same geometry, AE-SHP-058).
  - Modifiers during the drag: Shift = square/circle (release Shift after the mouse); Ctrl/Cmd (pressed after starting) = draw from the centre; Space or middle mouse = reposition while drawing; Esc = cancel.
  - Rounded Rectangle only, before release: Up/Down arrow or mouse wheel = increase/decrease Roundness (UNVERIFIED step: 1 px per press?); Left arrow = Roundness 0; Right arrow = maximum (half the shorter side).
  - Each tool remembers its last-used parameters (e.g. roundness) for the next drawing; double-clicking the tool resets them to defaults and also creates a layer-sized shape (AE-SHP-054).
  - On a mask target the same gestures create mask paths (always Bezier).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Keyboard modifiers are listed in ui-preview-render-io.md (shortcuts, shape layers).

### AE-SHP-053 · Drawing polygons and stars (绘制多边形和星形)

- **What it does:** Drag interaction of the Polygon (多边形工具) and Star (星形工具) tools, creating a Polystar.
- **Exact semantics:**
  - Drag from the centre outward: distance = Outer Radius, drag direction sets Rotation (a vertex points at the cursor); Shift-drag = no rotation (Rotation 0, release Shift after the mouse).
  - Before release: Up/Down arrow or wheel = add/remove points (Points ±1); Left/Right arrow = decrease/increase Outer Roundness (UNVERIFIED step); Page Up/Page Down = increase/decrease Inner Roundness (Star); Ctrl/Cmd = keep Inner Radius constant while the outer radius changes (otherwise Inner Radius scales proportionally with Outer Radius); Space = reposition; Esc = cancel.
  - Star tool sets Type = Star (default Inner Radius = Outer Radius/2, UNVERIFIED); Polygon tool sets Type = Polygon; default Points 5; tools remember last values.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-SHP-054 · Double-click a shape tool: layer-sized shape (双击形状工具)

- **What it does:** Double-clicking a shape tool in the Tools panel creates a shape or mask the size of the target layer.
- **Exact semantics:**
  - Shape layer selected: adds a shape whose bounds equal the layer's bounds — for shape layers this is the composition frame mapped into layer space (UNVERIFIED); no layer selected (F2): new shape layer with a comp-sized shape; a shape Path selected (not the group): replaces that path's geometry; non-shape layer selected: mask with the layer's size (Rectangle = full-frame mask, Ellipse = inscribed ellipse, Polygon/Star = inscribed polystar).
  - Also resets that tool's remembered parameters to defaults (Adobe note on tool settings).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Full-frame masks are commonly made this way (compositing-3d-color.md, masks).

### AE-SHP-055 · Pen tool drawing (钢笔工具)

- **What it does:** Draws Bezier shape paths (or masks) vertex by vertex.
- **Exact semantics:**
  - Click = corner vertex (zero tangents, straight segments); click-drag = smooth vertex whose out-tangent follows the drag and in-tangent is its mirror; Shift-click constrains the segment angle to multiples of 15°; Shift-drag constrains the handle angle to 15° steps; Alt/Option-drag while placing modifies only the outgoing handle (breaks the tangent); Space while dragging repositions the vertex being placed.
  - Close: click the first vertex (closed-circle cursor), double-click the last vertex, or Layer > Mask and Shape Path > Closed (闭合). Leave open: switch tools or press F2.
  - The last vertex is shown filled (selected), earlier ones hollow. Ctrl/Cmd temporarily switches to Selection; over existing paths the pen switches to Add/Delete/Convert Vertex (AE-SHP-044).
  - RotoBezier option on → tangents computed automatically (AE-SHP-045).
  - New shape: group "Shape 1" with Path 1 (Bezier, coordinates per AE-SHP-008), Stroke and Fill per Tools panel; gradient control points start at the layer centre.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion, bae-vector)
- **Notes:** Pen-tool and selection mouse behaviour common to masks: ui-preview-render-io.md (tools & mouse behaviour).

### AE-SHP-056 · Tools panel Fill and Stroke controls (工具面板的填充和描边选项)

- **What it does:** Tools-panel controls (visible with a shape layer selected or a shape/pen tool active) that set fill/stroke for new shapes and modify the selected shapes.
- **Exact semantics:**
  - Controls in order: Fill (填充) text control → Fill Options dialog; Fill swatch; Stroke (描边) text control → Stroke Options dialog; Stroke swatch; stroke width scrub field (px); then the Add menu and (Pen) RotoBezier / (shape tools) Bezier Path checkbox.
  - Fill/Stroke Options dialog: type buttons None (无), Solid Color (纯色), Linear Gradient (线性渐变), Radial Gradient (径向渐变); Blend Mode; Opacity (%); Preview checkbox.
  - Alt/Option-click the swatch cycles the type None → Solid → Linear → Radial; click the swatch opens the colour picker (Solid) or Gradient Editor (gradients).
  - With shapes selected, changes apply to all selected shapes' corresponding paints (and become the defaults for new shapes); mixed values show "?" in the swatch; setting a value applies it to all.
  - None means no Fill (or Stroke) attribute is created for new shapes; for existing shapes the attribute is disabled/removed (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-SHP-057 · Default fill and stroke settings of new shapes (新形状的默认填充和描边)

- **What it does:** The initial Tools-panel paint state used for new shapes, persisted across sessions.
- **Exact semantics:**
  - Persisted per user (preferences), not per project; last-used settings carry over.
  - First-run defaults UNVERIFIED: Fill Solid, colour a saturated red (best understanding sRGB ≈ (1, 0, 0)… exact value to be measured), Stroke Solid white, Stroke Width 2 px, Blend Mode Normal, Opacity 100 %.
  - The created Fill/Stroke attributes take these values; Stroke also takes Line Cap Butt, Line Join Miter, Miter Limit 4.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: store tool defaults in user settings (bae-ui), not in the project.

### AE-SHP-058 · Bezier Path option (贝塞尔曲线路径选项)

- **What it does:** Makes shape tools create Bezier Path attributes instead of parametric primitives.
- **Exact semantics:**
  - Tools-panel checkbox "Bezier Path" (贝塞尔曲线路径（译名待核）) while a shape tool is active, or hold Alt/Option before starting the drag (may release during the drag).
  - The created Path has exactly the geometry the primitive would generate (AE-SHP-010..012), as one Path attribute in the new group.
  - Masks are always Bezier.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-SHP-059 · Selecting and transforming groups in the viewer (在查看器中选择和变换组)

- **What it does:** Composition-panel interaction with shape contents: group selection mode vs. path editing, group transform box, gradient handles.
- **Exact semantics:**
  - Clicking a shape with the Selection tool selects the layer; clicking again (or double-clicking) enters the group and selects the group (dashed transform box with anchor), further double-clicks drill into nested groups / path editing (UNVERIFIED exact click sequence).
  - Dragging/handles of the group box edit the group's Transform (Position, Scale with Shift proportional, Rotation outside the box, Anchor by dragging the anchor icon); Alt/Option-drag duplicates the group while transforming.
  - With a group containing a gradient paint selected, Start Point, End Point and Highlight controls are draggable (pointer changes to the gradient pointer).
  - Arrow keys nudge the selected group (1 px, Shift 10 px) (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** Layer-level selection/handles: ui-preview-render-io.md (viewer interaction).

## Interchange and 3D

### AE-SHP-060 · Shape attribute type codes and match names (形状属性类型码与匹配名称)

- **What it does:** Canonical mapping between AE shape attributes, their scripting matchNames, Lottie `ty` codes and BAE schema names, used by import/export and scripting compatibility.
- **Exact semantics:**
  | AE attribute | matchName | Lottie `ty` | Key Lottie fields |
  |---|---|---|---|
  | Shape layer | `ADBE Vector Layer` | layer `ty: 4` | `shapes[]` |
  | Contents | `ADBE Root Vectors Group` | (layer `shapes`) | — |
  | Group | `ADBE Vector Group` | `gr` | `it[]`, `np`, `bm` |
  | Group Transform | `ADBE Vector Transform Group` | `tr` | `a p s r o sk sa` |
  | Rectangle | `ADBE Vector Shape - Rect` | `rc` | `d s p r` |
  | Ellipse | `ADBE Vector Shape - Ellipse` | `el` | `d s p` |
  | Polystar | `ADBE Vector Shape - Star` | `sr` | `d sy pt p r ir or is os` |
  | Path | `ADBE Vector Shape - Group` (value `ADBE Vector Shape`) | `sh` | `d ks{i,o,v,c}` |
  | Fill | `ADBE Vector Graphic - Fill` | `fl` | `c o r bm` |
  | Stroke | `ADBE Vector Graphic - Stroke` | `st` | `c o w lc lj ml ml2 d[] bm` |
  | Gradient Fill | `ADBE Vector Graphic - G-Fill` | `gf` | `t s e h a g o r` |
  | Gradient Stroke | `ADBE Vector Graphic - G-Stroke` | `gs` | `t s e h a g o w lc lj ml d[]` |
  | Merge Paths | `ADBE Vector Filter - Merge` | `mm` | `mm` |
  | Offset Paths | `ADBE Vector Filter - Offset` | `op` | `a lj ml` |
  | Pucker & Bloat | `ADBE Vector Filter - PB` | `pb` | `a` |
  | Repeater | `ADBE Vector Filter - Repeater` | `rp` | `c o m tr{a p s r so eo}` |
  | Round Corners | `ADBE Vector Filter - RC` | `rd` | `r` |
  | Trim Paths | `ADBE Vector Filter - Trim` | `tm` | `s e o m` |
  | Twist | `ADBE Vector Filter - Twist` | `tw` | `a c` |
  | Wiggle Paths | `ADBE Vector Filter - Roughen` | — (not exported) | — |
  | Wiggle Transform | `ADBE Vector Filter - Wiggler` | — | — |
  | Zig Zag | `ADBE Vector Filter - Zigzag` | `zz` | `s r pt` |
  - Common keys on every Lottie shape item: `nm` name, `mn` matchName, `hd` hidden (= disabled), `ix` property index, `bm` blend mode. Lottie also defines `no` (No Style) which AE never produces.
  - Property matchNames of each attribute are listed in the corresponding items above (source: docsforadobe shape layer match names; note the upstream typos `ADBE Vector Star Inner Roundess` / `Outer Roundess` are the real matchNames).
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Lottie export/import pipeline itself: ui-preview-render-io.md (Lottie export) and the BAE Lottie importer. BAE decision: keep AE matchNames as stable IDs in the schema so AE-script-compatible `property("ADBE …")` lookups work.

### AE-SHP-061 · Lottie fidelity rules for shape layers (Lottie 形状保真规则)

- **What it does:** Lists the AE behaviours that Lottie players implement differently, so BAE's renderer can choose AE semantics while still round-tripping Lottie files.
- **Exact semantics:**
  | Topic | AE (this spec) | lottie-web | Skottie |
  |---|---|---|---|
  | Merge Paths | Boolean ops, suppresses paints above (AE-SHP-031) | not implemented | implemented |
  | Group opacity | per paint (UNVERIFIED, AE-SHP-030) | per paint | isolated layer |
  | Paint Composite "Above" | supported (AE-SHP-004) | not in format | not in format |
  | Taper / Wave / Offset Copies | supported | not in format | not in format |
  | Wiggle Paths / Wiggle Transform | supported | not in format | not in format |
  | Twist | supported | ignored | ignored |
  | Polystar fractional Points | UNVERIFIED (AE-SHP-012) | floor | floor |
  | Ellipse kappa | UNVERIFIED (AE-SHP-011) | 0.5519 | 0.5519 (UNVERIFIED) |
  | Trim arc length | exact | 150 samples/segment | exact-ish |
  | Gradient midpoints | native | baked by exporter | baked |
  - BAE import of a Lottie file must reproduce lottie-web's look where the format is lossy (e.g. no Composite key → default order); BAE export bakes unsupported features (e.g. Wiggle Paths → path keyframes per frame, taper → filled outline path) when the user opts in.
- **Priority:** P1
- **Owner:** bae-vector (+ bae-media)
- **Notes:** Parity tests: render the same JSON with lottie-web (headless) and BAE and diff, per operator.

### AE-SHP-062 · Shape layer extrusion and bevel geometry (形状图层的凸出与斜面)

- **What it does:** With a 3D shape layer in a comp using the Cinema 4D or Advanced 3D renderer, shape layers gain Geometry Options to extrude and bevel the vector contents into 3D meshes.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Bevel Style (斜面样式) | enum | None | None, Angular, Concave, Convex | — | UNVERIFIED enum names/order |
  | Bevel Depth (斜面深度) | scalar | 2 (UNVERIFIED) | ≥ 0 | px | |
  | Hole Bevel Depth (洞斜面深度) | scalar | 100 (UNVERIFIED) | 1–100 | % of Bevel Depth | |
  | Extrusion Depth (凸出深度) | scalar | 0 | ≥ 0 | px | |
  - Group-level Material Options (`ADBE Vector Materials Group`) allow per-group materials (front/bevel/side) (UNVERIFIED property list).
  - Only available when the comp renderer is not Classic 3D; ignored by the Classic 3D renderer.
- **Priority:** P2
- **Owner:** bae-render (+ bae-vector)
- **Notes:** Renderer selection, materials and lighting: compositing-3d-color.md (Advanced 3D/C4D, P2). Introduced with the Cinema 4D renderer in AE CC 2013 (12.0) (UNVERIFIED); Advanced 3D renderer AE 24.x.

