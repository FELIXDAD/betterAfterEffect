## Render Queue

### AE-UI-600 · Render item data model (渲染项)

- **What it does:** A render item (render queue item) is a persisted project object that binds one composition to one set of Render Settings, one log level and one or more Output Modules; the ordered list of render items is the project's Render Queue.
- **Exact semantics:**
  - The Render Queue is an ordered list stored in the project (saved with the project file; scripting `app.project.renderQueue.items`, 1-based). Order = render order.
  - Fields of one render item:

    | Field (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Comp (合成) | ref → CompItem | — | — | — | Immutable after creation (scripting `comp` read-only: to change the comp, delete and re-add). Deleting the comp from the project deletes its render items. UNVERIFIED: exact behaviour when the source comp is deleted (AE removes the item). |
    | Render (渲染) | bool | true when the item is complete (has output path), otherwise false | — | — | Checkbox; `render` attribute. Setting true → status Queued; false → Unqueued. |
    | Label (标签) | label index 0–16 | label of the comp (UNVERIFIED) | 0..16 | — | Colour chip in the Label column; same label set as project items (see project-comp-layers.md, Labels). |
    | Comment (注释) | string | "" | — | — | Free text; Comment column hidden by default. |
    | Status (状态) | enum RQItemStatus | derived (AE-UI-602) | — | — | Read-only to scripts. |
    | Render Settings (渲染设置) | RenderSettings record (AE-UI-614..AE-UI-620) + template name | default RS template ("Best Settings") | — | — | Template name shown as underlined text; edits mark it "Custom" (UNVERIFIED: the label becomes "Custom: Best Settings"). |
    | Log (日志) | enum LogType | Errors Only | Errors Only / Plus Settings / Plus Per Frame Info | — | AE-UI-609. |
    | Output Modules (输出模块) | ordered list ≥ 1 of OutputModule | 1 module from default OM template | 1..n | — | AE-UI-605. |
    | Notify (通知) | bool | value of Preferences > Notifications "auto enable" (AE-UI-6xx Notifications) | — | — | `queueItemNotify` (AE 22.0). |
    | Skip Frames | int | 0 | 0..99 | frames | Scripting only (`skipFrames`): renders every (n+1)-th frame ("render on twos" = 1). No UI. |
    | Time Span Start / Duration | f64 | from Render Settings Time Span | — | seconds | Scripting `timeSpanStart`, `timeSpanDuration`; writing sets Time Span to Custom. |
    | Started (开始时间) | datetime? | null | — | — | Set when rendering of the item begins (`startTime`). |
    | Render Time (渲染时间) | int? | null | ≥ 0 | seconds | `elapsedSeconds`; excludes pauses (UNVERIFIED). |
    | Log File Path | path? | null | — | — | Shown under Render Settings/Log once a log was written. |
  - Items are independent: changing a template later does **not** change items already in the queue (templates are copied by value at apply time).
- **Priority:** P0 — export is the end of every AE workflow.
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting: `RenderQueueItem` (`comp`, `render`, `status`, `logType`, `outputModules`, `numOutputModules`, `skipFrames`, `startTime`, `elapsedSeconds`, `timeSpanStart`, `timeSpanDuration`, `templates`, `queueItemNotify`, `onStatusChanged`; methods `applyTemplate`, `saveAsTemplate`, `duplicate`, `remove`, `getSetting(s)`/`setSetting(s)` (AE 13.0) with English key strings such as "Proxy Use", "Color Depth", "Time Span Start"). BAE decision: render items are Prims in the project layer so they sync and undo like other data; the live render state (progress, elapsed) is not persisted in the CRDT, only the final status/start/elapsed/log path.

### AE-UI-601 · Render Queue panel layout and columns (渲染队列面板)

- **What it does:** Window > Render Queue (Ctrl+Alt+0 / Cmd+Opt+0) shows the queue as a table of render items with twirl-down rows for Render Settings, Log and each Output Module, plus a header area with the current render progress and the Render / Pause / Stop / Queue in AME buttons, and a footer with batch information.
- **Exact semantics:**
  - Top area (left→right): Current Render twirl + progress bar + Elapsed / Remaining (AE-UI-607); right side buttons **Queue in AME** (AME 中的队列), **Stop** (停止), **Pause** (暂停) / **Continue** (继续), **Render** (渲染). Before rendering only Render and Queue in AME are enabled.
  - Item table columns (context menu on a column header > Columns toggles visibility; columns can be reordered/resized like Project panel columns):

    | Column (中文) | Default visible | Content |
    |---|---|---|
    | Render (渲染) | yes | checkbox (Queued ↔ Unqueued) |
    | Label (标签) | yes | label colour chip, click opens label menu |
    | # | yes | 1-based position in queue |
    | Comp Name (合成名称) | yes | source comp name; twirl expands the item |
    | Status (状态) | yes | AE-UI-602 strings |
    | Started (开始时间) | yes | date/time render of the item started |
    | Render Time (渲染时间) | yes | elapsed render time of the item |
    | Comment (注释) | no | editable text |
    | Notify (通知) | no (UNVERIFIED: shown by default since AE 22.0 when notifications are enabled) | per-item notify checkbox |
  - Expanded item rows: `Render Settings: ▾ <template name>` (triangle opens the template menu, underlined name opens the Render Settings dialog); `Log: ▾ Errors Only`; for each output module `Output Module: ▾ <template name>` and `Output To: ▾ <file name>` with `+`/`−` buttons left of Output To (AE-UI-605). Expanding the Output Module group shows a summary of its settings (format, channels, output path) and the Post-Render Action menu.
  - Selection: click, Shift-click range, Ctrl/Cmd-click toggle; Up/Down arrows move selection; Shift+Up/Down extend (same shortcuts as Project panel). Multi-selection lets template menus apply to all selected items.
  - Footer (AE-UI-608): Message, RAM, Renders Started, Total Time Elapsed, notification bell + "Notify when queue completes" checkbox.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** The Render Queue opens as a tab in the Timeline frame by default in AE's standard workspaces (UNVERIFIED). Panel shortcut also listed in ui-preview-render-io.md, Panel framework (AE-UI-001). AE disables menus while a modal render runs ("Menus are disabled and Render Queue window is open" is a known AE support topic): see AE-UI-612.

### AE-UI-602 · Render item status values and state machine (渲染项状态)

- **What it does:** Each render item has one status that controls whether pressing Render processes it and reports the outcome.
- **Exact semantics:**
  - Enum (UI string / 中文 / scripting `RQItemStatus`):

    | UI string | 中文 | Scripting | Meaning |
    |---|---|---|---|
    | Unqueued | 未加入队列 | `UNQUEUED` | Listed, Render box off, not rendered by Render. |
    | Queued | 已加入队列 | `QUEUED` | Render box on, output valid; will render. |
    | Needs Output | 需要输出 | `NEEDS_OUTPUT` | At least one output module has no file path ("Not yet specified" (尚未指定)). Render box cannot be checked. |
    | Rendering | 正在渲染（译名待核） | `RENDERING` | Currently rendering. |
    | (paused) | — | `WILL_CONTINUE` | Rendering paused (UI shows Rendering + Continue button). |
    | User Stopped | 用户已停止 | `USER_STOPPED` | Stopped by Stop button or script. |
    | Failed | 失败 | `ERR_STOPPED` | Stopped because of an error; details in log. |
    | Done | 完成 | `DONE` | Rendered successfully. |
  - Transitions:
    ```
    create: has_output_path(all OMs) ? Queued : Needs Output
    Needs Output --(set path on all OMs)--> Queued
    Queued <--(toggle Render checkbox)--> Unqueued
    Render pressed: for item in queue order where status==Queued: -> Rendering
    Rendering --Pause--> WILL_CONTINUE --Continue--> Rendering
    Rendering --finished--> Done
    Rendering --error--> Failed
    Rendering --Stop--> User Stopped (+ new item, AE-UI-606)
    Done / Failed / User Stopped: Render checkbox disabled; item cannot be re-rendered (duplicate it instead)
    ```
  - Duplicating a Done item gives the copy status Queued (if its output is valid).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting `RenderQueueItem.onStatusChanged` callback fires on every status change. Failed items still keep partial output files on disk (UNVERIFIED for movie containers: AE leaves the incomplete file).

### AE-UI-603 · Adding compositions to the Render Queue (添加到渲染队列)

- **What it does:** Creates render items from compositions, footage or the current frame.
- **Exact semantics:**
  - | Command (中文) | Windows | macOS | Result |
    |---|---|---|---|
    | Composition > Add to Render Queue (合成 > 添加到渲染队列) | Ctrl+M | Cmd+M | One item per selected comp (or the active comp); default RS + OM templates for "Movie". |
    | Add active composition or selected items to render queue | Ctrl+Shift+/ (main keyboard) | Cmd+Shift+/ | Same as above. |
    | Composition > Save Frame As > File… (合成 > 帧另存为 > 文件) | Ctrl+Alt+S | Cmd+Opt+S | Item rendering the single frame at the CTI with the default "Frame" RS/OM templates (AE-UI-645). |
    | Composition > Pre-render… (合成 > 预渲染) | — | — | Item with default "Pre-Render" templates and Post-Render Action = Import & Replace Usage targeting the comp (AE-UI-638). |
    | Drag comp(s) from the Project panel into the Render Queue panel | — | — | Item(s) inserted at the drop position. |
    | Drag footage item into the Render Queue panel | — | — | Creates a new comp from the footage (same as New Comp from Selection) and adds it: a format-conversion shortcut. |
  - Each new item gets: the default Render Settings template for its kind (Movie / Frame / Pre-Render / Movie Proxy / Still Proxy; AE-UI-621), the default Output Module template for its kind (AE-UI-637), and an output file name from the default file-name template (AE-UI-640) in the folder resolved per AE-UI-640.
  - If Preferences > Output "Use Default File Name and Folder" is off (UNVERIFIED default), AE prompts with a Save dialog for the output path when adding via Ctrl+M (UNVERIFIED: modern AE no longer prompts and uses the last folder).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion, apps/bae)
- **Notes:** Scripting `app.project.renderQueue.items.add(comp)` returns the new RenderQueueItem. Ctrl+M on an already queued comp adds another item (no de-duplication); AE asks "This composition is already in the render queue" only for Make Movie in some versions (UNVERIFIED). Ctrl+Alt+M / Cmd+Opt+M = Add to Adobe Media Encoder Queue (AE-UI-611).

### AE-UI-604 · Managing render items (管理渲染项)

- **What it does:** Reordering, queuing/unqueuing, deleting, duplicating and revealing render items.
- **Exact semantics:**
  - | Action (中文) | Windows | macOS | Semantics |
    |---|---|---|---|
    | Toggle Render checkbox | click | click | Queued ↔ Unqueued (disabled for Needs Output / Done / Failed / User Stopped). |
    | Remove item (清除) | Delete / Edit > Clear | Delete | Removes from queue; files on disk untouched. Not allowed while rendering. |
    | Move up / down one | Ctrl+Alt+Up / Down | Cmd+Opt+Up / Down | Layer > Arrange > Bring Render Item Forward / Send Render Item Backward (将渲染项前移一层 / 将渲染项后移一层). |
    | Move to top / bottom | Ctrl+Alt+Shift+Up / Down | Cmd+Opt+Shift+Up / Down | Bring Render Item to Front / Send Render Item to Back (将渲染项置于顶层 / 将渲染项置于底层). |
    | Drag to reorder | drag | drag | Insertion line shows the drop gap. |
    | Duplicate (复制) | Ctrl+D | Cmd+D | Copy of settings and output modules; output file names get a unique suffix (e.g. `name_1`), status Queued/Needs Output; Done → Queued. |
    | Duplicate With File Name (与文件名一起复制) | Ctrl+Shift+D | Cmd+Shift+D | Copy keeping the identical output path(s) (will overwrite). |
    | Reveal Composition in Project (在项目中显示合成) | context menu | context menu | Selects the comp in the Project panel. |
    | Reveal in Explorer / Finder (output) | context menu on Output Module / click underlined path | same | Opens the output folder (UNVERIFIED menu string). |
  - Items can be edited only while not rendering.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** All of these are Commands (undoable) in BAE. Renumbering of the # column follows the new order.

### AE-UI-605 · Multiple output modules per render item (多个输出模块)

- **What it does:** One render pass feeds any number of output modules, each encoding the same rendered frames to its own format and file.
- **Exact semantics:**
  - `+` left of the last `Output To:` adds a module with the default OM template; `−` left of any `Output To:` removes it (the last remaining module cannot be removed (UNVERIFIED: the − button is disabled)).
  - Composition > Add Output Module (合成 > 添加输出模块) adds one default module to every selected render item.
  - Selecting output modules and pressing Ctrl+D / Cmd+D duplicates them inside their item.
  - Choosing a template from one module's Output Module menu with several items/modules selected applies it to all selected modules.
  - Rendering: each frame is rendered once at the Render Settings and handed to every module in order; each module applies its own post-processing (resize → crop → channel/depth/premultiply conversion → colour conversion → encode) independently.
  - Dragging an output module to the Project panel imports its file (or, before it is rendered, a placeholder of the right size/duration (UNVERIFIED)).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media, bae-ui-motion)
- **Notes:** Scripting `RenderQueueItem.outputModules.add()` (OMCollection), `OutputModule.remove()`. Implementation hint: share one GPU readback per frame between modules; perform resize/crop on GPU before readback when only one module exists.

### AE-UI-606 · Render, Pause, Continue and Stop (渲染、暂停、继续、停止)

- **What it does:** Starts rendering all Queued items in order, and lets the user pause, continue or stop the batch.
- **Exact semantics:**
  - **Render**: processes every item with status Queued, top to bottom; items added as Queued during a pause are not picked up (UNVERIFIED). Items with other statuses are skipped.
  - **Pause** → button becomes **Continue**; while paused AE cannot be used for anything else (settings cannot change). AE also auto-pauses when the output disk runs out of space; freeing space and pressing Continue resumes.
  - **Stop** (click): current item → User Stopped; a **new item is appended directly below it with status Unqueued**, an **incremented output file name**, and Time Span starting at the last successfully rendered frame (so the new item's first frame = last good frame of the stopped item) and the same end. Remaining queued items are not rendered.
  - **Alt+click Stop** (Windows) / **Option+click Stop** (macOS): current item → User Stopped; a new item is added with status **Queued**, the **same output file name** and the **same (full) duration** as the original (start over).
  - End of batch: success chime; a different sound on failure (AE-UI-610).
- **Priority:** P0 (Render, Stop) / P1 (Pause, resume-from-stop items)
- **Owner:** apps/bae (+ bae-model, bae-ui-motion)
- **Notes:** Scripting `RenderQueue.render()` (blocks until done), `pauseRendering(bool)`, `stopRendering()`, `rendering` (true while rendering or paused). BAE decision: rendering runs on a background job so the UI stays usable (like AE's Queue in AME but in-process); editing a comp that is currently rendering is blocked or the render uses an immutable snapshot of the document version taken at Render time (preferred, Loro version).

### AE-UI-607 · Current Render progress display (当前渲染)

- **What it does:** Shows progress of the item being rendered.
- **Exact semantics:**
  - Header line: `Current Render` twirl, progress bar spanning the panel, **Elapsed** (已用时间) and **Remaining** (剩余时间) estimate.
  - Progress bar colouring (AE 22.0+ with Multi-Frame Rendering): bright green = frames currently rendering, dark green = rendered but not yet written, blue = frames written to disk.
  - Info button / expanded details: concurrent frames rendering, time of current frame, start frame and end frame, **Estimated size** (估计大小) of the output, **Free space** (可用空间) on the target disk.
  - Expanded Current Render details collapse automatically after a timeout; Alt/Option-click the twirl keeps them open.
  - Remaining estimate: `remaining = elapsed / frames_done × (frames_total − frames_done)` (UNVERIFIED: AE's exact estimator; it is smoothed).
  - Frame counts: `frames_total = ceil(time_span_duration × render_fps)` honouring skipFrames.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Older AE (≤ CC 2021) showed a details table (Composition, Layer, Stage, Last/Difference/Average frame time, File Name, File Size, Est. Final File Size, Free Disk Space, Overflows, Current Disk). BAE may show the same data; numbers are informative only.

### AE-UI-608 · Batch information footer (批量渲染信息)

- **What it does:** The bottom bar of the Render Queue summarizes the whole batch.
- **Exact semantics:**
  - | Field (中文) | Content |
    |---|---|
    | Message (消息) | Status message, e.g. "Rendering 1 of 4". |
    | RAM | Memory available to rendering, e.g. "34% used of 32GB" (UNVERIFIED format). |
    | Renders Started (渲染开始时间) | Date/time the batch started. |
    | Total Time Elapsed (已用总时间) | Batch elapsed time **excluding pauses**. |
    | Notify when queue completes (渲染队列完成时通知) | checkbox, scripting `RenderQueue.queueNotify` (AE 22.0). |
- **Priority:** P2
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-609 · Render log files (渲染日志)

- **What it does:** Writes a text log per render item at the chosen verbosity.
- **Exact semantics:**
  - Log menu (日志) values in order (scripting `LogType`):

    | UI string | 中文 | Scripting | Behaviour |
    |---|---|---|---|
    | Errors Only | 仅错误 | `ERRORS_ONLY` | Log file created only if an error occurs. Default. |
    | Plus Settings | 增加设置 | `ERRORS_AND_SETTINGS` | Always written; lists render settings (and output module settings). |
    | Plus Per Frame Info | 增加每帧信息 | `ERRORS_AND_PER_FRAME_INFO` | Settings plus one line per rendered frame (frame number/time and render time). |
  - When a log is written its path appears under the Render Settings heading and the Log menu (clickable).
  - Location: a folder named `<project name> Logs` next to the first output module's file, file name `<comp name> <date/time>.txt` (UNVERIFIED: naming pattern).
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** BAE decision: also emit a machine-readable JSON log (`--log-format json` in `bae render`, AE-UI-655) with per-frame render times for benchmarks.

### AE-UI-610 · Render completion notifications and sounds (渲染完成通知)

- **What it does:** Notifies the user when items or the whole queue finish.
- **Exact semantics:**
  - A chime plays when all items in the queue finished; a different sound plays when a render failed.
  - Per-item Notify checkbox (Notify column) and footer "Notify when queue completes" send a Creative Cloud (desktop/mobile/watch) push notification (AE 22.0 / 2022).
  - Preferences > Notifications "Automatically enable notifications when you add a composition to the Render Queue" makes Notify checked by default for new items (AE-UI-6xx Preferences Notifications). The bell icon at the bottom-left of the panel opens that preference.
- **Priority:** P2 (sounds P1)
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** BAE decision: replace Creative Cloud push with an OS notification (macOS UserNotifications / Windows toast) plus an optional webhook URL; sounds via cpal. No account service dependency.

### AE-UI-611 · Queue in AME and Add to Adobe Media Encoder Queue (AME 中的队列 / 添加到 Adobe Media Encoder 队列)

- **What it does:** Hands queued items (with their Render Settings) or a comp to the separate Adobe Media Encoder application for background encoding with AME presets.
- **Exact semantics:**
  - **Queue in AME** button: sends every Queued item; render settings travel, **output module settings (format, channels, …) do not**, output file name and folder do (AME ignores AE's file-name templates, which can change image-sequence numbering). Scripting `queueInAME(render_immediately)`, `canQueueInAME`.
  - **Composition > Add to Adobe Media Encoder Queue** / File > Export > Add to Adobe Media Encoder Queue (Ctrl+Alt+M / Cmd+Opt+M): sends the comp itself; AME renders it through Dynamic Link with its own presets.
- **Priority:** P2 — BAE has no AME; covered by background rendering.
- **Owner:** apps/bae
- **Notes:** BAE decision: no AME integration. The equivalent is "Render in Background" (separate `bae render` process fed with a snapshot of the project, AE-UI-655) so the user keeps working; keep a menu item with the same position labelled "Queue in Background Renderer". AE 14.0 (CC 2017) added scripting support.

### AE-UI-612 · Application behaviour during rendering (渲染期间的应用行为)

- **What it does:** Defines what the rest of the app does while the Render Queue renders.
- **Exact semantics:**
  - AE renders modally: menus and panels are disabled (only Pause/Stop and the Render Queue panel react; scrolling/expanding the queue is allowed).
  - The Composition panel shows each rendered frame as it completes unless Caps Lock is on (or Preferences > Display "Show Rendering Progress in Info Panel and Flowchart" is off — UNVERIFIED relation); turning off the viewer update speeds up simple renders.
  - The Info panel shows the layer currently being rendered when the Display preference "Show Rendering Progress in Info Panel and Flowchart" is on.
  - Disk full on the output volume → automatic Pause (AE-UI-606).
  - Render Settings apply only to the render item; the comp itself is not changed by them.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** BAE decision: background rendering (see AE-UI-606) instead of modal lock; viewer preview of rendered frames optional; Caps Lock behaviour kept as "pause viewer updates".

### AE-UI-613 · Render queue automation interface (渲染队列脚本接口)

- **What it does:** Lets scripts and agents read and drive the queue (add items, apply templates, set settings, render).
- **Exact semantics:**
  - Must expose, at minimum, the AE scripting surface semantics: `renderQueue.items.add(comp)`, `item.render`, `item.status`, `item.applyTemplate(name)`, `item.saveAsTemplate(name)`, `item.getSettings(format)` / `setSettings(obj)` with AE's English setting keys (e.g. `"Quality"`, `"Resolution"`, `"Proxy Use"`, `"Effects"`, `"Color Depth"`, `"Frame Blending"`, `"Motion Blur"`, `"Time Span Start"`, `"Time Span Duration"`), `outputModule.applyTemplate`, `outputModule.getSettings` keys (`"Format"`, `"Output File Info" → {"Base Path","Subfolder Path","File Name","File Template","Full Flat Path"}`, `"Crop"`, `"Crop Top"`, `"Crop Left"`, `"Crop Bottom"`, `"Crop Right"`, …), `outputModule.file`, `postRenderAction`, `includeSourceXMP`, `renderQueue.render()/pauseRendering()/stopRendering()`.
  - `GetSettingsFormat` values: `STRING`, `STRING_SETTABLE`, `NUMBER`, `NUMBER_SETTABLE`, `SPEC`.
  - Known AE quirk: in AE the OM `Format` setting is readable but not settable via `setSettings`; BAE should make it settable.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** BAE decision: expose the same model through MCP tools (`render_queue.add`, `render_queue.set_settings`, `render_queue.render`) and the JS script plugin API, using the AE key strings as aliases so ported ExtendScript render scripts keep working.

## Render Settings

### AE-UI-614 · Render Settings: Quality, Resolution and Size (渲染设置：品质、分辨率、大小)

- **What it does:** The first block of the Render Settings dialog (渲染设置) overrides layer quality and the render resolution for one render item; the settings apply to the root comp **and all nested comps**.
- **Exact semantics:**
  - Dialog title "Render Settings" with the comp name ("Composition 'Comp 1'"). Fields (setting key for scripting in brackets):

    | Parameter (中文) | Type | Default (Best Settings) | Range / enum (menu order, scripting value) | Units | Notes |
    |---|---|---|---|---|---|
    | Quality (品质) [`"Quality"`] | enum | Best | Current Settings (当前设置) −1 / Best (最佳) 2 / Draft (草图) 1 / Wireframe (线框) 0 | — | Overrides every layer's Quality switch (see compositing-3d-color.md, Layer quality AE-CMP-059). Current Settings keeps each layer's switch. |
    | Resolution (分辨率) [`"Resolution"` = [x,y]] | enum/int pair | Full [1,1] | Current Settings (当前设置) / Full (完整) [1,1] / Half (二分之一) [2,2] / Third (三分之一) [3,3] / Quarter (四分之一) [4,4] / Custom… (自定义) [x,y] | factor | Custom opens "Custom Resolution": Render every x pixels horizontally, y pixels vertically (integer, UNVERIFIED range 1..99). Current Settings uses the comp's Resolution/Down Sample Factor. |
    | Size (大小) | read-only text | "1920 x 1080 → 1920 x 1080" | — | px | `comp size → rendered size`. |
  - Rendered size per axis: `out_w = ceil(comp_w / x)`, `out_h = ceil(comp_h / y)` (UNVERIFIED rounding; see compositing-3d-color.md, Resolution / Down Sample Factor AE-CMP-464 for the downsampling semantics, which are identical for final render).
  - Reduced-resolution renders keep the comp's pixel aspect ratio; the output module can upscale back (Resize, AE-UI-632).
  - Adobe's advice (not a rule): when rendering below Full, use Draft quality.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model, bae-ui-motion)
- **Notes:** Render Settings never modify the comp itself. The dialog has three sections: top (Composition), Time Sampling (时间采样), Options (选项).

### AE-UI-615 · Render Settings: Disk Cache, Proxy Use, Effects, Solo Switches, Guide Layers (磁盘缓存、代理使用、效果、独奏开关、引导层)

- **What it does:** Global overrides of cache use, proxies and per-layer switches for the render item.
- **Exact semantics:**

  | Parameter (中文) | Type | Default (Best Settings) | Enum (menu order, scripting value) | Notes |
  |---|---|---|---|---|
  | Disk Cache (磁盘缓存) [`"Disk Cache"`] | enum | Read Only | Read Only (只读) 0 / Current Settings (当前设置) 2 | Read Only: may read cached frames but writes no new frames to the disk cache. Current Settings: obeys Media & Disk Cache preferences (help calls it the "default" setting, but the Best Settings template ships with Read Only). |
  | Proxy Use (代理使用) [`"Proxy Use"`] | enum | Use No Proxies | Current Settings (当前设置) 2 / Use All Proxies (使用所有代理) 1 / Use Comp Proxies Only (仅使用合成代理) 3 / Use No Proxies (不使用代理) 0 | Current Settings uses each item's proxy toggle. See project-comp-layers.md, Proxy use when rendering (AE-PRJ-078). UNVERIFIED: menu order. |
  | Effects (效果) [`"Effects"`] | enum | Current Settings | Current Settings (当前设置) 2 / All On (全部开启) 1 / All Off (全部关闭) 0 | All On renders every applied effect even if its fx switch is off; All Off renders no effects (layer effects; UNVERIFIED whether layer styles count as effects). |
  | Solo Switches (独奏开关) [`"Solo Switches"`] | enum | Current Settings | Current Settings (当前设置) 2 / All Off (全部关闭) 0 | All Off = render as if no layer were soloed. |
  | Guide Layers (引导层) [`"Guide Layers"`] | enum | All Off | Current Settings (当前设置) 2 / All Off (全部关闭) 0 | Current Settings renders guide layers **of the top-level comp only**; guide layers in nested comps are never rendered. |
  - Evaluation: the overrides are applied when building the render graph for each comp in the tree (root and nested), except Guide Layers (root only by definition).
- **Priority:** P0 (Effects, Solo, Guide), P1 (Proxy Use), P2 (Disk Cache)
- **Owner:** bae-render (+ bae-model)
- **Notes:** These map to render-graph build flags in BAE: `effects_override: Option<bool>`, `ignore_solo`, `render_guides_root`, `proxy_policy`. Disk Cache maps to BAE's persistent frame cache policy (read-only during final render is the safe default).

### AE-UI-616 · Render Settings: Color Depth (颜色深度)

- **What it does:** Overrides the project bit depth for this render item.
- **Exact semantics:**
  - `"Color Depth"`: Current Settings (当前设置) −1 (default; uses the project's bpc) / 8 bits per channel (8 位/通道) 0 / 16 bits per channel (16 位/通道) 1 / 32 bits per channel (32 位/通道) 2.
  - The whole comp tree renders at that depth (effects that are 8-bpc-only still process in 8 bpc and are flagged in the comp; see effects.md, effect framework bpc).
  - Output depth is chosen separately in the Output Module (Depth, AE-UI-629); e.g. 32 bpc render into a PNG Millions of Colors output is quantised at encode time.
- **Priority:** P1
- **Owner:** bae-render (+ bae-color)
- **Notes:** Semantics of 8/16/32 bpc (clamping, 16-bit range 0..32768) in compositing-3d-color.md, Project color depth (AE-CMP-443) and Pixel encoding and clamping (AE-CMP-444). BAE decision: BAE's working buffer is RGBA16F by default; "8 bpc" render must still reproduce AE's 8-bpc clamping/quantisation per layer when fidelity mode is on.

### AE-UI-617 · Render Settings: Frame Blending and Motion Blur overrides (帧混合与运动模糊)

- **What it does:** Time Sampling (时间采样) overrides for frame blending and motion blur, including a shutter-angle override.
- **Exact semantics:**

  | Parameter (中文) | Type | Default (Best Settings) | Enum (menu order UNVERIFIED, scripting value) | Notes |
  |---|---|---|---|---|
  | Frame Blending (帧混合) [`"Frame Blending"`] | enum | On for Checked Layers | Current Settings (当前设置) 2 / On for Checked Layers (对选中图层打开) 1 / Off for All Layers (对所有图层关闭) 0 | On for Checked Layers renders frame blending for layers whose Frame Blending switch is set **regardless of the comp's Enable Frame Blending button**. Current Settings honours the comp button. |
  | Motion Blur (运动模糊) [`"Motion Blur"`] | enum | On for Checked Layers | Current Settings (当前设置) 2 / On for Checked Layers (对选中图层打开) 1 / Off for All Layers (对所有图层关闭) 0 | Same pattern with the Motion Blur switch and the comp's Enable Motion Blur. |
  | Override shutter angle (覆盖快门角度（译名待核）) | bool + number | off, value = comp shutter angle (180) | 0..720 (UNVERIFIED) | degrees | Enabled only when Motion Blur ≠ Off for All Layers. Replaces the comp's Shutter Angle for this render; Shutter Phase and samples still come from the comp (UNVERIFIED). |
  - Formulas for whether a layer blurs/blends: transform-keyframes-expressions.md, Motion blur switches and render settings (AE-ANI-102) and Frame blending switches (AE-ANI-099); rendering in compositing-3d-color.md, Motion blur rendering (AE-CMP-064) / Frame blending rendering (AE-CMP-065).
- **Priority:** P0 (Frame Blending/Motion Blur modes), P1 (shutter override)
- **Owner:** bae-render (+ bae-anim, bae-model)
- **Notes:** The Best Settings default "On for Checked Layers" means a final render shows motion blur on checked layers even if the user turned the comp-level button off for faster previews — a frequent AE surprise that BAE must reproduce.

### AE-UI-618 · Render Settings: Field Render and 3:2 Pulldown (场渲染与 3:2 Pulldown)

- **What it does:** Renders interlaced output (two fields per frame sampled at different times) and optionally applies 3:2 pulldown.
- **Exact semantics:**
  - Field Render (场渲染) [`"Field Render"`]: Off (关) 0 (default) / Upper Field First (高场优先) 1 / Lower Field First (低场优先) 2.
  - With field rendering, each output frame `n` at frame duration `T` is assembled from two renders: first field at `t = start + n·T`, second field at `t + T/2`; the first field occupies the upper (even, 0-based rows 0,2,4…) lines for Upper Field First, the lower (odd) lines for Lower Field First (UNVERIFIED row-parity convention).
  - 3:2 Pulldown (3:2 Pulldown) [`"3:2 Pulldown"`]: Off 0 / WSSWW 1 / SSWWW 2 / SWWWS 3 / WWWSS 4 / WWSSW 5 (phase of whole (W) and split (S) frames). Enabled only when Field Render ≠ Off. Used to render 23.976 fps comps to 29.97 fps interlaced video: 4 film frames → 5 video frames (10 fields) in the chosen phase.
  - DV Settings template = Best Settings with Lower Field First.
- **Priority:** P2 — interlaced delivery is rare for motion graphics.
- **Owner:** bae-render (+ bae-media)
- **Notes:** Field order of footage interpretation (separate fields) is in project-comp-layers.md, Interpret Footage. Cropping one row top/bottom in the Output Module swaps field order (AE-UI-633).

### AE-UI-619 · Render Settings: Time Span (时间跨度)

- **What it does:** Selects which part of the comp is rendered.
- **Exact semantics:**
  - `"Time Span"`: Length of Comp (合成长度) 0 / Work Area Only (仅工作区域) 1 (Best Settings default) / Custom (自定义) 2. Menu order in the dialog: UNVERIFIED (Work Area Only, Length of Comp, Custom).
  - Read-only text shows `Start: <tc>  End: <tc>  Duration: <tc>` in the project's time display format; **Set…** (设置) button (or choosing Custom) opens the Custom Time Span dialog (自定义时间范围) with Start, End, Duration fields (editing End updates Duration and vice versa).
  - Scripting keys `"Time Span Start"`, `"Time Span End"`, `"Time Span Duration"` (seconds); `RenderQueueItem.timeSpanStart/timeSpanDuration`; writing them switches Time Span to Custom.
  - Work Area Only follows later changes to the comp work area until rendering starts (UNVERIFIED: AE stores the mode, not the times).
  - Frames rendered: `N = round(duration × render_fps)` (UNVERIFIED rounding), sample times `t_k = start + k / render_fps`, `k = 0..N−1`.
  - Default for new items: comes from the RS template; AE 2026 Best Settings = Work Area Only (observed in a saved AE 2026 project). Older AE versions defaulted Best Settings to Length of Comp (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Work area definition: project-comp-layers.md, work area. `bae render` CLI `--start/--end` override this (AE-UI-655).

### AE-UI-620 · Render Settings: Frame Rate (帧速率)

- **What it does:** Sets the temporal sampling rate of the render without changing the comp.
- **Exact semantics:**
  - `"Frame Rate"`: Use comp's frame rate (使用合成的帧速率) 0 (default, shows the comp fps in parentheses) / Use this frame rate (使用此帧速率) 1 with a numeric field (`"Use this frame rate"`, fps, UNVERIFIED range 1..999, fractional allowed e.g. 29.97).
  - The rendered movie frame rate = this render frame rate (output modules encode at this rate; formats with fixed rates show the "Settings mismatch" warning, AE-UI-636).
  - Sampling: `t_k = start + k / fps_render`. Rendering a 24 fps comp at 12 fps samples every second comp frame; rendering at 48 fps samples between comp frames (layers with keyframes interpolate; footage holds or frame-blends).
  - Motion blur shutter interval uses the comp's frame duration and shutter angle, not the render rate (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model, bae-render)
- **Notes:** BAE times are in flicks (705,600,000 per second); `t_k` must be computed as `start_flicks + k * 705600000 / fps_render` with rational fps (e.g. 30000/1001) to avoid drift.

### AE-UI-621 · Render Settings: Options (Skip Existing Files, Use Storage Overflow) (选项：跳过现有文件)

- **What it does:** Options for partial and multi-machine sequence renders.
- **Exact semantics:**
  - **Skip existing files** (跳过现有文件) [`"Skip Existing Files"`], bool, default off ("allows multi-machine rendering"). Only for image-sequence outputs: before rendering, AE scans the output folder for files belonging to the same sequence (same name template, start number, frame rate and time span) and renders only missing frame numbers. Multiple machines sharing the same project and output folder each render frames that do not yet exist; AE writes a placeholder/"in progress" file per frame so other machines skip it (UNVERIFIED mechanism).
  - **Use storage overflow** (使用存储溢出（译名待核）): legacy option (removed from current AE, UNVERIFIED version) that continued a render onto overflow volumes (Preferences > Output) when the disk filled.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-media)
- **Notes:** Multi-Machine Settings (RS) + Multi-Machine Sequence (OM) templates use this. BAE decision: implement Skip Existing Files with an atomic "claim" file per frame (`<frame>.claim`, O_EXCL create) so it is safe across machines on a NAS; storage overflow not implemented.

### AE-UI-622 · Render Settings templates (渲染设置模板)

- **What it does:** Named presets of all Render Settings fields; the triangle menu next to "Render Settings:" applies one to the selected items.
- **Exact semantics:**
  - Built-in templates (AE 2026 order as listed by scripting `templates`): **Best Settings** (最佳设置), **Current Settings** (当前设置), **DV Settings** (DV 设置), **Draft Settings** (草图设置), **Multi-Machine Settings** (多机设置). AE also has a hidden internal template `_HIDDEN X-Factor` (not shown in UI).

    | Field | Best Settings | Current Settings | Draft Settings | DV Settings | Multi-Machine |
    |---|---|---|---|---|---|
    | Quality | Best | Current Settings | Draft | Best | Best |
    | Resolution | Full | Current Settings | Half (UNVERIFIED) | Full | Full |
    | Disk Cache | Read Only | Current Settings (UNVERIFIED) | Current Settings (UNVERIFIED) | Read Only | Read Only |
    | Proxy Use | Use No Proxies | Current Settings | Current Settings (UNVERIFIED) | Use No Proxies | Use No Proxies |
    | Effects | Current Settings | Current Settings | Current Settings | Current Settings | Current Settings |
    | Solo Switches | Current Settings | Current Settings | Current Settings | Current Settings | Current Settings |
    | Guide Layers | All Off | Current Settings (UNVERIFIED) | All Off | All Off | All Off |
    | Color Depth | Current Settings | Current Settings | Current Settings | Current Settings | Current Settings |
    | Frame Blending | On for Checked Layers | Current Settings | Current Settings (UNVERIFIED) | On for Checked Layers | On for Checked Layers |
    | Field Render | Off | Off | Off | Lower Field First | Off |
    | Motion Blur | On for Checked Layers | Current Settings | Current Settings (UNVERIFIED) | On for Checked Layers | On for Checked Layers |
    | Time Span | Work Area Only | Work Area Only (UNVERIFIED) | Work Area Only (UNVERIFIED) | Work Area Only | Work Area Only |
    | Frame Rate | comp | comp | comp | comp | comp |
    | Skip Existing Files | off | off | off | off | on |
  - Best Settings column observed in an AE 2026 project's `getSettings`; other columns derived from Adobe's descriptions ("DV = Best + Lower Field First", "Multi-Machine = Best + Skip Existing Files").
  - Ctrl/Cmd + choosing a template from the menu makes it the default for new **movie** items. The menu also lists "Custom…" (opens the dialog) and "Make Template…" (opens the Templates dialog, AE-UI-623).
  - Changing a template never updates items already in the queue.
- **Priority:** P0 (Best/Draft/Current, default selection), P2 (DV, Multi-Machine)
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `RenderQueueItem.templates`, `applyTemplate(name)`, `saveAsTemplate(name)`. Templates are stored in the application preferences, not in the project (AE stores them in its prefs file; items in the project embed full copies).

### AE-UI-623 · Render Settings Templates dialog and per-purpose defaults (渲染设置模板对话框)

- **What it does:** Edit > Templates > Render Settings… (编辑 > 模板 > 渲染设置) manages render-settings templates and chooses which template is the default for each kind of render.
- **Exact semantics:**
  - Defaults area (默认值), one menu each:

    | Default for (中文) | Used when | Default template |
    |---|---|---|
    | Movie Default (影片默认值（译名待核）) | Add to Render Queue | Best Settings |
    | Frame Default (帧默认值（译名待核）) | Composition > Save Frame As > File | Current Settings |
    | Pre-Render Default (预渲染默认值（译名待核）) | Composition > Pre-render | Best Settings |
    | Movie Proxy Default (影片代理默认值（译名待核）) | File > Create Proxy > Movie | Draft Settings |
    | Still Proxy Default (静止代理默认值（译名待核）) | File > Create Proxy > Still | Best Settings |
    UNVERIFIED: the default template per row (Frame Default = Current Settings and Movie Proxy = Draft Settings are from memory).
  - Settings area: Settings Name menu, **New…** (新建), **Edit…** (编辑), **Duplicate** (复制), **Delete** (删除), and **Save All…** (全部保存) / **Load…** (加载) to export/import a template file (`.ars`, After Effects Render Settings; UNVERIFIED extension).
  - Summary text shows the selected template's settings.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: templates stored as JSON in the user config dir, exportable; ship AE's built-in names so imported AE projects/scripts referencing "Best Settings" resolve.
