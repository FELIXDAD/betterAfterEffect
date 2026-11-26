//! 0.0.1 acceptance suite (macOS run, 2026-11-21): the real workbench, document
//! and file commands driven through egui input, the way the window drives them.
//!
//! [`Gui`] is the shell without winit: the app is assembled like `main.rs`, every
//! frame runs [`Workspace::ui`], and the invocations it returns go through the
//! same [`CommitGate`] and dispatch order as `shell::Shell::run_pending` /
//! `dispatch` (focused panel and workbench first, then Quit / file commands,
//! then the document). Native dialogs are replaced by [`Script`] (queued paths
//! and Save / Don't Save / Cancel answers); native menu clicks by
//! [`Gui::menu`], which reads the same `Registry<MenuItemSpec>` entries (title,
//! enabled state, invocation, keymap chord) `shell::menu::AppMenu` shows.
//!
//! Widgets are found by their accesskit label ([`egui_kittest`]) or, for rows the
//! panels paint themselves (timeline layer names), by the painted text — never
//! by fixed coordinates.
//!
//! Tests that need the global UI language (`BAE_LANG=zh`) or a different `PATH`
//! re-run themselves in a child process ([`in_child`]) so they cannot disturb
//! the other tests of this binary.
// The suites run on macOS (0.0.1 acceptance); the AE shortcut sweep runs on
// both platforms, so off macOS most helpers are unused.
#![cfg_attr(not(target_os = "macos"), allow(dead_code, unused_imports))]

mod appearance_colors;
mod appearance_preferences;
mod audio_preferences;
#[cfg(target_os = "windows")]
mod builtin_scripts;
mod camera_preferences;
mod comp_navigation;
#[cfg(target_os = "macos")]
mod dialogs;
mod export_preferences;
mod expression_preferences;
#[cfg(target_os = "macos")]
mod files_flow;
#[cfg(target_os = "macos")]
mod i18n_retest;
#[cfg(target_os = "macos")]
mod i18n_scan;
mod import_preferences;
mod layer_creation_preferences;
#[cfg(target_os = "macos")]
mod layers;
#[cfg(target_os = "macos")]
mod layout;
mod marker_label_preferences;
mod memory_preferences;
#[cfg(target_os = "macos")]
mod menus;
mod preference_controls;
mod project_preferences;
#[cfg(target_os = "macos")]
mod release_001;
#[cfg(target_os = "macos")]
mod render_queue;
mod settings;
mod shape_anchor;
mod shortcut_sweep;
#[cfg(target_os = "macos")]
mod shortcuts_dialog;
#[cfg(target_os = "macos")]
mod startup;
#[cfg(target_os = "macos")]
mod stress;
#[cfg(target_os = "macos")]
mod timeline;
#[cfg(target_os = "macos")]
mod tools;
mod window_workspaces;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_app::{App, AppBuilder};
use bae_model::{PrimPath, Stage};
use bae_ui::action::ids;
use bae_ui::{Invocation, WorkbenchState};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;

use crate::document::{Document, Outcome};
use crate::files::{self, CommitGate, FileDialogs, UnsavedChoice};
use crate::ui::{StatusInfo, Workspace};

/// Window size of the simulated GUI (logical points).
const SIZE: [f32; 2] = [1600.0, 1000.0];

/// Assembles the app exactly like `main.rs` (same modules, same layer sources).
pub(crate) fn build_app() -> App {
    let mut builder = AppBuilder::new();
    builder
        .add_module(bae_platform::PlatformModule)
        .add_module(bae_gpu::GpuModule)
        .add_module(crate::model_cli::ModelCliModule)
        .add_module(crate::mcp_cli::McpCliModule)
        .add_module(crate::pack_cli::PackCliModule)
        .add_module(bae_render::RenderModule)
        .add_module(bae_motion::MotionModule)
        .add_module(bae_plugin::PluginModule)
        .add_module(crate::shell::ShellModule)
        .add_module(bae_ui_motion::MotionUiModule);
    let _ = builder.register_arc::<dyn bae_render::SourceProvider>(
        bae_vector::SHAPE_PROVIDER,
        Arc::new(bae_vector::render::ShapeProvider::new()),
    );
    let _ = builder.register_arc::<dyn bae_render::SourceProvider>(
        bae_text::TEXT_PROVIDER,
        bae_text::render::TextProvider::shared(),
    );
    builder.build()
}

/// Scripted file dialogs: queued answers, and a record of what was asked.
#[derive(Default, Debug)]
pub(crate) struct Script {
    pub open: VecDeque<Option<PathBuf>>,
    pub save: VecDeque<Option<PathBuf>>,
    pub answers: VecDeque<UnsavedChoice>,
    /// Project names the unsaved-changes question was asked about.
    pub asked: Vec<String>,
    /// Suggested names of the save dialogs.
    pub suggested: Vec<String>,
    pub errors: Vec<String>,
    /// Answers to "replace the existing project?" (Save As onto one).
    pub replace: VecDeque<bool>,
    /// Project names that question was asked about.
    pub asked_replace: Vec<String>,
    /// Dialogs that were shown with nothing scripted (a test bug or a surprise).
    pub unexpected: Vec<String>,
}

impl FileDialogs for Script {
    fn pick_project(&mut self) -> Option<PathBuf> {
        self.open.pop_front().unwrap_or_else(|| {
            self.unexpected.push("open dialog".into());
            None
        })
    }

    fn pick_save_path(&mut self, name: &str, _dir: Option<&Path>) -> Option<PathBuf> {
        self.suggested.push(name.to_owned());
        self.save.pop_front().unwrap_or_else(|| {
            self.unexpected.push(format!("save dialog ({name})"));
            None
        })
    }

    fn ask_unsaved(&mut self, name: &str) -> UnsavedChoice {
        self.asked.push(name.to_owned());
        self.answers.pop_front().unwrap_or_else(|| {
            self.unexpected.push(format!("unsaved question ({name})"));
            UnsavedChoice::Cancel
        })
    }

    fn confirm_replace(&mut self, name: &str) -> bool {
        self.asked_replace.push(name.to_owned());
        self.replace.pop_front().unwrap_or_else(|| {
            self.unexpected.push(format!("replace question ({name})"));
            false
        })
    }

    fn error(&mut self, message: &str) {
        self.errors.push(message.to_owned());
    }
}

/// The shell without a window (see the module docs).
pub(crate) struct Gui {
    pub app: App,
    pub workspace: Workspace,
    pub doc: Document,
    pub dialogs: Script,
    pending: Vec<Invocation>,
    gate: CommitGate,
    passes: u64,
    status: StatusInfo,
    themed: bool,
    /// Every invocation dispatched, in order (after the commit gate).
    pub dispatched: Vec<Invocation>,
    /// Document outcomes of model commands: `(id, outcome)`.
    pub outcomes: Vec<(String, String)>,
    /// Quit went through (the window would close).
    pub quit: bool,
    /// Preferences (in memory) and the generation last applied.
    pub prefs: Arc<bae_prefs::PrefsStore>,
    prefs_generation: u64,
}

impl Gui {
    pub fn new() -> Self {
        Self::with_app(build_app())
    }

    fn with_app(mut app: App) -> Self {
        let mut doc = Document::new(app.resource::<Arc<bae_ui::history::HistoryLog>>().cloned());
        doc.add_module_commands(&app);
        // Preferences in memory (as the shell's store, never written). Applied
        // after a change only: the stress tests keep the model's deep undo stack
        // (the app starts with Levels of Undo, AE's 99 at most).
        let prefs = Arc::new(bae_prefs::PrefsStore::default());
        app.insert_resource(prefs.clone());
        let mut workspace = Workspace::new(&mut app, WorkbenchState::default());
        workspace.set_extra_commands(doc.command_titles());
        Gui {
            prefs_generation: prefs.generation(),
            prefs,
            app,
            workspace,
            doc,
            dialogs: Script::default(),
            pending: Vec::new(),
            gate: CommitGate::default(),
            passes: 0,
            status: StatusInfo::default(),
            themed: false,
            dispatched: Vec::new(),
            outcomes: Vec::new(),
            quit: false,
        }
    }

    /// Adds the process GPU (render queue, GPU preview). `false` when there is
    /// no adapter (CI without a GPU): the caller skips.
    pub fn with_gpu(&mut self) -> bool {
        match gpu() {
            Some(gpu) => {
                self.app.insert_resource(gpu);
                true
            }
            None => false,
        }
    }

    fn frame(&mut self, ui: &mut egui::Ui) {
        if !self.themed {
            // kittest creates its own context and runs this first pass at once, so the
            // fonts `theme::apply` installs (egui's bundled fonts are off) only take
            // effect from the next pass. Draw nothing and discard the pass: egui reruns
            // it with the fonts, as the app's first frame has them (`create_view`
            // themes the context before it). kittest's first frame has no input.
            bae_ui::theme::apply(ui.ctx());
            self.themed = true;
            ui.ctx().request_discard("bae: UI fonts installed");
            return;
        }
        let invocations = self
            .workspace
            .ui(ui, &self.status, &self.doc, self.app.resources());
        self.pending.extend(invocations);
        self.passes += 1;
        // As `Shell::sync_prefs`.
        let prefs = self.prefs.clone();
        if prefs.generation() != self.prefs_generation {
            self.prefs_generation = prefs.generation();
            crate::i18n::apply_setting(prefs.current().str(bae_prefs::keys::LANGUAGE));
            bae_ui::settings::apply(&prefs.current());
            self.doc.set_undo_limit(prefs.current().levels_of_undo());
        }
    }

    /// `shell::Shell::dispatch` without the window.
    pub fn dispatch(&mut self, inv: Invocation) {
        self.dispatched.push(inv.clone());
        let Some(inv) = self.workspace.handle(inv, &self.doc, self.app.resources()) else {
            return;
        };
        match inv.id.as_str() {
            ids::QUIT => {
                if crate::view_state::confirm_quit(
                    &self.app,
                    &mut self.doc,
                    &self.workspace,
                    &mut self.dialogs,
                ) {
                    self.quit = true;
                }
            }
            ids::ABOUT => self.workspace.show_about(),
            bae_ui::settings::ACTION => self
                .workspace
                .show_settings(inv.params.get("page").and_then(|p| p.as_str())),
            id if files::is_file_command(id) => {
                // As `Shell::dispatch`.
                crate::view_state::run_file_command(
                    &mut self.app,
                    &mut self.doc,
                    &mut self.workspace,
                    &mut self.dialogs,
                    &inv,
                );
            }
            ids::HELP => {}
            _ => {
                // As `Shell::dispatch`: a failure is explained in the status bar
                // until the next change.
                let outcome = match self.doc.run(&inv) {
                    Outcome::Changed => {
                        self.status.notice.clear();
                        "changed".to_owned()
                    }
                    Outcome::Unchanged => "unchanged".to_owned(),
                    Outcome::Unknown => "unknown".to_owned(),
                    Outcome::Failed(error) => {
                        self.status.notice = crate::ui::failure_notice(&error);
                        format!("failed: {error}")
                    }
                };
                self.outcomes.push((inv.id.clone(), outcome));
            }
        }
    }

    /// A native menu click on the contributed item `registry_id` (e.g.
    /// `motion.layer.new.solid`): queued like `UserEvent::Menu`. Returns `false`
    /// (and does nothing) when the item is disabled, as a greyed-out menu item.
    pub fn menu(&mut self, registry_id: &str) -> bool {
        let item = self.menu_item(registry_id);
        if !item.enabled {
            return false;
        }
        self.pending.push(item.invocation);
        true
    }

    /// What the native menu shows for a contributed item.
    pub fn menu_item(&self, registry_id: &str) -> MenuItemView {
        self.menu_items()
            .into_iter()
            .find(|i| i.id == registry_id)
            .unwrap_or_else(|| panic!("no menu item {registry_id}"))
    }

    /// Every contributed menu item of the active workspace, as
    /// `shell::menu::AppMenu::sync` shows it.
    pub fn menu_items(&self) -> Vec<MenuItemView> {
        let tree = self.workspace.contributed_menus(&self.app);
        let open = self.workspace.open_panels();
        let actions = self.workspace.handled_actions(&self.app);
        let cx = bae_ui::menu::MenuContext::new(
            self.doc.stage(),
            self.app.resources(),
            self.workspace.active_workspace(),
        )
        .with_open_panels(&open)
        .with_actions(&actions);
        tree.items()
            .into_iter()
            .map(|(id, spec)| MenuItemView {
                id: id.to_owned(),
                path: spec.path.clone(),
                title: spec.title.clone(),
                enabled: spec.is_enabled(&cx),
                chord: self
                    .workspace
                    .keymap()
                    .chord_for_invocation(&spec.invocation())
                    .filter(|c| !c.is_typing_key())
                    .map(|c| c.to_string()),
                invocation: spec.invocation(),
            })
            .collect()
    }

    /// What the status bar shows about the GPU (the window sets it from the
    /// device it created).
    pub fn set_gpu_status(&mut self, text: String) {
        self.status.gpu = text;
    }

    /// Queues an invocation as if a panel produced it this frame.
    pub fn invoke(&mut self, inv: Invocation) {
        self.pending.push(inv);
    }

    pub fn stage(&self) -> &Stage {
        self.doc.stage()
    }

    pub fn session(&self) -> bae_ui_motion::SharedSession {
        self.app
            .resource::<bae_ui_motion::SharedSession>()
            .cloned()
            .expect("session resource")
    }

    pub fn render_queue(&self) -> bae_ui_motion::SharedRenderQueue {
        self.app
            .resource::<bae_ui_motion::SharedRenderQueue>()
            .cloned()
            .expect("render queue resource")
    }

    /// Active comp of the motion session.
    pub fn active_comp(&self) -> Option<PrimPath> {
        self.session().lock().active_comp.clone()
    }

    /// Layers of the active comp, top first.
    pub fn layers(&self) -> Vec<PrimPath> {
        self.active_comp()
            .map(|c| bae_motion::query::comp_layers(self.stage(), &c))
            .unwrap_or_default()
    }

    /// Display name of a prim.
    pub fn name(&self, path: &PrimPath) -> String {
        bae_motion::query::display_name_of(self.stage(), path)
    }

    pub fn layer_names(&self) -> Vec<String> {
        self.layers().iter().map(|l| self.name(l)).collect()
    }

    pub fn selected_layers(&self) -> Vec<PrimPath> {
        self.session().lock().selected_layers.clone()
    }

    /// Current time of the active comp.
    pub fn now(&self) -> bae_model::Time {
        let comp = self.active_comp().expect("active comp");
        self.session().lock().time(&comp)
    }

    pub fn is_playing(&self) -> bool {
        self.session().lock().is_playing()
    }

    /// Ids of the model commands the document ran, in order.
    pub fn model_commands(&self) -> Vec<String> {
        self.outcomes.iter().map(|(id, _)| id.clone()).collect()
    }
}

/// A contributed menu item as the native menu shows it.
#[derive(Debug, Clone)]
pub(crate) struct MenuItemView {
    pub id: String,
    pub path: Vec<String>,
    pub title: String,
    pub enabled: bool,
    /// The chord shown next to the item (AE keymap, `Cmd+Y` notation).
    pub chord: Option<String>,
    pub invocation: Invocation,
}

pub(crate) type Ui = Harness<'static, Gui>;

/// A harness around a fresh [`Gui`].
pub(crate) fn gui() -> Ui {
    gui_with(Gui::new())
}

pub(crate) fn gui_with(gui: Gui) -> Ui {
    gui_sized(gui, egui::vec2(SIZE[0], SIZE[1]))
}

/// A harness with a window client area of `size` logical points.
pub(crate) fn gui_sized(gui: Gui, size: egui::Vec2) -> Ui {
    gui_scaled(gui, size, 1.0)
}

/// A harness with a window client area of `size` logical points on a display
/// with `pixels_per_point` (2.0: Retina).
pub(crate) fn gui_scaled(gui: Gui, size: egui::Vec2, pixels_per_point: f32) -> Ui {
    let mut h = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(pixels_per_point)
        .with_max_steps(64)
        // A 60 Hz display: double-clicks and playback see real frame times.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, gui: &mut Gui| gui.frame(ui), gui);
    pump(&mut h);
    settle(&mut h);
    h
}

/// After a frame: run the collected invocations through the commit gate, like
/// `Shell::run_pending` (the gate's 300 ms fallback is not needed: the next
/// frame always comes).
pub(crate) fn pump(h: &mut Ui) {
    let editing = h.ctx.egui_wants_keyboard_input();
    let stop = {
        let g = h.state_mut();
        let batch = std::mem::take(&mut g.pending);
        let admitted = g.gate.admit(batch, editing, g.passes);
        for inv in admitted.run {
            g.dispatch(inv);
        }
        admitted.stop_editing
    };
    if stop {
        h.ctx.memory_mut(|m| m.stop_text_input());
    }
}

/// One step of the harness (a frame per queued event) and the dispatch after it.
pub(crate) fn frame(h: &mut Ui) {
    h.step();
    pump(h);
}

/// A few frames: enough for commands to run and the session to pick up what
/// they created (selection of a new layer, comp opened in the timeline).
pub(crate) fn settle(h: &mut Ui) {
    for _ in 0..4 {
        frame(h);
    }
}

/// Brings `panel` to the front of its frame (as clicking its tab does): Add to
/// Render Queue opens the Render Queue in front of the Timeline (AE).
pub(crate) fn to_front(h: &mut Ui, panel: &str) {
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::action::ids::OPEN_PANEL,
        serde_json::json!({ "id": panel }),
    ));
    settle(h);
}

/// One input event in its own frame, then the dispatch: what the shell does
/// for an event that arrives on its own (`about_to_wait` after each redraw).
pub(crate) fn send(h: &mut Ui, event: Event) {
    h.input_mut().events.push(event);
    frame(h);
}

/// Several input events in one frame (a busy frame: events piled up).
pub(crate) fn send_batch(h: &mut Ui, events: Vec<Event>) {
    h.input_mut().events.extend(events);
    frame(h);
}

pub(crate) fn key_events(modifiers: Modifiers, key: Key) -> [Event; 4] {
    let k = |pressed| Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers,
    };
    [
        Event::ModifiersChanged(modifiers),
        k(true),
        k(false),
        Event::ModifiersChanged(Modifiers::NONE),
    ]
}

/// A key press (modifiers down, key down, key up, modifiers up), one frame per
/// event, without settling.
pub(crate) fn press(h: &mut Ui, modifiers: Modifiers, key: Key) {
    for e in key_events(modifiers, key) {
        send(h, e);
    }
}

/// A key press, then a few frames.
pub(crate) fn key(h: &mut Ui, modifiers: Modifiers, key: Key) {
    press(h, modifiers, key);
    settle(h);
}

pub(crate) fn cmd(key_: Key) -> (Modifiers, Key) {
    (Modifiers::COMMAND, key_)
}

pub(crate) fn type_text(h: &mut Ui, text: &str) {
    send(h, Event::Text(text.to_owned()));
    settle(h);
}

/// A full IME composition (macOS Pinyin: marked text growing, then dismissed
/// and the chosen characters committed), as egui-winit 0.36 turns winit's
/// `Ime::Preedit` / `Ime::Commit` into egui events.
pub(crate) fn ime_compose(h: &mut Ui, preedit: &[&str], commit: &str) {
    for p in preedit {
        send(
            h,
            Event::Ime(egui::ImeEvent::Preedit {
                text: (*p).to_owned(),
                active_range_chars: Some(0..p.chars().count()),
            }),
        );
    }
    send(
        h,
        Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }),
    );
    send(h, Event::Ime(egui::ImeEvent::Commit(commit.to_owned())));
    settle(h);
}

/// Press the primary button, drag across six frames and release.
pub(crate) fn drag(h: &mut Ui, from: Pos2, to: Pos2, modifiers: Modifiers) {
    send(h, Event::PointerMoved(from));
    send(h, Event::ModifiersChanged(modifiers));
    send(
        h,
        Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        },
    );
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        send(h, Event::PointerMoved(from + (to - from) * t));
    }
    send(
        h,
        Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers,
        },
    );
    send(h, Event::ModifiersChanged(Modifiers::NONE));
    settle(h);
}

/// Pointer click at `pos` (move, press, release).
pub(crate) fn click_at(h: &mut Ui, pos: Pos2) {
    click_at_mods(h, pos, Modifiers::NONE);
}

pub(crate) fn click_at_mods(h: &mut Ui, pos: Pos2, modifiers: Modifiers) {
    send(h, Event::PointerMoved(pos));
    send(h, Event::ModifiersChanged(modifiers));
    for pressed in [true, false] {
        send(
            h,
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            },
        );
    }
    send(h, Event::ModifiersChanged(Modifiers::NONE));
    settle(h);
}

pub(crate) fn double_click_at(h: &mut Ui, pos: Pos2) {
    // A user pauses between actions: without it the first click of this pair
    // would pair up with a click a few frames earlier.
    pause(h);
    send(h, Event::PointerMoved(pos));
    for pressed in [true, false, true, false] {
        send(
            h,
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        );
    }
    settle(h);
}

/// Idle frames worth half a second (egui's double-click window is 0.3 s).
pub(crate) fn pause(h: &mut Ui) {
    for _ in 0..30 {
        frame(h);
    }
}

/// The control `what` ("Stop", "Remove", "Show in Finder" …) of Render Queue item
/// `number` (1-based): its accessible name is "Render Item N “Comp” · what".
pub(crate) fn queue_control<'a>(
    h: &'a Ui,
    number: usize,
    what: &str,
) -> Option<egui_kittest::Node<'a>> {
    use egui_kittest::kittest::Queryable as _;
    let prefix = format!("Render Item {number} “");
    let suffix = format!("” · {what}");
    h.query_by(move |n| {
        n.label()
            .is_some_and(|l| l.starts_with(&prefix) && l.ends_with(&suffix))
    })
}

/// Where the text `text` was painted in the last frame (first match, inside its
/// clip rect). Finds rows the panels paint themselves (timeline layer names).
/// The Timeline's name column header: "Source Name" (AE's default) or
/// "Layer Name" (after clicking it).
pub(crate) fn timeline_header(h: &Ui) -> Option<Rect> {
    find_text(h, "Source Name").or_else(|| find_text(h, "Layer Name"))
}

pub(crate) fn find_text(h: &Ui, text: &str) -> Option<Rect> {
    find_texts(h, |t| t == text).into_iter().next()
}

/// Every painted text matching `pred`, as visible rects.
pub(crate) fn find_texts(h: &Ui, pred: impl Fn(&str) -> bool) -> Vec<Rect> {
    fn walk(shape: &egui::Shape, clip: Rect, pred: &dyn Fn(&str) -> bool, out: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Text(t) => {
                if pred(t.galley.text()) {
                    let rect = t.galley.rect.translate(t.pos.to_vec2()).intersect(clip);
                    if rect.is_positive() {
                        out.push(rect);
                    }
                }
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk(s, clip, pred, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        walk(&clipped.shape, clipped.clip_rect, &pred, &mut out);
    }
    out
}

/// Every painted text of the last frame (debugging aid for failures).
pub(crate) fn painted_texts(h: &Ui) -> Vec<String> {
    let mut out = Vec::new();
    visit_painted_text(h, |_, text| out.push(text.galley.text().to_owned()));
    out
}

fn visit_painted_text<State>(
    h: &Harness<'_, State>,
    mut visit: impl FnMut(Rect, &egui::epaint::TextShape),
) {
    fn walk(
        shape: &egui::Shape,
        clip: Rect,
        visit: &mut impl FnMut(Rect, &egui::epaint::TextShape),
    ) {
        match shape {
            egui::Shape::Text(t) => visit(clip, t),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, visit)),
            _ => {}
        }
    }
    for clipped in &h.output().shapes {
        walk(&clipped.shape, clipped.clip_rect, &mut visit);
    }
}

/// The process-wide GPU for tests (`None` without an adapter), shared with every
/// other GPU test in this binary (#154).
pub(crate) fn gpu() -> Option<Arc<bae_gpu::GpuContext>> {
    bae_gpu::testing::shared_context()
}

/// Whether a motion dialog (Composition Settings …) is open.
pub(crate) fn dialog_open(h: &Ui) -> bool {
    h.state()
        .app
        .resource::<bae_ui_motion::SharedDialogs>()
        .is_some_and(|d| d.is_open())
}

/// New Composition the AE way: Cmd+N opens Composition Settings, Enter (OK)
/// creates the comp with the dialog's settings and opens it.
pub(crate) fn new_comp(h: &mut Ui) {
    let (m, k) = cmd(Key::N);
    key(h, m, k);
    assert!(dialog_open(h), "Cmd+N opened no Composition Settings");
    key(h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(h), "Enter did not close Composition Settings");
}

/// OK (Enter) in the motion dialog when one is open: Layer > New > Solid asks
/// for Solid Settings first.
pub(crate) fn confirm_dialog(h: &mut Ui) {
    if dialog_open(h) {
        key(h, Modifiers::NONE, Key::Enter);
        assert!(!dialog_open(h), "Enter did not close the dialog");
    }
}

/// New Solid the AE way: Cmd+Y opens Solid Settings, Enter (OK) creates the
/// solid with the dialog's settings.
pub(crate) fn new_solid(h: &mut Ui) {
    let (m, k) = cmd(Key::Y);
    key(h, m, k);
    assert!(dialog_open(h), "Cmd+Y opened no Solid Settings");
    confirm_dialog(h);
}

/// New comp (Cmd+N, OK) with one layer from `Layer > New > <registry id>`
/// (OK in Solid Settings); returns the harness settled.
pub(crate) fn comp_with(h: &mut Ui, layers: &[&str]) {
    new_comp(h);
    assert!(h.state().active_comp().is_some(), "Cmd+N opened no comp");
    for id in layers {
        assert!(add_layer(h, id), "{id} disabled with a comp open");
        settle(h);
        confirm_dialog(h);
    }
}

/// Adds a layer through the menu item `id`, or, for `shape:rect` /
/// `shape:ellipse`, a shape layer with that path (what BAE's old Layer ▸ New ▸
/// Rectangle / Ellipse made; AE has no such menu items, it draws them with the
/// shape tools). Returns `false` when the menu item is greyed.
pub(crate) fn add_layer(h: &mut Ui, id: &str) -> bool {
    match id.strip_prefix("shape:") {
        Some(shape) => {
            h.state_mut().invoke(Invocation::with_params(
                "motion.new_layer",
                serde_json::json!({ "kind": "shape", "shape": shape }),
            ));
            true
        }
        None => h.state_mut().menu(id),
    }
}

/// Runs this test's body in a child process with `env` set (global UI language,
/// `PATH`), so it cannot disturb the other tests of the binary. In the child
/// (`BAE_ACC_CHILD` set) returns `true` and the caller runs the body.
pub(crate) fn in_child(test: &str, env: &[(&str, &str)]) -> bool {
    in_child_at(test, env, None)
}

/// [`in_child`] with the child's working directory set to `cwd`.
pub(crate) fn in_child_at(test: &str, env: &[(&str, &str)], cwd: Option<&Path>) -> bool {
    if std::env::var_os("BAE_ACC_CHILD").is_some() {
        return true;
    }
    let exe = std::env::current_exe().expect("test executable");
    let mut command = std::process::Command::new(exe);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .args([
            test,
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "--include-ignored",
        ])
        .env("BAE_ACC_CHILD", "1");
    for (k, v) in env {
        command.env(k, v);
    }
    let output = output_within(&mut command, std::time::Duration::from_secs(600));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child test {test} failed:\n{stdout}\n{stderr}"
    );
    false
}

/// `Command::output` (stdin closed, output captured) that fails the test, and
/// kills the process, if it has not exited within `limit`: a hang then fails
/// with a message instead of holding the CI job until its own limit.
pub(crate) fn output_within(
    command: &mut std::process::Command,
    limit: std::time::Duration,
) -> std::process::Output {
    use std::process::Stdio;
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("run {command:?}: {e}"));
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(limit) {
        Ok(output) => output.unwrap_or_else(|e| panic!("wait for {command:?}: {e}")),
        Err(_) => {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
            panic!("{command:?} still running after {limit:?}");
        }
    }
}

/// A path under the temp dir with spaces and Chinese characters.
pub(crate) fn tricky_dir(root: &Path) -> PathBuf {
    let dir = root.join("验收 测试 folder").join("工程 文件");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
