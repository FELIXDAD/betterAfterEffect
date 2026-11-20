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
