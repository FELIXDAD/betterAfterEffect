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
    | Effect Controls (效果控件) for selected layer | F3 or Ctrl+Shift+T | F3 or Cmd+Opt+T (as printed in Adobe's shortcut table; UNVERIFIED: some AE builds show Cmd+Shift+T in the Keyboard Shortcuts editor) |
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
  - Undock: panel menu **Undock Panel (浮动面板)** / **Undock Panel Group (浮动面板组)** (Adobe help calls the latter "Undock Frame"); or Ctrl-drag (Win) / Cmd-drag (Mac) the panel or group; or drag it outside the application window → a new floating window containing that panel/group. Floating windows accept docking like the main window and can go to a second monitor.
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
- **Notes:** Ctrl+` / Cmd+` is *not* a maximize variant: in AE it toggles expansion of the selected layers' properties in the Timeline (see ui-b). No other modifier variant of ` maximizes; the ` key is only bound to maximize/restore. Keyboard-shortcut list: see ui-preview-render-io.md, essential keyboard shortcuts (ui-b). Source: https://helpx.adobe.com/after-effects/using/workspaces-panels-viewers.html and keyboard-shortcuts-reference.html.

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
  - Predefined workspaces (AE 2024–2026; listed here alphabetically; UNVERIFIED: whether Window > Workspace lists them alphabetically or in workspace-bar order, and the exact set, which varies by version). Learn is opened by Help > Learn After Effects (学习 workspace = Learn, Project, Composition, Timeline panels):
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
  - Window > Workspace > **Save as New Workspace... (另存为新工作区（译名待核）)** (older help text: "New Workspace" / 新建工作区): name prompt, OK. UNVERIFIED: entering the name of an existing workspace asks to replace it.
  - Window > Workspace > **Delete Workspace (删除工作区)**: choose a workspace, OK; the currently active workspace cannot be deleted.
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
  | Gesture | Composition / Layer / Footage viewer | Timeline / Layer / Footage panels, pointer over the time navigator or time ruler | Lists (Project, Timeline, Render Queue, Flowchart, Effect Controls, Metadata, Effects & Presets) |
  |---|---|---|---|
  | Wheel forward / backward | zoom in / out. AE 25.2+ (Smooth Zoom, default Behaviour): about the **cursor**; legacy behaviour (older versions, or Preferences > Previews > Magnification set back to legacy/centre): about the **centre of the view** (about the feature region when tracking) | vertical scroll | vertical scroll |
  | Alt/Opt + wheel | zoom in / out with the *other* anchor: AE 25.2+ = about the **centre of the view**; legacy = about the **point under the pointer** (Alt/Opt dynamically swaps cursor vs centre) | zoom **time** in / out | — (UNVERIFIED: AE ≥ 2020 also zooms time with Alt+wheel over the Timeline layer-bar area) |
  | Shift + wheel | horizontal scroll | scroll in time; wheel **backward = later in time** and vice versa | horizontal scroll |
  - The wheel acts on the panel under the pointer even if that panel is not active (no click-to-focus needed).
  - Wheel over a tab row of a multi-panel group cycles the active tab (AE-UI-001).
  - Trackpad pinch: zoom (UNVERIFIED: supported on macOS in recent versions; treat as Alt+wheel).
  - Source: https://helpx.adobe.com/after-effects/using/general-user-interface-items.html ("Scroll or zoom with the mouse wheel", legacy text) and https://helpx.adobe.com/after-effects/using/modifying-using-views.html ("By default, zooming in or out with the mouse scroll wheel focuses on the cursor location. To zoom ... from the center of the view, hold Alt/Option"; Smooth Zoom section).
  - Preferences > Previews > Magnification (放大率（译名待核）): Type (Smooth / legacy fixed levels), Behaviour (zoom to cursor / centre), Trackpad Scrolling (scroll vs zoom) (UNVERIFIED: exact labels and value names). AE 25.3 on macOS made trackpad/Mouse scroll pan the view by default; the Trackpad Scrolling preference restores scroll-to-zoom.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** Zoom levels themselves: AE-UI-03x (Composition viewer magnification).

### AE-UI-012 · Scrubbable values (hot text) and numeric fields (可拖动数值)

- **What it does:** Every numeric value in Timeline, Effect Controls, Character/Paragraph, dialogs etc. is "hot text": drag horizontally to scrub it, click to type a new value.
- **Exact semantics:**
  - Hover → cursor becomes a pointing hand with left/right arrows. Drag left/right: value changes by `step × pixels`, default step 1 unit per pixel (property-specific: Opacity 1 %, Rotation 1°, Scale 1 %, Position 1 px; effect sliders use the slider's UI precision).
  - Modifiers while dragging: **Ctrl (Win) / Cmd (Mac)** = fine, ×0.1 step; **Shift** = coarse, ×10 step. Modifiers can be pressed/released mid-drag.
  - Click (no drag) → inline edit field with the value selected; Enter (main keyboard) or Tab commits (Tab moves to the next field), Esc cancels; clicking elsewhere commits.
  - Typed arithmetic: value fields and other number entries accept simple arithmetic expressions, e.g. `2*3` → 6, `4/2` → 2, `2e2` → 200 (documented since AE CS3). A relative change is made by typing the operator after the current value in the field (`100` → `100*1.5`). UNVERIFIED: whether parentheses and a leading bare operator (`*2` replacing the selected text) are evaluated.
  - Values clamp to the property's valid range (e.g. Opacity 0–100); some properties allow out-of-UI-range typing beyond the slider range (effect sliders, see AE-UI-11x Effect Controls).
  - Each completed drag is one undo step; while dragging the comp updates live.
  - Up/Down arrow keys in an edit field: UNVERIFIED (believed: no arrow-key increments in AE hot-text fields).
  - **Proportional Scrubbing (按比例拖动（译名待核）)** (AE Beta 26.2, 2026; P2): select the same property on several layers in the order wanted, then Ctrl+Alt-drag (Win) / Cmd+Opt-drag (Mac) its hot text: the first selected layer changes by 0 % of the scrub delta, the last by 100 %, layers in between by proportional shares, i.e. `delta_i = delta × i/(n−1)` for selection index i = 0..n−1 (UNVERIFIED: exactly linear by selection index). For Scale, all layers use the Constrain Proportions state of the last selected layer. Source: https://helpx.adobe.com/after-effects/using/proportional-scrubbing-in-timeline.html
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
    | Interpret Footage (解释素材) | opens Interpret Footage (Main) for the selected footage item (same as File > Interpret Footage > Main, Ctrl+Alt+G / Cmd+Opt+G) | drag a footage item onto it = Interpret Footage (Main) for that item; **Alt-click / Option-click** the button or **Alt/Option-drag** an item onto it = Interpret Footage for the item's **proxy** (File > Interpret Footage > Proxy). Source: https://helpx.adobe.com/after-effects/using/importing-interpreting-footage-items.html |
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
  | Alt+double-click (Win) / Option+double-click (Mac) footage | opens the source file in the player application the OS associates with that file type (BAE: open with OS default) |
  | Double-click folder | expands/collapses it (UNVERIFIED) |
  | Double-click placeholder or missing footage | does **not** open a Footage panel (scripting `FootageItem.openInViewer()` notes that missing and placeholder footage cannot be opened by double-clicking). UNVERIFIED: AE instead opens the Replace Footage File dialog so the user can relink. |
  | Double-click audio-only footage | opens it in a Footage panel with an audio waveform-less view and audio playback (UNVERIFIED) |
  | Numeric-keypad Enter | opens selected footage in Footage panels |
  - Ctrl+E (Win) / Cmd+E (Mac) = Edit Original (project-comp-layers.md, Edit Original).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-020 · Label colour menu and label groups (标签菜单)

- **What it does:** Clicking a label swatch (Project panel or Timeline) opens a menu of the 16 label colours plus None and Select Label Group; choosing a colour relabels all selected items/layers.
- **Exact semantics:**
  - Menu content: the label names in label order 1–16 (names/colours from Preferences > Labels; defaults in project-comp-layers.md, Labels), each with its swatch; separator; **None (无)** (label 0); separator; **Select Label Group (选择标签组)** = select all items (Project panel) / layers (Timeline) that have the same label as the clicked one. UNVERIFIED: the relative order of the colour list, None and Select Label Group (AE 2023+ may list Select Label Group first).
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
  - Since AE 25.2 (2025) **Smooth Zoom** (Preferences > Previews > Magnification > Type = Smooth; the alternative restores legacy fixed-level zooming (UNVERIFIED: exact label of the legacy value)): the magnification readout is hot text accepting any decimal value; wheel zoom is continuous; max zoom is limited by the image buffer, somewhere between 3200 % and 6400 %.
  - Keyboard: `.` zoom in, `,` zoom out (to next fixed level, around view centre); `/` (main keyboard) 100%; Shift+/ Fit; **Alt+/ (Opt+/)** Fit up to 100%. Double-click the Hand tool = centre (and fit) the comp in the panel; double-click the Zoom tool = 100% (Win) / 200% (Mac Retina) (UNVERIFIED: Retina value).
  - Mouse wheel zoom: AE 25.2+ default zooms around the cursor, Alt/Opt+wheel around the view centre (legacy versions: the reverse), see AE-UI-011. While scroll-zooming, Shift speeds up and Ctrl/Cmd slows down. Source: https://helpx.adobe.com/after-effects/using/modifying-using-views.html
  - Pan: Hand tool drag, Space-drag, H-drag, middle-mouse drag; Shift while panning = faster.
  - Magnification is per view, not saved in the project (UNVERIFIED: saved with workspace/viewer state).
  - With Auto resolution, magnification drives the effective resolution (compositing-3d-color.md, Auto resolution).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** AE renders vector content before scaling the preview, so zoomed-in vectors look pixelated unless rendered at that magnification; BAE decision: when performance allows, re-rasterise vectors at view magnification for sharp zoom (view-only; render output unaffected). Zoom Quality preference: compositing-3d-color.md, Viewer Quality preferences.

### AE-UI-025 · Resolution and Fast Previews menus (分辨率和快速预览菜单)

- **What it does:** Viewer popups that choose the comp's preview resolution (Full, Half, Third, Quarter, Custom, Auto) and the interactive fast-preview mode.
- **Exact semantics:**
  - Resolution / Down Sample Factor popup (分辨率/向下采样系数): Full (完整), Half (二分之一), Third (三分之一), Quarter (四分之一), Custom… (自定义, dialog with horizontal/vertical factors), Auto (自动). The value is a comp setting shared by all views of the comp (stored in Composition Settings). Shortcuts: Ctrl+J (Cmd+J) Full, Ctrl+Shift+J (Cmd+Shift+J) Half, Ctrl+Alt+J (Cmd+Opt+J) Custom (Adobe shortcut table); Ctrl+Alt+Shift+J (Cmd+Opt+Shift+J) Quarter (UNVERIFIED: present in AE's default keymap but not in the Adobe table). UNVERIFIED: menu order (Auto may be listed first). Auto adapts to the zoom of the view with the highest magnification when the panel has several views, and is ignored for comps with Preserve Resolution When Nested. The Layer viewer's resolution is tied to its comp's Composition viewer.
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
  - 3D View popup (3D 视图): entries and semantics in compositing-3d-color.md, 3D views. View > Switch 3D View ▸ (切换 3D 视图), View > Switch to Last 3D View (切换到上一个 3D 视图; also Esc). F10/F11/F12 switch to the three assignable views (defaults Front, Custom View 1, Active Camera); Shift+F10/F11/F12 assigns the current view to that key (View > Assign Shortcut to ▸). View > Reset Default Camera / Reset 3D View (重置 3D 视图（译名待核）) and View > Look at Selected Layers (查看选定图层) Ctrl+Alt+Shift+\ / Cmd+Opt+Shift+\ , Look at All Layers (查看所有图层): with a camera tool active, F = look at selected 3D layers and Ctrl+Shift+F (Cmd+Shift+F) = look at all 3D layers (Adobe shortcut table).
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
  - Shortcuts: Alt+1/2/3/4 (Opt+1/2/3/4) toggle red/green/blue/alpha as grayscale; Alt+Shift+1/2/3 colourised channels; Alt+Shift+4 (Opt+Shift+4) toggles RGB Straight; repeating returns to RGB. Alt/Option-click the Show Channel button toggles Alpha ↔ RGB. Layer panel only: Alt+5 Alpha Boundary, Alt+6 Alpha Overlay, Alt+X Refine Edge X-ray (AE-UI-044).
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
    | Time graph (时间图表) (right) | layer bar mode (图层条模式): layer duration bars (AE-UI-066), keyframes (AE-UI-071, AE-UI-072), markers (AE-UI-065); or Graph Editor mode (图表编辑器) (AE-UI-077+) |
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
    | A/V Features (A/V 功能) | shown | Video, Audio, Solo, Lock switches; keyframe navigator and stopwatch area for property rows (AE-UI-050, AE-UI-070) |
    | Label (标签) | shown | label swatch (AE-UI-020) |
    | # (layer number) | shown | layer index 1..N (AE-UI-051) |
    | Source Name / Layer Name (源名称 / 图层名称) | shown | twirl + name (AE-UI-051) |
    | Comment (注释) | hidden | AE-UI-056 |
    | Switches (开关) | shown | Shy, Collapse/Continuously Rasterize, Quality, Effect, Frame Blending, Motion Blur, Adjustment Layer, 3D Layer (AE-UI-052) |
    | Modes (模式) | shown (as alternative to Switches when toggled, see below) | Mode, T (Preserve Underlying Transparency), Track Matte (AE-UI-053) |
    | Parent & Link (父级和链接) | shown | AE-UI-054 |
    | Keys (键（译名待核）) | hidden | keyframe navigator in its own column (AE-UI-070) |
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
  - Video (视频): click toggles layer video (cameras/lights: on/off; adjustment layers: effect application). Shown for layers with video. Property groups that can be disabled (shape groups, text animators, masks' none-mode aside) also show an eyeball in this column; effects use their own fx switch (AE-UI-087) (UNVERIFIED list).
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
  - Twirl triangle left of the name expands/collapses the layer (AE-UI-069). Layer type icon between label and name (UNVERIFIED: AE shows a type icon only for shape/text/camera/light/null? — AE shows the source type icon).
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
- **Exact semantics:** Comment: click the cell to edit inline; searchable by the Timeline search field only when the column is shown. Keys: when visible, the keyframe navigator (AE-UI-070) moves from the A/V Features column into this column (UNVERIFIED).
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

### AE-UI-065 · Composition marker bin and markers in the time graph (合成标记素材箱与时间图表中的标记)

- **What it does:** Displays comp markers on the time ruler and layer markers on layer bars, and provides the gestures that create, move, extend and delete them.
- **Exact semantics:**
  - **Comp Marker Bin (合成标记素材箱)**: a button at the right end of the time ruler. Drag out of it onto the ruler → new comp marker at the drop time (Shift snaps). Drag a comp marker back onto the bin → deletes it. Click (no drag) does nothing (UNVERIFIED).
  - Comp markers are drawn on the time ruler as small house-shaped (pentagon, point down) flags in the marker's label colour with a vertical guide line through the time graph while hovered/dragged (UNVERIFIED: guide line only while dragging); the marker's Comment text is drawn to the right of the flag; numbered markers (0–9, set with Shift+main-keyboard digit, see ui-b) show the digit inside the flag.
  - Layer markers are drawn on the layer's duration bar row as small triangles/flags (UNVERIFIED shape) in label colour with the comment text to the right, clipped at the next marker.
  - A marker with duration > 0 is drawn as the flag plus a horizontal span bar to its out time, with a draggable handle at the out end.
  - Gestures (semantics in project-comp-layers.md, Marker editing operations): double-click = marker settings dialog; drag = move (Shift snaps); drag out-handle = change duration; Alt/Option-drag the flag of a zero-duration marker = create duration; Ctrl/Cmd-click = delete; right-click = marker context menu: Settings (设置), Lock Markers (锁定标记), Delete This Marker (删除此标记), Delete All Markers (删除所有标记), Update Markers From Source (从源更新标记) (precomp layers only), Label ▸ (AE 22.6+).
  - Hover a marker: tooltip shows its comment and time (UNVERIFIED).
  - Alt/Option-click two markers (or keyframes): the Info panel shows the duration between them.
  - `*` on the numeric keypad adds a marker at the CTI on the selected layers, or a comp marker if no layer is selected; works during preview (shortcut details: ui-b).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Marker data: project-comp-layers.md, Marker data model / Composition markers / Layer markers. Implementation hint: render marker labels with ellipsis clipping against the next marker x position; collision-free placement is not required (AE overlaps them).

### AE-UI-066 · Layer duration bars (图层持续时间条)

- **What it does:** In layer bar mode each layer is a horizontal bar spanning its In to Out point on the time graph; the bar is the handle for moving, trimming and slipping the layer.
- **Exact semantics:**
  - Geometry: x from `inPoint` to `outPoint` in comp time (frame-quantised edges); height = layer row height minus a small inset. Colour = the layer's label colour (AE draws it as a tinted/darker shade of the label colour; selected layers are drawn brighter/highlighted). Layers with label None draw a neutral grey bar (UNVERIFIED shades).
  - **Slip-edit bar (滑动编辑条（译名待核）)**: after a layer based on moving footage (or a precomp/audio) has been trimmed, a pale rectangle extends the bar over the excluded source frames at either end; it never appears for stills (Adobe help, Trim, extend, or slip-edit a layer).
  - **Negative layer time**: when a layer is extended before its layer time 0 (time-remapped, shape, still, camera, light, text layers), hash marks along the bottom of the bar mark the portion in negative layer time.
  - Stills, solids, text, shape, null, camera, light, adjustment layers: bar ends are always trimmable without a source limit (project-comp-layers.md, Layer extent limits).
  - Time-stretched layers (stretch ≠ 100 %) and time-remapped layers draw no special hatching; the Stretch column shows the factor (UNVERIFIED: AE draws no stretch indicator on the bar). Time-reversed layers (negative stretch) draw a diagonal hatch/stripe pattern along the bar (UNVERIFIED pattern).
  - Bars for layers whose Video and Audio switches are both off are drawn dimmed (UNVERIFIED).
  - Collapsed layers with keyframes show nothing on the bar; collapsed property groups show grey **summary keyframe dots** (汇总关键帧指示器（译名待核）) on the group row at times where any contained property has a keyframe.
  - Layers starting before 0 or ending after the comp end are clipped to the time graph extent.
  - Hover over the bar ends shows the trim cursor; over the bar body the move cursor; over the pale source region the slip cursor.
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Data: project-comp-layers.md, Layer time model. Implementation hint: draw all bars in one instanced GPU pass; label colour lookup from Preferences > Labels.

### AE-UI-067 · Layer selection, multiple selection and stacking reorder in the outline (图层选择与在大纲中重新排序)

- **What it does:** Selecting layers in the Timeline (also mirrored in the Composition viewer) and dragging them to new stacking positions.
- **Exact semantics:**
  - Click a layer row (name area or bar) = select only that layer (deselects properties/keyframes on other layers). Ctrl-click (Win) / Cmd-click (Mac) toggles a layer in the selection; Shift-click extends a contiguous range from the last clicked layer. Dragging a marquee starting in an empty area of the outline (left part) selects the touched layer rows (UNVERIFIED: AE marquee in the outline selects layers; in the time graph it selects keyframes).
  - Click empty space below the last layer = deselect all (Ctrl+Shift+A / Cmd+Shift+A also deselects all).
  - Layer selection is shared with the Composition viewer (selecting in either highlights in both) and is persisted per comp (`Layer.selected`, project-comp-layers.md, Layer selection state).
  - Keyboard selection: numeric keypad digits select by layer number; Ctrl/Cmd+↑/↓ select previous/next layer, add Shift to extend (full list: ui-b shortcuts).
  - Locked layers cannot be selected by clicking in the outline (AE flashes the lock switch, UNVERIFIED) and are skipped by Select All (Ctrl/Cmd+A).
  - **Drag reorder**: press on a selected layer's name/number area and drag vertically; a horizontal insertion line shows the target gap; release moves all selected layers there keeping relative order (project-comp-layers.md, Reorder layers). Dragging near the top/bottom edge auto-scrolls.
  - Dragging a layer's name onto another layer's Parent pick whip is not supported (only pick whip → target) (UNVERIFIED).
  - Selecting a property or property group name selects all its keyframes (AE-UI-072).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model, bae-motion)
- **Notes:** Implementation hint: selection is UI state stored in the doc as non-undoable view state (AE does not put selection changes on the undo stack).

### AE-UI-068 · Moving, trimming and slipping layer bars (移动、修剪和滑动图层条)

- **What it does:** Direct manipulation of layer bars in the time graph.
- **Exact semantics:**
  | Gesture | Result | Semantics |
  |---|---|---|
  | Drag bar body | moves selected layers in time (all selected move by the same delta) | project-comp-layers.md, Move layers in time |
  | Drag bar start/end edge | trims In/Out of that layer (all selected layers whose edge is grabbed? — UNVERIFIED: AE trims only the dragged layer) | project-comp-layers.md, Trim layer in/out points |
  | Drag pale untrimmed region (time-based sources) | slip edit | project-comp-layers.md, Slip edit |
  | Pan Behind tool (Y) dragging a bar | slip edit | same |
  | Ctrl-drag (Win) / Cmd-drag (Mac) a bar edge | time-stretches the layer instead of trimming (UNVERIFIED modifier; keyboard equivalents Ctrl/Cmd+Shift+, and Ctrl/Cmd+Alt/Opt+, stretch the In/Out to the CTI) | project-comp-layers.md, Time Stretch dialog |
  | Shift while dragging | snaps the moving edge/body to CTI, work area edges, comp/layer markers, keyframes and other layers' In/Out points | snapping applies after the drag begins |
  - While dragging, the Info panel shows the layer's new In, Out and delta (e.g. `In: 0:00:01:00, Out: …, Delta: +0:00:00:12`) (UNVERIFIED exact text).
  - Each drag is one undo step for all affected layers; drags are frame-quantised to the comp frame rate.
  - A small tooltip near the pointer shows the current In time while dragging (AE 2020+; UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Keyboard alternatives ([, ], Alt+[, Alt+], Alt+PageUp/PageDown…) are in ui-b shortcuts and project-comp-layers.md.

### AE-UI-069 · Property twirl-downs and property rows (属性展开与属性行)

- **What it does:** Each layer row expands into a tree of property groups and properties; each property row shows its name, its value(s) as hot text, and in the time graph its keyframes.
- **Exact semantics:**
  - Twirl (展开) triangle: click toggles one level; **Ctrl-click (Win) / Cmd-click (Mac)** a twirl expands/collapses the group and all its descendants recursively (UNVERIFIED modifier: AE help says Ctrl-click expands all subgroups). Clicking the triangle of one selected layer while several are selected toggles all selected (UNVERIFIED).
  - Group order under a layer follows the property tree (project-comp-layers.md, Layer top-level property groups): e.g. Text, Contents, Masks, Effects, Transform, Layer Styles, Geometry/Material Options, Audio. Groups that are empty are hidden (Masks with no masks etc.).
  - Reveal shortcuts (A, P, S, R, T, U, UU, E, EE, F, M, MM, L, LL, …; Shift+key adds to the revealed set; "SS" reveals the selected property) show only the requested properties and collapse others: list in ui-b shortcuts.
  - Property row layout (left to right): A/V column area: (a) keyframe navigator ◀ ◆ ▶ (only when the property has keyframes, AE-UI-070); (b) indent + twirl (only for groups, or for a property with an expression); (c) stopwatch (only for animatable properties); (d) Graph Editor set switch (only when animated, AE-UI-077); (e) property name; (f) value(s) as hot text (AE-UI-070); (g) for some properties: constrain-proportions link (Scale), "Edit…" link (e.g. Source Text has none; Material options none) or a popup (enumerated properties like Mask Mode, blend modes of shape fills) or a checkbox.
  - Effect group rows in the Timeline show an **fx** switch (效果开关（译名待核）) in the switch area to enable/disable that effect; masks show mode popup, Inverted checkbox and (in the Switches column area) nothing; shape groups and text animators show an eyeball in the A/V column.
  - Value colours: normal values in the theme's hot-text colour (blue); values computed by an active expression are drawn red (post-expression value); values of properties that are disabled draw grey.
  - Selecting a property name selects all its keyframes; selecting a group name selects the group (for delete/duplicate/rename).
  - Double-click a property row name: no effect (UNVERIFIED); double-click a group name: rename (masks, effects, shape groups, animators).
  - Multi-selected properties of the same type on several layers can be changed together by scrubbing one of them (relative change, transform-keyframes-expressions.md, Change values of multiple keyframes at once).
  - Property right-click context menu: Reset (重置), Edit Value… (编辑值…), Expression ▸? / Add Expression? (UNVERIFIED), Show Expression Editor? (UNVERIFIED list), Separate Dimensions (单独尺寸) (Position only), Toggle Hold Keyframe, Keyframe Interpolation…, Rove Across Time, Keyframe Velocity…, Keyframe Assistant ▸, Copy Expression Only, Reveal in Timeline? (UNVERIFIED list).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Property tree data: project-comp-layers.md, Property tree structure / Property value types. Implementation hint: flatten the visible tree into a row list each frame (only twirled-open groups) and virtualise.

### AE-UI-070 · Stopwatch, keyframe navigator and property value editing (秒表、关键帧导航器和属性值编辑)

- **What it does:** The per-property controls that turn animation on/off, step between keyframes and edit values directly in the Timeline.
- **Exact semantics:**
  - **Stopwatch (秒表)**: inactive (no keyframes) / active (has keyframes). Click inactive → creates a keyframe at the CTI with the current value. Click active → deletes all keyframes, value becomes the value at the CTI (transform-keyframes-expressions.md, Stopwatch and auto-keyframing). **Alt-click (Win) / Option-click (Mac)** → adds the default expression (or removes the existing one) (AE-UI-073).
  - When the stopwatch is inactive, the time graph shows an I-beam marker under the CTI on that property row (no interpolation).
  - **Keyframe navigator (关键帧导航器)**: `◀ [box] ▶`. Arrows jump the CTI to the previous / next keyframe of this property (arrow greyed when none in that direction). Middle box: filled diamond = CTI exactly on a keyframe (click removes it); empty box = CTI between keyframes (click adds a keyframe with the current value). Shown only when the property has ≥ 1 keyframe. With the Keys column shown, the navigator moves into that column.
  - **Value hot text (可拖动值)**: drag horizontally to scrub. Modifiers while scrubbing: **Ctrl (Win) / Cmd (Mac)** = fine (1/10 of normal increment), **Shift** = coarse (10× normal increment) (generic hot text: AE-UI-012). Click = edit as text field (Tab moves to the next value field; Enter commits; Esc cancels). Arithmetic in fields: `+N`, `-N`, `*N`, `/N` applied to the current value (UNVERIFIED in all fields; works in AE since CS5).
  - Multi-dimensional values (Position, Scale, Anchor Point…) show one hot text per dimension separated by commas; Scale shows the constrain-proportions link to its left.
  - Editing a value while the stopwatch is active and CTI is not on a keyframe creates a new keyframe (or, with auto-keyframe off and stopwatch off, changes the static value).
  - **Enable Auto-keyframe (启用自动关键帧)** (Timeline panel menu, default off): editing any interpolable property activates its stopwatch and adds a keyframe (not for popups, checkboxes or Source Text).
  - **Reset (重置)**: property context menu > Reset sets the default value (adds a keyframe at the CTI if animated; project-comp-layers.md, Reset property and group). Group reset: the group's Reset link/context menu resets every child. Keyboard resets for transforms (e.g. reset Rotation, Scale): ui-b shortcuts.
  - **Edit Value… (编辑值…)**: context menu opens the type-specific dialog (transform-keyframes-expressions.md, Edit Value dialogs for transform properties).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim, bae-model)
- **Notes:** Proportional Scrubbing (AE 25.x, 2025; UNVERIFIED version): when several properties of different layers are selected, scrubbing one scales all proportionally rather than adding the same delta (help page "Proportional Scrubbing in timeline"). BAE decision: implement absolute/relative modes first, proportional later (P2).

### AE-UI-071 · Keyframe icons and indices (关键帧图标与关键帧索引)

- **What it does:** In layer bar mode each keyframe is drawn as an icon whose shape encodes its incoming and outgoing temporal interpolation; alternatively keyframes are drawn as numbers.
- **Exact semantics:**
  - The icon is split vertically at the keyframe time: the **left half** shows the incoming interpolation, the **right half** the outgoing interpolation. A half for which there is no adjacent keyframe (in-side of the first key, out-side of the last key) or whose segment is overridden by a preceding Hold is drawn **dark grey** (filled darker) instead of the normal fill.
  - Shapes (BAE draws its own icons; dimensions relative to an icon box of size `s × s`, centred on the keyframe time, `s ≈ 9–11 px` at 100 % UI scale, UNVERIFIED size):
    ```
    half shape by interpolation type (right half; mirror for left half):
      Linear            -> half diamond: triangle (0,-s/2) (s/2,0) (0,s/2)
      Bezier / Continuous Bezier / Easy Ease (temporal Bezier, non-auto)
                        -> half hourglass: polygon (0,0) (s/2,-s/2) (s/2,s/2)
                           i.e. the full icon is an upright hourglass/bow-tie whose waist is at the centre
                           (UNVERIFIED: AE's Bezier half has a narrow waist rather than a point)
      Auto Bezier       -> half circle of radius s/2
      Hold (outgoing)   -> half square: rect x in [0, s/2], y in [-s/2, s/2]
                           (in-half after a hold key also shows the square half) (UNVERIFIED geometry)
    full icons: Linear/Linear = diamond; Ease/Ease = hourglass; AutoBez/AutoBez = circle;
                Hold/Hold = square; mixed = the two halves combined (e.g. Easy Ease In = left
                hourglass half + right diamond half).
    ```
  - Roving keyframes (spatial properties with Rove Across Time) are drawn as small dots instead of a full icon (UNVERIFIED size: about half the normal icon).
  - Colours: unselected = light grey fill; selected = highlighted with a blue outline/fill (theme highlight); keyframe colour labels (AE 22.6+) fill the icon with the label colour (transform-keyframes-expressions.md, Keyframe colour labels).
  - Spatial interpolation is not reflected in layer bar icons (only on the motion path).
  - **Use Keyframe Indices (使用关键帧索引)** (Timeline panel menu; alternative to **Use Keyframe Icons (使用关键帧图标)**, default icons): each keyframe is drawn as its 1-based index number in a small box (UNVERIFIED box style); numbers renumber as keys are added/removed.
  - Summary keyframes on collapsed groups: grey dots/small diamonds (AE-UI-066).
  - Graph Editor keyframes use a different style (AE-UI-081).
  - Hover a keyframe: tooltip with time and value (and property name in the Graph Editor).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Interpolation semantics: transform-keyframes-expressions.md (Keyframe data model, Hold interpolation, Easy Ease, Auto Bezier, Roving keyframes). Implementation hint: generate icons as SDF/vector paths so any half combination renders crisply at any UI scale; batch all visible keyframes in one draw.

### AE-UI-072 · Keyframe selection, moving and scaling in layer bar mode (在图层条模式下选择、移动和缩放关键帧)

- **What it does:** Mouse interaction with keyframe icons in the time graph.
- **Exact semantics:**
  | Gesture | Result |
  |---|---|
  | Click a keyframe | selects it (deselects other keys) |
  | Shift-click | toggles that key in the selection |
  | Drag a marquee starting on empty time-graph space | selects keys inside; Shift-marquee toggles (marquee over selected keys deselects them) |
  | Click a property name | selects all keys of that property |
  | Drag a selected key | moves all selected keys by the same time delta (frame-quantised unless keys between frames are allowed); Shift after drag start snaps to CTI, markers, other keys, In/Out |
  | Alt/Option-drag the first or last of ≥ 3 selected keys | scales the selection's timing proportionally around the opposite end (transform-keyframes-expressions.md, Scale keyframe timing by Alt-drag) |
  | Alt/Option+← / → | nudges selected keys one frame earlier/later |
  | Double-click a key | opens the value dialog for that key (transform-keyframes-expressions.md, Keyframe value dialog); for Source Text keys? (UNVERIFIED) |
  | Ctrl-click (Win) / Cmd-click (Mac) a key | cycles interpolation: Linear ↔ Auto Bezier; on a Hold key → Linear? (transform-keyframes-expressions.md, Interpolation toggles by click) |
  | Ctrl+Alt+H / Cmd+Opt+H | Toggle Hold Keyframe |
  | F9 / Shift+F9 / Ctrl+Shift+F9 (Win); F9 / Shift+F9 / Cmd+Shift+F9 (Mac) | Easy Ease / Easy Ease In / Easy Ease Out |
  | Alt/Option-click two keys | Info panel shows the duration between them |
  | Delete / Backspace | deletes selected keys |
  | Right-click a key | keyframe context menu (below) |
  - Keyframe context menu (zh-CN from the Adobe help): value readout (first line, disabled), Edit Value… (编辑值), Label ▸ (标签), Select Keyframe Label Group (选择关键帧标签组), Select Equal Keyframes (选择相同关键帧), Select Previous Keyframes (选择前面的关键帧), Select Following Keyframes (选择跟随关键帧), Toggle Hold Keyframe (切换定格关键帧), Keyframe Interpolation… (关键帧插值), Rove Across Time (漂浮穿梭时间), Keyframe Velocity… (关键帧速度), Keyframe Assistant ▸ (关键帧辅助): Convert Audio to Keyframes, Convert Expression to Keyframes, Create Keyframes from Data, Easy Ease, Easy Ease In, Easy Ease Out, Exponential Scale, RPF Camera Import, Sequence Layers, Time-Reverse Keyframes. Select Previous/Following are disabled when more than one key is selected.
  - Selecting keys also selects their layer; selecting a layer row deselects keys on other layers.
  - Info panel on selection: shows the selected key's time and interpolation (layer bar mode).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Semantics: transform-keyframes-expressions.md, Keyframe selection model, Drag keyframes in time and snapping, Nudge keyframes, Copy and paste keyframes. Keyframe assistants: transform-keyframes-expressions.md and project-comp-layers.md (Sequence Layers).

### AE-UI-073 · Expression field and expression controls in the Timeline (时间轴中的表达式字段与控件)

- **What it does:** When a property has an expression, an extra row under it shows four expression buttons and the editable expression text.
- **Exact semantics:**
  - Add/remove: Alt-click (Win) / Option-click (Mac) the stopwatch, or Animation > Add Expression (添加表达式) (Alt+Shift+= / Opt+Shift+=). Adding inserts the default expression `transform.position`-style self reference (`<group>.<property>` path, e.g. `transform.position`) selected for typing.
  - The property row gets a twirl; the expression sub-row ("Expression: <Property name>" label) contains, left to right:
    | Control (中文) | Behaviour |
    |---|---|
    | Enable Expression (启用表达式) — "=" icon | toggles `expressionEnabled`; highlighted (blue) when enabled; disabled → strike-through/grey "≠" (UNVERIFIED) and the value shows the pre-expression value |
    | Show Post-Expression Graph (显示后表达式图表) | toggles drawing of the post-expression curve in the Graph Editor (AE-UI-077) |
    | Expression pick whip (表达式关联器) | drag onto another property (Timeline, Effect Controls, Composition viewer target) → inserts a reference at the insertion point or replaces the selection (project-comp-layers.md, Property pick whip and expression links) |
    | Expression Language menu (表达式语言菜单) | hierarchical menu of objects/attributes/methods (Global, Vector Math, Random Numbers, Interpolation, Color Conversion, Other Math, JavaScript Math, Comp, Footage, Layer Sub-objects, Layer General, Layer Properties, Layer 3D, Layer Space Transforms, Camera, Light, Effect, Path Property, Property, Key, MarkerKey, Text, …, UNVERIFIED exact list per AE 26); choosing inserts the call template at the insertion point, replacing the selection |
  - **Expression field (表达式字段)**: a multi-line code editor in the time-graph area of the row. Clicking enters edit mode with the entire text selected. Enter/Return inserts a newline; commit with **Enter on the numeric keypad** or a click outside the field; Esc cancels? (UNVERIFIED). The field's height is resizable by dragging its bottom edge.
  - Expression editor features (AE 16.1 (2019)+, toggles in Preferences > Scripting & Expressions, see ui-c): syntax highlighting with configurable colours, line numbers, code folding, autocomplete (Tab/Enter accepts), bracket matching/auto-close, word wrap, font and size (UNVERIFIED list and defaults).
  - Errors: the property shows a yellow warning triangle next to the enable switch; clicking it shows the full error; the Composition/Layer viewer shows the expression error banner (transform-keyframes-expressions.md, Expression errors and fallback value).
  - Property context menu Expression items: Copy Expression Only (仅复制表达式), Reveal Expression Errors (显示表达式错误) (layer context), Convert Expression to Keyframes (Animation > Keyframe Assistant). E E (twice quickly) reveals all properties with expressions.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Expression semantics: transform-keyframes-expressions.md [ani-b]. Implementation hint: use a lightweight embedded code editor widget (egui-based) with incremental JS tokenizer for highlighting; never re-evaluate the expression per keystroke—commit on exit.

### AE-UI-074 · Timeline layer context menu (时间轴图层上下文菜单)

- **What it does:** Right-click on a layer row/bar opens a menu with the most common Layer-menu commands.
- **Exact semantics:**
  - Approximate order (UNVERIFIED exact order and separators for AE 26): New ▸ (新建), Layer Settings… (图层设置), Open Layer (打开图层), Open Layer Source (打开图层源), Reveal in Explorer / Reveal in Finder (在资源管理器中显示 / 在 Finder 中显示), Mask ▸ (蒙版), Mask and Shape Path ▸ (蒙版和形状路径), Quality ▸ (品质), Switches ▸ (开关), Transform ▸ (变换), Time ▸ (时间), Frame Blending ▸ (帧混合), 3D Layer (3D 图层), Guide Layer (参考图层), Environment Layer (环境图层（译名待核）), Markers ▸ (标记), Blending Mode ▸ (混合模式), Layer Styles ▸ (图层样式), Effect ▸ (效果), Keyframe Assistant ▸ (关键帧辅助), Tracking? (UNVERIFIED), Pre-compose… (预合成), Camera ▸ (摄像机), Create ▸ (创建: Create Shapes from Text / Masks from Text / Shapes from Vector Layer / Keyframes from Data), Auto-trace… (自动追踪), Invert Selection (反向选择), Select Children (选择子项), Rename (重命名), Reveal Expression Errors (显示表达式错误).
  - Right-clicking a non-selected layer first selects it (only it); right-clicking inside a multi-selection keeps the selection and the commands apply to all selected layers.
  - Right-click on empty outline space: New ▸ layer submenu, Composition Settings…, Preview?, Switches/Columns? (UNVERIFIED).
  - Right-click on a column header: Columns ▸ (AE-UI-009); Hide This (隐藏此项（译名待核）).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Each entry invokes the same command as the main Layer menu (menus: ui-b). BAE decision: generate context menus from the same command registry as the main menu, filtered by applicability, so items never drift.

### AE-UI-075 · Dragging items into the Timeline (将项目拖入时间轴)

- **What it does:** Drag-and-drop from the Project panel (or the OS) into the Timeline creates layers at a chosen stacking position and time, or replaces a layer source.
- **Exact semantics:**
  - Drop on the **layer outline**: an insertion line between layer rows marks the stacking position; the new layer's In point = CTI, or comp start if Preferences > General > Create Layers at Composition Start Time is on (project-comp-layers.md, New layer placement and new-layer preferences) (UNVERIFIED which applies to drag).
  - Drop in the **time graph**: the layer's In point = the time under the pointer (a vertical guide and Info panel time follow the drag; Shift snaps to CTI/markers/In-Out points).
  - Multiple items dropped together become layers stacked in their selection order, all at the same time (sequence them later with Sequence Layers).
  - **Alt-drag (Win) / Option-drag (Mac)** an item from the Project panel onto an existing layer row → Replace Layer Source (project-comp-layers.md, Replace layer source).
  - Dropping a comp into itself or into a comp it contains is refused (cycle).
  - Dropping files from the OS imports them first (project-comp-layers.md, Drag-and-drop import) then adds layers.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Drop onto the Composition viewer: AE-UI-018. Implementation hint: show the insertion indicator and the would-be bar ghost during drag.

### AE-UI-076 · Audio waveform and audio properties in the Timeline (时间轴中的音频波形)

- **What it does:** Layers with audio expose Audio Levels and a Waveform display in the Timeline.
- **Exact semantics:**
  - Twirl Audio (音频) > Audio Levels (音频电平) (dB, keyframable) and Waveform (波形). L reveals Audio Levels; LL reveals the Waveform.
  - The Waveform row draws the source audio amplitude envelope for each channel (stereo: two waveforms) over the layer's duration in the time graph; the row height is larger than a normal property row (UNVERIFIED); drawn post-Audio Levels? (UNVERIFIED: AE draws the source waveform, not affected by Audio Levels).
  - Waveform drawing respects time stretch and time remap (shows audio as it plays in comp time).
  - Graph Editor: Show Audio Waveforms option (AE-UI-079) overlays the waveform of selected layers behind the graphs.
  - Waveform generation is asynchronous: rows show empty until analysis finishes (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** Audio semantics (levels, Stereo Mixer effect): effects.md (Audio effects). Implementation hint: precompute per-file min/max peak pyramids (e.g. 256-sample buckets, mipmapped) in bae-media and cache them on disk next to the media cache.

## Graph Editor

### AE-UI-077 · Graph Editor layout and mode switch (图表编辑器布局)

- **What it does:** Replaces the layer-bar time graph of a Timeline panel with a 2D graph of property values or speeds over time; the layer outline on the left stays.
- **Exact semantics:**
  - Toggle: Graph Editor comp button in the Timeline (AE-UI-058) or Shift+F3. Per-Timeline-panel UI state, not undoable.
  - Regions: time ruler, work area and navigator at the top (shared with layer bar mode); graph area; vertical value axis labels on the left edge of the graph (current graph type) and grey reference-graph labels on the right (when Show Reference Graph is on); button bar along the bottom (AE-UI-080).
  - Which properties are drawn: Show Properties menu (AE-UI-078). Each property is drawn with one curve per dimension; curves of multi-dimensional properties use red / green / blue for X / Y / Z (colour components in their channel colour); when several properties are shown, each property's curve colour matches the colour of its value hot text in the layer outline (the outline recolours values while in Graph Editor mode).
  - **Graph Editor Set switch (图表编辑器集)**: a small graph icon next to the stopwatch on animated (keyframes or expression) properties; when on, the property is in the "Graph Editor Set" (persisted per property, UNVERIFIED persistence).
  - Expressions: the post-expression curve is drawn (read-only, distinct colour/lighter) when the property's Show Post-Expression Graph switch is on (AE-UI-073).
  - The CTI line and work area shading are drawn across the graph; layer In/Out braces, layer markers and audio waveforms per AE-UI-079 options.
  - Hover tooltips (Show Graph Tool Tips): over a keyframe: layer name, property name, time and value; over a segment: same at the hovered time.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-anim)
- **Notes:** Graph semantics (value/speed definitions, editing math): transform-keyframes-expressions.md, Graph Editor graph types / Value graph editing semantics / Speed graph editing semantics. Implementation hint: sample curves adaptively in screen space (≥ 1 sample per pixel column) from bae-anim's analytic `value(t)`/`speed(t)`; draw as GPU polylines.

### AE-UI-078 · Show Properties menu (选择图表编辑器中显示的属性)

- **What it does:** Chooses which properties appear in the Graph Editor.
- **Exact semantics:**
  | Option (中文) | Default | Behaviour |
  |---|---|---|
  | Show Selected Properties (显示选择的属性) | on | properties selected in the outline |
  | Show Animated Properties (显示动画属性) | on | all animated properties of the selected layers |
  | Show Graph Editor Set (显示图表编辑器集) | off (UNVERIFIED defaults) | properties whose Graph Editor Set switch is on, regardless of selection |
  - Options are independent check items; the union of the enabled sets is shown. With nothing qualifying, the graph area is empty.
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-079 · Graph Type and Options menu (选择图表类型和选项)

- **What it does:** Selects value vs speed graphs and toggles overlays and editing options.
- **Exact semantics:** Menu items in AE order (first three are a radio group):
  | Item (中文) | Default | Behaviour |
  |---|---|---|
  | Auto-Select Graph Type (自动选择图表类型) | selected | speed graph for spatial properties, value graph for others |
  | Edit Value Graph (编辑值图表) | — | value graph for all |
  | Edit Speed Graph (编辑速度图表) | — | speed graph for all |
  | Show Reference Graph (显示参考图表) | on (UNVERIFIED) | other graph type drawn in the background, read-only, with grey value labels on the right |
  | Show Audio Waveforms (显示音频波形) | off (UNVERIFIED) | waveform of each layer that has a property in the Graph Editor |
  | Show Layer In/Out Points (显示图层的入点/出点) | on (UNVERIFIED) | In and Out points of those layers drawn as curly braces `{` `}` |
  | Show Layer Markers (显示图层标记) | on (UNVERIFIED) | layer markers drawn as small triangles |
  | Show Graph Tool Tips (显示图表工具技巧) | on | hover tooltips |
  | Show Expression Editor (显示表达式编辑器) | off (UNVERIFIED) | shows the expression field of the focused property at the bottom of the Graph Editor |
  | Allow Keyframes Between Frames (允许帧之间的关键帧) | off | sub-frame key times (transform-keyframes-expressions.md, Allow Keyframes Between Frames) |
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** These are user preferences persisted across sessions (UNVERIFIED: stored in the preferences file, not the project).

### AE-UI-080 · Graph Editor button bar (图表编辑器底部按钮)

- **What it does:** The row of buttons at the bottom of the Graph Editor.
- **Exact semantics:** Left to right:
  | Button (中文) | Type | Action |
  |---|---|---|
  | Show Properties (选择图表编辑器中显示的属性) | menu | AE-UI-078 |
  | Graph Type and Options (选择图表类型和选项) | menu | AE-UI-079 |
  | Show Transform Box When Multiple Keys Are Selected (当多个关键帧被选中时显示变换框（译名待核）) | toggle, default on | AE-UI-082 |
  | Snap (对齐) | toggle | snapping of dragged keys (AE-UI-081) |
  | Auto-Zoom Graph Height (自动缩放图表高度) | toggle, default on (UNVERIFIED) | vertical scale continuously fits the visible curves; disables manual vertical pan/zoom |
  | Fit Selection to View (使选择适于查看) | button | fits time and value scale to the selected keys |
  | Fit All Graphs to View (使所有图表适于查看) | button | fits all shown graphs |
  | Separate Dimensions (单独尺寸) | toggle | Position only (transform-keyframes-expressions.md, Separate Dimensions of Position) |
  | Edit Selected Keyframes (编辑选定的关键帧（译名待核）) | menu | same as the keyframe context menu (AE-UI-072) |
  | Convert Selected Keyframes to Hold (将选定的关键帧转换为定格) | button | in+out → Hold |
  | Convert Selected Keyframes to Linear (将选定的关键帧转换为线性) | button | in+out → Linear |
  | Convert Selected Keyframes to Auto Bezier (将选定的关键帧转换为自动贝塞尔曲线) | button | in+out → Auto Bezier |
  | Easy Ease (缓动) | button | F9 |
  | Easy Ease In (缓入) | button | Shift+F9 |
  | Easy Ease Out (缓出) | button | Ctrl+Shift+F9 / Cmd+Shift+F9 |
  - Buttons that need a key selection are disabled when none is selected.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Semantics of conversions and eases: transform-keyframes-expressions.md, Interpolation toggles by click and Graph Editor buttons; Easy Ease. zh-CN strings marked are tooltip texts (UNVERIFIED exact wording).

### AE-UI-081 · Keyframe, handle and curve interaction in the Graph Editor (在图表编辑器中编辑关键帧和手柄)

- **What it does:** Direct manipulation of keys, Bezier direction handles and segments in the graph.
- **Exact semantics:**
  - Keyframe drawing: small squares on the curve. Selected = solid yellow; unselected = the curve colour; semi-selected (another key of the same property is selected) = hollow yellow square. Direction handles are drawn for selected keys with Bezier sides: a line from the key to a round handle end (UNVERIFIED colours).
  | Gesture | Result |
  |---|---|
  | Click key / Shift-click / marquee | select / toggle / select inside rectangle |
  | Alt/Option-click a segment between two keys | selects all keys of that property |
  | Drag key | value graph: time + value; speed graph: time only; Shift (after start) constrains to horizontal or vertical (UNVERIFIED) |
  | Drag key while Snap is on | snaps to key values, key times, CTI, In/Out points, markers, work area and comp start/end; an orange line shows the snap target; holding Ctrl/Cmd after the drag starts temporarily toggles snapping |
  | Drag a direction handle | changes speed and influence (transform-keyframes-expressions.md, Value graph / Speed graph editing semantics) |
  | Alt/Option-drag a handle | splits the in/out handles (key becomes Bezier with independent handles, continuity broken) (UNVERIFIED modifier) |
  | Shift-drag a handle (speed graph) | moves the in and out handles of the key together (UNVERIFIED) |
  | Drag a segment (value graph) | moves both adjacent handles together |
  | Ctrl/Cmd-click a key | toggles interpolation (Linear ↔ Auto Bezier; Bezier → Linear) |
  | Pen tool (G) click on a segment | adds a keyframe there; Pen click on a key deletes it? (UNVERIFIED) |
  | Double-click a key | opens the value dialog (UNVERIFIED in Graph Editor) |
  | Right-click a key | keyframe context menu (AE-UI-072) |
  - Info panel while a key/segment is selected: property min and max values and the speed at the CTI.
  - Pan/zoom: Hand tool drag (or Space/middle-button drag); wheel = vertical pan; Shift+wheel = horizontal pan; Alt/Option+wheel = horizontal zoom; Ctrl/Cmd+wheel = vertical zoom; Zoom tool click = zoom in, Alt/Option-click = zoom out; Zoom-tool Alt/Option-drag left/right = horizontal zoom out/in, up/down = vertical zoom in/out. Vertical pan/zoom is disabled while Auto-Zoom Graph Height is on.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** All edits go through bae-motion commands (`MoveKeys`, `SetTemporalEase`, `SetInterpolation`), one undo step per drag.

### AE-UI-082 · Graph Editor transform box UI (图表编辑器变换框)

- **What it does:** When ≥ 2 keys are selected and the Show Transform Box button is on, a free-transform box with 8 handles and a centre anchor point surrounds them.
- **Exact semantics:**
  - Box = bounding rectangle of the selected keys in (time, value-screen) space; 4 corner + 4 edge handles; anchor point at the box centre (movable).
  - Cursor changes: inside = move; over handle = straight double arrow (scale); over anchor = Move Anchor Point cursor.
  - Modifier gestures and their effects are specified in transform-keyframes-expressions.md, Graph Editor transform box (Shift ratio, Ctrl/Cmd about anchor, Alt/Opt single corner, Ctrl+Alt taper, Ctrl+Alt+Shift one side, negative scale reverses). In a speed graph the box moves/scales keys in time only.
  - Not shown in layer bar mode (there Alt-drag scaling is used, AE-UI-072).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** —

### AE-UI-083 · Graph Editor axes and units (图表编辑器坐标轴与单位)

- **What it does:** Labels the vertical axis so values and speeds can be read.
- **Exact semantics:**
  - Horizontal axis: comp time (shared time ruler, Time Display Style).
  - Value graph vertical labels: the property's own units — pixels for Position/Anchor Point, % for Scale/Opacity, degrees for Rotation, the property's display unit for effect sliders; colour channels 0–1 or 0–255 per the project's colour display (UNVERIFIED).
  - Speed graph vertical labels: units per second — px/sec, %/sec, °/sec (spatial speed always px/sec in comp space).
  - Reference graph labels: grey numbers on the right edge.
  - Gridlines at "nice" intervals (1-2-5 series) adapting to zoom (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-ui-motion
- **Notes:** Units definitions: transform-keyframes-expressions.md, Speed and influence units and ranges.

## Effect Controls and Effects & Presets

### AE-UI-084 · Effect Controls panel layout (效果控件面板)

- **What it does:** A viewer listing the effects of one layer, top to bottom in render order, each with its parameters as widgets; the most convenient place to edit effect parameters.
- **Exact semantics:**
  - Viewer panel (AE-UI-007): tab title `效果控件 <layer name>`; the viewer menu in the tab lists layers whose Effect Controls were opened; lock icon locks it to the current layer. F3 opens/closes it for the selected layer (also Ctrl+Shift+T / Cmd+Shift+T, Layer > Effect Controls). Selecting a different layer retargets an unlocked panel. Applying an effect opens/brings it to front.
  - Header line: `<comp name> • <layer name>` (UNVERIFIED separator).
  - Effect header row (per effect), left to right: effect visibility switch (eyeball since AE 26 visual refresh, previously "fx") (AE-UI-087); twirl; effect name (renameable: select + Enter); label swatch (AE 26.x, AE-UI-087); Reset Effect (重置) (text link, an icon since the visual refresh); Animation Presets popup (when enabled in the panel menu, AE-UI-088); About… (关于) (opens the plug-in's about text); informational icons at the right edge (8-bpc-only warning in 16/32-bpc projects, not GPU-accelerated, missing, …) with tooltips.
  - Parameter rows: stopwatch, parameter name, value widget (AE-UI-085); groups (topics) twirl; the Compositing Options group at the end (effects.md, Compositing Options).
  - Keyboard: ↑/↓ select previous/next effect; →/← expand/collapse selected effects; Ctrl+` / Cmd+` expands/collapses all property groups of the selected effects; Ctrl/Cmd-click a twirl expands/collapses recursively; Delete removes selected effects; Ctrl/Cmd+D duplicates.
  - Panel menu: Show Animation Presets (显示动画预设), Show Effect Label Swatch (显示效果标签色板), Use Label Color for Effect Background (使用标签颜色作为效果背景), Collapse All / Expand All? (UNVERIFIED), plus docking items.
  - Empty state: shows nothing when the layer has no effects (UNVERIFIED: no placeholder text).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Effect framework semantics: effects.md, Effect framework (AE-FX-001…026). Effect parameters also appear in the Timeline under Effects (E reveals).

### AE-UI-085 · Effect parameter widgets (效果参数控件)

- **What it does:** Each effect parameter type has a specific editing widget in Effect Controls (a compact hot-text form is used in the Timeline).
- **Exact semantics:**
  | Param type (effects.md item) | Widget in Effect Controls | Interactions |
  |---|---|---|
  | Slider / float slider (AE-FX-008) | hot-text value + twirl revealing a horizontal slider with min/max labels | scrub hot text (Ctrl/Cmd fine, Shift coarse); drag slider within slider range; type any value within valid range; right-click > Edit Value… (AE-UI-086) |
  | Angle (AE-FX-008) | hot text `Nx +D°` + twirl revealing a circular dial | drag on dial sets direction (Shift = 45° steps, UNVERIFIED); scrubbing the hot text crosses revolutions |
  | Checkbox (AE-FX-009) | checkbox with label text | click toggles (hold-only animation) |
  | Popup (AE-FX-009) | dropdown | choose item; keyframes are Hold |
  | Color (AE-FX-010) | swatch + eyedropper | click swatch → colour picker dialog (系统 / Adobe Color Picker per Preferences); eyedropper: click then click a pixel in any viewer (Ctrl/Cmd-click samples a 5×5 average, UNVERIFIED; Alt samples from the original? UNVERIFIED); samples the displayed (post-colour-management?) pixel — see effects.md, Color parameter |
  | Point / 3D Point (AE-FX-011) | `x, y(, z)` hot text + crosshair (effect point) button | click the crosshair button, then click in the Composition/Layer viewer to set the point; effect control point shown in viewer as a ⊕ handle draggable directly (when the effect is selected) |
  | Layer (AE-FX-012) | layer popup ("None" + all comp layers by `N. name`) + second popup Source / Masks / Effects | chooses the layer and whether its source, source+masks or source+masks+effects is used |
  | Button / group / no-value (AE-FX-013) | button, group header, custom UI | — |
  | Custom UI (e.g. Levels histogram, Curves) | effect-drawn area | effect-specific |
  - Every animatable parameter has a stopwatch (same behaviour as AE-UI-070); Alt/Opt-click adds an expression (the expression field then appears only in the Timeline).
  - Right-click a parameter: Reset, Edit Value…, Add Expression? / Copy Expression Only, Reveal in Timeline (在时间轴中显示（译名待核）) (UNVERIFIED list).
  - Value text uses the parameter's display unit (%, °, px, s); pixel-valued parameters show values in layer pixels regardless of preview resolution.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Implementation hint: widgets are generated from bae-effects parameter descriptors (type, ranges, flags, UI hints) so BAE and third-party effects share one renderer; custom UIs via a small widget trait.

### AE-UI-086 · Edit Value dialog for effect slider ranges (编辑值)

- **What it does:** Lets the user change a slider parameter's slider range for this instance (and type an exact value).
- **Exact semantics:**
  | Field (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Value (值) | number | current | any value inside valid range |
  | Slider Range (滑块范围) min / max | number | effect-defined slider min/max | each limited to the valid range |
  - Units popup for some parameters (e.g. pixels / percent of source / percent of composition for size values) (UNVERIFIED).
  - OK commits as one undo step; the new slider range is stored per instance (effects.md, Numeric parameters); keyframes are unaffected.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Transform-property Edit Value dialogs: transform-keyframes-expressions.md, Edit Value dialogs.

### AE-UI-087 · Effect reorder, enable, reset, labels and Compositing Options UI (效果排序、启用、重置、标签和合成选项)

- **What it does:** Effect-level controls in Effect Controls and in the Timeline Effects group.
- **Exact semantics:**
  - **Reorder**: drag an effect header up/down; an insertion line shows the target; drop moves the effect in the stack (render order). Several selected effects move together keeping order. Same gesture works in the Timeline Effects group.
  - **Enable switch (fx / eyeball)** (效果开关): per-effect on/off (effects.md, Effect switch and layer Effects switch). Alt/Option-click? (UNVERIFIED: no solo-effect modifier in AE).
  - **Reset Effect (重置效果)**: resets all parameters to defaults (if animated, sets keys at the CTI) (effects.md, Reset effect).
  - **Effect colour labels** (latest AE, 26.x, UNVERIFIED version): grey swatch = no label; click → label picker (16 label colours + None); with Use Label Color for Effect Background on, the effect row is tinted with the label colour.
  - **Compositing Options (合成选项)**: last group of every pixel effect: Effect Opacity slider and `+` / `−` buttons that add/remove Mask Reference popups (effects.md, Compositing Options: Effect Opacity / Mask References).
  - Missing effects show "Missing: <name>" with a warning icon (effects.md, Missing effects).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** —

### AE-UI-088 · Animation Presets menu in Effect Controls (效果控件中的动画预设菜单)

- **What it does:** Per-effect popup that applies saved presets of that same effect, or saves the current settings.
- **Exact semantics:**
  - Visible only when panel menu > Show Animation Presets is on (default on, UNVERIFIED).
  - Items: None (无), Save Selection as Animation Preset… (将选择项另存为动画预设（译名待核）), separator, then every .ffx found in the Presets folders that contains only this effect (UNVERIFIED filter), shown by preset name.
  - Choosing a preset sets this effect instance's parameters (and keyframes, offset to the CTI) from the preset instead of adding a new effect.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Preset file semantics: effects.md, Animation Presets (.ffx).

### AE-UI-089 · Effects & Presets panel (效果和预设面板)

- **What it does:** Browsable, searchable tree of all effects (by category) and animation presets.
- **Exact semantics:**
  - Layout: search field at top (AE-UI-010); new-preset button? (UNVERIFIED); tree below: `* Animation Presets` (动画预设) root folder (preset subfolders) followed by effect categories (3D Channel, Audio, Blur & Sharpen, Channel, …, Utility; full list effects.md, Effect categories and the Effect menu), each expandable.
  - Item icons encode type and bit depth: an effect icon with a number 8 / 16 / 32 for the maximum supported bpc; preset icon; GPU-accelerated badge (UNVERIFIED icon set; BAE draws its own).
  - Search: case-insensitive substring ("contains") match against effect and preset names at any position (e.g. "blur" finds Gaussian Blur, Fast Box Blur, Camera Lens Blur…); results keep their category folders, all expanded; empty folders hidden; clearing restores the previous expansion (UNVERIFIED). Spaces: words must all match? (UNVERIFIED: AE treats the whole string as one substring).
  - Panel menu (zh-CN from Adobe help): Save Animation Preset… (另存为动画预设), Browse Presets… (浏览预设) (opens Bridge; BAE: open a preset browser or the OS folder), Show Effects for All Color Depths (显示所有颜色深度的效果) (off → only effects supporting the project bit depth are listed), Show GPU Capable Effects Only (仅显示 GPU 支持的效果), Show Effects (显示效果), Show Animation Presets (显示动画预设), Manage Effects… (管理效果) (enable/disable individual third-party effects), Categories (类别) / Explorer Folders – Finder Folders (资源管理器文件夹 / Finder 文件夹) / Alphabetical (按字母顺序) (radio), Reveal in Explorer / Finder, Refresh List (刷新列表).
  - Presets load only while the panel is shown (AE behaviour); folders whose names are wrapped in parentheses are ignored.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Effect bit-depth and GPU flags: effects.md, Effect bit-depth support / GPU acceleration flag. BAE decision: all BAE built-ins are 32-bpc GPU effects, so the depth/GPU filters mainly matter for plug-ins.

### AE-UI-090 · Applying effects and presets from the panel (从面板应用效果和预设)

- **What it does:** Double-click or drag-and-drop applies effects/presets to layers.
- **Exact semantics:**
  | Gesture | Result |
  |---|---|
  | Double-click an effect with layers selected | applies it to every selected layer (appended at the bottom of each stack, or below the selected effect in Effect Controls, UNVERIFIED) |
  | Double-click an effect with no layer selected | nothing |
  | Double-click a preset with no layer selected | creates a new layer (e.g. text/shape/solid as the preset requires) and applies the preset |
  | Drag effect/preset onto a layer in the Timeline, Composition viewer (Info panel shows the target layer name) or Effect Controls | applies to that layer only; in Effect Controls the drop position sets the stack position |
  | Drag onto empty Composition viewer / empty Timeline area | preset: creates a new layer; effect: nothing (UNVERIFIED) |
  - Presets are applied at the CTI (keyframes offset so the preset's first key lands at the CTI).
  - Most recent effect: Ctrl+Alt+Shift+E / Cmd+Opt+Shift+E; most recent preset: Ctrl+Alt+Shift+F / Cmd+Opt+Shift+F; Animation > Recent Animation Presets (最近的动画预设).
  - Each application is one undo step ("Add Effect"/"Apply Preset").
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Apply semantics, paste-replace behaviour for single instances: effects.md, Apply, duplicate, copy/paste and delete effects.

## Character and Paragraph panels

### AE-UI-091 · Character panel (字符面板)

- **What it does:** Shows and edits character-level TextDocument attributes of the current text selection, selected text layers, or the defaults for new text.
- **Exact semantics:**
  - Layout (top to bottom; each control maps to a text.md item):
    | Control (中文) | Widget | Maps to (text.md) |
    |---|---|---|
    | Font family (设置字体系列) | searchable dropdown with font previews | Font selection fields (AE-TXT-013); menu details AE-UI-094 |
    | Font style (设置字体样式) | dropdown | Font selection fields |
    | Fill / Stroke colour boxes (填充颜色 / 描边颜色) with Swap Fill and Stroke (互换填充和描边（译名待核）), Set to Black / Set to White (设置为黑色 / 设置为白色), No Fill / No Stroke (无填充颜色 / 无描边颜色), eyedropper (吸管) | overlapping swatches (front one is active); click swatch = colour picker; click the back swatch = bring forward | Fill (AE-TXT-015), Stroke (AE-TXT-016) |
    | Font size (设置字体大小) | hot text + preset dropdown | AE-TXT-014 (px) |
    | Leading (设置行距) | hot text + dropdown incl. Auto (自动) | AE-TXT-020 |
    | Kerning (设置两个字符之间的字偶间距) | hot text + dropdown: Metrics (度量标准), Optical (视觉), 0, numbers | AE-TXT-019; manual value only with an insertion point between two characters (disabled for a range) |
    | Tracking (设置所选字符的字符间距) | hot text + dropdown | AE-TXT-018 (1/1000 em) |
    | Stroke width (描边宽度) | hot text + dropdown (px) | AE-TXT-016 |
    | Stroke position (描边位置（译名待核）) | dropdown: Fill Over Stroke (在描边上填充), Stroke Over Fill (在填充上描边), All Fills Over All Strokes (全部填充在全部描边之上), All Strokes Over All Fills (全部描边在全部填充之上) | Fill and stroke paint order (AE-TXT-017) |
    | Vertical Scale (垂直缩放) / Horizontal Scale (水平缩放) | hot text % | AE-TXT-022 |
    | Baseline Shift (设置基线偏移) | hot text px | AE-TXT-021 |
    | Tsume (比例间距) | hot text % | AE-TXT-026 |
    | Faux Bold (仿粗体), Faux Italic (仿斜体), All Caps (全部大写字母), Small Caps (小型大写字母), Superscript (上标), Subscript (下标) | toggle buttons (All Caps / Small Caps exclusive; Superscript / Subscript exclusive) | AE-TXT-023…025 |
    | Ligatures (连字), Hindi Digits (印地语数字) (UNVERIFIED presence per AE version) | checkboxes | AE-TXT-027 |
  - Mixed values across the target show an empty field (or "—"); editing sets all targets to the new value.
  - Hot text scrubbing behaves as AE-UI-012 (Ctrl/Cmd fine, Shift coarse); with a range selected, Alt+←/→ (Opt) changes kerning/tracking by 20 units (Ctrl/Cmd+Alt: 100) (keyboard: ui-b).
  - Eyedropper samples any pixel on screen (inside AE viewers: the displayed colour) and sets the front swatch.
  - Panel menu (UNVERIFIED order): Show Font Names in English (以英文显示字体名称), Reset Character (重置字符), Faux Bold/Italic, All Caps, Small Caps, Superscript, Subscript, Ligatures, No Break (无间断), Line Join ▸ Miter/Round/Bevel (线段连接 ▸ 斜接/圆角/斜面), Standard Vertical Roman Alignment (标准垂直罗马对齐方式), Tate-Chu-Yoko (直排内横排), Fractional Widths? (UNVERIFIED), Show Font Preview? (UNVERIFIED).
  - Opening: Window > Character (Ctrl+6 / Cmd+6); Auto-Open Panels in the Tools panel opens Character and Paragraph when a type tool becomes active.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-text)
- **Notes:** Field semantics, defaults, ranges and scripting names: text.md (TextDocument section). Implementation hint: the panel reads a merged "style summary" of the target and writes one SetTextStyle command per edit.

### AE-UI-092 · Paragraph panel (段落面板)

- **What it does:** Edits paragraph-level attributes of the paragraphs touched by the selection, of selected text layers, or the defaults for new text.
- **Exact semantics:**
  | Control (中文) | Widget | Maps to (text.md) |
  |---|---|---|
  | Left Align Text (居左对齐文本), Center Text (居中对齐文本), Right Align Text (居右对齐文本), Justify Last Line Left (最后一行左对齐), Justify Last Line Centered (最后一行居中对齐), Justify Last Line Right (最后一行右对齐), Justify All (两端对齐) | 7 radio buttons (justify variants are enabled only for paragraph text; vertical text shows top/centre/bottom variants) | AE-TXT-028 |
  | Left-to-Right / Right-to-Left text direction (从左到右 / 从右到左文本方向（译名待核）) | 2 radio buttons (AE 22.x+ with the South Asian / Middle Eastern composer) | AE-TXT-033 |
  | Indent Left Margin (缩进左边距), Indent Right Margin (缩进右边距), Indent First Line (首行缩进) | hot text px | AE-TXT-029 |
  | Add Space Before Paragraph (段前添加空格), Add Space After Paragraph (段后添加空格) | hot text px | AE-TXT-030 |
  - Target: if the insertion point is in a paragraph or text is selected, only the paragraphs at least partially selected; else all text of the selected text layers (and their selected Source Text keyframes); else the defaults for the next text entry.
  - Panel menu: Roman Hanging Punctuation (罗马式溢出标点), Top-to-Top Leading (顶到顶行距) / Bottom-to-Bottom Leading (底到底行距), Adobe Single-line Composer (Adobe 单行书写器) / Adobe Every-line Composer (Adobe 逐行书写器) (and Middle Eastern & South Asian variants), Hyphenation? (UNVERIFIED), Reset Paragraph (重置段落).
  - Window > Paragraph (Ctrl+7 / Cmd+7).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-text)
- **Notes:** Semantics: text.md, Paragraph alignment / indents / space / composer / leading type. zh-CN button names are tooltip strings (UNVERIFIED wording).

### AE-UI-093 · Target resolution for Character and Paragraph edits (字符和段落编辑的作用目标)

- **What it does:** Defines what an edit in the Character/Paragraph panels changes.
- **Exact semantics:**
  ```
  if a text layer is in text-edit mode:
      if a character range is selected:   apply character attrs to that range;
                                          paragraph attrs to every paragraph touching the range
      else (insertion point only):        character attrs become the "typing style" at the caret
                                          (affect only text typed next); paragraph attrs apply to
                                          the caret's paragraph
  else if text layers are selected:       apply to ALL characters/paragraphs of every selected text layer;
                                          if the layer has Source Text keyframes and some are selected,
                                          apply to those keyframes only (UNVERIFIED: otherwise to the
                                          keyframe at the CTI / creates one when the stopwatch is on)
  else:                                   store as defaults for the next new text layer (persisted per user)
  ```
  - With the Source Text stopwatch on and no Source Text key selected, a change at a CTI without a key creates a Source Text key (hold) at the CTI (text.md, Source Text keyframes).
  - Multiple layers: one undo step for all.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-text, bae-model)
- **Notes:** —

### AE-UI-094 · Font menu, favourites and missing fonts in the UI (字体菜单、收藏与缺失字体)

- **What it does:** The font family dropdown is a searchable list with previews, recent fonts, favourites and filters; missing fonts are shown distinctly.
- **Exact semantics:**
  - Typing in the closed field filters/jumps to matching family names; ↑/↓ or the mouse wheel over the field steps to the previous/next family and applies it live; Enter/Return accepts, Esc cancels.
  - Open menu: recent fonts at the top (count = Preferences > Type > Number of Recent Fonts to Display, UNVERIFIED default 10), then all families alphabetically; each row shows the family name and a preview ("Sample" in that font, size per Preferences > Type > Preview Size: Small / Medium / Large, UNVERIFIED default); a star toggles favourite; filter buttons at the top: Show favourites only (star), Show fonts from Adobe (Adobe Fonts), and a font classification filter? (UNVERIFIED).
  - Families with several styles open a submenu/expander of styles (UNVERIFIED in AE 26: the style is chosen in the separate style dropdown).
  - Missing font: the family name is shown in square brackets `[FontName]` and the text renders with a substitute (text.md, Missing fonts); Resolve Fonts… (解析字体) dialog from File > Resolve Fonts? (UNVERIFIED menu location).
  - Duplicate installed copies show an abbreviation suffix in parentheses (e.g. `(T1)`, `(TT)`, `(OTF)`).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-text)
- **Notes:** Font enumeration/matching: text.md, Font enumeration / Font matching on project open / Missing fonts. Implementation hint: render previews lazily (only visible rows) with a glyph cache; enumerate fonts on a background thread at startup.

## Align, Info, Audio and Preview panels

### AE-UI-095 · Align panel (对齐面板)

- **What it does:** Aligns or evenly distributes the selected layers horizontally or vertically, relative to the selection or to the composition frame.
- **Exact semantics:**
  - Controls: **Align Layers To (将图层对齐到)** popup: Selection (选区) | Composition (合成) (default Composition, UNVERIFIED); **Align Layers (对齐图层)**: 6 buttons — Left, Horizontal Center, Right, Top, Vertical Center, Bottom; **Distribute Layers (分布图层)**: 6 buttons — Top, Vertical Center, Bottom, Left, Horizontal Center, Right (UNVERIFIED button order).
  - Enable rules: align needs ≥ 2 layers (Selection) or ≥ 1 layer (Composition); distribute needs ≥ 3 layers. Locked layers are not moved. Text characters are not aligned within a layer.
  - Math (per layer `L` at the CTI):
    ```
    box(L) = axis-aligned bounding box in comp space of L's layer bounds
             (source rect / sourceRectAtTime for text & shape, see compositing-3d-color.md, Layer bounds)
             transformed by L's full layer-to-comp matrix (incl. parents)
    target (Selection):   Left = min(box.x0), Right = max(box.x1), HCenter = (min x0 + max x1)/2,
                          Top = min(box.y0), Bottom = max(box.y1), VCenter = (min y0 + max y1)/2
                          i.e. align to the extreme layer edge in that direction (help: "the layer most
                          closely representing the new alignment")  (UNVERIFIED for centre: AE uses the
                          centre of the selection's union box)
    target (Composition): Left = 0, Right = comp.width, HCenter = comp.width/2, Top = 0, ...
    align:      dx = target - edge(box(L)); move L by dx (comp space)
    distribute: sort layers by edge e (e.g. top edge); e_first, e_last stay;
                for layer i (0..n-1): e_i' = e_first + (e_last - e_first) * i/(n-1); move by e_i' - e_i
    ```
  - Moving a layer = change Position by the comp-space delta converted into the parent's space (parented layers) — if Position is animated, a keyframe is set/changed at the CTI; with Separate Dimensions the X/Y Position properties change.
  - 3D layers: AE aligns using the layers' 2D comp-space projection? (UNVERIFIED: help section is titled "in 2D space"; buttons may be disabled for 3D layers).
  - One undo step ("Align Layers").
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-motion, bae-anim)
- **Notes:** Also see Layer > Transform > Center In View (Ctrl/Cmd+Home) and Center Anchor Point in Layer Content (project-comp-layers.md, Fit to Comp and centring commands). BAE decision: put the align/distribute command in bae-motion so scripts and agents can use it.

### AE-UI-096 · Info panel (信息面板)

- **What it does:** Shows the colour and position under the pointer in any viewer, plus context information and transient messages about the current operation.
- **Exact semantics:**
  - Upper area: left = R, G, B, A of the pixel under the pointer (with a colour swatch); right = X, Y of the pointer in the viewer's coordinate system (comp pixels in the Composition panel, layer pixels in the Layer panel, footage pixels in the Footage panel); outside the frame values remain from the last position (UNVERIFIED).
  - Colour value display (panel menu, radio) (UNVERIFIED exact strings/order): Auto Color Display (自动颜色显示) — follows the project bit depth (8 → 0–255, 16 → 0–32768, 32 → decimal); 8-bpc (0-255); 10-bpc (0-1023); 16-bpc (0-32768); Decimal (0.0-1.0) (32-bpc values may exceed 1.0 or be negative); Percent (百分比); Web (hex `#RRGGBB`); HSB (H in degrees, S/B in %). Without Auto, the panel uses the chosen format regardless of project depth.
  - Values reported are the composited comp pixel at the current preview resolution in working-space values (not display-transformed) (UNVERIFIED; colour management: compositing-3d-color.md).
  - Lower area (context text), e.g. selected layer: name, Duration, In, Out; while dragging layers: In/Out/Δ; dragging keyframes/markers: time; Alt-click two keys/markers: duration between; selected keyframe: time and interpolation; Graph Editor selection: min/max value and speed; dragging an effect onto the Composition viewer: target layer name; time navigator click: its start/end; during preview: playback fps (green if real-time, red if slower; e.g. `Real-time` / `Non-real-time 12.4 fps`, UNVERIFIED wording) and cached-frame counts; during render: "Rendering..." progress.
  - Window > Info (Ctrl+2 / Cmd+2).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render, bae-color)
- **Notes:** Implementation hint: read back the single pixel under the pointer from the last displayed GPU frame (1×1 copy, async map) so hover never triggers a re-render.

### AE-UI-097 · Audio panel (音频面板)

- **What it does:** Shows audio level meters during audio playback and provides level sliders that edit the selected layer's Audio Levels.
- **Exact semantics:**
  - Two vertical VU meters (left/right channel) with peak/clip indicators at the top (red when clipping above 0 dBFS; click to reset, UNVERIFIED), a dB scale between them, and two level sliders (L, R) with linked dragging (drag between them moves both; dragging one moves only that channel, UNVERIFIED).
  - Level sliders edit the selected audio layer's Audio Levels (音频电平) property at the CTI (keyframe if animated); numeric fields at the bottom show the slider values.
  - Panel menu > Audio Options… (音频选项): Units (单位): Decibels (分贝) | Percentage (百分比); Slider Minimum (滑块最小值): -48 dB | -96 dB | -144 dB (UNVERIFIED values, default -48 dB). Slider top is +12 dB (UNVERIFIED).
  - Meters update only during previews that include audio and while scrubbing audio.
  - Window > Audio (Ctrl+4 / Cmd+4).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** Audio Levels semantics: project-comp-layers.md (Audio-only layer) and effects.md (Audio effects).

### AE-UI-098 · Preview panel layout and controls (预览面板)

- **What it does:** Transport buttons plus per-shortcut preview settings; behaviour is specified in the preview & cache items (ui-b).
- **Exact semantics:**
  - Transport row: First Frame (第一帧), Previous Frame (上一帧), Play/Stop (播放/停止), Next Frame (下一帧), Last Frame (最后一帧) buttons (UNVERIFIED exact names); Play/Stop uses the settings of the shortcut currently shown in the Shortcut menu.
  - Settings (per shortcut; the panel shows the set of the selected shortcut):
    | Control (中文) | Type | Notes |
    |---|---|---|
    | Shortcut (快捷键) | popup: Spacebar, Shift+Spacebar, Numpad 0, Shift+Numpad 0, Alt/Option+Numpad 0 (UNVERIFIED: Numpad − also listed in some help text) | selects which settings set is edited |
    | Reset (重置) | button | restores defaults for all shortcuts; Alt/Option-click restores CC 2014-like behaviours (Cache Before Playback on for Numpad 0) |
    | Mute Audio (静音音频（译名待核）) | toggle | |
    | Include Video (包括视频), Include Audio (包括音频), Include Overlays and Layer Controls (包括叠加和图层控件) | 3 toggles | at least one must stay on (turning off the last one turns another on) |
    | Loop (循环) | toggle | |
    | Cache Before Playback (播放前缓存) | checkbox | off by default for all shortcuts |
    | Range (范围) | popup: Work Area, Work Area Extended By Current Time, Entire Duration, Play Around Current Time (with pre/post-roll dialog) | |
    | Play From (播放自（译名待核）) | popup: Start of Range, Current Time | |
    | Frame Rate (帧速率) | popup: Auto or a value | |
    | Skip (跳过) | popup: 0–N frames | |
    | Resolution (分辨率) | popup: Auto, Full, Half, Third, Quarter, Custom | overrides the viewer resolution during preview |
    | Full Screen (全屏) | checkbox | |
    | On (Spacebar) Stop: If caching, play cached frames (如果缓存，则播放缓存的帧); Move time to preview time (将时间移到预览时间) | checkboxes | |
  - Defaults per shortcut are listed in ui-b (preview item).
  - Window > Preview (Ctrl+3 / Cmd+3).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** Semantics: ui-preview-render-io.md [ui-b], Preview and cache behaviour; preview resolution: compositing-3d-color.md.

## Tools panel

### AE-UI-099 · Tools panel layout and tool groups (工具面板)

- **What it does:** The toolbar (docked horizontally above the panels by default) holding all tools, grouped into flyouts, plus the tool options area, workspace switcher and help search.
- **Exact semantics:**
  - Left-to-right groups (default shortcut; flyout members in menu order; behaviours in ui-b, Tools):
    | Group | Members (中文) | Shortcut |
    |---|---|---|
    | Home (主页) | opens the Home screen (AE-UI-013) | — |
    | Selection (选取工具) | — | V |
    | Hand (手形工具) | — | H (Space = momentary) |
    | Zoom (缩放工具) | — | Z |
    | 3D camera navigation (摄像机导航工具（译名待核）) | Orbit Around Cursor / Scene / Camera POI; Pan Under Cursor / Pan Camera POI; Dolly to Cursor / Scene / Camera POI (AE 22.x+; older: Unified, Orbit, Track XY, Track Z camera tools) | 1, 2, 3 cycle within orbit / pan / dolly (UNVERIFIED keys); C cycles camera tools |
    | Rotation (旋转工具) | — | W |
    | Pan Behind (Anchor Point) (向后平移（锚点）工具) | — | Y |
    | Shape tools | Rectangle (矩形工具), Rounded Rectangle (圆角矩形工具), Ellipse (椭圆工具), Polygon (多边形工具), Star (星形工具) | Q (cycles) |
    | 3D primitives (AE 26, UNVERIFIED grouping) | Cube (立方体工具), … | — |
    | Pen tools | Pen (钢笔工具), Add Vertex (添加"顶点"工具), Delete Vertex (删除"顶点"工具), Convert Vertex (转换"顶点"工具), Mask Feather (蒙版羽化工具) | G (cycles) |
    | Type tools | Horizontal Type (横排文字工具), Vertical Type (直排文字工具) | Ctrl+T / Cmd+T (cycles) |
    | Paint tools | Brush (画笔工具), Clone Stamp (仿制图章工具), Eraser (橡皮擦工具) | Ctrl+B / Cmd+B (cycles) |
    | Roto tools | Roto Brush (Roto 笔刷工具), Refine Edge (调整边缘工具) | Alt+W / Opt+W (cycles) |
    | Puppet tools | Puppet Position Pin (人偶位置控点工具), Puppet Starch Pin (人偶固化控点工具), Puppet Bend Pin (人偶弯曲控点工具), Puppet Advanced Pin (人偶高级控点工具), Puppet Overlap Pin (人偶重叠控点工具) | Ctrl+P / Cmd+P (cycles) |
  - Flyouts: a group shows its last used member; a small corner triangle marks a flyout; press-and-hold or right-click? (UNVERIFIED) opens the member list; Alt/Option-click the group button cycles members; pressing the shortcut repeatedly cycles.
  - Tools that need a target that is absent are still selectable (e.g. Puppet without a layer) but do nothing on click (UNVERIFIED).
  - Right part of the toolbar: tool options area (AE-UI-100), Snapping toggle (对齐), workspace switcher (AE-UI-005, `>>` overflow), Search Help field (搜索帮助) (opens help results; BAE: search commands and docs).
  - Toolbar can be undocked/redocked as a panel (Window > Tools, Ctrl+1 / Cmd+1); in narrow widths groups collapse into flyouts.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** Tool behaviours, modifiers and shortcuts: ui-preview-render-io.md [ui-b], Tools. zh-CN tool names from the BAE glossary where listed; others (译名待核) are best effort — notably 调整边缘工具 vs 优化边缘工具 (ui-b uses 优化边缘工具（译名待核）). Never reuse Adobe tool icons.

### AE-UI-100 · Tool options area (工具选项)

- **What it does:** Context-sensitive controls to the right of the tools that change with the active tool.
- **Exact semantics:**
  | Active tool | Options shown (中文) |
  |---|---|
  | Selection, Rotation, camera tools (with 3D layers) | axis mode buttons Local / World / View Axis Mode (本地轴模式 / 世界轴模式 / 视图轴模式) (ui-b, Axis modes); Rotation tool adds Set: Orientation / Rotation (设置: 方向 / 旋转) popup for 3D layers |
  | Selection | Snapping (对齐) checkbox + snapping options menu (snap to edges, centres, corners, mask/shape points, 3D… — ui-b) |
  | Shape tools | Tool Creates Shape / Tool Creates Mask (工具创建形状 / 工具创建蒙版) toggle (only with a shape layer selected), Fill (填充) swatch + fill-type options (None, Solid, Linear, Radial) with blend mode and opacity, Stroke (描边) swatch + stroke width (px), Add (添加) ▸ menu (shape contents), Bezier Path (贝塞尔曲线路径) checkbox |
  | Pen | Tool Creates Shape / Mask toggle, Fill, Stroke, width, Add ▸, RotoBezier (RotoBezier) checkbox |
  | Type tools | Auto-Open Panels (自动打开面板) — opens Character and Paragraph |
  | Paint tools | Auto-Open Panels — opens Paint and Brushes |
  | Puppet tools | Mesh: Show (显示), Expansion (扩展), Density (密度); Record Options (记录选项) for Position pin sketching (UNVERIFIED list) |
  | Roto Brush / Refine Edge | brush size readout? (UNVERIFIED) |
  | Camera navigation | Set default / Reset? (UNVERIFIED) |
  - Fill/Stroke swatches: click = colour picker or gradient editor (shape: Gradient Editor dialog); Alt/Option-click the swatch cycles fill type (None → Solid → Linear → Radial) (UNVERIFIED); the fill/stroke labels open the Fill Options / Stroke Options dialogs (fill type buttons, blending mode, opacity, Preview checkbox).
  - Values set here become defaults for new shapes (persisted per user); changing them with shape layers selected also edits those layers' selected fills/strokes (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-ui-motion
- **Notes:** Shape fill/stroke semantics: shapes.md; Puppet: effects.md (Distort, Puppet) if covered there.

## Secondary panels

### AE-UI-101 · Properties panel (属性面板)

- **What it does:** A contextual inspector that shows the most relevant properties of the selected layer(s) — Layer Transform, and per type Text (character/paragraph), Shape (Layer Contents: fills, strokes, paths of the first shape group), Essential Properties of precomps — without twirling the Timeline.
- **Exact semantics:**
  - Tab title `属性: <layer name>`; with several layers selected, shows shared properties and edits apply to all (UNVERIFIED for mixed types).
  - Sections (twirlable): Layer Transform (图层变换): Anchor Point, Position, Scale (with constrain link), Rotation, Opacity, each with a stopwatch/keyframe navigator; Text: font, style, size, fill/stroke, tracking, leading, paragraph alignment (same semantics as AE-UI-091/092); Layer Contents (图层内容) / Shape Properties (形状属性) / Shape Transform (形状变换): Fill Color, Stroke Color/Width, Roundness, Skew, Skew Axis…; Essential Properties for precomp layers (AE 23.6 (Aug 2023)).
  - With nothing selected: shows composition info/settings shortcuts (UNVERIFIED).
  - Edits are ordinary property edits (keyframe rules as AE-UI-070).
- **Priority:** P1 (newer panel; not essential to AE users' core workflows)
- **Owner:** bae-ui-motion
- **Notes:** Introduced in AE beta Dec 2021 (22.2 beta); shipped in the release line around AE 23.x–24.x (UNVERIFIED exact version). Glossary strings: 属性, 图层变换, 图层内容, 形状属性, 形状变换, 填充颜色, 圆度, 倾斜, 倾斜轴, 重置. BAE decision: this panel is a natural place for agent-friendly "simple mode" editing.

### AE-UI-102 · Tracker panel (跟踪器面板)

- **What it does:** Entry point for motion tracking: starts the 3D camera tracker, Warp Stabilizer, and the point tracker (track/stabilize motion), and drives the point tracker's analysis and application.
- **Exact semantics:**
  - Buttons: Track Camera (跟踪摄像机) (applies 3D Camera Tracker effect), Warp Stabilizer (变形稳定器) (applies the effect), Track Motion (跟踪运动), Stabilize Motion (稳定运动) (create a tracker on the selected layer and open the Layer panel).
  - Point-tracker controls: Motion Source (运动源) popup (layers with video); Current Track (当前跟踪) popup; Track Type (跟踪类型): Stabilize (稳定), Transform (变换), Parallel Corner Pin (平行边角定位), Perspective Corner Pin (透视边角定位), Raw (原始); checkboxes Position (位置), Rotation (旋转), Scale (缩放); Motion Target (运动目标) + Edit Target… (编辑目标); Options… (选项) (Track Name, Tracker Plug-in, Channel: RGB/Luminance/Saturation, Process Before Match: Blur/Enhance, Track Fields, Subpixel Positioning, Adapt Feature On Every Frame, If Confidence is Below N%: Continue Tracking / Stop Tracking / Extrapolate Motion / Adapt Feature — UNVERIFIED list); Analyze (分析): Analyze 1 Frame Backward, Analyze Backward, Analyze Forward, Analyze 1 Frame Forward; Reset (重置); Apply (应用) (with Apply Dimensions X and Y / X only / Y only dialog).
  - Track points in the Layer panel: each has a feature region (inner box), search region (outer box) and attach point (+); dragging inside moves both regions; dragging region edges resizes; Alt/Option-drag moves only the feature region? (UNVERIFIED); while dragging, the feature region area is magnified 400 % (UNVERIFIED).
  - Tracking results are stored as Tracker > Track Point properties (Feature Center, Feature Size, Search Offset, Search Size, Confidence, Attach Point, Attach Point Offset) on the source layer, keyframed per frame.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-effects, bae-motion)
- **Notes:** Warp Stabilizer and 3D Camera Tracker semantics: effects.md (Distort / Perspective). Gap: the point-tracker algorithm (feature matching) is not specified in any sibling file; BAE decision pending (owner bae-effects, implementation hint: normalised cross-correlation on GPU with a coarse-to-fine pyramid, sub-pixel by parabolic fit). Window > Tracker.

### AE-UI-103 · Essential Graphics panel (基本图形面板)

- **What it does:** Authoring panel for Motion Graphics templates (.mogrt): pick a primary comp, drag properties in to expose them, group and format controls, export.
- **Exact semantics:**
  - Controls: Primary composition (主合成) popup, Name field, Solo Supported Properties (仅显示支持的属性（译名待核）), Add Formatting ▸ (Add Group, Add Comment) (添加格式（译名待核）), Edit Properties… (编辑属性), Export Motion Graphics Template… (导出动态图形模板); property list where dragged-in properties appear as controls (slider, checkbox, color, point, angle, dropdown, text, media replacement) with editable names and ranges.
  - Responsive Design – Time and Protected Regions are related comp features (project-comp-layers.md, Protected Region and Responsive Design – Time).
  - Exposed properties also become Essential Properties when the comp is nested (project-comp-layers.md, Essential Properties on precomp layers).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model, bae-package)
- **Notes:** .mogrt is Premiere-specific; BAE decision: support Essential Properties (precomp overrides) and defer .mogrt export.

### AE-UI-104 · Paint and Brushes panels (绘画面板与画笔面板)

- **What it does:** Settings for the Brush, Clone Stamp and Eraser tools.
- **Exact semantics:**
  - **Paint (绘画)**: Opacity (不透明度) %, Flow (流量) %, Mode (模式) (blend mode for the stroke), Channels (通道): RGBA / RGB / Alpha, Duration (时长): Constant (固定), Write On (写入), Single Frame (单帧), Custom (自定义) + frames; Erase (抹除) (Eraser only): Layer Source & Paint / Paint Only / Last Stroke Only; Clone Options (仿制选项) (Clone Stamp only): Preset buttons ×5, Source (源) layer, Aligned (已对齐), Lock Source Time (锁定源时间), Offset (偏移), Source Time Shift (源时间转移), Clone Source Overlay (仿制源叠加) + Difference mode toggle; foreground/background colour swatches with swap and eyedropper.
  - **Brushes (画笔)**: brush-tip preset list (thumbnails), Diameter (直径) px, Angle (角度) °, Roundness (圆度) %, Hardness (硬度) %, Spacing (间距) %, Brush Dynamics (画笔动态): Size / Angle / Roundness / Opacity / Flow each controlled by Off, Pen Pressure, Pen Tilt, Stylus Wheel; Minimum Size; save/delete preset; panel menu view modes (text only, small/large thumbnails…).
  - Values are used for the next stroke (the stroke then stores its own copy in the Paint effect's stroke properties).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects)
- **Notes:** Paint stroke rendering is the Paint effect (effects.md, if covered; otherwise a gap). Ctrl/Cmd-drag in the Layer panel with a paint tool sets brush diameter, release then drag sets hardness (ui-b, Paint tools). (UNVERIFIED zh-CN for several labels.)

### AE-UI-105 · Content-Aware Fill panel (内容识别填充面板)

- **What it does:** Fills transparent (masked-out) areas of a layer over time using surrounding frames; panel configures and generates a fill layer.
- **Exact semantics:**
  | Control (中文) | Type | Default |
  |---|---|---|
  | Alpha Expansion (Alpha 扩展) | int px | 0 (UNVERIFIED) |
  | Fill Method (填充方法): Object (对象), Surface (表面), Edge Blend (边缘混合) | enum | Object |
  | Lighting Correction (光照校正): None, Subtle, Moderate, Strong | enum | None |
  | Range (范围): Work Area (工作区域), Entire Duration (整个持续时间) | enum | Work Area |
  | Create Reference Frame (创建参考帧) | button | opens a Photoshop round trip (BAE: export frame) |
  | Generate Fill Layer (生成填充图层) | button | renders an image-sequence layer above the target |
  - A preview of the fill target region (pink overlay) is drawn in the Composition viewer while the panel is active (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-effects, bae-media)
- **Notes:** AE 16.1 (CC 2019). BAE decision: defer (requires heavy ML/patch-match inpainting).

### AE-UI-106 · Libraries, Learn and Lumetri Scopes panels (库、学习和 Lumetri 范围面板)

- **What it does:** Catalog item for three auxiliary panels: Creative Cloud Libraries (assets sync), Learn (in-app tutorials), Lumetri Scopes (video scopes for the active viewer).
- **Exact semantics:**
  - Libraries (库): cloud asset browser; drag items into the Project panel/comp. BAE: no equivalent cloud; may map to a local asset library.
  - Learn (学习): tutorial list (Learn workspace). BAE: optional onboarding.
  - Lumetri Scopes (Lumetri 范围): Waveform (RGB / Luma / YC), Vectorscope (YUV / HLS), Histogram, Parade (RGB / YUV) for the frame in the active viewer, with clamp/bit-depth options (UNVERIFIED list).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** BAE decision: only Lumetri-style scopes are worth implementing (GPU compute histograms on the displayed frame).

### AE-UI-107 · Motion Sketch panel (动态草图面板)

- **What it does:** Records a layer's Position as you drag in the Composition viewer, in real time over the work area.
- **Exact semantics:**
  | Control (中文) | Type | Default |
  |---|---|---|
  | Capture speed at (捕捉速度为) | % | 100 % |
  | Smoothing (平滑) | number | 1 (UNVERIFIED) |
  | Show: Wireframe (线框图), Background (背景) | checkboxes | Wireframe off, Background off? (UNVERIFIED) |
  | Start: / Duration: | readouts (work area) | — |
  | Start Capture (开始捕捉) | button | arms capture; capture starts at mouse-down in the viewer and ends at mouse-up or work-area end |
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Semantics (sampling, smoothing): transform-keyframes-expressions.md, Motion Sketch. Window > Motion Sketch.

### AE-UI-108 · Smoother panel (平滑器面板)

- **What it does:** Simplifies selected keyframes of a property by curve fitting.
- **Exact semantics:** Controls: Apply To (应用到): Spatial Path (空间路径) | Temporal Graph (时间图表) (auto-selected by property type; Spatial only for spatial properties); Tolerance (容差) (units of the property; default 1, UNVERIFIED); Apply (应用) button (needs ≥ 3 selected keys or the whole property).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Semantics: transform-keyframes-expressions.md, Smoother.

### AE-UI-109 · Wiggler panel (摇摆器面板)

- **What it does:** Adds random deviation keyframes between two or more selected keyframes.
- **Exact semantics:**
  | Control (中文) | Type | Default |
  |---|---|---|
  | Apply To (应用到): Spatial Path (空间路径) / Temporal Graph (时间图表) | enum | per property |
  | Noise Type (杂色类型): Smooth (平滑) / Jagged (锯齿) | enum | Smooth |
  | Dimension (维数): X / Y / (Z) / All the Same (全部相同) / All Independently (全部独立) | enum | All Independently (UNVERIFIED) |
  | Frequency (频率) | per second | 5.0 |
  | Magnitude (数量级) | property units | 1.0 |
  | Apply (应用) | button | — |
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Semantics: transform-keyframes-expressions.md, Wiggler.

### AE-UI-110 · Mask Interpolation panel (蒙版插值面板)

- **What it does:** Generates intermediate mask-path keyframes between two selected mask (or shape path) keyframes with smoother, more natural vertex correspondence than default interpolation.
- **Exact semantics:**
  | Control (中文) | Type | Default (UNVERIFIED all) |
  |---|---|---|
  | Keyframe Rate (关键帧速率) | per second or Auto | Auto |
  | Keyframe Fields (关键帧字段) (doubles rate) | bool | off |
  | Use "Linear" Vertex Paths (使用"线性"顶点路径) | bool | off |
  | Bending Resistance (抗弯强度) | 0–100 | 50 |
  | Quality (品质) | 0–100 | 50 |
  | Add Mask Shape Vertices (添加蒙版路径顶点) + amount + unit: Pixels Between Vertices / Total Vertices / Percentage of Outline | bool + number + enum | on, 1 % of outline? |
  | Matching Method (配合法): Auto, Curve, Polyline | enum | Auto |
  | Use 1:1 Vertex Matches (使用 1:1 顶点匹配) | bool | off |
  | First Vertices Match (第一个顶点匹配) | bool | on |
  | Apply (应用) | button | — |
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-motion)
- **Notes:** Gap: the smart mask interpolation algorithm is not specified in a sibling file (default mask path interpolation is in transform-keyframes-expressions.md, Mask path and shape path interpolation). BAE decision: defer.

### AE-UI-111 · Metadata panel (元数据面板)

- **What it does:** Shows and edits XMP metadata of the project and of the selected footage item, and controls which metadata is written to rendered files.
- **Exact semantics:** Sections Project (项目) and File (文件) with schema groups (Dublin Core, Basic, Rights, Dynamic Media…); editable text fields for writable fields; panel menu for metadata display and search field (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model, bae-media)
- **Notes:** Data: project-comp-layers.md, Project metadata (XMP). Output-module XMP options: ui-preview-render-io.md [ui-c].
