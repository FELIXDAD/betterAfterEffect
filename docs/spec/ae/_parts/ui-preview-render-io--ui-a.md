# After Effects inventory: UI, preview, render queue and I/O (AE-UI)

**Scope.** This file lists the After Effects user interface that BAE must reproduce in behaviour (layout and interaction, never Adobe visuals or icons) plus the user-facing preview, rendering and I/O features: every panel and viewer with its key behaviours, the Timeline in full, the tools with their shortcuts and mouse behaviour, essential keyboard shortcuts (Mac and Windows), menus, viewer features (grids, guides, rulers, safe margins, snapping), preview and cache behaviour, the Render Queue (render settings, output modules, templates), Preferences, basic import/export formats and headless rendering. Rendering, animation and data semantics live in the sibling files and are cross-referenced, not repeated: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md and ui-preview-render-io.md (this file). The item format, ID scheme, priorities and owners are defined in README.md.

## Application window, panels and workspaces

### AE-UI-001 · Panel framework and application window (面板与应用程序窗口)

- **What it does:** The application window is one frame tree of docked panels; every panel (Project, Composition, Timeline, Effect Controls, ...) lives in a panel group (tab well), can be opened/closed from the Window menu and has a panel menu, a close button and an optional context menu.
- **Exact semantics:**
  - Layout model: a tree of frames; each leaf is a **panel group** (tab well + content area). A group shows one active tab; scrolling the mouse wheel over the tab row cycles the active tab; when the tabs do not fit, a tab overflow scroller/chevron appears.
  - Every panel tab has: a gripper (drag handle, left part of the tab), the title, a close button (×) and a **panel menu** button (☰) at the top-right of the group that opens commands for the active panel. Viewer panels (Composition, Layer, Footage, Flowchart, Effect Controls) additionally show a **viewer menu** when the item name in the tab is clicked (see AE-UI-008).
  - Window menu toggles every panel; the panel's keyboard shortcut (table below) toggles it: if closed it opens and is brought to the front of its group; if open and frontmost it closes. Choosing a panel already open but hidden behind other tabs brings it to the front.
  - Closing a panel group gives its space to the neighbouring groups; closing a floating window closes all panels in it.
  - | Panel (中文) | Windows | macOS |
    |---|---|---|
    | Project (项目) | Ctrl+0 | Cmd+0 |
    | Render Queue (渲染队列) | Ctrl+Alt+0 | Cmd+Opt+0 |
    | Tools (工具) | Ctrl+1 | Cmd+1 |
    | Info (信息) | Ctrl+2 | Cmd+2 |
    | Preview (预览) | Ctrl+3 | Cmd+3 |
    | Audio (音频) | Ctrl+4 | Cmd+4 |
    | Effects & Presets (效果和预设) | Ctrl+5 | Cmd+5 |
    | Character (字符) | Ctrl+6 | Cmd+6 |
    | Paragraph (段落) | Ctrl+7 | Cmd+7 |
    | Paint (绘画) | Ctrl+8 | Cmd+8 |
    | Brushes (画笔) | Ctrl+9 | Cmd+9 |
    | Effect Controls (效果控件) for selected layer | F3 or Ctrl+Shift+T | F3 or Cmd+Shift+T (UNVERIFIED: Adobe table prints Cmd+Opt+T for macOS) |
    | Project Flowchart (流程图) | Ctrl+F11 | Cmd+F11 |
  - Panel-wide commands: Close active viewer or panel (closes its content first) Ctrl+W / Cmd+W; close the active panel or all viewers of the active viewer's type Ctrl+Shift+W / Cmd+Shift+W; `\` toggles focus between the Composition panel and Timeline panel of the current comp; Alt+Shift+, / . (Opt+Shift on macOS) cycles the active panel in the active frame; Ctrl+\ (Cmd+\) resizes the application or floating window to fit the screen (press again: contents fill the screen); Ctrl+Alt+\ (Cmd+Opt+\) also moves it to the main monitor.
  - Tab titles of content panels follow AE format: `Composition <name>` / `合成 <name>`, `Effect Controls <layer>` / `效果控件 <layer>`, `Timeline <comp>`, `Layer <layer>`, `Footage <item>`.
  - Panel framework services used by all panels: tool tips (Preferences > Appearance > Show Tool Tips), keyboard focus (the active panel draws a highlighted border; panel shortcuts override application shortcuts while their panel has focus), mouse wheel handling (AE-UI-012).
- **Priority:** P0
- **Owner:** bae-ui (+ apps/bae)
- **Notes:** Glossary (bae-11-ui-shell/docs/glossary.md, sections 二/三) is the source for panel and panel-menu terms. Panel menu entries for docking are AE-UI-002/003/005. Implementation hint: egui_dock maps directly to this tree (Tree/Node/Leaf with tabs). Undo history (Edit > History) semantics: see project-comp-layers.md, Undo.

### AE-UI-002 · Dock, group, undock and resize panels (停靠、分组和浮动面板)

- **What it does:** Panels and whole groups are rearranged by dragging onto highlighted drop zones; panels can float in their own windows; dividers resize neighbouring groups.
- **Exact semantics:**
  - Drag handles: dragging a **panel's tab gripper** moves that one panel; dragging the **group gripper** (the group's tab-well background/upper-right area) moves the whole group.
  - Drop zones while dragging: **docking zones** along the four edges of a group, frame or the application window (drop = split, inserting the panel next to that group, all affected groups resize); **grouping zone** in the middle of a group and on its tab row (drop = add as a new tab to that group). The zone under the pointer is highlighted (edge strip for dock, whole-group overlay for group).
  - Undock: panel menu **Undock Panel (浮动面板)** / **Undock Panel Group (浮动面板组)**; or Ctrl-drag (Win) / Cmd-drag (Mac) the panel or group; or drag it outside the application window → a new floating window containing that panel/group. Floating windows accept docking like the main window and can go to a second monitor.
  - Other panel-menu entries: Close Panel (关闭面板), Close Other Panels in Group (关闭组中的其他面板), Panel Group Settings (面板组设置) ▸ {Close Panel Group (关闭面板组), Maximize Panel Group (最大化面板组), Stacked Panel Group (堆叠的面板组), Solo Panels in Stack (堆栈中的单独面板), Small Tabs (小选项卡)}.
  - Resize: dragging the divider between two groups resizes every group sharing that divider (horizontal or vertical double-arrow cursor); dragging an intersection of three or more groups resizes in both axes (four-way cursor). Each panel has a minimum size; resizing stops when a neighbour reaches its minimum.
  - Layout changes are not undoable project edits; they belong to the current workspace (AE-UI-006).
- **Priority:** P0
- **Owner:** bae-ui
- **Notes:** Implementation hint: egui_dock supports tab drag with split/tab drop targets and window undocking; Ctrl/Cmd-drag-to-float needs a custom modifier check. Multi-monitor floating windows need one wgpu surface per OS window.

### AE-UI-003 · Maximize or restore panel group (最大化面板组)

- **What it does:** Temporarily enlarges one panel group to fill the whole application (or floating) window and restores the previous layout on the second invocation.
- **Exact semantics:**
  - Triggers: ` (accent grave, the unshifted key under ~ on US keyboards) with the pointer over a panel group → toggles maximize of **the group under the pointer** (not the focused one); double-click on the active panel's tab or on the empty part of the tab row; panel menu > Panel Group Settings > Maximize Panel Group / Restore Panel Group Size (恢复面板组大小).
  - While maximized, other groups are hidden but keep their state; switching workspace or pressing ` again restores. Only one group is maximized at a time.
  - Keyboard layouts where ` is a dead key: AE accepts the physical key; BAE should bind by physical key code.
- **Priority:** P0 (heavily used, e.g. maximizing the Composition viewer or the Timeline)
- **Owner:** bae-ui
- **Notes:** Shift+` / Ctrl+` variants: UNVERIFIED (AE has "maximize the panel under the pointer without the frame chrome" in some versions). Keyboard-shortcut list: see ui-preview-render-io.md, essential keyboard shortcuts (ui-b).

### AE-UI-004 · Stacked panel groups (堆叠的面板组)

- **What it does:** A panel group can display its panels as a vertical stack of collapsible headers instead of tabs, so several small panels (Info, Audio, Preview, Effects & Presets, Align, Character...) share one column.
- **Exact semantics:**
  - Panel menu > Panel Group Settings > **Stacked Panel Group** toggles stacked vs tabbed state of that group.
  - In stacked state each panel has a header bar; a single click on a header expands/collapses that panel. With **Solo Panels in Stack (堆栈中的单独面板)** on, expanding one panel collapses all others in the stack (accordion); off, several can be open at once and share the height.
  - **Small Tabs (小选项卡)** shrinks the tab/header height.
  - The default Standard/Default workspaces (AE 2017+) put the right-hand utility panels in a stacked group.
- **Priority:** P1
- **Owner:** bae-ui
- **Notes:** Implementation hint: an egui_dock leaf rendered as a CollapsingHeader list rather than a tab bar.

### AE-UI-005 · Workspaces and workspace switcher (工作区)

- **What it does:** A workspace is a named panel layout (frame tree, open panels, sizes, floating windows and monitor placement); predefined workspaces cover common tasks and the user can switch between them with one click from the workspace bar.
- **Exact semantics:**
  - Switchers: Window > Workspace > <name>; the **workspace bar** at the right side of the Tools panel (one button per workspace; the active one highlighted; those that do not fit go into a chevron » overflow menu, which also has Edit Workspaces...); Shift+F10 / Shift+F11 / Shift+F12 for workspaces assigned to those keys (Window > Assign Shortcut to "<name>" Workspace (将快捷键分配给"<name>"工作区（译名待核）) ▸ Shift+F10/F11/F12).
  - The divider between the tool buttons and the workspace bar is draggable to give the bar more or less width.
  - Predefined workspaces (AE 2024–2026, menu order alphabetical in Window > Workspace) (UNVERIFIED: exact set varies by version):
    | Workspace (中文) | Purpose / main panels |
    |---|---|
    | All Panels (所有面板（译名待核）) | every panel open |
    | Animation (动画) | Project, Comp, Timeline, Preview, Effects & Presets, Info, Motion Sketch/Smoother/Wiggler |
    | Default (默认) | Project + Effect Controls left, Composition centre, Timeline bottom, utility stack right (Info, Audio, Preview, Effects & Presets, Align, Libraries, Character, Paragraph, Tracker, Content-Aware Fill, Paint, Brushes) |
    | Effects (效果) | Effect Controls large, Effects & Presets |
    | Essentials (基本) | minimal set: Project, Comp, Timeline, Properties/Effects & Presets |
    | Learn (学习) | Learn, Project, Comp, Timeline |
    | Libraries (库) | Libraries panel prominent |
    | Minimal (最小值（译名待核）) | Comp + Timeline only |
    | Motion Tracking (运动跟踪) | Tracker, Layer viewer, Comp |
    | Paint (绘画) | Layer viewer, Paint, Brushes |
    | Review (审阅) | large viewer, Timeline |
    | Small Screen (小屏幕) | compact layout for laptop screens |
    | Standard (标准) | pre-2017 classic layout |
    | Text (文本) | Character, Paragraph |
    | Color (颜色（译名待核）) | Lumetri Scopes, Effect Controls |
  - Workspaces are stored per user as XML layout files in the preferences folder (`.../Adobe/After Effects/<version>/ModifiedWorkspaces`), the factory version separately, so Reset can return to it. A project stores the name of the workspace it was saved with; on open AE looks for a workspace with that name and switches to it unless Preferences > Project > **Maintain current workspace when opening projects** is on, or no match exists (then the current layout stays).
- **Priority:** P0 (switching + one Default layout); P1 (full predefined set)
- **Owner:** bae-ui (+ bae-ui-motion)
- **Notes:** BAE decision: BAE's own workspace presets (分镜/动效/剪辑/审阅/Agent, see docs/ARCHITECTURE.md §8) sit next to the AE-named ones; the AE "Default" layout is the reference for BAE's 动效 workspace. Lottie: not applicable.

### AE-UI-006 · Save, reset, delete and edit workspaces (保存、重置、删除和编辑工作区)

- **What it does:** Persists the current layout as a named workspace, returns a workspace to its saved layout, removes custom workspaces and reorders the workspace bar.
- **Exact semantics:**
  - AE continuously tracks layout edits as the *modified* state of the active workspace; they persist across sessions without an explicit save.
  - Window > Workspace > **Save Changes to this Workspace (保存对此工作区所做的更改)**: makes the modified layout the saved layout.
  - Window > Workspace > **Reset to Saved Layout (重置为已保存的布局)**: discards modifications, restoring the saved layout (for predefined workspaces never saved over, the factory layout).
  - Window > Workspace > **Save as New Workspace... (另存为新工作区（译名待核）)**: name prompt; duplicates names are rejected/overwrite after confirm (UNVERIFIED).
  - **Edit Workspaces... (编辑工作区...)** dialog: list of workspaces in bar order, drag to reorder; move entries to/from an "Overflow Menu" section and a "Do Not Show" section; Delete button (the active workspace cannot be deleted); OK/Cancel.
  - Locked viewers are not stored in workspaces (they are project-context-bound); unlock viewers before saving a multi-viewer workspace.
- **Priority:** P1
- **Owner:** bae-ui
- **Notes:** Implementation hint: serialize the dock tree (serde) to a per-user config file; keep a factory copy per predefined workspace.

### AE-UI-007 · Viewer panels, viewer menu and viewer lock (查看器)

- **What it does:** Composition, Layer, Footage, Flowchart and Effect Controls panels are *viewers*: one panel can hold several items (switch via the viewer menu) and can be locked so that opening another item creates a new viewer instead of replacing the shown one.
- **Exact semantics:**
  - Viewer menu (click the item name in the tab or the dropdown at the top of the viewer): lists the items opened in this viewer (most recent first, UNVERIFIED order), plus **Close <item>**, **Close All**, **Locked (锁定)** and **New <Viewer> (新建...查看器（译名待核）)**.
  - Lock: Toggle Viewer Lock button (padlock) in the viewer tab row or "Locked" menu item. Locked viewer: opening/selecting a new item opens it in a *new* viewer panel; choosing an item from the locked viewer's own menu still reuses it.
  - Ctrl+Alt+Shift+N / Cmd+Opt+Shift+N: split the frame containing the active viewer and create a viewer with the *opposite* lock state (the current one gets locked) in the new half.
  - Shift+, / Shift+. : cycle backward/forward through the items in the active viewer's menu (e.g. open comps).
  - Option *Open separate viewer per item*: users may keep one viewer per comp instead of switching items; each is dockable like any panel.
  - Viewer synchronisation: opening a comp (double-click in Project panel) opens or activates both its Composition viewer and its Timeline panel; activating a Timeline tab switches an unlocked Composition viewer to that comp.
  - **Edit-This-Look-At-That (ETLAT):** if a Composition viewer is locked, a Timeline for another comp is active and that other comp has no visible viewer, then view and preview commands (zoom, fit, preview start, snapshot take/show, channel display, grids/guides toggles, show on video preview device) act on the comp shown in the locked viewer.
  - Panel menu > **Panel Background Color (面板背景颜色（译名待核）)** for Composition/Layer/Footage viewers (AE 24.x+): 5 grey presets, Select Custom Color..., Custom (last custom). Affects only the pasteboard (fallow area) outside the frame.
- **Priority:** P0 (viewer menu, auto-switch, lock); P1 (ETLAT, background colour P2)
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** Composition-specific viewer features are AE-UI-0xx in "Composition viewer" below; Layer/Footage specifics in "Layer and Footage viewers".

### AE-UI-008 · Panel, viewer and context menus (面板菜单、查看器菜单和上下文菜单)

- **What it does:** Three menu kinds give commands relevant to where the user clicked: the panel menu (☰), the viewer menu (item name), and context menus (right-click / Ctrl-click on Mac).
- **Exact semantics:**
  - Panel menu: top-right ☰ button of the group; contents = docking entries (AE-UI-002) + panel-specific commands (e.g. Project panel: Columns, Thumbnail Transparency, Project Settings; Timeline: Columns, Composition Settings, Enable Auto-keyframe, Use Keyframe Icons / Use Keyframe Indices (UNVERIFIED wording), Show Cache Indicators, Show Grid; Effect Controls: Show Effect, Show Animation Presets, Show Effect Controls for...).
  - Context menu: right-click (Windows), right-click or Control-click (macOS). The clicked item becomes selected first if it was not part of the selection; if it was, the whole selection is kept (operations apply to all selected).
  - Context-menu content is panel- and target-specific; each panel item below lists its own.
- **Priority:** P0
- **Owner:** bae-ui (+ bae-ui-motion)
- **Notes:** Implementation hint: a single menu framework in bae-ui with command IDs; panels contribute entries; the same command IDs feed the main menu bar and the keymap.

### AE-UI-009 · Table columns: show, hide, reorder, resize, sort (列)

- **What it does:** The Project, Timeline and Render Queue panels are column tables whose columns can be shown/hidden, reordered, resized and (Project panel) sorted.
- **Exact semantics:**
  - Show/hide: right-click a column heading, or panel menu > Columns ▸ list of every column with a check mark for visible ones. Some columns are mandatory (Project: Name; Timeline: Source/Layer Name) and cannot be hidden.
  - Reorder: drag a column heading left/right; drop position shown by an insertion marker.
  - Resize: drag the separator at the right edge of a heading; some columns have fixed width (switch icon columns).
  - Sort (Project panel only): click a heading → ascending by that column; click again → descending; a sort indicator appears in the heading. Sorting applies within each folder level; folders sort among items by the same key (UNVERIFIED: AE sorts folders together with items by name rather than folders-first).
  - Column state (visibility, order, width, sort) persists per panel instance in the workspace/preferences, not in the project (UNVERIFIED: Timeline column set is stored per Timeline panel and survives reopen).
  - Search/filter only looks at shown columns (exceptions listed in AE-UI-010).
- **Priority:** P0
- **Owner:** bae-ui (+ bae-ui-motion)
- **Notes:** Implementation hint: egui_extras TableBuilder with a column model owned by bae-ui.

### AE-UI-010 · Search and filter fields (搜索和过滤)

- **What it does:** The Project, Timeline and Effects & Presets panels have a search field that filters their lists live while typing.
- **Exact semantics:**
  - Focus: click the field, or File > Find / Ctrl+F (Cmd+F) to focus the active panel's search field.
  - Matching is case-insensitive substring over the text of **shown columns** only; matching items are shown together with their ancestor folders / layers / property groups / categories (auto-expanded for context); non-matching are hidden.
  - Project and Timeline: spaces are AND operators (`Dark Solid` matches "Dark Red Solid"). Effects & Presets: spaces are literal characters (`Change Color` matches the effect "Change Color" only). All three: comma = OR; AND binds tighter than OR (`a b, c` = `(a AND b) OR c`).
  - Timeline: if any layers are selected, filtering applies only to the selected layers (unselected layers stay visible); otherwise all layers. Matches layer names, property names, comments, expression text (method names) and label names (e.g. typing a label colour name shows layers with that label).
  - Project special keywords (whole word): `missing` (source file missing; works even if File Path column hidden), `unused`, `used`; type names (e.g. `Cineon`) match through the Type column when it is shown.
  - Clearing the field ends the search and collapses folders/property groups that were expanded only by the search.
  - Search history: the magnifier icon opens a menu with two lists: top = 6 most recent searches (most recent first, filtered by what is typed), bottom = saved searches (max 10). Shift-click a recent entry to save it; hover + Delete/Backspace removes an entry from either list.
- **Priority:** P0 (filtering); P2 (history/saved searches)
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** —

### AE-UI-011 · Mouse wheel scroll and zoom in panels (使用鼠标滚轮滚动或缩放)

- **What it does:** The mouse wheel scrolls lists and zooms viewers and the Timeline, acting on the panel under the pointer even if that panel is not active.
- **Exact semantics:**
  | Gesture | Composition / Layer / Footage viewer | Timeline / Layer / Footage time ruler or navigator | Lists (Project, Timeline layers, Render Queue, Flowchart, Effect Controls, Metadata, Effects & Presets) |
  |---|---|---|---|
  | Wheel | zoom in (forward) / out (backward) about the pointer or the panel centre (Preferences > Previews > Magnification Behaviour = Follow Cursor (default UNVERIFIED) / Centered) | — | vertical scroll |
  | Alt/Opt + wheel | zoom in/out about the point under the pointer | zoom time in/out | Timeline layer area: zoom time (UNVERIFIED) |
  | Shift + wheel | horizontal scroll | scroll in time (wheel backward = forward in time) | horizontal scroll |
  - When tracking (Layer viewer with Tracker active), plain wheel zooms about the feature region.
  - Trackpad pinch: zoom (UNVERIFIED: AE ≥ 22 on macOS).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** Zoom levels themselves: AE-UI-03x (Composition viewer magnification).

### AE-UI-012 · Scrubbable values (hot text) and numeric fields (可拖动数值)

- **What it does:** Every numeric value in Timeline, Effect Controls, Character/Paragraph, dialogs etc. is "hot text": drag horizontally to scrub it, click to type a new value.
- **Exact semantics:**
  - Hover → cursor becomes a pointing hand with left/right arrows. Drag left/right: value changes by `step × pixels`, default step 1 unit per pixel (property-specific: Opacity 1 %, Rotation 1°, Scale 1 %, Position 1 px; effect sliders use the slider's UI precision).
  - Modifiers while dragging: **Ctrl (Win) / Cmd (Mac)** = fine, ×0.1 step; **Shift** = coarse, ×10 step. Modifiers can be pressed/released mid-drag.
  - Click (no drag) → inline edit field with the value selected; Enter (main keyboard) or Tab commits (Tab moves to the next field), Esc cancels; clicking elsewhere commits.
  - Typed arithmetic: AE accepts simple arithmetic in value fields, e.g. `+20`, `*2`, `/3`, `-10` relative to the current value, and full expressions like `100*1.5` (UNVERIFIED: supported since CS3 for operators + - * /; parentheses not supported).
  - Values clamp to the property's valid range (e.g. Opacity 0–100); some properties allow out-of-UI-range typing beyond the slider range (effect sliders, see AE-UI-11x Effect Controls).
  - Each completed drag is one undo step; while dragging the comp updates live.
  - Up/Down arrow keys in an edit field: UNVERIFIED (AE does not support arrow increments in hot-text fields).
  - Proportional scrubbing: Alt+drag in Timeline on a selected multi-layer property group scales values proportionally (AE 25.x "Proportional Scrubbing in timeline") (UNVERIFIED: modifier).
- **Priority:** P0
- **Owner:** bae-ui (+ bae-ui-motion)
- **Notes:** Units displayed per property (px, %, °, ×+° for rotation revolutions); Rotation shows `Nx+M.M°`. Property value types: see project-comp-layers.md, Property tree and value types. Implementation hint: an egui DragValue variant with AE modifier mapping and arithmetic parser.

### AE-UI-013 · Home screen (主页)

- **What it does:** The start screen shown at launch (unless disabled) and via the Home button in the Tools panel, offering New Project, Open Project, recent projects and learning links.
- **Exact semantics:**
  - Left column: Home, New Project (新建项目), Open Project (打开项目), (New/Open Team Project — Adobe cloud, not applicable).
  - Centre: list of recent projects (name, last opened, size) with search, and a "What's new"/learning area.
  - Preferences > General > **Show Home screen when no project is open** (UNVERIFIED label; called "Enable Home Screen" in some versions).
  - Home button (Tools panel, left end) toggles between the Home screen and the current project UI.
- **Priority:** P2
- **Owner:** bae-ui (+ apps/bae)
- **Notes:** BAE decision: Team Projects and Adobe account menu are not reproduced.

## Project panel

### AE-UI-014 · Project panel layout and information header (项目面板)

- **What it does:** Lists every item of the project (compositions, footage, solids, folders) as a sortable tree table, with an information area for the selected item at the top and item-creation buttons at the bottom.
- **Exact semantics:**
  - Regions, top to bottom:
    1. **Information area (信息区域)**: thumbnail of the selected item on the left (comp = its poster frame, default first frame; footage = a representative frame; audio-only = waveform-less audio icon), and text lines on the right. First line: item name followed by a ▼ usage drop-down showing `, used N times` (UNVERIFIED: exact wording; zh-CN "使用了 N 次"（译名待核）) that lists every comp using the item; choosing a comp opens it. Further lines: see project-comp-layers.md, Project panel column data (dimensions/PAR, Δ duration and fps, colour-depth descriptor, codec, audio format, colour profile). If the footage has non-square pixels, the PAR is shown next to the thumbnail.
    2. **Search field (搜索字段)** with the magnifier menu of recent/saved searches (AE-UI-010).
    3. **Column header row** (AE-UI-009, AE-UI-015).
    4. **Item list** (tree): one row per item; folders have a twirl triangle. Name cell shows the item-type icon (comp, folder, footage by kind, solid, placeholder) and the name. Missing footage names are drawn in italics (see project-comp-layers.md, Missing footage). Proxy-enabled items show a proxy indicator box left of the name (filled = proxy in use, hollow = proxy set but off).
    5. **Footer button bar** (AE-UI-017) and the Project Flowchart button at the top of the vertical scroll bar (AE-UI-017).
  - Clicking empty space in the item list deselects all; the information area then shows nothing (or the project file name, UNVERIFIED).
  - Alt-click (Win) / Option-click (Mac) a footage item shows its file creator ID in the information area (macOS type/creator; mostly meaningless today).
  - Panel menu (☰) entries specific to this panel: Columns ▸ (AE-UI-015), Thumbnail Transparency Grid (缩略图透明度网格) toggle (draws the checkerboard behind transparent thumbnail pixels; default off = black), Project Settings… (项目设置), plus the generic docking entries.
  - Preference > Appearance > **Disable Thumbnails in Project Panel (在项目面板中禁用缩略图)** (default off) hides the thumbnail and stops rendering thumbnails.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Composition > Set Poster Time (设置海报时间) sets the comp thumbnail frame (data: project-comp-layers.md, Composition Settings / poster time). Implementation hint: render thumbnails asynchronously at thumbnail size from the frame cache; never block the panel on decoding.

### AE-UI-015 · Project panel columns (项目面板列)

- **What it does:** The item list is a multi-column table; columns can be shown, hidden, reordered, resized and used for sorting, and the Comment column can be renamed.
- **Exact semantics:**
  - Columns available in the column-header context menu / panel menu Columns ▸ (values defined in project-comp-layers.md, Project panel column data):
    | Column (中文) | Default visible | Notes |
    |---|---|---|
    | Name (名称) | yes (cannot be hidden) | tree column with twirls and type icon |
    | Label (标签) | yes | colour swatch; click opens the label menu (AE-UI-020); header shows a tag glyph |
    | Type (类型) | yes | |
    | Size (大小) | yes | |
    | Media Duration (媒体持续时间（译名待核）) | yes | shown with Δ prefix |
    | File Path (文件路径) | yes | |
    | Date (日期（译名待核）) | no | UNVERIFIED default |
    | Comment (注释) | no | editable in place (click the cell); header can be renamed via context menu Rename This (重命名此项（译名待核）) |
    | Frame Rate (帧速率) | no | UNVERIFIED default |
    | In Point / Out Point (入点 / 出点) | no | UNVERIFIED |
    | Tape Name (磁带名称（译名待核）) and further XMP metadata columns (Scene, Take, Description, ...) | no | UNVERIFIED list; from Columns ▸ Project Metadata |
  - UNVERIFIED: AE's initial default column set (believed Name, Label, Type, Size, Media Duration, File Path).
  - Sort: clicking a column heading sorts by that column ascending (indicator triangle); clicking again sorts descending. Sorting applies within each folder level; folders sort with items by the same key (UNVERIFIED: folders are not grouped first). Sort is a per-panel view setting, saved with the project (UNVERIFIED) — not an undoable edit.
  - Name sort is natural-number aware (UNVERIFIED: "Comp 2" before "Comp 10").
  - Hidden columns are not searched by the search field (exception: the keyword `missing` searches the file path even when hidden), see AE-UI-010.
- **Priority:** P0 (Name/Label/Type/Size/Duration/File Path/Comment); P2 for XMP metadata columns
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Generic column behaviour is AE-UI-009. BAE decision: persist the column set and sort key per workspace (view state), not in the project file, unless AE fidelity demands otherwise.

### AE-UI-016 · Project panel tree, selection, rename and folder twirl (项目面板中的选择、重命名和文件夹展开)

- **What it does:** Standard tree-list interactions for selecting, renaming, expanding and moving items.
- **Exact semantics:**
  - Selection: click selects one item; Ctrl-click (Win) / Cmd-click (Mac) toggles; Shift-click extends a contiguous range; Up/Down arrows move the selection; Shift+Up/Down extends selection to the previous/next item; Ctrl/Cmd+A selects all items (visible items; contents of collapsed folders are selected via their folder, UNVERIFIED). Selection order is remembered and used when creating layers from several items (layers stack in selection order).
  - Rename: select one item and press Enter (Win) / Return (Mac), or context menu > Rename (重命名); the name cell becomes an edit field; Enter commits, Esc cancels. Renaming is undoable. Names need not be unique.
  - Twirl: click a folder's triangle to expand/collapse; Alt-click (Win) / Option-click (Mac) the triangle expands/collapses the folder and all nested folders (UNVERIFIED). Expanded state is view state.
  - Move: drag one or more items onto a folder row → moved into that folder (drop highlight on the folder); drag onto the information area → moved to the root. A folder cannot be dropped into itself/descendants (no-drop cursor). Commands: project-comp-layers.md, Folders.
  - Duplicate (Ctrl/Cmd+D), Copy/Paste, Delete: project-comp-layers.md, Duplicate and copy/paste items / Delete items and folders. Delete/Backspace deletes with a warning if items are in use; Ctrl+Backspace (Win) / Cmd+Delete (Mac) deletes without warning.
  - Numeric-keypad Enter with Project panel focus opens the selected footage items in Footage panels.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** —

### AE-UI-017 · Project panel buttons (项目面板按钮)

- **What it does:** Footer buttons give one-click access to Interpret Footage, New Folder, New Composition, project bit depth and Delete; a button above the scroll bar opens the Project Flowchart.
- **Exact semantics:**
  - Footer buttons, left to right (AE 2022+, UNVERIFIED exact order between New Composition and bit depth):
    | Button (中文) | Click | Modifier / drag behaviour |
    |---|---|---|
    | Interpret Footage (解释素材) | opens Interpret Footage for the selected footage item (same as Ctrl+Alt+G / Cmd+Opt+G) | drag a footage item onto it = open Interpret Footage for that item; Alt/Option-drag one item onto it = open Interpret Footage on the dropped item (UNVERIFIED distinction; AE help lists both gestures) |
    | Create a new Folder (新建文件夹) | creates a folder in the selected folder (or root), enters rename mode | dragging items onto it creates a folder containing them (UNVERIFIED) |
    | Create a new Composition (新建合成) | opens Composition Settings (new comp) | drag footage item(s) onto it = New Comp From Selection (基于所选项新建合成): one item → comp matching the item's size/PAR/fps/duration; several → the New Composition from Selection dialog (project-comp-layers.md, Composition Settings / new comp from footage) |
    | Project bit depth (项目位深度) "8 bpc"/"16 bpc"/"32 bpc" | opens Project Settings at the colour settings | Alt-click (Win) / Option-click (Mac) cycles 8 → 16 → 32 → 8 bpc without a dialog (undoable project setting change) |
    | Delete selected project items (删除所选项目项（译名待核）) | deletes selected items (with in-use warning) | drag items onto it = delete them |
  - Project Flowchart button (项目流程图) at the top of the vertical scroll bar: opens the Flowchart panel for the project (same as Ctrl+F11 / Cmd+F11), see AE-UI-022.
  - Older versions also had a "Project render settings / renderer" button in the footer (UNVERIFIED: removed in AE 2015).
- **Priority:** P0 (P2 for the flowchart button)
- **Owner:** bae-ui-motion (+ bae-model, bae-motion)
- **Notes:** Bit-depth semantics: compositing-3d-color.md, Colour management (project bit depth). Interpret Footage dialog: project-comp-layers.md, Interpret Footage. BAE decision: in BAE the bit-depth button may show "32 bpc float" as the default since BAE's compositor is float-native; still display and persist AE's three values.

### AE-UI-018 · Project panel drag and drop (项目面板拖放)

- **What it does:** Dragging items out of the Project panel creates layers, replaces layer sources, creates comps, or opens interpretation; dragging files in imports them.
- **Exact semantics:**
  - Drop targets and results:
    | Drag from Project panel to | Result |
    |---|---|
    | Composition viewer | new layer(s) in the viewer's comp, placed at the top of the stack (above the selected layer if one is selected, UNVERIFIED), positioned so the layer centre is at the drop point; Shift while dragging snaps the layer to the comp centre or edges; In point = comp start or CTI per Preferences > General/Composition > Create Layers at Composition Start Time (default on) |
    | Timeline layer outline | new layer(s) inserted at the highlighted stack position (a horizontal highlight bar previews it); In point per the same preference |
    | Timeline time graph | new layer(s); the vertical time marker during the drag shows the In point = drop time; Shift snaps the In point to the CTI |
    | a composition item in the Project panel (name or icon) | add as layers to that comp, centred, immediately above the selected layer of that comp or at the top (same as Ctrl+/ / Cmd+/ for the most recently activated comp) |
    | a selected layer in the Timeline with Alt (Win) / Option (Mac) held | Replace Layer Source with the dragged item (project-comp-layers.md, Replace source); Ctrl+Alt+/ (Cmd+Opt+/) does the same for selected layers |
    | New Composition / Interpret Footage / New Folder / Delete buttons | AE-UI-017 |
    | a folder row / information area | move into folder / to root (AE-UI-016) |
    | the OS desktop / Finder / Explorer | the OS copies the source file (P2; BAE may skip) |
  - Several items dropped at once create layers in the order the items were selected; a folder dropped into a comp adds all footage in it (UNVERIFIED: recursive).
  - Files dragged from the OS into the Project panel import them (project-comp-layers.md, Drag-and-drop import); dropped on a folder row imports into that folder.
  - Alt/Option-drag in the Composition panel of a layer (AE 25 beta+) duplicates it; preference-controlled vs camera navigation in 3D (see ui-preview-render-io.md tools part).
  - Invalid drop (e.g. a comp into itself or into a comp that it contains) shows the no-drop cursor; AE refuses with an alert about circular references (UNVERIFIED wording).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Layer creation semantics (duration of still layers, sizes, nested comp layers): project-comp-layers.md, Layer types and layer creation. Implementation hint: a single drag-payload type (list of item IDs + selection order) shared by all drop targets.

### AE-UI-019 · Project panel double-click and open behaviours (项目面板双击打开)

- **What it does:** Double-clicking an item opens it in the appropriate viewer.
- **Exact semantics:**
  | Item / gesture | Result |
  |---|---|
  | Double-click composition | opens the comp in a Composition viewer and its Timeline panel (brings existing tabs to front if already open) |
  | Double-click footage item (video, still, sequence, solid) | opens it in a Footage panel (shows interpreted footage; AE-UI-0xx Footage viewer) |
  | Alt+double-click (Win) / Option+double-click (Mac) footage | opens the source file in the OS-associated player application (BAE: open with OS default) |
  | Double-click folder | expands/collapses it (UNVERIFIED) |
  | Double-click placeholder | opens Replace Footage file dialog (UNVERIFIED) |
  | Double-click missing footage | opens Replace Footage file dialog to relink (UNVERIFIED) |
  | Double-click audio-only footage | opens it in a Footage panel with an audio waveform-less view and audio playback (UNVERIFIED) |
  | Numeric-keypad Enter | opens selected footage in Footage panels |
  - Ctrl+E (Win) / Cmd+E (Mac) = Edit Original (project-comp-layers.md, Edit Original).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-020 · Label colour menu and label groups (标签菜单)

- **What it does:** Clicking a label swatch (Project panel or Timeline) opens a menu of the 16 label colours plus None and Select Label Group; choosing a colour relabels all selected items/layers.
- **Exact semantics:**
  - Menu content: the label names in label order 1–16 (names/colours from Preferences > Labels; defaults in project-comp-layers.md, Labels), each with its swatch; separator; **None (无)** (label 0); separator; **Select Label Group (选择标签组)** = select all items (Project panel) / layers (Timeline) that have the same label as the clicked one.
  - Clicking the swatch of an unselected row applies to that row only (UNVERIFIED: or to the selection if the row is part of it). Edit > Label ▸ applies to the selection with the same menu.
  - Changing a label is one undoable command per click (project-comp-layers.md, Labels).
  - Label column can be widened to show label names in place of swatches.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** The same menu appears in the Timeline label column and in the layer bar context menu (AE-UI-0xx Timeline). Label colours drive the layer bar colour and the viewer handle colour.

### AE-UI-021 · Project panel context menu (项目面板上下文菜单)

- **What it does:** Right-click (Win) / Control-click (Mac) on an item or empty area shows commands applicable to the selection.
- **Exact semantics:**
  - On a footage item (order UNVERIFIED): Open in Viewer / Open in Footage Panel (在查看器中打开), Rename (重命名), Reveal in Explorer / Reveal in Finder (在资源管理器中显示 / 在 Finder 中显示), Reveal in Composition ▸ (在合成中显示), Reveal in Bridge (omitted in BAE), Interpret Footage ▸ (解释素材: Main…, Proxy…, Remember Interpretation, Apply Interpretation), Replace Footage ▸ (替换素材: File…, With Layered Comp, Placeholder…, Solid…), Reload Footage (重新加载素材), Edit Original… (编辑原稿), Set Proxy ▸ (设置代理), Create Proxy ▸ (创建代理), New Comp from Selection (基于所选项新建合成), Dependencies ▸ (Collect Files, Reduce Project, Remove Unused Footage, Consolidate All Footage, Find Missing Effects/Fonts/Footage), Solids Folder (纯色文件夹) on folders.
  - On a composition: Composition Settings… (合成设置), Open in Viewer, Rename, Add to Render Queue (添加到渲染队列), Add to Adobe Media Encoder Queue (omitted in BAE), Reveal in Composition, Composition Flowchart (合成流程图), Pre-render… (预渲染), Set Proxy ▸, Dependencies ▸.
  - On empty space: New ▸ (新建: Composition, Folder, Adobe Photoshop File, MAXON CINEMA 4D File — the last two omitted in BAE), Import ▸ (导入).
  - Each entry invokes the command defined in project-comp-layers.md (items, interpretation, replace/reload, proxies, dependencies) or ui-preview-render-io.md Render Queue.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Exact entry list varies by version and item type; BAE should derive enabled state from the same command availability predicates as the main menus.

### AE-UI-022 · Flowchart panel (流程图面板)

- **What it does:** A viewer panel showing the project or a comp as a node graph (comps, footage, layers, effects) with arrows for data flow.
- **Exact semantics:**
  - Open: Project Flowchart button / Ctrl+F11 (Cmd+F11) for the whole project; Composition > Composition Flowchart (合成流程图) or the Comp Flowchart button in the Timeline/Composition panel for one comp.
  - Footer controls: flow direction (Top to Bottom, Bottom to Top, Left to Right, Right to Left), Show Layers (显示图层), Show Footage (显示素材（译名待核）), Show Solids, Show Effects (显示效果（译名待核）), toggle straight vs angled connector lines, Clean Up / reset layout (UNVERIFIED names).
  - Tiles: comps (with a + button to expand to show their layers), footage, layers, effects; line colour: dark lines for active, grey for layers whose Video/Audio switch is off. Click selects the item/layer in Project/Timeline; double-click opens it; Delete removes the selected item/layer; the graph cannot rewire relationships.
  - Alt-drag (Option-drag) a tile moves it with its children (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Data/semantics: project-comp-layers.md, Project Flowchart. The Mini-Flowchart (comp navigator) is AE-UI-0xx in the Composition viewer section.

## Composition viewer

### AE-UI-023 · Composition panel layout and viewer toolbar (合成面板)

- **What it does:** The Composition panel is the main viewer of the active comp: an image area (pasteboard + comp frame), an optional Composition Navigator bar on top, and a bottom toolbar of view controls; it is also where layers are directly manipulated with tools.
- **Exact semantics:**
  - Regions: tab (`Composition <name>` with the viewer menu, AE-UI-007) → Composition Navigator bar (AE-UI-035; shown by default for nested comps) → image area (comp frame centred on a neutral pasteboard; content outside the frame is drawn only for selected layers' bounding boxes/paths, not pixels, unless Extended Viewer is on) → bottom toolbar.
  - Bottom toolbar, left to right (AE 2023+, UNVERIFIED exact order; older versions differ):
    | Control (中文) | Item |
    |---|---|
    | Always Preview This View (始终预览此视图) | AE-UI-036 |
    | Primary Viewer (主查看器（译名待核）) | AE-UI-036 |
    | Magnification ratio (放大率) | AE-UI-024 |
    | Choose grid and guide options (选择网格和参考线选项) | AE-UI-028 |
    | Toggle Mask and Shape Path Visibility (切换蒙版和形状路径可见性) | AE-UI-029 |
    | Current time (当前时间) | displays CTI in the project time display style; click = Go To Time dialog (AE-UI-0xx Timeline time display) |
    | Take Snapshot (拍摄快照) / Show Snapshot (显示快照) | AE-UI-031 |
    | Show Channel and Color Management Settings (显示通道及色彩管理设置) | AE-UI-032 |
    | Resolution / Down Sample Factor (分辨率/向下采样系数) | AE-UI-025 |
    | Region of Interest (目标区域) | AE-UI-027 |
    | Toggle Transparency Grid (切换透明网格) | AE-UI-026 |
    | 3D renderer button (渲染器) | shows Classic 3D / Advanced 3D / CINEMA 4D; click opens Composition Settings > 3D Renderer tab |
    | Draft 3D (草图 3D), 3D Ground Plane, Extended Viewer | AE-UI-034 |
    | 3D View (3D 视图) popup | AE-UI-030 |
    | Select View Layout (选择视图布局) | AE-UI-030 |
    | Toggle Pixel Aspect Ratio Correction (切换像素长宽比校正) | AE-UI-033 |
    | Fast Previews (快速预览) | AE-UI-025 |
    | Timeline (时间轴) button | brings the comp's Timeline panel to front |
    | Comp Flowchart (合成流程图) | AE-UI-022 |
    | Reset Exposure (重置曝光度) / Adjust Exposure (调整曝光度) | AE-UI-032 |
  - When the panel is too narrow, controls on the right are hidden (no wrapping); BAE may add an overflow menu.
  - Composition panel menu (☰) entries: View Options… (视图选项, AE-UI-037), Composition Settings… (合成设置), Enable Frame Blending, Enable Motion Blur, Draft 3D (UNVERIFIED), Show Composition Navigator (显示合成导航器), Flow Right to Left / Flow Left to Right, Composition Mini-Flowchart, Show 3D View Labels (显示 3D 视图标签), Show Snapshot / Purge snapshot (UNVERIFIED), plus docking entries.
  - Image area scroll bars appear when the zoomed image exceeds the view; empty space around the comp is the pasteboard.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Rendering of every view mode: compositing-3d-color.md, Preview rendering. Never copy Adobe's button glyphs; BAE draws its own icons and labels tool tips with the zh/en names above.

### AE-UI-024 · Magnification (放大率)

- **What it does:** Controls the on-screen zoom of the Composition, Layer and Footage viewers without changing the comp's resolution.
- **Exact semantics:**
  - Magnification ratio popup: Fit (适合), Fit up to 100% (适合 100%（译名待核）; macOS Retina shows "Fit up to 200%" because 200% = 1 device pixel per image pixel), then fixed levels 1.5%, 3.1%, 6.25%, 12.5%, 25%, 33.3%, 50%, 100%, 200%, 400%, 800%, 1600%, 3200%, 6400% (UNVERIFIED: exact smallest levels and labels, e.g. "6.2%"). Ctrl/Cmd while choosing applies to all views of the panel.
  - Since AE 25.x (2025) **Smooth Zoom** (Preferences > Previews > Magnification: Type = Smooth or Fixed (UNVERIFIED labels)): the magnification readout is hot text accepting any decimal value; wheel zoom is continuous; max zoom is buffer-limited between 3200% and 6400%.
  - Keyboard: `.` zoom in, `,` zoom out (to next fixed level, around view centre); `/` (main keyboard) 100%; Shift+/ Fit; Alt+Shift+/ (Opt+Shift+/) Fit up to 100%. Double-click Hand tool = Fit; double-click Zoom tool = 100% (Win) / 200% (Mac Retina).
  - Mouse wheel zoom (with Preferences default): zooms around the cursor; Alt/Opt+wheel zooms around the view centre (help text is contradictory, UNVERIFIED which is default; see AE-UI-011). While scroll-zooming, Shift speeds up and Ctrl/Cmd slows down (Smooth Zoom).
  - Pan: Hand tool drag, Space-drag, H-drag, middle-mouse drag; Shift while panning = faster.
  - Magnification is per view, not saved in the project (UNVERIFIED: saved with workspace/viewer state).
  - With Auto resolution, magnification drives the effective resolution (compositing-3d-color.md, Auto resolution).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** AE renders vector content before scaling the preview, so zoomed-in vectors look pixelated unless rendered at that magnification; BAE decision: when performance allows, re-rasterise vectors at view magnification for sharp zoom (view-only; render output unaffected). Zoom Quality preference: compositing-3d-color.md, Viewer Quality preferences.

### AE-UI-025 · Resolution and Fast Previews menus (分辨率和快速预览菜单)

- **What it does:** Viewer popups that choose the comp's preview resolution (Full, Half, Third, Quarter, Custom, Auto) and the interactive fast-preview mode.
- **Exact semantics:**
  - Resolution / Down Sample Factor popup (分辨率/向下采样系数): Full (完整), Half (二分之一), Third (三分之一), Quarter (四分之一), Custom… (自定义, dialog with horizontal/vertical factors), Auto (自动). The value is a comp setting shared by all views of the comp (stored in Composition Settings). Shortcuts: Ctrl+J (Cmd+J) Full, Ctrl+Shift+J (Cmd+Shift+J) Half, Ctrl+Alt+Shift+J (Cmd+Opt+Shift+J) Quarter, Ctrl+Alt+J (Cmd+Opt+J) Custom (UNVERIFIED: AE 2023 shortcut table); Alt+/ cycles? (UNVERIFIED, see ui-b shortcuts).
  - When Auto is active, the popup label shows "Auto" and the computed factor in parentheses, e.g. "(Quarter)".
  - Fast Previews popup (快速预览): Off (Final Quality) (关闭（最终品质）), Adaptive Resolution (自适应分辨率), Draft (草图), Fast Draft (快速草图), Wireframe (线框), separator, Fast Previews Preferences… and GPU Information… (GPU 信息（译名待核）). Shortcuts Ctrl+Alt+1…5 / Cmd+Opt+1…5. The icon changes to show that a non-Off mode is active. Since AE 18.0 Draft/Fast Draft are replaced by Draft 3D in the viewer (compositing-3d-color.md, Fast Previews modes).
  - Both are view-level settings for the comp (Fast Previews per view, UNVERIFIED).
- **Priority:** P0 (Resolution); P1 (Fast Previews)
- **Owner:** bae-ui-motion (+ bae-render, bae-model)
- **Notes:** Semantics: compositing-3d-color.md, Resolution / Down Sample Factor, Auto resolution, Fast Previews modes.

### AE-UI-026 · Toggle Transparency Grid (切换透明网格)

- **What it does:** Toggles whether transparent areas of the comp are shown over a checkerboard instead of the comp background colour.
- **Exact semantics:** Button toggles per view; off by default (comp background colour shown). Also in the Preview panel and Composition menu? (UNVERIFIED). Grid size and colours are preferences (Preferences > Previews/Appearance, UNVERIFIED). Does not affect output.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Rendering: compositing-3d-color.md, Transparency grid and composition background color.

### AE-UI-027 · Region of Interest button (目标区域)

- **What it does:** Restricts preview rendering to a user-drawn rectangle of the comp.
- **Exact semantics:**
  - Click the button (when no ROI exists) → cursor becomes a marquee; drag in the viewer draws the ROI rectangle; the button stays on. Click the button again toggles the ROI off/on (the rectangle is remembered). Alt/Option-click the button clears the existing ROI and starts drawing a new one (UNVERIFIED).
  - The ROI has 8 handles; drag handles to resize, drag inside (with the Selection tool) to move.
  - Composition > Crop Comp to Region of Interest (将合成裁剪到目标区域) changes comp size to the ROI (command, see compositing-3d-color.md, Region of Interest).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Semantics and the crop command: compositing-3d-color.md, Region of Interest.

### AE-UI-028 · Choose grid and guide options menu (选择网格和参考线选项)

- **What it does:** A viewer popup that toggles each visual aid in the active view.
- **Exact semantics:**
  - Menu entries (checkable, per view), in order: Title/Action Safe (标题/动作安全), Proportional Grid (对称网格), Grid (网格), Guides (参考线), Rulers (标尺), 3D Reference Axes (3D 参考轴（译名待核）).
  - Shortcuts: ' (apostrophe) safe zones; Ctrl+' (Cmd+') grid; Alt+' (Opt+') proportional grid; Ctrl+R (Cmd+R) rulers; Ctrl+; (Cmd+;) guides; snapping to grid / guides and lock guides (see ui-preview-render-io.md, viewer interaction (ui-b)).
  - Alt/Option-clicking the button toggles Title/Action Safe without opening the menu (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Appearance, spacing, custom safe margins, rulers/guides interaction and snapping are specified in the ui-b part of this file (viewer interaction: snapping, grids, guides, rulers, safe margins); preferences in the Preferences items (ui-c).

### AE-UI-029 · Toggle Mask and Shape Path Visibility (切换蒙版和形状路径可见性)

- **What it does:** Shows or hides mask paths and shape paths of selected layers in the view.
- **Exact semantics:** Per-view toggle, default on. When off, mask/shape paths are not drawn and cannot be clicked in the viewer (they remain editable in Timeline). Independent from View > Show Layer Controls (Ctrl+Shift+H / Cmd+Shift+H), which hides all layer controls (AE-UI-037).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-030 · 3D View popup and view layouts (3D 视图与视图布局)

- **What it does:** Each view of a Composition panel can look through the active camera or a working 3D view; the panel can be split into 1, 2 or 4 views.
- **Exact semantics:**
  - 3D View popup (3D 视图): entries and semantics in compositing-3d-color.md, 3D views. View > Switch 3D View ▸ (切换 3D 视图), View > Switch to Last 3D View (切换到上一个 3D 视图; also Esc). F10/F11/F12 switch to the three assignable views (defaults Front, Custom View 1, Active Camera); Shift+F10/F11/F12 assigns the current view to that key (View > Assign Shortcut to ▸). View > Reset Default Camera / Reset 3D View (重置 3D 视图（译名待核）) and View > Look at Selected Layers (查看选定图层) Ctrl+Alt+Shift+\ / Cmd+Opt+Shift+\ , Look at All Layers (查看所有图层) (UNVERIFIED shortcuts).
  - The popup is disabled/hidden for comps without 3D layers? (UNVERIFIED: it is enabled only when the comp contains a 3D layer.)
  - Select View Layout popup (选择视图布局): 1 View (1 个视图), 2 Views (Horizontal) (2 个视图 - 水平), 2 Views (Vertical)? (UNVERIFIED), 4 Views (4 个视图), 4 Views - Left, 4 Views - Right, 4 Views - Top, 4 Views - Bottom (UNVERIFIED exact list/order), then Share View Options (共享视图选项). Mouse wheel over the popup cycles layouts.
  - Multi-view: each view has its own 3D View, magnification, grid/guide toggles, channel, snapshot display, layer controls; the active view has a highlighted corner/frame; click (or middle-click to avoid changing layer selection) activates a view. Share View Options: grid/guides/etc. settings apply to all views; holding Ctrl/Cmd while changing a setting temporarily reverses sharing.
  - Show 3D View Labels (panel menu) draws the view name in each view's corner.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** BAE decision: views are UI state (not in the CRDT document); multi-view rendering reuses the evaluated comp and renders one camera per view.

### AE-UI-031 · Snapshots (快照)

- **What it does:** Stores up to four still images of a viewer for side-by-side comparison; a snapshot is displayed temporarily while a key or button is held.
- **Exact semantics:**
  - Take Snapshot (拍摄快照) button or Shift+F5 stores the current view image in slot 1 (F5); Shift+F6/F7/F8 store slots 2–4. A camera-shutter sound plays (UNVERIFIED: can be disabled).
  - Show Snapshot (显示快照) button: press and hold displays the slot-1 snapshot (most recent from the button/Shift+F5); F5–F8 press-and-hold shows that slot. Releasing restores the live view.
  - Snapshots can be shown in any viewer type (Composition/Layer/Footage); if size/aspect differ the snapshot is scaled to fit the current view.
  - Purge: Ctrl+Shift+F5…F8 (Cmd+Shift+F5…F8) purges one slot; Edit > Purge > Snapshot (清理 > 快照) purges all.
  - Snapshots are memory-only; never saved; never rendered.
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** Implementation hint: store the displayed GPU texture (at view resolution) per slot; show by swapping the texture shown by the viewer.

### AE-UI-032 · Show Channel, colour management toggle and exposure controls (显示通道与曝光度)

- **What it does:** Viewer controls to inspect single channels, toggle display colour management, and temporarily change preview exposure in stops.
- **Exact semantics:**
  - Show Channel and Color Management Settings popup (显示通道及色彩管理设置): RGB, Red (红色), Green (绿色), Blue (蓝色), Alpha (Alpha), RGB Straight (RGB 直接（译名待核）), separator, Colorize (着色) toggle, separator, Use Display Color Management (使用显示色彩管理) toggle (Shift+/ on numeric keypad), Simulate Output ▸ (模拟输出) (UNVERIFIED order). A coloured outline around the view indicates the shown channel (red/green/blue/white).
  - Shortcuts: Alt+1/2/3/4 (Opt+1/2/3/4) toggle red/green/blue/alpha as grayscale; Alt+Shift+1/2/3 colourised channels; repeating returns to RGB.
  - Reset Exposure (重置曝光度) button + Adjust Exposure (调整曝光度) hot-text control (f-stops, default 0.0, range UNVERIFIED ±40): drag/enter value; when ≠ 0 the reset button is highlighted (blue); clicking Reset sets 0, clicking again restores the last non-zero value. Per view, never affects output.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-render, bae-color)
- **Notes:** Semantics: compositing-3d-color.md, Show Channel views and Viewer exposure; colour management: compositing-3d-color.md, Colour management (display/simulate output).

### AE-UI-033 · Toggle Pixel Aspect Ratio Correction (切换像素长宽比校正)

- **What it does:** Shows non-square-pixel comps/footage stretched to square-pixel display aspect.
- **Exact semantics:** Per-view toggle button, default off; only meaningful when the comp/footage PAR ≠ 1. Preview-only.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Semantics and the zoom-quality interaction: compositing-3d-color.md, Viewer pixel aspect ratio correction.

### AE-UI-034 · Draft 3D, 3D Ground Plane and Extended Viewer buttons (草图 3D、3D 地平面和扩展查看器)

- **What it does:** Viewer toolbar buttons for the real-time 3D draft mode and its viewing aids.
- **Exact semantics:**
  - Draft 3D (草图 3D) toggle: comp-level, affects all views of the comp (compositing-3d-color.md, Draft 3D mode). When on, two extra buttons appear: 3D Ground Plane (3D 地平面（译名待核）) toggle and Extended Viewer (扩展查看器（译名待核）) toggle; press-and-hold Extended Viewer shows an opacity slider for content outside the comp frame.
  - The renderer button next to it opens Composition Settings > 3D Renderer.
- **Priority:** P1 (Draft 3D); P2 (ground plane, extended viewer)
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Semantics: compositing-3d-color.md, 3D Ground Plane and Extended Viewer.

### AE-UI-035 · Composition Navigator bar and Composition Mini-Flowchart (合成导航器和合成微型流程图)

- **What it does:** Navigation aids for nested comp networks: a breadcrumb bar on top of the Composition panel and a transient popup graph.
- **Exact semantics:**
  - Composition Navigator bar (合成导航器): shows the flow path of comps the active comp belongs to (most recently navigated path). Default order "Flow Right To Left" (downstream comps on the left, upstream on the right); panel menu Flow Left To Right reverses it (global preference). Downstream comp names are dimmed. Click a name to activate that comp in this viewer. Ellipsis buttons at either end show the full path when it is too long; mouse wheel over a comp button scrolls the path. Panel menu Show Composition Navigator toggles the bar.
  - Composition Mini-Flowchart (合成微型流程图): opened by Tab (Composition, Layer or Timeline panel active), the arrow right of a comp name in the navigator, Composition > Composition Mini-Flowchart, or the Timeline's mini-flowchart button. Shows the selected comp with its immediate upstream comps (sources) and downstream comps (comps using it); tile colours = label colours; a comp nested several times in one comp is one entry with "(N)". Upstream entries are sorted alphabetically or by layer order (press S to toggle; layer order uses the topmost instance); downstream always alphabetical. A dash instead of an arrow means no further flow in that direction. Arrow keys / clicking arrows navigate; Space or Enter/Return activates the selected comp; Esc, Shift or clicking outside closes.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Requires the item→usage reverse index (project-comp-layers.md, Reveal usage).

### AE-UI-036 · Always Preview This View and Primary Viewer (始终预览此视图和主查看器)

- **What it does:** Designates which viewer is used for previews and which drives audio / external video output.
- **Exact semantics:**
  - Always Preview This View (始终预览此视图) toggle (Composition, Layer, Footage panels): when on for a view, every preview command (Space, numeric 0, etc.) previews that view and brings it to front for the duration of the preview, regardless of which panel is focused. Only one view at a time (enabling it elsewhere disables it here).
  - Primary Viewer (主查看器（译名待核）) toggle (AE 2017+): defines which viewer/view is used for audio and Mercury Transmit external video preview; only one; when no view is primary, the most recently active view is used.
  - Without either, preview targets the comp of the active panel (Timeline or viewer), honouring locked-viewer rules (AE-UI-007, "ETLAT": Composition viewer locked + another comp's Timeline active → preview targets the locked viewer's comp).
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** Preview semantics: ui-preview-render-io.md, preview & cache behaviour (ui-b). BAE decision: external video output (Mercury Transmit) is out of scope; Primary Viewer selects the audio/preview target only.

### AE-UI-037 · View Options and Show Layer Controls (视图选项与显示图层控件)

- **What it does:** Per-view switches for which on-canvas layer controls are drawn (handles, masks, motion paths, effect control points, camera/light wireframes).
- **Exact semantics:**
  - View > View Options… (视图选项) Ctrl+Alt+U / Cmd+Opt+U dialog, per view:
    | Option (中文) | Default |
    |---|---|
    | Layer Controls (图层控件) master checkbox | on |
    | Handles (手柄) | on |
    | Masks (蒙版) | on |
    | Effect Controls (效果控件, effect control points) | on |
    | Motion Path (运动路径) | on |
    | Motion Path Tangents (运动路径切线) | on |
    | Camera and Spot Light Wireframes (摄像机和聚光灯线框) / Light wireframes | UNVERIFIED: popup "On Select" / "Off" / "On" |
    | Camera and Light Wireframes / 3D Reference Axes / Layer Paths (Shape paths) | UNVERIFIED |
    | Textures (纹理), Lighting (光照) | UNVERIFIED (OpenGL era) |
  - View > Show Layer Controls (显示图层控件) Ctrl+Shift+H / Cmd+Shift+H toggles the master switch (also hides the 3D reference axes).
  - Layer controls are drawn only for **selected** layers (handles, masks, motion paths), except mask/shape paths which are drawn for all layers that have the target visible (UNVERIFIED: AE draws masks only of selected layers).
  - Handle/path colour = layer label colour if Preferences > Appearance > Use Label Colors for Layer Handles and Paths (使用标签颜色用于图层手柄和路径) (default on); vertex/handle size = Preferences > Appearance > Path Point and Handle Size (路径点和手柄大小) (UNVERIFIED default 5 px).
- **Priority:** P0 (handles, masks, motion path); P1 (others)
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-038 · Layer bounding box and transform handles in the viewer (图层手柄)

- **What it does:** A selected 2D layer shows its bounding box with eight handles and its anchor point, allowing direct move/scale/rotate with the Selection and Rotation tools.
- **Exact semantics:**
  - Drawn for each selected layer: the layer's source rectangle (layer space source bounds, not mask bounds, transformed to comp space) as an outline, 8 square handles (4 corners + 4 edge midpoints), and the anchor point as a circle with crosshair at the layer's anchor in comp space. Shape and text layers without a fixed source use their content bounds (UNVERIFIED: text layers use the text bounding box, shape layers the content bounds).
  - Selection tool mouse behaviour on the viewer:
    | Gesture | Effect (one undo step per drag) |
    |---|---|
    | click on a layer | selects the topmost layer under the pointer (UNVERIFIED: AE hit-tests the layer's rendered alpha, not only its box) |
    | drag inside layer | move Position (Shift constrains to horizontal/vertical; snapping per ui-b) |
    | drag corner/edge handle | change Scale (edge = one axis, corner = both, non-uniform); Shift keeps aspect ratio (UNVERIFIED further modifiers) |
    | Alt/Option+drag handle | AE 2020+: scale about the opposite handle? (UNVERIFIED) |
    | near a corner handle outside the box with Selection tool | no rotation (AE requires the Rotation tool W) |
    | marquee drag on empty area | select layers whose bounds intersect the marquee |
    | double-click a precomp layer | opens the source comp; double-click a footage layer opens its Layer panel; double-click a text layer enters text editing; double-click a shape/mask path enters free-transform mode |
  - Arrow keys nudge Position of selected layers by 1 pixel at current magnification (Shift: 10 px); handled by ui-b shortcuts.
  - Rotation tool (W) drag rotates around the anchor point; Shift constrains to 45° steps.
  - Pan Behind tool (Y) drag moves the anchor point while compensating Position so the layer does not move visually (the semantic command is in transform-keyframes-expressions.md / bae-motion).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Tool shortcuts and full mouse behaviour per tool: ui-preview-render-io.md, tools (ui-b). Each drag must create keyframes when the property's stopwatch is on (at the CTI) and otherwise change the static value (transform-keyframes-expressions.md, Keyframes). Implementation hint: GPU picking with a layer-ID render target makes alpha hit-testing O(1).

### AE-UI-039 · Motion path display and editing in the viewer (运动路径)

- **What it does:** For a selected layer whose Position (or Anchor Point, or effect point) is animated, the viewer draws the spatial path with keyframe markers, frame dots and spatial Bezier handles that can be edited directly.
- **Exact semantics:**
  - Drawing: the path as a line in label colour; keyframes as small hollow squares (filled when selected); dots at each frame time (spacing visualises speed: close dots = slow); Bezier direction handles (tangents) at keyframes with spatial interpolation Bezier/Continuous/Auto (shown when the keyframe is selected or Motion Path Tangents option is on).
  - Extent: Preferences > Display > Motion Path (运动路径): No Motion Path (无运动路径), All Keyframes (所有关键帧), No More Than N Keyframes (不超过 N 个关键帧, default 15, UNVERIFIED default), Show Keyframes for Duration (No More Than 0:00:15:00, UNVERIFIED default/label).
  - Editing: drag a keyframe square → changes that keyframe's value (layer moves); drag a direction handle → changes spatial in/out tangents (converts to Bezier; tangent pair stays continuous unless Ctrl/Cmd-drag (Alt in pen-convert) breaks it, UNVERIFIED modifier); Pen tool/Convert Vertex tool (G) on a keyframe toggles linear/auto-bezier spatial interpolation; click a keyframe selects it (Shift-click adds).
  - Anchor-point path is drawn when the Pan Behind tool is active or Anchor Point is animated and selected (UNVERIFIED); in the Layer panel via View ▸ Anchor Point Path (AE-UI-04x).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Spatial interpolation semantics: transform-keyframes-expressions.md, spatial interpolation / roving. Implementation hint: sample the evaluated position at each frame once per edit (cheap) and cache the polyline.

### AE-UI-040 · Mask and shape path editing in the viewer (在查看器中编辑蒙版和形状路径)

- **What it does:** Mask paths (on any layer) and shape paths of selected layers are drawn with their vertices and can be edited in place in the Composition and Layer panels.
- **Exact semantics:**
  - Drawing: path outline in the mask's colour (masks have their own colour swatch in the Timeline) or layer label colour for shape paths; vertices as small squares (filled = selected, hollow = unselected); direction handles of selected vertices; the first vertex is drawn larger (UNVERIFIED).
  - Selection tool: click a vertex selects it; Shift-click toggles; drag selected vertices moves them; drag a segment moves the segment's two vertices (UNVERIFIED); marquee selects vertices; Alt/Option-click a path selects the entire mask/path; double-click a path → **Free Transform Points** (自由变换点) box (Ctrl+T / Cmd+T), drag box handles to scale/rotate the selected vertices about the box anchor; Enter/Esc or double-click outside exits.
  - Pen-tool family on paths (add/delete/convert vertex, close path, RotoBezier): see shapes.md, Shape tools / Bezier model and ui-preview-render-io.md tools (ui-b).
  - Editing a path when the Path stopwatch is on creates/updates a keyframe at the CTI (shape morphing).
  - With Toggle Mask and Shape Path Visibility off, paths are not drawn or editable (AE-UI-029).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-vector)
- **Notes:** Path data model: shapes.md, Bezier model; mask semantics: compositing-3d-color.md, Masks. Layer-panel mask editing behaves identically but in layer space (AE-UI-04x).

### AE-UI-041 · 3D transform gizmo and axis modes (3D 变换控件与轴模式)

- **What it does:** A selected 3D layer shows a 3D gizmo with coloured axes for constrained move, rotate and scale; the axis mode chooses the gizmo's orientation.
- **Exact semantics:**
  - Axis colours: X red, Y green, Z blue. Since AE 18.0 (2021, 3D Design Space) the gizmo has three modes switched by the active tool or by hovering: Position arrows (drag an arrow = move along that axis; drag the centre square/plane = move in a plane, UNVERIFIED), Scale cubes at the arrow tips (drag = scale along the axis; Shift? uniform), Rotation arcs (drag an arc = rotate about that axis; Shift snaps to 45°, UNVERIFIED). Before 18.0 only axis arrows were shown (Selection tool) and rotation used the Rotation tool's Orientation/Rotation set option.
  - Axis mode (Tools panel options, AE-UI-0xx Tools): Local Axis Mode (本地轴模式, gizmo aligned to the layer's own orientation), World Axis Mode (世界轴模式, aligned to comp axes), View Axis Mode (视图轴模式, aligned to the current view). Shortcut cycles? (UNVERIFIED).
  - Rotation tool option "Set: Orientation / Rotation" chooses whether rotating with the tool edits Orientation or X/Y/Z Rotation (旋转工具"设置"：方向/旋转).
  - Camera and light layers draw a wireframe (frustum/cone) and point-of-interest line; dragging the POI handle moves the Point of Interest.
  - Ground-plane / 3D reference axes (comp-centre axis tripod) drawn per AE-UI-028/034.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** 3D math and camera semantics: compositing-3d-color.md, Classic 3D renderer / Cameras; tool details: ui-b.

### AE-UI-042 · Composition panel context menu (合成面板上下文菜单)

- **What it does:** Right-click in the image area shows commands for the layer under the pointer (it becomes selected) or comp-level commands on empty pasteboard.
- **Exact semantics:**
  - On a layer (order UNVERIFIED): New ▸ (Text, Solid, Light, Camera, Null Object, Shape Layer, Adjustment Layer, …), Layer Settings… / Solid Settings…, Open Layer, Open Layer Source, Reveal in Explorer/Finder, Mask ▸, Mask and Shape Path ▸, Quality ▸, Switches ▸, Transform ▸, Time ▸, Frame Blending ▸, 3D Layer, Guide Layer, Environment Layer, Markers ▸, Blending Mode ▸, Layer Styles ▸, Effect ▸, Keyframe Assistant ▸, Track & Stabilize ▸, Open Effect Controls, Create ▸ (Create Shapes from Vector Layer, Create Masks from Text, …), Camera ▸, Pre-compose…, Invert Selection, Select ▸ (layers under the pointer — lets the user pick an occluded layer, UNVERIFIED), Select Children, Rename.
  - On empty area: New ▸, Composition Settings…, Reveal Composition in Project, Preview ▸, Switch 3D View ▸, Rename, Composition Flowchart, Composition Mini-Flowchart (UNVERIFIED list).
  - Every entry invokes the same command as the corresponding Layer / Composition menu item (menus: ui-b).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** —

## Layer and Footage viewers

### AE-UI-043 · Layer panel (图层面板)

- **What it does:** Shows one layer's source in layer space, before the layer's transforms (and, with Render off, before masks/effects), for tasks that need source coordinates: drawing masks, paint, Roto Brush, motion tracking, puppet pins, trimming.
- **Exact semantics:**
  - Open: double-click a footage/solid layer in the Timeline or Composition panel, or Layer > Open Layer (打开图层); for a precomp layer: Alt-double-click (Win) / Option-double-click (Mac) or Layer > Open Layer (plain double-click opens the source comp instead). Synthetic layers (text, shape, camera, light, null? (UNVERIFIED), adjustment-solid OK) cannot be opened; text/shape layers must be precomposed first. Tab title `Layer <layer name>` (`图层 <name>`).
  - Image area: the layer's source frame at source dimensions with the pasteboard around it; masks, paint strokes and effect control points are drawn in layer space. All viewer controls of AE-UI-023 apply except comp-only ones (3D view, view layouts, Draft 3D, renderer, Fast Previews, resolution — the Layer panel uses the comp's resolution, UNVERIFIED).
  - Extra controls above the time ruler: **View (视图)** popup and **Render (渲染)** checkbox (AE-UI-044); time ruler with layer In/Out brackets and buttons (AE-UI-045).
  - Layer-panel-only channel overlays: Alt+Shift+4 (Opt+Shift+4) alpha boundary, Alt+5 (Opt+5) alpha overlay (used by Roto Brush/Refine Edge); colour/opacity in the panel menu (UNVERIFIED).
  - Viewer lock (AE-UI-007) applies: an unlocked Layer viewer is replaced by the next opened layer.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** "Layer space" and source rect definitions: compositing-3d-color.md, Layer bounds. Implementation hint: render the layer node of the render graph only up to the chosen stage (source / masks / chosen effect), see AE-UI-044.

### AE-UI-044 · Layer panel View menu and Render checkbox (图层面板"视图"菜单和"渲染"复选框)

- **What it does:** Selects which stage of the layer's render pipeline is shown and which controls are editable in the Layer panel.
- **Exact semantics:**
  - View popup entries (UNVERIFIED exact order): **Masks (蒙版)** (default), **Anchor Point Path (锚点路径)** (shows and lets you edit the anchor-point motion path), then one entry per effect on the layer that has on-screen controls or paint/roto data, by effect instance name (e.g. "Paint" (绘画), "Roto Brush & Refine Edge" (Roto 笔刷和调整边缘), "Puppet" (操控), "Motion Tracker Points" (运动跟踪点) when tracking).
  - Render (渲染) checkbox (default on): on → the image shows the layer rendered through masks and effects **up to and including the effect chosen in View** (for "Masks"/"Anchor Point Path": through all masks and effects, UNVERIFIED); off → shows the original unaltered source (interpreted footage).
  - Choosing an effect in View makes its paths/points/strokes editable and hides others'; painting with Brush/Clone/Eraser requires a Paint view (AE switches View automatically to the Paint effect it creates).
  - Grid/guides, snapshots, channels, exposure, ROI behave as in the Composition viewer.
- **Priority:** P1 (Masks/Render P0)
- **Owner:** bae-ui-motion (+ bae-render, bae-effects)
- **Notes:** Effect render order: compositing-3d-color.md, render order; Paint/Roto: effects.md and AE-UI-0xx (Paint, Brushes panels).

### AE-UI-045 · Layer panel time ruler and trimming (图层面板时间标尺和修剪)

- **What it does:** The Layer panel's own time ruler shows the layer's full source duration; its In/Out brackets trim the layer in the composition.
- **Exact semantics:**
  - Time ruler (时间标尺) spans the layer's source duration (for still/solid layers the comp duration, UNVERIFIED), with its own CTI (synchronised with the comp CTI through the layer's time mapping), In point and Out point brackets, and readouts for In, Out and Duration (Δ) in the project time display style.
  - Trimming: drag the In or Out bracket; or move the CTI and click **Set In Point (设置入点)** / **Set Out Point (设置出点)** buttons (or Alt+[ / Alt+] (Opt+[ / Opt+]) at the CTI, UNVERIFIED in this panel). The change is the same command as trimming the layer bar in the Timeline (project-comp-layers.md, layer attributes & timing: trim); it does not change the layer's Start Time (the layer's source stays anchored), one undo step per drag.
  - Time-remapped layers: the ruler shows the remapped source; preference/panel menu option to show layer time vs comp time? (UNVERIFIED).
  - Keyboard J/K navigation also stops at Roto Brush span boundaries in this panel.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-UI-046 · Footage panel (素材面板)

- **What it does:** Shows a footage item (not a layer) as interpreted by Interpret Footage, with its own time ruler, for viewing and three-point-style editing into the active comp.
- **Exact semantics:**
  - Open: double-click the item in the Project panel, numeric-keypad Enter with Project panel focus, or Alt-double-click a layer in the Timeline (Layer > Open Layer Source (打开图层源)). Tab title `Footage <item name>` (`素材 <name>`). Alt-double-click in the Project panel opens the OS player instead (AE-UI-019).
  - Shows interpreted pixels (alpha interpretation, PAR, field separation, frame rate conform, colour profile) at the item's native size; no layer masks/effects.
  - Bottom/above-ruler controls: viewer controls of AE-UI-023 subset (magnification, grid/guides, snapshot, channel, exposure, transparency, PAR toggle, Always Preview, Primary Viewer), time ruler for the footage's duration with CTI and In/Out brackets, In/Out/Duration readouts, **Set In Point (设置入点)**, **Set Out Point (设置出点)**, **Edit Target (编辑目标（译名待核）)** label naming the comp that will receive edits (the most recently active comp), **Ripple Insert Edit (波纹插入编辑)** and **Overlay Edit (叠加编辑)** buttons (AE-UI-047).
  - Stills and solids: no time ruler controls (UNVERIFIED).
  - Audio-only footage: shows an audio waveform? (UNVERIFIED: AE shows a blank image area with audio playback on preview).
- **Priority:** P0 (viewing); P1 (editing controls)
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** Interpretation semantics: project-comp-layers.md, Interpret Footage.

### AE-UI-047 · Overlay Edit and Ripple Insert Edit (叠加编辑和波纹插入编辑)

- **What it does:** Inserts the Footage panel's trimmed range as a new layer into the edit-target comp at the comp CTI.
- **Exact semantics:**
  - Both create a layer from the footage item at the **top** of the layer stack, with the layer's In point at the comp's current time and the source trimmed to the Footage panel's In/Out (layer In point shows source frame = Footage In; duration = Out − In + 1 frame).
  - **Overlay Edit (叠加编辑)**: no other layers change.
  - **Ripple Insert Edit (波纹插入编辑)**: additionally splits every other layer that spans the CTI (as Split Layer at CTI, project-comp-layers.md, layer editing ops: Split Layer) and moves all resulting parts that start at or after the CTI later by the inserted duration, so their In points equal the inserted layer's Out point. UNVERIFIED: whether layers entirely after the CTI also shift (believed yes).
  - One undo step each; edit target = most recently activated comp (shown in the panel); disabled if no comp is open.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** The commands live in bae-motion (pure editing); the buttons are UI only.

## Timeline panel

### AE-UI-048 · Timeline panel layout (时间轴面板)

- **What it does:** The Timeline panel shows one composition: a layer outline (columns of switches and properties) on the left and a time graph (time ruler, layer duration bars, keyframes, markers, or the Graph Editor) on the right, sharing rows.
- **Exact semantics:**
  - Regions:
    | Region (中文) | Content |
    |---|---|
    | Top-left header | current-time display (AE-UI-060), search field (AE-UI-059), comp buttons: Composition Mini-Flowchart (合成微型流程图), and composition switches (AE-UI-058) |
    | Column header row (left) | headers of the visible columns (AE-UI-049) |
    | Layer outline (图层大纲（译名待核）) (left) | one row per layer in stacking order (layer 1 at top), each twirlable into property groups/properties |
    | Top of time graph (right) | time navigator (AE-UI-061), work area bar (AE-UI-062), time ruler with cache indicators (AE-UI-063/064), comp marker bin at the far right (AE-UI-065) |
    | Time graph (时间图表) (right) | layer bar mode (图层条模式): layer duration bars (AE-UI-066), keyframes (AE-UI-070), markers; or Graph Editor mode (图表编辑器) (AE-UI-077+) |
    | Bottom-left | pane toggle buttons: Layer Switches (图层开关) pane, Transfer Controls (转换控制) pane, In/Out/Duration/Stretch (入点/出点/持续时间/伸缩) pane, Frame Render Time (帧渲染时间) pane (AE-UI-049, AE-UI-057) |
    | Bottom centre | Toggle Switches / Modes (切换开关/模式) button (AE-UI-049) |
    | Bottom of time graph | zoom-out button, time zoom slider, zoom-in button (AE-UI-061) |
  - A vertical divider between outline and time graph is draggable; the CTI line spans the whole time graph.
  - Several Timeline panels can be open (one per comp, as tabs); the active Timeline's comp is the "active comp" for menu commands; `\` toggles focus between the comp's Timeline and Composition viewer.
  - Shift+F3 toggles layer bar mode / Graph Editor mode (also the Graph Editor comp switch).
  - Timeline panel menu (☰) (UNVERIFIED exact list): Columns ▸, Composition Settings…, Composition Mini-Flowchart, Show Cache Indicators (显示缓存指示器), Hide Shy Layers, Enable Frame Blending, Enable Motion Blur, Use Keyframe Icons / Use Keyframe Indices (使用关键帧图标 / 使用关键帧索引), Show Comp Name? plus docking entries.
  - Row height: layer rows and property rows have fixed heights (AE: ~19 px at default UI scale, UNVERIFIED); Preferences > Appearance affects text size only on some platforms.
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Implementation hint: virtualise rows (only build visible rows) and draw the time graph with GPU primitives; long comps with thousands of keyframes must scroll at 60 fps.

### AE-UI-049 · Timeline columns and panes (时间轴列与窗格)

- **What it does:** The layer outline consists of column groups that can be shown, hidden and reordered; pane buttons and F4 switch between the Switches and Modes sets.
- **Exact semantics:**
  - Columns (in default left-to-right order; Columns ▸ menu on any column header right-click):
    | Column (中文) | Default | Content (UI item) |
    |---|---|---|
    | A/V Features (A/V 功能) | shown | Video, Audio, Solo, Lock switches; keyframe navigator and stopwatch area for property rows (AE-UI-050, AE-UI-069) |
    | Label (标签) | shown | label swatch (AE-UI-020) |
    | # (layer number) | shown | layer index 1..N (AE-UI-051) |
    | Source Name / Layer Name (源名称 / 图层名称) | shown | twirl + name (AE-UI-051) |
    | Comment (注释) | hidden | AE-UI-056 |
    | Switches (开关) | shown | Shy, Collapse/Continuously Rasterize, Quality, Effect, Frame Blending, Motion Blur, Adjustment Layer, 3D Layer (AE-UI-052) |
    | Modes (模式) | shown (as alternative to Switches when toggled, see below) | Mode, T (Preserve Underlying Transparency), Track Matte (AE-UI-053) |
    | Parent & Link (父级和链接) | shown | AE-UI-054 |
    | Keys (键（译名待核）) | hidden | keyframe navigator in its own column (AE-UI-069) |
    | In (入) / Out (出) / Duration (持续时间) / Stretch (伸缩) | hidden | AE-UI-055 |
    | Render Time (渲染时间（译名待核）) | hidden | AE-UI-057 |
  - Pane buttons (bottom-left): Layer Switches pane toggles the Switches column; Transfer Controls pane toggles Modes; In/Out/Duration/Stretch pane toggles those four; Frame Render Time pane toggles Render Time. F4 = **Toggle Switches / Modes**: when both are visible it hides one; repeatedly alternates Switches ↔ Modes (also the bottom button). Shift+F4 shows/hides Parent & Link.
  - Columns can be dragged to reorder and resized where allowed (generic: AE-UI-009). Column visibility/order is per Timeline panel and saved with the workspace? (UNVERIFIED: saved in the project per comp).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Switch semantics are data attributes in project-comp-layers.md, Layer common attributes; this item only defines layout.

### AE-UI-050 · A/V Features switches UI (A/V 功能开关)

- **What it does:** Per-layer Video (eyeball), Audio (speaker), Solo and Lock switches with click, drag-across and modifier behaviours.
- **Exact semantics:**
  - Video (视频): click toggles layer video (cameras/lights: on/off; adjustment layers: effect application). Shown for layers with video. Property groups that can be disabled (shape groups, text animators, masks' none-mode aside) also show an eyeball in this column; effects use their own fx switch (AE-UI-068) (UNVERIFIED list).
  - Audio (音频): shown only for layers with audio.
  - Solo (独奏): click toggles; **Alt-click (Win) / Option-click (Mac)** solos this layer exclusively (unsolos all others). Solo semantics: project-comp-layers.md, Solo switch.
  - Lock (锁定): click toggles; locked layers cannot be selected in the viewer or Timeline (clicking a locked layer row selects nothing / AE flashes the lock, UNVERIFIED); Ctrl+L / Cmd+L locks selected; Ctrl+Shift+L / Cmd+Shift+L unlocks all layers.
  - **Drag across**: press on a switch and drag vertically across adjacent layers' same switch → each crossed switch is set to the state the first click produced (not toggled). One undo step for the whole drag.
  - Each click is one undo step; switch changes apply to the clicked layer only, not to the selection (UNVERIFIED: AE ignores selection for switches).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Shortcut "show only solo layers in previews": project-comp-layers.md, Solo switch. Implementation hint: drag-across = collect touched layer IDs during the drag and commit one batch command.

### AE-UI-051 · Layer number and Layer Name / Source Name column (图层编号与图层名称/源名称列)

- **What it does:** Shows each layer's index and either its layer name or its source name; supports rename and twirl.
- **Exact semantics:**
  - # column: 1-based stacking index. Typing a layer number on the numeric keypad selects that layer (e.g. 1 then 2 within a short interval = layer 12; UNVERIFIED delay).
  - Name header click toggles **Layer Name (图层名称)** ↔ **Source Name (源名称)** display for all rows. In Layer Name mode, a layer that has not been renamed shows its source name in square brackets, e.g. `[Comp 1]`; renamed layers show the name without brackets (UNVERIFIED: brackets exact).
  - Rename: select a layer and press Enter (Win) / Return (Mac), or context menu Rename; Enter commits; Esc cancels; empty name reverts to source name (layer name cleared). Renaming properties/groups (masks, effects, shape groups, text animators) is done the same way.
  - Twirl triangle left of the name expands/collapses the layer (AE-UI-068). Layer type icon between label and name (UNVERIFIED: AE shows a type icon only for shape/text/camera/light/null? — AE shows the source type icon).
  - Name is drawn in italics when the source is missing? (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Data: project-comp-layers.md, Layer name and source name; Layer index and layer ID.

### AE-UI-052 · Switches column UI (开关列)

- **What it does:** Eight per-layer switches in fixed order, each a clickable icon cell; some have more than two states.
- **Exact semantics:**
  | Switch (中文) | Click cycle | Notes |
  |---|---|---|
  | Shy (消隐) | on/off | hidden from outline when comp Hide Shy Layers is on |
  | Collapse Transformations / Continuously Rasterize (折叠变换 / 连续栅格化) | on/off | shown only for precomp and vector-source (AI/PDF/SVG?) layers, text and shape layers have it too (Continuously Rasterize is always conceptually on for text/shape in AE? — AE text/shape layers show the switch and it toggles continuous rasterize; UNVERIFIED) |
  | Quality and Sampling (质量和采样) | Best ↔ Draft (Wireframe via Layer > Quality only) | icon shows Best-Bilinear, Best-Bicubic, Draft; Bicubic is set via Layer > Quality > Bicubic (UNVERIFIED: Alt-click cycles sampling) |
  | Effect (效果, "fx") | on/off | disables all effects on the layer without changing each effect's own fx switch; shown only if the layer has effects |
  | Frame Blending (帧混合) | off → Frame Mix → Pixel Motion → off | shown only for time-based footage/precomp layers (UNVERIFIED cycle order) |
  | Motion Blur (运动模糊) | on/off | |
  | Adjustment Layer (调整图层) | on/off | |
  | 3D Layer (3D 图层) | on/off | per-character 3D text shows a distinct "3D with sublayers" icon |
  - Drag-across behaviour as in AE-UI-050. Switches not applicable to a layer type are blank and not clickable (e.g. no 3D switch on cameras/lights/audio-only; no Collapse on solids).
  - Clicking a switch on a layer that is part of a multi-selection applies to all selected layers? (UNVERIFIED; AE applies to the clicked layer only).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Semantics of each switch: project-comp-layers.md, Layer common attributes (AE-PRJ-361…369); rendering: compositing-3d-color.md.

### AE-UI-053 · Modes column: Mode, Preserve Transparency and Track Matte controls (模式列)

- **What it does:** Per-layer blending mode popup, the T (Preserve Underlying Transparency) checkbox and the Track Matte controls.
- **Exact semantics:**
  - **Mode (模式)** popup: blend modes in AE menu order with separators between groups (list and formulas: compositing-3d-color.md, Blend modes). Shift+= / Shift+- (main keyboard) cycles the selected layers' modes forward/backward (UNVERIFIED: shortcut, see ui-b). Mouse wheel over the popup? (UNVERIFIED).
  - **T (保留基础透明度)**: checkbox; clicking toggles; drag-across supported.
  - **Track Matte (轨道遮罩)** (AE 23.0 (2023) UI): a pick whip, a popup listing "No Track Matte" (无轨道遮罩) and every other layer by "N. name", and two toggles: Alpha/Luma (Alpha 遮罩/亮度遮罩) and Inverted (反转). Dragging the pick whip onto another layer row sets it as matte; any layer can be the matte (not only the layer above); the matte layer's Video switch is turned off automatically when assigned (UNVERIFIED: only when chosen from the old adjacent-layer workflow). Legacy (pre-23) UI: a TrkMat popup with No Track Matte, Alpha Matte "layer", Alpha Inverted Matte, Luma Matte, Luma Inverted Matte using the layer directly above.
  - The matte layer row shows a matte icon next to its name and the matted layer an icon too (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Data: project-comp-layers.md, Blending Mode attribute / Track Matte attribute / Preserve Underlying Transparency; rendering: compositing-3d-color.md, Track mattes.

### AE-UI-054 · Parent & Link column (父级和链接列)

- **What it does:** Per-layer pick whip and popup to set the layer's parent.
- **Exact semantics:**
  - Popup lists None (无) and every other layer as "N. layer name" (excluding the layer itself and its descendants, which are disabled or absent; UNVERIFIED). Choosing sets the parent with transform compensation.
  - Pick whip (spiral icon) left of the popup: drag the line onto a layer row in the outline or onto a layer in the Composition viewer; drop on empty → no change. Modifiers while releasing: **Alt/Option** = parent without compensation ("jump": the child keeps raw values and visually jumps), **Shift** = move child to parent's position (AE-PRJ-397).
  - With several layers selected, setting the parent on one sets it for all selected layers (UNVERIFIED but believed).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model, bae-anim)
- **Notes:** Parenting semantics: project-comp-layers.md, Assigning and removing parents; compensation math: transform-keyframes-expressions.md, Parent assignment compensation.

### AE-UI-055 · In, Out, Duration and Stretch columns (入点、出点、持续时间和伸缩列)

- **What it does:** Numeric timing readouts for each layer that can be edited directly.
- **Exact semantics:**
  - Values shown in the project time display style (timecode or frames); Stretch in percent.
  - In (入): hot text; editing moves the whole layer in time so its In point lands at the new value (duration preserved) (UNVERIFIED: AE moves rather than trims).
  - Out (出): hot text; editing moves the layer so the Out point lands there (UNVERIFIED: or trims).
  - Duration (持续时间) and Stretch (伸缩): clicking opens the **Time Stretch (时间伸缩)** dialog (project-comp-layers.md, Time Stretch dialog).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Layer time model: project-comp-layers.md, Layer time model.

### AE-UI-056 · Comment and Keys columns (注释和键列)

- **What it does:** Comment shows a free-text note per layer; Keys shows each property's keyframe navigator in a dedicated column.
- **Exact semantics:** Comment: click the cell to edit inline; searchable by the Timeline search field only when the column is shown. Keys: when visible, the keyframe navigator (AE-UI-069) moves from the A/V Features column into this column (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Data: project-comp-layers.md, Layer comment.

### AE-UI-057 · Frame Render Time pane (帧渲染时间)

- **What it does:** Shows, for the current frame, how long each layer (and its masks/effects/expressions) took to render, to find slow layers.
- **Exact semantics:**
  - Toggle with the Frame Render Time pane button (bottom-left of Timeline) (AE 17.0 (2020), "Composition Profiler"). The footer shows total "Frame Render Time: N ms" for the comp at the CTI.
  - Render Time column: a duration (ms) and a horizontal bar per layer, colour-coded relative to the slowest layer (UNVERIFIED colours: green/yellow/red). Twirling a layer shows times for masks, effects (each effect) and expressions.
  - Values are from the last render of the current frame; they update after each render at a new time.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Implementation hint: GPU timestamp queries per render-graph node; aggregate per layer.

### AE-UI-058 · Composition switches in the Timeline (合成开关)

- **What it does:** Comp-wide switches at the top right of the layer outline.
- **Exact semantics:**
  | Button (中文) | Effect |
  |---|---|
  | Composition Mini-Flowchart (合成微型流程图) | opens the mini-flowchart (AE-UI-035) |
  | Draft 3D (草图 3D) | in AE ≤ 17.x here; since 18.0 in the Composition panel (AE-UI-034) |
  | Hide Shy Layers (隐藏为其设置了"消隐"开关的所有图层) | hides shy layers from the outline (they still render) |
  | Enable Frame Blending (为设置了"帧混合"开关的所有图层启用帧混合) | comp master switch for layer frame blending |
  | Enable Motion Blur (为设置了"运动模糊"开关的所有图层启用运动模糊) | comp master switch for layer motion blur |
  | Graph Editor (图表编辑器) | toggles Graph Editor mode (Shift+F3) |
  - Each is a toggle with on/off visual state; changes are undoable comp-setting edits (except Graph Editor, which is UI state).
  - zh-CN tooltip strings above are the long tooltip forms (UNVERIFIED exact strings).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Semantics: project-comp-layers.md, Composition switches.

### AE-UI-059 · Timeline search field (时间轴搜索)

- **What it does:** Filters the layer outline to layers and properties whose names (and shown column contents) match the query.
- **Exact semantics:**
  - Generic search syntax (spaces = AND, comma = OR, recent/saved searches): AE-UI-010.
  - Matches layer names, property names, property group names, effect names, and the text of shown columns (e.g. Comment); matching properties are revealed (twirled open) with their parent groups for context; non-matching layers are hidden.
  - If layers are selected, the filter applies only to the selected layers (unselected layers stay visible); with no selection it applies to all layers.
  - Clearing the field (× button or Esc) restores the previous twirl state (UNVERIFIED).
  - Ctrl+F (Cmd+F) with the Timeline focused puts the cursor in this field.
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Typical uses: search "Opacity" to show all opacity properties; "missing" works only in the Project panel.

### AE-UI-060 · Current-time display and Go To Time (当前时间显示与转到时间)

- **What it does:** Large readout of the comp's current time at the top-left of the Timeline; scrubbable and editable.
- **Exact semantics:**
  - Main readout: current time in the project's Time Display Style (timecode `0:00:00:00` (`;` separators for drop-frame), or frames `00000`, or feet+frames); a smaller secondary line shows the alternative unit and the comp frame rate, e.g. `00120 (24.00 fps)` (UNVERIFIED format).
  - Ctrl-click (Win) / Cmd-click (Mac) the readout toggles the project's Time Display Style between timecode and frames (project setting, project-comp-layers.md, Time Display Style).
  - Drag horizontally = scrub the CTI (hot text). Click = edit inline (or Go To Time dialog in older versions) (UNVERIFIED: current AE edits inline); Alt+Shift+J (Opt+Shift+J) opens the **Go to Time (转到时间)** dialog.
  - Input syntax: absolute timecode with or without separators (e.g. `110` = 1 second 10 frames in timecode mode, digits fill from the right: UNVERIFIED), frames in frames mode; `+N` / `+-N` advance / go back N frames; `+1:00` advance one second.
  - Values outside the comp duration clamp to [0, duration − 1 frame].
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Same readout behaviour in the Composition/Layer/Footage viewer toolbars.

### AE-UI-061 · Time navigator and time zoom (时间导航器与时间缩放)

- **What it does:** Controls which part of the comp duration is visible in the time graph.
- **Exact semantics:**
  - **Time navigator (时间导航器)**: a thin bar above the work area spanning the full comp duration, with a highlighted span and Time Navigator Start / End brackets; dragging a bracket zooms; dragging the span pans. Clicking it shows its start/end times in the Info panel.
  - Double-click the time navigator: zoom to frame level (individual frames visible); double-click again: show the entire comp. Shift-double-click: show entire comp; again: return to the previous navigator range.
  - Zoom buttons and slider at the bottom of the time graph; `=` zoom in, `-` zoom out (main keyboard); `;` (Win) / `Shift+;` (Mac) toggles between full comp duration and the frame-level zoom? (help: "zoom out to show the entire comp; press again to zoom back to the navigator range"; UNVERIFIED key per platform, see ui-b).
  - Mouse wheel: plain wheel scrolls the layer outline vertically; Shift+wheel scrolls time horizontally; Alt/Option+wheel zooms time around the pointer (AE-UI-011).
  - `D` (with Timeline focus, zoomed in) centres the time graph on the CTI. During preview the time graph auto-scrolls to keep the CTI visible (UNVERIFIED).
  - Pinch gestures on trackpads zoom time.
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Time zoom is per Timeline panel view state, not undoable.

### AE-UI-062 · Work area bar (工作区域栏)

- **What it does:** A bar under the time navigator marking the work area (the range previewed and rendered by default).
- **Exact semantics:**
  - Drag the start or end handle to set Work Area Start / End; drag the middle to move it keeping its duration; Shift while dragging snaps to the CTI, keyframes, markers and layer In/Out (UNVERIFIED snap targets same as CTI snapping).
  - B / N set work area start / end at the CTI; Ctrl+Alt+B (Cmd+Opt+B) sets the work area to the duration of the selected layers, or to the whole comp if none selected.
  - Double-click the bar: resets the work area to the whole comp duration (UNVERIFIED: or to fit the panel view).
  - Right-click the bar: Trim Comp to Work Area (将合成修剪到工作区域), Lift Work Area (提升工作区域), Extract Work Area (提取工作区域) (UNVERIFIED context menu).
  - Shift+Home / Shift+End move the CTI to work area start / end.
  - Moving the work area is an undoable comp edit.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Data and commands: project-comp-layers.md, Work area / Trim Comp to Work Area / Lift and Extract Work Area.

### AE-UI-063 · Time ruler and current-time indicator (时间标尺与当前时间指示器)

- **What it does:** The time ruler labels comp time; the CTI marks the current frame; clicking/dragging moves it.
- **Exact semantics:**
  - Ruler labels in the project Time Display Style, density adapting to zoom (e.g. `0s 05s 10s` at coarse zoom, `00f 05f 10f` at fine zoom in timecode mode; UNVERIFIED exact label format); frames always sit on frame boundaries; time displayed honours the comp's start timecode.
  - CTI (当前时间指示器): a head on the ruler plus a vertical line through the time graph (and Graph Editor). Click anywhere on the ruler = jump; drag = scrub (comp viewer updates live, rendering at the current preview resolution; without RAM-cached frames it shows what can be rendered).
  - Modifiers while dragging the CTI: **Shift** snaps to keyframes, markers, layer In/Out points, comp start/end and work area start/end of selected/visible items; **Ctrl (Win) / Cmd (Mac)** scrubs video and audio; **Ctrl+Alt / Cmd+Opt** scrubs audio only (a short audio section loops while the mouse is held still); **Alt/Option** scrub while a preview is playing does not stop the preview (otherwise scrubbing stops it); Caps Lock (prior to drag): scrub only RAM-cached frames (UNVERIFIED).
  - Keyboard (details in ui-b shortcuts): Page Down / Page Up next / previous frame (Shift: ±10 frames), Home / End comp start / end, J / K previous / next visible item (keyframe, marker, work area edge), Shift+J/K among keyframes and markers of selected layers, I / O to In / Out of selected layers.
  - CTI is clamped to [0, duration − 1 frame]; CTI position is per comp, saved with the project (`CompItem.time`).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Implementation hint: CTI drag must never block on rendering; display the last rendered frame and render the newest requested time (cancel stale requests).

### AE-UI-064 · Cache indicators (缓存指示器)

- **What it does:** Coloured bars along the time ruler show which frames are cached for instant playback.
- **Exact semantics:**
  - Green bar segments (绿色) = frames in the RAM cache; blue bar segments (蓝色) = frames in the disk cache (persistent cache); no bar = not cached. Drawn directly under the work area bar / above the ruler ticks, spanning frame intervals.
  - Panel menu > Show Cache Indicators (显示缓存指示器) toggles them (default on, UNVERIFIED).
  - Indicators update live during preview caching and are invalidated (removed) for frames affected by any edit; frames whose cache is still valid stay green after undo/redo (the cache is keyed by content).
  - AE 24.x+ (High Performance Preview Playback / Preview from Disk Cache) may play from disk-cached (blue) frames.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Cache semantics: ui-preview-render-io.md, preview & cache behaviour (ui-b).
