//! The AE shortcut sweep: every AE default shortcut of the command catalog
//! (`bae_ui_motion::catalog`, docs/ui/ae-commands.md) pressed the way the
//! platform's windowing delivers it, with the panel of its AE context focused.
//!
//! - Every working command runs its catalog action, exactly once (no other
//!   binding, and no egui default such as Cmd+Shift+C turned into Copy, #220,
//!   takes the key).
//! - A command that does not run yet has no binding: its key does nothing.
//! - No two actions share a chord in one scope.
//! - A menu item's shortcut is bound exactly when its command runs.
//!
//! Uses this platform's AE keys (macOS: AE's Mac file). With
//! `BAE_SHORTCUT_REPORT=<file>` the sweep writes a Markdown coverage report
//! (CI uploads it as an artifact).

use std::fmt::Write as _;

use bae_ui::{Binding, Chord};
use bae_ui_motion::catalog::{self, BOUND_ELSEWHERE, Key as AeKey, NATIVE, SHADOWED};
use egui::{Event, Key, Modifiers};

use super::*;

/// The modifiers egui reports for a chord on this platform.
fn modifiers(c: &Chord) -> Modifiers {
    let mac = cfg!(target_os = "macos");
    Modifiers {
        alt: c.alt,
        ctrl: (c.command && !mac) || c.mac_ctrl,
        shift: c.shift,
        mac_cmd: c.command && mac,
        command: c.command,
    }
}

/// The events egui-winit sends for a chord: Cmd/Ctrl+C / X / V (with any other
/// modifiers) arrive as Copy / Cut / Paste without a key event.
fn platform_events(c: &Chord) -> Vec<Event> {
    let mods = modifiers(c);
    let mut v = vec![Event::ModifiersChanged(mods)];
    let clipboard = if c.command {
        match c.key {
            Key::C => Some(Event::Copy),
            Key::X => Some(Event::Cut),
            Key::V => Some(Event::Paste("x".into())),
            _ => None,
        }
    } else {
        None
    };
    match clipboard {
        Some(e) => v.push(e),
        None => {
            for pressed in [true, false] {
                v.push(Event::Key {
                    key: c.key,
                    physical_key: Some(c.key),
                    pressed,
                    repeat: false,
                    modifiers: mods,
                });
            }
        }
    }
    v.push(Event::ModifiersChanged(Modifiers::NONE));
    v
}

fn params(k: &AeKey) -> serde_json::Value {
    if k.params.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(k.params).unwrap()
    }
}

/// One AE shortcut of the catalog for this platform.
struct Row {
    key: &'static AeKey,
    chord: Chord,
    scope: Option<&'static str>,
}

/// Every AE chord of the catalog BAE can bind on this platform.
fn rows() -> Vec<Row> {
    let mac = cfg!(target_os = "macos");
    let mut out = Vec::new();
    for k in catalog::keys() {
        if k.flags & (NATIVE | SHADOWED) != 0 {
            continue;
        }
        for c in k.chords(mac) {
            if let Ok(chord) = Chord::parse(&c) {
                out.push(Row {
                    key: k,
                    chord,
                    scope: k.scope(),
                });
            }
        }
    }
    out
}

/// Whether the command behind a catalog key runs in this app: done in the
/// catalog, or (another agent's command) bound by its own keymap extension.
fn expected_binding<'a>(keymap: &'a bae_ui::Keymap, row: &Row) -> Option<&'a Binding> {
    keymap.bindings.iter().find(|b| {
        b.chord == row.chord
            && b.when.as_deref() == row.scope
            && b.action == row.key.action
            && b.params == params(row.key)
    })
}

/// The panel to press `row` in: its own scope, or for an application-wide key
/// the first of Timeline, Composition, Project, Effect Controls where no panel
/// binding takes the chord first (AE: a panel's shortcuts win while it has
/// focus, e.g. 1 is Go to Comp Marker in the Timeline, the Orbit tool elsewhere).
fn panel_for(keymap: &bae_ui::Keymap, row: &Row) -> &'static str {
    if let Some(scope) = row.scope {
        return scope;
    }
    ["timeline", "composition", "project", "effect_controls"]
        .into_iter()
        .find(|p| {
            keymap
                .lookup(&row.chord, Some(p))
                .is_none_or(|b| b.when.is_none())
        })
        .unwrap_or("timeline")
}

/// What pressing `chord` in a fresh app with `panel` focused dispatched.
fn press(chord: &Chord, panel: &str) -> Option<Vec<Invocation>> {
    let mut h = gui();
    let real = h
        .state()
        .app
        .get_registry::<bae_ui::PanelKind>()
        .and_then(|r| r.get(panel))
        .is_some_and(|k| !k.is_placeholder);
    if !real {
        return None;
    }
    if !h
        .state()
        .workspace
        .open_panels()
        .iter()
        .any(|p| p.kind() == panel)
    {
        // Registered but not in the layout (Render Queue): open it.
        h.state_mut().invoke(Invocation::with_params(
            bae_ui::action::ids::OPEN_PANEL,
            serde_json::json!({ "id": panel }),
        ));
        settle(&mut h);
    }
    h.state_mut().workspace.focus_panel(panel);
    if h.state().workspace.focused_kind().as_deref() != Some(panel) {
        return None;
    }
    settle(&mut h);
    let before = h.state().dispatched.len();
    for e in platform_events(chord) {
        send(&mut h, e);
    }
    settle(&mut h);
    Some(h.state().dispatched[before..].to_vec())
}

#[derive(Default)]
struct Report {
    total: usize,
    working: usize,
    passed: usize,
    unbound: usize,
    missing_panel: Vec<String>,
    failures: Vec<String>,
    conflicts: Vec<String>,
}

impl Report {
    fn markdown(&self) -> String {
        let mut s = String::new();
        let os = if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Windows"
        };
        let _ = writeln!(s, "# AE 快捷键扫描（{os}，CI）\n");
        let _ = writeln!(s, "- 目录里 BAE 能绑定的 AE 快捷键：{} 条", self.total);
        let _ = writeln!(
            s,
            "- 命令已实现、已绑定：{} 条，按下后正确运行：{} 条",
            self.working, self.passed
        );
        let _ = writeln!(
            s,
            "- 命令未实现（不绑定，按下什么都不做）：{} 条",
            self.unbound
        );
        let _ = writeln!(
            s,
            "- 作用域不是 BAE 已有的面板（素材、图层、流程图面板，跟踪器，文字编辑、绘画 / 仿制图章 / 摄像机工具状态），没测：{} 条",
            self.missing_panel.len()
        );
        for (title, list) in [
            ("失败", &self.failures),
            ("冲突", &self.conflicts),
            ("没测（作用域不是已有的面板）", &self.missing_panel),
        ] {
            let _ = writeln!(s, "\n## {title}（{}）\n", list.len());
            for l in list {
                let _ = writeln!(s, "- {l}");
            }
        }
        s
    }
}

/// The whole sweep (one test: the report covers every row).
#[test]
fn every_ae_shortcut_runs_its_catalog_action() {
    let keymap = gui().state().workspace.keymap().clone();
    let mut r = Report::default();
    for c in keymap.conflicts() {
        r.conflicts.push(format!("{c:?}"));
    }
    let rows = rows();
    r.total = rows.len();
    for row in &rows {
        let what = format!(
            "`{}/{}` {} → `{}` {}",
            row.key.context, row.key.command, row.chord, row.key.action, row.key.params
        );
        let bound = expected_binding(&keymap, row);
        let panel = panel_for(&keymap, row);
        if bound.is_none() {
            // Not running yet (or the chord stays with its own panel): the key
            // must not run the command.
            if row.key.status.works()
                && row.key.flags & BOUND_ELSEWHERE == 0
                && catalog::in_own_panel(row.key)
            {
                r.failures
                    .push(format!("{what}: works in the catalog but is not bound"));
                continue;
            }
            r.unbound += 1;
            if keymap.lookup(&row.chord, Some(panel)).is_some() {
                // A binding holds the chord in the panel pressed (another
                // command, or this one scoped to its own panel): not a no-op test.
                continue;
            }
            match press(&row.chord, panel) {
                None => r.missing_panel.push(what),
                Some(ran) => {
                    if ran.iter().any(|i| i.id == row.key.action) {
                        r.failures.push(format!("{what}: not implemented but ran"));
                    }
                }
            }
            continue;
        }
        r.working += 1;
        let Some(ran) = press(&row.chord, panel) else {
            r.missing_panel.push(what);
            continue;
        };
        let want = params(row.key);
        let hits = ran
            .iter()
            .filter(|i| i.id == row.key.action && (i.params == want || want.is_null()))
            .count();
        if hits == 1 {
            r.passed += 1;
        } else {
            let got: Vec<String> = ran
                .iter()
                .map(|i| format!("{} {}", i.id, i.params))
                .collect();
            r.failures
                .push(format!("{what}: ran {hits} times; dispatched {got:?}"));
        }
    }
    let report = r.markdown();
    if let Some(path) = std::env::var_os("BAE_SHORTCUT_REPORT") {
        std::fs::write(path, &report).unwrap();
    }
    println!("{report}");
    assert!(r.failures.is_empty() && r.conflicts.is_empty(), "{report}");
    assert!(r.passed > 100, "{report}");
}

/// Digits follow the focused panel like AE: in the Timeline 1 goes to comp
/// marker 1, in the Composition viewer it picks the Orbit camera tool.
#[test]
fn digits_follow_the_focused_panel() {
    let one = Chord::parse("1").unwrap();
    let ran = |panel| press(&one, panel).unwrap_or_default();
    let timeline = ran("timeline");
    assert!(
        timeline
            .iter()
            .any(|i| i.id == "motion.timeline.go_to_comp_marker" && i.params["number"] == 1),
        "{timeline:?}"
    );
    let viewer = ran("composition");
    assert!(
        viewer.iter().any(|i| i.id == "motion.tool.camera_orbit"),
        "{viewer:?}"
    );
}

/// A menu item's shortcut is bound exactly when its command runs: an item
/// greyed for want of a handler shows no shortcut, and an item that runs shows
/// AE's shortcut when the catalog has one.
#[test]
fn menu_items_and_their_shortcuts_agree() {
    let h = gui();
    let st = h.state();
    let keymap = st.workspace.keymap();
    let handled = st.workspace.handled_actions(&st.app);
    let tree = st.workspace.contributed_menus(&st.app);
    let mac = cfg!(target_os = "macos");
    let mut wrong = Vec::new();
    for (id, spec) in tree.items() {
        let inv = spec.invocation();
        let shown = keymap.chord_for_invocation(&inv);
        let live = !spec.handler_needed || handled.contains(&spec.handler_key());
        if !live && let Some(c) = shown {
            wrong.push(format!("{id}: greyed (no handler) but shows {c}"));
        }
        let ae = catalog::keys().iter().find(|k| {
            k.status.works()
                && k.flags & (NATIVE | SHADOWED) == 0
                && k.action == inv.id
                && params(k) == inv.params
                && k.chords(mac).iter().any(|c| Chord::parse(c).is_ok())
        });
        if live && ae.is_some() && shown.is_none() {
            wrong.push(format!(
                "{id}: runs, AE has a shortcut, the menu shows none"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
