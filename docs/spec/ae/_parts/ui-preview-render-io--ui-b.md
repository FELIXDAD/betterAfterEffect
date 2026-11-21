## Tools

### AE-UI-300 · Tool activation, momentary tools and tool cycling (激活工具和循环切换工具)

- **What it does:** Defines how a tool becomes active: clicking its button in the Tools panel (toolbar), tapping its single-key shortcut, holding the shortcut for a momentary (spring-loaded) switch, repeated presses to cycle through a tool group, and Alt/Opt-click on a group button.
- **Exact semantics:**
  - **Tool groups.** Tools that share a button form a group; the button shows the most recently used member and a small triangle; press-and-hold (or click the triangle) opens a flyout listing every member with its shortcut. Groups (AE 26.x toolbar order, left to right, after the Home button (主页); UNVERIFIED exact position of the camera groups): Selection; Hand; Zoom; camera tools as three groups Orbit / Pan / Dolly (AE 17.5 (October 2020) and later; before that one group Unified Camera / Orbit Camera / Track XY Camera / Track Z Camera; see AE-UI-310); Rotation; Pan Behind; Shape (Rectangle, Rounded Rectangle, Ellipse, Polygon, Star); Pen (Pen, Add Vertex, Delete Vertex, Convert Vertex, Mask Feather); Type (Horizontal, Vertical); Brush; Clone Stamp; Eraser; Roto Brush (Roto Brush, Refine Edge); Puppet (Position, Starch, Bend, Advanced, Overlap pins); 3D primitive group (Cube … , AE 26.x, AE-UI-321).
  - **Tap vs hold.** Pressing a single-letter tool shortcut and releasing it immediately makes that tool the active tool. Holding the key down activates it **momentarily**; on key release the previously active tool returns (Adobe help, "Activate tools" note). Implementation rule (BAE decision, AE's exact rule UNVERIFIED): revert on key-up if a pointer press happened while the key was held or the key was held longer than ~250 ms; otherwise latch.
  - **Cycling.** Pressing a group shortcut while a member of that group is already active advances to the next member of the group in group order and wraps (Q: Rectangle → Rounded Rectangle → Ellipse → Polygon → Star → Rectangle). C activates and cycles the camera tools (help text: Unified Camera → Orbit Camera → Track XY Camera → Track Z Camera; in AE 17.5+ the Orbit → Pan → Dolly groups, each using the variant last chosen in its flyout; see AE-UI-310); G cycles Pen ↔ Mask Feather only (Add/Delete/Convert Vertex are reached through the flyout or modifiers), and a preference turns the G cycling off; Ctrl+T / Cmd+T cycles Horizontal ↔ Vertical Type; Ctrl+B / Cmd+B cycles Brush → Clone Stamp → Eraser; Ctrl+P / Cmd+P cycles the Puppet tools; Alt+W / Opt+W toggles Roto Brush ↔ Refine Edge. Alt-click (Win) / Opt-click (Mac) on a group button in the Tools panel also advances to the next member. No Shift+key cycling exists in AE (UNVERIFIED: no "Use Shift Key for Tool Switch" preference like Photoshop).
  - **Context availability.** Some tools are disabled when they cannot act: camera tools need a 3D context (the help page says "only when the active composition contains a camera layer"; in AE 17.5+ a comp with any 3D layer also enables them because a default camera view exists, UNVERIFIED exact rule), paint tools and Roto Brush operate only in the Layer panel, Puppet tools need a selected layer with pixels. Selecting a disabled tool's shortcut does nothing.
  - **Tool options area.** The rest of the Tools panel row shows options of the active tool (for example Rotation: Set Orientation/Rotation; shape tools: Tool Creates Shape/Mask, Fill, Stroke, Bezier Path; Pen: RotoBezier; Type, Brush, Clone Stamp, Eraser: Auto-Open Panels (自动打开面板); Puppet: Mesh Expansion/Density, Record Options; 3D-capable tools: axis mode buttons (AE-UI-313); Selection: Snapping (AE-UI-357)).
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
- **Notes:** Layer-selection keyboard commands (Ctrl+Up/Down, numpad digits, Ctrl+A, F2) are in AE-UI-324 and AE-UI-334. Shape-layer modifiers (Alt = shape duplication, Ctrl/Cmd = direct selection) are AE-UI-305. Implementation hint: GPU picking with an ID buffer (one u32 layer index per pixel, rendered at viewer resolution) or CPU hit-testing on cached alpha at preview resolution; bounding-box hit for nulls/text.

### AE-UI-302 · Selection tool: move layers by dragging (使用选取工具拖动图层)

- **What it does:** Dragging a selected layer (or an unselected layer, which first becomes the sole selection) moves all selected layers in the viewer by editing Position.
- **Exact semantics:**
  - Drag delta in screen pixels is converted to comp pixels (divide by viewer magnification, account for pixel aspect ratio of the comp) and then to each layer's parent space; the Position of each selected layer changes by that delta (for parented layers Position is in parent space, so the delta is transformed by the inverse of the parent's world matrix, linear part only).
  - If Position has its stopwatch on, the drag sets/creates a keyframe at the current time (like any value change; see transform-keyframes-expressions.md, Keyframes). If Position is separated (Separate Dimensions), X/Y (and Z) Position properties are edited instead.
  - **Shift** (pressed after the drag starts) constrains movement to the horizontal or vertical axis, whichever has the larger delta so far.
  - **Ctrl (Win) / Cmd (Mac)** pressed after the drag starts toggles snapping for the duration of the drag (AE-UI-357).
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
  - **Shift-drag** any handle: scale proportionally; Adobe's wording is "Modify Scale, constrained to footage frame aspect ratio" (keyboard-shortcuts reference). For a side handle Shift makes the other axis follow. UNVERIFIED: when Scale X ≠ Scale Y at drag start, whether AE keeps that ratio or snaps back to the footage frame aspect (BAE: keep the start ratio).
  - Scale is applied about the layer's Anchor Point (Position is not changed), so the handle under the pointer follows the pointer only if the anchor is at the opposite edge; the scale factor is computed as `newScale = oldScale * (distance(anchorScreen, pointer) projected on the axis) / (distance(anchorScreen, handleStart) projected on the axis)`, sign included (dragging across the anchor flips to negative scale).
  - Ctrl/Cmd during a handle drag toggles snapping (AE-UI-357). Selection tool has no rotate-near-corner behaviour (use the Rotation tool, AE-UI-309).
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
  - Steps follow the fixed magnification levels of the viewer's magnification menu (list, Smooth Zoom and limits: ui-a AE-UI-024 Magnification; do not duplicate here). With Smooth Zoom (AE 25.x) the tool steps still go to the next fixed level (UNVERIFIED).
  - Keyboard equivalents (any tool, from the shortcuts reference): `.` (period, main keyboard) zoom in; `,` (comma) zoom out; `/` (main keyboard) 100%; Shift+/ zoom to fit; Alt+/ (Win) / Opt+/ (Mac) zoom up to 100% to fit. Double-click Zoom tool button = 100%.
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
    | Unified Camera (统一摄像机工具（译名待核）) | before AE 17.5 | Combines Orbit / Track XY / Track Z by mouse button | left = Orbit, middle = Track XY, right = Track Z |
    | Orbit Around Cursor (绕光标旋转（译名待核）) | AE 17.5 (October 2020) | Orbit around the point under the cursor | — |
    | Orbit Around Scene (绕场景旋转（译名待核）) | AE 17.5 | Orbit around the scene center | — |
    | Orbit Around Camera POI (绕摄像机目标点旋转（译名待核）); older name Orbit Camera Tool (轨道摄像机工具（译名待核）) | CS3 | Orbit around the point of interest | — |
    | Pan Under Cursor (在光标下平移（译名待核）) | AE 17.5 | Translate so the point under the cursor follows the cursor | — |
    | Pan Camera POI (平移摄像机目标点（译名待核）); older name Track XY Camera (跟踪 XY 摄像机工具（译名待核）) | CS3 | Translate camera and POI horizontally/vertically in view space | — |
    | Dolly Towards Cursor (向光标方向推拉（译名待核）) | AE 17.5 | Move along the line to the cursor point | — |
    | Dolly to Cursor (推拉至光标（译名待核）) | AE 17.5 | Move toward the cursor point, POI follows | — |
    | Dolly to Camera POI (推拉至摄像机目标点（译名待核）); older name Track Z Camera (跟踪 Z 摄像机工具（译名待核）) | CS3 | Move along the line to the POI; on orthographic views scales the view | — |
  - The cursor-based variants, the 1/2/3 navigation keys and Alt+mouse-button navigation arrived with the 3D Design Space in the October 2020 release (Adobe labels it version 17.5); community reports show C then cycling Orbit Around Cursor → Pan Under Cursor → Dolly Towards Cursor (the variant last chosen in each flyout is used). UNVERIFIED: whether the Unified Camera tool is still offered in AE 26.x (the scripting enum still has `Tool_CameraMaya`; the help page text still names the old four tools).
  - Shift-drag with Unified Camera = Orbit constrained to one axis. Drags may continue outside the panel once started.
  - The tool edits the camera that the active 3D view shows: the active camera layer's Position and Point of Interest (one-node cameras: Position and Orientation), creating keyframes if stopwatches are on; on working 3D views (Custom View 1-3, Front/Left/Top/Back/Right/Bottom) no layer property changes and View > Reset 3D View restores the view. Orbit is unavailable on fixed orthographic views.
  - Camera-tool shortcuts: F = move camera and POI to look at selected 3D layers; Ctrl+Shift+F / Cmd+Shift+F = look at all 3D layers; Ctrl+Alt+Shift+\ / Cmd+Opt+Shift+\ = look at selected layers (any tool).
- **Priority:** P1 (P0 for Unified Camera on a camera layer)
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Scripting enum only knows `Tool_CameraMaya`, `Tool_CameraOrbit`, `Tool_CameraTrackXY`, `Tool_CameraTrackZ`. Camera layer data: see compositing-3d-color.md, Cameras. Preference "Camera navigation: Z-depth sensitivity" etc. are ui-c (Preferences).

### AE-UI-311 · 3D camera navigation tools and keys (3D 摄像机导航工具（译名待核）)

- **What it does:** In the 3D Design Space (October 2020 release, Adobe-labelled version 17.5) camera navigation is spring-loaded: number keys 1/2/3 or Alt/Opt + left/middle/right mouse button orbit, pan and dolly the active 3D view or camera with any tool selected.
- **Exact semantics:**
  - | Control | Windows | macOS |
    |---|---|---|
    | Switch to Orbit camera control | 1 | 1 |
    | Switch to Pan camera control | 2 | 2 |
    | Switch to Dolly camera control | 3 | 3 |
    | Return to previous 3D view | Esc | Esc |
    | Switch to 3D view 1 / 2 / 3 (defaults Front / Custom View 1 / Active Camera) | F10 / F11 / F12 | F10 / F11 / F12 |
  - Hold Alt (Win) / Opt (Mac) and drag with the left / middle / right mouse button = Orbit / Pan / Dolly (temporary, the current tool stays active).
  - The navigation controls move the active 3D view or camera like the matching camera tool (AE-UI-310), using the variant currently chosen in that tool's flyout (UNVERIFIED).
  - Number keys 1-3 conflict with paint-tool keys (1/2 = stroke-duration moves, 3-7 = Clone Stamp presets) only while a paint tool is active in the Layer panel.
- **Priority:** P2
- **Owner:** bae-ui-motion
- **Notes:** Source: Adobe feature summary for the October 2020 release, https://helpx.adobe.com/after-effects/desktop/using/whats-new/2021.html ("use the numbers (1, 2, 3) on your keyboard or hold down the option/alt key and use your mouse buttons (left, middle and right) to orbit, pan and dolly"). 3D views menu: see compositing-3d-color.md, Cameras / 3D views and ui-a, Composition viewer.

### AE-UI-312 · 3D transform gizmo (3D 变换小工具（译名待核）)

- **What it does:** Selected 3D layers show an on-canvas gizmo with axis arrows, scale cubes and rotation arcs to move, scale and rotate along a single axis.
- **Exact semantics:**
  - Gizmo modes: Universal (all three combined, V while the Selection tool is active), Position (4), Scale (5), Rotation (6); Win and Mac identical.
  - Drag an arrow: Position changes along that axis only (axis per Axis Mode, AE-UI-313). Drag a scale cube: Scale along that axis. Drag a rotation arc: X/Y/Z Rotation (or Orientation, per the Rotation tool Set option). Shift snaps rotation to 15° (UNVERIFIED for gizmo).
  - The gizmo arrived with the 3D Design Space (October 2020 release, labelled 17.5). Multiple selected 3D layers share one gizmo (Adobe help topic "Single 3D Gizmo for multiple 3D layers"; UNVERIFIED: placed at the selection's average anchor; version that added it).
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
  - **Pan a layer behind its mask:** clicking inside a mask area with Pan Behind and dragging moves the layer content while the mask stays fixed in the comp. Adobe: "the mask is moved in relation to its layer, and … the layer is moved in relation to the composition", i.e. Position changes by the drag delta (parent space) and every Mask Path on the layer is offset by the opposite delta in layer space. UNVERIFIED: whether Anchor Point is used instead of Position for the layer move.
  - Ctrl/Cmd during drag: snapping toggle (AE-UI-357). Double-click button actions: see AE-UI-306. Shortcut Ctrl+Alt+Home / Cmd+Opt+Home: center anchor point in visible content.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/animating-shape-paths-masks.html ("Pan a layer behind its mask") and the anchor-point section of the layer-properties help page (double-click / Alt-double-click / Ctrl-double-click of the tool button, AE-UI-306). The compensating edit must be one Command (Anchor + Position, or Position + Mask Paths) so undo is atomic. When Position/Anchor are animated, AE writes keyframes at the current time only for properties with stopwatch on; the compensation is computed at the current time.

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
  - Click counts with the Type tool: double = word, triple = line, quadruple = paragraph, quintuple = whole text. Text-editing keys (arrows, Ctrl+arrows, Shift-extend, Home/End) per AE-UI-340.
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
  - Result is the Roto Brush & Refine Edge effect on the layer. The October 2020 release (17.5) introduced Roto Brush 2 (Sensei machine-learning propagation); a later "next-generation" Roto Brush model followed (UNVERIFIED version, AE 24.x).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Effect matchName `ADBE Samurai` ("Roto Brush & Refine Edge", docsforadobe first-party matchnames). BAE decision: ML segmentation is out of scope for M1; reserve the tool slot and the matchName.

### AE-UI-320 · Puppet tools (人偶工具)

- **What it does:** Ctrl+P / Cmd+P activates and cycles Puppet Position Pin (人偶位置控点工具), Puppet Starch Pin (人偶固化控点工具（译名待核）), Puppet Bend Pin (人偶弯曲控点工具（译名待核）), Puppet Advanced Pin (人偶高级控点工具（译名待核）) and Puppet Overlap Pin (人偶重叠控点工具（译名待核）); clicking a layer creates a triangle mesh and pins that deform it.
- **Exact semantics:**
  - First click inside the layer's alpha outline creates a Mesh (Puppet effect added) and a pin; later clicks add pins; dragging a pin moves its Position (keyframed if animation is recorded).
  - Tool options: Mesh: **Show** (显示（译名待核）) checkbox, **Expansion (扩展（译名待核）)** default 3 px (Adobe help: "The default value of three pixels for Expansion"), **Density (密度（译名待核）)** (Advanced engine; replaces the Legacy engine's **Triangles** target count; default UNVERIFIED, 10); **Record Options (记录选项（译名待核）)** dialog: **Speed** (ratio of recorded to playback speed; 100% = real time, >100% plays back slower; default UNVERIFIED 100%) and **Smoothing** (higher removes more keyframes; default UNVERIFIED 3).
  - Ctrl-drag (Win) / Cmd-drag (Mac) a pin = record (sketch) its motion in real time from the current time. Bend/Advanced pins: an outer circle with a square handle; drag the circle = rotation (Shift = 15° steps), drag the square = scale (Shift = 5% steps). Bend pins compute their location from the surrounding pins (no Position control); Advanced pins control Position, Scale and Rotation. Pin colours: Position yellow, Starch red, Overlap blue. New Overlap pin In Front default 50.
  - Advanced puppet engine is the default since AE 15.1; Puppet Engine (Legacy / Advanced) is chosen per effect in the Timeline. Bend and Advanced pins arrived in AE 2019 (16.0) (UNVERIFIED: third-party preview, not Adobe release notes).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects, bae-anim)
- **Notes:** Puppet effect matchName `ADBE FreePin3` (docsforadobe first-party matchnames). Scripting tool enum: `Tool_Pin` (Position), `Tool_PinStarch`, `Tool_PinDepth` (Overlap); Bend/Advanced pins have no ToolType value. Mesh deformation math (as-rigid-as-possible) is not in sibling files; treat as P2 gap.

### AE-UI-321 · 3D primitive tools (3D 基本体工具（译名待核）)

- **What it does:** AE 2026 adds parametric 3D mesh tools (Cube Tool / 立方体工具 and siblings) that create parametric mesh layers in the Advanced 3D renderer.
- **Exact semantics:**
  - UNVERIFIED: group contains Cube, Sphere, Cylinder, Cone, Torus, Plane (from the AE 2026 parametric-mesh feature); drag creates a parametric mesh layer sized by the drag.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Scripting: ParametricMesh object (AE 26.0, UNVERIFIED). Advanced 3D/meshes are P2 in compositing-3d-color.md.

## Keyboard shortcuts

Source for every row below unless marked otherwise: Adobe help "Keyboard shortcuts in After Effects" (keyboard-shortcuts-reference, last updated May 2026), EN and zh-CN pages. "Win" = Windows, "Mac" = macOS. Ctrl ↔ Cmd, Alt ↔ Opt unless the row says otherwise. Rows marked "menu" come from AE's menu accelerators and are not on the help page; they carry UNVERIFIED unless widely documented. "Numpad" = numeric keypad (Num Lock must be on in Windows).

### AE-UI-322 · Keymap infrastructure and per-workspace keymaps (键盘快捷键基础机制)

- **What it does:** The engine that maps key chords and key sequences to commands, with application-wide and panel-scoped bindings, momentary single-key tools, double-letter sequences, numpad-specific keys, and (BAE) a keymap that switches with the workspace.
- **Exact semantics:**
  - **Scopes.** Every binding is either **Application** (应用程序快捷键, works whatever panel has focus) or **Panel** (面板快捷键, works only while that panel type has keyboard focus). Lookup order: focused panel's bindings first, then application bindings. A panel binding that uses the same chord as an application binding wins while that panel is focused (the application command does not run).
  - **Focus rules.** Keyboard focus follows the last-clicked panel (blue focus frame). Text fields, the expression editor and the Type tool in edit mode capture printable keys and the arrows; Esc or Enter (main keyboard) leaves text entry (Enter on numpad commits text-layer editing, UNVERIFIED). Shortcuts with a modifier other than Shift still reach the menu (Ctrl+S saves while editing text).
  - **Key identity.** Numpad keys are distinct from main-keyboard keys (Numpad 0 ≠ 0, Numpad Enter ≠ Enter, Numpad / ≠ /, Numpad . ≠ .). Physical-key vs character: AE binds by the US-layout key position for letters and punctuation on supported layouts and shows a US English keyboard for unsupported layouts (UNVERIFIED exact rule for non-US layouts).
  - **Double-letter sequences** (UU, EE, MM, LL, AA, RR, PP, TT, FF, SS): the second press must follow the first "in quick succession". The first press executes its single-letter command immediately; the second press within the window replaces it with the double-letter command. Window length UNVERIFIED (AE appears to use about the OS double-click interval).
  - **Momentary tools.** Holding a single-letter tool key activates the tool until key-up (see AE-UI-300); Space and middle mouse button momentarily activate the Hand tool.
  - **Non-reassignable keys.** A, AA, E, EE, F, FF, L, LL, M, MM, P, PP, R, RR, S, SS, T, TT, U, UU cannot be reassigned in the editor (the property-reveal set).
  - **Mac function keys.** F9–F12 may be captured by macOS (Mission Control); AE does not remap them. On Mac, Preferences > General "Use System Shortcut Keys" (使用系统快捷键) gives OS shortcuts (for example Cmd+H Hide, Cmd+M Minimize) precedence over AE commands such as Cmd+M Add to Render Queue (UNVERIFIED exact preference label/location; help page calls it a Composition preference).
  - **Storage.** Custom sets are text files named "<set>.txt" in the per-version user preferences folder `Aftereffects Shortcut Settings.txt` style (UNVERIFIED file name); AE ships the "Adobe After Effects Default" set.
- **Priority:** P0 (shortcut engine); per-workspace switching P1.
- **Owner:** bae-ui (+ bae-ui-motion, apps/bae)
- **Notes:** BAE decision (ARCHITECTURE 8): the active keymap switches with the workspace: the motion workspace uses the AE keymap defined in AE-UI-324…AE-UI-346; the storyboard workspace uses an Illustrator-style keymap (V, A, P, Shift+M …). Both keymaps are data (TOML/JSON) loaded by bae-ui; commands are referenced by stable command IDs, never by menu label. Implementation hint: resolve chords in a two-level map (panel → application) and run sequences with a 300–500 ms timer; dispatch only command IDs so the CRDT/undo layer sees one Command per key press. Text-input capture must use the egui focus model so that typing into fields never fires tools.

### AE-UI-323 · Keyboard Shortcuts editor (键盘快捷键)

- **What it does:** A visual editor (Edit > Keyboard Shortcuts) that shows a keyboard map, a searchable command list and the per-key modifier list, and lets the user assign, clear and save shortcut sets.
- **Exact semantics:**
  - Open: Edit > Keyboard Shortcuts; Win Ctrl+Alt+' , Mac Cmd+Opt+' .
  - Layout: (1) keyboard layout; (2) Command list (命令列表) with a shortcut column and search field; (3) Key modifier list (键修饰符列表) showing, for the selected key, the command bound to every modifier combination. A Command drop-down switches between **Application Shortcuts** and one panel at a time (**Panel shortcuts**).
  - Key colours: gray = unassigned; purple = application shortcut; green = panel shortcut. Hover tooltip shows the full command name and scope.
  - Assigning: type in the shortcut column, or click it and press keys; or drag a command onto a key / onto the modifier list; or drag a key onto a command. A command may have several shortcuts.
  - Conflicts (warning at the bottom of the editor): application shortcut already used by another application shortcut; panel shortcut already used in the same panel; panel shortcut overriding an application shortcut while that panel has focus. Undo and Clear buttons become active; clicking the highlighted conflicting command selects it.
  - Presets: "Adobe After Effects Default" plus user sets; **Save As** saves a named set; Delete removes a user set.
  - Non-reassignable keys: see AE-UI-322.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** AE CC 2019 (16.0) introduced the visual editor (UNVERIFIED version); before that shortcuts were edited in a text file. BAE decision: one editor per workspace keymap; conflict check runs across the panel → application lookup order of AE-UI-322.

### AE-UI-324 · General shortcuts (常规快捷键)

- **What it does:** Application-wide editing shortcuts: undo/redo, clipboard, select, rename, open, duplicate, delete, stacking-order moves, purge, quit.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | Undo (撤消) | Ctrl+Z | Cmd+Z | Undo semantics: project-comp-layers.md (prj-a), Undo |
  | Redo (重做) | Ctrl+Shift+Z | Cmd+Shift+Z | |
  | Cut (剪切) | Ctrl+X | Cmd+X | menu |
  | Copy (复制) | Ctrl+C | Cmd+C | menu |
  | Paste (粘贴) | Ctrl+V | Cmd+V | menu; layers paste above the selected layer at their original time |
  | Clear (清除) | Delete / Backspace | Delete | menu |
  | Duplicate (重复) layers, masks, effects, text selectors, animators, puppet meshes, shapes, render items, output modules, comps | Ctrl+D | Cmd+D | |
  | Select All (全选) | Ctrl+A | Cmd+A | scope = focused panel |
  | Deselect All (取消全选) | F2 or Ctrl+Shift+A | F2 or Cmd+Shift+A | |
  | Rename selected layer, comp, folder, effect, group or mask (重命名) | Enter (main keyboard) | Return | |
  | Open selected layer, comp or footage item (打开) | Numpad Enter | Numpad Enter | |
  | Move selected layers, masks, effects or render items down / up one step in stacking order | Ctrl+Alt+Down / Ctrl+Alt+Up | Cmd+Opt+Down / Cmd+Opt+Up | |
  | Move selected items to bottom / top of stacking order | Ctrl+Alt+Shift+Down / Up | Cmd+Opt+Shift+Down / Up | |
  | Extend selection to next / previous item (Project, Render Queue, Effect Controls) | Shift+Down / Shift+Up | Shift+Down / Shift+Up | |
  | Purge All Memory (清理所有内存) | Ctrl+Alt+Numpad / | Cmd+Opt+Numpad / | see AE-UI-375 purge |
  | Interrupt running script (中断运行脚本) | Esc | Esc | |
  | Show filename of the frame at the current time in Info panel | Ctrl+Alt+E | Cmd+Opt+E | |
  | Quit (退出) | Ctrl+Q | Cmd+Q | |
  | Help (After Effects 帮助) | F1 | F1 | menu |

- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-ui, bae-model)
- **Notes:** Every row except Quit/Help/Interrupt issues a bae-model or bae-motion Command. Select All with the Timeline focused selects all layers; with keyframes selected in the Graph Editor it selects all visible keyframes (UNVERIFIED).

### AE-UI-325 · Project and Preferences shortcuts (项目和首选项快捷键)

- **What it does:** Shortcuts for creating/opening projects, project folders, Project Settings, Find, and Preferences.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | New Project (新建项目) | Ctrl+Alt+N | Cmd+Opt+N | |
  | Open Project (打开项目) | Ctrl+O | Cmd+O | |
  | Open most recent project (打开最近的项目) | Ctrl+Alt+Shift+P | Cmd+Opt+Shift+P | |
  | New Folder in Project panel (新建文件夹) | Ctrl+Alt+Shift+N | Cmd+Opt+Shift+N | Project panel focused; in a Composition viewer the same chord = New Comp Viewer (AE-UI-326) |
  | Project Settings (项目设置) | Ctrl+Alt+Shift+K | Cmd+Opt+Shift+K | also click the bpc button at the bottom of the Project panel |
  | Cycle project bit depth 8 → 16 → 32 bpc | Alt-click bpc button | Opt-click bpc button | |
  | Find in Project panel (查找) | Ctrl+F | Cmd+F | Project panel focused |
  | Preferences (首选项) | Ctrl+Alt+; | Cmd+Opt+; | Mac menu: After Effects > Settings (AE 24+) |
  | Restore default preferences | hold Ctrl+Alt+Shift while launching | hold Cmd+Opt+Shift while launching | asks for confirmation |

- **Priority:** P0 (new/open/save); P1 (others)
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Import, Save and Save As are in AE-UI-332 and AE-UI-346. Project data semantics: project-comp-layers.md (prj-a). BAE decision: "restore default preferences on launch" is a CLI flag (`bae --reset-prefs`) plus the modifier chord.

### AE-UI-326 · Panel, viewer, workspace and window shortcuts (面板、查看器、工作区和窗口快捷键)

- **What it does:** Open/close panels, switch workspaces, maximize the panel under the pointer, cycle viewers, and split viewers.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Project panel (项目) | Ctrl+0 | Cmd+0 |
  | Render Queue panel (渲染队列) | Ctrl+Alt+0 | Cmd+Opt+0 |
  | Tools (工具) | Ctrl+1 | Cmd+1 |
  | Info (信息) | Ctrl+2 | Cmd+2 |
  | Preview (预览) | Ctrl+3 | Cmd+3 |
  | Audio (音频) | Ctrl+4 | Cmd+4 |
  | Effects & Presets (效果和预设) | Ctrl+5 | Cmd+5 |
  | Character (字符) | Ctrl+6 | Cmd+6 |
  | Paragraph (段落) | Ctrl+7 | Cmd+7 |
  | Paint (绘画) | Ctrl+8 | Cmd+8 |
  | Brushes (画笔) | Ctrl+9 | Cmd+9 |
  | Effect Controls for selected layer (效果控件) | F3 or Ctrl+Shift+T | F3 or Cmd+Shift+T |
  | Flowchart for project (流程图) | Ctrl+F11 | Cmd+F11 |
  | Switch to workspace 1 / 2 / 3 (切换到工作区) | Shift+F10 / Shift+F11 / Shift+F12 | same |
  | Close active viewer or panel (closes content first) | Ctrl+W | Cmd+W |
  | Close active panel or all viewers of that type | Ctrl+Shift+W | Cmd+Shift+W |
  | New viewer: split the frame of the active viewer, new viewer gets the opposite lock state (新建合成查看器) | Ctrl+Alt+Shift+N | Cmd+Opt+Shift+N |
  | Maximize / restore panel under the pointer (最大化) | ` (accent grave) | ` |
  | Fit application/floating window to screen (press again: contents fill screen) | Ctrl+\ | Cmd+\ |
  | Move window to main monitor and fit | Ctrl+Alt+\ | Cmd+Opt+\ |
  | Toggle activation between Composition and Timeline panels of current comp | \ | \ |
  | Previous / next item in active viewer (e.g. open comps) | Shift+, / Shift+. | same |
  | Previous / next panel in active frame | Alt+Shift+, / Alt+Shift+. | Opt+Shift+, / Opt+Shift+. |
  | Activate a view in a multi-view layout without changing layer selection | middle-click | middle-click |

  Workspace slots: Shift+F10/F11/F12 map to the first three workspaces in the workspace bar order (Window > Workspace > "Assign Shortcut to <name> Workspace" sets which workspace each key opens, UNVERIFIED exact menu label).
- **Priority:** P0 (panel toggles, maximize, workspace switch); P1 (window fitting, cycling)
- **Owner:** bae-ui (+ bae-ui-motion)
- **Notes:** Panel/workspace framework: ui-preview-render-io.md (ui-a), workspaces. The ` key is the same physical key as ~ on US keyboards; AE uses it unshifted. BAE decision: switching workspace also switches the keymap (AE-UI-322).

### AE-UI-327 · Composition and work area shortcuts (合成和工作区域快捷键)

- **What it does:** Create comps, open Composition Settings, set and trim the work area, and navigate the comp hierarchy.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | New Composition (新建合成) | Ctrl+N | Cmd+N | |
  | Composition Settings (合成设置) | Ctrl+K | Cmd+K | |
  | Set work area start / end to current time (工作区域开头/结尾) | B / N | B / N | if the new start ≥ end (or end ≤ start) the other end moves to keep ≥ 1 frame (UNVERIFIED) |
  | Work area to selected layers' duration, or comp duration if none selected | Ctrl+Alt+B | Cmd+Opt+B | span = min in point … max out point of selected layers |
  | Trim Comp to Work Area (将合成修剪至工作区域) | Ctrl+Shift+X | Cmd+Shift+X | |
  | New Composition from Selection (基于所选项新建合成) | Alt+\ | Cmd+\ | help page lists different chords per OS |
  | Composition Mini-Flowchart (合成微型流程图) | Tab | Tab | |
  | Activate most recently active comp in the same nesting hierarchy | Shift+Esc | Shift+Esc | |
  | Add to Render Queue (添加到渲染队列) | Ctrl+M | Cmd+M | menu; see AE-UI-346 |

- **Priority:** P0 (new comp, settings, B/N); P1 (others)
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Work area data and Composition Settings: project-comp-layers.md (prj-b). Work-area range in previews: AE-UI-365.

### AE-UI-328 · Time navigation shortcuts (时间导航快捷键)

- **What it does:** Moves the current-time indicator (CTI) by frames, to boundaries, to keyframes/markers, and to layer in/out points.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Go To Time dialog (转到时间) | Alt+Shift+J | Opt+Shift+J |
  | Go to work-area start / end | Shift+Home / Shift+End | same |
  | Previous / next visible item in time ruler (keyframe, layer marker, comp marker, work-area start/end; Roto Brush span start/end/base frame in Layer panel) | J / K | J / K |
  | Go to start of comp, layer or footage (转到开始) | Home or Ctrl+Alt+Left | Home or Cmd+Opt+Left |
  | Go to end (转到结束) | End or Ctrl+Alt+Right | End or Cmd+Opt+Right |
  | Forward 1 frame (前进 1 帧) | Page Down or Ctrl+Right | Page Down or Cmd+Right |
  | Forward 10 frames | Shift+Page Down or Ctrl+Shift+Right | Shift+Page Down or Cmd+Shift+Right |
  | Back 1 frame (后退 1 帧) | Page Up or Ctrl+Left | Page Up or Cmd+Left |
  | Back 10 frames | Shift+Page Up or Ctrl+Shift+Left | Shift+Page Up or Cmd+Shift+Left |
  | Go to layer In point (转到图层入点) | I | I |
  | Go to layer Out point (转到图层出点) | O | O |
  | Previous / next In or Out point (any layer) | Ctrl+Alt+Shift+Left / Right | Cmd+Opt+Shift+Left / Right |
  | Scroll Timeline to current time | D | D |

  ```text
  // frame stepping in comp time (flicks); fd = comp frame duration in flicks
  next_frame(t)  = clamp(floor_frame(t) + fd, comp_start, comp_end_last_frame)
  "End" goes to the last frame start: comp_start + duration - fd (not to duration)
  "O" goes to the layer's last visible frame: out_point - fd (UNVERIFIED: AE shows the last rendered frame of the layer)
  J/K: candidate times = union of visible keyframes (of revealed properties of selected layers, or all layers if none selected — UNVERIFIED), layer markers, comp markers, work-area start/end; pick the nearest strictly before/after t.
  ```
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Time display/units: project-comp-layers.md (prj-a), project settings / timecode. 10-frame steps clamp at comp start/end. In the Layer/Footage panel the same keys navigate that panel's own time.

### AE-UI-329 · Preview shortcuts (预览快捷键)

- **What it does:** Start/stop previews, audio-only previews, scrubbing, snapshots and Fast Previews modes.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | Start / stop preview (开始或停止预览) | Space, Numpad 0, Shift+Numpad 0 | same | each shortcut has its own settings (AE-UI-364); Shift+Space is also a default slot (AE-UI-364) |
  | Preview Alternate Preview frames (default 5) around current time | Alt+Numpad 0 | Opt+Numpad 0 or Ctrl+Opt+0 | |
  | Audio-only preview from current time (仅预览音频) | Numpad . | Numpad . or Ctrl+. | |
  | Audio-only preview of work area | Alt+Numpad . | Opt+Numpad . or Ctrl+Cmd+. | |
  | Reset preview settings to RAM-Preview + Standard-Preview behaviour | Alt-click Reset in Preview panel | Opt-click Reset | |
  | Scrub video (手动预览视频) | drag or Alt-drag CTI (depends on Live Update) | drag or Opt-drag CTI | |
  | Scrub audio (手动预览音频) | Ctrl-drag CTI | Cmd-drag CTI | AE-UI-374 |
  | Toggle Mercury Transmit (切换 Mercury Transmit 视频预览) | Numpad / | Numpad / or Ctrl+/ | AE-UI-377 |
  | Take snapshot 1–4 (拍摄快照) | Shift+F5 / F6 / F7 / F8 | same | |
  | Show snapshot 1–4 in active viewer (显示快照) | F5 / F6 / F7 / F8 (hold) | same | |
  | Purge snapshot 1–4 (清理快照) | Ctrl+Shift+F5…F8 | Cmd+Shift+F5…F8 | |
  | Fast Previews: Off / Adaptive Resolution / Draft / Fast Draft / Wireframe (快速预览: 关闭/自适应分辨率/草稿/快速绘图/线框) | Ctrl+Alt+1 / 2 / 3 / 4 / 5 | Cmd+Opt+1…5 | Draft and Fast Draft need a 3D renderer that supports them |

- **Priority:** P0 (Space, Numpad 0, scrub); P1 (audio-only, snapshots); P2 (Mercury Transmit)
- **Owner:** bae-ui-motion (+ bae-render, bae-media)
- **Notes:** Snapshot semantics: ui-preview-render-io.md (ui-a), Snapshots. Preview behaviour: AE-UI-363…AE-UI-377. Fast Previews modes: compositing-3d-color.md (cmp-b), fast previews. Live Update toggle: ui-preview-render-io.md (ui-a), Timeline.

### AE-UI-330 · Viewer shortcuts (视图快捷键)

- **What it does:** Zoom, resolution, channel display, overlays (grid, guides, rulers, safe zones, layer controls) and snapping toggles in the Composition, Layer and Footage viewers.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Zoom in (放大) | . (main keyboard) | . |
  | Zoom out (缩小) | , | , |
  | Zoom to 100% | / (main keyboard) | / |
  | Zoom to fit (适合) | Shift+/ | Shift+/ |
  | Zoom up to 100% to fit (适合，最大 100%) | Alt+/ | Opt+/ |
  | Center comp in panel | double-click Hand tool | same |
  | Resolution Full / Half (完整/二分之一) | Ctrl+J / Ctrl+Shift+J | Cmd+J / Cmd+Shift+J |
  | Resolution Custom (自定义) | Ctrl+Alt+J | Cmd+Opt+J |
  | Resolution Quarter (四分之一) | Ctrl+Alt+Shift+J (menu, UNVERIFIED) | Cmd+Opt+Shift+J (UNVERIFIED) |
  | View Options dialog (视图选项) | Ctrl+Alt+U | Cmd+Opt+U |
  | Show R / G / B / Alpha as grayscale | Alt+1 / 2 / 3 / 4 | Opt+1 / 2 / 3 / 4 |
  | Show colorized R / G / B | Alt+Shift+1 / 2 / 3 | Opt+Shift+1 / 2 / 3 |
  | Toggle straight (unmultiplied) RGB | Alt+Shift+4 | Opt+Shift+4 |
  | Alpha boundary / alpha overlay (Layer panel, Roto) | Alt+5 / Alt+6 | Opt+5 / Opt+6 |
  | Refine Edge X-ray | Alt+X | Opt+X |
  | Display color management on/off for active view | Shift+Numpad / | Shift+Numpad / |
  | Prevent viewer rendering (lock display) | Caps Lock | Caps Lock |
  | Safe zones (标题/动作安全) | ' | ' |
  | Grid (网格) | Ctrl+' | Cmd+' |
  | Proportional grid (对称网格) | Alt+' | Opt+' |
  | Rulers (标尺) | Ctrl+R | Cmd+R |
  | Guides (参考线) | Ctrl+; | Cmd+; |
  | Snap to Grid (对齐到网格) | Ctrl+Shift+' | Cmd+Shift+' |
  | Snap to Guides (对齐到参考线) | Ctrl+Shift+; | Cmd+Shift+; |
  | Lock Guides (锁定参考线) | Ctrl+Alt+Shift+; | Cmd+Opt+Shift+; |
  | Show/hide layer controls (图层控件) | Ctrl+Shift+H | Cmd+Shift+H |
  | 3D views 1 / 2 / 3 (default Front / Custom View 1 / Active Camera) | F10 / F11 / F12 | same |
  | Return to previous 3D view | Esc | Esc |

- **Priority:** P0 (zoom, fit, resolution, channels, layer controls, grid/guides/rulers/safe); P1 (rest)
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Zoom steps and viewer behaviour: ui-preview-render-io.md (ui-a), Magnification; overlays: AE-UI-357…AE-UI-362; channel/exposure views: compositing-3d-color.md (cmp-b). The help page's Mac column for Ctrl+Shift+H is Cmd+Shift+H. Caps Lock stops viewer updates (AE-UI-372); a red bar appears at the bottom of the viewer (UNVERIFIED colour/text).

### AE-UI-331 · Timeline view shortcuts (时间轴视图快捷键)

- **What it does:** Time zoom, scrolling and column toggles in the Timeline panel.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | Zoom in time (放大时间) | = (main keyboard) | = | zooms about the CTI |
  | Zoom out time (缩小时间) | - (main keyboard) | - | |
  | Zoom to single-frame units; press again to show the whole comp | ; | ; | |
  | Zoom out to whole comp; press again to return to the Time Navigator span | Shift+; | Shift+; | |
  | Scroll topmost selected layer to top of Timeline | X | X | |
  | Scroll to current time | D | D | |
  | Show/hide Switches and Modes columns (切换开关/模式) | F4 | F4 | |
  | Show/hide Parent column (父级) | Shift+F4 | Shift+F4 | |
  | Toggle Graph Editor / layer bar mode (图表编辑器) | Shift+F3 | Shift+F3 | |
  | Expand selected layers to show all properties | Ctrl+` | Cmd+` | |
  | Expand a group and all child groups | Ctrl-click twirl triangle | Cmd-click twirl triangle | |
  | Find in Timeline (查找) | Ctrl+F | Cmd+F | Timeline focused |

- **Priority:** P0 (zoom, Shift+F3, F4, expand); P1 (others)
- **Owner:** bae-ui-motion
- **Notes:** Timeline columns, navigator and Graph Editor UI: ui-preview-render-io.md (ui-a), Timeline. In the Effect Controls panel Ctrl+` / Cmd+` expands selected effects, Ctrl/Cmd-click a triangle expands all child groups.

### AE-UI-332 · Footage and import shortcuts (素材和导入快捷键)

- **What it does:** Import, interpret, replace, reload and proxy footage items, and add items to comps.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Import File (导入文件) | Ctrl+I | Cmd+I |
  | Import Multiple Files (导入多个文件) | Ctrl+Alt+I | Cmd+Opt+I |
  | Open movie in Footage panel | double-click item in Project panel | same |
  | Add selected items to most recently active comp | Ctrl+/ (main keyboard) | Cmd+/ |
  | Replace source of selected layers with item selected in Project panel | Ctrl+Alt+/ (main keyboard) | Cmd+Opt+/ |
  | Replace source of a layer | Alt-drag item onto selected layer | Opt-drag item |
  | Delete footage item without warning | Ctrl+Backspace | Cmd+Delete |
  | Interpret Footage (解释素材) | Ctrl+Alt+G | Cmd+Opt+G |
  | Remember Interpretation (记住解释) | Ctrl+Alt+C | Cmd+Opt+C |
  | Edit Original (编辑原稿) | Ctrl+E | Cmd+E |
  | Replace Footage > File (替换素材) | Ctrl+H | Cmd+H |
  | Reload Footage (重新加载素材) | Ctrl+Alt+L | Cmd+Opt+L |
  | Set Proxy > File (设置代理) | Ctrl+Alt+P | Cmd+Opt+P |

- **Priority:** P0 (import, interpret); P1 (others)
- **Owner:** bae-ui-motion (+ bae-media, bae-model)
- **Notes:** Semantics of import, interpretation, proxies, replace: project-comp-layers.md (prj-a). Paste Interpretation is Ctrl+Alt+V in the Project panel (UNVERIFIED; the same chord is "Paste layers at current time" in the Timeline).

### AE-UI-333 · Effects and animation preset shortcuts (效果和动画预设快捷键)

- **What it does:** Remove effects, re-apply the last effect or preset, add effects from the Effects & Presets panel.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Remove All effects from selected layers (全部移除) | Ctrl+Shift+E | Cmd+Shift+E |
  | Apply most recently applied effect to selected layers (上次使用的效果) | Ctrl+Alt+Shift+E | Cmd+Opt+Shift+E |
  | Apply most recently applied animation preset (最近的动画预设) | Ctrl+Alt+Shift+F | Cmd+Opt+Shift+F |
  | Apply effect(s) to selected layers | double-click in Effects & Presets | same |
  | Effect Controls for selected layer | F3 or Ctrl+Shift+T | F3 or Cmd+Shift+T |

- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Effect framework: effects.md (fx-a).

### AE-UI-334 · Layer creation and selection shortcuts (图层创建和选择快捷键)

- **What it does:** Create layers of each type and select layers by number or stacking order.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | New Solid (纯色) | Ctrl+Y | Cmd+Y | opens Solid Settings |
  | New Null Object (空对象) | Ctrl+Alt+Shift+Y | Cmd+Opt+Shift+Y | |
  | New Adjustment Layer (调整图层) | Ctrl+Alt+Y | Cmd+Opt+Y | |
  | New Text layer (文本) | Ctrl+Alt+Shift+T | Cmd+Opt+Shift+T | Timeline/Comp focused; with text being edited the same chord toggles the paragraph composer |
  | New Light (灯光) | Ctrl+Alt+Shift+L | Cmd+Opt+Shift+L | |
  | New Camera (摄像机) | Ctrl+Alt+Shift+C | Cmd+Opt+Shift+C | |
  | Layer Settings for solid / light / camera / null / adjustment (图层设置) | Ctrl+Shift+Y | Cmd+Shift+Y | |
  | Select layer 1–999 by number | Numpad 0–9 (type digits quickly) | same | |
  | Toggle selection of layer by number | Shift+Numpad 0–9 | same | |
  | Select next / previous layer | Ctrl+Down / Ctrl+Up | Cmd+Down / Cmd+Up | |
  | Extend selection to next / previous layer | Ctrl+Shift+Down / Up | Cmd+Shift+Down / Up | |
  | Deselect all layers | Ctrl+Shift+A | Cmd+Shift+A | |
  | Open layer in Layer panel (precomp: open source comp) | double-click layer | same | |
  | Open source in Footage panel (precomp: Layer panel) | Alt-double-click | Opt-double-click | |

  New layers are inserted above the topmost selected layer (or at the top if none selected) and start at the comp start (Preferences > General "Create New Layers at Composition Start Time", default on). Typed layer numbers are buffered: digits pressed within a short interval build a multi-digit number (UNVERIFIED interval).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model, bae-motion)
- **Notes:** Layer types and settings: project-comp-layers.md (prj-b).

### AE-UI-335 · Layer timing, editing and switch shortcuts (图层时间、编辑和开关快捷键)

- **What it does:** Move, trim, split, reverse and time-remap layers; reorder, precompose, lock, show/hide; set quality, sampling and blend mode; fit to comp.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | Move In / Out point to current time | [ / ] | [ / ] | layer slides; duration unchanged |
  | Trim In / Out point to current time | Alt+[ / Alt+] | Opt+[ / Opt+] | |
  | Move In point to comp start / Out point to comp end | Alt+Home / Alt+End | Opt+Home / Opt+End | |
  | Move layers 1 frame earlier / later | Alt+Page Up / Alt+Page Down | Opt+Page Up / Down | |
  | Move layers 10 frames earlier / later | Alt+Shift+Page Up / Down | Opt+Shift+Page Up / Down | |
  | Set In / Out point by time-stretching | Ctrl+Shift+, / Ctrl+Alt+, | Cmd+Shift+, / Cmd+Opt+, | |
  | Split Layer (拆分图层; no selection = all layers) | Ctrl+Shift+D | Cmd+Shift+D | |
  | Pre-compose (预合成) | Ctrl+Shift+C | Cmd+Shift+C | |
  | Paste layers at current time | Ctrl+Alt+V | Cmd+Opt+V | |
  | Time-Reverse Layer (时间反向图层) | Ctrl+Alt+R | Cmd+Opt+R | |
  | Enable Time Remapping (启用时间重映射) | Ctrl+Alt+T | Cmd+Opt+T | |
  | Bring Layer Forward / Send Layer Backward (前移一层/后移一层) | Ctrl+] / Ctrl+[ | Cmd+] / Cmd+[ | menu (Layer > Arrange), UNVERIFIED |
  | Bring Layer to Front / Send Layer to Back (置于顶层/置于底层) | Ctrl+Shift+] / Ctrl+Shift+[ | Cmd+Shift+] / Cmd+Shift+[ | menu, UNVERIFIED |
  | Lock selected layers (锁定) | Ctrl+L | Cmd+L | |
  | Unlock All Layers (解锁所有图层) | Ctrl+Shift+L | Cmd+Shift+L | |
  | Toggle Video switch of selected layers | Ctrl+Alt+Shift+V | Cmd+Opt+Shift+V | |
  | Solo exclusively (turn off other solos) | Alt-click Solo switch | Opt-click | no default key for Solo |
  | Quality Best / Draft / Wireframe (最佳/草稿/线框) | Ctrl+U / Ctrl+Shift+U / Ctrl+Alt+Shift+U | Cmd+U / Cmd+Shift+U / Cmd+Opt+Shift+U | |
  | Sampling Bilinear / Bicubic for Best quality | Alt+B / Alt+Shift+B | Opt+B / Opt+Shift+B | |
  | Previous / next blending mode for selected layers | Shift+- / Shift+= (main keyboard) | same | wraps (UNVERIFIED) |
  | Center selected layers in view (moves Position) | Ctrl+Home | Cmd+Home | |
  | Center anchor point in visible content (在图层内容中居中放置锚点) | Ctrl+Alt+Home | Cmd+Opt+Home | |
  | Fit to Comp (适合复合) | Ctrl+Alt+F | Cmd+Opt+F | non-uniform scale |
  | Fit to Comp Width | Ctrl+Alt+Shift+H | Cmd+Opt+Shift+H | keeps aspect |
  | Fit to Comp Height | Ctrl+Alt+Shift+G | Cmd+Opt+Shift+G | keeps aspect |

  No default shortcut: Solo, Shy, Time Stretch dialog, Freeze Frame, Sequence Layers, Layer Styles, Auto-Orient dialog has Ctrl+Alt+O (AE-UI-337).
- **Priority:** P0 ([ ] Alt+[ ] split, precompose, lock, fit, reverse, time remap); P1 (rest)
- **Owner:** bae-ui-motion (+ bae-motion, bae-model)
- **Notes:** Command semantics: project-comp-layers.md (prj-b), layer editing ops / precompose; transform-keyframes-expressions.md (ani-a), time remap. The help lists "Fit to Comp" etc. under modifying properties. Win "Ctrl+Alt+O" is written "Command + Alt + O" on the Mac column of the help page; Mac = Cmd+Opt+O.

### AE-UI-336 · Property reveal shortcuts (显示属性快捷键)

- **What it does:** Single- and double-letter keys that twirl open only the named properties of the selected layers (or all layers) in the Timeline, with Shift to add/remove and Alt+Shift to set a keyframe.
- **Exact semantics:**

  | Key | Reveals (中文) |
  |---|---|
  | A | Anchor Point (锚点); lights/cameras: Point of Interest (目标点) |
  | P | Position (位置) |
  | S | Scale (缩放) |
  | R | Rotation and Orientation (旋转和方向) |
  | T | Opacity (不透明度); lights: Intensity (强度) |
  | U | all properties with keyframes |
  | UU | all modified properties (value ≠ default, keyframed or expressions) |
  | E | Effects group (效果) |
  | EE | properties with expressions |
  | M | Mask Path (蒙版路径) |
  | MM | all mask property groups |
  | F | Mask Feather (蒙版羽化) |
  | TT | Mask Opacity (蒙版不透明度) |
  | L | Audio Levels (音频电平) |
  | LL | audio waveform |
  | AA | Material Options group (材质选项) |
  | RR | Time Remap (时间重映射) |
  | PP | paint strokes, Roto Brush strokes, Puppet pins |
  | FF | instances of missing effects |
  | SS | only the selected properties and groups |

  ```text
  targets = selected layers, or all layers in the Timeline if none selected (shy layers obey the Hide Shy switch)
  key          : collapse every property of targets, then reveal the named set (path from layer root twirled open)
  Shift+key    : toggle the named set in/out of the currently revealed set, keep the rest
  Alt+Shift+key (Win) / Opt+Shift+key (Mac) for P,S,R,T,A: add a keyframe at CTI (or remove the one at CTI) for that property of each selected layer, enabling its stopwatch if needed
  Alt+Shift-click (Opt+Shift-click) a property or group name: hide it from the revealed set
  a target with nothing to reveal stays collapsed
  ```
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** These keys cannot be reassigned (AE-UI-322). Help page Mac column for add-keyframe reads "Option + property shortcut"; AE's actual Mac chord is Opt+Shift+P etc. (UNVERIFIED which wording is current). UU "modified" compares against the property's default value (UNVERIFIED for properties without a defined default). BAE decision: reveal state is UI-only (not stored in the project CRDT) but per-user persisted.

### AE-UI-337 · Property modification shortcuts (修改图层属性快捷键)

- **What it does:** Change transform values from the keyboard or by dragging values, and open the transform value dialogs.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Step |
  |---|---|---|---|
  | Drag a property value (scrubby slider) | drag | drag | default increment |
  | … ×10 | Shift-drag | Shift-drag | 10× |
  | … ×0.1 | Ctrl-drag | Cmd-drag | 0.1× |
  | Nudge Position 1 / 10 px at current magnification | Arrow / Shift+Arrow | same | see AE-UI-304 |
  | Rotation ±1° / ±10° | Numpad + / - ; Shift+Numpad + / - | same | Z Rotation for 3D |
  | Opacity (lights: Intensity) ±1% / ±10% | Ctrl+Alt+Numpad + / - ; Ctrl+Alt+Shift+Numpad + / - | Ctrl+Opt+Numpad + / - ; Ctrl+Opt+Shift+Numpad + / - | |
  | Scale +1% / −1% | Ctrl++ / Ctrl+- or Alt+Numpad + / - | Cmd++ / Cmd+- or Opt+Numpad + / - | |
  | Scale ±10% | Ctrl+Shift++ / - or Alt+Shift+Numpad + / - | Cmd+Shift++ / - or Opt+Shift+Numpad + / - | |
  | Rotation in 15° steps | Shift-drag with Rotation tool | same | |
  | Scale constrained to footage aspect | Shift-drag handle | same | |
  | Reset Rotation to 0° | double-click Rotation tool | same | |
  | Reset Scale to 100% | double-click Selection tool | same | |
  | Auto-Orient dialog (自动方向) | Ctrl+Alt+O | Cmd+Opt+O | |
  | Opacity dialog | Ctrl+Shift+O | Cmd+Shift+O | |
  | Rotation dialog | Ctrl+Shift+R | Cmd+Shift+R | |
  | Position dialog | Ctrl+Shift+P | Cmd+Shift+P | |

  ```text
  keyboard value changes apply to every selected layer; if the property is keyframed, a keyframe is
  added/updated at CTI; Scale ±1% adds to the percentage (100 -> 101), not multiplies (UNVERIFIED)
  scale ±% keeps the current x:y ratio (UNVERIFIED for non-uniform scale)
  ```
- **Priority:** P0 (drag values, nudges, reset); P1 (numpad and dialogs)
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Scale dialog has no default chord (Layer > Transform > Scale…, UNVERIFIED). The Mac Scale chord Cmd+- conflicts with nothing in AE because viewer zoom uses , and . keys.

### AE-UI-338 · Keyframe, Graph Editor and expression shortcuts (关键帧、图表编辑器和表达式快捷键)

- **What it does:** Select, move, ease and set interpolation of keyframes; open the interpolation and velocity dialogs; add expressions.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Select all keyframes of a property | click property name | same |
  | Select all visible keyframes and properties | Ctrl+Alt+A | Cmd+Opt+A |
  | Deselect all keyframes, properties, groups | Shift+F2 or Ctrl+Alt+Shift+A | Shift+F2 or Cmd+Opt+Shift+A |
  | Move keyframes 1 frame later / earlier | Alt+Right / Alt+Left | Opt+Right / Opt+Left |
  | Move keyframes 10 frames | Alt+Shift+Right / Left | Opt+Shift+Right / Left |
  | Keyframe Interpolation dialog (关键帧插值) | Ctrl+Alt+K | Cmd+Opt+K |
  | Toggle Hold Keyframe (定格关键帧) ↔ Auto Bezier | Ctrl+Alt+H | Cmd+Opt+H |
  | Toggle linear ↔ Auto Bezier (layer bar mode) | Ctrl-click keyframe | Cmd-click |
  | Toggle linear ↔ hold (layer bar mode) | Ctrl+Alt-click keyframe | Cmd+Opt-click |
  | Easy Ease (缓动) | F9 | F9 |
  | Easy Ease In (缓入) | Shift+F9 | Shift+F9 |
  | Easy Ease Out (缓出) | Ctrl+Shift+F9 | Cmd+Shift+F9 |
  | Keyframe Velocity dialog (关键帧速度) | Ctrl+Shift+K | Cmd+Shift+K |
  | Add/remove keyframe at CTI | Alt+Shift+P/S/R/T/A | Opt+Shift+P/S/R/T/A (help: "Option + property shortcut") |
  | Toggle stopwatch (keyframing) | click stopwatch | same |
  | Add/remove expression (添加/移除表达式) | Alt-click stopwatch | Opt-click stopwatch |
  | Reverse-paste copied keyframes | Ctrl+Shift+V | Cmd+Shift+V |
  | Show time between two keyframes/markers in Info panel | Alt-click both | Opt-click both |

  Shift-drag a keyframe snaps it to CTI, markers, other keys, in/out points (snapping in the Timeline). Ctrl+Alt+H on a hold key restores the interpolation it had before (UNVERIFIED: AE sets Auto Bezier per help label).
- **Priority:** P0 (F9 family, interpolation/velocity dialogs, Alt+arrows, Alt-click stopwatch); P1 (rest)
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** Interpolation/ease math: transform-keyframes-expressions.md (ani-a); expression semantics (ani-b). Ctrl+Shift+V "reverse paste" appears on the 2026 help page; version of introduction UNVERIFIED.

### AE-UI-339 · 3D layer and camera shortcuts (3D 图层和摄像机快捷键)

- **What it does:** Switch 3D views and camera navigation modes, gizmo modes, frame layers with the camera, and toggle shadows.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | 3D view 1 / 2 / 3 (defaults Front / Custom View 1 / Active Camera) | F10 / F11 / F12 | same |
  | Return to previous view | Esc | Esc |
  | Camera navigation Orbit / Pan / Dolly (with a camera tool active) | 1 / 2 / 3 | same |
  | Gizmo: Universal / Position / Scale / Rotation | V / 4 / 5 / 6 (UNVERIFIED context: while a 3D layer is selected and a camera navigation mode is active) | same |
  | Look at selected 3D layers (camera + point of interest move) | Ctrl+Alt+Shift+\ | Cmd+Opt+Shift+\ |
  | With a camera tool: frame selected 3D layers / all 3D layers | F / Ctrl+Shift+F | F / Cmd+Shift+F |
  | Toggle Casts Shadows (投影) for selected 3D layers | Alt+Shift+C | Opt+Shift+C |
  | New Light / New Camera | Ctrl+Alt+Shift+L / C | Cmd+Opt+Shift+L / C |

- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Camera tools and gizmo: AE-UI-310…AE-UI-312; 3D views and renderer: compositing-3d-color.md (cmp-b).

### AE-UI-340 · Text editing and formatting shortcuts (文本快捷键)

- **What it does:** Caret movement, selection and character/paragraph formatting while a text layer is in edit mode with a Type tool.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Move caret 1 char / 1 line (horizontal text) | Left/Right; Up/Down | same |
  | Move caret 1 word / 1 paragraph | Ctrl+Left/Right; Ctrl+Up/Down | Cmd+Left/Right; Cmd+Up/Down |
  | Vertical text: arrows rotate meaning (Up/Down = char, Left/Right = line) | | |
  | Extend selection 1 char / 1 line / 1 word | Shift+arrows; Ctrl+Shift+Left/Right | Shift+arrows; Cmd+Shift+Left/Right |
  | Caret / select to line start or end | Home/End; Shift+Home/End | same |
  | Select to start/end of text frame | Ctrl+Shift+Home/End | Cmd+Shift+Home/End |
  | Extend selection to click | Shift-click | same |
  | Select word / line / paragraph / all | double / triple / quadruple / quintuple click | same |
  | Select all text on a layer (enter edit mode) | double-click text layer | same |
  | Align left / center / right (vertical: top / center / bottom) | Ctrl+Shift+L / C / R | Cmd+Shift+L / C / R |
  | Justify, last line left / right / forced | Ctrl+Shift+J / Ctrl+Alt+Shift+J / Ctrl+Shift+F | Cmd+Shift+J / Cmd+Opt+Shift+J / Cmd+Shift+F |
  | All Caps / Small Caps (全部大写字母/小型大写字母) | Ctrl+Shift+K / Ctrl+Alt+Shift+K | Cmd+Shift+K / Cmd+Opt+Shift+K |
  | Superscript / Subscript (上标/下标) | Ctrl+Shift+= / Ctrl+Alt+Shift+= | Cmd+Shift+= / Cmd+Opt+Shift+= |
  | Horizontal / vertical scale to 100% | Ctrl+Shift+X / Ctrl+Alt+Shift+X | Cmd+Shift+X / Cmd+Opt+Shift+X |
  | Auto leading | Ctrl+Alt+Shift+A | Cmd+Opt+Shift+A |
  | Reset tracking to 0 | Ctrl+Shift+Q | Cmd+Ctrl+Shift+Q |
  | Font size −/+ 2 units | Ctrl+Shift+, / . | Cmd+Shift+, / . |
  | Font size −/+ 10 units | Ctrl+Alt+Shift+, / . | Cmd+Opt+Shift+, / . |
  | Leading +/− 2 units | Alt+Down / Alt+Up | Opt+Down / Opt+Up |
  | Leading +/− 10 units | Ctrl+Alt+Down / Up | Cmd+Opt+Down / Up |
  | Baseline shift −/+ 2 units | Alt+Shift+Down / Up | Opt+Shift+Down / Up |
  | Baseline shift −/+ 10 units | Ctrl+Alt+Shift+Down / Up | Cmd+Opt+Shift+Down / Up |
  | Kerning/tracking −/+ 20 (1/1000 em) | Alt+Left / Right | Opt+Left / Right |
  | Kerning/tracking −/+ 100 | Ctrl+Alt+Left / Right | Cmd+Opt+Left / Right |
  | Toggle paragraph composer (单行/每行书写器) | Ctrl+Alt+Shift+T | Cmd+Opt+Shift+T |
  | Exit edit mode, keep changes | Esc or Numpad Enter | same |

  Units: font size and leading in px; kerning/tracking applies to the pair at the caret when nothing is selected (kerning) or to the selection (tracking).
- **Priority:** P0 (caret, selection, exit edit); P1 (formatting chords)
- **Owner:** bae-ui-motion (+ bae-text)
- **Notes:** Text model and formatting semantics: text.md. While editing text, these chords take precedence over identical application chords (Ctrl+Shift+L is Unlock All Layers outside text edit; Ctrl+Shift+C Pre-compose; Ctrl+Shift+F Mask Feather) — panel-scope rule of AE-UI-322.

### AE-UI-341 · Mask shortcuts (蒙版快捷键)

- **What it does:** Create, select, transform, invert masks and open mask dialogs; change mask modes.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | New Mask (新建蒙版; layer-sized rectangle) | Ctrl+Shift+N | Cmd+Shift+N |
  | Select all points of a mask | Alt-click mask | Opt-click mask |
  | Select next / previous mask | Alt+` / Alt+Shift+` | Opt+` / Opt+Shift+` |
  | Free Transform Points (自由变换点) | double-click mask with Selection tool, or Ctrl+T with mask selected | same / Cmd+T |
  | Exit Free Transform | Esc (or Enter) | same |
  | Scale about centre in Free Transform | Ctrl-drag | Cmd-drag |
  | Nudge selected points 1 / 10 px | Arrow / Shift+Arrow | same |
  | Toggle smooth ↔ corner vertex | Ctrl+Alt-click vertex | Cmd+Opt-click |
  | Redraw Bezier handles | Ctrl+Alt-drag vertex | Cmd+Opt-drag |
  | Invert mask (反转) | Ctrl+Shift+I | Cmd+Shift+I |
  | Mask Feather dialog (蒙版羽化) | Ctrl+Shift+F | Cmd+Shift+F |
  | Mask Shape dialog (蒙版形状) | Ctrl+Shift+M | Cmd+Shift+M |
  | Mask mode None / Add / Subtract / Intersect / Darken / Difference | N / A / S / I / D / F | same (help Mac column lists S for Add, assumed typo) |

  Mask-mode letter keys apply to the selected mask(s) (UNVERIFIED context: they act as accelerators while the mask's Mode menu is focused/open; with a layer selected and no menu they are tool/reveal keys).
- **Priority:** P0 (new mask, free transform, invert, vertex edit); P1 (dialogs, mode keys)
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Mask semantics and modes: compositing-3d-color.md (cmp-a), masks. In shape layers Ctrl+T / Cmd+T with a Path property selected enters free-transform path mode (AE-UI-343).

### AE-UI-342 · Paint tool shortcuts (绘画工具快捷键)

- **What it does:** Modifier and key shortcuts for Brush, Clone Stamp and Eraser in the Layer panel.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Swap foreground/background colour | X | X |
  | Reset colours to black/white | D | D |
  | Sample colour under pointer to foreground | Alt-click | Opt-click |
  | Sample 4×4 px average | Ctrl+Alt-click | Cmd+Opt-click |
  | Brush size | Ctrl-drag | Cmd-drag |
  | Brush hardness | Ctrl-drag, then release Ctrl while dragging | Cmd-drag, release Cmd |
  | Join stroke to previous stroke | hold Shift when starting stroke | same |
  | Clone Stamp: set source point | Alt-click | Opt-click |
  | Momentary Eraser, Last Stroke Only | Ctrl+Shift | Cmd+Shift |
  | Clone overlay: show and move (Offset / Source Position) | Alt+Shift-drag | Opt+Shift-drag |
  | Clone Stamp presets 1–5 | 3, 4, 5, 6, 7 (main keyboard) | same |
  | Duplicate clone preset | Alt-click preset button | Opt-click |
  | Opacity 10%…90% / 100% | Numpad 1…9 / Numpad . | same |
  | Flow 10%…90% / 100% | Shift+Numpad 1…9 / Shift+Numpad . | same |
  | Move by stroke Duration frames earlier / later | Ctrl+Page Up / Down or 1 / 2 (main keyboard) | Cmd+Page Up / Down or 1 / 2 |

- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Paint tools themselves: AE-UI-318. Numpad digits select layers elsewhere (AE-UI-334); with a paint tool active they set opacity.

### AE-UI-343 · Shape creation modifier keys (形状图层快捷键)

- **What it does:** Keys held while dragging out a shape with a shape tool, plus grouping and free-transform shortcuts for shape layers.
- **Exact semantics:**

  | Action | Key while dragging |
  |---|---|
  | Constrain: rectangle → square, ellipse → circle, polygon/star → 0° rotation | Shift |
  | Reposition the shape being drawn | hold Space |
  | Rounded rectangle roundness up / down; polygon/star points +1 / −1 | Up / Down Arrow |
  | Rounded rectangle roundness 0 / maximum; polygon/star outer roundness − / + | Left / Right Arrow |
  | Star inner roundness + / − | Page Up / Page Down |
  | Star outer radius only (inner radius fixed) | Ctrl (Win) / Cmd (Mac) |
  | Draw from centre (shape tools draw from centre by default in shape layers; corner-anchored for masks) | Ctrl/Cmd for masks (UNVERIFIED) |

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Group Shapes (编组形状) | Ctrl+G | Cmd+G |
  | Ungroup Shapes (取消组合形状) | Ctrl+Shift+G | Cmd+Shift+G |
  | Free-transform a Path property | select Path in Timeline, Ctrl+T | Cmd+T |

- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-vector)
- **Notes:** Shape tools and primitive parameters: shapes.md (shape tools). Selection-tool modifiers inside shape layers: AE-UI-305.

### AE-UI-344 · Marker shortcuts (标记快捷键)

- **What it does:** Add, number, navigate and remove composition and layer markers.
- **Exact semantics:**

  | Command (中文) | Win | Mac |
  |---|---|---|
  | Add marker at current time (works during preview and audio-only preview) | Numpad * | Numpad * or Ctrl+8 |
  | Add marker and open marker dialog | Alt+Numpad * | Opt+Numpad * or Ctrl+Opt+8 |
  | Set numbered comp marker 0–9 | Shift+0…9 (main keyboard) | same |
  | Go to comp marker 0–9 | 0…9 (main keyboard) | same |
  | Show duration between two markers/keys in Info | Alt-click both | Opt-click both |
  | Remove marker | Ctrl-click marker | Cmd-click marker |

  ```text
  Numpad * target: selected layers → layer marker on each selected layer; no layer selected → comp marker
  during preview the marker lands on the frame being displayed when the key is pressed
  ```
- **Priority:** P0 (add marker); P1 (rest)
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Marker data: project-comp-layers.md (prj-b), comp markers / layer markers.

### AE-UI-345 · Motion tracking shortcuts (运动跟踪快捷键)

- **What it does:** Keyboard nudges of track points in the Layer panel.
- **Exact semantics:**

  | Command | Win | Mac |
  |---|---|---|
  | Move feature region, search region and attach point 1 / 10 px | Arrow / Shift+Arrow | same |
  | Move feature region and search region only 1 / 10 px | Alt+Arrow / Alt+Shift+Arrow | Opt+Arrow / Opt+Shift+Arrow |

- **Priority:** P2
- **Owner:** bae-ui-motion
- **Notes:** Tracker panel: ui-preview-render-io.md (ui-a), Tracker.

### AE-UI-346 · Save, export and render shortcuts (保存、导出和渲染快捷键)

- **What it does:** Save variants, add to the Render Queue or Adobe Media Encoder, save a single frame.
- **Exact semantics:**

  | Command (中文) | Win | Mac | Notes |
  |---|---|---|---|
  | Save (保存) | Ctrl+S | Cmd+S | |
  | Save As (另存为) | Ctrl+Shift+S | Cmd+Shift+S | |
  | Increment and Save (增量保存) | Ctrl+Alt+Shift+S | Cmd+Opt+Shift+S | appends/increments a trailing number: "proj.aep" → "proj 2.aep" (UNVERIFIED exact naming) |
  | Add to Render Queue (添加到渲染队列) | Ctrl+M (menu) or Ctrl+Shift+/ | Cmd+M or Cmd+Shift+/ | active comp or selected items |
  | Save Frame As > File (add current frame to RQ) | Ctrl+Alt+S | Cmd+Opt+S | |
  | Add to Adobe Media Encoder Queue | Ctrl+Alt+M | Cmd+Opt+M | |
  | Duplicate render item with same output filename | Ctrl+Shift+D | Cmd+Shift+D | Render Queue focused |
  | Render (start render queue) | Render button; Enter/Return while RQ focused (UNVERIFIED) | | |

- **Priority:** P0 (save, add to RQ); P1 (frame save); P2 (AME)
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Render Queue semantics: ui-preview-render-io.md (ui-c). BAE decision: "Add to Adobe Media Encoder Queue" is not provided; the menu slot maps to BAE's own export queue.

## Menus

Menu inventory of AE 2025/2026 (25.x/26.x) on Windows. On macOS the application menu "After Effects" holds About, Settings (Preferences, renamed from "Preferences" in AE 24.0, UNVERIFIED version), Services, Hide, Quit, and File > Exit moves there. Shortcuts are Windows; Mac per AE-UI-322 conversion. Chinese strings that are not in the BAE glossary are from AE zh-CN menus as remembered and are collectively （译名待核） until checked against an AE zh-CN build. "UI-only" = no project-data semantics (handled entirely by the UI shell). Menu framework (accelerators, enable state, recent lists): bae-ui; menu content: apps/bae.

### AE-UI-347 · File menu (文件菜单)

- **What it does:** Project-level commands: new/open/save, import/export, footage maintenance, dependencies, scripts, project settings, exit.
- **Exact semantics:**

  | Command (中文) | Shortcut | Maps to | P |
  |---|---|---|---|
  | New > New Project (新建项目) | Ctrl+Alt+N | project-comp-layers.md (prj-a), projects | P0 |
  | New > New Team Project (新建团队项目) | — | Team Projects (removed AE 25.x, UNVERIFIED) | P2 |
  | New > New Folder (新建文件夹) | Ctrl+Alt+Shift+N | prj-a, folders | P0 |
  | New > Adobe Photoshop File… / MAXON CINEMA 4D File… | — | out of scope (BAE decision) | P2 |
  | Open Project… (打开项目) | Ctrl+O | prj-a, projects | P0 |
  | Open Team Project… | — | P2 / omitted | P2 |
  | Open Recent (打开最近的文件) ▸ list (default 10 entries, UNVERIFIED) | Ctrl+Alt+Shift+P = most recent | UI-only + prj-a | P0 |
  | Browse in Bridge… (在 Bridge 中浏览) | Ctrl+Alt+Shift+O | UI-only, omitted (BAE decision) | P2 |
  | Close (关闭) | Ctrl+W | AE-UI-326 | P0 |
  | Close Project (关闭项目) | — | prj-a | P0 |
  | Save (保存) / Save As ▸ Save As…, Save a Copy…, Save a Copy As XML…, Save a Copy As 24.x/… (older version) | Ctrl+S / Ctrl+Shift+S | prj-a, project save; XML copy (.aepx) P2 | P0 |
  | Increment and Save (增量保存) | Ctrl+Alt+Shift+S | AE-UI-346 | P1 |
  | Revert (恢复) | — | prj-a, reload last saved | P1 |
  | Import ▸ File… (文件), Multiple Files… (多个文件), Adobe Premiere Pro Project…, Pro Import After Effects…, Vanishing Point (.vpe)…, Placeholder… (占位符), Solid… (纯色) | Ctrl+I / Ctrl+Alt+I | prj-a, import / placeholders / solids; ui-c, import formats | P0 (File, Multiple, Placeholder, Solid); P2 (others) |
  | Import Recent Footage (导入最近的素材) ▸ | — | UI-only + prj-a | P1 |
  | Export ▸ Add to Adobe Media Encoder Queue…, Add to Render Queue, Export Adobe Premiere Pro Project… | Ctrl+Alt+M / Ctrl+M | ui-c, Render Queue; BAE adds Lottie export here (ui-c, Lottie) | P0 (Add to RQ) |
  | Add Fonts from Adobe… (从 Adobe 添加字体) | — | omitted | P2 |
  | Adobe Dynamic Link ▸ | — | omitted (BAE decision) | P2 |
  | Find… (查找) | Ctrl+F | ui-a, Project panel search | P1 |
  | Add Footage to Comp (将素材添加到合成) | Ctrl+/ | AE-UI-332 | P1 |
  | New Comp from Selection… (基于所选项新建合成) | Alt+\ | prj-b, composition creation | P0 |
  | Dependencies ▸ Collect Files… (收集文件), Consolidate All Footage (整合所有素材), Remove Unused Footage (删除未用过的素材), Reduce Project (减少项目), Find Missing Fonts / Effects / Footage | — | prj-a, Collect Files / Reduce Project / missing footage (bae-package) | P0 (Collect, Remove Unused); P1 (rest) |
  | Watch Folder… (监视文件夹) | — | omitted (BAE decision: render farm via CLI) | P2 |
  | Scripts ▸ Run Script File…, Open Script Editor (removed), installed scripts list | — | bae-plugin scripting; UI-only list | P1 |
  | Create Proxy ▸ Still… / Movie… (创建代理) | — | prj-a, proxies | P1 |
  | Set Proxy ▸ File… / None (设置代理) | Ctrl+Alt+P | prj-a, proxies | P1 |
  | Interpret Footage ▸ Main… (主要), Proxy…, Remember Interpretation, Apply Interpretation | Ctrl+Alt+G / Ctrl+Alt+C / Ctrl+Alt+V | prj-a, Interpret Footage | P0 |
  | Replace Footage ▸ File…, With Layered Comp, Placeholder…, Solid… | Ctrl+H | prj-a, replace footage | P1 |
  | Reload Footage (重新加载素材) | Ctrl+Alt+L | prj-a | P1 |
  | License… (许可) | — | omitted | P2 |
  | Reveal in Explorer / Finder (在资源管理器中显示) | — | UI-only (OS reveal) | P1 |
  | Reveal in Bridge | — | omitted | P2 |
  | Project Settings… (项目设置) | Ctrl+Alt+Shift+K | prj-a, project settings | P0 |
  | Exit (退出) | Ctrl+Q | apps/bae | P0 |

- **Priority:** P0
- **Owner:** apps/bae (+ bae-ui, bae-ui-motion)
- **Notes:** Menu order follows AE 2025; exact positions of Team Project and Bridge entries UNVERIFIED for AE 26.x. BAE decision: Adobe-ecosystem commands (Bridge, Dynamic Link, Team Projects, Add Fonts from Adobe, Photoshop/C4D new files) are not built; their slots are absent rather than greyed.

### AE-UI-348 · Edit menu (编辑菜单)

- **What it does:** Undo/history, clipboard (including property-link copies), duplicate/split, lift/extract work area, select, label, purge, templates, preferences, keyboard shortcuts.
- **Exact semantics:**

  | Command (中文) | Shortcut | Maps to | P |
  |---|---|---|---|
  | Undo <action> (撤消) / Redo <action> (重做) | Ctrl+Z / Ctrl+Shift+Z | prj-a, undo; label shows the action name | P0 |
  | History ▸ (历史记录) list of recent undo steps | — | prj-a, undo (jump back N steps) | P1 |
  | Cut / Copy / Paste (剪切/复制/粘贴) | Ctrl+X / C / V | bae-model clipboard Commands | P0 |
  | Copy with Property Links (带属性链接复制) | Ctrl+Alt+C (UNVERIFIED; conflicts with Remember Interpretation in Project panel scope) | ani-b, expressions (pastes `thisComp.layer(…)…` expressions) | P1 |
  | Copy with Relative Property Links (带相对属性链接复制) | — | ani-b | P1 |
  | Copy Expression Only (仅复制表达式) | — | ani-b | P1 |
  | Paste Mocha mask | — | omitted | P2 |
  | Clear (清除) | Delete | bae-model delete | P0 |
  | Duplicate (重复) | Ctrl+D | prj-b, layer editing ops | P0 |
  | Split Layer (拆分图层) | Ctrl+Shift+D | prj-b, layer editing ops | P0 |
  | Lift Work Area (提升工作区域) / Extract Work Area (提取工作区域) | — | prj-b, layer editing ops (lift leaves gap, extract ripples) | P1 |
  | Select All (全选) / Deselect All (全部取消选择) | Ctrl+A / Ctrl+Shift+A | UI selection | P0 |
  | Label ▸ (标签) 16 colours + None, Select Label Group | — | prj-b, labels | P1 |
  | Purge ▸ (清理) All Cache, All Memory & Disk Cache, All Memory, All Disk Cache, All 3D Cache, Undo, Image Cache Memory | Ctrl+Alt+Numpad / = All Memory | AE-UI-375 | P1 |
  | Edit Original (编辑原稿) | Ctrl+E | prj-a (open in associated app) | P2 |
  | Edit in Adobe Audition | — | omitted | P2 |
  | Team Project ▸ | — | omitted | P2 |
  | Templates ▸ Render Settings…, Output Module… (模板) | — | ui-c, templates | P1 |
  | Preferences ▸ (首选项) General, Previews, Display, Import, Output, Grids & Guides, Labels, Media & Disk Cache, Video Preview, Appearance, New Project, Auto-Save, Memory & Performance, Audio Hardware, Audio Output Mapping, Type, Scripting & Expressions, 3D, Notifications, Startup & Repair (list per AE 25, UNVERIFIED completeness) | Ctrl+Alt+; | ui-c, Preferences dialog | P0 |
  | Sync Settings ▸ | — | omitted | P2 |
  | Keyboard Shortcuts (键盘快捷键) | Ctrl+Alt+' | AE-UI-323 | P2 |
  | Paste Interpretation… / Paste Layer styles etc. contextual | — | context-dependent | P1 |

- **Priority:** P0
- **Owner:** apps/bae (+ bae-ui, bae-model)
- **Notes:** Mac: Preferences live under After Effects > Settings. BAE decision: Undo/Redo label strings come from the Command's i18n name; History is backed by Loro/undo stack (ARCHITECTURE undo model).

### AE-UI-349 · Composition menu (合成菜单)

- **What it does:** Composition creation and settings, trim/crop, render-queue entry points, preview, frame saving, pre-render, flowchart, responsive design.
- **Exact semantics:**

  | Command (中文) | Shortcut | Maps to | P |
  |---|---|---|---|
  | New Composition… (新建合成) | Ctrl+N | prj-b, Composition Settings | P0 |
  | Composition Settings… (合成设置) | Ctrl+K | prj-b, Composition Settings | P0 |
  | Set Poster Time (设置海报时间) | — | prj-b, comp poster frame | P2 |
  | Trim Comp to Work Area (将合成修剪至工作区域) | Ctrl+Shift+X | prj-b, work area | P1 |
  | Crop Comp to Region of Interest (将合成裁剪到目标区域) | — | prj-b + cmp-b, ROI | P1 |
  | Add to Adobe Media Encoder Queue… | Ctrl+Alt+M | omitted (BAE export queue) | P2 |
  | Add to Render Queue (添加到渲染队列) | Ctrl+M | ui-c, Render Queue | P0 |
  | Add Output Module (添加输出模块) | — | ui-c, multiple outputs | P1 |
  | Preview ▸ Play Current Preview (播放当前预览) Space, Audio (音频) Numpad ., Audio from work area | Space / Numpad . | AE-UI-363, AE-UI-373 | P0 |
  | Save Frame As ▸ File… (文件) / Photoshop Layers… | Ctrl+Alt+S | ui-c (single-frame output) | P1 (File); P2 (PSD layers) |
  | Pre-render… (预渲染) | — | ui-c + prj-b (render, then replace with output) | P2 |
  | Save Current Preview (保存当前预览) | Ctrl+Numpad 0 | AE-UI-376 | P2 |
  | Open in Essential Graphics (在基本图形中打开) | — | ui-a, Essential Graphics | P2 |
  | Responsive Design – Time ▸ Create Intro, Create Outro, Create Protected Region from Work Area | — | prj-b, comp markers (protected region) | P2 |
  | Composition Flowchart (合成流程图) | Ctrl+F11 | ui-a, Flowchart | P2 |
  | Composition Mini-Flowchart (合成微型流程图) | Tab | ui-a, mini-flowchart | P2 |
  | VR ▸ | — | omitted | P2 |

- **Priority:** P0
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** Ctrl+Numpad 0 for Save Current Preview is UNVERIFIED. "Save RAM Preview" was renamed "Save Current Preview" in AE CC 2015 (13.5).

### AE-UI-350 · Layer menu (图层菜单)

- **What it does:** Layer creation, settings, masks, quality, switches, transform, time, frame blending, 3D, markers, blending modes, track matte, layer styles, arrangement, reveal, create, camera, auto-trace, pre-compose.
- **Exact semantics:**

  | Command (中文) | Shortcut | Maps to | P |
  |---|---|---|---|
  | New ▸ Text (文本), Solid (纯色), Light (灯光), Camera (摄像机), Null Object (空对象), Shape Layer (形状图层), Adjustment Layer (调整图层), Content-Aware Fill Layer, Adobe Photoshop File, MAXON CINEMA 4D File | Ctrl+Alt+Shift+T / Ctrl+Y / Ctrl+Alt+Shift+L / C / Y / — / Ctrl+Alt+Y | prj-b, layer types | P0 (text…adjustment); P2 (rest) |
  | Layer Settings… (图层设置) | Ctrl+Shift+Y | prj-b | P0 |
  | Open Layer (打开图层) / Open Layer Source (打开图层源) | — / Alt-double-click | ui-a, Layer/Footage viewers | P0 |
  | Reveal in Explorer/Finder | — | UI-only | P1 |
  | Mask ▸ New Mask, Mask Shape…, Mask Feather…, Mask Opacity…, Mask Expansion…, Reset Mask, Remove Mask, Remove All Masks, Mode ▸, Inverted, Locked, Motion Blur ▸, Feather Falloff ▸, Reset Mask, Unlock All Masks, Lock Other Masks, Hide Locked Masks | Ctrl+Shift+N / M / F / I | cmp-a, masks | P0 |
  | Mask and Shape Path ▸ RotoBezier, Closed, Set First Vertex, Free Transform Points (Ctrl+T) | Ctrl+T | cmp-a masks; shapes.md bezier model | P0 |
  | Quality ▸ Best / Draft / Wireframe / Bilinear / Bicubic | Ctrl+U … | cmp-a, quality/sampling | P0 |
  | Switches ▸ Hide Other Video, Show All Video, Unlock All Layers, Shy, Lock, Audio, Video, Effect, Collapse, Motion Blur, Adjustment Layer, 3D Layer… | Ctrl+Shift+L etc. | prj-b, switches | P0 |
  | Transform ▸ Reset, Anchor Point…, Position…, Scale…, Orientation…, Rotation…, Opacity…, Flip Horizontal, Flip Vertical, Center Anchor Point in Layer Content, Center In View, Fit to Comp, Fit to Comp Width, Fit to Comp Height, Auto-Orient… | AE-UI-335/337 | ani-a, transform; prj-b, auto-orient | P0 |
  | Time ▸ Enable Time Remapping, Time-Reverse Layer, Time Stretch…, Freeze Frame, Freeze on Last Frame, Create Proxy (no), Rate Stretch | Ctrl+Alt+T / Ctrl+Alt+R | ani-a, time remap/stretch | P0 |
  | Frame Blending ▸ Off / Frame Mix / Pixel Motion | — | ani-a + cmp-a, frame blending | P1 |
  | 3D Layer (3D 图层) | — | prj-b + cmp-b | P0 |
  | Guide Layer (参考图层) | — | prj-b, guide layers | P1 |
  | Environment Layer (环境图层) | — | cmp-b, Advanced 3D | P2 |
  | Markers ▸ Add Marker, Update Marker to Data, Set Marker Duration…, Lock Markers, Remove All Markers | Numpad * | prj-b, layer markers | P1 |
  | Preserve Transparency (保持基础透明度) | — | cmp-a, preserve transparency | P0 |
  | Blending Mode ▸ list, Next Blending Mode, Previous Blending Mode | Shift+= / Shift+- | cmp-a, blend modes | P0 |
  | Track Matte ▸ (轨道遮罩) layer list / Alpha / Luma / Inverted | — | cmp-a, track mattes | P0 |
  | Layer Styles ▸ Convert to Editable Styles, Show All, Remove All, Drop Shadow… Stroke | — | prj-b, layer styles | P1 |
  | Group Shapes / Ungroup Shapes (shape layers) | Ctrl+G / Ctrl+Shift+G | shapes.md | P1 |
  | Arrange ▸ Bring Layer to Front, Bring Layer Forward, Send Layer Backward, Send Layer to Back | Ctrl+Shift+] / Ctrl+] / Ctrl+[ / Ctrl+Shift+[ | prj-b, stacking | P0 |
  | Reveal ▸ Reveal Layer Source in Project, Reveal Layer in Project Flowchart, Reveal Expression Errors | — | UI-only | P1 |
  | Create ▸ Convert to Editable Text, Create Shapes from Text, Create Masks from Text, Create Shapes from Vector Layer, Create Keyframes from Data, Convert to Layered Comp, Convert Audio to Keyframes (moved to Animation > Keyframe Assistant in AE, UNVERIFIED) | — | text.md / shapes.md / bae-motion commands | P1 |
  | Camera ▸ Link Focus Distance to Point of Interest, Link Focus Distance to Layer, Set Focus Distance to Layer, Create Stereo 3D Rig, Create Orbit Null | — | cmp-b, cameras | P1 |
  | Auto-trace… (自动追踪) | — | effects/shapes; P2 | P2 |
  | Scene Edit Detection… | — | omitted | P2 |
  | Pre-compose… (预合成) | Ctrl+Shift+C | prj-b, precompose | P0 |

- **Priority:** P0
- **Owner:** apps/bae (+ bae-ui-motion, bae-motion)
- **Notes:** Submenu completeness in AE 26.x UNVERIFIED (e.g. Create ▸ "Convert to 3D model" options). Context menu on a Timeline layer mirrors most of this menu (ui-a, Timeline layer context menu).

### AE-UI-351 · Effect menu (效果菜单)

- **What it does:** Opens Effect Controls, re-applies the last effect, removes all effects, and lists every installed effect by category.
- **Exact semantics:**
  - Items: Effect Controls (效果控件) F3; "<last effect name>" (上次使用的效果) Ctrl+Alt+Shift+E; Remove All (全部移除) Ctrl+Shift+E; separator; one submenu per category in alphabetical order of the localized category name: 3D Channel, Audio, Blur & Sharpen, Boris FX Mocha, Channel, CINEMA 4D, Color Correction, Distort, Expression Controls, Generate, Immersive Video, Keying, Matte, Noise & Grain, Obsolete, Perspective, Simulation, Stylize, Synthetic Aperture, Text, Time, Transition, Utility, plus third-party categories. Effects inside a category are alphabetical.
  - Choosing an effect applies it to every selected layer (appended after existing effects, or after the selected effect in Effect Controls, UNVERIFIED); with no layer selected the menu items are disabled.
- **Priority:** P0
- **Owner:** apps/bae (+ bae-effects, bae-plugin)
- **Notes:** Effect inventory and categories: effects.md (fx-a, fx-b, fx-c). BAE decision: the menu is generated from the effect registry (built-in + plug-ins), so categories without BAE effects are absent.

### AE-UI-352 · Animation menu (动画菜单)

- **What it does:** Animation presets, keyframe commands and assistants, text animators, expressions, separate dimensions, tracking, and property reveal.
- **Exact semantics:**

  | Command (中文) | Shortcut | Maps to | P |
  |---|---|---|---|
  | Save Animation Preset… (保存动画预设) | — | effects.md (fx-a) presets (.ffx) | P1 |
  | Apply Animation Preset… (应用动画预设) / Recent Animation Presets ▸ / Browse Presets… | Ctrl+Alt+Shift+F = last | fx-a presets | P1 |
  | Add <Property> Keyframe (添加关键帧) | — | ani-a, keyframes | P0 |
  | Toggle Hold Keyframe (切换定格关键帧) | Ctrl+Alt+H | ani-a | P0 |
  | Keyframe Interpolation… (关键帧插值) | Ctrl+Alt+K | ani-a | P0 |
  | Keyframe Velocity… (关键帧速度) | Ctrl+Shift+K | ani-a | P0 |
  | Keyframe Assistant ▸ (关键帧辅助) Convert Audio to Keyframes, Convert Expression to Keyframes, Easy Ease, Easy Ease In, Easy Ease Out, Exponential Scale, RPF Camera Import, Sequence Layers…, Time-Reverse Keyframes | F9 / Shift+F9 / Ctrl+Shift+F9 | ani-a keyframe assistants (bae-motion) | P0 (Easy Ease family, Time-Reverse, Sequence Layers); P1 (rest) |
  | Animate Text ▸ (动画文本) property list | — | text.md, animators | P0 |
  | Add Text Selector ▸ Range / Wiggly / Expression | — | text.md, selectors | P0 |
  | Remove All Text Animators (移除所有文本动画器) | — | text.md | P1 |
  | Add Expression (添加表达式) | Alt+Shift+= (UNVERIFIED) | ani-b | P0 |
  | Separate Dimensions (单独尺寸) | — | ani-a, separate dimensions | P0 |
  | Track Camera (跟踪摄像机) / Warp Stabilizer (变形稳定器) / Track Motion (跟踪运动) / Track Mask (跟踪蒙版) / Track this Property | — | omitted for M1 (tracking is P2) | P2 |
  | Reveal Properties with Keyframes (显示带有关键帧的属性) | U | AE-UI-336 | P0 |
  | Reveal Properties with Animation | — | AE-UI-336 (keyframes + expressions) | P1 |
  | Reveal All Modified Properties (显示所有修改的属性) | UU | AE-UI-336 | P0 |

- **Priority:** P0
- **Owner:** apps/bae (+ bae-motion, bae-ui-motion)
- **Notes:** Exact list of Keyframe Assistant entries in 26.x UNVERIFIED (third-party assistants also appear here).

### AE-UI-353 · View menu (视图菜单)

- **What it does:** Viewer creation, zoom, resolution, display colour, rulers/guides/grid, view options, layer controls, 3D view commands, go to time.
- **Exact semantics:**

  | Command (中文) | Shortcut | Maps to | P |
  |---|---|---|---|
  | New Viewer (新建查看器) | Ctrl+Alt+Shift+N | ui-a, viewers | P1 |
  | Zoom In / Zoom Out (放大/缩小) | . / , | AE-UI-330, ui-a magnification | P0 |
  | Resolution ▸ Full, Half, Third, Quarter, Custom… (完整/二分之一/三分之一/四分之一/自定义) | Ctrl+J … | cmp-b, preview resolution | P0 |
  | Use Display Color Management (使用显示色彩管理) | Shift+Numpad / | cmp-b, display transform | P1 |
  | Simulate Output ▸ (模拟输出) | — | cmp-b | P2 |
  | Show Rulers / Hide Rulers (显示标尺) | Ctrl+R | AE-UI-360 | P0 |
  | Show Guides (显示参考线) | Ctrl+; | AE-UI-359 | P0 |
  | Snap to Guides (对齐到参考线) | Ctrl+Shift+; | AE-UI-359 | P1 |
  | Lock Guides (锁定参考线) | Ctrl+Alt+Shift+; | AE-UI-359 | P1 |
  | Clear Guides (清除参考线) | — | AE-UI-359 | P1 |
  | Import Guides… / Export Guides… (导入/导出参考线) | — | AE-UI-359 (JSON .guides file) | P2 |
  | Show Grid (显示网格) | Ctrl+' | AE-UI-358 | P0 |
  | Snap to Grid (对齐到网格) | Ctrl+Shift+' | AE-UI-358 | P1 |
  | View Options… (视图选项) | Ctrl+Alt+U | ui-a, View Options | P1 |
  | Show Layer Controls (显示图层控件) | Ctrl+Shift+H | ui-a, View Options and Show Layer Controls | P0 |
  | Reset 3D View (重置 3D 视图) | — | cmp-b, 3D views | P1 |
  | Create Camera from 3D View (从 3D 视图创建摄像机) | — | cmp-b, cameras | P1 |
  | Switch 3D View ▸ (切换 3D 视图) Active Camera, Front, Left, Top, Back, Right, Bottom, Custom View 1–3 | — | cmp-b | P1 |
  | Assign Shortcut to "<view>" ▸ F10/F11/F12 | — | AE-UI-339 | P2 |
  | Switch to Last 3D View (切换到上一个 3D 视图) | Esc | AE-UI-339 | P1 |
  | Look at Selected Layers (查看选定图层) / Look at All Layers (查看所有图层) | Ctrl+Alt+Shift+\ / — | AE-UI-339 | P1 |
  | Go to Time… (转到时间) | Alt+Shift+J | AE-UI-328 | P0 |

- **Priority:** P0
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** Safe-zone and proportional-grid toggles are in the viewer's grid-and-guide menu, not the View menu (ui-a, Choose grid and guide options).

### AE-UI-354 · Window menu (窗口菜单)

- **What it does:** Workspaces, extensions, and a toggle entry for every panel (checked when open in the current workspace), plus open viewers and script UI panels.
- **Exact semantics:**
  - **Workspace ▸** (工作区) list of workspaces (Default, Animation, Essential Graphics, Small Screen, Standard, Text, …), separator, Save as New Workspace…, Edit Workspaces…, Save Changes to this Workspace, Reset to Saved Layout. **Assign Shortcut to "<current>" Workspace ▸** Shift+F10 / Shift+F11 / Shift+F12.
  - **Find Extensions on Exchange…**, **Extensions ▸** (Adobe CEP/UXP panels) — BAE decision: omitted.
  - Panel toggles (alphabetical in AE): Align (对齐), Audio (Ctrl+4), Brushes (Ctrl+9), Character (Ctrl+6), Content-Aware Fill, Effect Controls (F3), Effects & Presets (Ctrl+5), Essential Graphics, Info (Ctrl+2), Learn, Libraries, Lumetri Scopes, Mask Interpolation, Media Browser, Metadata, Motion Sketch, Paint (Ctrl+8), Paragraph (Ctrl+7), Preview (Ctrl+3), Progress, Properties, Smoother, Tools (Ctrl+1), Tracker, Wiggler; then the viewer entries Composition: <name>, Layer: <name>, Footage: <name>, Project (Ctrl+0), Render Queue (Ctrl+Alt+0), Timeline: <name>, Flowchart; then installed ScriptUI panels (.jsx in ScriptUI Panels folder).
- **Priority:** P0 (panel toggles, workspace submenu); P2 (extensions)
- **Owner:** bae-ui (+ apps/bae)
- **Notes:** Panel inventory and workspaces: ui-a. Exact panel list in 26.x UNVERIFIED (Libraries and Learn panels were removed/renamed in recent versions). BAE decision: BAE-specific panels (Collaboration, Agent) appear in the same alphabetical list.

### AE-UI-355 · Help menu (帮助菜单)

- **What it does:** Documentation, references, account and update entries.
- **Exact semantics:**
  - Items (AE 25): After Effects Help… (After Effects 帮助) F1; Scripting Help…; Expression Reference…; Effect Reference…; Animation Presets…; Keyboard Shortcuts…; Enable Logging / log file entries; Welcome / Home Screen; Provide Feedback…; System Compatibility Report…; Manage My Account…; Sign Out; Updates…; About After Effects… (Win; on Mac in the application menu).
- **Priority:** P1
- **Owner:** apps/bae
- **Notes:** UI-only. Exact AE 26.x list UNVERIFIED. BAE decision: links point to BAE's own docs (Chinese and English); "Keyboard Shortcuts…" opens the in-app keymap viewer (AE-UI-323) rather than a web page.

### AE-UI-356 · Context menu inventory (上下文菜单)

- **What it does:** Right-click (Win) / Ctrl-click or right-click (Mac) menus in each panel that repeat the relevant main-menu commands for the item under the pointer.
- **Exact semantics:**
  - Right-click first selects the item under the pointer if it is not already selected, then opens the menu for the selection.
  - Composition viewer on a layer: Layer menu subset (New, Layer Settings, Open Layer, Mask, Quality, Switches, Transform, Time, Frame Blending, 3D Layer, Guide Layer, Markers, Blending Mode, Track Matte, Layer Styles, Effect ▸, Arrange, Reveal, Create, Camera, Pre-compose…, Tracking ▸). On empty canvas: New ▸, Composition Settings, Preview, Switch 3D View, Rulers/Guides/Grid toggles (UNVERIFIED exact set).
  - Timeline layer: same as viewer layer menu plus Columns ▸ in header context; Project panel item: New Comp from Selection, Interpret Footage, Replace Footage, Reveal in Explorer, Label, Proxy commands; Effect Controls: effect list (Effect ▸), Reset, Remove, Save Selection as Animation Preset; Render Queue: render item commands.
- **Priority:** P1
- **Owner:** bae-ui (+ apps/bae, bae-ui-motion)
- **Notes:** Panel-specific context menus are itemised in ui-a (Project panel context menu, Composition panel context menu, Timeline layer context menu); this item only fixes the shared rule. BAE decision: every context-menu entry is the same command ID as its main-menu twin.

## Viewer interaction

Overlays in this section draw in the Composition, Layer and Footage viewers only; they are never rendered into previews' cached frames or final output. By default overlay toggles apply to the active view only in multi-view layouts (ui-a, 3D View popup and view layouts). Viewer toolbar buttons and View Options/Show Layer Controls: ui-a (Choose grid and guide options menu; View Options and Show Layer Controls; Layer bounding box and transform handles; Motion path display and editing).

### AE-UI-357 · Layer snapping in the viewer (对齐)

- **What it does:** While dragging a layer, anchor point or mask/shape point in the Composition panel, the grabbed feature jumps ("snaps") to nearby features of other layers, with a visual indicator.
- **Exact semantics:**
  - **Toggle.** Tools panel **Snapping (对齐)** checkbox, shown when the Selection, Pan Behind or 3D-capable tools are active; state is application-wide. Holding Ctrl (Win) / Cmd (Mac) during a drag inverts the current state for that drag (snapping off → temporarily on; on → temporarily off). Default state UNVERIFIED (off in a fresh install).
  - **Snap feature of the dragged item** (the "magnet"): at mouse-down, the feature of the grabbed layer nearest to the pointer among: layer-bounds corners, edge midpoints, centre, anchor point, mask vertices, mask/shape path edges (UNVERIFIED: edge midpoints). With the Pan Behind tool the anchor point is the magnet.
  - **Snap targets:** on every other visible, unlocked layer in the comp: layer-bounds corners and edges (slide along edges), centres, anchor points, mask vertices and mask edges; 3D layers' corners/edges/centres in 3D space (projected through the active view). Comp frame edges/centre are targets as well (UNVERIFIED). Locked layers, hidden (eye off) layers and the dragged layers themselves are not targets.
  - **Threshold:** a fixed screen-space radius around the magnet, independent of zoom (UNVERIFIED value; BAE: 8 logical px).
  - **Indicator:** a small square on the magnet; when snapped to a target vertex the square becomes a double square; when snapped to an edge the edge highlights as a line along which the feature slides.
  - **Grid/guide snapping** are separate toggles (View > Snap to Grid, Snap to Guides; AE-UI-358, AE-UI-359) and act on the dragged layer's bounding-box edges (and anchor point, UNVERIFIED) against grid lines/guides, independent of the Snapping checkbox.
  ```text
  on drag(pointer_delta):
     proposed = start_pos + pointer_delta (in comp space; Shift axis-constraint applied first)
     if snapping_active xor ctrl_held:
        m = magnet position after proposed move (screen space)
        t = nearest target within radius (vertices/centres preferred over edges)
        if t: proposed += comp_space(t - m)
     emit SetPosition(proposed) (one undo step per drag)
  ```
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Introduced AE CC (12.0, 2013) as Ctrl/Cmd-drag snapping; the Snapping checkbox came in a later version (UNVERIFIED: AE CC 2014 13.0). Snapping in the Timeline (Shift-drag of keys, layer bars and CTI) and in the Graph Editor: ui-a, Timeline / Graph Editor. Implementation hint: collect target features once per drag from cached layer bounds of the EvaluatedComp at CTI; use a 2D grid hash in screen space.

### AE-UI-358 · Grid and proportional grid (网格和对称网格)

- **What it does:** Shows a regular pixel grid and/or a proportional grid over the viewer for alignment; layers can snap to the regular grid.
- **Exact semantics:**
  - Toggles: View > Show Grid (显示网格) Ctrl+' / Cmd+'; Proportional Grid (对称网格) Alt+' / Opt+' (in the viewer's Grid and Guide Options menu); View > Snap to Grid (对齐到网格) Ctrl+Shift+' / Cmd+Shift+'.
  - Preferences > Grids & Guides (网格和参考线):

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Grid > Color (网格颜色) | RGB | UNVERIFIED (green-grey) | | | |
  | Grid > Style (网格样式) | enum | Lines | Lines / Dashed Lines / Dots (实线/虚线/点线) | | |
  | Grid > Gridline Every (网格间隔) | int | UNVERIFIED (100) | ≥ 1 | px | comp pixels, not screen |
  | Grid > Subdivisions (分区) | int | UNVERIFIED (10) | ≥ 1 | count | 1 = no subdivision lines |
  | Proportional Grid > Horizontal (水平) | int | UNVERIFIED (3) | ≥ 1 | count | cells across |
  | Proportional Grid > Vertical (垂直) | int | UNVERIFIED (3) | ≥ 1 | count | cells down |

  ```text
  regular grid (comp space, origin = comp top-left (0,0), independent of ruler origin — UNVERIFIED):
     major lines at x = k*G, y = k*G; minor lines at k*G/S (S = subdivisions), drawn thinner/fainter
     grid squares keep their pixel size when the comp size changes
  proportional grid: x_i = i*W/H_count (i = 1..H_count-1), y_j = j*H/V_count; scales with comp size
  pixel aspect: grid is in comp pixels; with PAR correction on it is displayed squeezed accordingly
  snap to grid: dragged layer's bounding-box edges/corners snap to the nearest major or minor line within the screen threshold
  ```
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** The proportional grid cannot be snapped to (legacy feature). Grid preferences are stored in the machine-independent preferences file. BAE decision: grid settings are per-user preferences, not project data. Implementation hint: draw grids as a single full-viewer quad in a WGSL overlay pass using `fract(coord / spacing)`.

### AE-UI-359 · Guides (参考线)

- **What it does:** Non-rendering horizontal and vertical reference lines, created by dragging from the rulers, that layers can snap to.
- **Exact semantics:**
  - Create: with rulers shown (AE-UI-360), drag from the horizontal ruler (horizontal guide) or vertical ruler (vertical guide) into the viewer. While dragging, the Info panel shows the guide position in px from the ruler origin (UNVERIFIED).
  - Move: drag a guide with the Selection tool (pointer changes to a double arrow over a guide) when guides are unlocked. Delete: drag it back onto a ruler (or outside the viewer). View > Clear Guides (清除参考线) removes all guides of the item.
  - View > Show Guides (显示参考线) Ctrl+; ; View > Snap to Guides (对齐到参考线) Ctrl+Shift+; ; View > Lock Guides (锁定参考线) Ctrl+Alt+Shift+; (locked guides cannot be moved or deleted; they remain snap targets).
  - View > Import Guides… / Export Guides…: JSON file with extension `.guides` listing each guide's orientation and position (AE CC 2015.3 / 13.8 or later, UNVERIFIED version and schema).
  - Storage: guides belong to the viewed item (composition, or layer/footage source) and are saved with the project. Positions are in item pixels relative to the item's top-left, stored as floating-point values (UNVERIFIED sub-pixel).
  - Preferences > Grids & Guides > Guides: **Color (颜色)** default UNVERIFIED (light blue); **Style (样式)** Lines / Dashed Lines, default Lines.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Scripting: `CompItem` / `AVItem.guides` (read-only array of {orientationType, positionType, position}), `addGuide(orientationType, position)`, `removeGuide(index)`, `setGuide(position, index)` (AE 16.1 / CC 2019.1, UNVERIFIED version). BAE decision: guides are bae-model data on the comp (CRDT-synced, so collaborators see them); show/lock/snap flags are per-user UI state.

### AE-UI-360 · Rulers and ruler origin (标尺)

- **What it does:** Shows pixel rulers along the top and left of a viewer; the ruler origin (zero point) can be moved.
- **Exact semantics:**
  - Toggle: View > Show Rulers / Hide Rulers (显示标尺) Ctrl+R / Cmd+R; per view.
  - Units: pixels of the viewed item (comp pixels), labelled at zoom-dependent intervals; tick for the pointer position follows the mouse.
  - Origin: default at the item's top-left (0,0). Drag from the top-left ruler intersection (corner box) into the viewer to set a new zero point; double-click the corner box to reset the origin to (0,0) (UNVERIFIED for AE; standard Adobe behaviour).
  - Origin affects ruler labels and guide position readouts only; it does not change layer Position values or the Info panel's X/Y readout (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-361 · Title/action safe and center-cut safe zones (标题/动作安全)

- **What it does:** Draws overscan safe-zone rectangles (and center-cut lines for 16:9 comps) over the viewer.
- **Exact semantics:**
  - Toggle: ' (apostrophe) or the viewer's Grid and Guide Options menu > Title/Action Safe (标题/动作安全).
  - Preferences > Grids & Guides > Safe Margins (安全边距):

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Action-Safe (动作安全) | float | 10 | 0–100 (UNVERIFIED) | % of frame width/height (total, split equally between both sides) | 90 % visible area |
  | Title-Safe (标题安全) | float | 20 | 0–100 | % | 80 % visible area |
  | Center-cut Action-Safe (中心剪切动作安全) | float | 32.5 | 0–100 | % of frame width | |
  | Center-cut Title-Safe (中心剪切标题安全) | float | 40 | 0–100 | % of frame width | |

  ```text
  W,H = item size in px
  action rect: x0 = W*a/200, x1 = W - W*a/200, y0 = H*a/200, y1 = H - H*a/200   (a = 10 -> 5 % per side)
  title  rect: same with t = 20
  center-cut (only if frame aspect ≈ 16:9; tolerance UNVERIFIED): vertical lines only
     x = W*c/200 and x = W - W*c/200, c = 32.5 (action) and 40 (title)
     check: 16:9 -> 4:3 crop removes 25 % of width; 25 + 0.75*10 = 32.5 ; 25 + 0.75*20 = 40
  a centre cross-hair is drawn with the safe zones (UNVERIFIED)
  ```
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** Help text describes action-safe as 90 % and title-safe as 80 % of width and height (5 % / 10 % margins per side). BAE decision: safe-zone percentages are per-user preferences.

### AE-UI-362 · Motion path display preference (运动路径)

- **What it does:** Limits how much of a layer's spatial motion path (and its keyframe boxes) is drawn in the Composition panel.
- **Exact semantics:**
  - Preferences > Display (显示) > Motion Path (运动路径), radio group:

  | Option (中文) | Value | Default | Units |
  |---|---|---|---|
  | No Motion Path (无运动路径) | — | | |
  | All Keyframes (所有关键帧) | — | | |
  | No More Than N Keyframes (不超过 N 个关键帧) | int | N = 5 (UNVERIFIED) | keyframes before and after CTI |
  | No More Than T (不超过 … 时间) | time | selected by default, T = 0:00:15:00 (UNVERIFIED) | duration before and after CTI |

  ```text
  draw path for every selected layer with spatial animation (Position, Anchor Point, effect point params, Point of Interest)
  All Keyframes      : whole path from first to last key
  N keyframes        : path segments between keys k-N .. k+N where k = key index nearest CTI
  T                  : path sampled for times in [t - T, t + T]
  path shows: keyframe boxes (selected = filled), one dot per frame (spacing = speed), Bezier tangent handles
  ```
  - Related: Preferences > General **Path Point and Handle Size (路径点和手柄大小)** sets vertex/handle size for masks, shapes and motion paths (default 5 px, UNVERIFIED; range UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Motion-path drawing and editing in the viewer: ui-a, Motion path display and editing. Layer controls must be visible (Ctrl+Shift+H) and View Options > Motion Paths on for any path to show. Implementation hint: sample the evaluated position curve per frame on the CPU in bae-anim, upload as a polyline, cache by (property revision, range).

## Preview and cache

Sources: Adobe help "Previewing" (previewing.html, 2025/2026 revision), "Memory, storage and performance" (memory-storage.html), Preferences pages. Preview resolution, ROI, Fast Previews, Adaptive Resolution and viewer quality are in compositing-3d-color.md (cmp-b); the Preview panel's layout as a panel and the Timeline cache-indicator drawing are in ui-a (Preview panel; Cache indicators); snapshots are in ui-a (Snapshots); Always Preview This View / Primary Viewer are in ui-a.

### AE-UI-363 · Preview playback transport (预览播放控件)

- **What it does:** Starts and stops playback of the active (or primary) viewer's composition, layer or footage, and steps the current time, from the Preview panel, menus and shortcuts.
- **Exact semantics:**
  - Preview panel transport buttons, left to right: First Frame (第一帧) = Home; Previous Frame (上一帧) = Page Up (Shift-click: 10 frames); Play/Stop (播放/停止); Next Frame (下一帧) = Page Down (Shift-click: 10 frames); Last Frame (最后一帧) = End.
  - Start: any of the five preview shortcuts (Space, Shift+Space, Numpad 0, Shift+Numpad 0, Alt/Opt+Numpad 0) or the two audio shortcuts (Numpad ., Alt/Opt+Numpad .) starts a preview with **that shortcut's settings** (AE-UI-364). The Play button and Composition > Preview > Play Current Preview use the settings of the shortcut currently shown in the panel's **Shortcut** (快捷键) menu.
  - Target viewer: the Primary Viewer if one is set, else the most recently active viewer (Composition, Layer or Footage). A comp preview plays in its viewer; the Timeline CTI follows the "preview time" (a separate playhead shown during playback, UNVERIFIED drawing).
  - Stop: pressing any preview shortcut, Play/Stop, Play Current Preview, or Esc. A preview also stops when the user clicks in the previewing viewer, changes a time property (work area, time navigator), drags the CTI or clicks the time ruler without Alt/Opt, changes Composition Settings of the previewing comp, hides the previewing viewer behind another panel in its group, or starts the Render Queue. Selecting the viewer's tab does not stop it. Stop behaviour per shortcut: AE-UI-367.
  - **Reset** (重置) button: restores defaults for all shortcuts; Alt/Opt-click Reset restores AE CC 2014-style behaviour (Spacebar ≈ old Standard Preview, Numpad 0 ≈ old RAM Preview with Cache Before Playback on).
  - **Mute Audio** (静音) toggle in the panel mutes audio for all previews.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render, bae-media)
- **Notes:** AE CC 2015 (13.5) replaced RAM Preview / Standard Preview with this unified preview architecture. Scripting: no direct preview API besides `app.executeCommand` (UNVERIFIED). BAE decision: preview is driven by a playback clock on its own thread; the UI thread only displays frames delivered by the cache.

### AE-UI-364 · Per-shortcut preview settings (预览快捷键设置)

- **What it does:** Each preview shortcut stores its own Include Video / Audio / Overlays, Loop, Cache Before Playback, Range, Play From, Frame Rate, Skip, Resolution, Full Screen and stop options.
- **Exact semantics:**

  | Parameter (中文) | Type | Notes |
  |---|---|---|
  | Shortcut (快捷键) | enum | Spacebar, Shift+Spacebar, Numpad 0, Shift+Numpad 0, Alt/Opt+Numpad 0 (panel menu); Numpad . and Alt/Opt+Numpad . have their own stored settings (UNVERIFIED whether shown in the menu) |
  | Include Video (包括视频) | bool | |
  | Include Audio (包括音频) | bool | |
  | Include Overlays and Layer Controls (包括叠加和图层控件) | bool | overlays per viewer's grid/guide menu; layer controls per View Options; Ctrl/Cmd+Shift+H during playback toggles layer controls without changing this setting |
  | Loop (循环) | enum | Loop (循环) / Play Once / Ping-pong (UNVERIFIED: AE has Loop and Play Once only in 2015+; Ping-pong was removed) |
  | Cache Before Playback (播放前缓存) | bool | cache whole range first, then play in real time |
  | Range (范围) | enum | AE-UI-365 |
  | Play From (播放自) | enum | Range Start (范围开头) / Current Time (当前时间) |
  | Frame Rate (帧速率) | Auto or number | AE-UI-366 |
  | Skip (跳过) | int | AE-UI-366 |
  | Resolution (分辨率) | enum | AE-UI-366 |
  | Full Screen (全屏) | bool | AE-UI-366 |
  | If caching, play cached frames (如果缓存，则播放缓存的帧) | bool | AE-UI-367 |
  | Move time to preview time (将时间移到预览时间) | bool | AE-UI-367 |

  Rule: Include Video, Include Audio and Include Overlays cannot all be off; turning off the last one turns another one back on.

  **Defaults** (help page, AE 2025/2026):

  | Setting | Space | Shift+Space | Numpad 0 | Shift+Numpad 0 | Alt+Numpad 0 | Numpad . | Alt+Numpad . |
  |---|---|---|---|---|---|---|---|
  | Video | On | On | On | On | On | Off | Off |
  | Audio | On | On | On | On | On | On | On |
  | Overlays & layer controls | Off | Off | Off | Off | Off | Off | Off |
  | Cache before playback | Off | Off | Off | Off | Off | Off | Off |
  | Range | WA ext. by CTI | WA ext. by CTI | Work Area | WA ext. by CTI | Play Around Current Time | WA ext. by CTI | WA ext. by CTI |
  | Play From | Current Time | Current Time | Range Start | Range Start | Range Start | Current Time | Range Start |
  | Frame Rate | Auto | Auto | Auto | Auto | Auto | Auto | Auto |
  | Skip | 0 | 0 | 0 | 1 | 0 | 0 | 0 |
  | Resolution | Auto | Auto | Auto | Auto | Auto | Auto | Auto |
  | Full Screen | Off | Off | Off | Off | Off | Off | Off |
  | If caching, play cached frames | Off | On | On | On | On | On | On |
  | Move time to preview time | On | Off | Off | Off | Off | Off | Off |

  Loop default: Loop for all shortcuts (UNVERIFIED). Alt/Opt-click Reset additionally turns Cache Before Playback on for Numpad 0.
- **Priority:** P0 (Space and Numpad 0 behaviour); P1 (full per-shortcut editing)
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Settings are application preferences, not project data. Mac: Alt = Opt. BAE decision: store as a per-user preview profile table keyed by shortcut ID so that per-workspace keymaps (AE-UI-322) can bind other keys to the same profiles.

### AE-UI-365 · Preview range (预览范围)

- **What it does:** Defines which frames a preview plays and caches.
- **Exact semantics:**
  - Enum (menu order): **Work Area** (工作区域); **Work Area Extended By Current Time** (由当前时间扩展的工作区域); **Entire Duration** (整个持续时间); **Play Around Current Time** (围绕当前时间播放) with a settings dialog for Preroll and Postroll (seconds). No "Custom" range exists in AE (the comp work area is the custom range).
  ```text
  let [ws, we) = work area, t = CTI, [cs, ce) = comp (or layer/footage) duration
  Work Area                 : [ws, we)
  Entire Duration           : [cs, ce)
  Work Area Ext. by CTI     : if t <  ws          -> [t, we)
                              if t >= we          -> [ws, t]          (inclusive of CTI frame)
                                 and if Play From = Current Time -> [ws, ce)   (help: "from start point to last frame")
                              else (inside)       -> [ws, we)
  Play Around Current Time  : [t - preroll, t + postroll] clamped to [cs, ce); preroll/postroll defaults UNVERIFIED (2 s / 2 s);
                              older AE: "Alternate Preview" preference = 5 frames before t (Alt+Numpad 0)
  Layer / Footage viewers   : work area = the viewer's own in/out range (UNVERIFIED)
  ```
  - Play From: **Range Start** starts at the range start; **Current Time** starts at CTI (if CTI is outside the range, playback starts at range start, UNVERIFIED) and wraps to range start when looping.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Work area data: project-comp-layers.md (prj-b), work area.

### AE-UI-366 · Preview frame rate, skip, resolution and full screen (预览帧速率、跳过、分辨率和全屏)

- **What it does:** Per-shortcut overrides of playback rate, frame skipping, render resolution and full-screen output.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Frame Rate (帧速率) | Auto or float | Auto | menu presets (UNVERIFIED list: 8, 10, 12, 12.5, 15, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120) or typed value | fps | Auto = comp frame rate (layer/footage: its frame rate) |
  | Skip (跳过) | int | 0 (Shift+Numpad 0: 1) | 0–? (UNVERIFIED max 10) | frames | render 1 of every (skip+1) frames |
  | Resolution (分辨率) | enum | Auto | Auto, Full, Half, Third, Quarter, Custom… | | Auto = viewer resolution |
  | Full Screen (全屏) | bool | Off | | | plays on the full screen of the monitor showing the viewer (UNVERIFIED), Esc stops |

  ```text
  played frames = { range_start + k*(skip+1)*frame_dur(comp) } restricted to range
  each played frame is held for (skip+1) / preview_fps seconds -> playback speed = preview_fps / comp_fps
  frame_rate != Auto changes playback speed (time is NOT resampled); audio plays at real speed only when preview_fps == comp_fps (UNVERIFIED: audio is muted otherwise)
  Resolution override replaces the viewer's resolution for this preview only; cached frames are keyed by resolution, so changing it uses a different cache set
  ```
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Resolution semantics (downsample factors, Custom): compositing-3d-color.md (cmp-b), preview resolution.

### AE-UI-367 · Stopping a preview (停止预览)

- **What it does:** What happens to playback, caching and the current time when a preview is stopped with a given shortcut.
- **Exact semantics:**
  - Options stored on the shortcut **used to stop** (not the one that started):
    - **If caching, play cached frames**: on → stopping while caching is still in progress stops only caching; playback of the cached frames then restarts from the beginning of the range and continues (press again to stop playback). Off → stops both caching and playback.
    - **Move time to preview time**: on → after stopping, the CTI moves to the frame that was showing (the preview time). Off → CTI returns to where it was before the preview started.
  - Esc stops caching and playback without moving the CTI (UNVERIFIED that Esc honours neither option).
  ```text
  state machine: Idle -> (start) -> Caching+Playing (if !cache_before) | Caching (if cache_before)
     Caching complete -> PlayingRealtime (loops if Loop)
     stop(sc): if state has Caching and sc.if_caching_play_cached -> PlayingCached(from range start)
               else -> Idle; if sc.move_time -> CTI := preview_time
  ```
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Defaults per shortcut: AE-UI-364 (Space: move time on, play-cached off).

### AE-UI-368 · Real-time and non-real-time playback, fps display (实时播放和帧速率显示)

- **What it does:** Plays cached frames in real time and uncached frames as fast as they render, reporting the achieved frame rate.
- **Exact semantics:**
  - While caching, frames are displayed as soon as they are rendered; if rendering is slower than the target rate playback runs slower than real time — AE does not drop frames (UNVERIFIED; the help describes no frame dropping and offers Skip instead). Once the whole range is cached, playback loops in real time from cache.
  - Audio: plays in sync only when playback is real time; Preferences > Previews > **Mute Audio When Preview Is Not Real-time** (预览非实时时将音频静音) (default UNVERIFIED: off) mutes it otherwise; when not muted, audio stutters to stay in sync.
  - **Info panel** during preview shows the preview frame rate and whether it is real time: real-time text in green, non-real-time in red with the achieved fps (UNVERIFIED exact wording, e.g. "Frame Rate: 25 (Realtime)"). The current preview frame number/time is shown too.
  - Cached-frame playback uses only cached frames (RAM or, with Preview from Disk Cache, disk); if a cached frame is evicted during playback it is re-rendered (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render, bae-media)
- **Notes:** BAE decision: performance target (VISION): common motion-graphics comps play in real time at 1080p **without** a caching pass on reference hardware; see AE-UI-378. BAE may drop frames in a "real-time" mode (opt-in) to keep audio sync, but the AE-compatible default is "no drop, slow down".

### AE-UI-369 · RAM cache (内存缓存)

- **What it does:** Stores rendered frames (and intermediate layer/footage images) in RAM so that previews and edits reuse them; cached comp frames are marked with a green bar in the time ruler.
- **Exact semantics:**
  - Cached items: final comp frames at a given resolution/settings, plus layer-level and footage-level images; unmodified layers are composited from the cache, modified layers re-render. Frames that render very quickly are not cached. Frames are stored uncompressed.
  - **Global RAM cache** (AE CS6 11.0+): cache entries are keyed by the content that produced them, so cached frames come back after Undo/Redo, after returning a layer to an earlier state (switch visibility off and on), on identical frames elsewhere in time (loops, time remap, pasted keys), on duplicated layers/comps; rendering from the Render Queue with settings other than Current Settings does not destroy the cache.
  - Invalidation: any change affecting a frame's inputs removes the green bar for the affected frames only (e.g. editing a keyframe invalidates frames whose evaluation depends on that property's affected span; changing a layer outside a frame's time does not invalidate that frame).
  - Capacity: Preferences > Memory & Performance — RAM reserved for other applications; AE uses the rest (shared with Adobe apps). When full, new frames replace older ones (help: replacement of earlier frames, LRU-like, UNVERIFIED policy); during preview caching stops when full and playback uses only cached frames.
  - Indicators: green bar = RAM-cached frame (Timeline, Layer, Footage time rulers); layer cache indicators per layer when enabled by Ctrl/Cmd-choosing **Show Cache Indicators** in the Timeline panel menu.
  - RAM cache is purged on quit; Purge commands: AE-UI-375.
- **Priority:** P0
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** Cache indicator drawing: ui-a, Cache indicators. Implementation hint: key frames by a 128-bit hash of (EvaluatedComp subtree content hash, time, resolution, ROI, bpc, color settings); keep GPU-resident textures for the most recent frames and spill to CPU RAM; invalidation then is implicit (new hash) and undo restores hits for free.

### AE-UI-370 · Disk cache and persistent disk cache (磁盘缓存)

- **What it does:** Stores rendered frames on disk; frames persist across sessions and projects; with Preview from Disk Cache, playback can run from disk when RAM is full. Disk-cached frames show a blue bar.
- **Exact semantics:**
  - Preferences > Media & Disk Cache (媒体和磁盘缓存) > Disk Cache:

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Enable Disk Cache (启用磁盘缓存) | bool | On | | | |
  | Maximum Disk Cache Size (最大磁盘缓存大小) | int | 10 % of the volume size, max 100 | ≥ 1 (UNVERIFIED) | GB | AE requires 10 GB free beyond this and warns otherwise |
  | Choose Folder (选择文件夹) | path | per-user cache folder | | | fast separate SSD recommended |
  | Empty Disk Cache (清空磁盘缓存) | button | | | | |
  | Enable Preview from Disk Cache (启用从磁盘缓存预览) | bool | On | | | High Performance Preview Playback, AE 25.x (UNVERIFIED version) |
  | Enable Compressed Frames (Lossless) (启用压缩帧（无损）) | bool | UNVERIFIED (Off) | | | AE 26.0 (UNVERIFIED, earlier Beta) |

  - A frame is written to disk only if reading it back is faster than re-rendering it. Blue bar = frame available on disk (Timeline). When both, green wins (UNVERIFIED drawing priority).
  - Persistent: after a project is saved, disk-cached frames survive closing the project and quitting; when a project opens, the cache is scanned for matching frames and blue marks fill in progressively. Frames are shared between projects that produce identical frames. Roto Brush frames are not persistent. Purging one AE version's disk cache does not affect other versions.
  - Conformed Media Cache (Database and Cache folders, Clean Database & Cache) holds conformed audio/indexed media; it is not the frame cache (bae-media).
- **Priority:** P1
- **Owner:** bae-render (+ bae-media, bae-ui-motion)
- **Notes:** Scripting: `app.preferences` keys only (UNVERIFIED). BAE decision: disk cache entries are content-addressed (same key as AE-UI-369) and compressed losslessly by default (e.g. zstd of half-float/8-bit planes); persistent across sessions without requiring a save.

### AE-UI-371 · Cache frames when idle (空闲时缓存帧)

- **What it does:** Renders and caches frames in the background when the user has been idle for a set delay.
- **Exact semantics:**
  - Preferences > Previews (预览) > Cache Frames When Idle (空闲时缓存帧):

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Cache Frames When Idle (enable) | bool | UNVERIFIED (On) | | | |
  | Idle Delay Before Caching Starts | float | UNVERIFIED (2) | ≥ 0 (UNVERIFIED) | s | |
  | Cache Frames | enum | UNVERIFIED (Around Current Time) | From Current Time / Around Current Time / From Start of Range | | Around = alternately one frame after and one before CTI |
  | Cache Range | enum | UNVERIFIED (Work Area) | Work Area / Work Area Extended by Current Time / Entire Duration | | |

  - Any user interaction pauses idle caching; it resumes after the delay. Uses Multi-Frame Rendering.
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** Introduced AE 22.0 (2021) with Multi-Frame Rendering ("speculative preview"; UNVERIFIED name). The older "Cache Work Area in Background" (Composition menu, AE CC 2015.3–2020) used separate processes and the disk cache. BAE decision: background caching is on by default with 0.5 s delay and the "Around Current Time" order, because BAE's renderer is cheap enough.

### AE-UI-372 · Live Update, Caps Lock and interactive scrubbing (实时更新和手动预览)

- **What it does:** Controls whether viewers re-render while the user drags values, layers or the CTI.
- **Exact semantics:**
  - **Live Update** (实时更新) toggle in the Timeline panel (on by default): on → viewers re-render continuously during drags; off → only on mouse-up. Holding Alt (Win) / Opt (Mac) while dragging inverts the Live Update state for that drag.
  - Dragging the CTI (Timeline time ruler, Layer/Footage time ruler) scrubs video: each new time is rendered at the Fast Previews/Adaptive Resolution quality (cmp-b) and shown; cached frames show immediately. Shift-drag of the CTI snaps to keyframes, markers, in/out points, comp start/end and work-area start/end.
  - **Caps Lock** on: viewers stop updating; a red bar with a text reminder appears at the bottom of each viewer whose image is stale; turning Caps Lock off refreshes.
  - With Mercury Transmit to a video monitor and an OpenGL/GPU preview path, the external monitor updates only on mouse-up (help).
- **Priority:** P0 (CTI scrub, Live Update); P2 (Caps Lock)
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Share View Options (Select View Layout menu) makes Ctrl/Cmd scrubbing update multiple views (ui-a). BAE decision: Caps Lock is not bound by default (OS-level Caps Lock is unreliable on macOS); offer "Pause Viewer Updates" command instead.

### AE-UI-373 · Audio preview (音频预览)

- **What it does:** Plays comp audio during previews and audio-only previews, through the configured audio device.
- **Exact semantics:**
  - Audio-only previews: Numpad . (from current time, Range = WA ext. by CTI, Play From = Current Time) and Alt/Opt+Numpad . (from range start); Mac alternates Ctrl+. and Ctrl+Cmd+. on the main keyboard. Include Video is off for both. Composition > Preview > Audio (UNVERIFIED menu label).
  - Mixed audio = sum of all audible layers (Audio switch on, not muted by solo) with Audio Levels (dB) and effects from the Audio category applied; output at the device sample rate (AE resamples; comp audio sample rate set in Preview prefs/Audio Hardware, UNVERIFIED).
  - Preferences > Audio Hardware (音频硬件): Device Class, Default Output, Buffer Size, Sample Rate; Preferences > Audio Output Mapping (音频输出映射). Audio panel shows VU meters during playback.
  - Audio is not cached as frames; it is rendered ahead of playback (UNVERIFIED).
  - Mute Audio toggle (Preview panel) and Include Audio per shortcut (AE-UI-364).
- **Priority:** P0 (audio in Space preview); P1 (audio-only shortcuts)
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** Audio effects: effects.md (fx-c), Audio. Audio panel UI: ui-a. Implementation hint: mix on a real-time audio thread (cpal) from pre-decoded PCM; the playback clock is the audio clock when audio is enabled.

### AE-UI-374 · Audio scrubbing (音频擦洗（译名待核）)

- **What it does:** Plays short audio snippets while the CTI is dragged.
- **Exact semantics:**
  - Keyboard shortcuts page: Ctrl-drag (Win) / Cmd-drag (Mac) the CTI = scrub audio. Previewing page (2025/2026): Ctrl+Alt-drag / Cmd+Opt-drag = scrub audio only; Ctrl-drag / Cmd-drag = scrub audio and video. The two pages disagree (UNVERIFIED which is current; BAE follows the previewing page).
  - Each CTI move plays a short slice of audio starting at the new time (slice length UNVERIFIED, about one frame's worth looped/held while the mouse is stationary); dragging speed changes pitch is NOT applied (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** —

### AE-UI-375 · Purge memory and caches (清理)

- **What it does:** Frees RAM and/or disk caches and the undo history on demand.
- **Exact semantics:**
  - Edit > Purge (清理) ▸ (AE 2025/2026 memory help page order): **All Cache** (所有缓存), **All Memory & Disk Cache** (所有内存和磁盘缓存), **All Memory** (所有内存; Ctrl+Alt+Numpad / or Cmd+Opt+Numpad /), **All Disk Cache** (所有磁盘缓存), **All 3D Cache** (所有 3D 缓存), **Undo** (撤消), **Image Cache Memory** (图像缓存内存). Snapshot purge: Ctrl/Cmd+Shift+F5…F8.
  - All Memory: clears RAM image cache, snapshot? (UNVERIFIED), and the undo history. Undo: clears the undo stack only. Image Cache Memory: clears RAM frames, keeps undo. All Memory & Disk Cache: All Memory + Empty Disk Cache (confirmation dialog). "All Cache" also clears conformed media/database? (UNVERIFIED). All 3D Cache: Advanced 3D model/texture caches.
  - Purging does not synchronize the project database (help).
- **Priority:** P1
- **Owner:** bae-render (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision: "Purge Undo" is disabled while a collaborative session is active (history is shared CRDT state; ARCHITECTURE).

### AE-UI-376 · Save Current Preview (保存当前预览)

- **What it does:** Writes the frames of the current preview range to a movie file via the Render Queue using the cached frames.
- **Exact semantics:**
  - Composition > Save Current Preview; shortcut Ctrl+Numpad 0 / Cmd+Numpad 0 (UNVERIFIED). Uses the range and resolution of the last preview; adds a render item with output module template from Edit > Templates > Output Module "Default for RAM previews" (UNVERIFIED label) and renders immediately; cached frames are reused, uncached frames rendered.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** Render Queue templates: ui-c.

### AE-UI-377 · Mercury Transmit external video preview (Mercury Transmit 视频预览)

- **What it does:** Sends viewer frames to an external monitor or video device (Adobe Monitor, AJA, Blackmagic, etc.).
- **Exact semantics:**
  - Preferences > Video Preview (视频预览): **Enable Mercury Transmit** (启用 Mercury Transmit) (default Off), device list with per-device setup, **Disable video output when in the background**, **Preview output during render queue output** (UNVERIFIED label), **Enable Extended Display Range / HDR preview**. Toggle with Numpad / (Mac also Ctrl+Shift+/ or Ctrl+/ per page).
  - The output follows the Primary Viewer or the most recently active viewer; frames sent at comp resolution scaled to the device format; colour-managed per the output simulation (cmp-b).
- **Priority:** P2
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** BAE decision: M1 provides only "second-monitor full-screen preview" via a borderless window (no SDI devices); Mercury Transmit device API is out of scope.

### AE-UI-378 · BAE preview performance targets (BAE 预览性能目标)

- **What it does:** States BAE's measurable preview goals that replace AE's "cache first, then real time" model.
- **Exact semantics:**
  - BAE decision (VISION): common motion-graphics comps preview in real time at 1080p (1920×1080, comp fps up to 60) at Full resolution on reference hardware, from the first play, without a pre-caching pass. Final render ≥ 5× faster than AE on the same machine and benchmark project.
  - Interactive latency: value drag → updated viewer frame ≤ 1 frame (16.7 ms at 60 Hz) for benchmark comps; CTI scrub of uncached frames ≥ 30 fps at Half resolution (BAE target, to be measured).
  - Measurement: `cargo xtask bench` (UNVERIFIED tool name) runs benchmark comps headless and through the viewer path, reporting achieved fps and dropped/slow frames; the Info panel shows the same fps/real-time indicator as AE-UI-368.
  - Fallbacks when a frame misses its deadline: show the previous frame, continue rendering (AE behaviour), Adaptive Resolution steps down per cmp-b.
- **Priority:** P0
- **Owner:** bae-render (+ bae-ui-motion, bae-anim)
- **Notes:** Reference hardware and benchmark set are defined by workstream 00/QA (not in this file). Implementation hint: evaluate on one thread, record GPU command buffers per frame, pipeline N+1 evaluation while N renders; cache only expensive subtrees (precomps, heavy effects) by content hash.
