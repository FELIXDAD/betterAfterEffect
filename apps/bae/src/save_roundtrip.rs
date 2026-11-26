//! Regression tests: animation made with the motion commands survives every save
//! path of the shell's document layer (Save As `.bae` / `.baez`, Cmd/Ctrl+S right
//! after the last edit, `.baez` saved in place) and reads back the same through
//! `Document::open`, `Project::open` and `bae export-json`'s flattened JSON.
//!
//! The 5090 acceptance run reported a position keyframe missing after a save
//! (2 in the timeline, 1 in `bae export-json`); these tests pin the save side.

use std::path::Path;

use bae_model::{AttrSpec, PrimPath, Project, Time};
use bae_motion::query;
use bae_ui::Invocation;
use serde_json::{Value as Json, json};

use crate::document::{Document, Outcome};
use crate::files::{self, FileDialogs, FileOutcome, UnsavedChoice};

const COMP: &str = "/comps/Main";
const LAYER: &str = "/comps/Main/layers/Rect";
const POSITION: &str = "transform:position";
const SEC: i64 = Time::FLICKS_PER_SECOND;

/// A document with the motion commands, registered the way the shell does
/// (`MotionModule` → `Registry<dyn DynCommand>` → `Document::add_module_commands`).
fn motion_document() -> Document {
    let mut builder = bae_app::AppBuilder::new();
    builder.add_module(bae_motion::MotionModule);
    let app = builder.build();
    let mut doc = Document::new(None);
    doc.add_module_commands(&app);
    doc
}

fn run(doc: &mut Document, id: &str, params: Json) {
    match doc.run(&Invocation::with_params(id, params.clone())) {
        Outcome::Changed => {}
        other => panic!("{id} {params}: {other:?}"),
    }
}

fn vec3(x: f64, y: f64) -> Json {
    json!({"type": "vec3", "value": [x, y, 0.0]})
}

/// What the timeline does: new comp, Layer > New > Rectangle, the position
/// stopwatch at 0, then a value edit at 1 s (a second keyframe).
fn animate_rect(doc: &mut Document) {
    run(
        doc,
        "comp.create",
        json!({"name": "Main", "fps": 30, "duration": 5 * SEC}),
    );
    run(
        doc,
        "layer.create",
        json!({"comp": COMP, "kind": "shape", "shape": "rect", "name": "Rect"}),
    );
    run(
        doc,
        "attr.set_animated",
        json!({"path": LAYER, "attr": POSITION, "animated": true, "time": 0}),
    );
    run(
        doc,
        "attr.set_at",
        json!({"path": LAYER, "attr": POSITION, "time": SEC, "value": vec3(1500.0, 300.0)}),
    );
}

fn layer() -> PrimPath {
    LAYER.parse().unwrap()
}

/// The composed position attribute (what the timeline shows).
fn position(doc: &Document) -> AttrSpec {
    query::attr_spec(doc.stage(), &layer(), POSITION)
        .cloned()
        .unwrap_or_else(|| panic!("{LAYER} has no {POSITION}"))
}

/// Position keyframe times in `bae export-json`'s flattened output.
fn exported_key_times(path: &Path) -> Vec<String> {
    let project = Project::open(path).unwrap();
    let json = bae_model::export_flattened_json(&project.stage);
    let mut node = &json["prims"];
    for name in ["comps", "Main", "layers", "Rect"] {
        node = node
            .as_array()
            .and_then(|a| a.iter().find(|p| p["name"] == name))
            .unwrap_or_else(|| panic!("no prim {name} in export: {json}"));
        if name != "Rect" {
            node = &node["children"];
        }
    }
    let keys = &node["attrs"][POSITION]["keyframes"];
    let mut times: Vec<String> = keys
        .as_object()
        .unwrap_or_else(|| panic!("no keyframes in export: {node}"))
        .keys()
        .cloned()
        .collect();
    times.sort_by_key(|t| t.parse::<i64>().unwrap_or(i64::MAX));
    times
}

/// Reopen `path` through the document layer and the model and compare.
fn assert_reads_back(path: &Path, want: &AttrSpec) {
    let mut doc = Document::new(None);
    doc.open(path).unwrap();
    assert_eq!(
        &position(&doc),
        want,
        "{} via Document::open",
        path.display()
    );
    let project = Project::open(path).unwrap();
    assert_eq!(
        query::attr_spec(&project.stage, &layer(), POSITION),
        Some(want),
        "{} via Project::open",
        path.display()
    );
    let want_times: Vec<String> = want.keyframes.keys().map(|t| t.0.to_string()).collect();
    assert_eq!(
        exported_key_times(path),
        want_times,
        "{} via export-json",
        path.display()
    );
}

#[test]
fn two_position_keyframes_survive_save_as_bae_and_baez() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = motion_document();
    animate_rect(&mut doc);
    let want = position(&doc);
    assert_eq!(want.keyframes.len(), 2, "{want:?}");
    let values: Vec<_> = want.keyframes.values().map(|k| k.value.clone()).collect();
    assert_ne!(values[0], values[1], "the two keyframes differ");
    assert_eq!(
        want.keyframes.keys().copied().collect::<Vec<_>>(),
        [Time(0), Time(SEC)]
    );

    // Saved right after the last edit, no frame in between.
    let bae = dir.path().join("Keys.bae");
    doc.save_as(&bae).unwrap();
    assert!(!doc.is_dirty());
    assert_reads_back(&bae, &want);

    let baez = dir.path().join("Keys.baez");
    doc.save_as(&baez).unwrap();
    assert_reads_back(&baez, &want);
}

/// The 5090 repro: P, Alt+Shift+P at frame 0, Alt+Shift+P at frame 3, the
/// rectangle never moved — two keyframes 0.1 s apart with the same value.
#[test]
fn close_keyframes_with_equal_and_different_values_survive_save() {
    let dir = tempfile::tempdir().unwrap();
    let frame3 = 3 * SEC / 30;
    for (case, second) in [("same", None), ("different", Some(vec3(1200.0, 300.0)))] {
        let mut doc = motion_document();
        run(
            &mut doc,
            "comp.create",
            json!({"name": "Main", "fps": 30, "duration": 5 * SEC}),
        );
        run(
            &mut doc,
            "layer.create",
            json!({"comp": COMP, "kind": "shape", "shape": "rect", "name": "Rect"}),
        );
        // Alt+Shift+P at frame 0: the stopwatch.
        run(
            &mut doc,
            "attr.set_animated",
            json!({"path": LAYER, "attr": POSITION, "animated": true, "time": 0}),
        );
        // Alt+Shift+P at frame 3: a keyframe with the current (sampled) value.
        let value = match &second {
            Some(v) => v.clone(),
            None => serde_json::to_value(
                query::sample_attr(doc.stage(), &layer(), POSITION, Time(frame3)).unwrap(),
            )
            .unwrap(),
        };
        run(
            &mut doc,
            "attr.set_at",
            json!({"path": LAYER, "attr": POSITION, "time": frame3, "value": value}),
        );
        let want = position(&doc);
        assert_eq!(
            want.keyframes.keys().copied().collect::<Vec<_>>(),
            [Time(0), Time(frame3)],
            "{case}: in memory"
        );
        for ext in ["bae", "baez"] {
            let path = dir.path().join(format!("kf2-{case}.{ext}"));
            doc.save_as(&path).unwrap();
            assert_reads_back(&path, &want);
        }
    }
}

#[test]
fn interpolation_and_ease_survive_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = motion_document();
    animate_rect(&mut doc);
    let key = |t: i64| json!({"path": LAYER, "attr": POSITION, "time": t});
    run(&mut doc, "key.ease", json!({"keys": [key(SEC)]}));
    run(
        &mut doc,
        "key.set_interpolation",
        json!({"keys": [key(0)], "interp_out": "hold"}),
    );
    let want = position(&doc);
    assert_ne!(
        want.keyframes[&Time(0)].interp_out,
        want.keyframes[&Time(SEC)].interp_out,
        "{want:?}"
    );
    let bae = dir.path().join("Ease.bae");
    doc.save_as(&bae).unwrap();
    assert_reads_back(&bae, &want);
}

/// Answers nothing: Cmd/Ctrl+S on a saved project needs no dialog.
struct NoQuestions;

impl FileDialogs for NoQuestions {
    fn pick_project(&mut self) -> Option<std::path::PathBuf> {
        panic!("unexpected open dialog")
    }
    fn pick_save_path(&mut self, _: &str, _: Option<&Path>) -> Option<std::path::PathBuf> {
        panic!("unexpected save dialog")
    }
    fn ask_unsaved(&mut self, _: &str) -> UnsavedChoice {
        panic!("unexpected question")
    }
    fn confirm_replace(&mut self, _: &str) -> bool {
        panic!("unexpected replace question")
    }
    fn error(&mut self, message: &str) {
        panic!("{message}")
    }
}

#[test]
fn ctrl_s_right_after_the_second_keyframe_writes_it() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Quick.bae", "Quick.baez"] {
        let path = dir.path().join(name);
        let mut doc = motion_document();
        run(
            &mut doc,
            "comp.create",
            json!({"name": "Main", "fps": 30, "duration": 5 * SEC}),
        );
        run(
            &mut doc,
            "layer.create",
            json!({"comp": COMP, "kind": "shape", "shape": "rect", "name": "Rect"}),
        );
        run(
            &mut doc,
            "attr.set_animated",
            json!({"path": LAYER, "attr": POSITION, "animated": true, "time": 0}),
        );
        // First save with one keyframe, then the second keyframe and a silent
        // Cmd/Ctrl+S (the file command, as the shortcut runs it).
        doc.save_as(&path).unwrap();
        assert_eq!(exported_key_times(&path), ["0"]);
        run(
            &mut doc,
            "attr.set_at",
            json!({"path": LAYER, "attr": POSITION, "time": 2 * SEC, "value": vec3(100.0, 900.0)}),
        );
        assert!(doc.is_dirty());
        let saved = files::run(
            &mut doc,
            &mut NoQuestions,
            &Invocation::new(bae_ui::action::ids::SAVE),
        );
        assert_eq!(saved, Some(FileOutcome::Saved), "{name}");
        assert!(!doc.is_dirty());
        let want = position(&doc);
        assert_eq!(want.keyframes.len(), 2, "{name}: {want:?}");
        assert_reads_back(&path, &want);

        // A third edit (value change on an existing key) and save again.
        run(
            &mut doc,
            "attr.set_at",
            json!({"path": LAYER, "attr": POSITION, "time": 0, "value": vec3(10.0, 20.0)}),
        );
        doc.save().unwrap();
        assert_reads_back(&path, &position(&doc));
    }
}

#[test]
fn undo_and_redo_before_saving_keep_the_shown_keyframes() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = motion_document();
    animate_rect(&mut doc);
    run(&mut doc, bae_ui::action::ids::UNDO, json!(null));
    assert_eq!(position(&doc).keyframes.len(), 1);
    run(&mut doc, bae_ui::action::ids::REDO, json!(null));
    let want = position(&doc);
    assert_eq!(want.keyframes.len(), 2);
    let path = dir.path().join("Redo.bae");
    doc.save_as(&path).unwrap();
    assert_reads_back(&path, &want);
}

/// Every opinion the motion demo project holds (shapes, text, keyframes with
/// eases, expressions, several comps) reads back identically after a save: the
/// in-memory mirror and what the Loro snapshot stores never diverge.
#[test]
fn demo_project_reads_back_identically() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = motion_document();
    for inv in bae_ui_motion::demo::steps() {
        match doc.run(&inv) {
            Outcome::Changed | Outcome::Unchanged => {}
            other => panic!("demo step {}: {other:?}", inv.id),
        }
    }
    let before = serde_json::to_value(doc.stage().flatten()).unwrap();
    for name in ["Demo.bae", "Demo.baez"] {
        let path = dir.path().join(name);
        doc.save_as(&path).unwrap();
        let mut reopened = Document::new(None);
        reopened.open(&path).unwrap();
        let after = serde_json::to_value(reopened.stage().flatten()).unwrap();
        assert_eq!(before, after, "{name}");
    }
}

/// UI-side ordering, with the real workbench, the AE keymap and a field that
/// commits like the motion panels' hot numbers (on Enter or losing focus).
mod edit_then_save {
    use std::sync::Arc;

    use bae_app::{Owner, Registry, Resources};
    use bae_model::{AuthorId, Stage};
    use bae_ui::action::ids;
    use bae_ui::{
        ActionInfo, Invocation, Keymap, Node, Panel, PanelContext, PanelKind, Workbench,
        WorkbenchContext, WorkbenchState, WorkspacePreset,
    };
    use egui::{Context, Event, Key, Modifiers, RawInput};
    use serde_json::json;

    use crate::files::{Admitted, CommitGate};

    struct Field {
        text: String,
        focus: bool,
    }

    impl Panel for Field {
        fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
            let resp =
                ui.add(egui::TextEdit::singleline(&mut self.text).id(egui::Id::new("value")));
            if std::mem::take(&mut self.focus) {
                resp.request_focus();
            }
            if resp.lost_focus() {
                cx.invoke_with("attr.set_at", json!({ "value": self.text }));
            }
        }
    }

    fn workbench() -> Workbench {
        let owner = Owner::module("test");
        let kinds = Arc::new(Registry::<PanelKind>::new());
        kinds
            .register(
                owner.clone(),
                "field",
                PanelKind::new("Field", |_| {
                    Box::new(Field {
                        text: String::new(),
                        focus: true,
                    })
                }),
            )
            .unwrap();
        let presets = Arc::new(Registry::<WorkspacePreset>::new());
        presets
            .register(
                owner.clone(),
                "ws",
                WorkspacePreset::new("WS", "ae", Node::tabs(&["field"])),
            )
            .unwrap();
        let keymaps = Arc::new(Registry::<Keymap>::new());
        keymaps
            .register(owner, "ae", bae_ui::presets::ae_keymap())
            .unwrap();
        Workbench::with_registries(
            kinds,
            presets,
            keymaps,
            Arc::new(Registry::<ActionInfo>::new()),
            WorkbenchState::default(),
        )
    }

    /// One UI frame; returns what reaches the shell (as `Shell::redraw` collects it).
    fn frame(ctx: &Context, wb: &mut Workbench, events: Vec<Event>) -> Vec<Invocation> {
        let (stage, author, resources) = (Stage::new(), AuthorId::system(), Resources::new());
        let cx = WorkbenchContext {
            stage: &stage,
            author: &author,
            resources: &resources,
            title: "BAE",
            dirty: false,
        };
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = Vec::new();
        let mut full = ctx.run_ui(input, |ui| out = wb.ui(ui, &cx, |_| {}));
        full.textures_delta.clear();
        out.into_iter().filter(|inv| !wb.handle(inv, &cx)).collect()
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

    fn ids(list: &[Invocation]) -> Vec<&str> {
        list.iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn enter_and_save_in_one_frame_save_after_the_edit() {
        let ctx = bae_ui::testing::ctx_with_fonts();
        let mut wb = workbench();
        frame(&ctx, &mut wb, vec![]);
        frame(&ctx, &mut wb, vec![]);
        assert!(ctx.egui_wants_keyboard_input(), "the field is being edited");
        let batch = frame(
            &ctx,
            &mut wb,
            vec![
                Event::Text("500".into()),
                key(Key::Enter, Modifiers::NONE),
                key(Key::S, Modifiers::COMMAND),
            ],
        );
        // What used to be dispatched in this order: the save first.
        assert_eq!(ids(&batch), [ids::SAVE, "attr.set_at"]);
        let mut gate = CommitGate::default();
        let admitted = gate.admit(batch, ctx.egui_wants_keyboard_input(), 3);
        assert_eq!(ids(&admitted.run), ["attr.set_at", ids::SAVE]);
        assert!(!admitted.stop_editing && !gate.is_waiting());
    }

    #[test]
    fn save_while_typing_commits_the_value_first() {
        let ctx = bae_ui::testing::ctx_with_fonts();
        let mut wb = workbench();
        frame(&ctx, &mut wb, vec![]);
        frame(&ctx, &mut wb, vec![]);
        let batch = frame(
            &ctx,
            &mut wb,
            vec![Event::Text("500".into()), key(Key::S, Modifiers::COMMAND)],
        );
        // Cmd/Ctrl+S fires while the field still has the value uncommitted.
        assert_eq!(ids(&batch), [ids::SAVE]);
        assert!(ctx.egui_wants_keyboard_input());
        let mut gate = CommitGate::default();
        let admitted = gate.admit(batch, true, 3);
        assert_eq!(
            admitted,
            Admitted {
                run: vec![],
                stop_editing: true
            }
        );
        // Nothing runs before the next frame, even if the shell asks again.
        assert!(gate.admit(vec![], false, 3).run.is_empty());
        // The shell ends the edit; the next frame commits the typed value.
        ctx.memory_mut(|m| m.stop_text_input());
        let batch = frame(&ctx, &mut wb, vec![]);
        assert_eq!(
            batch,
            [Invocation::with_params(
                "attr.set_at",
                json!({"value": "500"})
            )]
        );
        let admitted = gate.admit(batch, ctx.egui_wants_keyboard_input(), 4);
        assert_eq!(ids(&admitted.run), ["attr.set_at", ids::SAVE]);
        assert!(!gate.is_waiting());
    }

    #[test]
    fn quit_waits_too_and_other_commands_are_not_held() {
        let mut gate = CommitGate::default();
        let batch = vec![
            Invocation::new(ids::QUIT),
            Invocation::new(ids::UNDO),
            Invocation::new(ids::OPEN_PANEL),
        ];
        let admitted = gate.admit(batch, true, 7);
        assert_eq!(ids(&admitted.run), [ids::UNDO, ids::OPEN_PANEL]);
        assert!(admitted.stop_editing && gate.is_waiting());
        let admitted = gate.admit(vec![Invocation::new(ids::SAVE)], false, 8);
        assert_eq!(ids(&admitted.run), [ids::QUIT, ids::SAVE]);
        // A window that never redraws: the shell releases them after a timeout.
        assert!(
            gate.admit(vec![Invocation::new(ids::SAVE)], true, 9)
                .stop_editing
        );
        assert_eq!(ids(&gate.release()), [ids::SAVE]);
        assert!(!gate.is_waiting());
        assert_eq!(ids(&gate.admit(vec![], false, 9).run), Vec::<&str>::new());
    }
}

/// The 5090 steps through the real motion UI (workbench, motion panels and
/// keymap, the shell's dispatch order), without a window: select the rectangle,
/// P, Alt+Shift+P at frame 0, PageDown ×3, Alt+Shift+P, then save and reopen.
mod ui_flow {
    use std::sync::Arc;

    use bae_model::Time;
    use bae_ui::history::HistoryLog;
    use bae_ui::{Invocation, WorkbenchState};
    use egui::{Context, Event, Key, Modifiers, RawInput};
    use serde_json::json;

    use super::{COMP, LAYER, POSITION, SEC, assert_reads_back, layer, position};
    use crate::document::{Document, Outcome};
    use crate::ui::{StatusInfo, Workspace};

    struct Ui {
        app: bae_app::App,
        workspace: Workspace,
        doc: Document,
        ctx: Context,
    }

    impl Ui {
        fn new() -> Self {
            let mut builder = bae_app::AppBuilder::new();
            builder
                .add_module(bae_ui::UiModule)
                .add_module(bae_ui_motion::MotionUiModule);
            let mut app = builder.build();
            let mut doc = Document::new(app.resource::<Arc<HistoryLog>>().cloned());
            doc.add_module_commands(&app);
            let workspace = Workspace::new(&mut app, WorkbenchState::default());
            Ui {
                app,
                workspace,
                doc,
                ctx: bae_ui::testing::ctx_with_fonts(),
            }
        }

        fn run(&mut self, id: &str, params: serde_json::Value) {
            match self.doc.run(&Invocation::with_params(id, params)) {
                Outcome::Changed => {}
                other => panic!("{id}: {other:?}"),
            }
        }

        /// One frame, then the frame's invocations dispatched like `Shell::dispatch`.
        fn frame(&mut self, events: Vec<Event>) -> Vec<String> {
            let input = RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1000.0),
                )),
                events,
                ..Default::default()
            };
            let status = StatusInfo::default();
            let resources = self.app.resources();
            let mut invocations = Vec::new();
            let (workspace, doc) = (&mut self.workspace, &self.doc);
            let mut full = self.ctx.run_ui(input, |ui| {
                invocations = workspace.ui(ui, &status, doc, resources);
            });
            full.textures_delta.clear();
            let mut ran = Vec::new();
            for inv in invocations {
                if let Some(inv) = self.workspace.handle(inv, &self.doc, resources) {
                    ran.push(format!("{} {}", inv.id, inv.params));
                    if let Outcome::Failed(e) = self.doc.run(&inv) {
                        panic!("{}: {e}", inv.id);
                    }
                }
            }
            ran
        }

        fn press(&mut self, key: Key, modifiers: Modifiers) -> Vec<String> {
            let mut ran = self.frame(vec![Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }]);
            // Commands a handler queues run after the next frame.
            ran.extend(self.frame(vec![]));
            ran
        }
    }

    #[test]
    fn alt_shift_p_twice_then_save_keeps_both_keyframes() {
        let dir = tempfile::tempdir().unwrap();
        let mut ui = Ui::new();
        ui.run(
            "comp.create",
            json!({"name": "Main", "fps": 30, "duration": 5 * SEC}),
        );
        ui.run(
            "layer.create",
            json!({"comp": COMP, "kind": "shape", "shape": "rect", "name": "Rect"}),
        );
        {
            let session = ui
                .app
                .resource::<bae_ui_motion::SharedSession>()
                .unwrap()
                .clone();
            let mut s = session.lock();
            s.open_comp(&COMP.parse().unwrap());
            s.selected_layers = vec![layer()];
        }
        ui.frame(vec![]);
        ui.frame(vec![]);
        let mut log = Vec::new();
        log.extend(ui.press(Key::P, Modifiers::NONE));
        let alt_shift = Modifiers {
            alt: true,
            shift: true,
            ..Modifiers::NONE
        };
        log.extend(ui.press(Key::P, alt_shift));
        for _ in 0..3 {
            log.extend(ui.press(Key::PageDown, Modifiers::NONE));
        }
        log.extend(ui.press(Key::P, alt_shift));
        let want = position(&ui.doc);
        let frame3 = Time(3 * SEC / 30);
        assert_eq!(
            want.keyframes.keys().copied().collect::<Vec<_>>(),
            [Time(0), frame3],
            "in memory after {log:#?}"
        );
        for name in ["kf2.bae", "kf2.baez"] {
            let path = dir.path().join(name);
            ui.doc.save_as(&path).unwrap();
            assert_reads_back(&path, &want);
        }
        let _ = (LAYER, POSITION);
    }

    /// The 5090 final acceptance: keys go to the focused widget before the
    /// motion shortcuts. Space plays with nothing focused; in the Render Queue's
    /// output field it types a space (paths with spaces) and does not play.
    #[test]
    fn space_types_in_text_fields_and_plays_otherwise() {
        let mut ui = Ui::new();
        ui.run(
            "comp.create",
            json!({"name": "Main", "fps": 30, "duration": 5 * SEC}),
        );
        let session = ui
            .app
            .resource::<bae_ui_motion::SharedSession>()
            .unwrap()
            .clone();
        session
            .lock()
            .open_comp(&bae_model::PrimPath::new(COMP).unwrap());
        ui.frame(vec![]);
        ui.frame(vec![]);
        assert!(!ui.ctx.text_edit_focused());
        ui.press(Key::Space, Modifiers::NONE);
        assert!(session.lock().is_playing(), "Space plays");
        ui.press(Key::Space, Modifiers::NONE);
        assert!(!session.lock().is_playing());

        // Composition > Add to Render Queue, then Tab to its output field. These
        // modifiers are Ctrl on Windows and ⌃⌘ on macOS, where AE moves the
        // command off the system's ⌘M.
        ui.press(Key::M, Modifiers::COMMAND | Modifiers::CTRL);
        let queue = ui
            .app
            .resource::<bae_ui_motion::SharedRenderQueue>()
            .unwrap()
            .clone();
        assert_eq!(queue.lock().items().len(), 1);
        let before = queue.lock().items()[0].output.clone();
        let mut typed = false;
        for _ in 0..300 {
            ui.press(Key::Tab, Modifiers::NONE);
            if !ui.ctx.text_edit_focused() {
                continue;
            }
            ui.frame(vec![
                Event::Key {
                    key: Key::Space,
                    physical_key: Some(Key::Space),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                Event::Text(" ".into()),
                Event::Text("v2".into()),
            ]);
            assert!(
                !session.lock().is_playing(),
                "Space in a text field must not play"
            );
            if queue.lock().items()[0].output != before {
                typed = true;
                break;
            }
        }
        assert!(typed, "Tab reaches the Render Queue output field");
        assert!(
            queue.lock().items()[0].output.contains(" v2"),
            "{}",
            queue.lock().items()[0].output
        );
    }
}
