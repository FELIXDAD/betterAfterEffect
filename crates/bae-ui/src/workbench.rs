//! The workbench: title bar, toolbar, workspace bar and dock, filled with the
//! registered panels; keyboard shortcuts; workspace switching, saving and resetting.
//!
//! The application shell (`apps/bae`) owns one [`Workbench`] and, every frame:
//!
//! 1. calls [`Workbench::ui`] inside `egui::Context::run_ui`, which draws everything
//!    and returns the command invocations collected this frame (shortcuts, menus
//!    in panels, the workspace bar, panel requests);
//! 2. passes each invocation (and each native menu click) to [`Workbench::handle`],
//!    which tries the focused panel and the workbench's own actions;
//! 3. runs whatever is left itself: undo/redo, application actions and
//!    `bae-model` commands (see [`crate::action`]).
//!
//! The persistent part ([`WorkbenchState`]: active workspace, layouts, panel view
//! state, shortcut customizations) is saved by the shell.

mod shortcuts;
mod workspaces;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bae_app::{App, Registry, RegistrySnapshot, Resources};
use bae_model::{AuthorId, Stage};
use egui::{Context, Event, Key, Rect, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::a11y::Accessible as _;
use crate::action::{ActionHandler, ActionInfo, Invocation, ids};
use crate::dock::{Dock, Layout, PanelHost, PanelId, TabInfo};
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::keymap::{Chord, Keymap, KeymapBindings, KeymapOverrides};
use crate::panel::{Panel, PanelContext, PanelKind, PanelRequests, PanelSpawn};
use crate::theme::{color, metrics};
use crate::workspace::WorkspacePreset;
use crate::{titlebar, widgets};

pub use shortcuts::CommandEntry;

/// Height of the status strip at the bottom of the window.
pub const STATUS_H: f32 = 20.0;
/// Width of the workspace bar at the right of the toolbar (AE 2026 default window).
const WORKSPACE_BAR_W: f32 = 525.5;

/// What the workbench remembers between sessions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkbenchState {
    /// Id of the active workspace.
    pub active: Option<String>,
    /// Current (possibly unsaved) layout of each workspace.
    pub current: BTreeMap<String, Layout>,
    /// Layouts saved with "Save Changes to this Workspace"; "Reset" returns here.
    pub saved: BTreeMap<String, Layout>,
    /// View state of panels ([`Panel::save_state`]), by panel id.
    pub panels: BTreeMap<String, serde_json::Value>,
    /// Shortcut customizations, by keymap id.
    pub keymaps: BTreeMap<String, KeymapOverrides>,
    /// Workspace bar order chosen in Edit Workspaces (ids; others follow).
    pub order: Vec<String>,
    /// Workspaces not shown in the bar (still in Window > Workspace).
    pub hidden: Vec<String>,
    /// Workspaces the user created with "Save as New Workspace".
    pub custom: BTreeMap<String, CustomWorkspace>,
}

/// A user-created workspace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomWorkspace {
    pub title: String,
    pub keymap: String,
    pub layout: Layout,
}

/// What the workbench needs from the shell for one frame.
pub struct WorkbenchContext<'a> {
    pub stage: &'a Stage,
    pub author: &'a AuthorId,
    pub resources: &'a Resources,
    /// Window title (project name), drawn in the custom title bar on macOS.
    pub title: &'a str,
    /// Unsaved changes (dot in the close button).
    pub dirty: bool,
}

/// The tools shown in the toolbar of AE-keymap workspaces (selection is view state
/// until the motion workspace, workstream 12, registers real tools).
const AE_TOOLS: [(Icon, &str); 18] = [
    (Icon::Home, "Home"),
    (Icon::Selection, "Selection Tool"),
    (Icon::Hand, "Hand Tool"),
    (Icon::Zoom, "Zoom Tool"),
    (Icon::Orbit, "Orbit Around Cursor Tool"),
    (Icon::PanCamera, "Pan Under Cursor Tool"),
    (Icon::Dolly, "Dolly Towards Cursor Tool"),
    (Icon::Rotate, "Rotation Tool"),
    (Icon::PanBehind, "Pan Behind (Anchor Point) Tool"),
    (Icon::Rectangle, "Rectangle Tool"),
    (Icon::Extrude, "Cube Tool"),
    (Icon::Pen, "Pen Tool"),
    (Icon::Type, "Horizontal Type Tool"),
    (Icon::Brush, "Brush Tool"),
    (Icon::CloneStamp, "Clone Stamp Tool"),
    (Icon::Eraser, "Eraser Tool"),
    (Icon::RotoBrush, "Roto Brush Tool"),
    (Icon::Puppet, "Puppet Position Pin Tool"),
];

pub struct Workbench {
    dock: Dock,
    panels: HashMap<PanelId, Box<dyn Panel>>,
    kinds: Arc<Registry<PanelKind>>,
    presets: Arc<Registry<WorkspacePreset>>,
    keymaps: Arc<Registry<Keymap>>,
    keymap_extensions: Arc<Registry<KeymapBindings>>,
    actions: Arc<Registry<ActionInfo>>,
    /// Module action handlers, tried after the focused panel.
    handlers: Arc<Registry<dyn ActionHandler>>,
    handlers_gen: u64,
    handler_conflicts: Vec<String>,
    keymap_conflicts: Vec<String>,
    /// Model commands `(id, English title)` for the shortcut editor.
    extra_commands: Vec<(String, String)>,
    editor: shortcuts::ShortcutEditor,
    dialogs: workspaces::Dialogs,
    /// Workspaces sorted by order, rebuilt when the registry changes.
    workspaces: Vec<(String, Arc<WorkspacePreset>)>,
    presets_gen: u64,
    keymaps_gen: u64,
    extensions_gen: u64,
    active: String,
    keymap: Keymap,
    state: WorkbenchState,
    requests: PanelRequests,
    tool: usize,
}

impl Workbench {
    /// A workbench over the app's registries, restoring `state`.
    pub fn new(app: &mut App, state: WorkbenchState) -> Self {
        Self::with_registries(
            app.registry::<PanelKind>(),
            app.registry::<WorkspacePreset>(),
            app.registry::<Keymap>(),
            app.registry::<ActionInfo>(),
            state,
        )
        .with_keymap_extensions(app.registry::<KeymapBindings>())
        .with_action_handlers(app.registry::<dyn ActionHandler>())
    }

    /// Module action handlers (`Registry<dyn ActionHandler>`), tried after the
    /// focused panel and before the shell's actions.
    pub fn with_action_handlers(mut self, handlers: Arc<Registry<dyn ActionHandler>>) -> Self {
        self.handlers = handlers;
        self.handlers_gen = u64::MAX;
        self.refresh_registries();
        self
    }

    /// Action ids claimed by more than one module handler (also logged).
    pub fn handler_conflicts(&self) -> &[String] {
        &self.handler_conflicts
    }

    /// Chords bound by more than one module keymap extension (also logged).
    pub fn keymap_conflicts(&self) -> &[String] {
        &self.keymap_conflicts
    }

    /// Module handlers in priority order: the active workspace's first, then
    /// registration order.
    fn ordered_handlers(&self) -> Vec<Arc<dyn ActionHandler>> {
        let snapshot = self.handlers.snapshot();
        let mut v: Vec<(usize, Arc<dyn ActionHandler>)> = snapshot
            .iter()
            .enumerate()
            .map(|(i, e)| (i, e.item().clone()))
            .collect();
        v.sort_by_key(|(i, h)| (h.workspace() != Some(self.active.as_str()), *i));
        v.into_iter().map(|(_, h)| h).collect()
    }

    /// Merge bindings other modules add to keymaps (`Registry<KeymapBindings>`).
    pub fn with_keymap_extensions(mut self, extensions: Arc<Registry<KeymapBindings>>) -> Self {
        self.keymap_extensions = extensions;
        self.extensions_gen = u64::MAX;
        self.refresh_registries();
        self
    }

    pub fn with_registries(
        kinds: Arc<Registry<PanelKind>>,
        presets: Arc<Registry<WorkspacePreset>>,
        keymaps: Arc<Registry<Keymap>>,
        actions: Arc<Registry<ActionInfo>>,
        state: WorkbenchState,
    ) -> Self {
        let mut wb = Workbench {
            dock: Dock::new(crate::dock::Node::tabs(&[])),
            panels: HashMap::new(),
            kinds,
            presets,
            keymaps,
            keymap_extensions: Arc::new(Registry::new()),
            actions,
            handlers: Arc::new(Registry::new()),
            handlers_gen: u64::MAX,
            handler_conflicts: Vec::new(),
            keymap_conflicts: Vec::new(),
            extra_commands: Vec::new(),
            editor: shortcuts::ShortcutEditor::default(),
            dialogs: workspaces::Dialogs::default(),
            workspaces: Vec::new(),
            presets_gen: u64::MAX,
            keymaps_gen: u64::MAX,
            extensions_gen: u64::MAX,
            active: String::new(),
            keymap: Keymap::default(),
            state,
            requests: PanelRequests::default(),
            tool: 1,
        };
        wb.refresh_registries();
        let active = wb
            .state
            .active
            .clone()
            .filter(|id| wb.preset(id).is_some())
            .or_else(|| wb.workspaces.first().map(|(id, _)| id.clone()))
            .unwrap_or_default();
        wb.load_workspace(&active);
        wb
    }

    // ---- queries -----------------------------------------------------------------

    /// Id of the active workspace.
    pub fn active_workspace(&self) -> &str {
        &self.active
    }

    /// Workspace ids and titles in bar order.
    pub fn workspaces(&self) -> impl Iterator<Item = (&str, &WorkspacePreset)> + '_ {
        self.workspaces.iter().map(|(id, p)| (id.as_str(), &**p))
    }

    /// The effective keymap of the active workspace (preset + user overrides).
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Id of the active workspace's keymap.
    pub fn keymap_id(&self) -> String {
        self.preset(&self.active)
            .map(|p| p.keymap.clone())
            .unwrap_or_default()
    }

    pub fn dock(&self) -> &Dock {
        &self.dock
    }

    pub fn dock_mut(&mut self) -> &mut Dock {
        &mut self.dock
    }

    /// The focused panel (shortcut routing, AE's blue frame).
    pub fn focused_panel(&self) -> Option<&PanelId> {
        self.dock.focused_panel()
    }

    /// Whether a panel instance exists (it has been shown at least once).
    pub fn has_instance(&self, panel: &PanelId) -> bool {
        self.panels.contains_key(panel)
    }

    /// The state to persist (captures the current layout and panel view state).
    pub fn state(&mut self) -> WorkbenchState {
        self.capture();
        self.state.clone()
    }

    /// Model commands `(id, English title)` the shortcut editor lists in addition
    /// to registered UI actions (the shell passes its `CommandRegistry` infos).
    pub fn set_extra_commands(&mut self, commands: Vec<(String, String)>) {
        self.extra_commands = commands;
    }

    /// Whether the Keyboard Shortcuts window is open.
    pub fn shortcut_editor_open(&self) -> bool {
        self.editor.open
    }

    /// Replace the user's customization of keymap `id` (shortcut editor).
    pub fn set_keymap_overrides(&mut self, id: &str, overrides: KeymapOverrides) {
        if overrides.is_empty() {
            self.state.keymaps.remove(id);
        } else {
            self.state.keymaps.insert(id.to_owned(), overrides);
        }
        self.rebuild_keymap();
    }

    pub fn keymap_overrides(&self, id: &str) -> KeymapOverrides {
        self.state.keymaps.get(id).cloned().unwrap_or_default()
    }

    fn preset(&self, id: &str) -> Option<&Arc<WorkspacePreset>> {
        self.workspaces
            .iter()
            .find(|(w, _)| w == id)
            .map(|(_, p)| p)
    }

    // ---- workspaces --------------------------------------------------------------

    /// Switch to workspace `id`, keeping the current one's layout for later.
    pub fn switch_workspace(&mut self, id: &str) -> bool {
        if self.preset(id).is_none() {
            return false;
        }
        if id != self.active {
            self.capture();
            self.load_workspace(id);
        }
        true
    }

    /// "Reset to Saved Layout": the last saved layout, or the preset.
    pub fn reset_workspace(&mut self) {
        let id = self.active.clone();
        let layout = self.saved_layout(&id);
        self.state.current.remove(&id);
        self.set_layout(layout);
    }

    /// "Save Changes to this Workspace".
    pub fn save_workspace(&mut self) {
        self.state
            .saved
            .insert(self.active.clone(), self.dock.layout.clone());
    }

    fn saved_layout(&self, id: &str) -> Layout {
        self.state
            .saved
            .get(id)
            .cloned()
            .or_else(|| self.preset(id).map(|p| p.layout.clone()))
            .unwrap_or_else(|| Layout {
                root: crate::dock::Node::tabs(&[]),
                floating: Vec::new(),
            })
    }

    fn load_workspace(&mut self, id: &str) {
        self.active = id.to_owned();
        self.state.active = Some(id.to_owned());
        let layout = self
            .state
            .current
            .get(id)
            .cloned()
            .unwrap_or_else(|| self.saved_layout(id));
        self.set_layout(layout);
        self.rebuild_keymap();
    }

    /// Replace the dock layout, dropping instances of panels it no longer shows
    /// (their view state is kept for when they come back).
    fn set_layout(&mut self, layout: Layout) {
        let keep = layout.panel_ids();
        let gone: Vec<PanelId> = self
            .panels
            .keys()
            .filter(|p| !keep.contains(p))
            .cloned()
            .collect();
        for id in gone {
            if let Some(panel) = self.panels.remove(&id)
                && let Some(state) = panel.save_state()
            {
                self.state.panels.insert(id.0.clone(), state);
            }
        }
        self.dock.set_layout(layout);
    }

    /// Store the live layout and panel view state into `self.state`.
    fn capture(&mut self) {
        if !self.active.is_empty() {
            self.state
                .current
                .insert(self.active.clone(), self.dock.layout.clone());
        }
        for (id, panel) in &self.panels {
            if let Some(state) = panel.save_state() {
                self.state.panels.insert(id.0.clone(), state);
            }
        }
    }

    fn refresh_registries(&mut self) {
        let generation = self.presets.generation();
        if generation != self.presets_gen {
            self.presets_gen = generation;
            self.rebuild_workspaces();
        }
        let generations = (
            self.keymaps.generation(),
            self.keymap_extensions.generation(),
        );
        if generations != (self.keymaps_gen, self.extensions_gen) {
            (self.keymaps_gen, self.extensions_gen) = generations;
            self.rebuild_keymap();
        }
        let generation = self.handlers.generation();
        if generation != self.handlers_gen {
            self.handlers_gen = generation;
            let snapshot = self.handlers.snapshot();
            let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for e in snapshot.iter() {
                for a in e.item().actions() {
                    owners.entry(a).or_default().push(e.id().to_string());
                }
            }
            self.handler_conflicts = owners
                .into_iter()
                .filter(|(_, o)| o.len() > 1)
                .map(|(a, o)| {
                    let m = format!(
                        "action `{a}` is claimed by several modules: {}",
                        o.join(", ")
                    );
                    tracing::warn!("{m}");
                    m
                })
                .collect();
        }
    }

    fn rebuild_keymap(&mut self) {
        let id = self.keymap_id();
        let mut base = self
            .keymaps
            .get(&id)
            .map(|k| (*k).clone())
            .unwrap_or_default();
        // Modules of the active workspace first, then registration order: the
        // first binding of a chord wins.
        let extensions = self.keymap_extensions.snapshot();
        let mut ext: Vec<(usize, &str, &KeymapBindings)> = extensions
            .iter()
            .enumerate()
            .filter(|(_, e)| e.item().keymap == id)
            .map(|(i, e)| (i, e.id().as_ref(), e.item().as_ref()))
            .collect();
        ext.sort_by_key(|(i, _, k)| (k.workspace.as_deref() != Some(self.active.as_str()), *i));
        self.keymap_conflicts.clear();
        let mut claimed: Vec<(&Chord, Option<&str>, &str)> = Vec::new();
        for (_, owner, k) in &ext {
            for b in &k.bindings {
                if let Some((_, _, first)) = claimed
                    .iter()
                    .find(|(c, w, o)| **c == b.chord && *w == b.when.as_deref() && o != owner)
                {
                    let m = format!(
                        "shortcut {} ({}) of `{owner}` conflicts with `{first}`; `{first}` wins",
                        b.chord,
                        b.when.as_deref().unwrap_or("global")
                    );
                    tracing::warn!("{m}");
                    self.keymap_conflicts.push(m);
                } else {
                    claimed.push((&b.chord, b.when.as_deref(), owner));
                }
            }
        }
        for (_, _, k) in &ext {
            base.bindings.extend(k.bindings.iter().cloned());
        }
        self.keymap = match self.state.keymaps.get(&id) {
            Some(overrides) => base.with_overrides(overrides),
            None => base,
        };
    }

    // ---- frame ---------------------------------------------------------------------

    /// Draw the workbench into `ui` (the whole window) and return the invocations
    /// collected this frame. `status` draws the contents of the bottom status strip.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        cx: &WorkbenchContext<'_>,
        status: impl FnOnce(&mut Ui),
    ) -> Vec<Invocation> {
        self.refresh_registries();
        // A modal dialog (the welcome / role picker) owns the keyboard: no
        // shortcuts and no panel keys reach the project behind it.
        let modal = self.role_picker_open();
        crate::panel::set_modal_open(ui.ctx(), modal);
        if !modal && !self.record_shortcut(ui.ctx()) {
            self.collect_shortcuts(ui.ctx());
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, color::APP_BG);
        titlebar::show(ui, cx.title, cx.dirty);

        // Toolbar strip: tools on the left, workspaces on the right.
        let top = full.top() + titlebar::height() + 0.5;
        let toolbar = Rect::from_min_max(
            pos2(full.left(), top),
            pos2(full.right(), top + metrics::TOOLBAR_H - 0.5),
        );
        ui.painter().rect_filled(toolbar, 0.0, color::PANEL);
        let bar_w = WORKSPACE_BAR_W.min(toolbar.width() * 0.5);
        let ws_rect = Rect::from_min_max(pos2(full.right() - bar_w, toolbar.top()), toolbar.max);
        ui.painter().vline(
            ws_rect.left() - 0.5,
            toolbar.y_range(),
            egui::Stroke::new(1.0, color::APP_BG),
        );
        if self.keymap_id() == "ae" {
            self.tools(ui, toolbar, ws_rect.left());
        }
        self.workspace_bar(ui, ws_rect);

        // Status strip.
        let status_rect = Rect::from_min_max(pos2(full.left(), full.bottom() - STATUS_H), full.max);
        ui.painter().rect_filled(status_rect, 0.0, color::PANEL);
        let mut status_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(status_rect.shrink2(vec2(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        status(&mut status_ui);

        // Dock.
        let dock_rect = Rect::from_min_max(
            pos2(
                full.left() + 1.0,
                toolbar.bottom() + metrics::TOOLBAR_GAP - 0.5,
            ),
            pos2(full.right() - 1.0, status_rect.top() - 1.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(dock_rect));
        let kinds = self.kinds.snapshot();
        let focused = self.dock.focused_panel().cloned();
        let mut host = Host {
            panels: &mut self.panels,
            kinds: &kinds,
            saved: &mut self.state.panels,
            cx,
            requests: &mut self.requests,
            focused,
        };
        self.dock.show(&mut child, &mut host);
        self.apply_panel_requests(cx);
        self.shortcut_window(ui.ctx());
        self.workspace_dialogs(ui.ctx());
        ime_follows_caret(ui.ctx());
        std::mem::take(&mut self.requests.invocations)
    }

    fn tools(&mut self, ui: &mut Ui, toolbar: Rect, right: f32) {
        let area = Rect::from_min_max(
            toolbar.min + vec2(20.0, 8.0),
            pos2(right - 8.0, toolbar.bottom() - 8.0),
        );
        let mut tools = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        tools.set_clip_rect(area);
        for (i, (icon, name)) in AE_TOOLS.into_iter().enumerate() {
            let selected = i == self.tool;
            // One tool is active at a time: radio buttons.
            let resp = icons::button(&mut tools, icon, 24.0, selected).labeled_toggle(
                egui::WidgetType::RadioButton,
                tr(name),
                selected,
            );
            if resp.clicked() && i > 0 {
                self.tool = i;
            }
        }
    }

    fn workspace_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let visible: Vec<&(String, Arc<WorkspacePreset>)> = self
            .workspaces
            .iter()
            .filter(|(id, _)| !self.state.hidden.contains(id) || *id == self.active)
            .collect();
        let names: Vec<&str> = visible.iter().map(|(_, p)| p.title.as_str()).collect();
        let active = visible
            .iter()
            .position(|(id, _)| *id == self.active)
            .unwrap_or(0);
        let Some(event) = widgets::workspace_bar(ui, rect, &names, active) else {
            return;
        };
        let inv = match event {
            widgets::WorkspaceEvent::Switch(i) => match visible.get(i) {
                Some((id, _)) => {
                    Invocation::with_params(ids::SWITCH_WORKSPACE, serde_json::json!({ "id": id }))
                }
                None => return,
            },
            widgets::WorkspaceEvent::Reset => Invocation::new(ids::RESET_WORKSPACE),
            widgets::WorkspaceEvent::Save => Invocation::new(ids::SAVE_WORKSPACE),
            widgets::WorkspaceEvent::Edit => Invocation::new(ids::EDIT_WORKSPACES),
        };
        self.requests.invocations.push(inv);
    }

    /// Turn key presses (and copy/cut/paste) into invocations through the keymap,
    /// consuming the events they trigger. Keys the focused widget uses stay with
    /// it ([`KeyFocus`]); with nothing focused every bound key is a shortcut.
    fn collect_shortcuts(&mut self, ctx: &Context) {
        let focus = KeyFocus::of(ctx);
        let typing = focus == KeyFocus::Text;
        let focused = self.dock.focused_panel().map(|p| p.kind().to_owned());
        let keymap = &self.keymap;
        let mut hits = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let Some(binding) =
                        keymap.match_event(*key, *physical_key, *modifiers, focused.as_deref())
                    else {
                        return true;
                    };
                    if focus.reserves(&binding.chord) {
                        return true;
                    }
                    hits.push(binding.invocation());
                    false
                }
                Event::Copy | Event::Cut if !typing => {
                    let id = if matches!(event, Event::Copy) {
                        "edit.copy"
                    } else {
                        "edit.cut"
                    };
                    hits.push(Invocation::new(id));
                    false
                }
                Event::Paste(text) if !typing => {
                    hits.push(Invocation::with_params(
                        "edit.paste",
                        serde_json::json!({ "text": text }),
                    ));
                    false
                }
                _ => true,
            });
        });
        self.requests.invocations.extend(hits);
    }

    fn apply_panel_requests(&mut self, cx: &WorkbenchContext<'_>) {
        for id in std::mem::take(&mut self.requests.close) {
            self.close_panel(&id, cx);
        }
        for id in std::mem::take(&mut self.requests.open) {
            self.open_panel(id);
        }
    }

    /// Open (or bring to front) a panel.
    pub fn open_panel(&mut self, id: PanelId) {
        self.dock.open(id.clone());
        self.dock.focus_panel(&id);
    }

    /// Close a panel, letting it save its view state first.
    pub fn close_panel(&mut self, id: &PanelId, cx: &WorkbenchContext<'_>) {
        self.dock.close(id);
        if let Some(mut panel) = self.panels.remove(id) {
            if let Some(state) = panel.save_state() {
                self.state.panels.insert(id.0.clone(), state);
            }
            let kinds = self.kinds.snapshot();
            let title = kinds.get(id.kind()).map_or("", |k| k.title.as_str());
            let mut pcx = PanelContext::new(
                id,
                title,
                cx.stage,
                cx.author,
                cx.resources,
                &mut self.requests,
            );
            panel.on_close(&mut pcx);
        }
    }

    /// Run `inv` if the focused panel or the workbench handles it. Returns `false`
    /// when the shell should handle it (undo/redo, application, model commands).
    pub fn handle(&mut self, inv: &Invocation, cx: &WorkbenchContext<'_>) -> bool {
        if let Some(id) = self.dock.focused_panel().cloned()
            && let Some(panel) = self.panels.get_mut(&id)
        {
            let kinds = self.kinds.snapshot();
            let title = kinds.get(id.kind()).map_or("", |k| k.title.as_str());
            let mut pcx = PanelContext::new(
                &id,
                title,
                cx.stage,
                cx.author,
                cx.resources,
                &mut self.requests,
            );
            pcx.focused = true;
            if panel.on_action(inv, &mut pcx) {
                self.apply_panel_requests(cx);
                return true;
            }
        }
        // Module handlers (menus and shortcuts work whichever panel has focus).
        let focused = self
            .dock
            .focused_panel()
            .cloned()
            .unwrap_or_else(|| PanelId::new("workbench"));
        for h in self.ordered_handlers() {
            if !h.actions().contains(&inv.id) {
                continue;
            }
            let mut pcx = PanelContext::new(
                &focused,
                "",
                cx.stage,
                cx.author,
                cx.resources,
                &mut self.requests,
            );
            if h.handle(inv, &mut pcx) {
                self.apply_panel_requests(cx);
                return true;
            }
        }
        match inv.id.as_str() {
            ids::SWITCH_WORKSPACE => {
                if let Some(id) = inv.id_param() {
                    self.switch_workspace(id);
                } else if let Some(i) = inv.params.get("index").and_then(|v| v.as_u64())
                    && let Some((id, _)) = self.workspaces.get(i as usize).cloned()
                {
                    self.switch_workspace(&id);
                }
                true
            }
            ids::RESET_WORKSPACE => {
                self.reset_workspace();
                true
            }
            ids::SAVE_WORKSPACE => {
                self.save_workspace();
                true
            }
            ids::OPEN_PANEL => {
                if let Some(id) = inv.id_param().map(PanelId::new) {
                    // AE: a panel shortcut closes the panel if it is already in front.
                    let toggle = inv.params.get("toggle").and_then(|v| v.as_bool()) == Some(true);
                    let in_front = self
                        .dock
                        .frame_of(&id)
                        .and_then(|f| self.dock.frame(f))
                        .and_then(|f| f.active_panel())
                        == Some(&id);
                    if toggle && in_front {
                        self.close_panel(&id, cx);
                    } else {
                        self.open_panel(id);
                    }
                }
                true
            }
            ids::CLOSE_PANEL => {
                if let Some(id) = self.dock.focused_panel().cloned() {
                    self.close_panel(&id, cx);
                }
                true
            }
            ids::MAXIMIZE_FRAME => {
                self.dock.toggle_maximize(None);
                true
            }
            ids::EDIT_WORKSPACES => {
                self.dialogs.edit_open = true;
                true
            }
            ids::NEW_WORKSPACE => {
                if let Some(title) = inv.params.get("title").and_then(|v| v.as_str()) {
                    self.save_as_new_workspace(title);
                } else {
                    self.dialogs.edit_open = true;
                }
                true
            }
            ids::KEYBOARD_SHORTCUTS => {
                if !self.editor.open {
                    self.editor.open = true;
                    self.editor.focus_search = true;
                }
                true
            }
            _ => false,
        }
    }
}

/// Anchor the IME candidate window at the text caret instead of the whole text
/// field. egui-winit passes `IMEOutput::rect` to `Window::set_ime_cursor_area`, so
/// with the field's rect the pinyin candidates open at the field's left edge; AE
/// (and native macOS / Windows text fields) open them under the caret.
pub fn ime_follows_caret(ctx: &Context) {
    ctx.output_mut(|o| {
        if let Some(ime) = &mut o.ime {
            ime.rect = ime.cursor_rect;
        }
    });
}

/// Chords a focused text field needs: plain typing, editing and navigation keys,
/// and the standard text-editing commands.
/// What has keyboard focus, for routing keys: the focused widget gets the keys
/// it uses, everything else stays a shortcut (AE: Space plays unless a text
/// field or a focused button wants it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyFocus {
    /// Nothing: every bound key is a shortcut.
    None,
    /// A text field: typing, caret movement, Enter and text commands.
    Text,
    /// A button, checkbox, combo box, slider …: activation and navigation.
    Widget,
}

impl KeyFocus {
    fn of(ctx: &Context) -> Self {
        if ctx.text_edit_focused() {
            KeyFocus::Text
        } else if ctx.memory(|m| m.focused().is_some()) {
            KeyFocus::Widget
        } else {
            KeyFocus::None
        }
    }

    /// The focused widget gets this chord instead of the keymap.
    fn reserves(self, chord: &Chord) -> bool {
        match self {
            KeyFocus::None => false,
            KeyFocus::Text => reserved_while_typing(chord),
            KeyFocus::Widget => reserved_by_focused_widget(chord),
        }
    }
}

/// Keys egui widgets use while focused: Space / Enter press buttons and toggle
/// checkboxes, Tab / Shift+Tab move focus, arrows change sliders and drag
/// values, Esc leaves.
fn reserved_by_focused_widget(chord: &Chord) -> bool {
    use Key::*;
    !chord.command
        && !chord.alt
        && !chord.mac_ctrl
        && matches!(
            chord.key,
            Space | Enter | Tab | Escape | ArrowLeft | ArrowRight | ArrowUp | ArrowDown
        )
}

fn reserved_while_typing(chord: &Chord) -> bool {
    use Key::*;
    if chord.is_typing_key() {
        return true;
    }
    let editing = matches!(
        chord.key,
        ArrowLeft
            | ArrowRight
            | ArrowUp
            | ArrowDown
            | Home
            | End
            | Backspace
            | Delete
            | Enter
            | Tab
    );
    let text_command = chord.command && !chord.alt && matches!(chord.key, Z | Y | A | C | V | X);
    editing || text_command
}

/// Adapts registered panels to the dock.
struct Host<'a, 'c> {
    panels: &'a mut HashMap<PanelId, Box<dyn Panel>>,
    kinds: &'a RegistrySnapshot<PanelKind>,
    saved: &'a mut BTreeMap<String, serde_json::Value>,
    cx: &'a WorkbenchContext<'c>,
    requests: &'a mut PanelRequests,
    focused: Option<PanelId>,
}

impl Host<'_, '_> {
    fn ensure(&mut self, id: &PanelId) {
        if self.panels.contains_key(id) {
            return;
        }
        let state = self.saved.remove(id.as_str());
        let panel: Box<dyn Panel> = match self.kinds.get(id.kind()) {
            Some(kind) => kind.create(&PanelSpawn {
                id,
                state: state.as_ref(),
                resources: self.cx.resources,
            }),
            None => Box::new(Missing(id.clone())),
        };
        self.panels.insert(id.clone(), panel);
    }

    /// Run `f` with the panel and a context for it.
    fn with_panel<R>(
        &mut self,
        id: &PanelId,
        f: impl FnOnce(&mut dyn Panel, &mut PanelContext<'_>) -> R,
    ) -> R {
        self.ensure(id);
        let title = self.kinds.get(id.kind()).map_or("", |k| k.title.as_str());
        let focused = self.focused.as_ref() == Some(id);
        let panel = self
            .panels
            .get_mut(id)
            .unwrap_or_else(|| unreachable!("panel instance was just created"));
        let mut cx = PanelContext::new(
            id,
            title,
            self.cx.stage,
            self.cx.author,
            self.cx.resources,
            self.requests,
        );
        cx.focused = focused;
        f(panel.as_mut(), &mut cx)
    }
}

impl PanelHost for Host<'_, '_> {
    fn tab(&mut self, panel: &PanelId) -> TabInfo {
        if self.kinds.get(panel.kind()).is_none() {
            return TabInfo::new(panel.as_str());
        }
        self.with_panel(panel, |p, cx| p.tab(cx))
    }

    fn ui(&mut self, ui: &mut Ui, panel: &PanelId) {
        self.with_panel(panel, |p, cx| p.ui(ui, cx));
    }

    fn menu(&mut self, ui: &mut Ui, panel: &PanelId) {
        self.with_panel(panel, |p, cx| p.menu(ui, cx));
    }

    fn on_close(&mut self, panel: &PanelId) {
        if let Some(mut p) = self.panels.remove(panel) {
            if let Some(state) = p.save_state() {
                self.saved.insert(panel.0.clone(), state);
            }
            let title = self
                .kinds
                .get(panel.kind())
                .map_or("", |k| k.title.as_str());
            let mut cx = PanelContext::new(
                panel,
                title,
                self.cx.stage,
                self.cx.author,
                self.cx.resources,
                self.requests,
            );
            p.on_close(&mut cx);
        }
    }

    fn set_locked(&mut self, panel: &PanelId, locked: bool) {
        self.with_panel(panel, |p, _| p.set_locked(locked));
    }
}

/// Shown for panel ids no module registers (a removed plugin, an old layout).
struct Missing(PanelId);

impl Panel for Missing {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        ui.centered_and_justified(|ui| {
            ui.weak(format!(
                "{} ({})",
                tr("This panel is not available."),
                self.0.as_str()
            ));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::Node;
    use crate::keymap::Binding;
    use bae_app::Owner;
    use egui::{Modifiers, RawInput};

    struct Echo;
    impl Panel for Echo {
        fn ui(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
            ui.label("echo");
        }
        fn on_action(&mut self, action: &Invocation, _cx: &mut PanelContext<'_>) -> bool {
            action.id == "echo.zoom"
        }
        fn save_state(&self) -> Option<serde_json::Value> {
            Some(serde_json::json!("kept"))
        }
    }

    type Registries = (
        Arc<Registry<PanelKind>>,
        Arc<Registry<WorkspacePreset>>,
        Arc<Registry<Keymap>>,
        Arc<Registry<ActionInfo>>,
    );

    fn registries() -> Registries {
        let owner = Owner::module("test");
        let kinds = Arc::new(Registry::<PanelKind>::new());
        for k in ["a", "b", "c"] {
            kinds
                .register(owner.clone(), k, PanelKind::new(k, |_| Box::new(Echo)))
                .unwrap();
        }
        let presets = Arc::new(Registry::<WorkspacePreset>::new());
        presets
            .register(
                owner.clone(),
                "one",
                WorkspacePreset::new(
                    "One",
                    "km",
                    Node::row(vec![(1.0, Node::tabs(&["a"])), (1.0, Node::tabs(&["b"]))]),
                ),
            )
            .unwrap();
        presets
            .register(
                owner.clone(),
                "two",
                WorkspacePreset::new("Two", "other", Node::tabs(&["c"])).order(1),
            )
            .unwrap();
        let keymaps = Arc::new(Registry::<Keymap>::new());
        keymaps
            .register(
                owner.clone(),
                "km",
                Keymap::new(
                    "KM",
                    vec![
                        Binding::new("Shift+F11", ids::SWITCH_WORKSPACE)
                            .params(serde_json::json!({ "id": "two" })),
                        Binding::new("U", "layer.reveal_keyframes"),
                    ],
                ),
            )
            .unwrap();
        keymaps
            .register(owner.clone(), "other", Keymap::new("Other", vec![]))
            .unwrap();
        let actions = Arc::new(Registry::<ActionInfo>::new());
        actions
            .register(owner, ids::UNDO, ActionInfo::new("Undo"))
            .unwrap();
        (kinds, presets, keymaps, actions)
    }

    fn frame(wb: &mut Workbench, events: Vec<Event>) -> Vec<Invocation> {
        let ctx = Context::default();
        frame_in(&ctx, wb, events)
    }

    fn frame_in(ctx: &Context, wb: &mut Workbench, events: Vec<Event>) -> Vec<Invocation> {
        let stage = Stage::new();
        let author = AuthorId::system();
        let resources = Resources::new();
        let cx = WorkbenchContext {
            stage: &stage,
            author: &author,
            resources: &resources,
            title: "BAE",
            dirty: false,
        };
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = Vec::new();
        let mut full = ctx.run_ui(input, |ui| out = wb.ui(ui, &cx, |_| {}));
        full.textures_delta.clear();
        let mut rest = Vec::new();
        for inv in out {
            if !wb.handle(&inv, &cx) {
                rest.push(inv);
            }
        }
        rest
    }

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// A panel with a button (keyboard-focused on request) and a text field.
    struct Focusable {
        clicks: Arc<std::sync::atomic::AtomicUsize>,
        text: Arc<std::sync::Mutex<String>>,
        focus: Arc<std::sync::Mutex<Option<&'static str>>>,
    }
    impl Panel for Focusable {
        fn ui(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
            let button = ui.button("Render");
            if button.clicked() {
                self.clicks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let mut text = self.text.lock().unwrap();
            let field = ui.text_edit_singleline(&mut *text);
            match self.focus.lock().unwrap().take() {
                Some("button") => button.request_focus(),
                Some("field") => field.request_focus(),
                _ => {}
            }
        }
    }

    #[test]
    fn focused_widgets_get_their_keys_before_shortcuts() {
        let clicks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let text = Arc::new(std::sync::Mutex::new(String::from("a")));
        let focus = Arc::new(std::sync::Mutex::new(None));
        let (kinds, presets, keymaps, actions) = registries();
        let (c, t, f) = (clicks.clone(), text.clone(), focus.clone());
        kinds
            .replace(
                Owner::module("test"),
                "a",
                Arc::new(PanelKind::new("A", move |_| {
                    Box::new(Focusable {
                        clicks: c.clone(),
                        text: t.clone(),
                        focus: f.clone(),
                    })
                })),
            )
            .unwrap();
        keymaps
            .replace(
                Owner::module("test"),
                "km",
                Arc::new(Keymap::new(
                    "KM",
                    vec![
                        Binding::new("Space", "motion.play"),
                        Binding::new("Enter", "layer.rename"),
                        Binding::new("U", "layer.reveal_keyframes"),
                    ],
                )),
            )
            .unwrap();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        let ctx = Context::default();
        let none = Modifiers::NONE;
        frame_in(&ctx, &mut wb, vec![]);
        // Nothing focused: Space plays.
        assert_eq!(
            frame_in(&ctx, &mut wb, vec![key(Key::Space, none)]),
            vec![Invocation::new("motion.play")]
        );

        // A focused button: Space and Enter press it, nothing else runs.
        *focus.lock().unwrap() = Some("button");
        frame_in(&ctx, &mut wb, vec![]);
        frame_in(&ctx, &mut wb, vec![]);
        assert!(frame_in(&ctx, &mut wb, vec![key(Key::Space, none)]).is_empty());
        assert!(frame_in(&ctx, &mut wb, vec![key(Key::Enter, none)]).is_empty());
        assert_eq!(clicks.load(std::sync::atomic::Ordering::Relaxed), 2);
        // Keys a button does not use are still shortcuts.
        assert_eq!(
            frame_in(&ctx, &mut wb, vec![key(Key::U, none)]),
            vec![Invocation::new("layer.reveal_keyframes")]
        );

        // A focused text field: Space types a space.
        *focus.lock().unwrap() = Some("field");
        frame_in(&ctx, &mut wb, vec![]);
        frame_in(&ctx, &mut wb, vec![]);
        let typed = frame_in(
            &ctx,
            &mut wb,
            vec![key(Key::Space, none), Event::Text(" ".into())],
        );
        assert!(typed.is_empty(), "{typed:?}");
        // …and U types a U.
        let typed = frame_in(
            &ctx,
            &mut wb,
            vec![key(Key::U, none), Event::Text("U".into())],
        );
        assert!(typed.is_empty(), "{typed:?}");
        assert_eq!(text.lock().unwrap().as_str(), "a U");
    }

    #[test]
    fn shortcuts_switch_workspaces_and_keymaps() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        assert_eq!(wb.active_workspace(), "one");
        let rest = frame(&mut wb, vec![key(Key::U, Modifiers::NONE)]);
        assert_eq!(rest, vec![Invocation::new("layer.reveal_keyframes")]);
        assert!(wb.has_instance(&"a".into()) && wb.has_instance(&"b".into()));

        let rest = frame(&mut wb, vec![key(Key::F11, Modifiers::SHIFT)]);
        assert!(
            rest.is_empty(),
            "workspace.switch is handled by the workbench"
        );
        assert_eq!(wb.active_workspace(), "two");
        assert_eq!(wb.keymap_id(), "other");
        // The other keymap has no `U`.
        assert!(frame(&mut wb, vec![key(Key::U, Modifiers::NONE)]).is_empty());
        // Panels of the old workspace were dropped but kept their view state.
        assert!(!wb.has_instance(&"a".into()));
        assert_eq!(wb.state().panels.get("a"), Some(&serde_json::json!("kept")));
    }

    #[test]
    fn layouts_are_kept_per_workspace_and_reset_to_saved() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        frame(&mut wb, vec![]);
        wb.open_panel("c".into());
        assert!(wb.dock().contains(&"c".into()));
        wb.switch_workspace("two");
        wb.switch_workspace("one");
        assert!(
            wb.dock().contains(&"c".into()),
            "unsaved change survives a switch"
        );
        wb.reset_workspace();
        assert!(
            !wb.dock().contains(&"c".into()),
            "reset returns to the preset"
        );

        wb.open_panel("c".into());
        wb.save_workspace();
        wb.close_panel(
            &"c".into(),
            &WorkbenchContext {
                stage: &Stage::new(),
                author: &AuthorId::system(),
                resources: &Resources::new(),
                title: "",
                dirty: false,
            },
        );
        wb.reset_workspace();
        assert!(
            wb.dock().contains(&"c".into()),
            "reset returns to the saved layout"
        );

        // State survives a restart.
        let state = wb.state();
        let (kinds, presets, keymaps, actions) = registries();
        let wb2 = Workbench::with_registries(kinds, presets, keymaps, actions, state);
        assert!(wb2.dock().contains(&"c".into()));
        assert_eq!(wb2.active_workspace(), "one");
    }

    #[test]
    fn focused_panel_gets_actions_first_and_overrides_apply() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        frame(&mut wb, vec![]);
        wb.open_panel("b".into());
        let cx_stage = Stage::new();
        let author = AuthorId::system();
        let resources = Resources::new();
        let cx = WorkbenchContext {
            stage: &cx_stage,
            author: &author,
            resources: &resources,
            title: "",
            dirty: false,
        };
        assert!(wb.handle(&Invocation::new("echo.zoom"), &cx));
        assert!(!wb.handle(&Invocation::new("prim.create"), &cx));

        let mut ov = KeymapOverrides::default();
        ov.bind(Binding::new("U", "custom.thing"));
        wb.set_keymap_overrides("km", ov);
        let rest = frame(&mut wb, vec![key(Key::U, Modifiers::NONE)]);
        assert_eq!(rest, vec![Invocation::new("custom.thing")]);
    }

    #[test]
    fn shortcut_editor_records_and_asks_before_replacing() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        wb.set_extra_commands(vec![("prim.create".into(), "Create Prim".into())]);
        let rows = wb.shortcut_commands();
        assert!(rows.iter().any(|r| r.id == ids::UNDO));
        assert!(rows.iter().any(|r| r.id == "prim.create"));
        assert_eq!(
            wb.command_title(ids::SWITCH_WORKSPACE, &serde_json::json!({"id": "two"})),
            "workspace.switch: Two"
        );

        // Record `U` for prim.create: it collides with layer.reveal_keyframes.
        let entry = rows.iter().find(|r| r.id == "prim.create").unwrap().clone();
        wb.editor.open = true;
        wb.editor.recording = Some(entry.clone());
        frame(&mut wb, vec![key(Key::U, Modifiers::NONE)]);
        assert!(wb.editor.confirm.is_some(), "conflict must be confirmed");
        let (binding, other) = wb.editor.confirm.take().unwrap();
        assert_eq!(other, "layer.reveal_keyframes");
        wb.bind(binding);
        assert_eq!(
            wb.keymap()
                .lookup(&Chord::parse("U").unwrap(), None)
                .unwrap()
                .action,
            "prim.create"
        );
        assert!(wb.keymap().conflicts().is_empty());

        // A free chord binds immediately; Esc cancels recording.
        wb.editor.recording = Some(entry.clone());
        frame(
            &mut wb,
            vec![key(Key::K, Modifiers::COMMAND | Modifiers::SHIFT)],
        );
        assert!(wb.editor.confirm.is_none());
        assert_eq!(wb.keymap().chords_for("prim.create").count(), 2);
        wb.editor.recording = Some(entry);
        frame(&mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(wb.editor.recording.is_none());
        assert_eq!(wb.keymap().chords_for("prim.create").count(), 2);
        // Customizations persist.
        assert!(!wb.state().keymaps.get("km").unwrap().is_empty());
    }

    #[test]
    fn ime_composition_in_text_fields() {
        use egui::ImeEvent;
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        let stage = Stage::new();
        let author = AuthorId::system();
        let resources = Resources::new();
        let cx = WorkbenchContext {
            stage: &stage,
            author: &author,
            resources: &resources,
            title: "",
            dirty: false,
        };
        let ctx = Context::default();
        let run = |wb: &mut Workbench, events: Vec<Event>| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = Vec::new();
            let mut full = ctx.run_ui(input, |ui| out = wb.ui(ui, &cx, |_| {}));
            full.textures_delta.clear();
            (out, full.platform_output.ime)
        };
        // Cmd+Alt+' equivalent: open the editor; its search field takes focus.
        assert!(wb.handle(&Invocation::new(ids::KEYBOARD_SHORTCUTS), &cx));
        run(&mut wb, vec![]);
        let (_, ime) = run(&mut wb, vec![]);
        let ime = ime.expect("a focused text field asks for the IME");

        // Pinyin composition; a `U` that slips through as a key event must not
        // trigger the `U` shortcut while a text field has focus.
        let preedit = |t: &str| {
            Event::Ime(ImeEvent::Preedit {
                text: t.into(),
                active_range_chars: None,
            })
        };
        let (out, during) = run(
            &mut wb,
            vec![preedit("ni'hao"), key(Key::U, Modifiers::NONE)],
        );
        assert!(out.is_empty(), "no shortcut fires while composing: {out:?}");
        let during = during.expect("IME stays enabled while composing");
        // The candidate window is anchored at the caret, inside the field.
        assert_eq!(during.rect, during.cursor_rect);
        assert!(during.cursor_rect.height() <= 24.0);
        assert!(ime.rect.min.y >= 0.0 && during.cursor_rect.min.x >= ime.rect.min.x);

        // macOS commit sequence (egui-winit docs): Preedit("") then Commit.
        run(
            &mut wb,
            vec![preedit(""), Event::Ime(ImeEvent::Commit("你好".into()))],
        );
        assert_eq!(wb.editor.search, "你好");
        // Backspace removing the whole composition commits nothing.
        run(&mut wb, vec![preedit("j"), preedit("")]);
        assert_eq!(wb.editor.search, "你好");
        // The caret moved after the committed text.
        let (_, after) = run(&mut wb, vec![]);
        assert!(after.unwrap().cursor_rect.min.x > ime.cursor_rect.min.x);
    }

    #[test]
    fn user_workspaces_order_hide_delete_and_persist() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        wb.open_panel("c".into());
        let id = wb.save_as_new_workspace("  Mine ").unwrap();
        assert_eq!(wb.active_workspace(), id);
        assert!(wb.dock().contains(&"c".into()));
        let titles = |wb: &Workbench| {
            wb.workspaces()
                .map(|(_, p)| p.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(titles(&wb), ["One", "Two", "Mine"]);
        wb.move_workspace(&id, -1);
        wb.move_workspace(&id, -1);
        wb.move_workspace(&id, -1);
        assert_eq!(titles(&wb), ["Mine", "One", "Two"]);
        wb.set_workspace_hidden("two", true);
        assert!(
            !wb.delete_workspace(&id),
            "the active workspace cannot be deleted"
        );
        assert!(!wb.delete_workspace("one"), "presets cannot be deleted");

        // Survives a restart, in the same order.
        let state = wb.state();
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb = Workbench::with_registries(kinds, presets, keymaps, actions, state);
        assert_eq!(wb.active_workspace(), id);
        assert_eq!(titles(&wb), ["Mine", "One", "Two"]);
        assert_eq!(wb.state().hidden, ["two"]);
        wb.switch_workspace("one");
        assert!(wb.delete_workspace(&id));
        assert_eq!(titles(&wb), ["One", "Two"]);
        // Workspace actions are routed through `handle`.
        let resources = Resources::new();
        let stage = Stage::new();
        let author = AuthorId::system();
        let cx = WorkbenchContext {
            stage: &stage,
            author: &author,
            resources: &resources,
            title: "",
            dirty: false,
        };
        assert!(wb.handle(
            &Invocation::with_params(ids::NEW_WORKSPACE, serde_json::json!({ "title": "Two B" })),
            &cx
        ));
        assert_eq!(wb.workspaces().count(), 3);
        assert!(wb.handle(&Invocation::new(ids::EDIT_WORKSPACES), &cx));
        frame(&mut wb, vec![]);
        wb.show_role_picker();
        frame(&mut wb, vec![]);
    }

    /// Issue #112: while the modal welcome / role picker is open, shortcuts
    /// and panel keys do not reach the project behind it.
    #[test]
    fn no_shortcuts_behind_a_modal() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        let ctx = Context::default();
        wb.show_role_picker();
        frame_in(&ctx, &mut wb, vec![]);
        assert!(frame_in(&ctx, &mut wb, vec![key(Key::U, Modifiers::NONE)]).is_empty());
        assert!(crate::panel::modal_open(&ctx));
        frame_in(&ctx, &mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!wb.role_picker_open());
        frame_in(&ctx, &mut wb, vec![]);
        assert!(!crate::panel::modal_open(&ctx));
        assert_eq!(
            frame_in(&ctx, &mut wb, vec![key(Key::U, Modifiers::NONE)]),
            vec![Invocation::new("layer.reveal_keyframes")]
        );
    }

    #[test]
    fn role_picker_closes_on_escape_and_outside_click() {
        let (kinds, presets, keymaps, actions) = registries();
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default());
        let ctx = Context::default();
        let pointer = |pos: egui::Pos2, pressed: bool| {
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        wb.show_role_picker();
        for _ in 0..3 {
            frame_in(&ctx, &mut wb, vec![]);
        }
        assert!(wb.role_picker_open());
        // A click inside the dialog keeps it open.
        let inside = egui::pos2(700.0, 450.0);
        frame_in(&ctx, &mut wb, pointer(inside, true));
        frame_in(&ctx, &mut wb, pointer(inside, false));
        assert!(wb.role_picker_open());
        frame_in(&ctx, &mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!wb.role_picker_open());
        assert_eq!(wb.active_workspace(), "one", "closing keeps the workspace");

        wb.show_role_picker();
        for _ in 0..3 {
            frame_in(&ctx, &mut wb, vec![]);
        }
        let outside = egui::pos2(20.0, 880.0);
        frame_in(&ctx, &mut wb, pointer(outside, true));
        frame_in(&ctx, &mut wb, pointer(outside, false));
        assert!(!wb.role_picker_open());
    }

    #[test]
    fn modules_extend_keymaps() {
        let (kinds, presets, keymaps, actions) = registries();
        let extensions = Arc::new(Registry::<KeymapBindings>::new());
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default())
                .with_keymap_extensions(extensions.clone());
        extensions
            .register(
                Owner::module("motion"),
                "motion.timeline",
                KeymapBindings::new("km", vec![Binding::new("J", "timeline.prev_key").when("b")]),
            )
            .unwrap();
        extensions
            .register(
                Owner::module("other"),
                "other",
                KeymapBindings::new("not-active", vec![Binding::new("K", "x")]),
            )
            .unwrap();
        frame(&mut wb, vec![]);
        wb.open_panel("b".into());
        let rest = frame(&mut wb, vec![key(Key::J, Modifiers::NONE)]);
        assert_eq!(rest, vec![Invocation::new("timeline.prev_key")]);
        assert!(frame(&mut wb, vec![key(Key::K, Modifiers::NONE)]).is_empty());
        // Unregistering (plugin unload) removes the shortcut.
        extensions.unregister("motion.timeline");
        assert!(frame(&mut wb, vec![key(Key::J, Modifiers::NONE)]).is_empty());
    }

    struct Claim {
        workspace: Option<&'static str>,
        tag: &'static str,
        log: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl ActionHandler for Claim {
        fn actions(&self) -> Vec<String> {
            vec!["mod.go".into()]
        }
        fn workspace(&self) -> Option<&str> {
            self.workspace
        }
        fn handle(&self, _inv: &Invocation, _cx: &mut PanelContext<'_>) -> bool {
            self.log.lock().unwrap().push(self.tag);
            true
        }
    }

    #[test]
    fn module_handlers_run_by_workspace_priority() {
        let (kinds, presets, keymaps, actions) = registries();
        let handlers = Arc::new(Registry::<dyn ActionHandler>::new());
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        for (id, workspace) in [("a", None), ("b", Some("two"))] {
            let h = Claim {
                workspace,
                tag: id,
                log: log.clone(),
            };
            handlers
                .register_arc(Owner::module(id), id, Arc::new(h))
                .unwrap();
        }
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default())
                .with_action_handlers(handlers);
        assert_eq!(
            wb.handler_conflicts().len(),
            1,
            "{:?}",
            wb.handler_conflicts()
        );
        let resources = Resources::new();
        let stage = Stage::new();
        let author = AuthorId::system();
        let cx = WorkbenchContext {
            stage: &stage,
            author: &author,
            resources: &resources,
            title: "",
            dirty: false,
        };
        // Workspace "one": no preference, registration order → a.
        assert!(wb.handle(&Invocation::new("mod.go"), &cx));
        // Workspace "two": b belongs to it and goes first.
        wb.switch_workspace("two");
        assert!(wb.handle(&Invocation::new("mod.go"), &cx));
        assert_eq!(*log.lock().unwrap(), ["a", "b"]);
        // Ids nobody claims fall through to the shell.
        assert!(!wb.handle(&Invocation::new("model.command"), &cx));
    }

    #[test]
    fn keymap_extensions_prefer_the_active_workspace() {
        let (kinds, presets, keymaps, actions) = registries();
        let extensions = Arc::new(Registry::<KeymapBindings>::new());
        let mut wb =
            Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default())
                .with_keymap_extensions(extensions.clone());
        let first = KeymapBindings::new("km", vec![Binding::new("J", "first.j")]);
        extensions
            .register(Owner::module("first"), "first", first)
            .unwrap();
        let second = |ws: &str| {
            Arc::new(
                KeymapBindings::new("km", vec![Binding::new("J", "second.j")]).for_workspace(ws),
            )
        };
        extensions
            .register_arc(Owner::module("second"), "second", second("elsewhere"))
            .unwrap();
        // Same chord in two modules: a conflict, the first registered wins.
        assert_eq!(
            frame(&mut wb, vec![key(Key::J, Modifiers::NONE)]),
            vec![Invocation::new("first.j")]
        );
        assert_eq!(wb.keymap_conflicts().len(), 1);
        // The module of the active workspace ("one") wins.
        extensions
            .replace(Owner::module("second"), "second", second("one"))
            .unwrap();
        assert_eq!(
            frame(&mut wb, vec![key(Key::J, Modifiers::NONE)]),
            vec![Invocation::new("second.j")]
        );
        assert!(
            wb.keymap_conflicts()[0].ends_with("`second` wins"),
            "{:?}",
            wb.keymap_conflicts()
        );
    }

    #[test]
    fn typing_keys_are_left_to_text_fields() {
        assert!(reserved_while_typing(&Chord::parse("U").unwrap()));
        assert!(reserved_while_typing(&Chord::parse("Cmd+Z").unwrap()));
        assert!(reserved_while_typing(
            &Chord::parse("Shift+ArrowLeft").unwrap()
        ));
        assert!(!reserved_while_typing(&Chord::parse("Cmd+S").unwrap()));
        assert!(!reserved_while_typing(&Chord::parse("F9").unwrap()));
    }
}
