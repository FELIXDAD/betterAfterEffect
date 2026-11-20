# After Effects inventory: project, compositions and layers (AE-PRJ)

## Scope

This file inventories everything an After Effects user touches before and around animation itself: the project as a container (project items and folders, import, Interpret Footage, proxies, solids, placeholders, Project Settings, undo, Collect Files / Reduce Project) and the composition/layer layer of the document (Composition Settings, work area, composition markers, every layer type, the layer property tree, layer timing, labels, layer switches, parenting, stacking, layer editing operations, precompose, replace source, sequence layers, auto-orient, layer markers and layer styles). It specifies the AE behaviour and stored data that BAE must reproduce; rendering maths, animation maths and panel UI live in the sibling files README.md, project-comp-layers.md (this file), transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md and ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

Conventions used below: "AE" means the current After Effects release (26.x/27.0, 2026) unless a version is named; time values are given in seconds or frames as AE shows them, and BAE stores every time as integer flicks (705,600,000 per second, ARCHITECTURE 4.4). Scripting names refer to the After Effects Scripting Guide (docsforadobe) object model.

## Project container

### AE-PRJ-001 · Project document and root folder (项目)

- **What it does:** A project is the single document AE has open; it owns a tree of project items (compositions, footage, folders) under a virtual root folder, plus the render queue and the project settings.
- **Exact semantics:**
  - Exactly one project is open per AE instance. Opening or creating another project closes the current one (with a save prompt if it is dirty, see AE-PRJ-009).
  - The root folder is a virtual `FolderItem` (`app.project.rootFolder`) that is not itself shown in the Project panel and has no name the user can edit; items whose `parentFolder` is the root appear at top level.
  - Project-level owned data (each specified elsewhere): the item tree (AE-PRJ-014 onward), Project Settings (see section "Project Settings"), the render queue (see ui-preview-render-io.md, Render Queue), project XMP metadata (AE-PRJ-013), the project-level solids folder name (AE-PRJ-021), used fonts list (read-only, derived), and the saved panel/workspace state.
  - `numItems` counts every item in the project at any depth (folders included), not only top-level items. `items` is a 1-based flat collection over all items.
  - A new empty project has zero items and default Project Settings (or the settings of the template project, AE-PRJ-012).
  - File formats: `.aep` binary (primary), `.aepx` XML (AE-PRJ-005), `.aet` template (AE-PRJ-012).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting: `app.project`, `Project.rootFolder`, `Project.items`, `Project.numItems`, `Project.activeItem`, `Project.selection`. BAE decision: BAE stores the project as its own `.bae` directory package (ARCHITECTURE 4.6) with a `Project` Prim and `Folder`/`Comp`/`Asset` Prims; the root folder maps to the Project Prim's children. BAE does not read or write `.aep` in M1; AE-project interchange is out of scope for this item. BAE may allow several projects open in separate windows, but each window keeps AE's one-project semantics.

### AE-PRJ-002 · Item IDs and lookup (项目项 ID（译名待核）)

- **What it does:** Every project item carries an integer ID that is unique within the project and stable across save/reopen, so scripts, expressions and references can address items independently of name and position.
- **Exact semantics:**
  - `Item.id`: positive integer, unique among items in the project, persistent in the saved file, never reused while the project is open. AE assigns IDs from a project-wide counter; the numeric values carry no meaning (do not rely on creation order).
  - `Project.itemByID(id)` returns the Item with that ID (AE 13.0 (CC 2014) and later). UNVERIFIED: result for an ID that no item has (null vs exception; the scripting guide does not say).
  - Layers have their own IDs (`Layer.id`, `Project.layerByID(id)`, both added in AE 22.0 (2022)), unique within the project. `layerByID` returns null when no layer has the ID and throws when the argument is not an unsigned integer.
  - Duplicate (AE-PRJ-019) and Import of an `.aep` into a project (AE-PRJ-037) assign new IDs to the new items.
  - Items are also addressable by 1-based index in `Project.items` / `FolderItem.items`; indices change when items are added, removed or sorted, IDs do not.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Source: https://ae-scripting.docsforadobe.dev/general/project/ (itemByID, layerByID). BAE decision: BAE uses Prim paths for addressing and additionally keeps a stable opaque item ID per Prim (needed by expressions `comp("name")` fallbacks, scripting-like agent APIs and the CRDT, where names are not unique). IDs must survive move between folders and rename.

### AE-PRJ-003 · New, Open and Close Project (新建项目 / 打开项目 / 关闭项目)

- **What it does:** Creates an empty project, opens a saved project file, or closes the current project.
- **Exact semantics:**
  - **New Project** (File > New > New Project): closes the current project (save prompt if dirty) and creates an untitled project ("Untitled Project.aep" in the title bar) with default Project Settings; if Preferences > Project > New Project loads template is on, the new untitled project is a copy of the chosen template (AE-PRJ-012).
  - **Open Project** (File > Open Project): accepts `.aep`, `.aepx`, `.aet`. Opening an `.aet` creates an untitled copy (AE-PRJ-012). Opening a project saved by an older major version converts it on open and the project opens as an untitled/converted copy so the original is not overwritten (AE asks for a new name on the first save). UNVERIFIED: the converted project title shows "<name> (converted)". A project saved by a newer major version cannot be opened.
  - **Open Recent** (File > Open Recent) lists recently opened projects.
  - On open AE reports, in one dialog each, missing footage, missing effects and missing fonts (see AE-PRJ-072); the project still opens.
  - Preferences > Project > Maintain Current Workspace When Opening Projects: when on, the workspace stored in the project is ignored and the current one kept.
  - **Close Project** (File > Close Project): closes with a save prompt if dirty; scripting `Project.close(CloseOptions.DO_NOT_SAVE_CHANGES | PROMPT_TO_SAVE_CHANGES | SAVE_CHANGES)`.
- **Priority:** P0
- **Owner:** apps/bae (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision: `.bae` packages and `.baez` single files (ARCHITECTURE 16) are the open/save formats; a newer format version than the app supports must refuse to open (or open read-only) rather than silently drop data, matching AE's newer-version refusal.

### AE-PRJ-004 · Save, Save As and Save a Copy (保存 / 另存为 / 保存副本)

- **What it does:** Writes the project to disk under its current name, under a new name that becomes the open project, or as a copy that does not change the open project.
- **Exact semantics:**
  | Command (中文) | Effect on open project | Effect on disk |
  |---|---|---|
  | Save (保存) | clears dirty flag; untitled project behaves as Save As | overwrites the current file |
  | Save As > Save As (另存为) | open project takes the new name/location | new file written; original file unchanged (keeps its last saved state) |
  | Save As > Save a Copy (保存副本) | open project keeps its name, location and dirty state | copy written; the copy is not opened |
  | Save As > Save a Copy As XML (将副本另存为 XML) | unchanged | `.aepx` copy, see AE-PRJ-005 |
  | Save As > Save a Copy As <previous major version> (将副本另存为 <以前的主版本号>) | unchanged | down-saved copy, see AE-PRJ-006 |
  - Saving an `.aepx` as `.aep` (or vice versa) via Save As converts the format.
  - File paths of footage are stored both absolute and relative to the project file (used by relinking, AE-PRJ-073).
  - Save is undo-neutral: it does not create or clear undo history.
  - Scripting: `Project.save([file])` (same as Save / Save As when a file is passed), `Project.saveWithDialog()` returns false if the user cancels.
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae, bae-ui-motion)
- **Notes:** Shortcuts: Save Ctrl/Cmd+S; Save As Ctrl+Shift+S / Cmd+Shift+S (see ui-preview-render-io.md, shortcuts). BAE decision: because BAE is CRDT-based with per-client update logs (ARCHITECTURE 4.6), "Save" compacts the Loro snapshots of the edited sublayers; the dirty flag means "changes not yet compacted/flushed to the package". Save a Copy must write a self-consistent package without touching the open package's update logs.

### AE-PRJ-005 · Save a Copy As XML (将副本另存为 XML)

- **What it does:** Writes a copy of the project as a text-based `.aepx` XML file meant for automated workflows.
- **Exact semantics:**
  - The XML file stores much of the project as hex-encoded binary blobs; human-readable/editable strings include marker attributes (comments, chapter and cue-point parameters), file paths of footage and proxy sources, composition/footage/layer/folder names and comments.
  - Footage item names appear as strings only when customised (otherwise derived from the file name); solid names are not exposed. Workspace and view names appear but edits to them are ignored on open.
  - An `.aepx` opens exactly like an `.aep`.
- **Priority:** P2 — interchange/automation only.
- **Owner:** bae-model (+ apps/bae)
- **Notes:** BAE decision: the BAE counterpart is `bae export-json` (ARCHITECTURE 4.6), which exports any sublayer as readable JSON; BAE does not need an XML format. AE's XML is not a stable or documented schema.

### AE-PRJ-006 · Save a Copy As previous major version (将副本另存为 <以前的主版本号>)

- **What it does:** Writes a copy in the file format of the previous major AE version so it can be opened there.
- **Exact semantics:** Only the immediately previous major version is offered (e.g. AE 27.0 offers "Save a Copy As 26.x"). Features that the older version does not support are dropped from the copy (AE does not warn per feature). The open project is unchanged.
- **Priority:** P2
- **Owner:** bae-model
- **Notes:** UNVERIFIED: exact menu label in AE 27.0 ("Save a Copy As 26.x"). BAE decision: BAE format versions are declared in `project.json`; a down-save/export to an older BAE format version, if ever offered, must list dropped features in a report instead of dropping silently.

### AE-PRJ-007 · Increment and Save (递增和保存)

- **What it does:** Saves the project under a new automatically numbered name in the same folder.
- **Exact semantics:**
  - The new file is written in the same folder as the current project file. Its name is the current base name followed by a number; if the base name already ends in a number, that number is increased by 1 (e.g. `shot_v07.aep` -> `shot_v08.aep`; digit count is preserved when possible).
  - UNVERIFIED: when the base name has no trailing number AE appends " 2" (with a space), e.g. `shot.aep` -> `shot 2.aep`.
  - UNVERIFIED: after the command the open project is the new incremented file (title bar shows the new name) and the previous file keeps its last saved state; Adobe's help text calls the result a "copy".
  - If the target name already exists, AE keeps incrementing until it finds an unused name. UNVERIFIED.
  - Disabled for an untitled (never saved) project, which uses Save As instead. UNVERIFIED.
- **Priority:** P1 — widely used versioning habit.
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Shortcut Ctrl+Alt+Shift+S / Cmd+Opt+Shift+S. BAE decision: BAE has built-in version history (ARCHITECTURE 7); Increment and Save should still exist because users rely on visible versioned files for hand-off.

### AE-PRJ-008 · Revert (还原（译名待核）)

- **What it does:** Discards all changes since the last save and reloads the saved file.
- **Exact semantics:** File > Revert asks for confirmation, then reloads the project from disk; the undo history is cleared. Disabled when the project is untitled or not dirty. Image caches are discarded for items whose source changed.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** UNVERIFIED: whether Revert is enabled for a non-dirty project. BAE decision: in collaborative mode Revert must only revert the local author's unsaved ops (ARCHITECTURE 4.5), never remote collaborators' changes; in single-user mode it reloads the last saved snapshot.

### AE-PRJ-009 · Dirty state and revision counter (项目已修改状态（译名待核）)

- **What it does:** Tracks whether the project has unsaved changes and counts edits.
- **Exact semantics:**
  - `dirty` is true after any undoable change (and some non-undoable ones, such as Interpret Footage changes or render-queue edits) since the last save; dirty projects show `*` after the name in the window title.
  - `revision`: integer, starts at 1 for a new project, increases on every user action; read-only.
  - Closing, quitting, opening another project or New Project prompts Save / Don't Save / Cancel when dirty.
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting `Project.dirty` (read-only, added in AE 17.5 (CC 2020); officially undocumented, listed by the docsforadobe guide), `Project.revision` ("every user action increases the revision number; a new project starts at revision 1", scripting guide). UNVERIFIED: which non-undoable actions set dirty. Implementation hint: derive dirty from "op log length since last flush" per sublayer; revision = monotonic op counter (useful for cache keys).

### AE-PRJ-010 · Auto-Save (自动保存)

- **What it does:** Periodically writes backup copies of the open project (saved or untitled) to an Auto-Save folder, keeping a limited number of versions.
- **Exact semantics:** Preferences > Auto-Save (首选项 > 自动保存):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Save Every (保存间隔) | bool + int | on, 20 | UNVERIFIED: 1-? | minutes | interval counted from the last auto-save, not the last manual save |
  | Save when starting render queue (启动渲染队列时保存) | bool | UNVERIFIED: on | — | — | saves when render queue items are added/modified/started |
  | Ask to recover an auto-saved project if After Effects closes unexpectedly (当 After Effects 意外关闭时，询问是否恢复自动保存的项目) | bool | UNVERIFIED: on | — | — | marked beta; see AE-PRJ-011 |
  | Maximum Project Versions (最大项目版本数) | int | 5 | UNVERIFIED: 1-99 | count | per project; oldest auto-save deleted when exceeded |
  | Auto-Save Location (自动保存位置) | enum | Next to Project | Next to Project (项目旁边（译名待核）) / Custom Location (自定义位置) | — | Custom takes a folder |
  - Location "Next to Project": a folder named "Adobe After Effects Auto-Save" (UNVERIFIED exact name) is created in the project's folder; backup files are named `<project name> auto-save <n>.aep` with n cycling up to Maximum Project Versions (UNVERIFIED naming).
  - Untitled projects are also auto-saved ("improved Auto-Save" of AE 27.0, Nov 2026, first in the AE Beta from Sep 2026; older versions only auto-saved saved projects) to the custom or default auto-save location.
  - Auto-save never overwrites the user's project file and does not clear the dirty flag. Auto-save applies to local projects only.
  - Projects written by the newest auto-save feature do not open in earlier AE versions.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** UNVERIFIED: the defaults marked above (20 min and 5 versions are the long-standing AE defaults). "Improved Auto-Save" with crash recovery and untitled-project auto-save is listed in What's New for AE 27.0 (Nov 2026), after an AE Beta preview (build 27.0.044, 2026-09-15). Source: https://helpx.adobe.com/after-effects/desktop/work-with-projects/after-effects-projects/auto-save-and-crash-recovery.html BAE decision: BAE's incremental update logs (ARCHITECTURE 4.6) are append-only and already crash-safe; auto-save in BAE means periodic snapshot compaction plus a bounded set of rollback snapshots in `cache/` or the custom location, with the same user-facing settings.

### AE-PRJ-011 · Crash recovery (崩溃恢复)

- **What it does:** After an unexpected quit, offers to reopen the most recent auto-save.
- **Exact semantics:** If the preference in AE-PRJ-010 is on, the next launch offers to open the most recent auto-save. Any auto-save, however opened, opens as an untitled "Recovered Project" (恢复的项目); Save then shows the Save As dialog so the original project is not overwritten unless the user chooses it.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** Introduced with the improved Auto-Save in AE 27.0 (Nov 2026); the preference is labelled "(Currently in beta)" in Preferences. BAE decision: BAE can recover to the last op in the update log rather than the last auto-save; it should still open the recovered state as a distinct, clearly-labelled document until the user confirms.

### AE-PRJ-012 · Project templates (.aet) (模板项目)

- **What it does:** A template project is a project file that, when opened, yields a new untitled project with the template's contents and settings.
- **Exact semantics:**
  - `.aet` = same content as `.aep`; renaming `.aep` to `.aet` (or Save a Copy then rename) makes a template. Opening an `.aet` creates an untitled project; saving never writes back into the template.
  - Preferences > Project > New Project loads template (新建项目加载模板, default off) + Choose Project Template (选择项目模板): New Project then opens a copy of the chosen `.aet`, `.aep` or `.aepx` file. Used to preset color management, folder structure, render settings etc.
  - AE no longer installs sample template projects.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-model)
- **Notes:** BAE decision: a template is a read-only `.bae`/`.baez` copied into a new untitled project; item IDs may be kept (no collisions in a fresh project).

### AE-PRJ-013 · Project metadata (XMP) (项目元数据（译名待核）)

- **What it does:** Stores XMP metadata (title, label, creator, etc.) on the project, editable in the Metadata panel and by scripts; passed to rendered files when the output module option is on.
- **Exact semantics:** `Project.xmpPacket`: RDF/XML string, read/write. Contents are free-form XMP; AE adds its own fields (e.g. creation/modify dates). Footage XMP is read on import and can be shown as Project panel columns (AE-PRJ-017). Output modules can include project XMP in rendered files (see ui-preview-render-io.md, Render Queue).
- **Priority:** P2
- **Owner:** bae-model
- **Notes:** BAE decision: store as an opaque key/value metadata map on the Project Prim; XMP serialisation only at export time.

## Project items and folders

### AE-PRJ-014 · Project item common attributes (项目项)

- **What it does:** Every entry in the Project panel (composition, footage, folder) is an item with a name, comment, label, ID, parent folder and selection state.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | name (名称) | string | type-dependent ("Comp 1", file name, "Black Solid 1", "Untitled 1" folder) | non-empty in UI | — | not required to be unique; for file footage the display name is independent of the file name |
  | comment (注释) | string | "" | ≤ 15,999 bytes after encoding conversion | — | user-only, no effect on rendering; shown in the Comment column |
  | label (标签) | int | per-type default from Preferences > Labels > Label Defaults | 0 = None, 1-16 = label colours | — | see project-comp-layers.md, Labels |
  | id | int | assigned | positive | — | AE-PRJ-002 |
  | parentFolder | FolderItem | root folder or the selected folder at creation | — | — | AE-PRJ-018 |
  | selected | bool | false | — | — | multiple selection allowed |
  | typeName | string | — | "Composition" / "Footage" / "Folder" (localised: 合成 / 素材 / 文件夹) | — | read-only, derived |
  | guides | array | [] | — | — | per-item viewer guides; see ui-preview-render-io.md, guides |
  - Rename: select item, Enter/Return or context menu Rename (重命名); undoable.
  - Item order inside a folder is a Project-panel sort only (by any column, toggled ascending/descending) and has no effect on rendering.
  - `Item.remove()` deletes the item (AE-PRJ-020).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Item.name/comment/label/id/parentFolder/selected/typeName`, `Item.dynamicLinkGUID` (string GUID used by Dynamic Link; BAE may ignore). The Comment column heading can be renamed per user ("Rename This"), which is a UI preference, not item data. BAE decision: names are stored on the Prim as a display attribute separate from the Prim path segment so duplicate names are allowed like AE.

### AE-PRJ-015 · Audio/visual item attributes (AVItem) (音视频项目属性（译名待核）)

- **What it does:** Compositions and footage items share a set of size/time attributes that layers and expressions read.
- **Exact semantics:**
  | Parameter (中文) | Type | Range | Notes |
  |---|---|---|---|
  | width / height (宽度 / 高度) | int | 1..30000 px | comp: read/write; footage: from source, writable only for solids |
  | pixelAspect (像素长宽比) | float | 0.01..100.0 | UI shows 2 decimals; stored exactly (e.g. UI 0.91 = 0.909091, 1.09 = 1.09402, 1.21 = 1.21212, 1.33 = 1.33333, 1.46 = 1.45869) |
  | duration (持续时间) | float s | 0..10800 | still footage: 0; comp: read/write; footage: from main source after interpretation (conform, pulldown, loop) |
  | frameRate (帧速率) | float fps | 1..999 (UI maximum per Adobe help; the scripting guide still documents 1..99) | footage: = `mainSource.displayFrameRate`; change via conform frame rate |
  | frameDuration | float s | 1/999..1 | exact reciprocal of frameRate |
  | hasVideo / hasAudio | bool | — | read-only |
  | footageMissing | bool | — | true for placeholders and missing files |
  | usedIn | CompItem[] | — | every comp that has a layer (or another reference, e.g. track matte source) using this item; snapshot, not live |
  | useProxy / proxySource | bool / FootageSource | — | AE-PRJ-074 |
  | time | float s | — | current time when previewed from the Project panel/Footage viewer |
  - A still footage item's duration is 0; layers made from it get the still-footage default duration (AE-PRJ-043) or the comp duration.
  - `usedIn` is the basis of in-use warnings (AE-PRJ-020), Reveal in Composition (AE-PRJ-022), Remove Unused Footage and Reduce Project.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Scripting `AVItem.*`; `isMediaReplacementCompatible` (AE 18.0) relates to Essential Graphics media replacement (see ui-preview-render-io.md, Essential Graphics). BAE decision: frame rates are stored as exact rationals (e.g. 30000/1001) and durations as flicks; the AE float view is derived. Expressions see these via `comp`/`footage` objects (see transform-keyframes-expressions.md, expression language).

### AE-PRJ-016 · Footage item and footage sources (素材项目)

- **What it does:** A footage item wraps one main source (file, solid or placeholder), an optional proxy source, and the interpretation settings that turn decoded media into comp-ready frames.
- **Exact semantics:**
  - Source kinds (`FootageItem.mainSource`):
    | Kind | Scripting | Typical name | isStill | Notes |
    |---|---|---|---|---|
    | File (文件) | `FileSource` (`file`, `missingFootagePath`, `reload()`) | file name, or sequence pattern `name_[0000-0099].png` | true for single images | movie, audio, image, image sequence, vector/layered stills, data files |
    | Solid (纯色) | `SolidSource` (`color` [r,g,b] 0..1) | "<Color> Solid <n>" | true | AE-PRJ-079 |
    | Placeholder (占位符) | `PlaceholderSource` | user name | duration 0 => still | AE-PRJ-081 |
  - Shared interpretation fields on every source (`FootageSource`): `hasAlpha`, `alphaMode`, `invertAlpha`, `premulColor`, `nativeFrameRate`, `conformFrameRate`, `displayFrameRate`, `fieldSeparationType`, `highQualityFieldSeparation`, `removePulldown`, `loop`, `isStill`; defined in the Interpret Footage section.
  - `FootageItem.file`: the main source file (null for solids/placeholders).
  - Video/audio-only files produce items with `hasVideo`/`hasAudio` set accordingly; an audio-only item has width/height of 0 in the UI. UNVERIFIED: scripting reports width/height 0 for audio-only footage.
  - Footage can be trimmed in the Footage viewer and inserted (Ripple Insert / Overlay Edit), see ui-preview-render-io.md, Footage viewer.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Scripting `FootageItem.mainSource`, `replace()`, `replaceWithPlaceholder()`, `replaceWithSequence()`, `replaceWithSolid()`, `openInViewer()`. BAE decision: maps to the `Asset` Prim (ARCHITECTURE 4.2) with `source` (file reference per ARCHITECTURE 16 / solid colour / placeholder spec), `interpretation`, and `proxy` sub-records. Data files (JSON/CSV/TSV/MGJSON) are footage items without video (see transform-keyframes-expressions.md, data footage). Lottie/bodymovin: project items have no Lottie object of their own; footage referenced by exported layers becomes an entry of the root `assets` array (`id` string, referenced by the layer's `refId`): image asset `{id, w, h, u, p, e}` (`e: 1` = embedded, `p` is then a base64 data URL and `u` is empty; `e: 0` = external file `u` + `p`; optional `t: "seq"` marks one frame of an image sequence), sound asset (same file fields), precomposition asset `{id, layers, fr?}` for comps, data source asset `t: 3`; solids produce no asset (solid layer `ty: 1` carries `sc`, `sw`, `sh`), folders, comments and labels are not exported. Source: https://lottiefiles.github.io/lottie-docs/assets/

### AE-PRJ-017 · Project panel column data (项目面板列数据（译名待核）)

- **What it does:** Each item exposes read-only derived values shown as sortable Project panel columns and in the information header above the item list.
- **Exact semantics:**
  | Column (中文) | Value | Applies to |
  |---|---|---|
  | Name (名称) | item name | all |
  | Label (标签) | label colour index | all |
  | Type (类型) | "Composition", "Folder", "Solid", "Placeholder", or the file format/codec family (e.g. "QuickTime", "PNG Sequence", "Photoshop") | all |
  | Size (大小) | file size on disk (bytes, shown in KB/MB/GB); sequences: total | file footage |
  | Media Duration (媒体持续时间（译名待核）) | duration in the project time display style; Δ notation | comps, time-based footage |
  | File Path (文件路径（译名待核）) | absolute path of main source | file footage |
  | Date (日期（译名待核）) | file modification date | file footage |
  | Comment (注释) | item comment | all |
  | Frame Rate (帧速率) | displayFrameRate | comps, time-based footage |
  | In Point / Out Point (入点 / 出点) | UNVERIFIED: source in/out from embedded timecode | time-based footage |
  | Tape Name, Video Info, Audio Info, Status | UNVERIFIED: present in the Columns menu in recent versions | footage |
  - Information header (thumbnail area) for a selected footage item: name and usage count ("used N times"), `W x H (PAR)`, `Δ duration, fps`, colour depth descriptor ("Millions of Colors" = 8-bit RGB, "Millions of Colors+" = 8-bit RGBA, "Trillions of Colors" = 16-bit, "Floating Point" = 32-bit float) with alpha interpretation in parentheses (e.g. "(Straight)", "(Premultiplied)"), codec name, audio "48.000 kHz / 32 bit U / Stereo", and the embedded colour profile when present. For a comp: `W x H (PAR)`, `Δ duration, fps`. UNVERIFIED: exact header strings.
  - Project metadata (XMP) fields can be added as extra columns (Columns > Project Metadata).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media, bae-ui-motion)
- **Notes:** Panel behaviour (show/hide/reorder/resize columns, sort, search/filter, thumbnail) is specified in ui-preview-render-io.md, Project panel. Alt/Option-click on a footage item shows the file creator ID (macOS type/creator). BAE decision: column values are computed from Asset metadata cached at import, not by re-probing files on every repaint.

### AE-PRJ-018 · Folders (文件夹)

- **What it does:** Folders organise items in the Project panel; they can be nested to any depth and have no effect on rendering.
- **Exact semantics:**
  - Create: File > New > New Folder (新建文件夹) or the Create a New Folder button; the new folder is created inside the currently selected folder (or at the root), named "Untitled 1", "Untitled 2", ... and put into rename mode. UNVERIFIED: default name pattern.
  - Move: drag items/folders into a folder; drag to the information area at the top of the panel to move to the root. A folder cannot be moved into itself or its descendants. Moving is undoable. Scripting: set `Item.parentFolder`.
  - Import with a folder selected places imported items in that folder; Import Folder creates a folder (AE-PRJ-026).
  - `FolderItem.items` / `numItems` list only direct children; `FolderItem.item(i)` is 1-based.
  - Deleting, duplicating and collecting folders: see Delete items and folders, Duplicate items, and Collect Files (only comps, not folders, count as "selected comps").
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `ItemCollection.addFolder(name)`, `FolderItem`. Team Projects did not allow nested folders (removed feature). BAE decision: `Folder` Prims; folder membership is a pure organisational attribute so moving never invalidates render caches.

### AE-PRJ-019 · Duplicate and copy/paste items (重复 / 复制)

- **What it does:** Creates copies of selected project items in the same project.
- **Exact semantics:**
  - Edit > Duplicate (重复, Ctrl/Cmd+D) with Project panel focus duplicates each selected item into the same folder, placed right after the original and selected.
  - Duplicating a composition is shallow: the new comp contains copies of the layers (with all properties, keyframes, expressions, masks, effects, markers), but nested comps and footage are referenced, not duplicated. Comp-level data (settings, markers, work area, guides) is copied.
  - Name of a duplicate: a trailing number in the name is incremented to the next unused value (e.g. "Comp 1" -> "Comp 2"); if there is no trailing number, " 2" is appended. UNVERIFIED: footage duplicates may keep the same name instead (footage items commonly show the same file name twice).
  - Duplicating a footage item copies its main source reference, interpretation and proxy settings; the file is not copied.
  - Duplicating a folder duplicates the folder and (recursively) its contents. UNVERIFIED.
  - Edit > Copy then Paste with Project panel focus behaves like Duplicate into the selected folder. UNVERIFIED.
  - Expressions in a duplicated comp that refer to the comp by name are not renamed.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Duplicating a footage item to the desktop by dragging copies the file in the OS (out of scope for BAE M1). BAE decision: "Duplicate comp deep" (duplicate nested comps too) is a common third-party script; consider offering it as a separate command.

### AE-PRJ-020 · Delete items and folders (删除项目（译名待核）)

- **What it does:** Removes items from the project, removing any layers that use them.
- **Exact semantics:**
  - Select items and press Delete/Backspace (or Edit > Clear). If any selected footage/comp is used in compositions (`usedIn` non-empty), AE shows a warning stating that the item(s) are used in compositions and asks to confirm; confirming removes every layer that uses the item from every comp. UNVERIFIED: exact dialog text and whether the count of uses is shown.
  - Deleting a folder deletes all of its contents recursively; a non-empty folder triggers a confirmation. UNVERIFIED: whether the confirmation also appears when the contents are unused.
  - Deleting a comp removes all precomp layers referencing it.
  - The operation is a single undo step that restores the items, their IDs and all removed layers in their original stacking positions.
  - Render queue items whose comp is deleted are removed from the render queue.
  - Deleting a footage item used as a proxy elsewhere is not possible (proxies are sources, not items).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Item.remove()` (no warning). Expressions that referenced deleted layers/comps break and show errors (see transform-keyframes-expressions.md, expression errors). Bulk variants: AE-PRJ-096 (Remove Unused Footage), AE-PRJ-097 (Consolidate All Footage), AE-PRJ-095 (Reduce Project).

### AE-PRJ-021 · Solids folder (纯色文件夹)

- **What it does:** A designated folder into which every new solid footage item (from solid, null and adjustment layers) is placed automatically.
- **Exact semantics:**
  - The project has at most one designated solids folder. It is created on demand, at the root, the first time a solid is created; its default name is "Solids" (纯色), taken from Preferences > Project > New Project Solids Folder (新建项目纯色文件夹) at project creation.
  - Since AE 23.x ("enhanced solids folder organisation"): the solids folder can be renamed; on the next solid creation AE asks whether to keep using the renamed folder (Yes) or create a new folder with the default name (No). It can be nested in other folders and keeps receiving new solids. Any folder can be made the solids folder via its context menu (Solids Folder (纯色文件夹)); only one at a time.
  - If the designated folder was deleted, a new one is created at the root on the next solid creation.
  - New null objects and adjustment layers create solid footage items too and go to the same folder.
- **Priority:** P1
- **Owner:** bae-model (+ bae-motion)
- **Notes:** UNVERIFIED: AE version of enhanced solids folder (believed AE 23.0 (2022)). The preference shows the current project's solids-folder name for reference; changing it affects new projects only. BAE decision: BAE null and adjustment layers need no footage source (they are layer types); BAE should still create a solid Asset for Solid layers so that "Affect all layers that use this solid" (AE-PRJ-080) works, but need not create assets for nulls/adjustments unless exported to AE.

### AE-PRJ-022 · Reveal usage (在合成中显示 / 在项目中显示图层源)

- **What it does:** Navigates between an item and the layers that use it.
- **Exact semantics:**
  - Project panel context menu > Reveal in Composition (在合成中显示): submenu lists each use as "<comp name>, <layer name>" (from `usedIn` plus layer scan); choosing one opens that comp, selects the layer and moves the CTI into the layer's span if needed. UNVERIFIED: CTI behaviour.
  - Timeline layer context menu > Reveal Layer Source in Project (在项目中显示图层源): selects the layer's source item in the Project panel, expanding folders.
  - Reveal Composition in Project for a comp's Timeline (UNVERIFIED menu name).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Requires a reverse index source item -> layers; BAE should keep it incrementally (also used by in-use warnings and Reduce Project).

### AE-PRJ-023 · Project Flowchart (项目流程图)

- **What it does:** A read-only node graph of comps, footage and layers with arrows showing which item feeds which comp.
- **Exact semantics:** Project flowchart (Ctrl/Cmd+F11) or Composition Flowchart (Composition > Composition Flowchart) shows tiles for comps, footage items and (when expanded) layers; arrows show nesting/use. Mid-gray lines indicate the Video/Audio switch of that layer is off. Selecting a tile selects the item/layer in the Project/Timeline panel; Delete deletes the selected item or layer; relationships cannot be edited in the flowchart.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Panel UI belongs with ui-preview-render-io.md panels; listed here because it is a view of project-item relationships. BAE decision: a dependency graph view is cheap to provide from the same reverse index as AE-PRJ-022.

## Import

### AE-PRJ-024 · Import File dialog (导入文件)

- **What it does:** File > Import > File (Ctrl/Cmd+I) opens a file browser that creates footage items (or compositions) from one or more selected files, referencing them in place.
- **Exact semantics:**
  - Also opened by double-clicking an empty area of the Project panel, or via a folder's context menu > Import (imports into that folder). Otherwise new items go into the selected folder (or root).
  - The file content is never copied into the project; the item stores a reference (absolute + relative path) to the file.
  - Dialog options (shown depending on the selected file):
    | Parameter (中文) | Type | Default | Notes |
    |---|---|---|---|
    | Import As (导入为) | enum | Footage for flat files; for layered files the last used value (UNVERIFIED) | Footage (素材) / Composition (合成) / Composition - Retain Layer Sizes (合成 - 保持图层大小); AE-PRJ-029 |
    | <format> Sequence (序列) | bool | sticky: AE remembers the last state | AE-PRJ-038 |
    | Force alphabetical order (强制按字母顺序排列) | bool | off | AE-PRJ-039 |
    | Multiple Sequences (多个序列) | bool | off | imports each selected file's sequence as its own item |
    | Create Composition (创建合成（译名待核）) | bool | off | UNVERIFIED: present in recent versions; creates a comp from the imported item(s) |
    | Files of type / Enable (文件类型) | enum | All Acceptable Files | "All Footage Files" excludes project files |
  - The bottom of the dialog shows what will be imported (e.g. the detected sequence range).
  - Windows only: selecting a folder and pressing Import Folder imports the folder (AE-PRJ-026).
  - Selecting several files imports each as a separate item (unless they form a sequence and Sequence is on).
  - If a file has an alpha channel without an alpha label and the Interpret Unlabeled Alpha As preference (AE-PRJ-042) is Ask User, the Interpret Footage dialog opens for that file during import.
  - Layered PSD/AI/PDF/EPS files imported as Footage open the Layer Options dialog (AE-PRJ-030).
  - The undo step "Import" covers the whole batch.
- **Priority:** P0
- **Owner:** bae-media (+ bae-model, bae-ui-motion)
- **Notes:** Scripting `Project.importFile(ImportOptions)`, `ImportOptions.importAs` (`ImportAsType.FOOTAGE | COMP | COMP_CROPPED_LAYERS | PROJECT`), `ImportOptions.sequence`, `forceAlphabetical`, `rangeStart`/`rangeEnd` (sub-range), `canImportAs()`; `Project.importFileWithDialog()`; `Project.setDefaultImportFolder(folder)`. Supported formats: see ui-preview-render-io.md, import/export format lists. BAE decision: the importer probes files asynchronously (metadata first, decode on demand) so a large batch import never blocks the UI.

### AE-PRJ-025 · Import Multiple Files and Import Recent Footage (导入多个文件 / 导入最近的素材（译名待核）)

- **What it does:** Import Multiple Files keeps the import dialog open for repeated selections; Recent Footage re-imports a recently imported file.
- **Exact semantics:**
  - File > Import > Multiple Files (Ctrl+Alt+I / Cmd+Opt+I): after each Open, the dialog reappears until Done/Cancel; all imports land in the selected folder.
  - File > Import Recent Footage: lists recently imported files (most recent first); choosing one imports it again as a new item with default interpretation. UNVERIFIED: list length 10.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** —

### AE-PRJ-026 · Import folders (导入文件夹)

- **What it does:** Imports the contents of a disk folder either as one image sequence or as individual items inside a new project folder.
- **Exact semantics:**
  - Dragging a disk folder from Finder/Explorer into the Project panel imports its contents as a still-image sequence (single footage item) when the files form a sequence.
  - Alt/Option-dragging a folder imports every file as an individual item inside a new project folder named after the disk folder, preserving sub-folder structure. UNVERIFIED: sub-folder recursion.
  - Windows Import dialog "Import Folder" button: same as Alt-drag. UNVERIFIED.
  - Files AE cannot import are skipped; AE reports them. UNVERIFIED: whether skipped files are reported.
- **Priority:** P1
- **Owner:** bae-media (+ bae-model)
- **Notes:** —

### AE-PRJ-027 · Drag-and-drop import (拖动导入)

- **What it does:** Files dragged from the OS into AE are imported; dropping into a comp also creates layers.
- **Exact semantics:**
  - Drop on the Project panel: import into the folder under the pointer (or root).
  - Drop on a Composition viewer or Timeline: import, then add each imported item as a layer in the active comp. Viewer drop: layer centred where it would be for Add to Composition (UNVERIFIED: drop position is ignored and layer is centred). Timeline drop: inserted at the drop row in the layer stack; layer In point at the CTI when dropped on the layer area (UNVERIFIED).
  - Layered files dropped in are imported per the Drag Import Multiple Items As preference (AE-PRJ-028) without showing the Layer Options dialog.
  - Dragging a Render Queue output module into the Project panel imports its output file, or a placeholder if not yet rendered (AE-PRJ-083).
  - Dragging items from the Project panel into a comp is layer creation, not import (see project-comp-layers.md, layer creation).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** —

### AE-PRJ-028 · Drag Import Multiple Items As preference (拖拽导入多个项目为)

- **What it does:** Chooses how layered files are imported when dragged in (no dialog is shown).
- **Exact semantics:** Preferences > Import > Drag Import Multiple Items As:
  | Value (中文) | Meaning |
  |---|---|
  | Footage (素材) | merged single footage item |
  | Composition (合成) | comp with document-size layers |
  | Composition - Retain Layer Sizes (合成 - 保留图层尺寸) | comp with cropped layers |
  Default: UNVERIFIED: Composition - Retain Layer Sizes. The current help text says it controls what AE creates "when you drag multiple files into a composition"; older AE manuals describe the same menu as deciding whether layered still-image files dragged into AE come in as compositions.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** The task brief's "Default Drag Import As" is this preference; AE has used the label "Drag Import Multiple Items As" since at least the CS-era manuals (the name was not changed in the AE 26 preferences refresh). Older versions listed only Composition and Composition - Retain Layer Sizes; current help also lists Footage. Source: https://helpx.adobe.com/after-effects/desktop/get-started/preferences-and-settings/preferences.html

### AE-PRJ-029 · Import As Composition for layered files (导入为合成)

- **What it does:** Imports a layered PSD, AI, PDF/EPS (or SVG, AE-PRJ-036) file as a composition whose layers reference per-layer footage items, either all at document size or cropped to each layer's bounds.
- **Exact semantics:**
  - Results: one comp named after the file (without extension) and a folder "<file name> Layers" containing one footage item per source layer (name "<layer name>/<file name>"). UNVERIFIED: folder and item naming patterns.
  - **Composition**: every layer footage item has the document size; layer anchor points are at the document centre; positions are the document centre.
  - **Composition - Retain Layer Sizes**: every layer footage item is cropped to that layer's non-transparent bounds; the layer's anchor point is at the centre of its bounds and its Position puts it at its original location. Layers that extend beyond the canvas keep their full extent. UNVERIFIED: whether pixels outside the canvas are kept for PSD.
  - Comp settings: width/height = document size (rounded up to integers for vector files); PAR from the document (PSD pixel aspect metadata, else 1); frame rate and duration from the last-used comp settings; duration of every layer = comp duration. UNVERIFIED: frame rate/duration source.
  - Layer order, names, visibility (hidden source layers import with Video switch off), opacity, blend modes and other mappings: AE-PRJ-032 (PSD), AE-PRJ-034 (AI/PDF/EPS).
  - Image sequences of layered files imported as a composition: each source layer becomes its own sequence footage item.
- **Priority:** P1
- **Owner:** bae-media (+ bae-model, bae-motion)
- **Notes:** Scripting `ImportAsType.COMP` / `COMP_CROPPED_LAYERS`. BAE decision: per-layer footage items reference the same file plus a layer selector (`layerName`/`layerIndex`) rather than extracting files.

### AE-PRJ-030 · Layer Options for layered footage import (图层选项)

- **What it does:** When a layered file is imported as Footage, chooses whether to merge all layers or take one layer, and at what dimensions.
- **Exact semantics:** Dialog shown after Import As Footage for PSD/AI/PDF/EPS:
  | Parameter (中文) | Type | Default | Values / Notes |
  |---|---|---|---|
  | Layer Options (图层选项) | enum | Merged Layers | Merged Layers (合并的图层) / Choose Layer (选择图层) + layer popup |
  | Layer Styles (图层样式) | enum | Merge Layer Styles into Footage | only with Choose Layer for PSD: Merge Layer Styles into Footage (合并图层样式到素材) / Ignore Layer Styles (忽略图层样式（译名待核）) |
  | Footage Dimensions (素材尺寸) | enum | Document Size | only with Choose Layer: Document Size (文档大小) / Layer Size (图层大小) |
  - Merged PSD: transparency and layer masks of all layers are merged into one alpha that is premultiplied with white (AE sets the interpretation accordingly). Single PSD layer: layer mask combined with transparency, interpreted as straight alpha.
  - A merged footage item can later be converted to a composition: File > Replace Footage > With Layered Comp (all uses) or Layer > Convert to Layered Comp (one layer) (AE-PRJ-033).
- **Priority:** P1
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** UNVERIFIED: the exact Layer Styles values for single-layer import; the defaults. Scripting has no layer selection in `ImportOptions` (merged only).

### AE-PRJ-031 · Editable Layer Styles vs Merge Layer Styles into Footage (可编辑的图层样式 / 合并图层样式到素材)

- **What it does:** When importing a PSD as a composition, chooses whether Photoshop layer styles become live AE layer styles or are baked into each layer's pixels.
- **Exact semantics:**
  - Shown in the composition import dialog (Layer Options) for PSD files that contain layer styles. Default: Editable Layer Styles. UNVERIFIED default.
  - Editable Layer Styles (可编辑的图层样式): supported styles (Drop Shadow, Inner Shadow, Outer Glow, Inner Glow, Bevel and Emboss, Satin, Color Overlay, Gradient Overlay, Pattern Overlay, Stroke, Blending Options) are created as AE Layer Styles with the same values (see project-comp-layers.md, Layer styles). A layer with layer styles breaks 3D intersections and shadow casting (Classic 3D). UNVERIFIED: Pattern Overlay is not editable in AE.
  - Merge Layer Styles into Footage (合并图层样式到素材): styles rendered by the Photoshop engine into the layer footage; faster, may not match Photoshop exactly; does not interfere with 3D intersections/shadows.
- **Priority:** P1
- **Owner:** bae-media (+ bae-render)
- **Notes:** BAE decision: BAE has no Photoshop engine; "Merge" must be implemented by rendering BAE's own layer-style implementation into a cached image, or by reading the PSD's composite per-layer data if present.

### AE-PRJ-032 · Photoshop (PSD) composition import mapping (Photoshop 文件导入)

- **What it does:** Defines how each Photoshop document feature maps onto AE comps, layers and properties when importing as a composition.
- **Exact semantics:**
  | Photoshop feature | AE result |
  |---|---|
  | pixel layer | footage layer from a per-layer footage item |
  | layer position | layer Position (document- or bounds-centred per Import As) |
  | opacity | layer Opacity (%) |
  | fill opacity | UNVERIFIED: baked into the layer pixels / Blending Options in editable styles |
  | blend mode | same-named AE blend mode (see compositing-3d-color.md, blend modes); unsupported modes fall back to Normal |
  | visibility off | layer imported with Video switch off |
  | layer mask (raster) | merged into the per-layer footage alpha (straight) |
  | vector mask / clipping path | AE mask(s) on the layer (Add mode), editable and animatable |
  | layer group | nested composition (precomp) with the group's name; group opacity/blend mode on the precomp layer |
  | clipping group (clipping mask) | precomp of the clipped layers (comp-size) with Preserve Underlying Transparency on each clipped layer |
  | adjustment layer (Levels, Curves, Hue/Saturation, Brightness/Contrast, Color Balance, Invert, Threshold, Posterize, Channel Mixer, Photo Filter, Black & White, Vibrance, Selective Color, Exposure, Gradient Map...) | AE adjustment layer with the corresponding AE effect(s) where one exists (e.g. Levels, Curves, Hue/Saturation); unsupported adjustments are imported as an empty adjustment layer. UNVERIFIED: exact supported list |
  | fill layers (Solid Color / Gradient / Pattern) | rasterised footage layers (UNVERIFIED) |
  | text layer | footage layer of rasterised text; Layer > Create > Convert to Editable Text makes a text layer (AE-PRJ-035) |
  | Smart Object | single merged footage layer |
  | layer styles | AE-PRJ-031 |
  | image guides | composition guides |
  | video layer / 3D layer | not supported |
  | CMYK, LAB, Duotone, Monotone, Tritone layered file | imported as one flattened image |
  - Layer names stay linked: renaming a layer in Photoshop keeps the link on Reload; deleting a layer makes that layer item Missing; layers added after import are not picked up on Reload.
  - Only RGB and Grayscale layered PSDs keep layers.
- **Priority:** P1
- **Owner:** bae-media (+ bae-motion, bae-effects)
- **Notes:** Layer > New > Adobe Photoshop File creates an empty PSD at comp size, imports it and opens Photoshop (P2, BAE: not applicable). BAE decision: implement a PSD reader (layer records, masks, vector masks, groups, adjustment descriptors) in bae-media; adjustment-layer mapping depends on effects.md coverage. Large Document Format (PSB) UNVERIFIED supported.

### AE-PRJ-033 · Convert merged footage to layered comp (转换为分层合成)

- **What it does:** Replaces a merged layered-file footage item (or one layer's use of it) with an equivalent layered composition.
- **Exact semantics:** File > Replace Footage > With Layered Comp (图层合成): creates the layered comp (as Import As Composition) and replaces every use of the footage item with the new comp; Layer > Convert to Layered Comp: same, for the selected layer only. The layer keeps its transforms, effects and timing; the new precomp layer's source is the new comp. UNVERIFIED: which Import As variant (document size vs retain sizes) is used.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-media)
- **Notes:** —

### AE-PRJ-034 · Illustrator, PDF and EPS import (Illustrator / PDF / EPS 导入)

- **What it does:** Imports vector documents, rasterised by AE at the comp-required scale, as footage or as layered compositions.
- **Exact semantics:**
  - AI (AI, AI4, AI5, EPS, PS) and PDF are continuously rasterisable vector footage; without Continuously Rasterize (see compositing-3d-color.md, collapse/continuous rasterize) the layer is rasterised once at 100% of the document size and then transformed as pixels.
  - Empty areas become transparent (alpha generated). AE does not read embedded colour profiles from AI files; assign one in Interpret Footage > Color tab (AE-PRJ-057).
  - Fractional document sizes are rounded up to whole pixels (e.g. 216.5 x 275.5 -> 217 x 276; Adobe's help example prints "217x278", apparently a typo), which can leave a transparent/black row at the right/bottom edge.
  - Crop box: AI documents use the artboard (crop area) as the frame; when an AI document has several artboards, the first artboard defines the comp frame and art outside it is retained outside the comp frame only when imported as a layered composition. UNVERIFIED: bounding-box vs artboard rules for single-layer and merged imports.
  - PDF: only the first page is imported. A page-selection option is not offered (UNVERIFIED for AE 27).
  - AI blending modes are read only from files saved as Illustrator CS2 or earlier.
  - Interpret Footage > More Options for AI/PDF/EPS: Antialiasing More Accurate / Faster (AE-PRJ-056).
  - Layered import maps AI top-level layers to AE layers (sublayers are merged into their top-level layer).
  - Since AE 26.3 (June 2026) imported AI layers can be converted to shape layers preserving gradient fills, strokes and transparency (see shapes.md, Create Shapes from Vector Layer).
- **Priority:** P1
- **Owner:** bae-media (+ bae-vector)
- **Notes:** BAE decision: BAE can import PDF/AI (PDF-compatible) via a PDF parser into Vello paths and rasterise at any scale (true continuous rasterisation); EPS/PS (PostScript) import is P2 and may be dropped. Implementation hint: cache rasterisations per scale bucket.

### AE-PRJ-035 · Convert PSD and Illustrator text to editable text (转换为可编辑文字)

- **What it does:** Turns a text layer imported from a PSD (or, since AE 27.0, an Illustrator file) into a native AE text layer.
- **Exact semantics:**
  - PSD: select the imported footage layer whose source is a Photoshop text layer and choose Layer > Create > Convert to Editable Text. A new text layer replaces it, with font, size, tracking, leading, colour, paragraph settings, transform and point/paragraph type from the PSD; the original footage layer is turned off (Video switch off) and kept below. UNVERIFIED: whether the original layer is kept.
  - AI (AE 27.0, Nov 2026): Illustrator text converts to editable text layers preserving fonts, size, fills, positions, styles and variable-font settings.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-text, bae-media)
- **Notes:** Text layer model: see text.md.

### AE-PRJ-036 · SVG import (SVG 导入)

- **What it does:** Imports SVG files as rasterised footage or as a composition of editable shape layers.
- **Exact semantics:** Since AE 26.3 (June 2026): Import As Footage gives a vector footage item (rasterised like AI); Import As Composition creates a comp with native shape layers (paths, fills, strokes, gradients). Pasting SVG text/Illustrator clipboard content into a comp creates shape layers (Edit > Paste). UNVERIFIED: mapping of SVG groups/transforms/clip paths/masks and unsupported features.
- **Priority:** P1 — common hand-off format for motion designers; cheap with Vello/usvg.
- **Owner:** bae-media (+ bae-vector)
- **Notes:** BAE decision: use a usvg-class parser to produce BAE shape Prims (see shapes.md for the target model).

### AE-PRJ-037 · Import an After Effects project into a project (导入 After Effects 项目（译名待核）)

- **What it does:** Imports all items of another project file into the open project.
- **Exact semantics:** File > Import > File with an `.aep`/`.aepx`/`.aet` selected (Import As: Project): all items of that project are added inside a new folder named after the imported project file (e.g. "other.aep"), keeping its folder structure; every imported item gets a new ID; render queue items of the imported project are added to the render queue. Fonts/effects missing are reported as on open. The solids folder of the imported project is not merged with the current one. UNVERIFIED: render-queue and solids-folder behaviour.
- **Priority:** P1
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting `ImportAsType.PROJECT`. Import of Premiere Pro projects (`.prproj`: sequences become comps, clips become layers), Pro Import (AAF/OMF/FCP XML) and Dynamic Link footage are P2 and out of BAE M1. BAE decision: importing one `.bae` into another is a sublayer/Prim copy with ID remap; referencing instead of copying is a BAE-only feature (ARCHITECTURE 4.1 Reference).

## Image sequences and still-image defaults

### AE-PRJ-038 · Image sequence import and numbering (静止图像序列)

- **What it does:** Imports a numbered series of still images in one folder as a single footage item, one file per frame.
- **Exact semantics:**
  - Enabled by the "<format> Sequence" checkbox (e.g. "PNG Sequence", "OpenEXR Sequence") in the Import dialog. The checkbox state is sticky across imports. Selecting any one file of a sequence imports the whole sequence; dragging a disk folder imports its sequence.
  - Detection (AE-compatible rule set; partly UNVERIFIED):
    ```
    name = stem + ext
    find the LAST run of decimal digits in stem -> prefix, digits, suffix
      (AE scripting isFileNameNumbered() finds a digit run; examples: "image0120.png" -> 120,
       "ima0102ge.png" -> 102, "003image.png" -> 3)
    members = files in the same folder with the same prefix, suffix and extension (case-insensitive on
              macOS/Windows) whose middle part is all digits
    frame order = ascending integer value of the digits
    first frame = lowest number found (or rangeStart); last = highest (or rangeEnd)
    frame count = last - first + 1   (gaps become missing frames, AE-PRJ-041)
    ```
    UNVERIFIED: AE's exact choice when a name has several digit runs (e.g. "shot01_v02_0001.png": AE uses the last run); whether different zero-padding widths (frame9 / frame10) are treated as one sequence (believed yes).
  - Item name: `<prefix>[<first>-<last>]<suffix>.<ext>` with the original zero padding, e.g. `render_[0000-0249].png`.
  - Interpretation (alpha, PAR, colour profile, bit depth, size) is taken from the first image of the sequence; all frames are assumed to have the same size. UNVERIFIED: frames of a different size are scaled/centred or cropped.
  - Frame rate = Preferences > Import > Sequence Footage > Frames Per Second (AE-PRJ-044) unless overridden in Interpret Footage. Duration = frame count / frame rate.
  - Uncheck Sequence to import one numbered file as a single still.
  - Multiple Sequences (多个序列) checkbox: select files from different sequences in the same folder to import each as a separate sequence item.
  - Rendering a comp with a numbered sequence output uses the comp start frame number as the first output number (see ui-preview-render-io.md, Render Queue).
- **Priority:** P0
- **Owner:** bae-media (+ bae-model)
- **Notes:** Scripting `ImportOptions.sequence`, `FootageItem.replaceWithSequence(file, forceAlphabetical)`, `AVItem.setProxyWithSequence()`. BAE decision: store the sequence as `{dir, prefix, suffix, ext, padding, first, last, frame_list?}` so relinking and Collect Files can enumerate frames without listing directories; scanning must be lazy for network storage (AE also does not stat each frame by default).

### AE-PRJ-039 · Force alphabetical order (强制按字母顺序排列)

- **What it does:** Imports a set of files as a sequence ordered by file name, ignoring numeric gaps.
- **Exact semantics:** With Force alphabetical order on, every matching file becomes one consecutive frame in alphabetical (name) order; gaps in numbering do not create missing frames (e.g. Seq1, Seq2, Seq3, Seq5 -> 4 frames). It also allows sequences whose names are not numbered. Incompatible with a sub-range (scripting throws if `rangeStart`/`rangeEnd` are set together with `forceAlphabetical`). UNVERIFIED: sort is plain code-point order (so "Seq10" sorts before "Seq2"), not natural order.
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** Scripting `ImportOptions.forceAlphabetical`; `replaceWithSequence(file, forceAlphabetical)`.

### AE-PRJ-040 · Import a sequence sub-range (导入序列的子集)

- **What it does:** Imports only part of a numbered sequence.
- **Exact semantics:** In the Import dialog select the first wanted file, Shift-click the last wanted file, keep Sequence on, Open: the item covers only that number range. Scripting (undocumented) `ImportOptions.rangeStart`/`rangeEnd` (integers; frame numbers): ignored when `sequence` is false; throws when `forceAlphabetical` is true; a `rangeEnd` beyond the last existing file creates missing frames; `rangeStart > rangeEnd` throws and resets to the full range.
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** —

### AE-PRJ-041 · Missing frames in sequences (缺失帧)

- **What it does:** Handles gaps and unreadable files inside an imported image sequence.
- **Exact semantics:**
  - A number missing from the range is a missing frame; it is rendered as the missing-footage placeholder image (colour bars, AE-PRJ-082) in that frame only, with the item still usable.
  - If Preferences > Import > Report Missing Frames (报告缺失帧) is on, AE warns at import (and at open/reload) listing the count and the first missing frame. UNVERIFIED: exact message content.
  - By default AE does not stat every frame at import (fast import from network storage), so unresolved aliases/shortcuts or offline files inside the range are not reported until rendered; Verify Individual Files (验证个体文件, slower) checks each file.
  - Frames that exist but fail to decode render as UNVERIFIED: missing-frame colour bars / an error message in the Info panel.
- **Priority:** P1
- **Owner:** bae-media (+ bae-render)
- **Notes:** BAE decision: missing frames must never abort a render; the render log lists them (AE does the same in the render log only when reporting is on).

### AE-PRJ-042 · Interpret Unlabeled Alpha As preference (将未标记的 Alpha 通道解释为)

- **What it does:** Decides the alpha interpretation of imported files that have an alpha channel but no metadata saying straight or premultiplied.
- **Exact semantics:** Preferences > Import > Import Defaults:
  | Value (中文) | Behaviour at import |
  |---|---|
  | Ask User (询问用户) | Interpret Footage dialog appears for each such file |
  | Guess (猜测) | runs the Guess algorithm (AE-PRJ-048); if uncertain AE prompts or uses its best guess |
  | Ignore Alpha (忽略 Alpha 通道) | alphaMode = Ignore |
  | Straight (Unmatted) (直接（无蒙版）) | alphaMode = Straight |
  | Premultiplied (Matted with Black) (预乘（以黑色蒙版）) | alphaMode = Premultiplied, premulColor = [0,0,0] |
  | Premultiplied (Matted with White) (预乘（以白色蒙版）) | alphaMode = Premultiplied, premulColor = [1,1,1] |
  Default: Ask User. UNVERIFIED default.
  - Files whose format labels the alpha (e.g. PNG = straight, QuickTime with an alpha-type flag, PSD single layer = straight, PSD merged = premultiplied with white, EXR = premultiplied) never consult this preference. UNVERIFIED: per-format labelling list.
  - Interpretation rules (AE-PRJ-060) are applied after this.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** UNVERIFIED: whether the preference applies to every unlabeled format or only to formats that can be either (TGA, TIFF, some QuickTime codecs). The alpha maths is in AE-PRJ-047.

### AE-PRJ-043 · Still footage default duration (静止素材)

- **What it does:** Sets the duration given to layers created from still images (and the length of comps created from layered stills).
- **Exact semantics:** Preferences > Import > Still Footage:
  | Value (中文) | Meaning | Default |
  |---|---|---|
  | Use Composition Length (使用合成时长) | the layer spans the comp duration from its In point | UNVERIFIED: selected |
  | Use Custom Duration (使用自定义持续时间) + duration field | fixed duration in the project time display style | UNVERIFIED: 0:00:10:00 |
  - The footage item itself always has duration 0 (`isStill`); only layers (and layered-still import comps) get this duration.
  - Still layers can be extended indefinitely by trimming; they have no source time (time remap on a still is a no-op).
  - Placeholders with duration 0 and solids are still footage too and follow the same rule. UNVERIFIED for solids: solid layers always take the comp length.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion)
- **Notes:** Older UI names: "Length of Composition" / custom duration ("合成的长度"); UNVERIFIED.

### AE-PRJ-044 · Sequence footage import preferences (序列素材)

- **What it does:** Sets the default frame rate and validation for imported image sequences.
- **Exact semantics:** Preferences > Import > Sequence Footage:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Frames Per Second (每秒帧数) | float | 30 | UNVERIFIED: 1-999 | fps | becomes the item's native rate; changing it later affects only new imports |
  | Report Missing Frames (报告缺失帧) | bool | UNVERIFIED: on | — | — | AE-PRJ-041 |
  | Verify Individual Files (验证个体文件) | bool | off | — | — | slower; checks every frame file |
  - Formats that store a rate in each file (DPX/Cineon headers, EXR metadata) still use this preference unless the frame rate is read from the file; UNVERIFIED: AE ignores per-file rate metadata for DPX.
- **Priority:** P0
- **Owner:** bae-media (+ apps/bae)
- **Notes:** —

### AE-PRJ-045 · Indeterminate Media NTSC timecode preference (非确定性媒体 NTSC)

- **What it does:** Chooses drop-frame or non-drop-frame timecode for NTSC-rate media whose timecode type is unknown (e.g. image sequences).
- **Exact semantics:** Preferences > Import > Import Defaults > Indeterminate Media NTSC: Use Dropframe (使用丢帧时间码) / Use Non-Dropframe (使用非丢帧时间码). Default: UNVERIFIED: Dropframe. Applies only to footage at 29.97/59.94 (and 23.976 is always non-drop). Affects timecode display of footage start time and source timecode, not timing.
- **Priority:** P2
- **Owner:** bae-model (+ bae-media)
- **Notes:** Timecode display maths: see project-comp-layers.md, Composition Settings / time display, and AE-PRJ-086.

## Interpret Footage

### AE-PRJ-046 · Interpret Footage dialog and interpretation model (解释素材)

- **What it does:** Per-footage-item settings that tell AE how to read the source (alpha type, frame rate, start timecode, fields and pulldown, pixel aspect ratio, looping, format-specific options, input colour space); they change how decoded frames enter compositions, never the file.
- **Exact semantics:**
  - Open: File > Interpret Footage > Main (主要, Ctrl+Alt+G / Cmd+Opt+G), the Interpret Footage button at the bottom of the Project panel, or dragging an item onto that button. File > Interpret Footage > Proxy (代理) (or Alt/Option-click the button) edits the proxy source's interpretation (AE-PRJ-074).
  - Tabs: Main Options (主要选项) and Color (颜色) (named Color Management (色彩管理) in older versions). Sections in Main Options, in UI order: Alpha (AE-PRJ-047), Frame Rate (AE-PRJ-049), Start Timecode (AE-PRJ-050), Fields and Pulldown (AE-PRJ-051, AE-PRJ-052), Other Options: Pixel Aspect Ratio (AE-PRJ-053) and Loop (AE-PRJ-055), More Options button (AE-PRJ-056). A Preview checkbox applies changes live.
  - Sections that do not apply are disabled (e.g. Frame Rate, Fields and Loop for stills; Alpha when the source has no alpha).
  - Changing interpretation invalidates cached frames of the item and of every comp using it; it is undoable and marks the project dirty.
  - Decode pipeline for one comp-requested frame (render semantics beyond this point: compositing-3d-color.md):
    ```
    t_layer_source  (seconds into the footage item, from layer timing / time remap; see transform-keyframes-expressions.md)
    t_item = t_layer_source mod D_base         if loop > 1   (AE-PRJ-055)
    frame  = source frame for t_item at displayFrameRate (AE-PRJ-049), after pulldown removal (AE-PRJ-052)
    field  = field selection if fields are separated (AE-PRJ-051)
    pixels = decode(frame) -> alpha interpretation (AE-PRJ-047) -> input colour transform (AE-PRJ-057)
    pixels are then treated as having pixelAspect (AE-PRJ-053) when placed in the comp
    ```
- **Priority:** P0
- **Owner:** bae-media (+ bae-model, bae-ui-motion)
- **Notes:** Scripting: all settings live on `FootageItem.mainSource` / `proxySource` (`FootageSource` attributes). BAE decision: interpretation is a plain data record on the Asset Prim (`alpha_mode`, `premul_color`, `invert_alpha`, `conform_rate`, `start_tc_override`, `field_order`, `preserve_edges`, `pulldown`, `par`, `loop`, `color`), applied by bae-media when it produces frames; decoded-frame cache keys must include it.

### AE-PRJ-047 · Alpha interpretation (Alpha 通道解释)

- **What it does:** Declares whether the source alpha is ignored, straight or premultiplied (with which matte colour), and whether it is inverted.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Values | Notes |
  |---|---|---|---|---|
  | Alpha mode | enum | from file label, else the Interpret Unlabeled Alpha As preference (AE-PRJ-042) | Ignore (忽略) `AlphaMode.IGNORE`; Straight - Unmatted (直接 - 无遮罩) `AlphaMode.STRAIGHT`; Premultiplied - Matted With Color (预乘 - 有彩色遮罩) `AlphaMode.PREMULTIPLIED` | Guess button: AE-PRJ-048 |
  | Matte color (遮罩颜色（译名待核）) | RGB 0..1 | black [0,0,0] | any (eyedropper / colour picker) | only for Premultiplied; scripting `premulColor` |
  | Invert Alpha (反转 Alpha) | bool | off | — | ignored when alpha mode is Ignore or there is no alpha |
  - Conversion to AE's internal representation (AE layers carry straight colour plus alpha; see compositing-3d-color.md, alpha model):
    ```
    a = stored_alpha (normalised 0..1); if invert_alpha: a = 1 - a
    Ignore:        a_out = 1;           rgb_out = rgb                        (premultiplied fringes stay visible)
    Straight:      a_out = a;           rgb_out = rgb
    Premultiplied: a_out = a;
                   if a > 0: rgb_out = (rgb - (1 - a) * C) / a              (C = matte colour, per channel)
                   else:     rgb_out = 0   (UNVERIFIED: AE may use C or leave rgb for a == 0)
    8/16 bpc: rgb_out clamped to [0, 1]; 32 bpc: UNVERIFIED (believed unclamped, so over-range values survive)
    ```
  - Interpreting premultiplied footage as straight produces a dark (black-matte) or light (white-matte) halo on soft edges; the Remove Color Matting effect can fix it per layer (see effects.md, Remove Color Matting).
  - Only the first alpha channel of files with several alpha/extra channels is used.
  - Footage without alpha: alpha controls disabled, a_out = 1.
- **Priority:** P0
- **Owner:** bae-media (+ bae-render)
- **Notes:** UNVERIFIED: order of Invert Alpha relative to unmatting (believed: invert first). Scripting `FootageSource.alphaMode`, `premulColor`, `invertAlpha`, `hasAlpha`. Implementation hint: do the unmatte in the decode-upload compute shader together with colour conversion; BAE's compositor works premultiplied internally (compositing-3d-color.md) so straight -> premultiplied is a multiply, and premultiplied-with-black input can skip the divide entirely.

### AE-PRJ-048 · Guess alpha (猜测)

- **What it does:** Estimates whether a footage alpha is straight or premultiplied, and the matte colour, from the pixels.
- **Exact semantics:** Interpret Footage > Alpha > Guess (and the Guess import preference) sets alpha mode, matte colour and invert flag to AE's best estimate; if AE cannot guess confidently it beeps and leaves the setting (UI) / uses a default. Scripting `FootageSource.guessAlphaMode()` (no-op when `hasAlpha` is false). The algorithm is not documented. UNVERIFIED heuristic consistent with observed behaviour:
  ```
  for semi-transparent pixels (0 < a < 1):
    if every channel c <= a (+ tolerance): premultiplied with black is consistent
    if every channel c >= 1 - a (- tolerance): premultiplied with white is consistent
    otherwise: straight
  choose the hypothesis consistent with (almost) all sampled pixels; if none clearly wins: not confident
  ```
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** BAE decision: implement the heuristic on the first decoded frame (downsampled); report confidence to the UI.

### AE-PRJ-049 · Frame rate interpretation and conform (帧速率)

- **What it does:** Chooses the frame rate at which the footage's frames are laid out in time: the file's own rate or a conformed (assumed) rate.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Use frame rate from file (使用文件中的帧速率（译名待核）) | radio | selected for movies | — | — | shows the native rate, e.g. "(29.970 fps)" |
  | Conform to frame rate (匹配帧速率) | radio + float | native rate | UNVERIFIED: 0.001..999 | fps | scripting `conformFrameRate` (0 = use native); documented UI max 999 fps (scripting docs still say 99) |
  | Assume this frame rate (假定此帧速率) | float | Sequence Footage preference (30) | as above | fps | shown instead of the radios for image sequences |
  - Time mapping (no frame blending; frame blending is a layer switch, see transform-keyframes-expressions.md):
    ```
    rate = conformFrameRate if != 0 else nativeFrameRate      ( * 0.8 when pulldown removed, AE-PRJ-052 )
    N    = number of frames in the file (after pulldown removal)
    D    = N / rate                                            (footage duration, seconds)
    frame_index(t) = clamp(floor(t * rate + eps), 0, N - 1)    for 0 <= t < D
    ```
    UNVERIFIED: AE's exact epsilon/rounding for t landing exactly on a frame boundary (BAE: compute in flicks with exact rational rates so no epsilon is needed).
  - Conforming changes playback speed and item duration (15 -> 30 fps plays twice as fast, half as long). Audio of the same item is NOT conformed: it keeps its original speed and duration, so A/V sync changes (Adobe help). Keyframes on layers using the item stay at their comp times. UNVERIFIED: the item duration is then the video duration and audio beyond it is cut.
  - Layers already using the item keep their In point and keyframe times; their Out point is re-derived from the new duration if the layer was not trimmed. UNVERIFIED.
  - Variable-frame-rate sources are presented as constant-rate (AE-PRJ-063).
- **Priority:** P0
- **Owner:** bae-media (+ bae-anim)
- **Notes:** Scripting `nativeFrameRate`, `conformFrameRate`, `displayFrameRate`. A layer Time Stretch (e.g. 100.1% for 30 -> 29.97) is the alternative that also stretches audio (see transform-keyframes-expressions.md, time stretch). BAE decision: store rates as rationals (30000/1001, not 29.97).

### AE-PRJ-050 · Start timecode (起始时间码（译名待核）)

- **What it does:** Sets the footage's media start time (source timecode of its first frame), used for time display, not for rendering.
- **Exact semantics:**
  - Options: Use source file timecode (default when the file carries timecode, e.g. QuickTime tmcd track, MXF, DPX header) / Override start timecode with a user value (in the project time display style). UNVERIFIED: section added in AE 22.x-23.x; label names.
  - Without embedded timecode the media start is 0 (image sequences may use the first frame number when the time display is Frames; see AE-PRJ-086 Footage Start Time).
  - The media start offsets the timecode shown in the Footage viewer, the Project panel In/Out columns and the layer "source time" displays; drop-frame vs non-drop for NTSC rates follows the file, or the Indeterminate Media NTSC preference (AE-PRJ-045).
  - It does not shift where the footage starts in a comp (layer In point is unaffected).
- **Priority:** P1
- **Owner:** bae-media (+ bae-model)
- **Notes:** Expressions: `sourceTime()` and footage time displays (see transform-keyframes-expressions.md). BAE decision: store `media_start` as flicks + drop-frame flag on the Asset.

### AE-PRJ-051 · Separate fields (分离场)

- **What it does:** Treats each interlaced frame as two fields displaced in time and reconstructs a full-height image per field.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Values | Notes |
  |---|---|---|---|---|
  | Separate Fields (分离场) | enum | from file field-order label; DV and D1 footage automatically separated; else Off | Off (关) `FieldSeparationType.OFF` / Upper Field First (高场优先) `UPPER_FIELD_FIRST` / Lower Field First (低场优先) `LOWER_FIELD_FIRST` | upper field = odd lines 1,3,5... (top line is line 1) |
  | Preserve Edges (Best Quality Only) (保留边缘（仅最佳品质）) | bool | off | — | scripting `highQualityFieldSeparation`; better reconstruction in static areas at Best quality |
  - Semantics: the footage keeps its frame rate; each source frame k provides field 1 at time k/rate and field 2 at (k + 0.5)/rate. A field image is made full-height by interpolating the missing lines (line doubling/interpolation; with Preserve Edges, non-moving areas take lines from the other field). Comps at twice the frame rate, or renders with Field Render on, show every field; a comp at the footage rate samples the first field. UNVERIFIED: exact interpolation kernel.
  - Field order labels in files are honoured automatically; the dialog overrides them.
- **Priority:** P2 — interlaced sources are legacy for a 2026 motion-graphics tool.
- **Owner:** bae-media
- **Notes:** Output field rendering: see ui-preview-render-io.md, Render Settings. Scripting `FootageSource.fieldSeparationType`, `guessPulldown()` also sets field order.

### AE-PRJ-052 · Remove 3:2 / 24Pa pulldown (移除 Pulldown)

- **What it does:** Reconstructs the original 23.976/24 fps progressive frames from telecined 29.97/30 fps interlaced video.
- **Exact semantics:**
  - Requires Separate Fields not Off (setting pulldown with fields Off is an error).
  - Remove (移除) menu, phases named by the pattern of Whole (W) and Split (S) frames in the first five video frames: Off; WSSWW; SSWWW; SWWWS; WWWSS; WWSSW (3:2); and the same five phases for 24Pa (24P Advance, Panasonic DVX100). Scripting `PulldownPhase.OFF`, `WSSWW` ... `WWSSW`, `WSSWW_24P_ADVANCE` ... `WWSSW_24P_ADVANCE`.
  - Buttons Guess 3:2 Pulldown (猜测 3:2 Pulldown) and Guess 24Pa Pulldown (猜测 24Pa Pulldown) analyse the footage and set field order + phase (scripting `guessPulldown(PulldownMethod.PULLDOWN_3_2 | ADVANCE_24P)`).
  - Effect: 4 progressive frames are rebuilt from each 5 video frames; `displayFrameRate = conformFrameRate * 0.8` (29.97 -> 23.976, 30 -> 24); if conformFrameRate was 0 it is set to the native rate when pulldown is enabled.
  - Mixed-phase footage must be imported several times with different phases and trimmed.
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** —

### AE-PRJ-053 · Pixel aspect ratio presets (像素长宽比)

- **What it does:** Declares the shape of the footage's pixels; AE scales the footage horizontally by PAR_footage / PAR_comp when placing it in a comp, and corrects the viewer display.
- **Exact semantics:** Pixel Aspect Ratio menu (same list in Interpret Footage, Composition Settings and Solid Settings), in menu order:
  | Menu entry (中文) | UI value | Exact ratio | Typical frame |
  |---|---|---|---|
  | Square Pixels (方形像素) | 1.0 | 1/1 | any square-pixel source |
  | D1/DV NTSC (0.91) | 0.91 | 10/11 = 0.909091 | 720x486, 720x480, 4:3 |
  | D1/DV NTSC Widescreen (1.21) (D1/DV NTSC 宽银幕) | 1.21 | 40/33 = 1.212121 | 720x486, 720x480, 16:9 |
  | D1/DV PAL (1.09) | 1.09 | 128/117 = 1.094017 | 720x576, 4:3 |
  | D1/DV PAL Widescreen (1.46) (D1/DV PAL 宽银幕) | 1.46 | 512/351 = 1.458689 | 720x576, 16:9 |
  | Anamorphic 2:1 (2) (变形 2:1) | 2.0 | 2/1 | 2:1 anamorphic lens / film transfer |
  | HDV 1080/DVCPRO HD 720 (1.33) | 1.33 | 4/3 = 1.333333 | 1440x1080, 960x720, 16:9 |
  | DVCPRO HD 1080 (1.5) | 1.5 | 3/2 | 1280x1080, 16:9 |
  - The exact ratios above match the values reported by scripting (`pixelAspect` returns 0.909091, 1.21212, 1.09402, 1.45869, 1.33333 for the UI values 0.91, 1.21, 1.09, 1.46, 1.33). Since AE CS4 these are the clean-aperture-correct values; projects from AE CS3 or earlier (0.9, 1.2, 1.07, 1.42) are upgraded on open.
  - UNVERIFIED: menu order of Anamorphic 2:1 relative to the HD entries; whether a Custom entry exists for footage (Composition Settings has none; scripting accepts any 0.01..100).
  - Comp placement: a layer from footage with PAR_f in a comp with PAR_c is displayed with horizontal scale factor PAR_f / PAR_c applied before the layer transform (layer Scale still shows 100%). See compositing-3d-color.md, nested comp/footage sampling.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-render)
- **Notes:** Square-pixel equivalents of SD comp presets: NTSC D1 720x534, NTSC D1 Widescreen 872x486, PAL D1/DV 788x576, PAL D1/DV Widescreen 1050x576 (CS4+). Viewer PAR correction: see ui-preview-render-io.md, viewer options. Chinese names for the D1/DV entries are the English strings in zh-CN (UNVERIFIED).

### AE-PRJ-054 · Pixel aspect ratio detection on import (像素长宽比自动识别（译名待核）)

- **What it does:** Sets an imported item's PAR from file metadata, or guesses it from the frame size.
- **Exact semantics:**
  - Explicit metadata wins: QuickTime (pasp atom), AVI, PSD (pixel aspect) and other formats that store PAR are tagged automatically (e.g. 16:9 DV -> D1/DV NTSC Widescreen).
  - Without metadata: 720x486 or 720x480 -> D1/DV NTSC (0.91); 720x576 -> D1/DV PAL (1.09); everything else -> Square. UNVERIFIED: whether 1440x1080 without metadata is guessed as 1.33 (believed no).
  - The interpretation rules file can add size-based rules (AE-PRJ-060).
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** A square-pixel image that happens to be 720x480 is mis-tagged and must be reset to Square by the user.

### AE-PRJ-055 · Loop footage (循环)

- **What it does:** Repeats a time-based footage item N times end-to-end, making its duration N times longer.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Loop (循环) ... Times (次) | int | 1 | 1..9999 | count |
  ```
  D_base = duration after frame-rate/pulldown interpretation
  item.duration = loop * D_base
  t_item = t mod D_base          for 0 <= t < loop * D_base
  ```
  - Not available for stills (error in scripting). Audio loops together with video. UNVERIFIED.
  - Layers already using the item are not automatically extended; the user trims them out. UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-media (+ bae-model)
- **Notes:** Scripting `FootageSource.loop`. Expression `loopOut()` is a different, keyframe-level feature (transform-keyframes-expressions.md). Implementation hint: never duplicate decoded frames; map time only.

### AE-PRJ-056 · More Options (format-specific source settings) (更多选项)

- **What it does:** Opens format-specific interpretation dialogs for certain footage types.
- **Exact semantics:**
  | Footage type | Dialog | Key options (defaults) |
  |---|---|---|
  | Illustrator / PDF / EPS | EPS/AI options | Antialiasing: More Accurate / Faster (UNVERIFIED default More Accurate) |
  | Camera Raw (incl. CinemaDNG subset) | Adobe Camera Raw dialog | full ACR develop settings; settings of the first image apply to a raw sequence |
  | Cineon / DPX | Cineon Settings (Cineon 设置) | Conversion type preset (Standard / Over Range / Custom...), 10 Bit Black Point 95, Converted Black Point 0, 10 Bit White Point 685, Converted White Point 1023 (10-bit) / 255 (8-bit), Gamma 1.7, Highlight Rolloff 20, Logarithmic Conversion / Units. UNVERIFIED: all values and names; AE recommends instead colour-managing DPX with a log profile |
  | RED R3D | R3D Source Settings | RED SDK decode/colour settings (interpreted as 32 bpc Rec.709) |
  | ARRIRAW, Sony RAW, Canon RAW, ProRes RAW | source settings | vendor raw settings |
  | OpenEXR | (none; channels via EXtractoR / Cryptomatte effects) | — |
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** BAE decision: M1 needs only the AI/PDF antialias option; camera raw and cinema raw formats are out of M1 scope. DPX log handling should go through colour management (bae-color) rather than the legacy Cineon dialog.

### AE-PRJ-057 · Footage colour interpretation (Color tab) (颜色 / 色彩管理)

- **What it does:** Declares the colour space of the footage so AE can convert it into the project working space, or exempts it from conversion.
- **Exact semantics:** Interpret Footage > Color tab. Options differ by Project Settings colour engine (AE-PRJ-087):
  | Parameter (中文) | Type | Default | Values / Notes |
  |---|---|---|---|
  | Media Color Space / Embedded Profile (媒体色彩空间 / 嵌入的配置文件) | read-only | from file | embedded ICC (PSD, TIFF, PNG, JPEG) or video colour metadata (ProRes, H.264/HEVC, XAVC/AVC MXF, ARRI/Sony/RED/ProRes/Canon RAW); "None" if absent |
  | Override Media Color Space (覆盖媒体色彩空间) | enum | off (use media/embedded space) | Adobe engine: any installed ICC profile (former "Assign Profile" (指定配置文件)); OCIO engine: any colour space of the OCIO config (Show all lists all spaces incl. display/view transforms) |
  | Interpret As Linear Light (解释为线性光) | enum | Off | Off / On / On for 32 bpc; treats the assigned profile's tone curve as gamma 1.0; works even when colour management is off |
  | Preserve RGB (保留 RGB) | bool | off | no conversion into the working space: RGB numbers kept, appearance not (use for data such as displacement maps, normal maps) |
  | Description text | read-only | — | explains the resulting conversion |
  - With colour management off (working space None, Adobe engine) no conversion occurs except Interpret As Linear Light. Footage with no embedded/assigned profile and no rule is assumed to be in the working space.
  - Non-RGB sources (CMYK, Y'CbCr, camera raw) cannot be assigned a profile; their native space is shown and converted automatically.
  - OCIO engine: the default media space comes from the OCIO config's default/file rules set in Project Settings.
  - Conversion maths and Expand ITU-R 601 legacy handling: see compositing-3d-color.md, colour management.
- **Priority:** P1
- **Owner:** bae-color (+ bae-media, bae-model)
- **Notes:** UNVERIFIED: AE version in which "Assign Profile" was renamed "Override Media Color Space" (believed AE 24.x/25.0 with the colour-engine rework). Scripting has no direct `FootageSource` colour attributes (UNVERIFIED). BAE decision: store `{embedded, override, linear_light, preserve_rgb}` on the Asset; bae-color resolves it to a transform.

### AE-PRJ-058 · Copy and apply interpretation (记住解释 / 应用解释)

- **What it does:** Copies the interpretation of one footage item to others.
- **Exact semantics:** File > Interpret Footage > Remember Interpretation (记住解释, Ctrl+Alt+C / Cmd+Opt+C) stores the selected item's main-source interpretation; File > Interpret Footage > Apply Interpretation (应用解释, Ctrl+Alt+V / Cmd+Opt+V) applies it to all selected footage items in one undo step. Settings that do not apply to a target (e.g. frame rate for a still, alpha for a source without alpha) are skipped. UNVERIFIED: whether colour settings are included (believed yes) and which settings are skipped.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** BAE decision: implement as a Command taking the interpretation record and a list of target Assets.

### AE-PRJ-059 · Footage interpretation on replacement preference (替换时的素材解释规则)

- **What it does:** Chooses whether replacing a footage item's source keeps or resets its interpretation.
- **Exact semantics:** Preferences > Import > Footage Interpretation on Replacement: Reset (重置): new source gets default interpretation for that file; Remember (记住): frame rate, alpha interpretation and other settings of the replaced footage are kept. Default: UNVERIFIED (believed Remember, matching AE's long-standing behaviour of keeping interpretation on Replace Footage). Applies to File > Replace Footage > File and to Alt-drag replacement in the Project panel. UNVERIFIED: introduced as a preference in AE 25.x.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** See Replace Footage (AE-PRJ-065).

### AE-PRJ-060 · Interpretation rules file (解释规则文件)

- **What it does:** A user-editable text file of rules that set default interpretation for imported footage matching file type, codec, size and frame rate.
- **Exact semantics:** File `interpretation rules.txt` (some updates install a versioned name) in the per-version preferences folder (macOS `~/Library/Preferences/Adobe/After Effects/<version>/`, Windows `%APPDATA%\Adobe\After Effects\<version>\`). Each non-comment line is `match ~ action`: match fields (width, height, frame rate, file-type four-char code, codec four-char code) with `*` wildcards; action fields (PAR, field order, pulldown, alpha, frame rate, colour profile...). Applied at import after file metadata, before user changes; AE must be restarted after editing. UNVERIFIED: exact field order and syntax; AE 26 still ships the file.
- **Priority:** P2
- **Owner:** bae-media (+ apps/bae)
- **Notes:** Alt/Option-click an item in the Project panel shows its file-type and codec codes. BAE decision: provide import rules as a JSON/TOML preference with typed fields rather than AE's positional format.

## Video, audio and data import

### AE-PRJ-061 · Video file metadata on import (视频素材导入)

- **What it does:** Reads the technical properties of a movie file into the footage item and its default interpretation.
- **Exact semantics:** On import AE reads and stores:
  | Property | Source | Used for |
  |---|---|---|
  | width, height | stream dimensions (clean vs production aperture as tagged) | item size |
  | native frame rate | container/stream time base (e.g. 30000/1001) | AE-PRJ-049 |
  | frame count / duration | container | item duration |
  | pixel aspect ratio | pasp / container tag, else size guess | AE-PRJ-053, AE-PRJ-054 |
  | field order | stream/container tag (progressive, TFF, BFF) | AE-PRJ-051 |
  | alpha presence and type | codec (e.g. ProRes 4444, PNG, Animation, HAP Alpha) and alpha-type flag | AE-PRJ-047 |
  | bit depth class | codec (8, 10, 12, 16 bit, float) | Project panel depth descriptor; decode precision |
  | colour metadata | primaries / transfer / matrix / range tags, embedded ICC | AE-PRJ-057 |
  | embedded timecode | tmcd track, MXF/DPX timecode | media start (AE-PRJ-050) |
  | audio streams | see AE-PRJ-062 | — |
  | XMP metadata, tape/reel name | container | Project panel columns |
  - Y'CbCr sources are converted to RGB using the tagged matrix and range (video range 16-235 expanded to full range for 8-bit). UNVERIFIED: handling of untagged range.
  - Hardware-accelerated decoding (Preferences > Import > Video Footage > Enable hardware accelerated decoding, H.264/HEVC) changes speed only, not results. UNVERIFIED default on.
- **Priority:** P0
- **Owner:** bae-media
- **Notes:** Format/codec support list: see ui-preview-render-io.md, import/export format lists. BAE decision: decode via platform frameworks (VideoToolbox / Media Foundation) plus bundled royalty-free decoders; no statically linked FFmpeg (AGENTS.md rule).

### AE-PRJ-062 · Audio import (音频素材导入)

- **What it does:** Imports audio-only files and the audio streams of movies as the audio part of footage items.
- **Exact semantics:**
  - Audio-only files (WAV, AIFF, MP3, AAC/M4A, ...) create items with `hasAudio = true`, `hasVideo = false`, duration from the stream.
  - Read: sample rate, sample format/bit depth, channel count and layout. AE's audio engine mixes in stereo: mono sources are placed in both channels; multichannel (e.g. 5.1) sources are downmixed to stereo. UNVERIFIED: downmix coefficients and whether only the first stereo pair is used.
  - Audio is resampled to the project audio sample rate (AE-PRJ-088) for preview and render; resampling quality: UNVERIFIED.
  - Compressed audio may be conformed into the Media Cache (conformed .cfa files and .pek peak files) the first time it is used. UNVERIFIED for AE 26.
  - Only the first audio stream of a multi-stream file is used. UNVERIFIED.
  - Audio is never affected by frame-rate conform (AE-PRJ-049) but is by Loop (AE-PRJ-055).
- **Priority:** P0
- **Owner:** bae-media
- **Notes:** Audio layer properties (Audio Levels, waveform) and audio effects: see project-comp-layers.md, layer types, and effects.md, Audio effects. Media Cache settings: see ui-preview-render-io.md, Preferences.

### AE-PRJ-063 · Variable-frame-rate media (可变帧速率素材（译名待核）)

- **What it does:** Presents footage whose frames have irregular timestamps (phone/screen recordings) as constant-frame-rate footage.
- **Exact semantics:** AE assigns a constant native frame rate to VFR footage (the stream's nominal/average rate) and maps comp time to source frames by timestamp. UNVERIFIED behaviour (AE does not document its VFR algorithm; observed: frames are shown by presentation timestamp, so duplicated/dropped frames appear where the source had gaps, and A/V sync is preserved). Conforming (AE-PRJ-049) of VFR footage re-times by frame index and breaks sync.
  ```
  BAE rule (proposed, AE-compatible in effect):
    nominal_rate = most common frame interval of the stream (fallback: frame_count / duration)
    frame_at(t) = the last frame whose PTS <= t + half_frame_epsilon   (PTS relative to first frame)
  ```
- **Priority:** P1 — very common source today; wrong handling causes A/V drift.
- **Owner:** bae-media
- **Notes:** UNVERIFIED: AE version that started detecting VFR and warning about it. Implementation hint: build a PTS index at import (keyframe scan) so random access is O(log n).

### AE-PRJ-064 · Data file import (JSON, CSV, TSV, MGJSON) (数据文件导入（译名待核）)

- **What it does:** Imports structured data files as footage items that expressions can read and that can drive data layers.
- **Exact semantics:**
  - JSON (`.json`, AE 15.0), CSV and TSV/TXT (UNVERIFIED: AE 16.1/17.0), and mgJSON (`.mgjson`, motion-graphics JSON with timed streams, AE 15.0) import as footage items without video or audio (duration 0 for JSON/CSV; mgJSON has duration from its samples).
  - Added to a comp, a JSON/CSV item makes a layer with no image whose data is read by expressions (`footage("data.json").sourceData`, `.dataValue([row, col])` for CSV); an mgJSON item makes a Data layer whose Data properties are keyframed from the streams. Layer > Time > Align Video to Data uses mgJSON timing.
  - From AE 15.1, a `.jsx` file with the same content as a JSON file can be imported as data (for expressions `footage("x.jsx").sourceData`); Adobe's help recommends it as the workaround for JSON files whose expressions do not work, via Replace Footage (Source: https://helpx.adobe.com/after-effects/using/data-driven-animation.html).
  - Reload Footage re-reads the file; expressions re-evaluate.
- **Priority:** P1
- **Owner:** bae-media (+ bae-anim, bae-model)
- **Notes:** Expression API: see transform-keyframes-expressions.md, data footage. Layer type: see project-comp-layers.md, layer types.

## Replace, reload and missing footage

### AE-PRJ-065 · Replace Footage with file (替换素材 > 文件)

- **What it does:** Points an existing footage item at a different file; every layer using the item switches to the new source and keeps all its properties.
- **Exact semantics:**
  - File > Replace Footage > File (Ctrl+H / Cmd+H) with one footage item selected; the Import dialog appears (with Sequence / Force alphabetical options). The item keeps its ID, name (unless the name was derived from the file, in which case it follows the new file name; UNVERIFIED), label, comment, folder, proxy and all uses; layers keep masks, effects, keyframes, expressions, timing and transforms.
  - Interpretation: kept or reset per the Footage Interpretation on Replacement preference (AE-PRJ-059); properties that are read from the new file (size, native rate, duration, alpha presence) always update.
  - If the new source has a different size, layers keep their transform values, so anchor point/position relative to the image change; if shorter, layers are not trimmed (frames past the end hold the last frame? UNVERIFIED: AE shows nothing / holds last frame beyond source end for footage layers whose Out point exceeds the source).
  - Alt/Option-drag an item from the Project panel onto another selected item in the Project panel replaces all uses of the target item(s) with the dragged item (the target item remains, now unused). UNVERIFIED: whether the target items are deleted.
  - Single-layer replacement (Alt-drag onto a layer) is layer Replace Source: see project-comp-layers.md, Replace layer source.
  - Replacing works on missing items (relink) and placeholders (AE-PRJ-083).
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-motion)
- **Notes:** Scripting `FootageItem.replace(file)`, `replaceWithSequence(file, forceAlphabetical)`. BAE decision: Replace is a Command that swaps the Asset's source record; layer Prims reference the Asset by ID so no layer is touched.

### AE-PRJ-066 · Replace Footage with placeholder or solid (替换素材 > 占位符 / 纯色)

- **What it does:** Turns an existing footage item into a placeholder or a solid while keeping all its uses.
- **Exact semantics:**
  - File > Replace Footage > Placeholder (占位符): dialog with Name, Width, Height, Frame Rate, Duration (defaults: the item's current values); the item becomes a `PlaceholderSource` (rendered as colour bars, AE-PRJ-082).
  - File > Replace Footage > Solid (纯色): Solid Settings dialog (AE-PRJ-079); the item becomes a `SolidSource` and moves into the solids folder. UNVERIFIED: whether it is moved.
  - File > Replace Footage > With Layered Comp: AE-PRJ-033.
  - Scripting `replaceWithPlaceholder(name, width, height, frameRate, duration)`, `replaceWithSolid(color, name, width, height, pixelAspect)`.
- **Priority:** P1
- **Owner:** bae-model (+ bae-motion)
- **Notes:** —

### AE-PRJ-067 · Reload Footage (重新加载素材)

- **What it does:** Re-reads the selected footage items' source files from disk so changes made by other applications appear.
- **Exact semantics:** File > Reload Footage (Ctrl+Alt+L / Cmd+Opt+L) on selected items (or footage layers' sources): drops cached frames, re-probes metadata (size, duration, frame count, layers of PSD/AI), keeps the interpretation. For layered files, renamed source layers stay linked, deleted layers become Missing, new layers are not added. For image sequences the frame range is re-scanned. Missing files become found if they reappeared. Not undoable (UNVERIFIED). Scripting `FileSource.reload()` (main source only).
- **Priority:** P0
- **Owner:** bae-media (+ bae-render)
- **Notes:** Implementation hint: compare the stored fingerprint (size + mtime, ARCHITECTURE 16) to decide what actually changed and invalidate only those caches.

### AE-PRJ-068 · Automatic footage reloading (自动重新加载素材)

- **What it does:** Reloads changed footage automatically when AE becomes the active application again.
- **Exact semantics:** Preferences > Import > Automatic Footage Reloading > Auto Reload: All Footage Types (所有素材类型) / Non-sequence Footage (非序列素材, default) / Off (关). On application activation AE checks modification dates of the relevant sources and reloads changed ones (as Reload Footage). Image sequences are excluded by default because checking every frame is slow.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** BAE decision: use OS file-watch notifications (FSEvents / ReadDirectoryChangesW / inotify) instead of polling on activation, with the same three-way preference.

### AE-PRJ-069 · Reveal source file (在 Finder 中显示 / 在资源管理器中显示)

- **What it does:** Opens the OS file browser at the footage item's source file.
- **Exact semantics:** Project panel context menu > Reveal in Finder (macOS) / Reveal in Explorer (Windows) (在 Windows 资源管理器中显示); Reveal in Bridge (在 Bridge 中显示) when Bridge is installed. For sequences, the first file is selected. Disabled for missing files, solids and placeholders. Also available for a layer's source from the Timeline context menu (Reveal Layer Source in Project, then reveal). UNVERIFIED: Timeline also offers Reveal in Finder directly.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Reveal in Bridge is P2 / not applicable to BAE.

### AE-PRJ-070 · Edit Original (编辑原稿)

- **What it does:** Opens the footage's source file in the application associated with it, and reloads it when saved.
- **Exact semantics:** Edit > Edit Original (Ctrl+E / Cmd+E) with a footage item or a layer selected opens the source in its creating/associated app. For a sequence selected in the Timeline/Composition panel the currently displayed frame's file opens; selected in the Project panel, the first file. Changes are picked up when AE becomes active again (auto reload or manual Reload Footage). Alt/Option-double-click in the Project panel opens the file in the OS-associated player.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** —

### AE-PRJ-071 · Missing footage state and display (缺失素材)

- **What it does:** A footage item whose source file cannot be found stays in the project with all its uses, flagged as missing and rendered as colour bars.
- **Exact semantics:**
  - Detection: on project open, reload, or when a decode fails because the file is gone. `AVItem.footageMissing = true`; `FileSource.missingFootagePath` holds the last known path.
  - Project panel: name in italics, Type/File Path show it as Missing; search filter `missing` finds them. Size/duration/frame rate/interpretation remain as last known, so layer timing is unchanged.
  - Rendering: every frame of the item is replaced by a colour-bars placeholder image at the item's width x height (the same image as placeholders, AE-PRJ-082); effects, masks and transforms still apply to it.
  - Missing frames inside a sequence: AE-PRJ-041.
  - Render Queue: GUI renders proceed using colour bars (UNVERIFIED: a warning is shown before rendering); `aerender` stops on missing footage unless `-continueOnMissingFootage` is given (see ui-preview-render-io.md, headless render).
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-render)
- **Notes:** BAE decision: the missing state is derived (not persisted) from the asset resolution result of ARCHITECTURE 16; the persisted data is the reference record. A missing asset must never be dropped or rewritten on save.

### AE-PRJ-072 · Missing footage, effects and fonts reports (查找缺失的素材 / 效果 / 字体（译名待核）)

- **What it does:** Reports, on project open and on demand, the footage, effects and fonts the project needs but the machine lacks.
- **Exact semantics:**
  - On open AE shows a dialog per category: "N files are missing" with the last missing path; a list of missing effects (by display name); a list of missing fonts (suppressible: Preferences > "Do not warn me about missing fonts when opening projects"). UNVERIFIED exact texts.
  - File > Dependencies > Find Missing Footage / Find Missing Effects / Find Missing Fonts (added in AE CC 12.0 (2013); the Project panel search term "missing" offers the same filters): filters the Project panel to missing footage, or the Timeline panel to layers that use missing effects/fonts.
  - Missing effects: the effect instance remains on the layer with all parameter values preserved and is shown as missing; it renders as a pass-through (no effect). Missing fonts: text layers keep the font name and render with a substitute font until the font is installed; Edit > Find and replace font / `Project.replaceFont()` can retarget. See text.md, fonts, and effects.md, effect framework.
  - Scripting `Project.usedFonts` lists fonts used by text layers.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion, bae-plugin, bae-text)
- **Notes:** BAE decision: matches ARCHITECTURE 4.6 (plugin data kept with plugin ID/version and shown as placeholder when the plugin is missing); missing-items report is also produced by Collect Files and `.baez` packaging.

### AE-PRJ-073 · Relinking missing footage (重新链接素材（译名待核）)

- **What it does:** Finds moved footage automatically on open, lets the user point one missing item at its new location, and then relinks other missing items found in the same folder.
- **Exact semantics:**
  - Search on open: for each file reference AE tries the stored absolute path; if not found, the path relative to the project file (stored at save time), which is how collected projects open on other machines. UNVERIFIED: order of these two attempts.
  - Manual relink: double-click a missing item (or Replace Footage > File) and choose the file. After a manual relink, AE searches the chosen file's folder for other missing items (same file names, applying the same old-folder -> new-folder relationship) and relinks them, reporting how many additional files were found. UNVERIFIED: exact search scope (same folder only vs relative sub-folders).
  - Relinking keeps interpretation, IDs and all uses.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-ui-motion)
- **Notes:** BAE decision (ARCHITECTURE 16): every asset stores (a) path relative to the project, (b) original absolute path, (c) fingerprint (size, mtime, optional content hash); resolution order on open is relative -> absolute -> name+fingerprint search in the project folder and recent search folders -> missing with "Relink". After one manual relink, the same directory mapping (old prefix -> new prefix) is applied to all other missing assets. Paths are stored with `/` separators, and team path-mapping rules (e.g. `Z:\` <-> `/Volumes/NAS`) are applied before searching. This is a superset of AE's behaviour.

## Proxies

### AE-PRJ-074 · Set Proxy (设置代理)

- **What it does:** Assigns a substitute source (usually a smaller or still version) to a footage item or composition; while the proxy is in use, every comp uses the proxy in place of the original.
- **Exact semantics:**
  - File > Set Proxy > File (文件) (Ctrl+Alt+P / Cmd+Opt+P) with one footage item or comp selected: choose a file (Sequence / Force alphabetical options available); `useProxy` becomes true. File > Set Proxy > None (无): removes the proxy, `useProxy` false.
  - Compositions can have proxies too (typically a pre-rendered movie of the comp); a comp with an active proxy is not rendered from its layers when used as a precomp. (See Render Settings Proxy Use, AE-PRJ-078.)
  - The proxy source has its own interpretation (File > Interpret Footage > Proxy).
  - Scripting: `AVItem.setProxy(file)`, `setProxyWithSequence(file, forceAlphabetical)`, `setProxyWithSolid(color, name, width, height, pixelAspect)`, `setProxyWithPlaceholder(name, width, height, frameRate, duration)`, `setProxyToNone()`; `proxySource`, `useProxy`.
  - Anything applied to layers (masks, effects, keyframes, expressions) is unaffected by switching between proxy and original.
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** BAE decision: BAE additionally generates automatic decode proxies/caches in `cache/` (ARCHITECTURE 4.6); those are invisible performance caches and must not be confused with user proxies, which are project data.

### AE-PRJ-075 · Proxy scaling to the original (代理缩放（译名待核）)

- **What it does:** Makes a proxy of any size behave exactly like the original's dimensions and duration in comps.
- **Exact semantics:**
  ```
  sx = original.width  / proxy.width ;  sy = original.height / proxy.height
  proxy pixels are resampled by (sx, sy) to original.width x original.height before the layer transform
  (if the frame aspect ratios differ, sx != sy and the proxy is stretched; this is slower)
  layer bounds, anchor point, masks and effect coordinates stay in original pixel space
  ```
  - Duration: the item keeps the original's duration, frame rate and timing; a still proxy is held for the whole duration; a movie proxy is sampled at original time t (UNVERIFIED: a shorter/longer proxy movie is not stretched, frames past its end hold or show empty).
  - PAR: the original's PAR is used. UNVERIFIED.
  - Recommended: same frame aspect ratio, e.g. 160x120 proxy for 640x480 (exact 4x).
- **Priority:** P1
- **Owner:** bae-render (+ bae-media)
- **Notes:** Implementation hint: treat proxy use as a lower-resolution mip of the original source: the render graph samples the proxy texture with a scale factor instead of materialising an upscaled image.

### AE-PRJ-076 · Proxy indicators and toggle (代理指示器（译名待核）)

- **What it does:** Shows and toggles whether each item currently uses its proxy.
- **Exact semantics:** Project panel, left of the item name: filled box = proxy assigned and in use (project-wide); hollow box = proxy assigned but original in use; no box = no proxy. Clicking the box toggles `useProxy` (undoable, UNVERIFIED). When the item is selected, the proxy's file name is shown in bold at the top of the Project panel. Proxy use is project-wide, not per comp.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** —

### AE-PRJ-077 · Create Proxy (创建代理)

- **What it does:** Renders a still or movie proxy of the selected footage or comp through the Render Queue and assigns it automatically.
- **Exact semantics:** File > Create Proxy > Still (静止图像) or Movie (影片) with a footage item or comp selected (Project or Timeline panel): asks for a name/location, adds a render queue item with Post-Render Action = Set Proxy (设置代理) and default proxy templates. Still: renders the frame at the current time of the Footage/Composition panel (the poster frame). Default templates: UNVERIFIED (believed Render Settings "Draft Settings" at half resolution and output module "Photoshop" for stills / "Lossless with Alpha" for movies, settable in Edit > Templates > Render Settings > "Movie Proxy Default" / "Still Proxy Default"). After rendering, the output is set as the item's proxy and is in use.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-media, bae-ui-motion)
- **Notes:** Render Queue post-render actions: see ui-preview-render-io.md, Render Queue.

### AE-PRJ-078 · Proxy use when rendering (代理使用)

- **What it does:** Render Settings decide whether a render uses proxies.
- **Exact semantics:** Render Settings > Proxy Use (代理使用): Current Settings (当前设置, default; each item's `useProxy`) / Use All Proxies (使用所有代理) / Use Comp Proxies Only (仅使用合成代理) / Use No Proxies (不使用代理). UNVERIFIED: menu order and Chinese strings. Default render-settings template "Best Settings" uses Use No Proxies (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render (+ bae-media)
- **Notes:** Specified fully in ui-preview-render-io.md, Render Queue (render settings); listed here for the proxy data model. Scripting `RenderQueueItem` setting "Proxy Use".

## Solids

### AE-PRJ-079 · Solid footage items and Solid Settings dialog (纯色设置)

- **What it does:** Creates or edits a solid-colour footage item of a given size, pixel aspect ratio and colour.
- **Exact semantics:**
  - Create: Layer > New > Solid (Ctrl+Y / Cmd+Y) creates the item plus a layer in the active comp; File > Import > Solid creates only the item. Edit: Layer > Solid Settings (Ctrl+Shift+Y / Cmd+Shift+Y) on a solid layer, or on a solid item in the Project panel.
  - Dialog fields in UI order:
    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Name (名称) | string | "<colour name> Solid <n>" (e.g. "Black Solid 1"), updated as colour changes until edited | — | — | UNVERIFIED: AE's colour-naming table |
    | Width (宽度) | number | comp width (no comp: last used) | 1..30000 px | per Units | Lock Aspect Ratio to <ratio> checkbox |
    | Height (高度) | number | comp height | 1..30000 px | per Units | — |
    | Units (单位) | enum | pixels | pixels (像素) / inches (英寸) / millimeters (毫米) / % of composition (合成的 %) | — | inches and mm use 72 ppi (UNVERIFIED); % relative to comp size |
    | Pixel Aspect Ratio (像素长宽比) | enum | comp PAR | AE-PRJ-053 list | — | — |
    | Frame Aspect Ratio | read-only | — | — | — | shown, e.g. "16:9 (1.78)" |
    | Make Comp Size (制作合成大小) | button | — | — | — | sets width, height and PAR to the comp's |
    | Color (颜色) | RGB | last used; initially 17% grey 45/255 (AE CS6+, per Adobe help) | 0..1 per channel (picker in 8-bit or float per project depth) | — | — |
    | Affect all layers that use this solid (影响使用此纯色的所有图层) | bool | off | — | — | Solid Settings only, AE-PRJ-080 |
  - The item is `isStill`, duration 0; it renders as a uniform colour with alpha 1 over its whole frame. Colour is stored in the project working space (no colour conversion; with colour management on it is interpreted as working-space values). UNVERIFIED.
  - New items go to the solids folder (AE-PRJ-021).
  - Each New Solid creates a new item (solids are not reused automatically even if identical). UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion, bae-ui-motion)
- **Notes:** Scripting `ItemCollection.addSolid(color, name, width, height, pixelAspect, duration)`, `SolidSource.color`; `LayerCollection.addSolid(...)`. Solid layers (layer type) and Lottie `ty: 1` (`sc`, `sw`, `sh`): see project-comp-layers.md, layer types. Null and adjustment layers created by AE also have solid sources (white 100x100 null solid "Null 1"; comp-size adjustment solid). UNVERIFIED: AE 2026 still creates solid items for nulls.

### AE-PRJ-080 · Affect all layers that use this solid (影响使用此纯色的所有图层)

- **What it does:** Decides whether editing a solid layer's settings changes the shared solid item or forks a new one.
- **Exact semantics:**
  ```
  SolidSettings(layer L, new settings S, affect_all):
    item = L.source
    if affect_all or item.usedIn has only L (single use):     # UNVERIFIED single-use shortcut
        item.width/height/par/color/name = S                 # all layers using item change
    else:
        new_item = new solid item with S (in solids folder); L.source = new_item
  ```
  - Size changes keep the layer's Anchor Point/Position values (the anchor point is not re-centred). UNVERIFIED: AE re-centres the anchor point when the anchor was at the old centre.
  - Editing a solid item from the Project panel always affects all its layers.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** UNVERIFIED: whether the checkbox is hidden or disabled when only one layer uses the solid.

## Placeholders

### AE-PRJ-081 · Import placeholder (导入占位符)

- **What it does:** Creates a footage item that stands in for footage not yet available, with chosen name, size, frame rate and duration.
- **Exact semantics:** File > Import > Placeholder (占位符) dialog:
  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Name (名称) | string | "Placeholder" (UNVERIFIED) | — | — |
  | Width (宽度) | int | UNVERIFIED: 640 or last comp width | 1..30000 | px |
  | Height (高度) | int | UNVERIFIED: 480 or last comp height | 1..30000 | px |
  | Frame Rate (帧速率) | float | UNVERIFIED: 30 | 1..999 | fps |
  | Duration (持续时间) | time | UNVERIFIED: 0:00:30:00 | ≥ 0 (0 => still) | display style |
  - Creates a `PlaceholderSource` item with `footageMissing = true`. Recommended: match the expected real footage.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Project.importPlaceholder(name, width, height, frameRate, duration)`; `FootageItem.replaceWithPlaceholder(...)`.

### AE-PRJ-082 · Placeholder rendering (占位符显示)

- **What it does:** Placeholders and missing footage render as a colour-bars test image.
- **Exact semantics:** Every frame of a placeholder (or missing footage, AE-PRJ-071) renders as a colour-bars image at the item's size, alpha 1. The pattern is AE's own colour-bars still. UNVERIFIED: exact bar layout and values (believed 75% SMPTE-style vertical bars: grey/white, yellow, cyan, green, magenta, red, blue, with lower bands). Effects, masks and transforms apply normally. In Draft/Adaptive resolution it is downsampled like footage.
- **Priority:** P1
- **Owner:** bae-render (+ bae-media)
- **Notes:** BAE decision: generate the pattern procedurally in a shader (no bundled image). Exact colours are not important for fidelity; document BAE's pattern in the renderer.

### AE-PRJ-083 · Replacing placeholders and output placeholders (替换占位符 / 输出占位符)

- **What it does:** Placeholders are swapped for real footage with Replace Footage, or are created from Render Queue outputs and replaced automatically after rendering.
- **Exact semantics:**
  - Replace: select the placeholder, File > Replace Footage > File (or double-click it): all uses switch to the real file; masks, effects, keyframes, expressions on layers are kept. Interpretation defaults to the new file's (AE-PRJ-059 applies). UNVERIFIED.
  - Output placeholder: dragging a Render Queue output module heading into the Project panel creates a placeholder for that output (name/size/rate/duration of the output) and sets the item's Post-Render Action to Import & Replace Usage (导入和替换用法); when the output finishes rendering, every use of the placeholder is replaced by the rendered file. A comp that uses an output placeholder of another queued item can render after it (pre-render chains; render order by queue order).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-model, apps/bae)
- **Notes:** Post-render actions: see ui-preview-render-io.md, Render Queue.

## Project Settings

### AE-PRJ-084 · Project Settings dialog (项目设置)

- **What it does:** Stores project-wide settings that affect every composition: rendering backend, time display, colour pipeline, audio rate and expression engine.
- **Exact semantics:**
  - File > Project Settings (Ctrl+Alt+Shift+K / Cmd+Opt+Shift+K), also from the Project panel menu and by clicking the bit-depth button at the bottom of the Project panel (opens the Color tab).
  - Tabs in UI order: Video Rendering and Effects (视频渲染和效果), Time Display Style (时间显示样式), Color (颜色), Audio (音频), Expressions (表达式). UNVERIFIED: AE 26/27 may add further tabs (e.g. 3D).
  - All settings are saved in the project file (not in preferences), are undoable as one step per OK, and invalidate caches when they change rendering (bit depth, colour, GPU type does not change results by design).
  - New projects take the defaults below, or the template project's settings (AE-PRJ-012).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** BAE decision: Project Settings live on the `Project` Prim in the root sublayer (ARCHITECTURE 4.3 "root: 工程设置"); a stronger sublayer may not override them (UNVERIFIED design choice; to be decided by core/doc-model).

### AE-PRJ-085 · Video Rendering and Effects setting (视频渲染和效果)

- **What it does:** Chooses the GPU or CPU backend AE uses for GPU-capable effects and the renderer.
- **Exact semantics:** Use (使用): Mercury GPU Acceleration (Metal) (macOS) / Mercury GPU Acceleration (CUDA) (NVIDIA, Windows) / Mercury GPU Acceleration (OpenCL) (deprecated) / Mercury Software Only (仅 Mercury 软件). Default: the best GPU type available on the machine; Software Only if no supported GPU. Results are intended to be identical across backends (small floating-point differences exist). Scripting `Project.gpuAccelType` (`GpuAccelType.CUDA | METAL | OPENCL | SOFTWARE`, AE 13.8) and `app.availableGPUAccelTypes`. Opening a project saved with an unavailable backend falls back silently. UNVERIFIED: whether OpenCL is still listed on Windows in AE 26.
- **Priority:** P2 — BAE is GPU-only via wgpu.
- **Owner:** bae-model (+ bae-render)
- **Notes:** BAE decision: store the AE value for round-trip only; BAE always renders through wgpu (Metal/Vulkan/DX12) and a CPU fallback is not an M1 goal. Effect GPU/CPU parity is a per-effect concern (effects.md).

### AE-PRJ-086 · Time Display Style (时间显示样式)

- **What it does:** Chooses how times are shown throughout the project: SMPTE timecode, frame numbers, or feet+frames; display only, never timing.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Values | Notes |
  |---|---|---|---|---|
  | Timecode (时间码) | radio | selected | — | `TimeDisplayType.TIMECODE` |
  | Footage Start Time (素材开始时间) | enum | Use Media Source (使用媒体源) | Use Media Source / Start at 00:00:00:00 (从 00:00:00:00 开始) | `footageTimecodeDisplayStartType`: `FTCS_USE_SOURCE_MEDIA` / `FTCS_START_0`; only with Timecode |
  | Frames (帧) | radio | — | — | `TimeDisplayType.FRAMES` |
  | Use Feet + Frames (使用英尺 + 帧数) | bool + enum | off; 35mm | 35mm (16 frames per foot) / 16mm (40 frames per foot) | `framesUseFeetFrames`, `feetFramesFilmType` (`MM35`, `MM16`) |
  | Frame Count (帧计数) | enum | UNVERIFIED: Timecode Conversion | Timecode Conversion (时间码转换) / Start at 0 (从 0 开始) / Start at 1 (从 1 开始) | `framesCountType` (`FC_TIMECODE_CONVERSION`, `FC_START_0`, `FC_START_1`); `displayStartFrame` 0/1 shortcut |
  ```
  Timecode display of time t (seconds) at frame rate r, comp/footage start offset s:
    base = round(r)                         (29.97 -> 30, 23.976 -> 24, 59.94 -> 60)
    n = floor((t + s) * r + eps)            frame count
    non-drop:  HH:MM:SS:FF  with FF = n mod base, SS = (n / base) mod 60, ...   separators ':'
    drop-frame (only r = 29.97 or 59.94; Composition Settings choose DF/NDF):
       d = 2 (29.97) or 4 (59.94) frame numbers dropped at the start of every minute except minutes divisible by 10
       separators ';'   (e.g. 0;00;59;29 -> 0;01;00;02)
  Frames display:   Start at 0 -> n ; Start at 1 -> n + 1 ; Timecode Conversion -> n including the source/comp start timecode offset
  Feet + Frames:    feet = n div fpf, frames = n mod fpf (fpf = 16 or 40), shown as "ffff+ff"
  ```
  - Ctrl/Cmd-click on any current-time display cycles Timecode <-> Frames/Feet+Frames for the project.
  - "Show Both Timecode and Frames in Timeline Panel" (Preferences > Appearance, default on) shows the other unit in small text.
  - Drop-frame for a comp is chosen in Composition Settings (see project-comp-layers.md, Composition Settings); footage DF/NDF follows the source.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** UNVERIFIED: the frame-count default. Scripting `timeDisplayType`, `framesCountType`, `displayStartFrame`, `footageTimecodeDisplayStartType`, `framesUseFeetFrames`, `feetFramesFilmType`; `timeToCurrentFormat()` / `currentFormatToTime()` globals use these settings. BAE decision: BAE times are flicks; display conversion uses the exact rational rate, so drop-frame maths is exact.

### AE-PRJ-087 · Color settings (颜色设置)

- **What it does:** Stores the project's colour pipeline choices: colour engine, bit depth, working space and blending options.
- **Exact semantics:** Project Settings > Color tab, stored values (render semantics: compositing-3d-color.md, colour management; bit depth):
  | Parameter (中文) | Type | Default | Values | Scripting / notes |
  |---|---|---|---|---|
  | Color Engine (颜色引擎) | enum | Adobe color managed (Adobe 颜色管理（译名待核）) | Adobe color managed / OCIO color managed (OCIO 颜色管理（译名待核）) | OCIO engine shipped in AE 23.2 (Feb 2023; beta from 23.1, Oct 2022); OCIO library 2.3.2 in AE 25.0; OCIO 2.5.1 with ACES 2.0 configs since AE 26.5 (Sep 2026) |
  | Bit Depth (位深度) | enum | 8 bits per channel | 8 / 16 / 32 (float) bits per channel (位/通道) | `bitsPerChannel`; Alt/Option-click the Project panel depth button cycles it |
  | Working Space (工作空间) | enum | None (无) | None or any RGB ICC profile (e.g. sRGB IEC61966-2.1, HDTV (Rec. 709), Rec.709 Gamma 2.4, Rec.2020, ProPhoto RGB...) | `workingSpace` ("" = None); `listColorProfiles()`; None = colour management off |
  | Working gamma (工作伽玛（译名待核）) | enum | 2.4 | 2.2 / 2.4 | `workingGamma`; used only when Working Space is None (affects some gamma-dependent operations); UNVERIFIED default |
  | Linearize Working Space (线性化工作空间) | bool | off | — | `linearizeWorkingSpace`; available only with a working space |
  | Blend Colors Using 1.0 Gamma (使用 1.0 灰度系数混合颜色) | bool | off | — | `linearBlending`; affects only blending between layers |
  | Match Legacy After Effects QuickTime Gamma Adjustments (匹配旧版 After Effects QuickTime 灰度系数调整（译名待核）) | bool | off | — | legacy 1.8/2.2 gamma handling for QuickTime |
  | Compensate for Scene-referred Profiles (补偿场景引用的配置文件（译名待核）) | bool | UNVERIFIED: on | — | `compensateForSceneReferredProfiles` (AE 16.0) |
  | OCIO Configuration (OCIO 配置) | enum/path | ACES 2.0 Studio v4.0 (UNVERIFIED default) | built-in configs (ACES 1.3 Studio/CG, ACES 2.0 CG v4.0, ACES 2.0 Studio v4.0) / Custom (.ocio file) | OCIO engine only |
  | Working Color Space (OCIO) (工作色彩空间) | enum | config default (e.g. ACEScg) | any scene-linear space of the config | OCIO only |
  | Display Color Space (OCIO) (显示色彩空间) | enum | config default display/view | config displays/views | OCIO only; also switchable in the viewer |
  - "Blend Text Colors Using 1.0 Gamma": not present in AE (UNVERIFIED); text blending follows the layer blending option.
  - Changing bit depth or working space converts nothing stored (colours are stored as floats); results re-render.
- **Priority:** P0 — bit depth and working space; OCIO fields P1.
- **Owner:** bae-model (+ bae-color)
- **Notes:** Owner of semantics: compositing-3d-color.md (bae-color). BAE decision: BAE may default new projects to 16 bpc or 32 bpc float internally for performance/quality (GPU), but must expose and store the AE setting so AE-faithful 8 bpc rounding can be reproduced when requested.

### AE-PRJ-088 · Audio sample rate (音频设置 - 采样率)

- **What it does:** Sets the sample rate at which AE mixes and previews audio for the project.
- **Exact semantics:** Project Settings > Audio > Sample Rate (采样率): enum, default 48.000 kHz (UNVERIFIED). Values (UNVERIFIED list): 22.050, 32.000, 44.100, 48.000, 88.200, 96.000 kHz. All audio sources are resampled to this rate for preview and render mixing; Output Module audio settings may resample again for output (see ui-preview-render-io.md, Render Queue audio).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** UNVERIFIED: AE version that moved the default from 44.1 to 48 kHz; the offered list may differ by platform (a 2021 Adobe community thread reports a macOS AE 18.2 offering nothing above 48 kHz while Windows showed 96 kHz). Scripting exposes no Project attribute for the audio sample rate (UNVERIFIED).

### AE-PRJ-089 · Expression engine setting (表达式引擎)

- **What it does:** Chooses which JavaScript engine evaluates the project's expressions.
- **Exact semantics:** Project Settings > Expressions > Expression Engine (表达式引擎): JavaScript (default for new projects since AE 16.0 (CC 2019); ECMAScript 2018) / Legacy ExtendScript (旧版 ExtendScript, ECMAScript 3). Scripting `Project.expressionEngine` = `"javascript-1.0"` or `"extendscript"`. Projects saved before AE 16.0 open with Legacy ExtendScript. Switching re-evaluates all expressions; syntax valid only in one engine produces errors in the other.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Engine semantics and the incompatibility list: see transform-keyframes-expressions.md, expression engines. BAE decision: BAE evaluates both settings with QuickJS (modern JS), applying a legacy-compatibility shim for the documented ExtendScript differences when the stored setting is Legacy ExtendScript.

## Undo, history and memory purge

### AE-PRJ-090 · Undo and redo (撤消 / 重做)

- **What it does:** Steps backward and forward through changes to project data, one named action at a time.
- **Exact semantics:**
  - Edit > Undo <action name> (Ctrl/Cmd+Z), Edit > Redo <action name> (Ctrl+Shift+Z / Cmd+Shift+Z). The menu item shows the action name (e.g. "Undo Change Position", "Undo Import").
  - Depth: up to 99 most recent actions. Older AE had Preferences > General > Levels of Undo (CS4-era default 32, max 99); the field was removed from the dialog in AE 15.0 (Oct 2017) and the value is 99 unless edited in the preferences text file (source: Adobe community answer; UNVERIFIED for AE 26/27).
  - Any new undoable action after an undo discards the redo stack.
  - One user gesture = one undo step: a whole drag (scrub) is one step; a multi-layer operation is one step; a script wrapped in `app.beginUndoGroup(name)` / `app.endUndoGroup()` is one step named `name`.
  - Undo history is per session: it is cleared by opening/closing/reverting a project and is not saved in the file. Saving does not clear it.
  - Cached frames are kept across edits and reused after undo/redo (global RAM cache).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** BAE decision (ARCHITECTURE 4.5): every Command execution is one undo unit and carries an `author`; undo/redo is author-scoped (a user undoes only their own Commands, never collaborators' or another agent's), implemented as inverse Ops applied on top of the CRDT state. BAE may keep a deeper (unbounded or memory-bounded) history; 99 is AE's limit, not a requirement.

### AE-PRJ-091 · Undoable scope (可撤消的操作（译名待核）)

- **What it does:** Defines which actions enter the undo stack.
- **Exact semantics:**
  | Undoable (project data) | Not undoable |
  |---|---|
  | property values, keyframes, expressions | panel scrolling, zoom/pan of viewers, viewer resolution/magnification |
  | layer create/delete/reorder/trim/switches | tool activation, panel/workspace layout |
  | items: import, delete, rename, move to folder, interpretation, proxy set, solid settings | preferences, keyboard shortcuts |
  | comp settings, work area, markers | CTI (current time) moves; layer/item/keyframe selection (UNVERIFIED: some selection changes are recorded as part of other actions) |
  | project settings | Revert (cannot be undone), Purge, Save |
  | render queue add/remove/settings (UNVERIFIED) | rendering itself; Reload Footage (UNVERIFIED) |
  - Locking a layer prevents accidental modification but the lock switch itself is undoable.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** BAE decision: view state (zoom, CTI, selection, solo-for-preview) lives in BAE's non-persisted `session` sublayer (ARCHITECTURE 4.3) and therefore never enters undo; selection changes may optionally be restored together with the undone Command for usability.

### AE-PRJ-092 · Edit > History (历史记录)

- **What it does:** Lists recent undoable actions so the user can jump back several steps at once.
- **Exact semantics:** Edit > History submenu lists recent actions (most recent first); choosing one undoes that action and every action after it (redo remains available for them). AE has no History panel. UNVERIFIED: number of entries listed (believed the full undo stack, up to 99).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: BAE adds a History panel listing Commands with author, time and affected Prims, filterable by author; jumping is author-scoped like undo (ARCHITECTURE 4.5). Long-term versions/snapshots are a separate feature (ARCHITECTURE 7).

### AE-PRJ-093 · Purge (清理)

- **What it does:** Frees caches and the undo stack on demand.
- **Exact semantics:** Edit > Purge (清理) submenu, AE 26 order:
  | Command (中文) | Effect |
  |---|---|
  | All Cache (所有缓存（译名待核）) | all RAM caches + disk cache + 3D cache (UNVERIFIED) |
  | All Memory & Disk Cache (所有内存和磁盘缓存) | RAM caches and the disk cache (same as Empty Disk Cache in Media & Disk Cache preferences); not other AE versions' caches |
  | All Memory (所有内存) | all RAM caches (rendered frames, image buffers) and the undo stack; Alt/Option + All Memory also forces resynchronisation of the project database |
  | All Disk Cache (所有磁盘缓存（译名待核）) | disk cache only |
  | All 3D Cache (所有 3D 缓存（译名待核）) | Advanced 3D renderer caches |
  | Undo (撤消) | clears the undo (and redo) stack |
  | Image Cache Memory (图像缓存内存) | RAM frame cache only |
  | Snapshot (快照) | clears the stored viewer snapshots |
  - Purge is not undoable; it never changes project data.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-render, bae-model)
- **Notes:** UNVERIFIED: which items exactly All Memory clears in AE 26 (believed: undo + image caches). Cache behaviour: see ui-preview-render-io.md, preview & cache. BAE decision: provide the same commands; "Undo" purge clears only the local author's stacks.

## Dependencies and packaging

### AE-PRJ-094 · Collect Files (收集文件)

- **What it does:** Copies the project and the source files it needs into one new folder, with a report of files, effects and fonts, so the project can be archived, moved or rendered elsewhere.
- **Exact semantics:** File > Dependencies > Collect Files (文件 > 依赖项 > 收集文件). AE asks to save the project first if it is dirty. Dialog fields in UI order:
  | Parameter (中文) | Type | Default | Values / Notes |
  |---|---|---|---|
  | Collect Source Files (收集源文件) | enum | UNVERIFIED: All | All (全部): all footage incl. unused and proxies / For All Comps (对于所有合成): footage + proxies used by any comp / For Selected Comps (对于选定合成): used by comps selected in the Project panel (directly or nested) / For Queued Comps (对于已加入队列的合成): used directly or indirectly by Queued render items / None (Project Only) (无（仅项目）): copy project only |
  | Generate Report Only (仅生成报告) | bool | off | write the report, copy nothing |
  | Obey Proxy Settings (服从代理设置) | bool | off | on: copy only what the current proxy settings use (proxy or original); off: copy both proxies and originals. For Queued Comps uses the render settings' Proxy Use instead |
  | Reduce Project (减少项目) | bool | off | only with For All / Selected / Queued Comps: the collected project has unused items removed (as AE-PRJ-095) |
  | Change render output to <name> folder (将渲染输出更改为 … 文件夹) | bool + string | off; folder name UNVERIFIED | re-targets output modules with valid status (Queued, Unqueued, Will Continue) to a folder inside the collection |
  | Enable 'Watch Folder' render (启用"监视文件夹"渲染) | bool | off | writes `<project name>_RCF.txt` render control file (AE-PRJ-098) |
  | Maximum Number of Machines (计算机的最大数目) | int | UNVERIFIED: 10 | only with Watch Folder; number of render engines allowed; dialog shows how many items render multi-machine |
  | Comments (注释) | button/text | empty | appended at the end of the report |
  | Info text | read-only | — | "N file(s) (size) will be collected" summary |
  - Output: the user names a new folder (default "<project name> folder", UNVERIFIED) that receives:
    ```
    <name> folder/
      <project name>.aep               copy whose footage paths point into (Footage)
      <project name>Report.txt         report
      (Footage)/                       copies of footage; sub-folders mirror the Project panel folder tree
      (<output folder>)/               only with "Change render output to"
      <project name>_RCF.txt           only with Enable Watch Folder render
    ```
    Folder names in parentheses tell watch-folder render engines not to search them. Image sequences are copied with all their frames (sequence frames into their own sub-folder, UNVERIFIED). Solids and placeholders have no files. Missing files are listed, not copied.
  - Report contents: collection date, project/collected-project names, collection options, number and total size of collected files, list of missing footage (with last known paths), effects used (name, count, and which layers/comps; UNVERIFIED detail level), fonts used (PostScript names), render settings / output modules for queued items when relevant, and the user's comments. UNVERIFIED: exact layout and fields.
  - The open project is unchanged and remains open (later edits are not in the collected copy).
- **Priority:** P1
- **Owner:** bae-package (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision (ARCHITECTURE 16): BAE offers both "Collect files to folder" (AE-style directory, paths rewritten relative) and a single-file `.baez` zip package (media stored uncompressed, project data compressed, zip64), with options: only used assets, optional trimming of video to used ranges (original media and project untouched), only used sequence frames, optional font embedding (licence permitting), and a report of missing assets, sizes and trim results. Fonts and effects are not collected by AE; BAE may embed fonts.

### AE-PRJ-095 · Reduce Project (精简项目)

- **What it does:** Deletes every comp not selected and not nested inside a selected comp, plus all footage they no longer need.
- **Exact semantics:**
  ```
  keep = selected comps (Project panel) ∪ all comps/footage reachable from them through layer sources
         (precomp layers, footage layers, track-matte/source layers), recursively;
         reachability includes layers whose Video/Audio switches are off
  delete every item not in keep (comps and footage; folders are kept? UNVERIFIED: empty folders are removed)
  ```
  - Available only when the Project panel is active and at least one comp is selected.
  - Expressions that refer to a non-subordinate comp: that comp is deleted and the expression breaks (Adobe: "removes the nonsubordinate composition and the applied expression"); AE shows a message afterwards reminding that the command can be undone. Workaround: nest the referenced comp into the referencing comp with its switches off.
  - Shows a count of removed items; undoable as one step. UNVERIFIED message text.
  - Render-queue items of deleted comps are removed.
- **Priority:** P1
- **Owner:** bae-package (+ bae-model)
- **Notes:** Scripting `Project.reduceProject(array_of_items)` returns the number of removed items. BAE decision: offer an option (default on) that also treats expression references (`comp("...")`, `footage("...")`) as reachability edges, avoiding AE's silent expression breakage; the report lists any items kept only because of expressions.

### AE-PRJ-096 · Remove Unused Footage (移除未使用素材)

- **What it does:** Deletes every footage item that no comp uses.
- **Exact semantics:** File > Dependencies > Remove Unused Footage: deletes footage items (files, solids, placeholders) whose `usedIn` is empty; comps are never removed even if unused; footage used only as a proxy does not exist as an item. Shows the number of items removed (e.g. "N unused footage items removed"); undoable. Footage referenced only by expressions (e.g. data files read by `footage()`) is unused and gets removed. UNVERIFIED: message text; whether a footage item used as a layer in an unused comp counts as used (yes: any comp counts).
- **Priority:** P1
- **Owner:** bae-model (+ bae-package)
- **Notes:** Scripting `Project.removeUnusedFootage()` returns the count. BAE decision: treat expression `footage("name")` references as uses.

### AE-PRJ-097 · Consolidate All Footage (整合所有素材)

- **What it does:** Merges duplicate footage items that reference the same source with identical interpretation.
- **Exact semantics:** File > Dependencies > Consolidate All Footage: for each group of footage items with the same source file (same path; for sequences same file set) and identical Interpret Footage settings, keeps one item and retargets every layer that used another member to it, then deletes the others. Solids are consolidated when colour, size and PAR match (UNVERIFIED). Items with different interpretation are not merged. Shows the count; undoable. Which member survives: UNVERIFIED (believed the first in project order).
- **Priority:** P2
- **Owner:** bae-model (+ bae-package)
- **Notes:** Scripting `Project.consolidateFootage()` returns the count.

### AE-PRJ-098 · Watch Folder rendering (监视文件夹)

- **What it does:** Lets one or more AE instances or render engines watch a folder and render projects collected into it.
- **Exact semantics:** File > Watch Folder starts watch mode on a chosen folder (also: a project named `Watch This Folder.aep` puts AE in watch mode for its folder; Windows `afterfx.exe -wf <folder>`; `-re` for render engine). AE renders each project that has a render control file `<project>_RCF.txt` (written by Collect Files with Enable Watch Folder render), in alphabetical order, splitting image-sequence renders across up to Maximum Number of Machines. Fonts, effects and codecs must be installed on every machine; movie files cannot be split across machines. Folders in parentheses are not searched.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-package)
- **Notes:** Headless rendering (`aerender`) is specified in ui-preview-render-io.md. BAE decision: watch-folder rendering is out of M1; a `bae render` CLI plus an external queue covers the use case.

## Composition item and Composition Settings

### AE-PRJ-300 · Composition item data model (合成)

- **What it does:** A composition is a project item that owns an ordered stack of layers, a time base (frame rate, duration, start timecode), a frame (width, height, pixel aspect) and comp-level render switches. It is both the unit of editing and the unit nested as a precomp layer or rendered by the Render Queue.
- **Exact semantics:** Persisted fields of a `Comp` prim (each field is specified in its own item below; this table is the canonical list for `bae-model`):
  | Field (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | name (合成名称) | string | "Comp 1" (合成 1) | non-empty | — | AE-PRJ-301 |
  | width / height (宽度 / 高度) | int | 1920 / 1080 UNVERIFIED: sticky, see AE-PRJ-303 | 4..30000 | px | AE-PRJ-303 |
  | pixelAspect (像素长宽比) | f64 | 1.0 | 0.01..100.0 | ratio | AE-PRJ-304 |
  | frameRate (帧速率) | f64 | 29.97 UNVERIFIED (sticky) | 1..999 UNVERIFIED (scripting doc says 1..99) | fps | AE-PRJ-305 |
  | dropFrame | bool | true when fps is 29.97/59.94 UNVERIFIED | — | — | AE-PRJ-305 |
  | resolutionFactor (分辨率) | [int; 2] | [1, 1] | 1..99 each | px skip | AE-PRJ-306 |
  | displayStartTime / displayStartFrame (开始时间码 / 开始帧) | time | 0 | -3:00:00:00..23:59:00:00 | s | AE-PRJ-307 |
  | duration (持续时间) | time | 0:00:30:00 UNVERIFIED (sticky) | ≥ 1 frame .. 10800 s | s | AE-PRJ-308 |
  | bgColor (背景颜色) | RGB f32×3 | [0, 0, 0] | 0..1 | — | AE-PRJ-309 |
  | preserveNestedFrameRate | bool | false | — | — | AE-PRJ-311 |
  | preserveNestedResolution | bool | false | — | — | AE-PRJ-312 |
  | shutterAngle / shutterPhase / motionBlurSamplesPerFrame / motionBlurAdaptiveSampleLimit | int | 180 / -90 / 16 / 128 | see AE-PRJ-313 | deg / samples | AE-PRJ-313 |
  | renderer (渲染器) | enum | Classic 3D | see AE-PRJ-314 | — | AE-PRJ-314 |
  | motionBlur, frameBlending, hideShyLayers, draft3d | bool | see AE-PRJ-315 | — | — | AE-PRJ-315 |
  | workAreaStart / workAreaDuration (工作区域) | time | 0 / duration | inside [0, duration] | s | AE-PRJ-323 |
  | time (current time indicator, 当前时间指示器) | time | 0 | [0, duration) | s | persisted per comp |
  | markers (合成标记) | Marker[] | [] | — | — | AE-PRJ-326 |
  | layers (图层) | Layer[] | [] | index 1 = top | — | AE-PRJ-331 onward |
  | comment (注释), label (标签), id | from item base | label Sandstone UNVERIFIED (index 15, see AE-PRJ-393) | — | — | item base fields are owned by prj-a |
- Derived, read-only values: `frameDuration = 1 / frameRate`; `frameAspect = width * pixelAspect / height`; `numLayers`; `activeCamera` (AE-PRJ-315); `hasVideo` = true; `hasAudio` = any layer has audio; `usedIn` = comps that contain a precomp layer of this comp.
- Every field listed here is undoable and is a separate CRDT attribute (not a single blob) so that two users can change, for example, the duration and the background colour at the same time.
- **Priority:** P0 the core document object.
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem` (extends `AVItem` → `Item`). Lottie: the root animation object is a comp (`w`, `h`, `fr`, `ip`, `op`, `layers`, `markers`, `mb`, `ddd`); nested comps live in `assets[]` with `id` + `layers`. Lottie has no background colour, no PAR and no start timecode. Time values are stored as flicks (705,600,000 per second) per ARCHITECTURE.md 4.4; the scripting API exposes seconds as f64. Item base fields (name uniqueness rules, folders, comment, label column, item ID) are specified in project-comp-layers.md [prj-a].

### AE-PRJ-301 · Composition Name (合成名称)

- **What it does:** The display name of the comp item, also used as the default source name of any precomp layer that references it.
- **Exact semantics:**
  - New comps are named `Comp N` (zh-CN `合成 N`), where N is the smallest positive integer such that the name is not already used by another comp in the project UNVERIFIED: AE uses "one more than the highest existing Comp N".
  - Names need not be unique; AE allows duplicate comp names (expressions that use `comp("name")` then resolve to the first match in project item order).
  - Comps created from footage take the footage item name (file name without extension for New Comp from Selection, AE-PRJ-317); Precompose uses "Pre-comp N" (预合成 N), AE-PRJ-401.
  - Renaming a comp updates the source name of every precomp layer that uses it; precomp layers with a custom layer name keep their layer name (AE-PRJ-388).
  - Renaming does NOT rewrite expressions that reference the comp by name (`comp("Old")`); these then fail. UNVERIFIED: AE 13.x+ updates expression references on layer renames inside the same comp, not on comp renames.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Scripting: `CompItem.name`. Lottie: `nm` on the root and on `assets[i]` is optional. BAE decision: Expressions should reference comps by stable ID internally and display the name, so renames never break links; still accept `comp("name")` from imported AE projects.

### AE-PRJ-302 · Composition Settings presets (合成设置预设)

- **What it does:** A Preset (预设) menu in the Composition Settings dialog fills Width, Height, Pixel Aspect Ratio and Frame Rate in one step; users can save and delete their own presets.
- **Exact semantics:**
  - A preset stores exactly: width, height, pixelAspect, frameRate (and the drop-frame choice for 29.97/59.94 UNVERIFIED). It does NOT store resolution, start timecode/frame, duration, background colour or any Advanced or 3D Renderer tab setting.
  - Choosing a preset sets those four fields; editing any of them afterwards switches the menu to "Custom" (自定义).
  - Save (保存) asks for a name and appends the preset; Delete (删除) removes the selected preset; Alt/Option-click on Save or Delete restores the factory preset list.
  - Main factory presets (AE 2024-2026, order abbreviated; exact list UNVERIFIED):
    | Preset | W×H | PAR | fps |
    |---|---|---|---|
    | Social Media Portrait HD · 1080x1920 · 29.97 fps | 1080×1920 | 1.0 | 29.97 |
    | Social Media Landscape HD · 1920x1080 · 29.97 fps | 1920×1080 | 1.0 | 29.97 |
    | Social Media Square · 1080x1080 · 29.97 fps | 1080×1080 | 1.0 | 29.97 |
    | Social Media Portrait (4:5) UNVERIFIED | 1080×1350 | 1.0 | 29.97 |
    | HDTV 1080 23.976 / 24 / 25 / 29.97 | 1920×1080 | 1.0 | as named |
    | HD · 1920x1080 · 23.976 / 24 / 25 / 29.97 / 50 / 59.94 / 60 fps | 1920×1080 | 1.0 | as named |
    | HDV/HDTV 720 23.976 / 25 / 29.97 | 1280×720 | 1.0 | as named |
    | HDV 1080 25 / 29.97 | 1440×1080 | 1.33 | as named |
    | DVCPRO HD 720 / 1080 variants | 960×720 / 1280×1080 / 1440×1080 | 1.33 / 1.5 | as named |
    | UHD 4K 23.976 / 24 / 25 / 29.97 / 50 / 59.94 / 60 | 3840×2160 | 1.0 | as named |
    | 4K (DCI) 23.976 / 24 / 25 | 4096×2160 | 1.0 | as named |
    | UHD 8K 23.976 / 24 / 25 / 29.97 | 7680×4320 | 1.0 | as named |
    | NTSC DV / NTSC DV Widescreen / NTSC D1 / NTSC D1 Widescreen / NTSC D1 Square Pix | 720×480 / 720×486 / 720×534 | 0.91 / 1.21 / 1.0 | 29.97 |
    | PAL D1/DV / PAL D1/DV Widescreen / PAL D1/DV Square Pix | 720×576 / 768×576 | 1.09 / 1.46 / 1.0 | 25 |
    | Cineon Half / Cineon Full | 1828×1332 / 3656×2664 | 1.0 | 24 |
    | Film (2K) / Film (4K) | 2048×1556 / 4096×3112 | 1.0 | 24 |
    | Web Video, 320 x 240 / 640 x 480 | as named | 1.0 | 29.97 UNVERIFIED |
  - Custom presets are stored in application preferences (not in the project) and do not travel between machines.
- **Priority:** P1 presets are a convenience; the four fields themselves are P0.
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: store user presets in a portable JSON file in the user config directory so they can be shared. The factory list is data, not code. Preset names are not localised in AE's zh-CN UI except "Custom" (自定义).

### AE-PRJ-303 · Width, Height and Lock Aspect Ratio (宽度、高度、锁定长宽比)

- **What it does:** Sets the comp frame size in pixels; an optional lock keeps the frame aspect ratio when one dimension is edited.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Width (宽度) | int | 1920 UNVERIFIED (fresh prefs; otherwise last-used) | 4..30000 | px | |
  | Height (高度) | int | 1080 UNVERIFIED | 4..30000 | px | |
  | Lock Aspect Ratio to w:h (锁定长宽比为 w:h) | bool | false UNVERIFIED | — | — | dialog-only state, not stored in the comp |
  - The label shows the reduced ratio `w/g : h/g` with `g = gcd(w, h)` and the decimal `w/h` to 2 places, e.g. "16:9 (1.78)".
  - With the lock on, editing Width sets `height = round(width * h0 / w0)` and vice versa, where `w0, h0` are the values when the lock was turned on.
  - When the size changes on an existing comp, layers are repositioned according to the Anchor setting (AE-PRJ-310). Layer content is never scaled.
  - The help page warns that the practical maximum may be lower than 30000×30000 because a full 8 bpc frame of that size is about 3.5 GB.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem.width`/`height` (range [4..30000]). Lottie: `w`, `h` (integers). Width and height are the frame at Full resolution in comp pixels (non-square if PAR ≠ 1). Implementation hint: tile the frame buffer above the GPU max texture size (typically 16384) rather than lowering the 30000 limit.

### AE-PRJ-304 · Pixel Aspect Ratio (像素长宽比)

- **What it does:** Declares the shape of the comp's pixels so non-square formats display and composite correctly.
- **Exact semantics:** Menu values in AE order (same list as Interpret Footage, see project-comp-layers.md [prj-a]):
  | Menu entry | Stored value | Exact ratio |
  |---|---|---|
  | Square Pixels (方形像素) | 1.0 | 1/1 |
  | D1/DV NTSC (0.91) | 0.9091 | 10/11 |
  | D1/DV NTSC Widescreen (1.21) | 1.2121 | 40/33 |
  | D1/DV PAL (1.09) | 1.0940 | 128/117 UNVERIFIED |
  | D1/DV PAL Widescreen (1.46) | 1.4587 | 512/351 UNVERIFIED |
  | Anamorphic 2:1 (2) | 2.0 | 2/1 |
  | HDV 1080/DVCPRO HD 720 (1.33) | 1.3333 | 4/3 |
  | DVCPRO HD 1080 (1.5) | 1.5 | 3/2 |
  - Range 0.01..100.0 via scripting; the dialog only offers the menu.
  - Frame aspect ratio shown read-only: `width * pixelAspect / height`.
  - Layer sources with a different PAR are resampled into comp PAR at render time (cmp-a).
- **Priority:** P1 square pixels covers nearly all modern work; non-square must still load and render correctly.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `AVItem.pixelAspect`. Lottie has no PAR (assumed 1.0); exporting a non-square comp to Lottie needs a scale. The Composition viewer's "Toggle Pixel Aspect Ratio Correction" is UI only (ui-preview-render-io.md [ui-a]).

### AE-PRJ-305 · Frame Rate and drop-frame timecode (帧速率、丢帧)

- **What it does:** Sets how many frames per second the comp samples and renders, and for 29.97/59.94 whether timecode is displayed drop-frame.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Frame Rate (帧速率) | f64 | 29.97 UNVERIFIED (sticky) | 1..999 UNVERIFIED | fps | menu: 8, 10, 12, 12.5, 15, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120 UNVERIFIED; any typed value allowed |
  | Drop Frame / Non-Drop Frame (丢帧 / 无丢帧) | enum | Drop Frame UNVERIFIED | — | — | menu shown only when frame rate is 29.97 or 59.94 |
  - 23.976, 29.97, 59.94 are stored exactly as 24000/1001, 30000/1001, 60000/1001 when typed that way (AE rounds the typed decimal internally to 1001-based rates UNVERIFIED).
  - Drop-frame affects only display: frame numbers 0 and 1 (29.97) or 0-3 (59.94) are skipped at the start of every minute except minutes divisible by 10. The separator becomes `;` (00;00;00;00) for drop-frame and `:` for non-drop.
    ```
    // drop-frame display for 29.97 (D = 2), 59.94 (D = 4)
    fpsNom = round(fps)                // 30 or 60
    framesPer10Min = fpsNom*600 - D*9
    framesPerMin   = fpsNom*60  - D
    d = frame / framesPer10Min; m = frame % framesPer10Min
    frame += 9*D*d + (m > D ? D*((m - D) / framesPerMin) : 0)
    then split frame with fpsNom into hh;mm;ss;ff
    ```
  - Changing the frame rate never moves layers or keyframes in seconds; only the frame grid changes (keyframes not on the new grid stay off-grid).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting: `AVItem.frameRate` (doc range [1.0..99.0]; UI accepts higher, UNVERIFIED), `frameDuration`, `CompItem.dropFrame`. Lottie `fr`. Flicks represent all listed rates exactly (ARCHITECTURE.md 4.4); arbitrary typed rates (e.g. 7.5) are also exact if 705,600,000 is divisible by the frame duration numerator, otherwise BAE stores the rate as a rational and rounds frame times to the nearest flick. Timecode display base (Project Settings) is prj-a.

### AE-PRJ-306 · Resolution (分辨率)

- **What it does:** Sets the comp's render downsample factor for previews (and for rendering when Render Settings say "Current Settings").
- **Exact semantics:** Menu in AE order:
  | Entry (中文) | resolutionFactor |
  |---|---|
  | Full (完整) | [1, 1] |
  | Half (二分之一) | [2, 2] |
  | Third (三分之一) | [3, 3] |
  | Quarter (四分之一) | [4, 4] |
  | Custom... (自定义) | [x, y], each 1..99 UNVERIFIED (Custom Resolution dialog: "Render every x pixels horizontally, y pixels vertically") |
  - Rendered buffer size = `ceil(width / x) × ceil(height / y)` UNVERIFIED rounding.
  - The same value is shown and edited from the Composition viewer's resolution menu (which also offers Auto); "Auto" is a viewer preference, not a comp field.
  - Not saved in presets (AE-PRJ-302).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.resolutionFactor`. Rendering effects of resolution (effect parameter scaling, nested comp resolution) are specified in compositing-3d-color.md [cmp-a]. Not in Lottie.

### AE-PRJ-307 · Start Timecode / Start Frame (开始时间码 / 开始帧)

- **What it does:** Sets the timecode or frame number displayed for the comp's first frame. It changes labelling only, never rendering.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Start Timecode (开始时间码) | time | 0:00:00:00 | -3:00:00:00 .. 23:59:00:00 (AE 17.1+; before 17.1 minimum 0) | timecode | label "Start Frame" (开始帧) when Project Settings display style is Frames; in Frames mode default is 0 or 1 depending on Project Settings "Frame count" (start at 0 / start at 1) UNVERIFIED |
  - All internal times (layer startTime, keyframe times, expression `time`) are relative to comp time 0, not to displayStartTime. Displayed time = `displayStartTime + t`.
  - Expressions: `thisComp.displayStartTime` returns this value in seconds.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem.displayStartTime` (s, range [-10800.0..86339.0]), `displayStartFrame` (int, AE 17.1+). Not stored by Lottie. BAE decision: store as an integer frame count plus the comp frame rate to avoid the float issues AE 17.1 had to work around.

### AE-PRJ-308 · Composition Duration (持续时间)

- **What it does:** Length of the comp timeline; frames at or beyond it do not exist.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Duration (持续时间) | time | 0:00:30:00 UNVERIFIED (fresh prefs; sticky afterwards) | 1 frame .. 3:00:00:00 (10800 s) | timecode/frames | entered in the current display style, relative to Start Timecode ("is 0:00:30:00 Base 30" read-out) |
  - Valid frames are `0 .. ceil(duration * fps) - 1`; the comp's last frame starts at `duration - frameDuration`.
  - Changing duration never moves layers. Shortening it does not trim layer in/out points: layer bars and keyframes past the end are kept and simply not visible/renderable; lengthening reveals them again (AE-PRJ-319).
  - If the work area ended at the old duration, it follows the new end; otherwise it is clipped to `[0, duration]` (AE-PRJ-323).
  - Footage longer than 3 h can be used but times after 3 h display incorrectly (AE help).
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Scripting: `AVItem.duration` (read/write for comps, [0.0..10800.0]). Lottie root `op` - `ip` in frames (bodymovin exports the work area; see AE-PRJ-323). BAE decision: no hard 3 h limit is required internally (flicks in i64 cover ~418 years), but keep 3 h in the dialog for AE parity and project round-trip.

### AE-PRJ-309 · Background Color (背景颜色)

- **What it does:** The colour the Composition viewer shows behind all layers and that is used as the fill when rendering to formats without alpha.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Background Color (背景颜色) | RGB | black [0, 0, 0] | 0..1 per channel | — | colour picker + eyedropper; no alpha |
  - Rendering rule: the comp's rendered image always has the background as fully transparent (alpha 0) internally. The background colour is visible only (a) in the viewer when the transparency grid is off and (b) in output when the output channels are RGB only (no alpha), where the composite is flattened over the background colour.
  - When the comp is nested as a precomp layer, its background is transparent; the containing comp's background applies. To keep it, users add a solid layer.
  - With RGB+Alpha output, background colour does not appear in RGB for premultiplied output: pixels with alpha 0 are (0,0,0); for straight output, UNVERIFIED: AE writes the background colour into the RGB of fully transparent pixels ("matted with background color" for premultiplied is a Render Queue option, see ui-preview-render-io.md [ui-c]).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.bgColor` ([R, G, B] in 0..1). Expressions cannot read it. Lottie has no background (players draw transparent). Implementation hint: never write the background into the accumulation buffer; apply it only in the final display/export flatten pass, so precomps and alpha output stay correct for free.

### AE-PRJ-310 · Anchor for resizing (锚点)

- **What it does:** In the Advanced tab, a 3×3 grid of buttons chooses which point of the frame stays fixed when Width/Height change, so existing layers keep their position relative to that point.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Anchor (锚点) | enum 9 | Center (中) | TL, T, TR, L, C, R, BL, B, BR | — | dialog-only, not stored |
  ```
  ax = {L:0, C:0.5, R:1}[col]; ay = {T:0, C:0.5, B:1}[row]
  dx = (newW - oldW) * ax ; dy = (newH - oldH) * ay   // comp pixels
  for each layer with no parent (2D and 3D, incl. cameras and lights UNVERIFIED):
      Position += (dx, dy, 0)   // static value and every Position keyframe;
                                 // also X/Y Position when dimensions are separated
  ```
  - Parented layers are not moved (they follow their parent). Layer content and anchor points are not changed.
  - Applies only when the size changes through this dialog (and through Crop Comp to Region of Interest, which uses its own offset, AE-PRJ-320).
  - UNVERIFIED: layers whose Position is driven by an expression are not changed (the expression result is unaffected); BAE should warn.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Not a stored setting (scripting has no attribute). Implement as part of the "Set Comp Settings" command so the size change and all position offsets are one undo step.

### AE-PRJ-311 · Preserve frame rate when nested or in render queue (在嵌套时或在渲染队列中保留帧速率)

- **What it does:** Makes a comp keep its own frame rate when it is nested in a comp with another rate or rendered with a different Render Settings frame rate, producing held frames instead of resampling.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Preserve frame rate when nested or in render queue (在嵌套时或在渲染队列中保留帧速率) | bool | false | not inherited by new comps (not sticky), not in presets |
  - When true, any request for this comp at time t (from a parent comp or the render queue) is evaluated at `t' = floor((t + ε) * fps_own) / fps_own` (snap to this comp's frame grid, measured from this comp's time 0), so inner content changes only on this comp's frames.
  - The Render Queue frame rate override is ignored for this comp (output still has the requested frame count; frames repeat).
  - Data only here; rendering semantics and interaction with time remap/stretch are in compositing-3d-color.md [cmp-a] "nested comp res/fps".
- **Priority:** P1
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting: `CompItem.preserveNestedFrameRate`. Adobe recommends it instead of Posterize Time. ε: AE adds a small tolerance so that exact frame boundaries are not lost to float error; BAE uses exact flick arithmetic instead.

### AE-PRJ-312 · Preserve resolution when nested (在嵌套时保留分辨率)

- **What it does:** Makes a nested comp render at its own Resolution setting instead of inheriting the containing comp's resolution.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Preserve resolution when nested (在嵌套时保留分辨率) | bool | false | not sticky, not in presets |
  - false (default): a nested comp renders at the resolution factor of the comp that contains it (recursively from the top-level comp being previewed/rendered).
  - true: the nested comp renders at its own `resolutionFactor`, then is scaled to the parent's sampling.
- **Priority:** P2
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.preserveNestedResolution`. Render behaviour: compositing-3d-color.md [cmp-a].

### AE-PRJ-313 · Composition motion-blur settings (运动模糊设置)

- **What it does:** Comp-wide parameters for layer motion blur: how long the virtual shutter is open, when it opens relative to the frame, and how many sub-frame samples are taken.
- **Exact semantics:** Advanced tab, AE order:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shutter Angle (快门角度) | int | 180 | 0..720 | degrees | exposure = angle/360 frame durations |
  | Shutter Phase (快门相位) | int | -90 | -360..360 | degrees | shutter open offset from frame start |
  | Samples Per Frame (每帧采样数) | int | 16 | 2..64 | samples | minimum; used for 3D layers, shape layers and when motion cannot be measured (also for effects that do their own MB UNVERIFIED) |
  | Adaptive Sample Limit (自适应采样限制) | int | 128 | 16..256 | samples | maximum for 2D layers with adaptive sampling |
  ```
  frameDur = 1 / comp.frameRate
  shutterOpen  = t + (phase / 360) * frameDur      // default: t - 0.25 frame
  shutterClose = shutterOpen + (angle / 360) * frameDur  // default: t + 0.25 frame
  ```
  - With the defaults the exposure window is centred on the frame time. Angle 0 means no blur even when switches are on.
  - Sample placement and adaptive count: see transform-keyframes-expressions.md [ani-a] "motion-blur sampling"; accumulation: compositing-3d-color.md [cmp-a].
  - The Render Settings dialog can override the shutter angle (and use comp's phase) for a render; see ui-preview-render-io.md [ui-c].
- **Priority:** P0 motion blur with default shutter is basic AE output.
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting: `shutterAngle`, `shutterPhase`, `motionBlurSamplesPerFrame`, `motionBlurAdaptiveSampleLimit`. Lottie root `mb: {sa, sp, spf, asl}` (Shutter Angle, Shutter Phase, Samples per Frame, Adaptive Sample Limit; rarely supported by players). Expression access: `thisComp.shutterAngle`, `thisComp.shutterPhase`. These four settings are sticky for new comps like the rest of Composition Settings: AE help says new comps reuse the last-used settings except the two Preserve options (Source: https://helpx.adobe.com/after-effects/using/composition-basics.html).

### AE-PRJ-314 · 3D Renderer selection (3D 渲染器)

- **What it does:** Chooses the engine that renders 3D layers in the comp, and its options.
- **Exact semantics:** Renderer menu (menu order UNVERIFIED):
  | Entry (中文) | `CompItem.renderer` string | Versions | BAE |
  |---|---|---|---|
  | Classic 3D (经典 3D) | "ADBE Advanced 3d" (historical name: Classic 3D was called "Advanced 3D" in AE CS3-CS5) | all; default for new comps (renderer choice is sticky with the last-used Composition Settings) UNVERIFIED | P0 |
  | Advanced 3D (高级 3D) | "ADBE Calder" | AE 24.0 (October 2023) on | P2 |
  | CINEMA 4D (CINEMA 4D) | "ADBE Ernst" | AE 13.5/CC 2015 UNVERIFIED to present (deprecated, hidden by default in recent versions UNVERIFIED) | P2 |
  | Ray-traced 3D (光线追踪 3D) | "ADBE Picasso" | AE 11.0 (CS6) to AE 2019 UNVERIFIED; legacy projects only | not supported |
  - Renderer options (Options… button):
    - Classic 3D: Shadow Map Resolution (阴影图分辨率) UNVERIFIED values: Comp size-based default, 256, 512, 1024, 2048, 4096, 8000; specified in compositing-3d-color.md [cmp-b].
    - Advanced 3D: quality, anti-aliasing, ray-traced shadows / reflections, environment light behaviour (cmp-b).
    - CINEMA 4D: Quality slider (Draft … Typical … Extreme) controlling ray threshold/ray depth/reflection depth/shadow depth/anti-aliasing; Choose Cinema 4D Installation.
  - Classic 3D does not render extrusions/bevel (Geometry Options are hidden); switching renderer keeps extrusion data but does not render it (see compositing-3d-color.md [cmp-b], Advanced 3D / CINEMA 4D renderer).
  - Unknown renderer strings in an imported project fall back to Classic 3D with a warning.
- **Priority:** P0 for storing the field and Classic 3D; other renderers P2.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.renderer`, `CompItem.renderers` (installed modules). The match-name strings are counter-intuitive: "ADBE Advanced 3d" is CLASSIC 3D and "ADBE Calder" is the new Advanced 3D; an AE 2024 beta briefly renamed them ("ADBE Classic 3d"/"ADBE Advanced 3d"/"ADBE Cinema 4d") and Adobe reverted the change. Importers should accept both spellings. Source: https://community.adobe.com/questions-534/match-names-for-renderers-changing-314356. Lottie: no renderer field (players are effectively Classic 3D or 2D only). BAE decision: BAE's single GPU 3D renderer implements Classic 3D semantics first; Advanced 3D is a later addition behind the same enum.

### AE-PRJ-315 · Composition switches: Motion Blur, Frame Blending, Hide Shy Layers, Draft 3D, active camera (合成开关)

- **What it does:** Comp-level toggles in the Timeline panel header that globally gate per-layer switches, plus the derived active camera.
- **Exact semantics:**
  | Switch (中文) | Field | Default | Effect |
  |---|---|---|---|
  | Enables Motion Blur for all layers with the Motion Blur switch set (为设置了"运动模糊"开关的所有图层启用运动模糊) | `motionBlur` | true UNVERIFIED (on in new comps since AE CS6) | layer blur happens iff comp.motionBlur && layer.motionBlur && shutterAngle > 0 |
  | Enables Frame Blending for all layers with the Frame Blend switch set (为设置了"帧混合"开关的所有图层启用帧混合) | `frameBlending` | true UNVERIFIED | layer frame blending happens iff comp.frameBlending && layer.frameBlendingType != None |
  | Hide all layers for which the Shy switch is set (隐藏为其设置了"消隐"开关的所有图层) | `hideShyLayers` | false | hides shy layers in the Timeline only; never affects rendering (AE-PRJ-361) |
  | Draft 3D (草图 3D) | `draft3d` | false | viewer-only: disables lights, shadows, camera depth of field (and in AE 2022+ adds an extended 3D ground plane/viewer) for interactive speed; never affects Render Queue output |
  | Active camera (活动摄像机) | `activeCamera` (derived) | — | the top-most (lowest index) camera layer that has Video on and whose time span contains the current time; null → default comp camera (cmp-b) |
  - Render Settings can override the comp motion-blur and frame-blending switches ("For Checked Layers" / "Off for All Layers") and they are respected per render (ui-preview-render-io.md [ui-c]).
- **Priority:** P0 (motion blur, frame blending, shy, active camera); Draft 3D P1.
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** Scripting: `CompItem.motionBlur`, `frameBlending`, `hideShyLayers`, `draft3d`, `activeCamera` (read-only). Expressions: `thisComp.activeCamera`. Lottie: no comp on/off switches (root `mb` object carries only `sa`, `sp`, `spf`, `asl`); the per-layer Motion Blur switch is layer `mb` (boolean) in the Lottie schema (Source: https://lottiefiles.github.io/lottie-docs/layers/). Also in this group in AE 2026: the "Enable Live Update", "Graph Editor", "Brainstorm" and "Frame Render Time" toggles, which are UI (ui-preview-render-io.md [ui-a]).

## Creating and resizing compositions

### AE-PRJ-316 · New Composition (新建合成)

- **What it does:** Composition > New Composition (Ctrl+N / Cmd+N) opens Composition Settings and creates an empty comp in the project.
- **Exact semantics:**
  - Dialog fields are pre-filled with the values used the last time Composition Settings was confirmed (any comp), EXCEPT "Preserve frame rate when nested or in render queue" and "Preserve resolution when nested", which always start off. On first launch (fresh preferences) the defaults are those in AE-PRJ-300 (UNVERIFIED: 1920×1080, square, 29.97 fps, 0:00:30:00, black, Full).
  - The name defaults to the next free "Comp N" (AE-PRJ-301).
  - The new comp is created in the currently selected Project panel folder (or the folder containing the selected item; root otherwise), selected, and opened in the Timeline and Composition panels. UNVERIFIED: in AE the new comp goes to the project root unless a folder is selected.
  - Cancel creates nothing. The whole operation is one undo step.
  - The Project panel "Create a new Composition" button does the same with no item selected; with footage selected, or footage dragged onto it, it runs New Comp from Selection (AE-PRJ-317).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `app.project.items.addComp(name, width, height, pixelAspect, duration, frameRate)`. BAE decision: keep the "sticky last settings" behaviour but store it in user preferences, not in the project.

### AE-PRJ-317 · New Comp from Selection (基于所选项新建合成)

- **What it does:** File > New Comp from Selection (Alt+\\ on Windows / Cmd+\\ on macOS, per the AE shortcut reference), or dragging footage onto the New Composition button or into an empty Timeline, creates a comp whose settings match a footage item and places the item(s) in it as layers.
- **Exact semantics:**
  - **One item selected:** no dialog. comp.name = item name (without extension UNVERIFIED); width, height, pixelAspect = the item's; frameRate = the item's frame rate for movies/sequences/comps, and for stills the last-used comp frame rate UNVERIFIED; duration = item duration for movies, sequences and comps, and for stills the Preferences > Import > Still Footage duration when it is set to a fixed duration; with the factory setting "Length of Composition" the last-used comp duration is used UNVERIFIED; one layer is added spanning the comp; the comp opens.
  - **Several items selected:** the New Composition from Selection dialog (基于所选项新建合成) appears:
    | Parameter (中文) | Type | Default | Range | Notes |
    |---|---|---|---|---|
    | Create: Single Composition / Multiple Compositions (单个合成 / 多个合成) | enum | Multiple Compositions UNVERIFIED | — | |
    | Use Dimensions From (使用尺寸来自) | item menu | first selected item | selected items | Single only; supplies width, height, PAR (and frame rate UNVERIFIED) |
    | Still Duration (静止持续时间) | time | Preferences > Import still length UNVERIFIED | ≥ 1 frame | Single: layer length for stills; Multiple: comp duration for each still |
    | Add to Render Queue (添加到渲染队列) | bool | false | — | adds every created comp to the Render Queue |
    | Sequence Layers (序列图层) | bool | false | — | Single only; arranges layers end-to-end (AE-PRJ-406 math) |
    | Overlap (重叠) | bool | false | — | Single + Sequence only |
    | Duration (持续时间) | time | 0:00:01:00 UNVERIFIED | ≥ 0 | overlap length |
    | Transition (过渡) | enum | Dissolve Front Layer UNVERIFIED | Off / Dissolve Front Layer / Cross Dissolve Front and Back Layers | see Sequence Layers |
  - Single Composition: one comp named after the "Use Dimensions From" item UNVERIFIED; layers stacked in selection order with the first selected item at the top (index 1) UNVERIFIED; all layers start at 0 unless Sequence Layers is on; comp duration = max(layer out points) (sequenced: last layer's out point).
  - Multiple Compositions: one comp per selected item, each created as in the one-item case (with Still Duration for stills); none is opened UNVERIFIED.
  - If a selected item is a comp, it is nested as a precomp layer (no copy).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Footage frame-rate source follows Interpret Footage (project-comp-layers.md [prj-a]). Also used by dragging footage to an empty Composition panel. Audio-only items create a comp with the last-used frame size UNVERIFIED.

### AE-PRJ-318 · Changing composition duration and layer clipping (更改合成持续时间)

- **What it does:** Defines what happens to layers, keyframes, work area and markers when a comp's duration is lengthened or shortened.
- **Exact semantics:**
  - Layer startTime, inPoint, outPoint, keyframes and layer markers are never modified by a duration change.
  - Shortening: any part of a layer at or after the new duration is outside the comp; the Timeline clips the bars visually; layers entirely outside are still in the stack and still evaluate (e.g. expressions referencing them work). Comp markers after the end are kept but hidden UNVERIFIED.
  - Lengthening: previously hidden parts reappear. Layers whose source is a still, solid, text, shape, null, camera, light or adjustment layer are NOT extended automatically; their outPoint stays where it was. UNVERIFIED: AE does not auto-extend layers that ended exactly at the old comp end.
  - Work area: clamped into `[0, duration]`; if it ended at the old end, it now ends at the new end UNVERIFIED.
  - CTI: clamped to `duration - frameDuration` if past the end.
  - Precomp layers that use this comp keep their in/out points; if the nested comp became shorter than the precomp layer's span, frames beyond the nested comp's end render transparent (no hold) unless time remapping holds them.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion)
- **Notes:** Lottie: layer `ip`/`op` may extend beyond root `op`; players clip. BAE decision: offer an optional "extend layers that ended at the comp end" checkbox (off by default for AE parity).

### AE-PRJ-319 · Trim Comp to Work Area (将合成修剪到工作区域)

- **What it does:** Composition > Trim Comp to Work Area (Ctrl+Shift+X / Cmd+Shift+X) shortens the comp so it contains only the work area.
- **Exact semantics:**
  ```
  ws = comp.workAreaStart; wd = comp.workAreaDuration
  for each layer: layer.startTime -= ws          // in/out points, keyframes, markers move with it
  for each comp marker: marker.time -= ws
  comp.displayStartTime += ws  // timecode of every remaining frame is preserved UNVERIFIED
  comp.duration = wd
  comp.workAreaStart = 0; comp.workAreaDuration = wd
  comp.time = clamp(comp.time - ws, 0, wd - frameDuration)
  ```
  - Layers are not trimmed; parts outside the new range simply lie outside the comp.
  - Comp markers that end up at negative time or past the end are kept UNVERIFIED (AE may delete them).
  - Not available when the work area already equals the whole comp UNVERIFIED (menu enabled but no-op).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** zh-CN 将合成修剪到工作区域 (from the zh-CN keyboard-shortcut reference; menu string not checked in the AE 2026 UI). Single undo step.

### AE-PRJ-320 · Crop Comp to Region of Interest (将合成裁剪到目标区域)

- **What it does:** Composition > Crop Comp to Region of Interest resizes the comp frame to the current Region of Interest rectangle drawn in the Composition viewer, keeping layers visually in place.
- **Exact semantics:**
  ```
  roi = (x0, y0, w, h) in comp pixels (integers, inside the frame)
  comp.width = w; comp.height = h
  for each unparented layer: Position -= (x0, y0, 0)   // all keyframes too, like AE-PRJ-310 with explicit offset
  ROI is cleared afterwards
  ```
  - Available only when an ROI is set in the active Composition viewer. Duration, frame rate and other settings are unchanged.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** ROI itself is a viewer state (compositing-3d-color.md [cmp-b] "preview resolution/ROI"). zh-CN 将合成裁剪到目标区域（译名待核）.

### AE-PRJ-321 · Crop Comp to Selected Layer(s) Bounds (将合成裁切到所选图层边界)

- **What it does:** Resizes the comp (shrinking or growing) to the union of the visible bounds of the selected 2D layers. Added in AE 26.3 (June 2026) per Adobe "What's new".
- **Exact semantics:**
  ```
  r = union over selected 2D layers of layerBoundsInComp(layer, comp.time)  // transformed, masked visible bounds UNVERIFIED (mask/effect inclusion)
  r = integer rectangle (floor(min), ceil(max))
  comp.width = r.w; comp.height = r.h        // may be larger than before (expand)
  for each unparented layer: Position -= (r.x, r.y, 0)
  ```
  - Disabled when any selected layer is 3D, or when no layer is selected. Uses the bounds at the current time only (animated layers may leave the new frame at other times).
  - Available from the Composition menu and from the layer context menu in the Timeline and Composition panels.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-model, bae-render)
- **Notes:** Layer bounds definition: compositing-3d-color.md [cmp-a] "layer bounds". zh-CN 将合成裁切到所选图层边界 (Source: https://helpx.adobe.com/cn/after-effects/using/composition-basics.html).

### AE-PRJ-322 · Duplicate composition (复制合成)

- **What it does:** Edit > Duplicate (Ctrl+D) with a comp selected in the Project panel creates an independent copy of the comp.
- **Exact semantics:**
  - Copies every comp field (AE-PRJ-300) and every layer with all properties, keyframes, expressions, markers, effects, masks and switches. Layer IDs are new; parent links and track-matte links are remapped to the copies.
  - Nested comps (precomp layers) and footage are NOT duplicated: the copy's layers reference the same items (shallow duplicate).
  - Name: "<name> 2"; if the name already ends in a number, that number is incremented to the next free one UNVERIFIED (e.g. "Main" → "Main 2", "Main 2" → "Main 3").
  - The copy is placed in the same folder, directly after the original UNVERIFIED.
  - Expressions in the copy that reference `thisComp` refer to the copy; those that reference `comp("Original")` still point to the original.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting: `CompItem.duplicate()`. If project-comp-layers.md [prj-a] specifies item duplication generally, this item adds only the comp-specific remapping rules. BAE decision: offer a separate "Deep Duplicate" (duplicating nested comps) as a BAE extension; AE needs scripts for this.

## Work area

### AE-PRJ-323 · Work area (工作区域)

- **What it does:** A sub-range of the comp timeline that limits previews, caching (by preference) and, by default, rendering; it is the bracket bar under the Timeline time ruler.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | workAreaStart (工作区域开头) | time | 0 | 0 .. duration - 1 frame | s | snapped to frames |
  | workAreaDuration (工作区域持续时间) | time | comp duration | 1 frame .. duration - start | s | end = start + duration (exclusive) |
  - Set work area start to CTI: **B**; set end: **N**. `B: start = cti` (if `cti >= end`, end is moved to `comp.duration` UNVERIFIED); `N: end = cti + frameDuration` so the frame under the CTI is included (if `end <= start`, start is moved to 0 UNVERIFIED).
  - Ctrl+Alt+B / Cmd+Opt+B: work area = union span of the selected layers' [inPoint, outPoint) clipped to the comp, or the whole comp when no layer is selected.
  - Dragging the bar's ends sets start/end (Shift snaps); dragging the middle moves it keeping its duration; double-clicking the bar resets it to the whole comp.
  - Shift+Home / Shift+End: move CTI to work-area start / last frame of the work area.
  - Uses: preview range (unless the Preview panel range is set otherwise, ui-preview-render-io.md [ui-b]); Render Settings Time Span default "Work Area Only" for comps whose work area is not the full comp, otherwise "Length of Comp" UNVERIFIED (ui-preview-render-io.md [ui-c]); Trim Comp to Work Area; Lift/Extract Work Area; New Comp from work area (Precompose does not use it).
  - The work area may never be empty; on comp duration change it is clamped (AE-PRJ-318).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem.workAreaStart`, `workAreaDuration`. Lottie: bodymovin exports root `ip`/`op` from the work area (in frames) UNVERIFIED; importers should set the work area to [ip, op). Shortcuts: ui-preview-render-io.md [ui-b].

### AE-PRJ-324 · Lift Work Area and Extract Work Area (提升工作区域、提取工作区域)

- **What it does:** Edit > Lift Work Area removes the part of layers inside the work area and leaves a gap; Edit > Extract Work Area removes it and closes the gap (ripple delete).
- **Exact semantics:**
  ```
  targets = selected layers, or all unlocked layers if none selected
  [a, b) = work area
  for each layer L in targets with span [L.in, L.out) overlapping [a, b):
      if L.in >= a and L.out <= b: delete L                         // fully inside
      elif L.in < a and L.out > b:                                   // spans the work area
          R = split L at b (AE-PRJ-383 Split rules: duplicate above, named like Split Layer)
          L.out = a; R.in = b
      elif L.in < a: L.out = a                                       // overlaps the start
      else: L.in = b                                                 // overlaps the end
  Extract only, afterwards:
      for each affected piece with in >= b (and every target layer lying fully after b UNVERIFIED):
          piece.startTime -= (b - a)        // keyframes and markers move with the layer
  comp duration and work area are unchanged
  ```
  - Keyframes are not deleted; each piece keeps the full keyframe set and only its in/out points change (same as Split Layer) UNVERIFIED.
  - Locked layers are never changed. Comp markers are not moved by Extract UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** One undo step. Help (Selecting and arranging layers) states the "selected or all unlocked layers" rule and the gap vs ripple behaviour; the split details are UNVERIFIED.

## Markers

### AE-PRJ-325 · Marker data model (标记)

- **What it does:** A marker is a time-stamped record carrying comment, duration, chapter, web link, cue point and label data; composition markers and layer markers use the same record type.
- **Exact semantics:** Marker dialog fields in AE order (dialog title "Composition Marker" (合成标记) / "Layer Marker" (图层标记)):
  | Field (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Time (时间) | time | CTI when added | any comp time | s | comp markers: comp time; layer markers: stored relative to the layer (see AE-PRJ-327) |
  | Duration (持续时间) | time | 0 | ≥ 0 | s | 0 = point marker; >0 = span marker with out point |
  | Label (标签) | int | 0 (None) | 0..16 | — | AE 16.0 (2019); colours from Preferences > Labels (AE-PRJ-392) |
  | Comment (注释) | string | "" (numbered comp markers created with Shift+0..9 get "0".."9", AE-PRJ-326) | — | — | shown next to the marker |
  | Chapter (章节) | string | "" | — | — | written as chapter points to formats that support them |
  | URL (URL) | string | "" | — | — | Web Links group (Web 链接) |
  | Frame Target (帧目标) | string | "" | — | — | HTML frame/window target for the URL |
  | Cue Point Name (提示点名称)（译名待核） | string | "" | — | — | Flash/F4V cue point; group "Flash Cue Point" (Flash 提示点)（译名待核） |
  | Event / Navigation (事件 / 导航)（译名待核） | enum | Event | Event, Navigation | — | scripting `eventCuePoint` true = Event |
  | Parameter Name / Parameter Value (参数名称 / 参数值)（译名待核） | ordered list of string pairs | [] | — | — | Add/Remove buttons; scripting `getParameters()`/`setParameters(obj)` |
  | Protected Region (受保护区域)（译名待核） | bool | false | — | — | comp markers only; AE 16.0; AE-PRJ-330 |
  | Marker type (标记类型)（译名待核）: Comment (注释) / Segmentation (分段) | enum | Comment | Comment, Segmentation | — | layer marker dialog radio choice; AE 22.3 (April 2022, with Scene Edit Detection, AE-PRJ-422); each type has its own default label colour in Preferences > Labels UNVERIFIED (which indices); scripting name UNVERIFIED |
  - Markers are values of a special property `ADBE Marker` (type MARKER) on each layer, and `CompItem.markerProperty` for comps (AE-PRJ-350 property types). Keyframe-like: each marker is a key with a time; they cannot have interpolation.
  - A marker with link or cue-point data shows a small dot in its icon (UI).
  - Markers do not affect rendering, except Protected Region (time behaviour of nested comps) and output metadata (chapters/URLs/cue points/XMP are written by the output module, ui-preview-render-io.md [ui-c]).
- **Priority:** P0 for time/duration/comment/label; P2 for chapter, URL, frame target, cue points.
- **Owner:** bae-model
- **Notes:** Scripting: `MarkerValue(comment, chapter, url, frameTarget, cuePointName, params)`, attributes `comment, chapter, url, frameTarget, cuePointName, eventCuePoint, duration, label, protectedRegion`. Expressions: `marker.key(i).time/duration/comment/chapter/url/frameTarget/eventCuePoint/cuePointName/parameters/protectedRegion` (transform-keyframes-expressions.md [ani-b]). Lottie: root `markers: [{tm, cm, dr}]` (frames); bodymovin uses comp markers only; layer markers are not exported UNVERIFIED. Custom label colours cannot be set per marker (only the 16 indices).

### AE-PRJ-326 · Composition markers (合成标记)

- **What it does:** Markers on the comp's time ruler, used for navigation, sync points, numbered cue positions and output chapters.
- **Exact semantics:**
  - Add at CTI with no layer selected: Layer > Add Marker or numpad `*`; numpad Alt/Opt+`*` adds and opens the dialog; drag from the Comp Marker Bin (合成标记素材箱) button at the right end of the time ruler to any time. Pressing `*` during a preview adds markers without stopping playback.
  - Layer > Add Marker, numpad `*` and the bin create a blank comp marker (comment "").
  - Numbered markers: Shift+0..9 (main keyboard) creates a marker with comment "0".."9" at CTI; if a comp marker with that number already exists, AE does not create a new one but MOVES the existing one to the CTI. Pressing 0..9 (main keyboard) moves the CTI to that numbered marker. Deleting a numbered marker does not renumber the others; changing a marker's comment away from its number frees that number for reuse.
  - Delete: drag the marker onto the Comp Marker Bin, or Ctrl/Cmd-click it.
  - Uniqueness: at most one comp marker can start at a given time; adding or moving a marker onto another marker's start time replaces (deletes) the other.
  - Any number of comp markers per comp; marker times outside [0, duration) are allowed but hidden UNVERIFIED.
  - Expressions read them via `thisComp.marker`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/layer-markers-composition-markers.html (blank vs numbered markers, one marker per start time, Shift+number moves an existing numbered marker). Comp markers correspond to Premiere Pro sequence markers. Exact keyboard bindings: ui-preview-render-io.md [ui-b].

### AE-PRJ-327 · Layer markers (图层标记)

- **What it does:** Markers attached to a layer's duration bar; they move with the layer and are visible to expressions as `marker` on that layer.
- **Exact semantics:**
  - Add: with layers selected, Layer > Add Marker or numpad `*` adds one marker at CTI to each selected layer; Alt/Opt+numpad `*` adds and opens the dialog. Default comment "".
  - Time base: markers are stored in layer time (relative to the layer's startTime) so moving the layer in time moves its markers. Displayed/scripted time is comp time: `markerCompTime = layer.startTime + markerLayerTime * stretch/100` UNVERIFIED (time stretch scales marker positions like keyframes; Time-Reverse mirrors them UNVERIFIED).
  - Markers may lie outside the layer's in/out span (e.g. after trimming); they are kept.
  - Any number of markers per layer; several layer markers may share a time UNVERIFIED (the replace rule documented for comp markers is not documented for layer markers).
  - Split Layer / Duplicate copy all markers to the new layer (AE-PRJ-383, AE-PRJ-384); Precompose "Move all attributes" moves them into the new comp's layer (they stay layer markers there).
  - Footage layers: if the Preferences "Create Layer Markers From Footage XMP Metadata" option (Disk / Media & Disk Cache category depending on version) is on (default on), markers are created from the source file's temporal XMP metadata when the layer is created.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Matchname `ADBE Marker` at layer root (shown as "Marker" (标记) in the Timeline only when markers exist UNVERIFIED). Layer markers correspond to Premiere Pro clip markers; they are retained when rendering to QuickTime.

### AE-PRJ-328 · Marker editing operations (编辑标记)

- **What it does:** Moving, retiming, deleting and locking comp and layer markers.
- **Exact semantics:**
  | Operation | Gesture | Result |
  |---|---|---|
  | Open settings | double-click marker, or context menu > Settings (设置) | marker dialog (AE-PRJ-325); Time field moves the marker |
  | Move | drag the marker (in point); Shift snaps | time changes, duration unchanged |
  | Set duration | drag the marker's out-point handle; or Alt/Opt-drag the marker icon to the right on a 0-duration marker | duration changes; time unchanged. Time and duration cannot change in one drag |
  | Delete one | Ctrl/Cmd-click the marker; or drag a comp marker onto the Comp Marker Bin; context menu > Delete This Marker (删除此标记) | removed |
  | Delete all | context menu > Delete All Markers (删除所有标记) | all markers of that layer (or the comp) removed |
  | Lock | context menu > Lock Markers (锁定标记) | all markers of that layer / comp become non-editable by drag UNVERIFIED version (AE 2022+?) |
  | Update Markers From Source (从源更新标记) | layer marker context menu | AE-PRJ-329 |
  - Moving the CTI with Shift-drag, and dragging layers/keyframes with Shift, snaps to markers (and marker out points) of both kinds.
  - J/K navigation stops at markers (ui-preview-render-io.md [ui-b]).
- **Priority:** P0 (move, duration, delete); P1 (lock, delete all).
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** All marker edits are undoable; Lock is persisted per marker list UNVERIFIED (scripting has no attribute).

### AE-PRJ-329 · Markers on precomp layers and Update Markers From Source (预合成图层上的标记、从源更新标记)

- **What it does:** When a comp is nested, its comp markers are copied as layer markers onto the new precomp layer; "Update Markers From Source" re-synchronises them later.
- **Exact semantics:**
  - On creating a precomp layer (drag comp into comp, Precompose, New Comp from Selection with a comp), every comp marker of the source comp becomes a layer marker on that layer at the same source time (mapped through layer startTime/stretch). They are independent copies: editing or deleting the source comp's markers does not change them.
  - Layer marker context menu > Update Markers From Source: replaces ALL markers of the layer (including user-added ones) with fresh copies of the source comp's comp markers; for footage layers it restores markers from the source file's temporal XMP metadata.
  - Protected-region markers of the source comp appear on the precomp layer and keep `protectedRegion = true` (scripting doc).
  - UNVERIFIED: since AE 16.0 (2019) the copies are created on nesting; older versions did not copy markers.
- **Priority:** P1
- **Owner:** bae-model (+ bae-motion)
- **Notes:** BAE decision: optionally display the source comp's live markers as read-only ghosts on precomp layers in addition to the copied ones; keep AE copy semantics for data and AEP round-trip.

### AE-PRJ-330 · Protected Region and Responsive Design – Time (受保护区域、响应式设计 - 时间)

- **What it does:** A comp marker with Protected Region set marks a time span of a comp that must not be time-stretched when the comp is nested and its precomp layer is stretched or its duration is changed (responsive design).
- **Exact semantics:**
  - Create: tick Protected Region in the comp marker dialog (marker should have duration > 0), or Composition > Responsive Design – Time > Create Intro (创建入点)（译名待核） / Create Outro / Create Protected Region From Work Area; "Create Intro" makes a protected region from comp start to the CTI, "Create Outro" from CTI to comp end UNVERIFIED.
  - Effect when nested: if the precomp layer (or the nested comp duration via Comp Settings) is time-stretched, frames inside protected regions play at 100 % speed; only unprotected spans are stretched to absorb the difference. Implemented by AE as an automatic time-remap mapping UNVERIFIED.
  - A comp with protected regions shows them as shaded spans on the comp marker in the parent's precomp layer UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-model, bae-motion)
- **Notes:** AE 16.0 (2019). Scripting: `MarkerValue.protectedRegion`. Rarely used outside Motion Graphics templates; time-mapping math in transform-keyframes-expressions.md [ani-a] "time remap" if specified there, otherwise to be defined by bae-anim.

## Layer types

### AE-PRJ-331 · Layer type taxonomy (图层类型)

- **What it does:** Defines the closed set of layer kinds a comp can contain, how each maps to AE's scripting classes and to Lottie `ty`, and the defaults shared by all new layers.
- **Exact semantics:**
  | BAE kind | AE scripting class / flag | Source | Lottie `ty` (bodymovin layerTypes) | Default label (Preferences > Labels) | Item |
  |---|---|---|---|---|---|
  | Footage (still) | AVLayer, source FootageItem (still) | image file | 2 image (`still`) | Still: Lavender UNVERIFIED | AE-PRJ-332 |
  | Footage (movie / image sequence) | AVLayer, source FootageItem | movie / sequence | 9 `video` / 8 `imageSeq` (bodymovin internal; not in Lottie spec, players ignore) | Video: Aqua UNVERIFIED | AE-PRJ-332 |
  | Audio | AVLayer, hasVideo false | audio file | 6 audio | Audio: Sea Foam UNVERIFIED | AE-PRJ-333 |
  | Solid | AVLayer, source SolidSource | solid item | 1 solid | Solid: Red UNVERIFIED | AE-PRJ-334 |
  | Text | TextLayer | none | 5 text | Text: Red UNVERIFIED | AE-PRJ-335 |
  | Shape | ShapeLayer | none | 4 shape | Shape: Blue UNVERIFIED | AE-PRJ-336 |
  | Null | AVLayer `nullLayer` true | 100×100 null solid | 3 null | Null Object: Red UNVERIFIED | AE-PRJ-337 |
  | Camera | CameraLayer | none | 13 camera | Camera: Pink UNVERIFIED | AE-PRJ-338 |
  | Light | LightLayer | none | 14 light (bodymovin internal; not exported) | Light: Pink UNVERIFIED | AE-PRJ-339 |
  | Adjustment | AVLayer `adjustmentLayer` true | comp-size white solid | 12 adjustment (bodymovin internal; not exported) | Adjustment: Purple UNVERIFIED | AE-PRJ-340 |
  | Precomp | AVLayer, source CompItem | comp | 0 precomp | the comp item's label (Composition: Sandstone) | AE-PRJ-341 |
  | Guide (switch on any layer) | `guideLayer` true | any | 11 guide (not exported) | unchanged | AE-PRJ-342 |
  | Photoshop / CINEMA 4D / 3D model / data | AVLayer / ThreeDModelLayer | file | — / — / — / 15 data | 3D Models / Other 3D: UNVERIFIED | AE-PRJ-343..346 |
  - Common defaults for every new layer: index 1 (top) or directly above the topmost selected layer (AE-PRJ-388 new-layer insertion); `startTime = 0` and `inPoint = 0` when Preferences > General "Create Layers at Composition Start Time" (在合成开始时间创建图层) is on (default on), otherwise both = CTI; Video on, Audio on (if hasAudio), Solo/Lock/Shy off, Quality Best, Sampling Bilinear, blend mode Normal, no track matte, no parent, 2D, Motion Blur and Frame Blending off, Effects active, comment empty, `isNameSet` false.
  - Synthetic layers (solid, text, shape, null, adjustment, camera, light) default to an out point at the comp end; footage layers to the source duration (stills: Preferences > Import > Still Footage, default "Length of Composition").
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** ARCHITECTURE.md 4.2 kinds: `Shape | Text | Solid | Footage | Null | Camera | Light | Adjustment | PreComp | Audio`; guide and adjustment are flags in AE (a solid, footage or precomp layer can be adjustment), so BAE models `adjustmentLayer`/`guideLayer` as bool attributes on visual layers and keeps "Adjustment" only as a creation preset. Lottie `ty` values per lottie-docs layers page: 0, 1, 2, 3, 4, 5, 6, 13, 15; others are bodymovin internals. Lottie `hd` = hidden (exporter writes it for guide/disabled layers UNVERIFIED).

### AE-PRJ-332 · Footage layer: still, movie, image sequence (素材图层)

- **What it does:** A layer whose source is an imported footage item; it shows the item's frames over time according to Interpret Footage.
- **Exact semantics:**
  - Size: `width × height` of the source (after Interpret Footage PAR; layer pixels are source pixels). Default Anchor Point `(w/2, h/2)`; Position `(compW/2, compH/2)` (or the drop point when dragged into the Composition panel; Shift snaps to centre/edges); Scale 100 %; Rotation 0; Opacity 100 %.
  - Name: source name (layer name empty → shows source name; AE-PRJ-388).
  - Duration: movie/sequence = source duration (layer may be trimmed but not extended past the source unless time-remapped; AE-PRJ-379 timing); still = Still Footage preference (default length of comp), and stills can be extended indefinitely.
  - Property groups: Marker, Time Remap (movies, sequences; not stills UNVERIFIED), Motion Trackers, Masks, Effects, Essential Properties (no), Transform, Layer Styles, Geometry/Material Options (when 3D), Audio (when the source has audio).
  - Movie frames are sampled per AE-PRJ-373 layer-time formula; the source frame index is `floor(sourceTime * sourceFps + ε)`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-render)
- **Notes:** Lottie: still images are `ty: 2` with `refId` to an image asset (`assets[].p`, `u`, `w`, `h`); movies/sequences are not part of the Lottie spec. Decoding and interpretation: project-comp-layers.md [prj-a] and bae-media.

### AE-PRJ-333 · Audio-only layer (音频图层)

- **What it does:** A layer whose source has audio but no video; it contributes only to the comp's audio mix.
- **Exact semantics:**
  - `hasVideo = false`; the Video switch column is empty; it never renders pixels and cannot be a track matte. It has no Transform group, so it is useless as a parent; UNVERIFIED: whether AE lists it in the Parent menu at all (BAE: exclude it).
  - Property groups: Marker, Time Remap, Effects (audio effects only), Audio > Audio Levels (音频电平) (TwoD, dB, default [0, 0], range -192..+24 dB UNVERIFIED). No Transform, Masks or Layer Styles.
  - Timeline shows waveform under Audio > Waveform (UI).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** Lottie `ty: 6` with `au` audio settings (`lv` level) and `refId` to a sound asset. Audio preview/mixing: ui-preview-render-io.md [ui-b]; Audio effects: effects.md [fx-c].

### AE-PRJ-334 · Solid layer (纯色图层)

- **What it does:** A layer whose source is a solid-colour footage item, used for backgrounds, effect hosts and masks.
- **Exact semantics:** Layer > New > Solid (Ctrl+Y / Cmd+Y) opens Solid Settings (纯色设置):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Name (名称) | string | "<Colour name> Solid N" e.g. "Black Solid 1" (黑色 纯色 1) UNVERIFIED zh format | — | — | colour name derived from the colour |
  | Width / Height (宽度 / 高度) | int | comp size | 1..30000 | px (entry also in the chosen Units) | Lock Aspect Ratio checkbox |
  | Units (单位) | enum | pixels | pixels, inches, millimeters, % of composition | — | dialog-only |
  | Pixel Aspect Ratio (像素长宽比) | enum | comp PAR | AE-PRJ-304 list | — | |
  | Make Comp Size (制作合成大小) | button | — | — | — | sets width/height/PAR to the comp's |
  | Color (颜色) | RGB | last used; UNVERIFIED: fresh-install value | 0..1 | — | |
  - Creates a solid item in the project's Solids (纯色) folder and a layer using it. Layer size = solid size; Anchor `(w/2, h/2)`; Position comp centre.
  - Layer > Solid Settings (Ctrl+Shift+Y) on a solid layer edits the source; "Affect all layers that use this solid" (影响使用此纯色的所有图层) checked → modify the item in place; unchecked → create a new solid item for this layer only.
  - Property groups: Marker, Masks, Effects, Transform, Layer Styles, Geometry/Material Options (3D). No Time Remap or Audio.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion)
- **Notes:** Solid footage items and the Solids folder (incl. "Enhanced solids folder organization") are owned by project-comp-layers.md [prj-a]. Lottie `ty: 1` with `sc` (colour hex string), `sw`, `sh`.

### AE-PRJ-335 · Text layer (文本图层)

- **What it does:** A sourceless vector layer that renders a TextDocument.
- **Exact semantics:**
  - Create: Layer > New > Text (Ctrl+Alt+Shift+T) or click with the Type tool (point text) / drag (paragraph text). Via menu the insertion point is the centre of the Composition viewer.
  - Defaults: Anchor Point `(0, 0)` (the text baseline origin); Position = click point, or comp centre via the menu; layer name = the text typed (first line UNVERIFIED), updating live until the layer is renamed (`isNameSet` false).
  - Property groups: Text (ADBE Text Properties: Source Text, Path Options, More Options, Animators), Masks, Effects, Transform, Layer Styles, Geometry Options (Advanced 3D / CINEMA 4D), Material Options (3D).
  - Always rasterized from vectors at render resolution. The Collapse switch is shown as Continuously Rasterize (effect on render order UNVERIFIED, AE-PRJ-362; cmp-a "collapse/continuous rasterize"). `width`/`height` are not meaningful; bounds come from `sourceRectAtTime`.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Everything about text content and layout: text.md. Lottie `ty: 5`, data in `t` (`t.d` document keyframes, `t.p` path, `t.m` more options, `t.a` animators).

### AE-PRJ-336 · Shape layer (形状图层)

- **What it does:** A sourceless vector layer whose content is a tree of shape groups, paths, fills, strokes and operators.
- **Exact semantics:**
  - Create: Layer > New > Shape Layer (empty "Shape Layer N" (形状图层 N)), or draw with a shape/pen tool with no shape layer selected.
  - Defaults: Anchor Point `(0, 0)`; Position `(compW/2, compH/2)`; drawn shapes are placed so they appear where drawn (path coordinates relative to the layer origin). AE 2020 (17.0)+ "Shape Layer" names, label Blue UNVERIFIED.
  - Property groups: Contents (ADBE Root Vectors Group), Masks, Effects, Transform, Layer Styles, Geometry Options (Advanced 3D / CINEMA 4D), Material Options (3D).
  - Always rasterized from vectors at render resolution. The Collapse switch is shown as Continuously Rasterize (effect on render order UNVERIFIED, AE-PRJ-362).
- **Priority:** P0
- **Owner:** bae-vector (+ bae-model)
- **Notes:** Content model: shapes.md. Lottie `ty: 4`, `shapes[]`.

### AE-PRJ-337 · Null object (空对象)

- **What it does:** An invisible layer used as a parent, controller or expression host; it has transforms but never renders.
- **Exact semantics:**
  - Create: Layer > New > Null Object (Ctrl+Alt+Shift+Y). Name "Null N" (空 N); AE creates a 100×100 px null source in the Solids folder UNVERIFIED (shown as "Null N" item).
  - Defaults: size 100×100; Anchor Point `(0, 0)` (the top-left corner of the 100×100 box; not in Adobe docs, but confirmed by Adobe Community answers and the open feature request "Default null object's anchor point must be 50,50", https://community.adobe.com/t5/after-effects-ideas/default-null-object-s-anchor-point-must-be-50-50/idi-p/13401323); Position `(compW/2, compH/2)` (community answers), so the box's top-left corner sits at the comp centre; label Red UNVERIFIED.
  - Rendering: never rendered in the output or as a track matte (contributes no pixels; a null used as a matte acts as fully transparent UNVERIFIED); the Composition viewer draws its bounds as a 100×100 outline with handles.
  - Opacity exists but affects nothing (not inherited by children, AE-PRJ-396 parenting). Effects can be applied (typically Expression Controls); masks UNVERIFIED.
  - Can be 3D (no Material Options UNVERIFIED). Video switch: toggling off still lets children follow it (transform evaluation does not depend on Video, AE-PRJ-396).
  - Converting: any AV layer can be made null via scripting `nullLayer` is read-only; UI has no switch.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `Layer.nullLayer` (read-only), `layers.addNull(duration)`. Lottie `ty: 3` (players skip drawing). BAE decision: nulls need no source item; store as `Layer { kind: Null }` with fixed 100×100 bounds and create no project item (import of AEP solids-folder nulls maps them).

### AE-PRJ-338 · Camera layer (摄像机图层)

- **What it does:** A 3D-only layer that defines the viewpoint, lens and depth of field for the comp's 3D layers.
- **Exact semantics:**
  - Create: Layer > New > Camera (Ctrl+Alt+Shift+C) → Camera Settings dialog (type One-Node / Two-Node, preset 50 mm default, zoom, DOF...; defined in compositing-3d-color.md [cmp-b]). Name "Camera N" (摄像机 N).
  - Transform group (no Anchor Point, Scale, Opacity): Point of Interest (目标点; Two-Node only; matchName `ADBE Anchor Point`, default comp centre `(w/2, h/2, 0)`), Position (default `(w/2, h/2, -zoom)` UNVERIFIED, see cmp-b), Orientation, X/Y/Z Rotation.
  - Property groups: Marker, Transform, Camera Options (ADBE Camera Options Group). No masks, effects, styles, audio, time remap.
  - Always 3D (no 3D switch). It has a Video switch; turning it off makes the camera inactive (`activeCamera` rule, AE-PRJ-315).
  - Auto-Orient options: Off, Orient Along Path, Orient Towards Point of Interest (AE-PRJ-408).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting: `CameraLayer`, `layers.addCamera(name, centerPoint)`. Lottie `ty: 13` with `pe` (perspective) and `ks` (`a` as point of interest). Default label Pink UNVERIFIED.

### AE-PRJ-339 · Light layer (灯光图层)

- **What it does:** A 3D-only layer that lights 3D layers (and casts shadows) in the comp.
- **Exact semantics:**
  - Create: Layer > New > Light (Ctrl+Alt+Shift+L) → Light Settings dialog. Light Type menu (AE order): Parallel (平行), Spot (聚光), Point (点), Ambient (环境), Environment (环境光（译名待核）; Advanced 3D only, AE-PRJ-347; `LightType.ENVIRONMENT` added to scripting in AE 24.3). Scripting `LightType.PARALLEL / SPOT / POINT / AMBIENT / ENVIRONMENT`.
  - Name "<Type> Light N" (e.g. "Spot Light 1") UNVERIFIED (older versions "Light N").
  - Transform: Point of Interest (Parallel, Spot), Position (all but Ambient), Orientation + X/Y/Z Rotation (all but Ambient; Point lights ignore rotation). Ambient lights have no Transform group UNVERIFIED.
  - Property groups: Marker, Transform, Light Options (ADBE Light Options Group): Intensity, Color, Cone Angle, Cone Feather, Falloff, Radius, Falloff Distance, Casts Shadows, Shadow Darkness, Shadow Diffusion (per type; cmp-b).
  - Auto-Orient options: Off, Orient Along Path, Orient Towards Point of Interest.
  - Light Settings also offers "Light Source" (layer) for Environment type (`LightLayer.lightSource`).
- **Priority:** P0 data (render semantics in cmp-b are P1).
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `LightLayer.lightType`, `layers.addLight(name, centerPoint)`. Lottie: not in spec (bodymovin `ty: 14` not exported).

### AE-PRJ-340 · Adjustment layer (调整图层)

- **What it does:** A layer whose effects (and masks, opacity) apply to the composite of all layers below it instead of to its own pixels.
- **Exact semantics:**
  - Create: Layer > New > Adjustment Layer (Ctrl+Alt+Y / Cmd+Opt+Y): a comp-sized white solid named "Adjustment Layer N" (调整图层 N) in the Solids folder, with the Adjustment Layer switch on, label Purple UNVERIFIED, Anchor `(w/2, h/2)`, Position comp centre.
  - The Adjustment Layer switch (AE-PRJ-368) can be set on any AV layer (solid, footage, precomp, shape, text) to turn it into an adjustment layer; its own colour pixels are then ignored and only its alpha (masks, opacity, source alpha) acts as the effect's coverage.
  - Property groups: same as solid.
  - Rendering semantics: compositing-3d-color.md [cmp-a] "adjustment layers".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.adjustmentLayer`. Lottie: not supported by the spec (bodymovin `ty: 12` internal).

### AE-PRJ-341 · Precomposition layer (预合成图层)

- **What it does:** A layer whose source is another comp, rendering that comp's output (nesting).
- **Exact semantics:**
  - Created by dragging a comp item into a comp, Precompose (AE-PRJ-401), or New Comp from Selection.
  - Size = nested comp `width × height`; Anchor `(w/2, h/2)`; Position comp centre; duration = nested comp duration (can be trimmed; extending past the nested comp's end shows nothing unless time-remapped).
  - Time mapping: `nestedTime = (compTime - layer.startTime) * 100/stretch` (plus time remap if enabled), sampled per the nested comp's frame-rate rules (AE-PRJ-311).
  - Property groups: Marker (copies of source comp markers, AE-PRJ-329), Time Remap, Masks, Effects, Essential Properties (AE-PRJ-420, when the source comp has them), Transform, Layer Styles, Geometry/Material Options (3D), Audio (if the nested comp has audio).
  - Collapse Transformations switch available (AE-PRJ-362).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Lottie `ty: 0`, `refId` → `assets[].layers`, `w`, `h`, `tm` (time remap in seconds). Nesting rules (cycles) in AE-PRJ-403.

### AE-PRJ-342 · Guide layer (引导层)

- **What it does:** Any layer can be flagged as a guide layer: it is visible in its own comp's viewer but excluded from rendered output and from parent comps.
- **Exact semantics:**
  - Toggle: Layer > Guide Layer (引导层), or the layer context menu. Flag `guideLayer` (bool, default false).
  - A guide layer renders in the Composition viewer of its own comp. It is skipped when (a) rendering through the Render Queue, unless Render Settings > Guide Layers is "Current Settings" (当前设置), which renders guide layers of the top-level comp only (default "All Off" (全部关闭)（译名待核）), and (b) the comp is nested in another comp (guide layers in nested comps are never rendered).
  - It still evaluates (expressions may reference it; it can still be a parent).
  - Used as a track matte: UNVERIFIED (matte still works in its own comp viewer, not in output).
  - Timeline shows a guide-layer icon next to the layer name.
- **Priority:** P1
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.guideLayer`. Lottie: bodymovin skips guide layers (layerType 11). zh-CN: the zh-CN help (layers page) uses 引导层/引导图层; menu string in AE 2026 not checked. Source (Render Settings rule): https://helpx.adobe.com/after-effects/using/basics-rendering-exporting.html.

### AE-PRJ-343 · Photoshop file layers (Photoshop 图层)

- **What it does:** PSD files import as merged footage or as a composition whose layers each reference one Photoshop layer; Layer > New > Adobe Photoshop File creates a new PSD.
- **Exact semantics:**
  - Import as Composition / Composition – Retain Layer Sizes: each PS layer becomes a footage layer whose source item is "<layer name>/<file>.psd"; PS layer opacity, blend mode, visibility, layer masks, vector masks, layer styles (as AE Layer Styles), text (editable via Layer > Create > Convert to Editable Text) and adjustment layers (as AE effects where supported) are converted. Import options are prj-a.
  - Layer > New > Adobe Photoshop File creates a comp-sized blank PSD, imports it and adds it as the top layer; the source item re-reads the file when it changes on disk.
- **Priority:** P2
- **Owner:** bae-media (+ bae-model)
- **Notes:** BAE decision: PSD import is a P2 importer; layer-style conversion relies on AE-PRJ-409 Layer Styles.

### AE-PRJ-344 · CINEMA 4D file layers (CINEMA 4D 图层)

- **What it does:** A .c4d file imported as footage renders through the Cineware effect (applied automatically) using a Cinema 4D engine.
- **Exact semantics:** Layer > New > MAXON CINEMA 4D File creates a new .c4d; importing places a footage layer with the Cineware effect; render settings (Standard/Software/Current, camera selection, multi-pass) live in the effect.
- **Priority:** P2
- **Owner:** bae-plugin (+ bae-media)
- **Notes:** Requires Maxon's renderer; BAE will not ship it. BAE decision: import as an unsupported-footage placeholder preserving the reference.

### AE-PRJ-345 · 3D model layer (3D 模型图层)

- **What it does:** A layer whose source is an imported 3D model (GLB, glTF, OBJ, FBX; FBX with embedded animation was added after the initial release, UNVERIFIED: in which version), rendered by the Advanced 3D renderer.
- **Exact semantics:**
  - Introduced AE 24.0 (October 2023); scripting `ThreeDModelLayer` (subclass of AVLayer) AE 24.4.
  - Always 3D. Property groups: Marker, Time Remap (for animated models), Transform (Anchor Point, Position, Scale, Orientation, X/Y/Z Rotation, Opacity), Options for the model (animation selection, mesh/material settings UNVERIFIED), Material Options.
  - In Classic 3D comps a 3D model layer renders as a bounding box/nothing UNVERIFIED; the help says Advanced 3D is the only native renderer that can render models.
  - Default anchor: model's pivot/origin; position comp centre; scale chosen so the model fits the view (AE 25.x "Decluttered 3D Model Import" changes import behaviour UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-render (+ bae-media, bae-model)
- **Notes:** Default label "3D Models" (Preferences > Labels). Lottie: unsupported.

### AE-PRJ-346 · Data layer: JSON, mgJSON, CSV, TSV (数据图层)

- **What it does:** Data files import as footage; adding a data item to a comp creates a layer exposing its data (mgJSON/CSV/TSV as data streams; plain JSON only through expressions).
- **Exact semantics:**
  - Supported inputs: JSON, MGJSON, CSV, TSV, BVH (per help). Plain `.json` footage is not shown as streams; expressions read it via `footage("x.json").sourceData`. mgJSON/CSV/TSV layers show a Data group with dynamic and static streams (keyframed data, one stream per column/property) that can be linked with expressions or converted to keyframes.
  - Data layers do not render.
- **Priority:** P2
- **Owner:** bae-model (+ bae-media, bae-anim)
- **Notes:** Data-driven animation added in AE 15.1 (2018) UNVERIFIED. Lottie `ty: 15` data layer (bodymovin `data`). Expression access: transform-keyframes-expressions.md [ani-b] "data footage".

### AE-PRJ-347 · Environment light (环境光)

- **What it does:** A light type that uses an image (typically an HDRI equirectangular footage layer) to light 3D layers with image-based lighting, reflections and backgrounds in Advanced 3D comps.
- **Exact semantics:**
  - Light Type "Environment" (`LightType.ENVIRONMENT`, scripting AE 24.3; the feature shipped with Advanced 3D, UNVERIFIED: in AE 24.0). Light Options: Intensity, Color, Source (`LightLayer.lightSource`: until AE 25.1 only HDR/EXR layers; AE 25.2+ any 2D video, still or precomp layer in the same comp; a 3D layer is rejected with "Invalid light source specified"), Casts Shadows + Shadow Darkness.
  - With no Source, AE uses a built-in default environment map. Choosing a layer as Source turns that layer's Video switch off (user may re-enable it).
  - The default environment light casts no shadows; with Casts Shadows on, 3D layers inside the shadow volume cast voxel shadows (subject to their Casts/Accepts Shadows options).
  - There is no background-visibility option on the light: Layer > Light > Create Environment Light Background Layer adds a solid at the bottom of the stack with CC Environment linked to the light's Source and a 5 px Fast Box Blur.
  - Transform: Orientation/rotations only (rotate the environment).
  - Legacy: `AVLayer.environmentLayer` flags a layer as the environment in Ray-traced 3D comps (deprecated renderer).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Rendering: compositing-3d-color.md [cmp-b] Advanced 3D. Sources: Adobe help "Create realistic reflections and ambient lighting in a 3D scene" (environment lights), ae-scripting `LightLayer.lightSource`.

## Layer property tree

### AE-PRJ-348 · Property tree structure (属性树)

- **What it does:** Every animatable or settable thing on a layer is a node in a tree: leaf Properties under PropertyGroups, rooted at the layer. Expressions, scripts, AEP files and the Timeline all address properties through this tree.
- **Exact semantics:**
  - Node kinds (`PropertyType`): `PROPERTY` (leaf with a value), `NAMED_GROUP` (fixed children, e.g. Transform; children addressed by name/matchName), `INDEXED_GROUP` (user-orderable list of same-kind children, e.g. Masks, Effects, shape Contents, text Animators; children addressed by 1-based index; users can add, delete, duplicate, rename and reorder them).
  - Every node has: `matchName` (stable, language-independent ID, e.g. `ADBE Opacity`), `name` (display name; localised; for indexed-group children user-renamable, e.g. "Mask 1"), `propertyIndex` (1-based position in parent), `propertyDepth` (0 = layer, 1 = top-level group...), `parentProperty`.
  - Flags on every node: `enabled` (eyeball for effects/masks/layer styles/shape items; for a layer = Video switch), `canSetEnabled`, `active`, `selected`, `elided` (organisational group not shown in the Timeline and not indenting its children, e.g. a text layer's internal group), `isModified` (changed since creation), `isEffect`, `isMask`.
  - Addressing: by path of matchNames or names or indices; expressions use display names or `("ADBE …")` matchNames or indices interchangeably (`layer.effect("Blur")("Blurriness")` ≡ `layer("ADBE Effect Parade")(1)(1)`).
  - Dynamic children: a group can add a child only if its kind accepts that matchName (`canAddProperty`); adding to an indexed group appends at the end unless inserted.
  - Twirl state (expanded/collapsed in the Timeline) is UI state, persisted per property UNVERIFIED.
- **Priority:** P0 the backbone of the data model and AE interop.
- **Owner:** bae-model
- **Notes:** Scripting `PropertyBase`, `Property`, `PropertyGroup`. BAE decision: each Prim attribute carries its AE matchName as metadata so AEP/scripting/expression addressing works without a separate mapping table; display names come from i18n. Lottie uses short keys instead of matchNames (`ks`, `masksProperties`, `ef`, `shapes`, `t`) but also writes `mn` (matchName) on effects, shapes and layers.

### AE-PRJ-349 · Layer top-level property groups and matchNames (图层属性组)

- **What it does:** Lists the fixed top-level groups of each layer kind, their matchNames and Timeline display order; this is the canonical interop table.
- **Exact semantics:** Layer matchNames: `ADBE AV Layer` (footage, solid, null, adjustment, precomp, audio), `ADBE Text Layer`, `ADBE Vector Layer` (shape), `ADBE Camera Layer`, `ADBE Light Layer`, 3D model layer `ADBE 3D Model Layer` UNVERIFIED.
  | Group (中文) | matchName | Exists on | Notes |
  |---|---|---|---|
  | Marker (标记) | `ADBE Marker` | all | MARKER property (AE-PRJ-327); hidden until markers exist |
  | Text (文本) | `ADBE Text Properties` | text | Source Text `ADBE Text Document`, Path Options `ADBE Text Path Options`, More Options `ADBE Text More Options`, Animators `ADBE Text Animators` (text.md) |
  | Contents (内容) | `ADBE Root Vectors Group` | shape | indexed group (shapes.md) |
  | Time Remap (时间重映射) | `ADBE Time Remapping` | AV layers with time-varying source (movie, sequence, precomp, audio, 3D model) | OneD seconds; only when enabled (AE-PRJ-376) |
  | Motion Trackers (动态跟踪器) | `ADBE MTrackers` | AV layers | tracker data (ui-a Tracker) |
  | Masks (蒙版) | `ADBE Mask Parade` | AV, text, shape | indexed; each `ADBE Mask Atom` (cmp-a) |
  | Effects (效果) | `ADBE Effect Parade` | AV, text, shape, audio (audio effects) | indexed (effects.md) |
  | Essential Properties (基本属性) | `ADBE Layer Overrides` | precomp layers whose comp has Essential Graphics controllers | contains overrides and Media Replacement `ADBE Layer Source Alternate`; version in AE-PRJ-420 |
  | Transform (变换) | `ADBE Transform Group` | all except audio (and ambient light) | see below |
  | Layer Styles (图层样式) | `ADBE Layer Styles` | AV, text, shape | shown only when a style is added (AE-PRJ-409) |
  | Geometry Options (几何选项) | `ADBE Extrsn Options Group` (text/shape: Bevel, Extrusion Depth…) or `ADBE Plane Options Group` (footage/solid: Curvature, Segments) | 3D layers in CINEMA 4D / Advanced 3D comps | hidden in Classic 3D |
  | Material Options (材质选项) | `ADBE Material Options Group` | 3D AV/text/shape layers | cmp-b |
  | Camera Options (摄像机选项) | `ADBE Camera Options Group` | camera | Zoom `ADBE Camera Zoom`, Depth of Field, Focus Distance, Aperture, Blur Level, Iris Shape… |
  | Light Options (灯光选项) | `ADBE Light Options Group` | light | Intensity `ADBE Light Intensity`, Color `ADBE Light Color`, Cone Angle… |
  | Audio (音频) | `ADBE Audio Group` | layers with audio | Audio Levels `ADBE Audio Levels` (TwoD, dB) |
  | Data (数据) | UNVERIFIED | data layers | AE-PRJ-346 |
  - Transform group children (matchName → value type, default): Anchor Point `ADBE Anchor Point` (ThreeD_SPATIAL; camera/light: Point of Interest), Position `ADBE Position` (ThreeD_SPATIAL; X/Y/Z Position `ADBE Position_0/_1/_2` OneD when separated), Scale `ADBE Scale` (ThreeD, % [100,100,100]), Orientation `ADBE Orientation` (ThreeD, 3D only), X Rotation `ADBE Rotate X`, Y Rotation `ADBE Rotate Y` (3D only), Rotation / Z Rotation `ADBE Rotate Z` (OneD, degrees), Opacity `ADBE Opacity` (OneD %, 0..100, default 100). 2D layers report 2D values in the UI but store 3 components (z ignored). Semantics and matrix order: transform-keyframes-expressions.md [ani-a].
  - Timeline display order (top to bottom): Time Remap (when enabled), Text / Contents, Masks, Effects, Essential Properties, Transform, Layer Styles, Geometry Options, Material Options, Audio. Scripting order of AVLayer top-level groups per the docsforadobe AVLayer match-name page: `ADBE Marker`, `ADBE Time Remapping`, `ADBE MTrackers`, `ADBE Mask Parade`, `ADBE Effect Parade`, `ADBE Layer Overrides`, `ADBE Transform Group`, … (exact numeric indices UNVERIFIED); BAE must address by matchName, not index.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Sources: docsforadobe "Match Names > Layer" pages (AVLayer, 3D Layer, Camera, Light, Text, Shape, Layer Styles). Lottie: `ks` (transform: `a`, `p` (or split `p.x`/`p.y`), `s`, `r`/`rx`/`ry`/`rz`, `or`, `o`, `sk`, `sa`), `masksProperties`, `ef`, `sy`, `shapes`, `t`, `tm`, `au`.

### AE-PRJ-350 · Property value types (属性值类型)

- **What it does:** Every leaf property has one value type that fixes its storage, interpolation and expression result type.
- **Exact semantics:** `PropertyValueType` (AE scripting) in documentation order:
  | Type | Value | Interpolation | Examples |
  |---|---|---|---|
  | NO_VALUE | none | — | effect group headers, Effect "Compositing Options" topic, Layer Styles headers |
  | ThreeD_SPATIAL | [x, y, z] f64 | spatial (motion path, spatial tangents, roving) + temporal | Position, Anchor Point, Point of Interest, 3D Point Control |
  | ThreeD | [x, y, z] f64 | temporal per dimension | Scale, Orientation |
  | TwoD_SPATIAL | [x, y] f64 | spatial + temporal | effect Point Control (e.g. Lens Flare "Flare Center"), Beam start/end points |
  | TwoD | [x, y] f64 | temporal | Audio Levels, Mask Feather |
  | OneD | f64 | temporal | Rotation, Opacity, sliders, angles, checkboxes (0/1), popups (1-based), Time Remap |
  | COLOR | [r, g, b, a] f64 in 0..1 (may exceed 1 in 32 bpc) | temporal per channel | Fill Color, Light Color |
  | CUSTOM_VALUE | opaque blob | not interpolable (hold) or effect-defined | Levels Histogram, Curves, gradient ramp data |
  | MARKER | MarkerValue | none (hold) | `ADBE Marker` |
  | LAYER_INDEX | int, 0 = none | hold | Layer popups in effects (e.g. Set Matte "Take Matte From Layer"); stores a layer reference, displayed as index. AE 26.5 adds a render-stage popup next to layer parameters: `LayerInputStageType.SOURCE` = 0 (before masks and effects), `ALL_EFFECTS` = -1 (masks + all effects), `ONLY_MASKS` = -2 (masks, no effects); scripting `layerInputStage`, `inputLayerAndStage` = [layerIndex, stageIndex], `getInputStageCycleSafeLimit()` |
  | MASK_INDEX | int, 0 = none | hold | Mask popups in effects (Stroke "Path") |
  | SHAPE | Shape {vertices, inTangents, outTangents, closed, featherSegLocs…} | path interpolation (vertex-wise) | Mask Path, shape Path |
  | TEXT_DOCUMENT | TextDocument | hold only | Source Text |
  - Integer-like OneD properties (checkbox, popup, layer/mask index) only allow Hold keyframes UNVERIFIED for checkbox/popup; AE shows hold-icon keys.
  - Boolean switches (Video, Shy…) are NOT properties; they are layer attributes and cannot be keyframed.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Expression result types: transform-keyframes-expressions.md [ani-b]. BAE decision: LAYER_INDEX stores a layer ID internally (AE stores the layer ID too and shows the index UNVERIFIED) so reordering does not break the reference. 32 bpc colours: compositing-3d-color.md [cmp-b].

### AE-PRJ-351 · Property attributes and flags (属性特性)

- **What it does:** Per-property metadata that governs what UI and edits are allowed: keyframing, expressions, spatiality, separation, units and value limits.
- **Exact semantics:**
  | Attribute | Type | Meaning |
  |---|---|---|
  | canVaryOverTime | bool | may have keyframes or an expression (stopwatch shown). False for e.g. effect "Compositing Options" topic headers, some popups UNVERIFIED |
  | isTimeVarying | bool | has keyframes or an enabled expression (implies canVaryOverTime) |
  | isSpatial | bool | value is a position: motion path and spatial interpolation apply |
  | dimensionsSeparated / isSeparationLeader / isSeparationFollower / separationDimension / separationLeader | bool / int / ref | Separate Dimensions state; Position leads X/Y/Z Position followers (ani-a) |
  | canSetExpression | bool | an expression may be set (false e.g. for Marker in older versions, for MASK_INDEX UNVERIFIED) |
  | expression / expressionEnabled / expressionError | string / bool / string | ani-b |
  | unitsText | string | "pixels", "degrees", "percent", "seconds", "dB" … (display only) |
  | hasMin / minValue, hasMax / maxValue | bool / number | hard limits enforced on set (values outside throw in scripting; UI clamps). E.g. Opacity 0..100; effect limits per parameter in effects.md |
  | numKeys | int | keyframe count |
  | alternateSource / canSetAlternateSource | item | Media Replacement for MOGRT (AE 18.0) |
  | essentialPropertySource | ref | for Essential Properties overrides, the original property (AE 22.0) |
  | propertyParameters | string[] | item strings of any dropdown-menu property, incl. custom Dropdown Menu Control items (getter added AE 26.0; setter `setPropertyParameters()` AE 17.0.1) |
  - Slider ranges vs limits: many effect parameters have a UI slider range smaller than the valid range (e.g. slider 0..100 but typed values up to 1000); only hasMin/hasMax limits are enforced. The slider range is effect metadata (effects.md [fx-a]).
  - Values typed beyond limits are clamped; expression results beyond limits are clamped at evaluation UNVERIFIED (e.g. Opacity expression returning 150 renders as 100).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting names as in the table. BAE decision: keep these flags in the property schema (static per matchName) and not per instance, except state (dimensionsSeparated, expression, keyframes).

### AE-PRJ-352 · Reset property and group (重置)

- **What it does:** Restores a property or every property in a group to its default value.
- **Exact semantics:**
  - Group: click "Reset" (重置) next to the group name in the Timeline (Transform, Effect, Layer Style, Text animator groups…) or Effect Controls; resets every child property.
  - Single property: context menu on the property name > Reset.
  - If the property has keyframes, Reset does NOT delete them: it adds/sets a keyframe at the CTI with the default value. Without keyframes it sets the static value. Expressions are kept UNVERIFIED.
  - Defaults are layer-dependent where AE defines them so: Anchor Point resets to the layer's source centre (or (0,0) for text/shape), Position to the comp centre UNVERIFIED (AE resets Position to comp centre for AV layers).
  - Resetting an effect resets its parameters but not its enabled state or name.
  - Related: double-click Pan Behind tool resets Anchor Point to the layer's default location; Alt/Opt+double-click additionally moves the layer to the comp centre.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** One undo step per reset. Implementation: defaults come from the property schema; layer-dependent defaults are computed by a function of the layer (source size, comp size).

## Layer common attributes

### AE-PRJ-353 · Layer name and source name (图层名称、源名称)

- **What it does:** A layer has an optional user-set name; when unset it displays its source's name. The Timeline column toggles between Layer Name and Source Name views.
- **Exact semantics:**
  - Stored: `name: Option<String>`. `isNameSet = name.is_some()`; for sourceless layers (text, shape, camera, light, null? UNVERIFIED) AE always reports `isNameSet = true` because a name is generated at creation ("Shape Layer 1", "Camera 1"); text layers are special: the name tracks the first line of the Source Text until renamed.
  - Displayed name in Layer Name view: `name` if set, else `[sourceName]` in square brackets. Brackets mean "name taken from source"; renaming removes them; clearing the name (rename to empty) restores the source name UNVERIFIED.
  - Source Name view shows the source item name regardless (for sourceless layers the layer name).
  - Rename: select layer, Enter/Return, type, Enter. Also for property groups that allow it (masks, effects, shape groups, animators).
  - Changing the source item name updates every layer without its own name. Replace Source (AE-PRJ-404) keeps a set name.
  - `isNameFromSource` (AVLayer) = !isNameSet && source != null.
  - Names need not be unique within a comp; `thisComp.layer("name")` resolves to the topmost (lowest index) match.
  - Renaming a layer updates expressions in the project that reference it by name (`thisComp.layer("Old")` → `"New"`) UNVERIFIED version: AE 17.x+ does this for references within the same project; earlier versions broke them.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Layer.name`, `isNameSet`, `AVLayer.isNameFromSource`. Lottie `nm` (always explicit; exporters write the displayed name). BAE decision: expressions reference layers by ID internally and render names on display so renames never break.

### AE-PRJ-354 · Layer comment (注释)

- **What it does:** Free-text note per layer shown in the Timeline Comments column.
- **Exact semantics:** `comment: String`, default "". Edited by clicking in the Comments column (hidden by default). Not used by rendering; searchable in the Timeline search field. Copied by Duplicate/Split/copy-paste.
- **Priority:** P2
- **Owner:** bae-model
- **Notes:** Scripting `Layer.comment`. Not in Lottie. Used by tagging scripts.

### AE-PRJ-355 · Layer index and layer ID (图层索引、图层 ID)

- **What it does:** `index` is the 1-based stacking position (1 = top); `id` is a persistent unique identifier that survives reordering.
- **Exact semantics:**
  - `index ∈ 1..numLayers`, contiguous, recomputed after every insert/delete/reorder. Displayed in the Timeline "#" column. Expressions `thisLayer.index`, `thisComp.layer(i)`.
  - `id`: integer unique within the project and persistent across save/reload (scripting `Layer.id`, read-only, AE 22.0; lookup `app.project.layerByID(id)`). Not shown in the UI. When a project is imported into another project, all its layers get new IDs.
  - Parent links, track matte links, LAYER_INDEX effect parameters and expression pick-whip links (when written by AE as `thisComp.layer("name")`) refer to layers; AE stores parent/matte/LAYER_INDEX by internal ID so reordering keeps them, while expressions refer by name or index text.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Lottie: `ind` (any integer, unique within a comp; exporters write index) and `parent` (an `ind`). BAE: Prim path uses ID, display uses index.

### AE-PRJ-356 · Layer selection state (图层选择状态)

- **What it does:** Which layers (and properties/keyframes) are selected in a comp; commands operate on the selection.
- **Exact semantics:** `selected: bool` per layer, per property and per keyframe; selection is per comp and saved in the project (AE persists the selection UNVERIFIED). Locked layers cannot be selected via Composition panel or Timeline clicks. Ctrl+A selects all layers, Ctrl+Shift+A deselects; Ctrl+Up/Down selects the next layer above/below (ui-preview-render-io.md [ui-b]).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Scripting `Layer.selected`, `CompItem.selectedLayers` (0-based order of selection), `selectedProperties`. BAE decision: selection lives in the `session` sublayer (ARCHITECTURE.md 4.3), never in the shared document, so collaborators do not fight over it; selection ORDER is kept because Sequence Layers, Precompose and New Comp from Selection depend on it.

### AE-PRJ-357 · Video switch (视频)

- **What it does:** The eyeball toggles whether the layer's visual contributes to previews and renders.
- **Exact semantics:**
  - `enabled: bool`, default true. For adjustment layers: whether their effects apply. For cameras: whether the camera can be the active camera. For lights: whether the light lights the scene.
  - A disabled layer still evaluates its transforms and expressions (children follow it; expressions can sample it). It can still be used as a track matte: assigning a track matte turns the matte layer's Video off and the matte still works (in all versions; AE 23.0+ lets the user turn the matte's Video back on to also show it).
  - `active(t) = enabled && soloRule(layer) && inPoint <= t < outPoint` (scripting `Layer.active`, `activeAtTime(t)`).
  - Layer > Switches > Show All Video; Hide Other Video (disables all but selected).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `Layer.enabled`. Lottie `hd: true` for hidden layers (exporters often omit disabled layers instead).

### AE-PRJ-358 · Audio switch (音频)

- **What it does:** The speaker icon toggles whether the layer's audio is heard in previews and included in renders.
- **Exact semantics:** `audioEnabled: bool`, default true; only shown when `hasAudio`. `audioActive(t) = audioEnabled && audioSoloRule && in span`. No effect on visuals.
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** Scripting `AVLayer.audioEnabled`, `audioActive`, `audioActiveAtTime()`.

### AE-PRJ-359 · Solo switch (独奏)

- **What it does:** Soloing isolates layers: when any layer of a category is soloed, only soloed layers of that category render.
- **Exact semantics:**
  ```
  // categories: visual (AV layers with video, text, shape, solid, null, adjustment), light, audio
  anySoloVisual = any(L.solo for L in comp if L is visual)
  anySoloLight  = any(L.solo for L in comp if L is light)        UNVERIFIED (cameras: solo has no effect)
  anySoloAudio  = any(L.solo for L in comp if L.hasAudio)
  visualRenders(L) = L.enabled && (!anySoloVisual || L.solo)
  lightOn(L)       = L.enabled && (!anySoloLight  || L.solo)
  audible(L)       = L.audioEnabled && (!anySoloAudio || L.solo)
  ```
  - Multiple solos combine as a union. Alt/Opt-click a Solo switch: solo this layer and unsolo all others.
  - The Video switch of non-soloed layers is drawn dimmed (UI).
  - Solo affects previews and renders; Render Settings "Solo Switches" (独奏开关): Current Settings (当前设置) (default) or All Off (全部关闭) ignores solos for that render.
  - Solo of a layer inside a precomp affects only that precomp (solo is per comp).
  - Track matte layers: soloing a layer that uses a matte still renders its matte (the matte is not "rendered" as visual output) UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `Layer.solo`. Preference "Solo Switches Affect Nested Comps" does not exist; AE help: soloing excludes other layers of the same type; lights and audio layers are unaffected by soloing a video layer.

### AE-PRJ-360 · Lock switch (锁定)

- **What it does:** Prevents a layer from being selected or edited.
- **Exact semantics:** `locked: bool`, default false. A locked layer cannot be selected in the Composition panel or Timeline; attempts make it flash; its properties cannot be changed by UI (scripts also cannot change most attributes of a locked layer; Scale Composition.jsx fails on locked layers). Commands that act on "all layers" (Lift/Extract, Select All) skip locked layers. Layer > Switches > Lock (Ctrl+L / Cmd+L) locks the selection; Unlock All Layers is Ctrl+Shift+L / Cmd+Shift+L (AE shortcut reference). Rendering unaffected.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Layer.locked`. BAE decision: Agent/collaborator commands must also respect Lock (Command validation rejects edits to locked layers unless the command unlocks first).

### AE-PRJ-361 · Shy switch and Hide Shy Layers (消隐)

- **What it does:** Marks layers to be hidden from the Timeline layer outline when the comp's Hide Shy Layers switch is on.
- **Exact semantics:** `shy: bool`, default false. Visible in Timeline iff `!(comp.hideShyLayers && layer.shy)`. Never affects rendering, previews or selection via the Composition panel (shy layers can still be clicked in the viewer UNVERIFIED). Layer > Switches > Shy toggles; Timeline panel menu > Hide Shy Layers.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Layer.shy`, `CompItem.hideShyLayers`. zh-CN AE 2026 UI: 消隐 (glossary); some help pages say 隐蔽.

### AE-PRJ-362 · Collapse Transformations / Continuously Rasterize switch (折叠变换 / 连续栅格化)

- **What it does:** One switch with two meanings: for precomp layers it collapses the nested comp's transforms into the parent's (no intermediate flattening); for vector-source layers (Illustrator/PDF/EPS/SVG footage) it re-rasterizes vectors at the transformed resolution every frame.
- **Exact semantics:**
  - `collapseTransformation: bool`, default false. Settable when `canSetCollapseTransformation`. Per the AE help (Layers page, switch list), the switch means Collapse Transformations for precomp layers and Continuously Rasterize for shape layers, text layers and vector-footage layers (Illustrator, PDF, EPS, SVG). The precompose help page states that text and shape layers are always continuously rasterized; UNVERIFIED: for them the switch then only changes the render order (with it on, masks/effects are applied after the layer transform, like other continuously rasterized layers) rather than sharpness; details in cmp-a.
  - Data only here. Rendering consequences (blend modes passing through, 3D layers interleaving with the parent comp, mask/effect order change, frame boundary removal, adjustment layers inside collapsed precomps) are specified in compositing-3d-color.md [cmp-a] "collapse/continuous rasterize".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.collapseTransformation`, `canSetCollapseTransformation`. Lottie `ct` (0/1; older `cp`).

### AE-PRJ-363 · Quality switch (品质)

- **What it does:** Sets how precisely the layer is rendered: Best, Draft or Wireframe.
- **Exact semantics:**
  | Value (中文) | Scripting | Meaning |
  |---|---|---|
  | Best (最佳) | `LayerQuality.BEST` | subpixel positioning, anti-aliasing, full effect precision; sampling per AE-PRJ-364 |
  | Draft (草图) | `LayerQuality.DRAFT` | no anti-aliasing, no subpixel positioning (positions rounded to pixels), some effects approximate |
  | Wireframe (线框) | `LayerQuality.WIREFRAME` | layer drawn as its bounding box outline only |
  - Default Best. Clicking the Quality switch toggles among Best (bilinear) → Draft → Best (bicubic) → back to Best (bilinear) (order as listed in the help); Layer > Quality (图层 > 质量) offers all of these plus Wireframe. Ctrl+U / Ctrl+Shift+U / Ctrl+Alt+Shift+U (Cmd on macOS) set Best / Draft / Wireframe for the selection (AE shortcut reference lists the three shortcuts in that order).
  - Render Settings "Quality" (Current Settings / Best / Draft / Wireframe) can override it for a render.
  - Duplicated or split layers keep the Quality setting.
- **Priority:** P0 (Best/Draft); Wireframe P2.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.quality`. Rendering details: compositing-3d-color.md [cmp-a] "quality/sampling/subpixel". zh-CN help: switch 品质, menu 图层 > 质量, values 最佳 / 草图 / 线框, sampling 双线性 / 双立方 (Source: https://helpx.adobe.com/cn/after-effects/using/layers.html).

### AE-PRJ-364 · Sampling: Bilinear / Bicubic (采样：双线性 / 双立方)

- **What it does:** For Best-quality layers, chooses the resampling filter used when the layer is transformed.
- **Exact semantics:** `samplingQuality ∈ {BILINEAR (双线性) default, BICUBIC (双立方)}`; AE 12.0 (CC). Alt+B / Opt+B = Best/Bilinear; Alt+Shift+B / Opt+Shift+B = Best/Bicubic. Ignored in Draft/Wireframe. Effects' internal transforms still use bilinear unless the effect implements otherwise.
- **Priority:** P1
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.samplingQuality`. Filter kernels: compositing-3d-color.md [cmp-a].

### AE-PRJ-365 · Effect switch (效果)

- **What it does:** The fx switch toggles all effects of a layer on/off for rendering without changing each effect's own on/off state.
- **Exact semantics:** `effectsActive: bool`, default true; shown only when the layer has effects. Effective `effect.on = layer.effectsActive && effect.enabled`. Layer styles are not affected UNVERIFIED. Masks are not affected.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.effectsActive`. Per-effect eyeball: `PropertyBase.enabled` (effects.md [fx-a]).

### AE-PRJ-366 · Frame Blending switch (帧混合)

- **What it does:** Per-layer choice of how to synthesize in-between frames when a footage/precomp layer is time-stretched, remapped or conformed to a different frame rate.
- **Exact semantics:**
  | Value (中文) | Scripting | Lottie |
  |---|---|---|
  | Off (关) | `FrameBlendingType.NO_FRAME_BLEND` | — |
  | Frame Mix (帧混合) | `FrameBlendingType.FRAME_MIX` | — |
  | Pixel Motion (像素运动) | `FrameBlendingType.PIXEL_MOTION` | — |
  - Default Off. Clicking the switch cycles Off → Frame Mix → Pixel Motion → Off UNVERIFIED; Layer > Frame Blending > Off / Frame Mix / Pixel Motion.
  - Only movie, image-sequence and precomp layers can use it (stills, text, shapes, solids: switch not available).
  - Effective iff `comp.frameBlending && layer.frameBlendingType != NO_FRAME_BLEND` (AE-PRJ-315). `AVLayer.frameBlending` (read-only) reports the layer switch.
  - Math: transform-keyframes-expressions.md [ani-a] "frame blending"; rendering: cmp-a.
- **Priority:** P1 (Frame Mix); Pixel Motion P2.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Render Settings Frame Blending override: ui-preview-render-io.md [ui-c].

### AE-PRJ-367 · Motion Blur switch (运动模糊)

- **What it does:** Per-layer opt-in to motion blur.
- **Exact semantics:** `motionBlur: bool`, default false. Effective iff `comp.motionBlur && layer.motionBlur && comp.shutterAngle > 0` (and Render Settings motion blur not "Off for All Layers"). Available on all visual layers (not camera/light/audio; camera motion blurs 3D layers that have the switch on). Comp settings: AE-PRJ-313.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting `AVLayer.motionBlur`. Lottie layer `mb` (boolean; lottie-web ignores it).

### AE-PRJ-368 · Adjustment Layer switch (调整图层)

- **What it does:** Turns any visual AV layer into an adjustment layer (AE-PRJ-340) or back.
- **Exact semantics:** `adjustmentLayer: bool`, default false (true for layers made by New > Adjustment Layer). Allowed on footage, solid, precomp, text and shape layers (shape/text adjustment layers are a common AE technique: the shape/text alpha becomes the effect coverage); not on camera, light, audio-only layers. UNVERIFIED: whether a null object can be switched to an adjustment layer. Toggling does not change effects, masks or transforms. Render semantics: cmp-a.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.adjustmentLayer`.

### AE-PRJ-369 · 3D Layer switch (3D 图层)

- **What it does:** Makes a layer participate in 3D space (Z position, X/Y rotation, orientation, cameras, lights).
- **Exact semantics:**
  - `threeDLayer: bool`, default false. Available on footage, solid, null, precomp, text, shape, adjustment layers; cameras, lights and 3D model layers are always 3D; audio-only layers cannot be 3D.
  - Turning ON: Position, Anchor Point and Scale gain a z component (Position z = 0, Anchor z = 0, Scale z = 100 %); adds Orientation [0,0,0], X Rotation 0, Y Rotation 0, Geometry Options, Material Options; Rotation is renamed Z Rotation (same value/keys).
  - Turning OFF: Orientation, X Rotation, Y Rotation, Geometry Options and Material Options are REMOVED with their values, keyframes and expressions (not restored by turning 3D on again); Position/Anchor/Scale keep their keyframes and expressions, but their z values are hidden and ignored.
  - Text with "Enable Per-character 3D" shows a distinct switch icon (`threeDPerChar`, text.md).
  - Layer > 3D Layer toggles for the selection.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting `AVLayer.threeDLayer`, `threeDPerChar`. Lottie `ddd: 1` on the layer (and on the root if any layer is 3D). BAE decision: keep removed 3D values in an undo-only snapshot so that 2D→3D right after 3D→2D can be restored by Undo, but not by re-toggling (AE parity).

### AE-PRJ-370 · Preserve Underlying Transparency (保留基础透明度)

- **What it does:** The "T" switch in the Modes column makes the layer paint only where the layers below already have opacity.
- **Exact semantics:** `preserveTransparency: bool`, default false; available on visual layers. Data only here; formula in compositing-3d-color.md [cmp-a] "preserve transparency".
- **Priority:** P1
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.preserveTransparency`. Not in Lottie.

### AE-PRJ-371 · Blending Mode attribute (混合模式)

- **What it does:** Stores how the layer is combined with the composite below it.
- **Exact semantics:** `blendingMode: enum`, default Normal. Menu groups in AE order (scripting `BlendingMode.*`, Lottie `bm` where it exists):
  | Group | Modes |
  |---|---|
  | Normal | Normal (正常) `NORMAL` bm 0, Dissolve (溶解) `DISSOLVE`, Dancing Dissolve (动态抖动溶解) `DANCING_DISSOLVE` |
  | Subtractive | Darken (变暗) `DARKEN` bm 4, Multiply (相乘) `MULTIPLY` bm 1, Color Burn (颜色加深) `COLOR_BURN` bm 7, Classic Color Burn (经典颜色加深) `CLASSIC_COLOR_BURN`, Linear Burn (线性加深) `LINEAR_BURN`, Darker Color (较深的颜色) `DARKER_COLOR` |
  | Additive | Add (相加) `ADD` bm 16, Lighten (变亮) `LIGHTEN` bm 5, Screen (屏幕) `SCREEN` bm 2, Color Dodge (颜色减淡) `COLOR_DODGE` bm 6, Classic Color Dodge (经典颜色减淡) `CLASSIC_COLOR_DODGE`, Linear Dodge (线性减淡) `LINEAR_DODGE`, Lighter Color (较浅的颜色) `LIGHTER_COLOR` |
  | Complex | Overlay (叠加) `OVERLAY` bm 3, Soft Light (柔光) `SOFT_LIGHT` bm 9, Hard Light (强光) `HARD_LIGHT` bm 8, Linear Light (线性光) `LINEAR_LIGHT`, Vivid Light (亮光) `VIVID_LIGHT`, Pin Light (点光) `PIN_LIGHT`, Hard Mix (实色混合) `HARD_MIX` bm 17 |
  | Difference | Difference (差值) `DIFFERENCE` bm 10, Classic Difference (经典差值) `CLASSIC_DIFFERENCE`, Exclusion (排除) `EXCLUSION` bm 11, Subtract (相减) `SUBTRACT`, Divide (相除) `DIVIDE` |
  | HSL | Hue (色相) `HUE` bm 12, Saturation (饱和度) `SATURATION` bm 13, Color (颜色) `COLOR` bm 14, Luminosity (发光度) `LUMINOSITY` bm 15 |
  | Matte | Stencil Alpha (模板 Alpha) `STENCIL_ALPHA`, Stencil Luma (模板亮度) `STENCIL_LUMA`, Silhouette Alpha (轮廓 Alpha) `SILHOUETTE_ALPHA`, Silhouette Luma (轮廓亮度) `SILHOUETTE_LUMA` |
  | Utility | Alpha Add (Alpha 添加) `ALPHA_ADD`, Luminescent Premul (冷光预乘) `LUMINESCENT_PREMUL` |
  - Shift+= / Shift+- cycles through modes for selected layers (ui-b). Formulas: compositing-3d-color.md [cmp-a] "blend modes & formulas".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Chinese mode names follow AE zh-CN UI UNVERIFIED（译名待核） (verify against AE 2026). Scripting `AVLayer.blendingMode`.

### AE-PRJ-372 · Track Matte attribute (轨道遮罩)

- **What it does:** Stores which layer this layer uses as a matte and how (alpha/luma, normal/inverted).
- **Exact semantics:**
  | Field | Type | Default | Notes |
  |---|---|---|---|
  | trackMatteLayer | layer ref or null | null | AE 23.0+: any layer in the same comp chosen via the Track Matte pick whip/menu; before 23.0: implicitly the layer directly above |
  | trackMatteType | enum | NO_TRACK_MATTE | `ALPHA` (Alpha 遮罩), `ALPHA_INVERTED` (Alpha 反转遮罩), `LUMA` (亮度遮罩), `LUMA_INVERTED` (亮度反转遮罩) |
  - AE 23.0 UI: Track Matte column (layer menu + pick whip) plus two toggle buttons: Alpha/Luma and Normal/Inverted. Choosing a matte layer turns that layer's Video switch off (it still works as a matte). One matte layer can serve several layers.
  - Read-only derived: `hasTrackMatte`, `isTrackMatte`.
  - Constraints: the matte layer cannot be the layer itself; cycles (A mattes B mattes A) are prevented UNVERIFIED; matte must be in the same comp.
  - Legacy projects (pre-23.0): matte = layer at index-1; on open, AE converts to the explicit link.
  - Duplicate/Precompose rules: AE-PRJ-384, AE-PRJ-402. Render formulas: compositing-3d-color.md [cmp-a] "track mattes".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.setTrackMatte(layer, type)`, `removeTrackMatte()`, legacy `trackMatteType`. Lottie: `tt` on the matted layer (1 alpha, 2 alpha inverted, 3 luma, 4 luma inverted), `td: 1` on the matte layer, `tp` = `ind` of the matte layer (newer exporters; default = layer above).

## Layer timing

### AE-PRJ-373 · Layer time model: startTime, inPoint, outPoint, stretch (图层时间)

- **What it does:** Defines where a layer sits in comp time, which part of it is visible, and how comp time maps to the layer's own (source) time.
- **Exact semantics:**
  | Field (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | startTime (开始时间) | time | 0 or CTI (AE-PRJ-388) | -10800..10800 | s (comp time) | comp time at which layer time 0 (source frame 0) occurs |
  | inPoint (入点) | time | = startTime for new layers | -10800..10800 | s (comp time) | first visible instant (inclusive) |
  | outPoint (出点) | time | in + duration | -10800..10800 | s (comp time) | end of visibility (exclusive) |
  | stretch (伸缩) | f64 | 100 | -9900..9900; values in (0, 1) are set to 1 and values in (-1, 0) to -1 (scripting doc); exactly 0 UNVERIFIED (treat as invalid) | % | negative = reversed playback |
  | duration (持续时间) | derived | | | s | `outPoint - inPoint` (Timeline Duration column) |
  ```
  k = stretch / 100
  layerTime(compTime) = (compTime - startTime) / k          // a.k.a. source time before time remap
  compTime(layerTime) = startTime + layerTime * k
  visible(compTime)   = inPoint <= compTime < outPoint      // half-open; evaluated at frame start times
  if timeRemapEnabled: sourceTime = TimeRemap.valueAt(compTime) else sourceTime = layerTime
  for k < 0: source time decreases as compTime increases (AE-PRJ-375)
  ```
  - Keyframes, layer markers and the time-remap curve are stored in layer time, so they move with startTime and scale with stretch. Expressions expose comp time (`key.time`, `time`) and layer time via `sourceTime()`/`thisLayer.sourceTime(t)` (ani-b).
  - Footage sampling: source frame = `floor(sourceTime * sourceFps + ε)` within `[0, sourceDuration)` (ε handles float error; BAE uses exact rationals).
  - The In/Out/Duration/Stretch Timeline columns edit these fields: typing In moves the whole layer (keeps duration) UNVERIFIED; Alt/Opt-click on In/Out moves that point to CTI (AE-PRJ-380); Ctrl/Cmd-click In/Out stretches the layer so that point reaches the CTI; clicking Duration or Stretch opens Time Stretch (AE-PRJ-374).
  - Alt+Home / Opt+Home moves the in points of selected layers to comp start; Alt+End to comp end (out point at comp end).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting `Layer.startTime`, `inPoint`, `outPoint`, `stretch`, `time`. Lottie `st` (frames), `ip`, `op` (frames), `sr` (stretch as factor, 1 = 100 %). lottie-web: `layerTime = (compFrame - st) / sr`. Evaluation math: transform-keyframes-expressions.md [ani-a] "time remap/frame selection".

### AE-PRJ-374 · Time Stretch dialog (时间伸缩)

- **What it does:** Layer > Time > Time Stretch changes the playback speed of a whole layer (and its keyframes) by a factor, holding one point in place.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Stretch Factor (拉伸系数) | f64 | current stretch | -9900..9900 (|x| ≥ 1) | % | |
  | New Duration (新持续时间) | time | current duration × new/old | > 0 | timecode | linked to Stretch Factor: `newDur = oldDur * newStretch/oldStretch` |
  | Hold in Place (原位定格) | enum | Layer In-point | Layer In-point (图层入点), Current Frame (当前帧), Layer Out-point (图层出点) | — | |
  ```
  r = newStretch / oldStretch
  pivot = (hold == In) ? inPoint : (hold == Current) ? CTI : outPoint
  // every comp-time attribute T of the layer (inPoint, outPoint, startTime) maps
  T' = pivot + (T - pivot) * r
  // keyframes/markers are in layer time -> they follow automatically
  ```
  - Negative factor reverses the layer (like AE-PRJ-375 but with the hold point semantics).
  - The layer's frame sampling follows the new mapping; inter-frame results need frame blending (AE-PRJ-366).
  - Applies to every layer type, including sourceless text and shape layers, where it only rescales keyframe and marker timing (UNVERIFIED: camera and light layers).
  - With time remap enabled, Time Stretch scales the Time Remap keyframe timing too.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Help: dialog fields "Stretch Factor", "New Duration", "Hold in Place" (zh-CN help: 拉伸系数, 新持续时间, 原位定格). Ctrl/Cmd-click on In/Out column: stretch to CTI. Precision: AE rounds New Duration to whole frames when typed in timecode UNVERIFIED.

### AE-PRJ-375 · Time-Reverse Layer (时间反向图层)

- **What it does:** Layer > Time > Time-Reverse Layer (Ctrl+Alt+R / Cmd+Opt+R) plays the layer backwards while keeping its in and out points.
- **Exact semantics:**
  ```
  stretch' = -stretch
  startTime' = inPoint + outPoint - startTime
  // inPoint and outPoint unchanged; keyframes, markers (stored in layer time) mirror around (in+out)/2
  ```
  - All keyframes of all properties reverse order (help). Applying twice restores the original.
  - The layer bar shows a red/hatched stripe for negative stretch (UI).
  - Adobe recommends reversing inside a precomp to keep keyframes un-reversed; "Time-Reverse Keyframes" (Animation > Keyframe Assistant) is the keyframe-only variant (ani-a).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Scripting: no direct method; set `stretch` negative and adjust startTime. Lottie: `sr` negative is not supported by lottie-web UNVERIFIED; exporter should bake time remap instead.

### AE-PRJ-376 · Enable Time Remapping and the Time Remap property (启用时间重映射、时间重映射)

- **What it does:** Layer > Time > Enable Time Remapping (Ctrl+Alt+T / Cmd+Opt+T) adds a keyframable Time Remap property whose value is the source time shown at each comp time.
- **Exact semantics:**
  - Available when `canSetTimeRemapEnabled`: layers with a time-varying source (movie, image sequence, audio, precomp, 3D model). Not for stills, solids, text, shape, null, camera, light.
  - On enable: Time Remap (`ADBE Time Remapping`, OneD, units seconds, displayed as timecode/frames) gets two linear keyframes: at the layer in point with value = layerTime(inPoint), and at the layer out point with value = layerTime(outPoint) (for an untrimmed layer: 0 and the source duration) UNVERIFIED for trimmed layers.
  - After enabling, the layer's out point can be dragged beyond the source end (the layer becomes extendable both directions); after the last keyframe the value holds (constant extrapolation), so the last frame freezes.
  - Values are clamped at evaluation to `[0, sourceDuration - 1 frame]` for footage UNVERIFIED (precomps: beyond the nested comp's duration render transparent UNVERIFIED).
  - Disabling (toggle again) removes the property and all its keyframes; in/out points that exceed the source are clamped back UNVERIFIED.
  - Time Remap can have expressions (common: `loopOut()`), and the property is not affected by the Speed graph semantics beyond normal keyframes.
  - With Time Remap on, stretch still scales its key times (AE-PRJ-374) but the source time comes only from the Time Remap value.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Scripting `AVLayer.timeRemapEnabled`. Lottie precomp `tm` (value in seconds of the nested comp, keyframable); lottie-web uses `tm` only for `ty: 0`. Evaluation: ani-a "time remap/frame selection".

### AE-PRJ-377 · Freeze Frame (冻结帧)

- **What it does:** Layer > Time > Freeze Frame shows the frame at the CTI for the layer's whole duration.
- **Exact semantics:**
  ```
  enable time remapping if needed; DELETE all existing Time Remap keyframes
  add one keyframe at CTI with value = layerTime(CTI) (current source time), interpolation Hold
  ```
  - Layer in/out points are unchanged. Applies to each selected layer that can time-remap.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Help confirms deletion of previous Time Remap keyframes. Lottie export: `tm` static value.

### AE-PRJ-378 · Freeze On Last Frame (在最后一帧上冻结)（译名待核）

- **What it does:** Layer > Time > Freeze On Last Frame holds the layer's last source frame until the end of the comp.
- **Exact semantics:**
  ```
  enable time remapping (keeping existing keys UNVERIFIED)
  lastT = time of the last source frame in comp time = compTime(sourceDuration - sourceFrameDur)
  add a Hold keyframe at lastT with value = sourceDuration - sourceFrameDur
  outPoint = comp.duration
  ```
- **Priority:** P2
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Introduced AE 16.1 (2019) UNVERIFIED. Help text describes "freeze the last frame of a layer until the end of the composition; sets time remapping keyframes and extends the layer".

### AE-PRJ-379 · Layer extent limits: stills versus time-based sources (图层时长限制)

- **What it does:** Defines how far a layer's in/out points can be dragged depending on its source.
- **Exact semantics:**
  | Source | Can extend in/out beyond source? |
  |---|---|
  | Movie, image sequence, audio, precomp, 3D model (time remap off) | No: `inPoint >= compTime(0)` and `outPoint <= compTime(sourceDuration)` (reversed: mirrored); trims only |
  | Same with time remap on | Yes, both directions, any length (frames hold) |
  | Still image, solid, null, adjustment (solid source), text, shape, camera, light | Yes, any length both directions; startTime is just a reference |
  | Image sequence interpreted as a single still (one file) | as still |
  - Looping footage (Interpret Footage > Loop N times) multiplies `sourceDuration` (prj-a).
  - In and out points are snapped to comp frames when edited in the UI; scripting can set fractional values.
  - A layer may have zero duration? No: minimum duration is one comp frame UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Lottie layers have no source-length constraint (`ip`/`op` free).

## Layer editing operations

### AE-PRJ-380 · Move layers in time (在时间上移动图层)

- **What it does:** Shifts whole layers (with keyframes and markers) earlier or later in comp time.
- **Exact semantics:**
  - Every move changes `startTime`, `inPoint`, `outPoint` by the same delta `d` (keyframes/markers follow because they are in layer time).
  | Gesture | Delta |
  |---|---|
  | Drag the duration bar (Shift snaps to CTI, markers, other layers' in/out, keyframes, work area) | drag amount, frame-quantised |
  | Alt/Opt+PageUp / PageDown | -1 / +1 comp frame |
  | Alt/Opt+Shift+PageUp / PageDown | -10 / +10 frames |
  | `[` | per selected layer: `d = CTI - inPoint` (each layer's in point to CTI) |
  | `]` | per selected layer: `d = CTI - outPoint` (each out point to CTI; out point = end of the frame at CTI UNVERIFIED: AE uses `CTI + 1 frame`) |
  | Shift+`[` | one `d` for all: `CTI - min(inPoint)` (keeps relative offsets) |
  | Shift+`]` | one `d` for all: `CTI(+1 frame) - max(outPoint)` |
  | Alt/Opt+Home | `d = 0 - inPoint` (in point to comp start) |
  | Alt/Opt+End | `d = comp.duration - outPoint` |
  | Typing a value in the In / Out column; Alt/Opt-click the In/Out value | moves the layer so that point equals the typed value / CTI |
  - Movement is not limited to the comp range (layers can start before 0 or after the end).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Shortcuts from the AE help table (Selecting and arranging layers; Keyboard shortcuts). Scripting: set `startTime`.

### AE-PRJ-381 · Trim layer in/out points (修剪图层入点/出点)

- **What it does:** Changes only the in point or out point of layers, without moving the layer content in time.
- **Exact semantics:**
  | Gesture | Effect per selected layer |
  |---|---|
  | Drag the start/end of the duration bar (Shift snaps) | inPoint / outPoint follow the pointer, frame-quantised |
  | Alt/Opt+`[` | `inPoint = CTI` |
  | Alt/Opt+`]` | `outPoint = CTI + 1 frame` (the frame at CTI stays visible) UNVERIFIED |
  | Alt/Opt+Shift+`[` | trims/extends all selected layers by one shared delta so that the earliest in point = CTI |
  | Alt/Opt+Shift+`]` | same for the latest out point |
  | Layer panel Set In / Set Out buttons | same as Alt+[ / Alt+] in layer time |
  - Clamped by source extent (AE-PRJ-379) and `inPoint < outPoint` (minimum one frame).
  - startTime, keyframes and markers do NOT move. Keyframes outside the new span are kept (invisible).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting `inPoint`/`outPoint`. Trimming never alters the footage item.

### AE-PRJ-382 · Slip edit (滑动编辑)

- **What it does:** For a trimmed time-based layer, shifts which source frames play inside the fixed in/out span by dragging the pale slip-edit bar (or dragging the layer with the Pan Behind tool in the Timeline).
- **Exact semantics:**
  ```
  d = drag delta (frames)
  startTime += d                       // source content moves
  inPoint, outPoint unchanged          // clamped so the span stays inside the source
  for each keyframe of the layer:
      if selected: stays attached to the source (comp time += d)
      else:        keeps its comp time  => layerTime -= d / k
  ```
  - Only for layers with a time-based source (no slip bar for stills/solids/text/shapes). Shift+F2 deselects keyframes but keeps the layer selected, so a slip leaves all keyframes in place.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Help: "Dragging the slip edit bar moves selected keyframes, but does not move unselected keyframes." Precompose keyframes behave the same.

### AE-PRJ-383 · Split Layer (拆分图层)

- **What it does:** Edit > Split Layer (Ctrl+Shift+D / Cmd+Shift+D) cuts selected layers at the CTI into two independent layers.
- **Exact semantics:**
  ```
  targets = selected layers, or ALL layers if none selected (locked layers skipped UNVERIFIED)
  for each L in targets with L.inPoint < CTI < L.outPoint:
      R = full copy of L (properties, keyframes, expressions, effects, masks, markers, switches, parent, matte link)
      L.outPoint = CTI ; R.inPoint = CTI
      position R: directly above L if Preferences > Composition "Create Split Layers Above Original Layer" (default on UNVERIFIED), else directly below
  ```
  - Both layers keep ALL keyframes at their original times (help).
  - Track mattes: the split layer keeps using the same matte layer; in legacy (pre-23) implicit mattes, AE also duplicates the matte so each piece keeps its matte directly above ("Any applied track mattes retain their order on top of the layer").
  - Naming: the new layer gets the original name with a number appended/incremented ("Title" → "Title 2") UNVERIFIED; unnamed footage layers stay unnamed.
  - Selection afterwards: the new layers are selected UNVERIFIED.
  - Children parented to L stay parented to L (the first piece) UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** One undo step. Help: "If no layer is selected when you select Edit > Split Layer, all layers are split at the current time."

### AE-PRJ-384 · Duplicate layer (重复图层)

- **What it does:** Edit > Duplicate (Ctrl+D / Cmd+D) copies selected layers in place, each copy directly above its original.
- **Exact semantics:**
  - Copies everything: properties, keyframes, expressions, effects, masks, layer styles, markers, switches, label, comment, parent link, timing. New layer ID.
  - Stacking: each copy is inserted directly above its original; the copies become selected and the originals are deselected.
  - Naming: if the layer has a set name ending in a number, the copy takes the next free number in the comp ("Shape Layer 1" → "Shape Layer 2" or higher if taken); a set name without number gets " 2" appended; unset names stay unset (copy shows `[source]`).
  - Track mattes (AE 23.0+ explicit links): a copy of a matted layer points to the same matte layer; if the matte layer is duplicated in the same operation, the copy points to the matte copy. Legacy implicit mattes: duplicating preserves "the relative ordering of the layer and its track matte" (copies of the pair are placed together).
  - Parent links: copy keeps the same parent; if the parent was duplicated in the same operation, copies link to the parent copy UNVERIFIED.
  - Expressions are copied verbatim: references by name resolve to the original targets (e.g. `thisComp.layer("Null 1")`); `thisLayer` refers to the copy.
  - Alt/Opt-drag of a layer in the Composition panel also duplicates (UNVERIFIED: a preference choosing between duplicating layers and camera navigation for Alt-drag in recent versions).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting `Layer.duplicate()`. Exact name-increment algorithm UNVERIFIED (AE scans existing names).

### AE-PRJ-385 · Copy, cut and paste layers (复制、剪切、粘贴图层)

- **What it does:** Clipboard transfer of whole layers within or between comps.
- **Exact semantics:**
  - Copy (Ctrl+C) with layers selected and no property/keyframe selected copies whole layers; if a mask, effect or keyframe is selected only that component is copied (Shift+F2 deselects components first).
  - Paste (Ctrl+V): pasted layers keep their original startTime/in/out (comp times). Placement: directly above the topmost selected layer in the target comp; with no selection, at the top of the stack. Pasted order: top-to-bottom in the order the layers were selected before copying (first selected ends up on top).
  - Paste with Ctrl+Alt+V / Cmd+Opt+V ("Paste layers at current time" in the shortcut reference): same, but the pasted layers are moved in time so their in points are at the CTI; UNVERIFIED: whether several pasted layers keep their relative offsets (earliest in point at CTI) or all start at the CTI.
  - Cut (Ctrl+X) = copy + delete.
  - Cross-comp paste: sources are referenced (no new items); parent links to layers not pasted are dropped unless the target comp has the same layer? UNVERIFIED (AE keeps parent only when pasting into the same comp); track matte links to non-pasted layers are dropped (AE 23+) UNVERIFIED; expressions are copied verbatim and may break.
  - Cross-project / to text editors: AE puts a proprietary clipboard format; pasting a layer into a text editor yields nothing useful (transform keyframes paste as tab-separated "Adobe After Effects Keyframe Data", ani-a).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision: the BAE clipboard carries a JSON fragment of Prims so layers can be pasted between BAE instances and to Agents.

### AE-PRJ-386 · Delete layers (删除图层)

- **What it does:** Removes selected layers (Delete / Backspace, Edit > Clear).
- **Exact semantics:**
  - Removes the layers and everything they own. Sources remain in the project.
  - Children of a deleted layer: parent becomes None; their world transform at the CTI is preserved by rewriting local values (same as unparenting, AE-PRJ-397) UNVERIFIED (AE may instead let children jump).
  - Layers using a deleted layer as track matte lose the matte (`trackMatteType` → none).
  - Effect LAYER_INDEX parameters pointing to it become None.
  - Expressions referencing it by name fail and show an expression error (they are not rewritten).
  - Locked layers cannot be deleted.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting `Layer.remove()`.

### AE-PRJ-387 · Overlay Edit and Ripple Insert Edit from the Footage panel (叠加编辑、波纹插入编辑)

- **What it does:** Inserts a trimmed footage clip (in/out set in the Footage panel) into the active comp at the CTI.
- **Exact semantics:**
  - Overlay Edit: new layer at the top of the stack, `inPoint = CTI`, trimmed to the Footage panel in/out.
  - Ripple Insert Edit: same, and every other layer that spans the CTI is split at the CTI; all split-off parts and layers starting at/after the CTI are moved later by the inserted duration so their in points equal the inserted layer's out point.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Footage panel UI: ui-preview-render-io.md [ui-a].

### AE-PRJ-388 · New layer placement and new-layer preferences (新图层的位置与首选项)

- **What it does:** Rules for where new layers go in the stack and in time, and the preferences that change them.
- **Exact semantics:**
  - Stack position: when one or more layers are selected, a new layer (from Layer > New, drag-and-drop to the comp name/icon, Ctrl+/) is inserted directly above the topmost selected layer; otherwise at index 1. Dragging into the Timeline layer outline inserts where the highlight bar shows; dragging into the Composition panel inserts above the selected layer or at the top.
  - Time: `startTime = inPoint = 0` when "Create Layers at Composition Start Time" (在合成开始时间创建图层) is on (default on; Preferences > Composition in AE 2026, formerly General); otherwise CTI. Dragging into the time graph sets the in point at the drop time (Shift snaps to CTI).
  - Space: centred in the comp (Position = comp centre) unless dropped in the Composition panel at a point.
  - Other preferences affecting new layers: "Center Anchor Point in New Shape Layers" (在新形状图层上居中放置锚点) (shape layers drawn with tools get their anchor at the shape's centre; default on UNVERIFIED, AE 17.0+), "Default Spatial Interpolation to Linear", "Default Position Properties to Separated Dimensions" (ani-a), Still Footage duration (Preferences > Import), label defaults (AE-PRJ-393).
  - Multiple items dropped at once: order follows Project-panel selection order (first selected on top).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Help: "New layers are created immediately above a selected layer and at the center of the composition. If no layer is selected, then new layers are created at the top of the layer stack." zh-CN preference strings（译名待核）.

### AE-PRJ-389 · Fit to Comp and centring commands (适合复合 / 居中)（译名待核）

- **What it does:** Layer > Transform commands that size or centre layers relative to the comp.
- **Exact semantics:**
  | Command | Shortcut | Effect |
  |---|---|---|
  | Fit to Comp (适合复合) | Ctrl+Alt+F / Cmd+Opt+F | Scale (non-uniform allowed) so the layer's source rect fills the comp frame exactly; Position = comp centre; Anchor unchanged at source centre UNVERIFIED (AE sets anchor to centre?) |
  | Fit to Comp Width | Ctrl+Alt+Shift+H / Cmd+Opt+Shift+H (preserves each layer's aspect ratio) | uniform scale so layer width = comp width |
  | Fit to Comp Height | Ctrl+Alt+Shift+G / Cmd+Opt+Shift+G (preserves aspect ratio) | uniform scale so layer height = comp height |
  | Center Anchor Point in Layer Content (在图层内容中居中放置锚点) | Ctrl+Alt+Home | Anchor = centre of `sourceRectAtTime(CTI)` (visible content, AE 17.0+), Position adjusted so the layer does not move |
  | Center In View (在视图中居中) | Ctrl+Home | Position so that the anchor point lands at the centre of the current Composition view |
  - Each applies to all selected layers; when Scale/Position are keyframed, a keyframe is set at CTI.
  - Parented layers: computations happen in the parent's space UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Transform semantics: transform-keyframes-expressions.md [ani-a].

## Stacking order

### AE-PRJ-390 · Layer stacking order and render order (图层堆积顺序)

- **What it does:** The order of layers in the Timeline defines compositing order: index 1 is the top (frontmost) layer; layers are composited from the bottom (highest index) upward.
- **Exact semantics:**
  ```
  for i in numLayers down to 1:          // bottom to top
      composite(layer[i]) over accumulated result   // using blend mode, matte, adjustment rules (cmp-a)
  ```
  - 2D layers follow index order strictly. Consecutive runs of 3D layers are depth-sorted by the 3D renderer instead; a 2D layer between 3D layers splits them into separate 3D groups (cmp-b "2D/3D grouping").
  - Index also defines the implicit track matte (legacy: layer above), the adjustment layer scope (all layers below), and Preserve Underlying Transparency's "underlying" composite.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Lottie: `layers[0]` is the TOP layer (same as AE index 1); players draw the array in reverse. Render order details: compositing-3d-color.md [cmp-a] "render order".

### AE-PRJ-391 · Reorder layers: Arrange commands and drag (排列图层)

- **What it does:** Changes layer indices.
- **Exact semantics:**
  | Command (中文) | Win / Mac shortcut | Effect on the selected set S |
  |---|---|---|
  | Bring Layer Forward (将图层前移一层) | Ctrl+Alt+↑ / Cmd+Opt+↑ | each layer in S moves up one index (blocks of adjacent selected layers move together; a layer already at 1 stays) |
  | Send Layer Backward (将图层后移一层) | Ctrl+Alt+↓ / Cmd+Opt+↓ | each moves down one index |
  | Bring Layer to Front (将图层置于顶层) | Ctrl+Alt+Shift+↑ / Cmd+Opt+Shift+↑ | S moved to indices 1..|S| keeping their relative order |
  | Send Layer to Back (将图层置于底层) | Ctrl+Alt+Shift+↓ / Cmd+Opt+Shift+↓ | S moved to the bottom keeping relative order |
  - Drag in the layer outline: the highlight bar shows the insertion gap; all selected layers move there keeping relative order.
  - Cut + Paste reorders in selection order (AE-PRJ-385).
  - Reordering never changes parent links, explicit track matte links (AE 23+) or layer IDs; it does change `index` and any expressions that use `thisComp.layer(index)` or `index - 1` semantics.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Layer > Arrange menu (排列). Scripting `moveBefore`, `moveAfter`, `moveToBeginning`, `moveToEnd`. zh-CN strings（译名待核）.

## Labels

### AE-PRJ-392 · Label colours (标签颜色)

- **What it does:** Sixteen named colour labels (plus None) that can be assigned to project items, layers, keyframes, markers and effects for organisation.
- **Exact semantics:** Preferences > Labels > Label Colors; index stored is 0..16 (0 = None). Factory names and colours (values as stored in AE preference files since CC 2014 and widely published; not documented by Adobe help, UNVERIFIED against an AE 2026 install):
  | Index | Name (中文)（译名待核） | Default sRGB |
  |---|---|---|
  | 0 | None (无) | — (no colour) |
  | 1 | Red (红色) | #B53838 |
  | 2 | Yellow (黄色) | #E4D84C |
  | 3 | Aqua (浅绿色) | #A9CBC7 |
  | 4 | Pink (粉红色) | #E5BCC9 |
  | 5 | Lavender (淡紫色) | #A9A9CA |
  | 6 | Peach (桃红色) | #E7C19E |
  | 7 | Sea Foam (海泡色) | #B3C7B3 |
  | 8 | Blue (蓝色) | #677DE0 |
  | 9 | Green (绿色) | #4AA44C |
  | 10 | Purple (紫色) | #8E2C9A |
  | 11 | Orange (橙色) | #E8920D |
  | 12 | Brown (棕色) | #7F452A |
  | 13 | Fuchsia (紫红色) | #F46DD6 |
  | 14 | Cyan (青色) | #3DA2A5 |
  | 15 | Sandstone (砂岩) | #A89677 |
  | 16 | Dark Green (深绿色) | #1E401E |
  - Names and colours are user-editable in Preferences (application-wide, not per project); the project stores only indices, so a project shows different colours on machines with different prefs.
  - Label colours also tint: layer bars in the Timeline, layer handles/paths in the viewer (preference "Use Label Color for Layer Handles and Paths"), viewer tabs ("Use Label Color for Related Tabs"), and mask colours ("Cycle Mask Colors (uses label colors)").
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: label attributes are integers 0..16; custom colours cannot be set from scripts. BAE decision: store the label palette in the project (with user override) so collaborators see the same colours; still store indices for AE interop.

### AE-PRJ-393 · Default labels by item and layer type (标签默认值)

- **What it does:** Preferences > Labels > Label Defaults assigns a label automatically to new items and layers by type.
- **Exact semantics:** Categories (AE 2026 order) with factory defaults UNVERIFIED (values from AE CC defaults):
  | Category (中文) | Default label |
  |---|---|
  | Composition (合成) | Sandstone (15) |
  | Video (视频) | Aqua (3) |
  | Audio (音频) | Sea Foam (7) |
  | Still (静止图像) | Lavender (5) |
  | Folder (文件夹) | Yellow (2) |
  | Solid (纯色) | Red (1) |
  | Camera (摄像机) | Pink (4) |
  | Light (灯光) | Pink (4) |
  | Adjustment (调整) | Purple (10) |
  | Null Object (空对象) | Red (1) |
  | Shape (形状) | Blue (8) |
  | Text (文本) | Red (1) |
  | 3D Models (3D 模型) | UNVERIFIED |
  | Other 3D (其他 3D) | UNVERIFIED |
  - A footage/precomp layer takes the label of its source item at creation time (not linked afterwards).
  - Changing a default affects only items/layers created afterwards.
- **Priority:** P1
- **Owner:** bae-model
- **Notes:** "3D Models" and "Other 3D" categories exist in AE 2024+.

### AE-PRJ-394 · Assigning labels and Select Label Group (指定标签、选择标签组)

- **What it does:** Commands to set the label of selected items/layers and to select everything with the same label.
- **Exact semantics:**
  - Click the label swatch in the Label column (Timeline or Project panel) → menu of the 16 colours + None + Select Label Group; or Edit > Label > <colour>. Applies to all selected layers/items.
  - Edit > Label > Select Label Group (选择标签组): selects all layers in the comp (or all items in the Project panel, depending on the active panel) that have the same label as the selected one(s) (union over the labels of the current selection UNVERIFIED).
  - Recolouring a whole group = Select Label Group then pick a new label.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion, bae-model)
- **Notes:** Scripting `Layer.label`, `Item.label`.

### AE-PRJ-395 · Keyframe, marker and effect labels (关键帧、标记和效果的颜色标签)

- **What it does:** Colour labels on individual keyframes (AE 22.6, 2022), on comp/layer markers (AE 16.0), and on effect instances in Effect Controls (AE 26.5, September 2026, per Adobe "What's new").
- **Exact semantics:**
  - Keyframe label: per-keyframe int 0..16, default 0; set via keyframe context menu > Label; shown as keyframe icon colour; Select Label Group works on keyframes UNVERIFIED. Scripting `Property.keyLabel(i)`, `setLabelAtKey(i, n)`.
  - Marker label: AE-PRJ-325 field.
  - Effect label: per-effect int 0..16, default 0 (grey swatch = no label); shown as a coloured square swatch next to the effect name in Effect Controls, clicking it opens the label picker; optionally tints the effect row background; scripting attribute name UNVERIFIED.
  - Labels never affect rendering. Copy/paste of keyframes carries their labels UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Versions from docsforadobe (`keyLabel` added 22.6; MarkerValue.label 16.0) and What's New (Color labels for effects in AE 26.5).

## Parenting and property links

### AE-PRJ-396 · Parent relationship and what it inherits (父级)

- **What it does:** A child layer's transform becomes relative to its parent layer's transform, so moving, rotating or scaling the parent carries the child.
- **Exact semantics:**
  - Data: `parent: Option<LayerId>` (one parent per layer; any number of children). Not animatable (cannot change over time).
  - Inherited: the parent's complete layer transform (Anchor Point, Position, Scale incl. negative values, Rotation / X/Y/Z Rotation, Orientation, and Auto-Orient rotation), recursively up the chain:
    ```
    worldMatrix(child, t) = worldMatrix(parent, t) * localMatrix(child, t)
    ```
    (matrix definition and order: transform-keyframes-expressions.md [ani-a] "parenting/auto-orient math").
  - NOT inherited: Opacity, effects, masks, layer styles, blend mode, track matte, quality/switches, Video/solo visibility, time (stretch, remap, in/out), and 3D-ness (a 2D child of a 3D parent stays 2D; a 3D child of a 2D parent is 3D with the parent treated as z = 0) UNVERIFIED for the 2D/3D mixing details (ani-a).
  - The parent's transform is evaluated at comp time t even outside its in/out span and when its Video switch is off.
  - Non-uniform parent scale combined with child rotation produces shear in the child (matrix product; no decomposition).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Help: parenting affects all transform properties but not Opacity. Scripting `Layer.parent`. Lottie `parent` = parent's `ind`. Expressions: `parent`, `hasParent`, `toWorld()` (ani-b).

### AE-PRJ-397 · Assigning and removing parents: normal, Alt/Option (jump), Shift (move to parent) (指定和移除父级)

- **What it does:** Parent & Link column pick whip / menu sets or clears a parent, with modifiers controlling whether the child stays in place.
- **Exact semantics:**
  | Action | Child result |
  |---|---|
  | Pick-whip or choose parent from the Parent & Link menu (no modifier) | child keeps its current world appearance: its local Position, Scale, Rotation (and Orientation / X/Y Rotation for 3D) are rewritten so that `world(t_cti)` is unchanged |
  | Alt/Opt + assign | child keeps its local values; they are now interpreted in parent space, so the child "jumps" |
  | Shift + assign | child is moved to the parent's location: child Position set so the child's anchor coincides with the parent's anchor point (UNVERIFIED: Rotation/Scale unchanged) |
  | Choose None, or Ctrl/Cmd-click the pick whip | unparent; local values rewritten so the world appearance is unchanged |
  | Alt/Opt + None, or Alt+Ctrl / Opt+Cmd-click the pick whip | unparent keeping local values (child jumps) |
  - Rewriting with keyframes: the same inverse parent matrix (parent world matrix evaluated at the CTI) is applied to every keyframe value of the rewritten properties and to the static value UNVERIFIED (AE applies the CTI-time correction to all keys; it does not bake per-key parent motion). Expressions on rewritten properties are not changed UNVERIFIED.
  - Rewrite decomposition: Position = inverse(parentWorld) · childWorldPosition; Rotation -= parent world rotation; Scale /= parent world scale (component-wise) — exact only without shear; with shear AE's result is approximate UNVERIFIED.
  - Selecting several layers and pick-whipping one of them parents all selected layers.
  - Layer context menu > Select Children (选择子项): adds all descendants (recursive UNVERIFIED) of the selected layer to the selection.
  - Scripting: `layer.parent = p` behaves like the no-modifier assignment (keeps world appearance); `layer.setParentWithJump(p)` behaves like Alt.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Help text in "Layer properties > Parent and child layers". Info panel shows a hint about modifiers while dragging. zh-CN pick whip = 关联器; Parent & Link = 父级和链接.

### AE-PRJ-398 · Parenting constraints (父级约束)

- **What it does:** Which layers may be parents of which.
- **Exact semantics:**
  - Parent must be in the same comp. Cycles are impossible: the Parent menu and pick whip exclude the layer itself and all its descendants.
  - Any layer with a Transform group can be a parent or child: AV layers, nulls, text, shape, cameras, lights (incl. a camera parented to a null). Audio-only layers and ambient lights cannot be parents or children UNVERIFIED.
  - 2D and 3D layers can be mixed in either direction (ani-a defines how 3D parent transforms apply to a 2D child).
  - Parents with negative scale mirror their children (allowed).
  - Deleting a parent: AE-PRJ-386. Duplicating/Precomposing: AE-PRJ-384, AE-PRJ-401.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Command validation must reject cycles (including those created by collaborators concurrently: BAE decision: on CRDT merge, a cycle is broken by clearing the parent link with the latest timestamp).

### AE-PRJ-399 · Property pick whip and expression links (属性关联器、表达式链接)

- **What it does:** Links one property to another by generating an expression, via the property pick whip or Alt-drag.
- **Exact semantics:**
  - Each property with an expression or in "Parent & Link" mode shows a pick whip (spiral icon). Dragging it from property A onto property B (in the same or another comp, in the Timeline, Effect Controls or Composition panel) sets A's expression to a reference to B, e.g. `thisComp.layer("Null 1").transform.position` (same comp) or `comp("Main").layer("Null 1").effect("Slider Control")("Slider")`.
  - Dimension mismatch: AE adapts automatically, e.g. linking 1D to a 2D property produces `temp = …; [temp, temp]`; linking 2D to 1D uses `[0]` of the source.
  - Alt/Opt-click the stopwatch opens the expression editor; Alt-dragging a property's pick whip onto a layer in the Timeline sets the expression to that layer reference UNVERIFIED.
  - Since AE 15.0 (CC 2018) the pick whip can target properties in other comps and in the Effect Controls panel of other layers.
  - Expression text references layers by name; renaming updates them (AE-PRJ-353).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-ui-motion)
- **Notes:** Expression language: transform-keyframes-expressions.md [ani-b]. BAE decision: keep a structured "link" record next to the generated expression text so links survive renames and can be visualised.

### AE-PRJ-400 · Copy With Property Links / Copy With Relative Property Links (复制并保留属性链接)（译名待核）

- **What it does:** Edit > Copy With Property Links copies properties (or whole layers) so that pasting creates expressions linking each pasted property back to its source.
- **Exact semantics:**
  - Copy With Property Links (AE 15.0): pasted properties get expressions referencing the source by absolute path including the comp: `comp("Main").layer("A").transform.opacity`.
  - Copy With Relative Property Links (AE 16.0 UNVERIFIED): references use `thisComp`, so pasting into a duplicated comp links within that comp.
  - When whole layers are copied this way, every property that has a value (not only modified ones UNVERIFIED) is linked; effects are copied with their parameters linked.
  - Paste puts the links on the selected target layer or creates new layers as normal paste does.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Common for building rigs and Essential Graphics masters.

## Precompose and nesting

### AE-PRJ-401 · Precompose (预合成)

- **What it does:** Layer > Pre-compose (Ctrl+Shift+C / Cmd+Shift+C) replaces the selected layers with one precomp layer whose source is a new comp containing them.
- **Exact semantics:** Dialog fields:
  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | New composition name (新合成名称) | string | single layer: "<layer name> Comp N" UNVERIFIED; several: "Pre-comp N" (预合成 N) | |
  | Leave all attributes in "<comp>" (保留"<合成>"中的所有属性) | radio | selected when available UNVERIFIED | not available when more than one layer, a text layer or a shape layer is selected (help); UNVERIFIED: camera, light, null, adjustment |
  | Move all attributes into the new composition (将所有属性移动到新合成) | radio | forced when Leave is unavailable | |
  | Adjust composition duration to the time span of the selected layers (将合成持续时间调整为所选图层的时间跨度) | bool | false UNVERIFIED | only with Move all; newer AE (UNVERIFIED version, ≈ AE 2020+) |
  | Open New Composition (打开新合成) | bool | false UNVERIFIED | opens the new comp in Timeline/viewer |
  **Leave all attributes** (single AV layer L):
  ```
  new comp C: width,height = L.source size (layer size); PAR, fps = original comp's UNVERIFIED (fps: source's?); duration = original comp duration UNVERIFIED
  inside C: one layer referencing L.source with DEFAULT transforms, no effects/masks/keyframes, timing UNVERIFIED (startTime = L.startTime so frames align)
  L.source = C   // L keeps name? (layer name becomes C's name if L had no set name), transforms, masks, effects, styles, keyframes, expressions, switches, matte, parent, timing
  ```
  **Move all attributes:**
  ```
  new comp C: width,height,PAR,fps,bgColor? = original comp's; duration = original comp duration
  move each selected layer (with everything it owns) into C keeping its index order, timing and transforms
  insert one precomp layer P at the index of the topmost selected layer; P: startTime 0, in 0, out = C.duration, default transforms (anchor = C centre, position = comp centre), label = C's label
  if "Adjust duration" checked:
      s = min(L.inPoint), e = max(L.outPoint) over selected; C.duration = e - s
      every moved layer startTime -= s ; P.startTime = s ; P.in = s ; P.out = e
  ```
  - New comp item goes into the same Project folder as the original comp UNVERIFIED.
  - Undo is one step.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Help: Leave keeps properties on the new precomp layer and the new comp has the layer's frame size; Move gives the new comp the original comp's frame size; Leave is unavailable for multiple layers, text or shape layers. zh-CN dialog strings（译名待核）.

### AE-PRJ-402 · Precompose: fate of links, mattes, parents, expressions and markers (预合成后的关系处理)

- **What it does:** Defines what happens to cross-layer relationships when layers move into a new comp.
- **Exact semantics:**
  | Relationship | Result (Move all attributes) |
  |---|---|
  | Parent inside the moved set | kept |
  | Moved child whose parent stays outside | parent link removed; the child keeps its local values UNVERIFIED (so it jumps) — BAE should bake world transform at CTI instead and warn |
  | Layer outside whose parent is moved inside | parent link removed (cross-comp parents are impossible) UNVERIFIED |
  | Track matte: both moved | kept |
  | Track matte: matted layer moved, matte not (AE 23+) | link removed; legacy implicit matte: the layer loses its matte |
  | Track matte: matte moved, matted layer stays | matted layer loses its matte UNVERIFIED |
  | Expressions inside moved layers using `thisComp.layer("X")` where X stays outside | not rewritten; they now resolve in the new comp and error unless a same-named layer exists there |
  | Expressions outside referencing moved layers | not rewritten; error UNVERIFIED (AE 17+ may rewrite to `comp("Pre-comp 1").layer("X")` UNVERIFIED) |
  | Layer markers | move with their layers |
  | Comp markers of the original comp | stay; the new comp has none (so the precomp layer gets no markers) |
  | Effects with LAYER_INDEX params pointing across the boundary | set to None UNVERIFIED |
  | Effects, masks, styles of moved layers | move with them (Move) / stay on the precomp layer (Leave) |
  | Solo/shy/lock/label | move with layers; the precomp layer is unlocked, not solo, not shy |
  - With "Leave all attributes" nothing changes for links because the layer stays in place.
- **Priority:** P1 (P0 for the parent/matte rules, which can visibly break comps)
- **Owner:** bae-motion (+ bae-model)
- **Notes:** These rules are largely UNVERIFIED and must be confirmed against AE 26 before implementation; BAE should surface a summary of broken links after Precompose.

### AE-PRJ-403 · Composition nesting rules (合成嵌套规则)

- **What it does:** Constraints and inheritance rules for comps used as layers in other comps.
- **Exact semantics:**
  - No cycles: a comp cannot be added to itself or to any comp that it contains directly or indirectly; attempts are refused with an error ("cannot add a composition to itself" UNVERIFIED wording).
  - A comp may be nested any number of times in any number of comps; all instances share the same source (edits propagate).
  - Precomp layer size = nested comp width × height; the layer's time maps via startTime/stretch/time remap (AE-PRJ-373, AE-PRJ-376).
  - Inherited from the containing comp unless "Preserve …" options are set: resolution (AE-PRJ-312), frame rate sampling (AE-PRJ-311); background colour is NOT inherited (nested background transparent).
  - Preferences > Composition "Switches Affect Nested Comps" (开关影响嵌套的合成) (default on): the precomp layer's Quality, Motion Blur, Frame Blending and Wireframe/Draft switches propagate into the nested comp's layers UNVERIFIED (exact list in cmp-a). 3D renderer: a nested comp renders with its own renderer setting unless collapsed.
  - Preferences > Composition "Synchronize Time of All Related Items" (同步所有相关项目的时间) (default on): moving the CTI in one comp moves it in related comps (UI only).
  - Comp Navigator / Mini-Flowchart (Tab), Shift+Esc to the previous comp in the network: UI (ui-a).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Render semantics of nested comps: compositing-3d-color.md [cmp-a] "nested comp res/fps" and "collapse".

## Replacing sources

### AE-PRJ-404 · Replace layer source (替换图层源)

- **What it does:** Alt/Option-drag a project item onto selected layers in the Timeline (or Composition panel) to swap the source of those layers while keeping everything else.
- **Exact semantics:**
  - Keeps: layer name if set (unset → shows new source name), transforms (values unchanged, not rescaled), keyframes, expressions, effects, masks, styles, switches, timing (startTime, in, out, stretch), parent, matte, markers, label, comment.
  - Changes: `source`; layer width/height become the new source's; Anchor Point is NOT recentred (a different-size source appears offset) UNVERIFIED; in/out clamped to the new source's extent if it is time-based and shorter UNVERIFIED.
  - Allowed sources: any footage, solid or comp item, except a comp that would create a cycle. Text, shape, camera, light, null layers have no replaceable source.
  - Applies to every selected layer. Also via scripting `AVLayer.replaceSource(item, fixExpressions)`: with `fixExpressions = true`, expressions referencing the old source name are updated.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Help: "Alt-drag (Windows) or Option-drag (Mac OS) a footage item from the Project panel onto a selected layer in the Timeline".

### AE-PRJ-405 · Replace Footage for all usages (替换素材)

- **What it does:** File > Replace Footage > File (Ctrl+H / Cmd+H) relinks a footage item to another file, changing every layer that uses it.
- **Exact semantics:**
  - The item keeps its ID, name (UNVERIFIED: name updates to the new file name unless the item was renamed), label, folder and every usage; Interpret Footage settings are kept if compatible UNVERIFIED.
  - Other Replace Footage variants: With Layered Comp, Placeholder, Solid, With After Effects Composition UNVERIFIED list.
  - Alt-dragging an item onto another item in the Project panel replaces all usages of the target item with the dragged item (Project panel replace) UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Footage item relinking, missing footage and placeholders are owned by project-comp-layers.md [prj-a]; this item only states the per-layer effect: all layers referencing the item change together while keeping their own attributes (AE-PRJ-404 list).

## Sequencing layers in time

### AE-PRJ-406 · Sequence Layers keyframe assistant (序列图层)

- **What it does:** Animation > Keyframe Assistant > Sequence Layers arranges selected layers end to end in selection order, optionally overlapping them with dissolve transitions.
- **Exact semantics:** Dialog:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Overlap (重叠) | bool | false | — | — | |
  | Duration (持续时间) | time | 0:00:01:00 UNVERIFIED | any; negative = gap | timecode | used only when Overlap is on |
  | Transition (过渡) | enum | Dissolve Front Layer UNVERIFIED | Off (关), Dissolve Front Layer (溶解前景图层), Cross Dissolve Front and Back Layers (交叉溶解前景和背景图层)（译名待核） | — | used only when Overlap is on and Duration > 0 |
  ```
  L[1..n] = selected layers in SELECTION order (Ctrl/Cmd-click order); L[1] does not move
  o = overlap ? Duration : 0
  for i in 2..n:
      d = (L[i-1].outPoint - o) - L[i].inPoint
      move L[i] by d   // startTime/in/out/keyframes/markers shift (AE-PRJ-380); no trimming
  transitions (o > 0), for each i in 2..n, overlap window [a, b] = [L[i].inPoint, L[i-1].outPoint]:
      front = the one of L[i-1], L[i] that is higher in the stack (lower index)
      Dissolve Front Layer: Opacity keys on front only: if front is L[i-1]: (a,100%)->(b,0%) else (a,0%)->(b,100%)
      Cross Dissolve Front and Back: L[i-1]: (a,100%)->(b,0%) AND L[i]: (a,0%)->(b,100%)
      keys are linear; existing Opacity values are overwritten at those times UNVERIFIED (AE multiplies? no: sets absolute 0/100)
  ```
  - Layers need durations shorter than the comp to leave room (help); layers may extend past the comp end.
  - Stacking order is not changed.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Help confirms: first selected layer stays, order = selection order, negative Duration leaves gaps, "Cross Dissolve Front And Back Layers to use the transparency of the selected layers". The same options appear in New Comp from Selection (AE-PRJ-317). Opacity key exact values UNVERIFIED.

### AE-PRJ-407 · Quick Offset (快速偏移)

- **What it does:** Ctrl+Alt-drag (Win) / Cmd+Opt-drag (Mac) on selected layers (or keyframes across layers) staggers them evenly in time by selection order.
- **Exact semantics:**
  ```
  L[1..n] = selected layers in selection order; D = drag delta (frames)
  for i in 1..n: move L[i] by round(D * (i-1)/(n-1))   // L[1] fixed, L[n] moves D
  ```
  - Keyframes move with their layers. An overlay shows "Total Offset" and "Per Layer" in the Timeline's time display unit.
  - With keyframes selected across several layers, Ctrl+Alt / Cmd+Opt-drag staggers the keyframes across the layers in the same way (help); UNVERIFIED: the exact grouping when one layer has several selected keyframes (assumed: each layer's selected keys move as a group).
  - The overlay unit follows the Timeline time display (timecode or frames).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Recent feature (exact version UNVERIFIED); help page "Selecting and arranging layers > Quickly offset your layers and keyframes" (zh-CN help: 快速偏移). Rounding to whole frames UNVERIFIED.

## Auto-Orient

### AE-PRJ-408 · Auto-Orient setting (自动方向)

- **What it does:** Layer > Transform > Auto-Orient (Ctrl+Alt+O / Cmd+Opt+O) makes a layer rotate automatically to follow its motion path, face the camera, or aim at its point of interest.
- **Exact semantics:** `autoOrient` enum (scripting `AutoOrientType`), default Off for every layer type except two-node cameras and Spot/Parallel lights, which default to Orient Towards Point of Interest:
  | Option (中文) | Scripting | Available for | Lottie |
  |---|---|---|---|
  | Off (关) | `NO_AUTO_ORIENT` | all | `ao: 0` |
  | Orient Along Path (沿路径定向) | `ALONG_PATH` | all layers with Transform (2D: Z rotation; 3D: full orientation along the tangent) | `ao: 1` |
  | Orient Towards Camera (定位于摄像机) | `CAMERA_OR_POINT_OF_INTEREST` | 3D AV/text/shape layers (not cameras/lights) | — |
  | Orient Towards Point of Interest (定位于目标点) | `CAMERA_OR_POINT_OF_INTEREST` | cameras and lights (Spot, Parallel) | — (camera `a` used as POI) |
  | Independently orient each character towards camera (独立定向每个字符朝向摄像机)（译名待核） | `CHARACTERS_TOWARD_CAMERA` | text layers with per-character 3D | — |
  - Setting Off on a two-node camera turns it into a one-node camera behaviour (POI ignored; the Point of Interest property disappears) UNVERIFIED for property visibility.
  - Auto-orient rotation is added on top of the layer's Rotation/Orientation values (math: transform-keyframes-expressions.md [ani-a] "parenting/auto-orient math").
  - Children inherit the auto-oriented transform (AE-PRJ-396).
- **Priority:** P0 (Off/Along Path/Towards POI); P1 (Towards Camera); P2 (per-character).
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting `Layer.autoOrient`. Dialog menu labels change with layer type; zh-CN labels UNVERIFIED（译名待核）.

## Layer Styles

### AE-PRJ-409 · Layer Styles framework and render order (图层样式)

- **What it does:** Photoshop-compatible styles (shadows, glows, bevel, overlays, stroke) attached to a layer; they render after the layer's transform and blend directly with the layers below.
- **Exact semantics:**
  - Container: `ADBE Layer Styles` group (shown only after a style is added), children in fixed order: Blending Options (`ADBE Blend Options Group`), Drop Shadow (`dropShadow/…`), Inner Shadow (`innerShadow/…`), Outer Glow (`outerGlow/…`), Inner Glow (`innerGlow/…`), Bevel and Emboss (`bevelEmboss/…`), Satin (`chromeFX/…`), Color Overlay (`solidFill/…`), Gradient Overlay (`gradientFill/…`), Pattern Overlay (`patternFill/…`, import-only: cannot be added in AE), Stroke (`frameFX/…`). Each style has an enable eyeball (`<style>/enabled`).
  - Commands: Layer > Layer Styles > <style> adds it to the selected layers; Delete removes the selected style; Layer > Layer Styles > Remove All; Convert To Editable Styles (for PSD layers with merged styles); Copy/Paste styles.
  - Render order per layer: source → masks → effects → transform (incl. motion blur) → layer styles → blend with the composite below. Styles therefore follow the transformed layer edges (shadows do not rotate with the layer's angle setting).
  - Classification: interior styles (Inner Shadow, Inner Glow, Satin, Color Overlay, Gradient Overlay, Bevel and Emboss) affect the layer's opaque pixels; exterior styles (Drop Shadow, Outer Glow) interact only with the layers below; Stroke is both (position dependent).
  - Compositing stack, bottom to top (Photoshop order): Drop Shadow, Outer Glow, [layer fill × Fill Opacity, then interior styles: Satin? order per Photoshop: Color Overlay, Gradient Overlay, Pattern Overlay, Satin, Inner Glow, Inner Shadow, Bevel highlight/shadow], Stroke UNVERIFIED exact interior order; the whole result is then multiplied by layer Opacity.
  - On vector layers (text, shape, Illustrator footage) edge styles follow the vector outlines; on raster layers they follow the alpha edge.
  - 3D: a layer with a layer style cannot intersect other 3D layers or cast/receive shadows; 3D layers on either side of it cannot intersect each other (it splits the 3D group, like a 2D layer).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Help "Blending modes and layer styles". Lottie layer `sy[]` with `ty` per bodymovin layerStyleTypes: stroke 0, drop shadow 1, inner shadow 2, outer glow 3, inner glow 4, bevel and emboss 5, satin 6, color overlay 7, gradient overlay 8; lottie-web renders only a subset (none on canvas) UNVERIFIED. Defaults below follow Photoshop's style defaults and are UNVERIFIED for AE unless stated.

### AE-PRJ-410 · Drop Shadow style (投影)

- **What it does:** Casts an offset, blurred copy of the layer's alpha behind the layer.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Type | Default | Range | Units |
  |---|---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | enum | Multiply | blend mode list | — |
  | Color (颜色) | `color` | COLOR | black | — | — |
  | Opacity (不透明度) | `opacity` | OneD | 75 % UNVERIFIED | 0..100 | % |
  | Use Global Light (使用全局光) | `useGlobalAngle` | bool | On UNVERIFIED | — | — |
  | Angle (角度) | `localLightingAngle` | OneD | 120° | any | degrees (light direction; shadow offset opposite) |
  | Distance (距离) | `distance` | OneD | 5 | 0..30000 UNVERIFIED | px |
  | Spread (扩展) | `chokeMatte` | OneD | 0 % | 0..100 | % |
  | Size (大小) | `blur` | OneD | 5 | 0..250 UNVERIFIED | px |
  | Noise (杂色) | `noise` | OneD | 0 % | 0..100 | % |
  | Layer Knocks Out Drop Shadow (图层挖空投影) | `layerConceals` | bool | On | — | — |
  ```
  angle = useGlobal ? globalAngle : localAngle
  offset = distance * (-cos(angle), +sin(angle))   // light from angle; shadow opposite (y down) UNVERIFIED sign
  m = alpha(layer) shifted by offset; m = spread(m, spread%*size); m = blur(m, size)
  if layerKnocksOut: m *= (1 - alpha(layer))
  shadow = color with alpha m * opacity, blended with mode under the layer
  ```
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 1` keys `c, o, a, s, d, ch, bm, no, lc` (lottie-docs schema drop-shadow-style). Blur kernel per Photoshop (approximate Gaussian) UNVERIFIED.

### AE-PRJ-411 · Inner Shadow style (内阴影)

- **What it does:** Draws a shadow inside the layer's edges as if the layer were a hole.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Multiply | — | — |
  | Color (颜色) | `color` | black | — | — |
  | Opacity (不透明度) | `opacity` | 75 % | 0..100 | % |
  | Use Global Light (使用全局光) | `useGlobalAngle` | On | — | — |
  | Angle (角度) | `localLightingAngle` | 120° | — | degrees |
  | Distance (距离) | `distance` | 5 | 0..30000 | px |
  | Choke (阻塞) | `chokeMatte` | 0 % | 0..100 | % |
  | Size (大小) | `blur` | 5 | 0..250 | px |
  | Noise (杂色) | `noise` | 0 % | 0..100 | % |
  - Mask = inverted layer alpha, offset, choked, blurred, then multiplied by layer alpha (only inside). Defaults UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 2`.

### AE-PRJ-412 · Outer Glow style (外发光)

- **What it does:** Adds a glow around the outside of the layer's edges.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Screen | — | — |
  | Opacity (不透明度) | `opacity` | 75 % | 0..100 | % |
  | Noise (杂色) | `noise` | 0 % | 0..100 | % |
  | Color Type (颜色类型) | `AEColorChoice` | Color (颜色) | Color, Gradient (渐变) | — |
  | Color (颜色) | `color` | RGB(255, 255, 190) UNVERIFIED | — | — |
  | Colors (颜色) | `gradient` | gradient (colour → transparent) | — | CUSTOM_VALUE gradient |
  | Gradient Smoothness (渐变平滑度) | `gradientSmoothness` | 100 % | 0..100 | % |
  | Technique (方法) | `glowTechnique` | Softer (柔和) | Softer, Precise (精确) | — |
  | Spread (扩展) | `chokeMatte` | 0 % | 0..100 | % |
  | Size (大小) | `blur` | 5 | 0..250 | px |
  | Range (范围) | `inputRange` | 50 % | 1..100 | % |
  | Jitter (抖动) | `shadingNoise` | 0 % | 0..100 | % |
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 3`. Defaults UNVERIFIED (Photoshop defaults).

### AE-PRJ-413 · Inner Glow style (内发光)

- **What it does:** Adds a glow inside the layer from its edges or centre.
- **Exact semantics:** Same parameters as Outer Glow (AE-PRJ-412) with Choke (阻塞, `chokeMatte`) instead of Spread, plus:
  | Parameter (中文) | matchName suffix | Default | Values |
  |---|---|---|---|
  | Source (源) | `innerGlowSource` | Edge (边缘) | Center (居中), Edge (边缘) |
  - Default Blend Mode Screen, Opacity 75 %, Size 5, Range 50 % UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 4`.

### AE-PRJ-414 · Bevel and Emboss style (斜面和浮雕)

- **What it does:** Adds highlight and shadow shading so the layer looks raised or engraved.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range / values | Units |
  |---|---|---|---|---|
  | Style (样式) | `bevelStyle` | Inner Bevel (内斜面) | Outer Bevel (外斜面), Inner Bevel, Emboss (浮雕效果), Pillow Emboss (枕状浮雕), Stroke Emboss (描边浮雕)（译名待核） | — |
  | Technique (方法) | `bevelTechnique` | Smooth (平滑) | Smooth, Chisel Hard (雕刻清晰), Chisel Soft (雕刻柔和) | — |
  | Depth (深度) | `strengthRatio` | 100 % | 1..1000 | % |
  | Direction (方向) | `bevelDirection` | Up (上) | Up, Down (下) | — |
  | Size (大小) | `blur` | 5 | 0..250 | px |
  | Soften (柔化) | `softness` | 0 | 0..16 | px |
  | Use Global Light (使用全局光) | `useGlobalAngle` | On | — | — |
  | Angle (角度) | `localLightingAngle` | 120° | — | degrees |
  | Altitude (高度) | `localLightingAltitude` | 30° | 0..90 | degrees |
  | Highlight Mode (高光模式) | `highlightMode` | Screen | blend modes | — |
  | Highlight Color (高光颜色) | `highlightColor` | white | — | — |
  | Highlight Opacity (高光不透明度) | `highlightOpacity` | 75 % | 0..100 | % |
  | Shadow Mode (阴影模式) | `shadowMode` | Multiply | blend modes | — |
  | Shadow Color (阴影颜色) | `shadowColor` | black | — | — |
  | Shadow Opacity (阴影不透明度) | `shadowOpacity` | 75 % | 0..100 | % |
  - Gloss contour, contour and texture sub-options of Photoshop are not available in AE (preserved only from PSD UNVERIFIED). Defaults UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 5`. Help recommends it over the Bevel Alpha effect when highlight and shadow need different blend modes.

### AE-PRJ-415 · Satin style (光泽)

- **What it does:** Applies interior shading that creates a satiny finish from two offset, blurred copies of the layer's shape.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Multiply | — | — |
  | Color (颜色) | `color` | black | — | — |
  | Opacity (不透明度) | `opacity` | 50 % | 0..100 | % |
  | Angle (角度) | `localLightingAngle` | 19° | — | degrees |
  | Distance (距离) | `distance` | 11 | 1..250 | px |
  | Size (大小) | `blur` | 14 | 0..250 | px |
  | Invert (反转) | `invert` | On | — | — |
  ```
  a = alpha shifted by +offset, b = alpha shifted by -offset (offset from angle, distance)
  m = blur(|a - b|, size); if invert: m = 1 - m; m *= alpha(layer)
  ```
  - Defaults and formula UNVERIFIED (Photoshop behaviour).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 6`. Satin does not use Global Light.

### AE-PRJ-416 · Color Overlay style (颜色叠加)

- **What it does:** Fills the layer's opaque pixels with a solid colour.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Normal | — | — |
  | Color (颜色) | `color` | red RGB(255, 0, 0) UNVERIFIED | — | — |
  | Opacity (不透明度) | `opacity` | 100 % | 0..100 | % |
  - `result = blend(mode, layerRGB, color) * opacity` inside the layer alpha.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 7`.

### AE-PRJ-417 · Gradient Overlay style (渐变叠加)

- **What it does:** Fills the layer's opaque pixels with a gradient.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range / values | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Normal | — | — |
  | Opacity (不透明度) | `opacity` | 100 % | 0..100 | % |
  | Colors (颜色) | `gradient` | black → white UNVERIFIED | gradient editor | — |
  | Gradient Smoothness (渐变平滑度) | `gradientSmoothness` | 100 % | 0..100 | % |
  | Angle (角度) | `angle` | 90° | — | degrees |
  | Style (样式) | `type` | Linear (线性) | Linear, Radial (径向), Angle (角度), Reflected (对称的), Diamond (菱形)（译名待核） | — |
  | Reverse (反向) | `reverse` | Off | — | — |
  | Align with Layer (与图层对齐) | `align` | On | — | — |
  | Scale (缩放) | `scale` | 100 % | 10..150 | % |
  | Offset (偏移) | `offset` | [0, 0] | — | px |
  - Align with Layer On: gradient spans the layer's bounding box; Off: spans the comp frame UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 8`.

### AE-PRJ-418 · Stroke style (描边)

- **What it does:** Outlines the layer's alpha edge with a solid colour.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range / values | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Normal | — | — |
  | Color (颜色) | `color` | red RGB(255, 0, 0) UNVERIFIED | — | — |
  | Size (大小) | `size` | 3 | 1..250 | px |
  | Opacity (不透明度) | `opacity` | 100 % | 0..100 | % |
  | Position (位置) | `style` | Outside (外部) | Outside, Inside (内部), Center (居中) | — |
  - Photoshop fill types Gradient/Pattern for strokes are not available in AE.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 0`. Distinct from the shape-layer Stroke (shapes.md) and the Stroke effect (effects.md).

### AE-PRJ-419 · Layer style Blending Options (混合选项)

- **What it does:** Per-layer options that control how the styles and the layer content blend: global light, fill opacity, channel exclusion, interior-group blending and blend ranges.
- **Exact semantics:**
  | Parameter (中文) | matchName | Type | Default | Range | Notes |
  |---|---|---|---|---|---|
  | Global Light Angle (全局光角度) | `ADBE Global Angle2` | OneD | 120° | — | used by styles with Use Global Light on |
  | Global Light Altitude (全局光高度) | `ADBE Global Altitude2` | OneD | 30° | 0..90 | Bevel and Emboss |
  | Advanced Blending (高级混合) | `ADBE Adv Blend Group` | group | — | — | |
  | Fill Opacity (填充不透明度) | `ADBE Layer Fill Opacity2` | OneD % | 100 | 0..100 | scales layer content (and interior styles UNVERIFIED) but NOT exterior styles; layer Opacity scales everything |
  | Red / Green / Blue (红色 / 绿色 / 蓝色) | `ADBE R/G/B Channel Blend` | bool (OneD 0/1) | On | — | Off: that channel of the result is taken from the underlying composite (channel excluded from blending); animatable |
  | Blend Interior Styles as Group (将内部样式混合成组) | `ADBE Blend Interior` | bool | Off | — | On: interior styles are blended with the layer first and the group uses the layer's blending mode |
  | Use Blend Ranges from Source (使用源的混合范围) | `ADBE Blend Ranges` | bool | Off | — | applies the "Blend If" ranges stored in the source PSD |
  - Knockout (挖空, None/Shallow/Deep in Photoshop) is not exposed in AE: the docsforadobe Blending Options matchName list has exactly Global Light Angle, Global Light Altitude, Advanced Blending > Fill Opacity, Red, Green, Blue, Blend Interior Styles as Group, Use Blend Ranges from Source.
  - The Blending Options group exists only after a style has been added (adding then deleting a style leaves it).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Matchnames from docsforadobe Layer Styles page. Lottie: blending options are not exported UNVERIFIED.

## Essential Properties

### AE-PRJ-420 · Essential Properties on precomp layers (基本属性)

- **What it does:** Properties of a nested comp that are added to its Essential Graphics panel appear on every precomp layer of that comp under Essential Properties, where they can be overridden and keyframed per instance (formerly "Master Properties").
- **Exact semantics:**
  - Source side: a comp's Essential Graphics panel controllers (properties dragged into the panel, with optional renamed labels, grouped) define the list; supported types include Slider/Angle/Checkbox/Color/Point/Dropdown values, Source Text, Media Replacement (alternate source) UNVERIFIED.
  - Layer side: `ADBE Layer Overrides` group on each precomp layer, one property per controller, initially showing the source value (not overridden).
  - Linked vs unlinked: initially every Essential Property is "linked" to its source property. Changing its value, keyframes or expression on a precomp layer "unlinks" it: that instance alone renders with the override; other instances keep the source. Unlinked properties show their name in bold and enable the Pull / Push buttons (per property and on the Essential Properties group; also in the Properties panel).
  - Pull: discards the override (value, keyframes and expression) and relinks to the source. Push: writes the override (value, keyframes and expression) into the source property, relinks, and thereby updates every instance whose property is still linked. Group-level Pull/Push acts on the selected Essential Properties, or on all of them when none is selected.
  - The source comp is the "Primary" comp set in the Essential Graphics panel.
  - Expressions can read them via `layer.essentialProperty("Name")` (AE 2020+ expression) UNVERIFIED API name (ani-b).
  - Changing the source comp's controller list adds/removes Essential Properties on all its layers (removed overrides are lost).
  - Rendering: the nested comp is rendered once per distinct set of overrides (instances with different overrides render separately).
  - Introduced as Master Properties in AE 16.1 (CC 2019 April release) UNVERIFIED version; renamed Essential Properties in AE 2020 (17.0) UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Source: Adobe help page "Essential Properties" (linked/unlinked, Pull/Push). Scripting `Property.essentialPropertySource` (AE 22.0), `AVLayer.addToMotionGraphicsTemplate()`, `CompItem.motionGraphicsTemplateName`. Essential Graphics panel UI: ui-preview-render-io.md [ui-a]. BAE: maps naturally onto USD-style per-instance opinions (ARCHITECTURE.md 4.1 Reference + override).

## Other composition and layer commands

### AE-PRJ-421 · Composition poster time (设置标识时间)（译名待核）

- **What it does:** Chooses which frame of a composition is used as its thumbnail (poster frame, 标识帧) in the Project panel.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Poster time (标识时间) | time | 0 (first frame of the comp) | [0, duration) | s (comp time, relative to comp start) | set by Composition > Set Poster Time (合成 > 设置标识时间) to the CTI |
  - Command: Composition > Set Poster Time stores the current CTI of the active comp as the poster time (undoable).
  - The Project panel thumbnail renders the comp at the poster time; transparent pixels are drawn black unless Project panel menu > Thumbnail Transparency Grid (缩略图透明度网格) is on; Preferences > Appearance > Disable Thumbnails in Project Panel (在项目面板中禁用缩略图) turns thumbnails off.
  - Never affects rendering or output. If the comp is shortened so that poster time ≥ duration, the thumbnail falls back to the last valid frame UNVERIFIED.
  - Footage items: the thumbnail of a movie is its first frame unless a poster frame was set UNVERIFIED (AE has no Set Poster Time for footage in the Composition menu).
- **Priority:** P2 — cosmetic, but persisted per comp and round-tripped in .aep.
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Not exposed in the ExtendScript `CompItem` object UNVERIFIED (no `posterTime` attribute documented). The Essential Graphics panel has its own "Set Poster Time" button for Motion Graphics templates (ui-preview-render-io.md [ui-a], Essential Graphics). Implementation hint: cache the thumbnail as a small texture keyed by (comp version, poster time) and re-render lazily when the comp changes.

### AE-PRJ-422 · Scene Edit Detection (场景编辑检测)

- **What it does:** Analyses selected video footage layers for cut points (scene changes) and either marks them with layer markers or splits the layer at each cut, optionally precomposing every resulting segment.
- **Exact semantics:**
  - Invocation: select one or more footage layers, Layer > Scene Edit Detection (图层 > 场景编辑检测) or the layer context menu. Only layers whose source is video footage qualify; precomp layers, other layer types and layers with Time Remapping enabled are rejected.
  - Dialog options:
    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Create Layer Markers (创建图层标记) | radio | UNVERIFIED (believed selected) | — | — | adds numbered markers of type Segmentation (分段) at each detected cut |
    | Split Layer at Scene Edit (在场景编辑位置拆分图层) | radio | — | — | — | splits the layer at each cut with Split Layer semantics (AE-PRJ-383); one layer per scene |
    | Create Pre-comps (创建预合成) | bool | Off UNVERIFIED | — | — | only with Split Layer; each resulting layer is precomposed with "Move all attributes" semantics (AE-PRJ-401) UNVERIFIED which option |
  - Pre-comps created by the split option are placed in the Project panel in a folder named after the footage layer, nested inside a folder "Scene Edit Detection" (场景编辑检测).
  - Marker comments are sequential numbers (1, 2, 3 …) UNVERIFIED (exact comment text); marker type = Segmentation, whose default label colour comes from Preferences > Labels (AE-PRJ-392).
  - Detection: a machine-learning (Adobe Sensei) shot-boundary detector; analysis runs over the layer's source frames between its in and out points UNVERIFIED (whether trimmed portions are analysed); cut times are frame-quantised to the source frame where the new shot begins. Fades and cuts to black are often not detected (user reports).
  - The whole operation is one undo step.
- **Priority:** P2 — convenience; the detector algorithm is BAE's own choice.
- **Owner:** bae-media (+ bae-motion, bae-model)
- **Notes:** AE 22.3 (April 2022); beta from build 22.0x88 (2021); ported from Premiere Pro. Marker type (Comment / Segmentation) introduced with it: see AE-PRJ-325. BAE decision: implement detection with a cheap CPU/GPU histogram + luma-difference shot-boundary detector (no ML runtime, to respect the size budget); expose a sensitivity threshold as an advanced option. Implementation hint: decode at low resolution (e.g. 64x36 thumbnails) on bae-media's decode path; the split/marker edits go through bae-motion Commands.
