## Tools

### AE-UI-300 · Tool activation, momentary tools and tool cycling (激活工具和循环切换工具)

- **What it does:** Defines how a tool becomes active: clicking its button in the Tools panel (toolbar), tapping its single-key shortcut, holding the shortcut for a momentary (spring-loaded) switch, repeated presses to cycle through a tool group, and Alt/Opt-click on a group button.
- **Exact semantics:**
  - **Tool groups.** Tools that share a button form a group; the button shows the most recently used member and a small triangle; press-and-hold (or click the triangle) opens a flyout listing every member with its shortcut. Groups (AE order): Selection; Hand; Zoom; Orbit/Pan/Dolly camera navigation (AE 22.x+ 3D design space, see AE-UI-311); Rotation; Unified Camera + camera tools; Pan Behind; Shape (Rectangle, Rounded Rectangle, Ellipse, Polygon, Star); Pen (Pen, Add Vertex, Delete Vertex, Convert Vertex, Mask Feather); Type (Horizontal, Vertical); Brush; Clone Stamp; Eraser; Roto Brush (Roto Brush, Refine Edge); Puppet (Position, Starch, Bend, Advanced, Overlap pins).
  - **Tap vs hold.** Pressing a single-letter tool shortcut and releasing it immediately makes that tool the active tool. Holding the key down activates it **momentarily**; on key release the previously active tool returns. Implementation rule: if key-up happens after any pointer drag/click occurred while the key was held, or after a hold threshold (UNVERIFIED: AE uses "used while held" rather than a fixed time), revert; otherwise latch.
  - **Cycling.** Pressing a group shortcut while a member of that group is already active advances to the next member of the group in group order and wraps (Q: Rectangle → Rounded Rectangle → Ellipse → Polygon → Star → Rectangle). C cycles Unified Camera → Orbit → Track XY (Pan) → Track Z (Dolly) camera tools (names per AE-UI-310); G cycles Pen ↔ Mask Feather only (Add/Delete/Convert Vertex are reached through the flyout or modifiers), and a preference turns the G cycling off; Ctrl+T / Cmd+T cycles Horizontal ↔ Vertical Type; Ctrl+B / Cmd+B cycles Brush → Clone Stamp → Eraser; Ctrl+P / Cmd+P cycles the Puppet tools; Alt+W / Opt+W toggles Roto Brush ↔ Refine Edge. Alt-click (Win) / Opt-click (Mac) on a group button in the Tools panel also advances to the next member. No Shift+key cycling exists in AE (UNVERIFIED: no "Use Shift Key for Tool Switch" preference like Photoshop).
  - **Context availability.** Some tools are disabled when they cannot act: camera tools need a 3D view (a comp containing a camera or 3D layer, UNVERIFIED exact rule), paint tools and Roto Brush operate only in the Layer panel, Puppet tools need a selected layer with pixels. Selecting a disabled tool's shortcut does nothing.
  - **Tool options area.** The rest of the Tools panel row shows options of the active tool (for example Rotation: Set Orientation/Rotation; shape tools: Tool Creates Shape/Mask, Fill, Stroke, Bezier Path; Pen: RotoBezier; Type, Brush, Clone Stamp, Eraser: Auto-Open Panels (自动打开面板); Puppet: Mesh Expansion/Density, Record Options; 3D-capable tools: axis mode buttons (AE-UI-313); Selection: Snapping (AE-UI-359)).
  - **Tool state is per application, not per viewer**: the active tool applies to every Composition/Layer/Footage viewer. The current tool is saved with the workspace-independent application state, not in the project.
  - Text-editing and numeric-field focus suspend single-key tool shortcuts (typing V into a text layer types "v").
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** Scripting: `app.project.toolType` (read/write) with `ToolType` enum values `Tool_Arrow`, `Tool_Rotate`, `Tool_CameraMaya` (Unified Camera), `Tool_CameraOrbit`, `Tool_CameraTrackXY`, `Tool_CameraTrackZ`, `Tool_Paintbrush`, `Tool_CloneStamp`, `Tool_Eraser`, `Tool_Hand`, `Tool_Magnify`, `Tool_PanBehind`, `Tool_Rect`, `Tool_RoundedRect`, `Tool_Oval`, `Tool_Polygon`, `Tool_Star`, `Tool_TextH`, `Tool_TextV`, `Tool_Pen`, `Tool_Feather`, `Tool_PenPlus`, `Tool_PenMinus`, `Tool_PenConvert`, `Tool_Pin`, `Tool_PinStarch`, `Tool_PinDepth` (Overlap), `Tool_Quickselect` (Roto Brush), `Tool_Hairbrush` (Refine Edge). The Tools panel itself (layout, workspace bar, Ctrl+1 toggle) is ui-preview-render-io.md (ui-a), Tools panel. BAE decision: the tool state machine lives in bae-ui-motion; every edit a tool performs is issued as a bae-model / bae-motion Command, merged into one undo step per drag.

### AE-UI-301 · Selection tool: selecting layers in the viewer (选取工具)

- **What it does:** V activates the Selection tool, which selects, moves and scales layers, masks, paths and effect control points in the Composition and Layer viewers.
- **Exact semantics:**
  - **Click** on a layer's visible pixels or its bounding box interior (for layers without pixels: null outline, camera/light icon, text glyph box) selects that layer only and deselects other layers. Hit-testing order is the stacking order top → bottom; locked layers (Lock switch) are never hit. Hidden (eye off) layers are not hit. Shy layers are still hit in the viewer.
  - **Shift-click** (Win/Mac) and **Ctrl-click** (Win) / **Cmd-click** (Mac) toggle the clicked layer in the selection without deselecting others.
  - **Click on empty canvas** deselects all layers (masks and paths too).
  - **Marquee:** dragging from an empty area draws a selection rectangle; on release every unlocked visible layer whose bounds intersect the rectangle is selected (replacing the selection; Shift-marquee adds). When a layer already has a mask or shape path selected, marquee selects vertices of the selected paths instead (see AE-UI-306).
  - Layer bounds and handles of selected layers are drawn in the layer's label colour (Preferences > Appearance > Use Label Color for Layer Handles and Paths, UNVERIFIED default on); handles: 8 squares (4 corners + 4 side midpoints) and the anchor point crosshair.
  - Selection in the viewer and the Timeline are the same selection set (selecting in one highlights in the other).
  - **Middle-mouse click** in a multi-view layout activates that view without changing layer selection.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Layer-selection keyboard commands (Ctrl+Up/Down, numpad digits, Ctrl+A, F2) are in AE-UI-322 and AE-UI-332. Shape-layer modifiers (Alt = shape duplication, Ctrl/Cmd = direct selection) are AE-UI-305. Implementation hint: GPU picking with an ID buffer (one u32 layer index per pixel, rendered at viewer resolution) or CPU hit-testing on cached alpha at preview resolution; bounding-box hit for nulls/text.

### AE-UI-302 · Selection tool: move layers by dragging (使用选取工具拖动图层)

- **What it does:** Dragging a selected layer (or an unselected layer, which first becomes the sole selection) moves all selected layers in the viewer by editing Position.
- **Exact semantics:**
  - Drag delta in screen pixels is converted to comp pixels (divide by viewer magnification, account for pixel aspect ratio of the comp) and then to each layer's parent space; the Position of each selected layer changes by that delta (for parented layers Position is in parent space, so the delta is transformed by the inverse of the parent's world matrix, linear part only).
  - If Position has its stopwatch on, the drag sets/creates a keyframe at the current time (like any value change; see transform-keyframes-expressions.md, Keyframes). If Position is separated (Separate Dimensions), X/Y (and Z) Position properties are edited instead.
  - **Shift** (pressed after the drag starts) constrains movement to the horizontal or vertical axis, whichever has the larger delta so far.
  - **Ctrl (Win) / Cmd (Mac)** pressed after the drag starts toggles snapping for the duration of the drag (AE-UI-359).
  - 3D layers: dragging the layer (not a gizmo axis) moves it in the plane parallel to the view (UNVERIFIED: AE moves in the view plane for free drag; along an axis when an axis arrow is dragged, AE-UI-312).
  - The Info panel shows the layer Position (and the pointer delta) while dragging. The viewer updates live; with Live Update off, only the bounding box outline moves until release (UNVERIFIED for AE 2024+, where Live Update is always on for viewers).
  - One drag = one undo step ("Change Position" / "更改位置" UNVERIFIED undo name).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim, bae-model)
- **Notes:** Dragging a layer over the boundary of the viewer auto-scrolls the view (UNVERIFIED). Alt-drag does not duplicate layers in AE (use Ctrl+D). Implementation hint: during drag send uncommitted "preview" values to the evaluator and commit a single Command on release.

### AE-UI-303 · Selection tool: scale layers with layer handles (使用图层手柄缩放)

- **What it does:** Dragging one of the eight layer handles of a selected layer changes its Scale.
- **Exact semantics:**
  - Corner handle: scales X and Y freely (each axis follows the pointer independently). Side handle: scales only the axis perpendicular to that side.
  - **Shift-drag** any handle: scale proportionally, constrained to the footage frame aspect ratio (the ratio of Scale X/Scale Y at drag start is kept; for a side handle Shift makes the other axis follow).
  - Scale is applied about the layer's Anchor Point (Position is not changed), so the handle under the pointer follows the pointer only if the anchor is at the opposite edge; the scale factor is computed as `newScale = oldScale * (distance(anchorScreen, pointer) projected on the axis) / (distance(anchorScreen, handleStart) projected on the axis)`, sign included (dragging across the anchor flips to negative scale).
  - Ctrl/Cmd during a handle drag toggles snapping (AE-UI-359). Selection tool has no rotate-near-corner behaviour (use the Rotation tool, AE-UI-309).
  - Multiple selected layers: only the layer whose handle is dragged changes (UNVERIFIED: AE applies the same percentage change to all selected layers).
  - Double-clicking the **Selection tool button** in the Tools panel resets Scale of selected layers to 100%.
  - 3D layers: handles drag scale along the layer's local X/Y (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** The same handles appear for masks/paths only in Free Transform mode (AE-UI-306). Lottie has no equivalent (authoring only).

### AE-UI-304 · Nudge layers and points with the keyboard (使用箭头键微移)

- **What it does:** Arrow keys move selected layers (or selected mask/path points, or tracker regions) in space; Alt/Opt + Page Up/Page Down move selected layers in time.
- **Exact semantics:**
  - | Action | Windows | macOS | Amount |
    |---|---|---|---|
    | Move selected layers (Position) | Arrow key | Arrow key | 1 screen pixel at current magnification |
    | Move selected layers | Shift+Arrow | Shift+Arrow | 10 screen pixels at current magnification |
    | Move selected mask/path points | Arrow / Shift+Arrow | Arrow / Shift+Arrow | 1 / 10 px at current magnification |
    | Move anchor point (Layer panel, View: Anchor Point Path) | Arrow / Shift+Arrow | Arrow / Shift+Arrow | 1 / 10 px at Layer-panel magnification |
    | Move selected layers 1 frame earlier/later | Alt+PgUp / Alt+PgDn | Opt+PgUp / Opt+PgDn | 1 frame |
    | Move selected layers 10 frames earlier/later | Alt+Shift+PgUp / PgDn | Opt+Shift+PgUp / PgDn | 10 frames |
    | Move selected keyframes 1 frame later/earlier | Alt+Right / Alt+Left | Opt+Right / Opt+Left | 1 frame |
    | Move selected keyframes 10 frames | Alt+Shift+Right / Left | Opt+Shift+Right / Left | 10 frames |
    | Rotation (Z) of selected layers | numpad + / − | numpad + / − | ±1° (Shift: ±10°) |
    | Opacity (light: Intensity) | Ctrl+Alt+numpad + / − | Ctrl+Opt+numpad + / − | ±1% (Shift: ±10%) |
    | Scale | Ctrl+numpad + / − or Alt+numpad + / − | Cmd+numpad + / − or Opt+numpad + / − | ±1% (Shift: ±10%) |
  - "1 pixel at current magnification": comp-space delta = `1 / magnification` (at 200% zoom one press moves 0.5 px; at 50% it moves 2 px).
  - Arrow keys act on the Composition viewer when it (or the Timeline) has focus and layers are selected; in a focused numeric field they change the value instead (AE-UI-012 in ui-a).
  - Each key press is one undoable change and keys at current time when the stopwatch is on.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Time moves of layers keep relative timing of the selection. Keyframe nudges: see transform-keyframes-expressions.md, Keyframes.

### AE-UI-305 · Selection tool modifiers in shape layers (形状图层中的选取工具修饰键)

- **What it does:** With the Selection tool over a shape layer, holding Alt/Opt temporarily turns it into the Shape Duplication tool and holding Ctrl/Cmd into the Direct Selection tool.
- **Exact semantics:**
  - **Alt (Win) / Opt (Mac) + drag** a selected shape (group) in a shape layer: duplicates the shape group (new group inserted above the original inside the same parent group) and moves the duplicate.
  - **Ctrl (Win) / Cmd (Mac) + click**: Direct Selection — selects the individual shape/path under the pointer inside nested groups, bypassing group-level selection.
  - Plain click on a shape layer selects the layer; a second click (or double-click) drills into groups (UNVERIFIED exact drill-in rule); double-clicking a path enters path editing (vertices shown).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-vector)
- **Notes:** Shape data and path editing semantics: see shapes.md, Shape tools and Bezier model.

### AE-UI-306 · Double-click actions in viewers (查看器中的双击操作)

- **What it does:** Double-clicking with the Selection tool opens the "inside" of the clicked thing: text editing, path editing, mask free transform or the nested composition.
- **Exact semantics:**
  - | Double-click target (Selection tool) | Result |
    |---|---|
    | Text layer in Composition viewer | Switch to the matching Type tool and select all text of the layer (text edit mode) |
    | Mask path (vertex or segment) | Enter mask **Free Transform** mode: a bounding box with 8 handles + anchor around the selected points (all points if none selected); Esc or Enter exits |
    | Shape path | Path editing (vertices); with whole path selected, Free Transform Points (UNVERIFIED) |
    | Precomp layer | Open the source composition (in its own Composition/Timeline tab); Alt/Opt-double-click opens the precomp layer in the Layer panel |
    | Footage/solid layer | Open the layer in the Layer panel; Alt/Opt-double-click opens its source in the Footage panel |
    | Layer with Roto Brush / Paint strokes | Open in the Layer panel (paint/roto happen there) |
  - Free Transform (Ctrl+T / Cmd+T with mask or Path property selected in the Timeline also enters it): drag inside box = move points; handles = scale points (Shift proportional, Ctrl/Cmd-drag scales around the center point); outside box = rotate points (Shift = 45° steps, UNVERIFIED); the transform anchor can be dragged. Esc exits.
  - Double-clicking **tool buttons** in the Tools panel: Selection → reset Scale to 100% for selected layers; Rotation → reset Rotation to 0°; Hand → center the composition in the viewer; Zoom → magnification 100%; Pan Behind → reset Anchor Point to default (layer center); Alt/Opt-double-click Pan Behind → reset anchor and move the layer to the comp center; Ctrl/Cmd-double-click Pan Behind → center anchor point in the visible content (= Ctrl+Alt+Home); Rectangle/Ellipse (or other shape tool) → add a mask/shape that fills the selected layer's bounds (comp frame for shape layers, UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Mask Free Transform data edit is a bae-model Command on Mask Path values; see compositing-3d-color.md, Masks and shapes.md, Bezier model.

### AE-UI-307 · Hand tool (手形工具)

- **What it does:** Pans the view of a viewer (or Timeline/Graph Editor) by dragging.
- **Exact semantics:**
  - Activation: H (latch), hold **Spacebar** (momentary, any tool, pointer over a viewer) or hold the **middle mouse button** and drag.
  - Dragging scrolls the viewer content by the pointer delta (1:1 in screen pixels); the comp may be panned beyond its edges into the pasteboard.
  - Spacebar conflict with preview: a Spacebar press-and-release without dragging starts/stops preview; Spacebar held and dragged pans and does not start preview (UNVERIFIED: AE decides on drag movement, not timing).
  - Double-click the Hand tool button: center the composition in the viewer.
  - In the Timeline/Graph Editor the Hand tool (and Spacebar-drag, UNVERIFIED) scrolls time/value.
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Mouse-wheel scrolling/zooming is ui-a (AE-UI-011).

### AE-UI-308 · Zoom tool (缩放工具)

- **What it does:** Changes viewer magnification around the clicked point.
- **Exact semantics:**
  - Z activates Zoom In; holding Alt (Win) / Opt (Mac) switches to Zoom Out while held. Click = zoom in one step centered at the click point; Alt/Opt-click = zoom out one step. Drag (UNVERIFIED: AE zoom tool has no marquee zoom; a drag behaves like click).
  - Steps follow the magnification menu list: 1.5%, 3.1%, 6.25%, 12.5%, 25%, 33.3%, 50%, 66.7%, 100%, 200%, 300%, 400%, 800%, 1600%, 3200%, 6400%. Minimum 1.5%, maximum 6400% (UNVERIFIED minimum/maximum).
  - Keyboard equivalents (any tool): `.` (period, main keyboard) zoom in; `,` (comma) zoom out; `/` (main keyboard) 100%; Shift+/ fit; Alt+/ (Opt+/) zoom up to 100% to fit. Double-click Zoom tool button = 100%.
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Magnification menu, Fit, "Fit up to 100%": see ui-a, Composition viewer. Keyboard zoom centres on the view center (UNVERIFIED; mouse-wheel zoom centres on the pointer).

### AE-UI-309 · Rotation tool (旋转工具)

- **What it does:** W activates the Rotation tool; dragging rotates selected layers around their anchor points.
- **Exact semantics:**
  - Drag angle = signed angle swept by the pointer around the layer's anchor point in screen space, applied as a delta to Rotation (2D layers) or to Orientation or X/Y/Z Rotation (3D layers, per the **Set** option).
  - **Shift-drag** snaps to 15° increments (Rotation or Orientation).
  - Tool option for 3D layers: **Set: Orientation / Rotation** (设置：方向 / 旋转) — chooses which property a free drag on a 3D layer changes. UNVERIFIED default: Orientation.
  - Rotation is not wrapped: dragging multiple full turns accumulates (e.g. 1x+45°).
  - Double-click the Rotation tool button resets Rotation of selected layers to 0°.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Rotation property representation (revolutions + degrees): see transform-keyframes-expressions.md, Transform properties.

### AE-UI-310 · Camera tools (摄像机工具)

- **What it does:** Tools that move the active camera layer or a working 3D view (Custom View, orthographic views) interactively.
- **Exact semantics:**
  - | Tool (中文) | Since | Action | Unified-tool mouse button |
    |---|---|---|---|
    | Unified Camera (统一摄像机工具) | CS6 | Combines the three below by mouse button | — |
    | Orbit Around Cursor (绕光标旋转（译名待核）) | AE 16.1 (2019) UNVERIFIED | Orbit around the point under the cursor | left button |
    | Orbit Around Scene (绕场景旋转（译名待核）) | AE 16.1 UNVERIFIED | Orbit around the scene center | — |
    | Orbit Around Camera POI (绕摄像机目标点旋转（译名待核）); older name Orbit Camera Tool (轨道摄像机工具) | CS3 | Orbit around the point of interest | — |
    | Pan Under Cursor (在光标下平移（译名待核）) | AE 16.1 UNVERIFIED | Translate so the point under the cursor follows the cursor | middle button |
    | Pan Camera POI (平移摄像机目标点（译名待核）); older name Track XY Camera (跟踪 XY 摄像机工具) | CS3 | Translate camera and POI horizontally/vertically in view space | — |
    | Dolly Towards Cursor (向光标方向推拉（译名待核）) | AE 16.1 UNVERIFIED | Move along the line to the cursor point | right button |
    | Dolly to Cursor (推拉至光标（译名待核）) | AE 16.1 UNVERIFIED | Move toward the cursor point, POI follows | — |
    | Dolly to Camera POI (推拉至摄像机目标点（译名待核）); older name Track Z Camera (跟踪 Z 摄像机工具) | CS3 | Move along the line to the POI; on orthographic views scales the view | — |
  - C activates the camera group and repeated C cycles Unified → Orbit → Pan (Track XY) → Dolly (Track Z) (the variant last chosen in each sub-group's flyout is used).
  - Shift-drag with Unified Camera = Orbit constrained to one axis. Drags may continue outside the panel once started.
  - The tool edits the camera that the active 3D view shows: the active camera layer's Position and Point of Interest (one-node cameras: Position and Orientation), creating keyframes if stopwatches are on; on working 3D views (Custom View 1-3, Front/Left/Top/Back/Right/Bottom) no layer property changes and View > Reset 3D View restores the view. Orbit is unavailable on fixed orthographic views.
  - Camera-tool shortcuts: F = move camera and POI to look at selected 3D layers; Ctrl+Shift+F / Cmd+Shift+F = look at all 3D layers; Ctrl+Alt+Shift+\ / Cmd+Opt+Shift+\ = look at selected layers (any tool).
- **Priority:** P1 (P0 for Unified Camera on a camera layer)
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Scripting enum only knows `Tool_CameraMaya`, `Tool_CameraOrbit`, `Tool_CameraTrackXY`, `Tool_CameraTrackZ`. Camera layer data: see compositing-3d-color.md, Cameras. Preference "Camera navigation: Z-depth sensitivity" etc. are ui-c (Preferences).

### AE-UI-311 · 3D camera navigation tools and keys (3D 摄像机导航工具（译名待核）)

- **What it does:** In the 3D Design Space (AE 22.x, 2022, UNVERIFIED version) the Tools panel shows Orbit, Pan and Dolly navigation buttons that work with any tool selected via number keys or modifiers.
- **Exact semantics:**
  - | Control | Windows | macOS |
    |---|---|---|
    | Switch to Orbit camera control | 1 | 1 |
    | Switch to Pan camera control | 2 | 2 |
    | Switch to Dolly camera control | 3 | 3 |
    | Return to previous 3D view | Esc | Esc |
    | Switch to 3D view 1 / 2 / 3 (defaults Front / Custom View 1 / Active Camera) | F10 / F11 / F12 | F10 / F11 / F12 |
  - The navigation controls move the active 3D view or camera like the matching camera tool (AE-UI-310), using the default variant (UNVERIFIED: "around cursor").
  - Number keys 1-3 conflict with Clone Stamp preset keys only while a paint tool is active in the Layer panel.
- **Priority:** P2
- **Owner:** bae-ui-motion
- **Notes:** UNVERIFIED: AE also offers temporary navigation by Alt+left/middle/right mouse drag in 3D views (Maya-style) — check before implementing. 3D views menu: see compositing-3d-color.md, Cameras / 3D views and ui-a, Composition viewer.

### AE-UI-312 · 3D transform gizmo (3D 变换小工具（译名待核）)

- **What it does:** Selected 3D layers show an on-canvas gizmo with axis arrows, scale cubes and rotation arcs to move, scale and rotate along a single axis.
- **Exact semantics:**
  - Gizmo modes: Universal (all three combined, V while the Selection tool is active), Position (4), Scale (5), Rotation (6); Win and Mac identical.
  - Drag an arrow: Position changes along that axis only (axis per Axis Mode, AE-UI-313). Drag a scale cube: Scale along that axis. Drag a rotation arc: X/Y/Z Rotation (or Orientation, per the Rotation tool Set option). Shift snaps rotation to 15° (UNVERIFIED for gizmo).
  - Multiple selected 3D layers share one gizmo placed at the selection's average anchor (AE 22.x "Single 3D gizmo for multiple 3D layers", UNVERIFIED version).
  - Gizmo colours: X red, Y green, Z blue (AE convention).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Older AE (before 3D design space) showed only three axis arrows on the selected 3D layer; dragging an arrow constrained movement. Shift+drag along a gizmo axis: UNVERIFIED.

### AE-UI-313 · Axis modes: Local, World, View (本地轴模式 / 世界轴模式 / 视图轴模式)

- **What it does:** Chooses the coordinate frame in which the 3D axis arrows / gizmo are displayed and in which axis-constrained drags happen.
- **Exact semantics:**
  - | Mode (中文) | Axes aligned to | Scripting |
    |---|---|---|
    | Local Axis Mode (本地轴模式) | The layer's own orientation (after parenting) | — |
    | World Axis Mode (世界轴模式) | Composition world X/Y/Z, regardless of layer rotation | — |
    | View Axis Mode (视图轴模式) | The current 3D view (screen X/Y and view depth) | — |
  - Buttons appear in the Tools panel options when a tool that can act on 3D layers is active (Selection, Rotation, camera-related, Pan Behind; UNVERIFIED list). Default: Local.
  - Axis mode changes only how drags are mapped; camera tools always act along the view's local axes and are unaffected.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Keyboard shortcuts: none by default (UNVERIFIED).

### AE-UI-314 · Pan Behind (Anchor Point) tool (向后平移（锚点）工具)

- **What it does:** Y activates the tool; dragging moves a selected layer's anchor point without moving the layer in the comp (compensating Position), or with Alt/Opt changes Anchor Point only.
- **Exact semantics:**
  - Drag on a selected layer's anchor point in the Composition viewer: Anchor Point changes by the drag delta mapped to layer space; Position changes by the matching amount mapped to parent space so that the layer's rendered pixels stay put (`ΔPosition = M_linear(layer) · ΔAnchor`, where M_linear is the layer's scale/rotation/orientation; see transform-keyframes-expressions.md, Transform matrix order).
  - **Alt-drag (Win) / Opt-drag (Mac)**: only Anchor Point changes; the layer content shifts.
  - In the **Layer panel** (View: Anchor Point Path), dragging the anchor moves only Anchor Point; with Pan Behind in the Layer panel the layer image moves relative to masks (masks are moved in relation to the layer).
  - Dragging a layer (not its anchor) with Pan Behind in the Composition viewer pans the layer content behind its masks (Anchor Point and Position change together so that masks stay fixed in comp space, UNVERIFIED exact compensation).
  - Ctrl/Cmd during drag: snapping toggle (AE-UI-359). Double-click button actions: see AE-UI-306. Shortcut Ctrl+Alt+Home / Cmd+Opt+Home: center anchor point in visible content.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** The compensating edit must be one Command (Anchor + Position) so undo is atomic. When Position/Anchor are animated, AE writes keyframes at the current time only for properties with stopwatch on; the compensation is computed at the current time.

### AE-UI-315 · Shape and mask tools (形状工具和蒙版工具)

- **What it does:** Q cycles Rectangle, Rounded Rectangle, Ellipse, Polygon and Star; dragging creates a parametric shape in a shape layer or a mask on a layer depending on selection and the Tools panel mode.
- **Exact semantics:**
  - **Target rule** at drag start: (a) a shape layer is selected and Tool Creates Shape is on → add a shape group to that layer; (b) a non-shape layer that can have masks (footage, solid, text, precomp, adjustment) is selected → create a mask on it (Tool Creates Mask); (c) no layer selected → create a new shape layer named "Shape Layer N" (形状图层 N) with the shape. Tools panel buttons **Tool Creates Shape (工具创建形状)** / **Tool Creates Mask (工具创建蒙版)** appear when a shape layer is selected and choose between (a) and a mask on the shape layer.
  - Options shown for shape creation: Fill (填充) and Stroke (描边) type/colour, Stroke Width (描边宽度) in px, **Bezier Path (贝塞尔曲线路径)** checkbox (create a plain path instead of a parametric primitive).
  - Modifiers while dragging (Win/Mac identical except Ctrl/Cmd): Shift = square/circle/zero-rotation polygon/star; Spacebar = reposition the shape during the drag; Up/Down Arrow = star/polygon points ±1 or rounded-rect roundness ±; Left/Right Arrow = rounded-rect roundness 0 / max, polygon/star outer roundness −/+; Page Up/Down = star inner roundness ±; Ctrl (Win) / Cmd (Mac) = change star outer radius only (inner radius stays). Ctrl (Win)/Cmd (Mac) for rectangle/ellipse = draw from center (UNVERIFIED for shapes; for masks AE uses Ctrl-drag = from center).
  - Double-click the tool button: create a shape/mask fitting the layer/comp bounds (AE-UI-306).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-vector, bae-motion)
- **Notes:** Primitive parameters and defaults (Rectangle Size, Roundness, Star Points, radii): see shapes.md, Shape tools and Primitives. Masks created by tools: see compositing-3d-color.md, Masks. Lottie: rectangle `ty: "rc"`, ellipse `"el"`, polystar `"sr"` (`sy: 1` star, `2` polygon).

### AE-UI-316 · Pen tool group (钢笔工具组)

- **What it does:** Draws and edits Bezier mask/shape paths: Pen, Add Vertex, Delete Vertex, Convert Vertex and Mask Feather tools (G).
- **Exact semantics:**
  - | Tool (中文) | Action |
    |---|---|
    | Pen (钢笔工具) | Click = corner vertex; click-drag = smooth vertex with symmetric tangents; click the first vertex = close path; same target rule as shape tools (AE-UI-315) |
    | Add Vertex (添加"顶点"工具) | Click a segment to insert a vertex without changing the curve shape (de Casteljau split) |
    | Delete Vertex (删除"顶点"工具) | Click a vertex to delete it; neighbouring segment re-fits to the remaining tangents |
    | Convert Vertex (转换"顶点"工具) | Click a smooth vertex = make corner (retract handles); drag a corner vertex = pull out new smooth handles; drag a handle = break handle symmetry |
    | Mask Feather (蒙版羽化工具) | Click a mask segment to add a variable-feather point; drag it to set feather width (AE CS6+) |
  - Modifiers: with Pen active, Ctrl (Win) / Cmd (Mac) = temporary Selection tool; with Selection active and pointer over a path, Ctrl+Alt (Win) / Cmd+Opt (Mac) = Pen-family tool depending on target (Add Vertex over a segment, Convert Vertex over a vertex); Alt (Win)/Opt (Mac) with Pen over a vertex = Convert Vertex; Shift while dragging a tangent constrains to 45° (UNVERIFIED).
  - Ctrl+Alt-click (Win) / Cmd+Opt-click (Mac) a vertex toggles smooth/corner; Ctrl+Alt-drag / Cmd+Opt-drag redraws its handles.
  - **RotoBezier (RotoBezier)** checkbox: new paths get auto-computed tangents (tension per vertex) instead of manual handles.
  - G cycles Pen ↔ Mask Feather (preference can turn this off so G keeps the Pen).
- **Priority:** P0 (Pen, Add/Delete/Convert); Mask Feather P1
- **Owner:** bae-ui-motion (+ bae-vector)
- **Notes:** Bezier vertex/tangent model, RotoBezier math and variable mask feather rendering: see shapes.md, Bezier model and compositing-3d-color.md, Masks.

### AE-UI-317 · Type tools (文字工具)

- **What it does:** Ctrl+T / Cmd+T activates and cycles Horizontal Type (横排文字工具) and Vertical Type (直排文字工具); click creates point text, drag creates paragraph text, click inside an existing text layer edits it.
- **Exact semantics:**
  - Click on empty canvas: new text layer with point text at the click position (insertion point blinks). Drag: new paragraph text with a bounding box of the dragged rectangle (Alt/Opt-drag from center, UNVERIFIED). Click on an existing text layer's text: edit that layer at the click point (no new layer).
  - While editing: Esc or Enter on the numeric keypad commits and leaves edit mode (layer stays selected); clicking another tool commits; Ctrl/Cmd temporarily gives the Selection tool to move the layer (UNVERIFIED).
  - Click counts with the Type tool: double = word, triple = line, quadruple = paragraph, quintuple = whole text. Text-editing keys (arrows, Ctrl+arrows, Shift-extend, Home/End) per AE-UI-327.
  - Auto-Open Panels option opens Character and Paragraph panels when the tool is chosen.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-text)
- **Notes:** Point vs paragraph text data and layout: see text.md, Point and paragraph text.

### AE-UI-318 · Paint tools: Brush, Clone Stamp, Eraser (绘画工具：画笔、仿制图章、橡皮擦)

- **What it does:** Ctrl+B / Cmd+B activates and cycles Brush (画笔工具), Clone Stamp (仿制图章工具) and Eraser (橡皮擦工具); strokes are painted in the Layer panel as vector paint strokes on the layer's Paint effect.
- **Exact semantics:**
  - Painting happens only in the Layer panel; using a paint tool in the Composition viewer opens the layer in the Layer panel (UNVERIFIED: AE shows a prompt/opens it automatically).
  - Modifiers (Win/Mac): Ctrl-drag / Cmd-drag = set brush size, release Ctrl/Cmd while dragging then continue = set hardness; Shift at stroke start = join to previous stroke; Alt-click / Opt-click = sample foreground colour (Brush) or set clone source (Clone Stamp); Ctrl+Alt-click / Cmd+Opt-click = average colour of 4×4 px; Ctrl+Shift / Cmd+Shift held = momentary Eraser with Last Stroke Only; Alt+Shift-drag / Opt+Shift-drag (Clone Stamp) = show and move the source overlay; numpad digit = opacity (1 = 10% … 9 = 90%, numpad . = 100%); Shift+numpad digit = flow (Shift+numpad . = 100%); X swap foreground/background colours; D reset to black/white; 3-7 on main keyboard = Clone Stamp presets; Ctrl+PgUp/PgDn (Cmd) or 1/2 on main keyboard = move by stroke Duration frames.
  - Paint panel settings (Opacity, Flow, Mode, Channels, Duration: Constant / Write On / Single Frame / Custom) belong to ui-a, Paint panel.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Paint strokes render via the Paint effect (see effects.md if covered; otherwise P1 gap to be owned by bae-effects). Brushes panel tips: ui-a.

### AE-UI-319 · Roto Brush and Refine Edge tools (Roto 笔刷工具和优化边缘工具)

- **What it does:** Alt+W / Opt+W activates and toggles Roto Brush (Roto 笔刷工具) and Refine Edge (优化边缘工具) for interactive foreground segmentation in the Layer panel.
- **Exact semantics:**
  - Drag = add foreground stroke (green); Alt-drag (Win) / Opt-drag (Mac) = background stroke (red); Ctrl-drag / Cmd-drag = brush size. Strokes on a base frame propagate forward/backward over a span; J/K jump to span begin/end/base frame in the Layer panel.
  - Refine Edge strokes mark hair/fuzzy borders for edge refinement; Alt+X / Opt+X toggles Refine Edge X-ray. Alt+5 / Opt+5 alpha boundary, Alt+6 / Opt+6 alpha overlay in the Layer panel.
  - Result is the Roto Brush & Refine Edge effect on the layer. AE 2020 (17.5) introduced Roto Brush 2 (machine-learning propagation), AE 23.0 "Roto Brush 3 / Next-gen" (UNVERIFIED version names).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** BAE decision: ML segmentation is out of scope for M1; reserve the tool slot and effect matchName `ADBE Samurai` (UNVERIFIED matchName).

### AE-UI-320 · Puppet tools (人偶工具)

- **What it does:** Ctrl+P / Cmd+P activates and cycles Puppet Position Pin (人偶位置控点工具), Puppet Starch Pin (人偶固化控点工具（译名待核）), Puppet Bend Pin (人偶弯曲控点工具（译名待核）), Puppet Advanced Pin (人偶高级控点工具（译名待核）) and Puppet Overlap Pin (人偶重叠控点工具（译名待核）); clicking a layer creates a triangle mesh and pins that deform it.
- **Exact semantics:**
  - First click inside the layer's alpha outline creates a Mesh (Puppet effect added) and a pin; later clicks add pins; dragging a pin moves its Position (keyframed if animation is recorded). Bend pins carry Rotation and Scale; Advanced pins carry Position, Rotation, Scale.
  - Tool options: Mesh: **Show** (显示) checkbox, **Expansion (扩展)** default 3 px (UNVERIFIED), **Density (密度)** default 10 (UNVERIFIED); Record Options (motion sketch of pins with Ctrl/Cmd-drag: speed default 100%, smoothing default 3, UNVERIFIED).
  - Ctrl-drag (Win) / Cmd-drag (Mac) a pin = record (sketch) its motion in real time from the current time. Bend/Advanced pins: rotation and scale handles around the pin.
  - AE CC 2018 (15.0) added Advanced and Bend pins (UNVERIFIED version); Puppet engine "Legacy" vs "Advanced" choice in the effect.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects, bae-anim)
- **Notes:** Puppet effect matchName `ADBE FreePin3` (UNVERIFIED). Mesh deformation math (as-rigid-as-possible) is not in sibling files; treat as P2 gap.

### AE-UI-321 · 3D primitive tools (3D 基本体工具（译名待核）)

- **What it does:** AE 2026 adds parametric 3D mesh tools (Cube Tool / 立方体工具 and siblings) that create parametric mesh layers in the Advanced 3D renderer.
- **Exact semantics:**
  - UNVERIFIED: group contains Cube, Sphere, Cylinder, Cone, Torus, Plane (from the AE 2026 parametric-mesh feature); drag creates a parametric mesh layer sized by the drag.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Scripting: ParametricMesh object (AE 26.0, UNVERIFIED). Advanced 3D/meshes are P2 in compositing-3d-color.md.
