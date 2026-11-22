//! 0.0.1 final regression (macOS, dev 160fab5), section B: the AE parity P0
//! round checked against the AE 26 command catalog (`docs/ui/ae-commands.md`,
//! generated from `bae-ui-motion`'s catalog and AE's own menus and shortcut
//! files), from the user's side:
//!
//! - every implemented command with a macOS AE shortcut runs once when the
//!   shortcut is pressed, and the menu shows that shortcut;
//! - every enabled menu item does something (no item that is clickable but has
//!   no handler); the Effect menu is greyed (no effects in 0.0.1).

use egui::{Key, Modifiers};

use super::*;

const CATALOG: &str = include_str!("../../../../docs/ui/ae-commands.md");

/// One implemented (✅ / ◐) catalog row.
#[derive(Debug, Clone)]
struct Row {
    /// AE menu path or AE command name.
    what: String,
    action: String,
    params: serde_json::Value,
    /// macOS shortcuts (alternatives).
    mac: Vec<String>,
    /// Panel kind the shortcut works in (empty: any).
    scope: String,
    status: String,
}

fn catalog_rows() -> Vec<Row> {
    let mut out = Vec::new();
    let mut shortcut_only = false;
    for line in CATALOG.lines() {
        if line.starts_with("## 只有快捷键") {
            shortcut_only = true;
        }
        if !line.starts_with("| ") || line.starts_with("|---") {
            continue;
        }
        let c: Vec<&str> = line.trim_matches('|').split(" | ").map(str::trim).collect();
        if c.len() != 8 {
            continue;
        }
        let (what, action, params, mac, scope, status) = if shortcut_only {
            (c[0], c[1], c[2], c[4], c[5], c[7])
        } else {
            (c[0], c[2], c[3], c[5], "", c[7])
        };
        let action = action.trim_matches('`');
        if !action.contains('.') {
            continue;
        }
        let params = params.trim_matches('`');
        let params = if params.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(params).unwrap_or(serde_json::Value::Null)
        };
        let mac = mac
            .split(" / ")
            .map(|s| s.replace("\\`", "`").replace("\\\\", "\\"))
            .filter(|s| !s.is_empty())
            .collect();
        out.push(Row {
            what: what.trim_matches('`').to_owned(),
            action: action.to_owned(),
            params,
            mac,
            scope: scope.to_owned(),
            status: status.to_owned(),
        });
    }
    out
}

fn params_match(want: &serde_json::Value, got: &serde_json::Value) -> bool {
    match want.as_object() {
        None => true,
        Some(w) => w.iter().all(|(k, v)| got.get(k) == Some(v)),
    }
}

/// A comp with a solid, a text and a rectangle shape layer; the top layer is
/// selected and has two position keyframes; the time is at 0.5 s.
fn rich() -> Ui {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
    assert!(add_layer(&mut h, "shape:rect"));
    settle(&mut h);
    let top = h.state().layers()[0].clone();
    h.state().session().lock().selected_layers = vec![top.clone()];
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    h.state_mut().invoke(Invocation::with_params(
        "attr.set_at",
        serde_json::json!({"path": top, "attr": "transform:position",
                           "time": bae_model::Time::FLICKS_PER_SECOND,
                           "value": {"type": "vec3", "value": [1200.0, 300.0, 0.0]}}),
    ));
    settle(&mut h);
    let comp = h.state().active_comp().unwrap();
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    h.state()
        .session()
        .lock()
        .set_time(&info, bae_model::Time::from_seconds(0.5));
    settle(&mut h);
    h
}

const TIMELINE: (&str, egui::Vec2) = ("Layer Name", egui::vec2(700.0, 120.0));
const VIEWER: (&str, egui::Vec2) = ("Fit", egui::vec2(300.0, -250.0));

/// Enabled menu items that do nothing in the state of [`rich`] (a layer
/// selected, no keyframe selected), with their issue. Remove when fixed.
const KNOWN_NOOPS: &[(&str, &str)] = &[];

/// Clicks an empty part of the panel whose header shows `anchor`.
fn focus(h: &mut Ui, anchor: &str, offset: egui::Vec2) {
    let at =
        find_text(h, anchor).unwrap_or_else(|| panic!("no {anchor:?}: {:?}", painted_texts(h)));
    click_at(h, at.center() + offset);
}

fn egui_modifiers(c: &bae_ui::keymap::Chord) -> Modifiers {
    Modifiers {
        alt: c.alt,
        ctrl: c.mac_ctrl,
        shift: c.shift,
        mac_cmd: c.command,
        command: c.command,
    }
}

#[test]
fn every_implemented_ae_shortcut_runs_its_command_once() {
    let rows = catalog_rows();
    let mut problems = Vec::new();
    let (mut tried, mut skipped) = (0, Vec::new());
    for row in rows
        .iter()
        .filter(|r| matches!(r.status.as_str(), "✅" | "◐"))
    {
        // Panels the harness can focus; the others are covered by their own tests.
        // Viewer actions work in the viewer (AE binds them to its viewer
        // contexts only); the rest with the Timeline focused.
        let target = match row.scope.as_str() {
            _ if row.action.starts_with("viewer.") => VIEWER,
            "" | "timeline" => TIMELINE,
            "composition" => VIEWER,
            other => {
                skipped.push(format!("{} ({other})", row.action));
                continue;
            }
        };
        // Window-system items the app menu owns (Hide, Quit …): not the keymap's.
        if row.action.starts_with("app.") && row.action != "app.settings" {
            continue;
        }
        for alt in &row.mac {
            let Ok(chord) = bae_ui::keymap::Chord::parse(alt) else {
                skipped.push(format!("{} “{alt}” (key name)", row.action));
                continue;
            };
            tried += 1;
            let mut h = rich();
            focus(&mut h, target.0, target.1);
            // The click on the empty graph clears the selection: select again.
            let top = h.state().layers()[0].clone();
            h.state().session().lock().selected_layers = vec![top];
            let before = h.state().dispatched.len();
            key(&mut h, egui_modifiers(&chord), chord.key);
            let after: Vec<Invocation> = h.state().dispatched[before..].to_vec();
            let hits = after
                .iter()
                .filter(|i| i.id == row.action && params_match(&row.params, &i.params))
                .count();
            if hits != 1 {
                problems.push(format!(
                    "{} “{alt}” → {} expected once, ran {hits}×; dispatched {:?}",
                    row.what,
                    row.action,
                    after.iter().map(|i| i.id.as_str()).collect::<Vec<_>>()
                ));
            }
        }
    }
    println!("shortcuts tried: {tried}; skipped: {skipped:?}");
    assert!(tried > 100, "only {tried} shortcuts tried");
    assert!(problems.is_empty(), "{}\n", problems.join("\n"));
}

#[test]
fn menus_show_the_ae_shortcuts() {
    let rows = catalog_rows();
    let h = rich();
    let items = h.state().menu_items();
    let mut problems = Vec::new();
    for row in rows
        .iter()
        .filter(|r| matches!(r.status.as_str(), "✅" | "◐"))
    {
        let Some(first) = row.mac.first() else {
            continue;
        };
        let Ok(want) = bae_ui::keymap::Chord::parse(first) else {
            continue;
        };
        // The native menu leaves out keys that type text (`.`, U, Space …): they
        // would fire inside text fields (the harness filters them like
        // `AppMenu::sync`). The app menu's own items are the system's.
        if want.is_typing_key() || row.action.starts_with("app.") {
            continue;
        }
        for item in items.iter().filter(|i| {
            i.invocation.id == row.action && params_match(&row.params, &i.invocation.params)
        }) {
            let want = want.to_string();
            if item.chord.as_deref() != Some(want.as_str()) {
                problems.push(format!(
                    "{} ({}): menu shows {:?}, AE {want}",
                    item.path.join(" ▸ "),
                    row.action,
                    item.chord
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every enabled menu item, run on its own in a fresh project with a comp,
/// layers, a selection and keyframes: none may be clickable yet unhandled
/// (the document answers "unknown"), and none may panic.
#[test]
fn every_enabled_menu_item_does_something() {
    let items = rich().state().menu_items();
    let mut problems = Vec::new();
    let mut ran = 0;
    for item in items.iter().filter(|i| i.enabled) {
        // Quit and the native app menu items (AppKit handles Services, Hide,
        // Show All) are covered by their own tests.
        if item.invocation.id.starts_with("app.") && item.invocation.id != "app.settings" {
            continue;
        }
        let mut h = rich();
        if item.invocation.id.starts_with("viewer.") {
            focus(&mut h, VIEWER.0, VIEWER.1);
        }
        let before = h.state().outcomes.len();
        assert!(h.state_mut().menu(&item.id), "{} disabled again", item.id);
        settle(&mut h);
        ran += 1;
        for (id, outcome) in &h.state().outcomes[before..] {
            if outcome == "unknown" && !KNOWN_NOOPS.iter().any(|(k, _)| k == id) {
                problems.push(format!(
                    "{} ({id}): enabled, but nothing handles it",
                    item.path.join(" ▸ ")
                ));
            }
        }
    }
    println!("enabled menu items run: {ran} of {}", items.len());
    // 0.0.1 has no effects: the Effect menu is greyed as a whole.
    let effect = bae_ui::tr("Effect");
    for item in items
        .iter()
        .filter(|i| i.path.first().is_some_and(|p| p == "Effect" || p == effect))
    {
        // Effect ▸ Effect Controls opens the panel (enabled in AE as well).
        if item.enabled && item.invocation.id != "window.panel" {
            problems.push(format!(
                "Effect menu item enabled: {}",
                item.path.join(" ▸ ")
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// ---------------------------------------------------------------------------
// B3: tools in the Composition viewer.

/// The Composition viewer's image area.
fn canvas(h: &Ui) -> Rect {
    use egui_kittest::kittest::Queryable as _;
    h.root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer")
}

fn drag(h: &mut Ui, from: Pos2, to: Pos2, modifiers: Modifiers) {
    send(h, Event::PointerMoved(from));
    send(h, Event::ModifiersChanged(modifiers));
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    };
    send(h, button(from, true));
    for i in 1..=6 {
        send(
            h,
            Event::PointerMoved(from + (to - from) * (i as f32 / 6.0)),
        );
    }
    send(h, button(to, false));
    send(h, Event::ModifiersChanged(Modifiers::NONE));
    settle(h);
}

/// Display names of the prims under `layer/<folder>/`, by path.
fn under(h: &Ui, layer: &PrimPath, folder: &str) -> Vec<(String, String)> {
    let stage = h.state().stage();
    let root = format!("{}/{folder}/", layer.as_str());
    stage
        .traverse()
        .into_iter()
        .filter(|p| p.as_str().starts_with(&root))
        .map(|p| {
            let name = bae_motion::query::display_name_of(stage, &p);
            (p.as_str().to_owned(), name)
        })
        .collect()
}

fn no_failures(h: &Ui, step: &str) {
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(_, o)| o.starts_with("failed"))
        .collect();
    assert!(failed.is_empty(), "{step}: {failed:?}");
}

fn deselect(h: &Ui) {
    let s = h.state().session();
    let mut s = s.lock();
    s.selected_layers.clear();
    s.selected_keys.clear();
}

/// Undo then redo: the last action is one undo step.
fn one_undo_step(h: &mut Ui, step: &str, count: impl Fn(&Ui) -> usize) {
    let after = count(h);
    key(h, Modifiers::COMMAND, Key::Z);
    let undone = count(h);
    key(h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert!(
        undone < after,
        "{step}: Cmd+Z did not undo it ({after} → {undone})"
    );
    assert_eq!(count(h), after, "{step}: Cmd+Shift+Z did not redo it");
}

#[test]
fn viewer_tools_create_shapes_masks_paths_and_text() {
    let mut h = gui();
    comp_with(&mut h, &[]);
    let c = canvas(&h);
    let at = |fx: f32, fy: f32| c.min + egui::vec2(c.width() * fx, c.height() * fy);
    let layers = |h: &Ui| h.state().layers().len();

    // Rectangle, Rounded Rectangle, Ellipse, Polygon, Star: Q and Q again
    // (AE cycles the flyout); a drag with nothing selected makes a shape layer.
    let groups = ["Rectangle", "Rectangle", "Ellipse", "Polystar", "Polystar"];
    for (i, group) in groups.iter().enumerate() {
        key(&mut h, Modifiers::NONE, Key::Q);
        deselect(&h);
        let n = layers(&h);
        drag(&mut h, at(0.3, 0.3), at(0.45, 0.5), Modifiers::NONE);
        assert_eq!(layers(&h), n + 1, "shape tool {i}: no new layer");
        let top = h.state().layers()[0].clone();
        let names: Vec<String> = under(&h, &top, "contents")
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert!(
            names.iter().any(|n| n == &format!("{group} 1")),
            "shape tool {i}: contents {names:?}"
        );
        one_undo_step(&mut h, &format!("shape tool {i}"), layers);
        no_failures(&h, &format!("shape tool {i}"));
    }

    // Rectangle tool with Shift: a square.
    key(&mut h, Modifiers::NONE, Key::Q); // Star → back round to Rectangle
    deselect(&h);
    drag(&mut h, at(0.2, 0.2), at(0.5, 0.3), Modifiers::SHIFT);
    let square = h.state().layers()[0].clone();
    let rect_path = under(&h, &square, "contents")
        .into_iter()
        .find(|(p, _)| p.ends_with("Rectangle_Path_1"))
        .map(|(p, _)| PrimPath::new(&p).unwrap())
        .unwrap_or_else(|| panic!("rectangle path: {:?}", under(&h, &square, "contents")));
    let size =
        bae_motion::query::static_value(&h.state().stage().prim(&rect_path).unwrap(), "size");
    match size {
        Some(bae_model::Value::Vec2([w, hh])) => {
            assert!((w - hh).abs() < 0.5 && w > 1.0, "Shift-drag size {w}×{hh}")
        }
        other => panic!("rectangle size {other:?}"),
    }

    // A selected shape layer: the drag adds another group to it.
    h.state().session().lock().selected_layers = vec![square.clone()];
    let n = layers(&h);
    drag(&mut h, at(0.6, 0.6), at(0.7, 0.7), Modifiers::NONE);
    assert_eq!(
        layers(&h),
        n,
        "a selected shape layer got a new layer instead"
    );
    let groups = under(&h, &square, "contents")
        .into_iter()
        .filter(|(p, _)| p.matches("/contents/").count() == 1 && !p.ends_with("/contents"))
        .count();
    assert!(groups >= 2, "{:?}", under(&h, &square, "contents"));

    // A selected solid: the shape tools draw a mask on it.
    assert!(add_layer(&mut h, "motion.layer.new.solid"));
    settle(&mut h);
    confirm_dialog(&mut h);
    let solid = h.state().layers()[0].clone();
    h.state().session().lock().selected_layers = vec![solid.clone()];
    let n = layers(&h);
    drag(&mut h, at(0.25, 0.25), at(0.55, 0.55), Modifiers::NONE);
    assert_eq!(
        layers(&h),
        n,
        "the rectangle tool made a layer over a selected solid"
    );
    let masks = |h: &Ui| under(h, &solid, "masks").len();
    assert!(masks(&h) > 0, "no mask on the selected solid");
    one_undo_step(&mut h, "rectangle mask", masks);

    // Pen (G): three clicks and a click on the first point close a mask.
    key(&mut h, Modifiers::NONE, Key::G);
    h.state().session().lock().selected_layers = vec![solid.clone()];
    let before = masks(&h);
    for p in [at(0.6, 0.2), at(0.8, 0.25), at(0.7, 0.45), at(0.6, 0.2)] {
        click_at(&mut h, p);
    }
    assert!(masks(&h) > before, "pen clicks made no mask");
    no_failures(&h, "pen mask");
    // …and with nothing selected, a shape layer with a path.
    deselect(&h);
    let n = layers(&h);
    for p in [at(0.1, 0.6), at(0.3, 0.65), at(0.2, 0.85), at(0.1, 0.6)] {
        click_at(&mut h, p);
    }
    assert_eq!(
        layers(&h),
        n + 1,
        "pen with nothing selected: no shape layer"
    );
    no_failures(&h, "pen shape");

    // Type tool (Cmd+T): click, type (with an IME composition), commit.
    key(&mut h, Modifiers::COMMAND, Key::T);
    deselect(&h);
    let n = layers(&h);
    click_at(&mut h, at(0.5, 0.8));
    type_text(&mut h, "Hi ");
    ime_compose(&mut h, &["n", "ni", "你好"], "你好");
    key(&mut h, Modifiers::COMMAND, Key::Enter);
    assert_eq!(layers(&h), n + 1, "type tool click made no text layer");
    let text = h.state().layers()[0].clone();
    assert_eq!(h.state().name(&text), "Hi 你好");
    no_failures(&h, "type tool");
}

// ---------------------------------------------------------------------------
// B4: dialogs with bad input, names, Esc, undo and a save round trip.

/// The control labeled `label` nearest the window centre (where dialogs open).
fn dialog_control(h: &Ui, label: &str) -> Rect {
    use egui_kittest::kittest::Queryable as _;
    let center = egui::pos2(SIZE[0] / 2.0, SIZE[1] / 2.0);
    h.query_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| {
            a.center()
                .distance(center)
                .total_cmp(&b.center().distance(center))
        })
        .unwrap_or_else(|| panic!("no control labeled {label:?}"))
}

fn type_into(h: &mut Ui, label: &str, text: &str) {
    let r = dialog_control(h, label);
    click_at(h, r.center());
    key(h, Modifiers::COMMAND, Key::A);
    if text.is_empty() {
        key(h, Modifiers::NONE, Key::Backspace);
    } else {
        type_text(h, text);
    }
    key(h, Modifiers::NONE, Key::Tab);
}

/// Cmd+N, `field` set to `text`, Enter straight from the field (OK): the new
/// comp's settings. (Tab then Enter: #205.)
fn new_comp_with(h: &mut Ui, field: &str, text: &str) -> bae_motion::query::CompInfo {
    let (m, k) = cmd(Key::N);
    key(h, m, k);
    assert!(dialog_open(h), "Cmd+N opened no dialog");
    let r = dialog_control(h, field);
    click_at(h, r.center());
    key(h, Modifiers::COMMAND, Key::A);
    if text.is_empty() {
        key(h, Modifiers::NONE, Key::Backspace);
    } else {
        type_text(h, text);
    }
    key(h, Modifiers::NONE, Key::Enter);
    assert!(
        !dialog_open(h),
        "{field} = {text:?}: Enter did not close the dialog"
    );
    let comp = h.state().active_comp().unwrap();
    bae_motion::query::comp_info(h.state().stage(), &comp).unwrap()
}

#[test]
fn dialogs_handle_bad_numbers_names_escape_undo_and_reopen() {
    let mut h = gui();
    let mut report = Vec::new();
    for text in ["0", "-100", "999999", "abc", "", "1e9", "12.7"] {
        let info = new_comp_with(&mut h, "Width", text);
        report.push(format!("Width {text:?} → {}×{}", info.width, info.height));
        assert!(
            (1..=30000).contains(&info.width) && (1..=30000).contains(&info.height),
            "Width {text:?} gave {}×{}",
            info.width,
            info.height
        );
        no_failures(&h, "comp width");
    }
    for text in ["0", "-5", "1000", "abc", ""] {
        let info = new_comp_with(&mut h, "Frame Rate", text);
        report.push(format!(
            "Frame Rate {text:?} → {}",
            info.frame_rate.as_f64()
        ));
        assert!(
            (1.0..=999.0).contains(&info.frame_rate.as_f64()),
            "Frame Rate {text:?} gave {}",
            info.frame_rate.as_f64()
        );
    }
    for text in ["0", "-1", "abc"] {
        let info = new_comp_with(&mut h, "Duration", text);
        report.push(format!("Duration {text:?} → {:?}", info.duration));
        assert!(
            info.duration > bae_model::Time::ZERO,
            "Duration {text:?}: {:?}",
            info.duration
        );
    }
    println!("{}", report.join("\n"));

    // Name with spaces, Chinese (typed and composed) and an emoji.
    let comps = |h: &Ui| bae_motion::query::comps(h.state().stage()).len();
    let n = comps(&h);
    let (m, k) = cmd(Key::N);
    key(&mut h, m, k);
    let r = dialog_control(&h, "Composition Name");
    click_at(&mut h, r.center());
    key(&mut h, Modifiers::COMMAND, Key::A);
    type_text(&mut h, "我的 合成 ");
    ime_compose(&mut h, &["x", "xi"], "戏");
    type_text(&mut h, " ✨");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    let comp = h.state().active_comp().unwrap();
    assert_eq!(h.state().name(&comp), "我的 合成 戏 ✨");
    assert_eq!(comps(&h), n + 1);
    one_undo_step(&mut h, "new comp", comps);

    // Esc cancels: no comp, no undo step.
    let undo = h.state().doc.undo_state().0;
    key(&mut h, m, k);
    type_into(&mut h, "Width", "640");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!dialog_open(&h), "Esc left the dialog open");
    assert_eq!(comps(&h), n + 1, "Esc made a comp");
    assert_eq!(h.state().doc.undo_state().0, undo, "Esc added an undo step");

    // Solid Settings: bad size, a Chinese name, Esc and Enter.
    let layers = |h: &Ui| h.state().layers().len();
    let l = layers(&h);
    key(&mut h, Modifiers::COMMAND, Key::Y);
    assert!(dialog_open(&h), "Cmd+Y opened no Solid Settings");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(layers(&h), l, "Esc made a solid");
    key(&mut h, Modifiers::COMMAND, Key::Y);
    let r = dialog_control(&h, "Name");
    click_at(&mut h, r.center());
    key(&mut h, Modifiers::COMMAND, Key::A);
    type_text(&mut h, "背景 纯色");
    type_into(&mut h, "Width", "0");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(layers(&h), l + 1, "Enter made no solid");
    let solid = h.state().layers()[0].clone();
    assert_eq!(h.state().name(&solid), "背景 纯色");
    no_failures(&h, "solid");

    // Saved and reopened, every setting is the same.
    let tmp = tempfile::tempdir().unwrap();
    let path = tricky_dir(tmp.path()).join("对话框 往返.bae");
    let before = bae_model::export_flattened_json(h.state().stage());
    h.state_mut().dialogs.save.push_back(Some(path.clone()));
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let mut g = gui();
    g.state_mut().dialogs.open.push_back(Some(path));
    key(&mut g, Modifiers::COMMAND, Key::O);
    assert_eq!(bae_model::export_flattened_json(g.state().stage()), before);
}

// ---------------------------------------------------------------------------
// C: the new UI in one language (Settings pages, dialogs, Graph Editor, tool
// options), Chinese and English.

const SETTINGS_PAGES: &[&str] = &[
    "startup_repair",
    "project",
    "composition",
    "previews",
    "appearance",
    "grids_guides",
    "labels",
    "type",
    "import",
    "export",
    "audio",
    "disk",
    "memory_cpu",
    "video",
    "3d",
    "scripting_expressions",
];

fn new_ui_in_one_language(zh: bool) {
    crate::i18n::init_from_env();
    let (mut h, extra_ok) = super::i18n_retest::gui_with_status();
    comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
    let solid = h.state().layers()[1].clone();
    let mut problems = Vec::new();
    let mut scan = |h: &mut Ui, step: &str| {
        settle(h);
        for p in super::i18n_retest::wrong_language(h, zh, &extra_ok) {
            problems.push(format!("{step}: {p}"));
        }
    };
    for page in SETTINGS_PAGES {
        h.state_mut().invoke(Invocation::with_params(
            "app.settings",
            serde_json::json!({ "page": page }),
        ));
        scan(&mut h, &format!("settings {page}"));
        key(&mut h, Modifiers::NONE, Key::Escape);
    }
    // Dialogs: (step, action, needs the solid selected).
    let dialogs: &[(&str, &str, bool)] = &[
        (
            "composition settings",
            "motion.comp.composition_settings",
            false,
        ),
        ("new solid", "motion.layer.new.solid", false),
        ("layer settings", "motion.layer.layer_settings", true),
        ("new camera", "motion.layer.new.camera", false),
        ("new light", "motion.layer.new.light", false),
        ("pre-compose", "motion.layer.pre_compose", true),
    ];
    for (step, action, select) in dialogs {
        if *select {
            h.state().session().lock().selected_layers = vec![solid.clone()];
        }
        let id = h
            .state()
            .menu_items()
            .into_iter()
            .find(|i| i.invocation.id == *action || i.id == *action)
            .map(|i| i.id)
            .unwrap_or_else(|| panic!("no menu item for {action}"));
        assert!(h.state_mut().menu(&id), "{step}: menu item greyed");
        settle(&mut h);
        assert!(dialog_open(&h), "{step}: no dialog");
        scan(&mut h, step);
        key(&mut h, Modifiers::NONE, Key::Escape);
        assert!(!dialog_open(&h), "{step}: Esc left it open");
    }
    // Keyframe dialogs on two selected position keys of the solid.
    h.state().session().lock().selected_layers = vec![solid.clone()];
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    h.state_mut().invoke(Invocation::with_params(
        "attr.set_at",
        serde_json::json!({"path": solid, "attr": "transform:position",
                           "time": bae_model::Time::FLICKS_PER_SECOND,
                           "value": {"type": "vec3", "value": [100.0, 100.0, 0.0]}}),
    ));
    settle(&mut h);
    let keys: Vec<bae_motion::KeyRef> =
        bae_motion::query::attr_spec(h.state().stage(), &solid, "transform:position")
            .unwrap()
            .keyframes
            .keys()
            .map(|t| bae_motion::KeyRef::new(solid.clone(), "transform:position", *t))
            .collect();
    // Keyframe dialogs are Timeline actions: focus it (the click clears the
    // selection, so select the keys afterwards).
    focus(&mut h, bae_ui::tr("Layer Name"), TIMELINE.1);
    for (step, m, k) in [
        (
            "keyframe interpolation",
            Modifiers::COMMAND | Modifiers::ALT,
            Key::K,
        ),
        (
            "keyframe velocity",
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::K,
        ),
    ] {
        h.state().session().lock().selected_keys = keys.clone();
        key(&mut h, m, k);
        // The keyframe dialogs are the Timeline's own (`Session::key_dialog`).
        assert!(
            h.state().session().lock().key_dialog.is_some(),
            "{step}: no dialog"
        );
        scan(&mut h, step);
        key(&mut h, Modifiers::NONE, Key::Escape);
        assert!(
            h.state().session().lock().key_dialog.is_none(),
            "{step}: Esc left it open"
        );
    }
    // Graph Editor and the shape / pen tool options.
    key(&mut h, Modifiers::SHIFT, Key::F3);
    scan(&mut h, "graph editor");
    key(&mut h, Modifiers::SHIFT, Key::F3);
    for (step, k) in [("shape tool options", Key::Q), ("pen tool options", Key::G)] {
        key(&mut h, Modifiers::NONE, k);
        scan(&mut h, step);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn new_ui_has_no_english_in_chinese() {
    if !in_child(
        "acceptance::release_001::new_ui_has_no_english_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    new_ui_in_one_language(true);
}

#[test]
fn new_ui_has_no_chinese_in_english() {
    if !in_child(
        "acceptance::release_001::new_ui_has_no_chinese_in_english",
        &[("BAE_LANG", "en")],
    ) {
        return;
    }
    new_ui_in_one_language(false);
}

// ---------------------------------------------------------------------------
// B8: the round's new layer properties survive Save As .bae / .baez.

#[test]
fn new_layer_properties_survive_save_as_bae_and_baez() {
    let mut h = rich();
    for id in ["motion.layer.new.camera", "motion.layer.new.light"] {
        assert!(add_layer(&mut h, id), "{id}");
        settle(&mut h);
        confirm_dialog(&mut h);
    }
    // A mask on the solid (rectangle tool drag over it).
    let solid = h
        .state()
        .layers()
        .into_iter()
        .find(|l| bae_motion::query::layer_kind(h.state().stage(), l).as_deref() == Some("solid"))
        .unwrap();
    h.state().session().lock().selected_layers = vec![solid.clone()];
    key(&mut h, Modifiers::NONE, Key::Q);
    let c = canvas(&h);
    drag(
        &mut h,
        c.min + c.size() * 0.3,
        c.min + c.size() * 0.6,
        Modifiers::NONE,
    );
    key(&mut h, Modifiers::NONE, Key::V);
    let initial = bae_model::export_flattened_json(h.state().stage());
    // Every enabled item of the Layer submenus (switches, quality, blending
    // modes, 3D …) and Edit ▸ Label, on all layers but the camera.
    let targets: Vec<PrimPath> = h
        .state()
        .layers()
        .into_iter()
        .filter(|l| {
            bae_motion::query::layer_kind(h.state().stage(), l).as_deref() != Some("camera")
        })
        .collect();
    let items = h.state().menu_items();
    let mut ran = Vec::new();
    for item in items.iter().filter(|i| {
        let p: Vec<&str> = i.path.iter().map(String::as_str).collect();
        (p.first() == Some(&"Layer") && p.len() >= 2 && !matches!(p[1], "New" | "Open Layer"))
            || p.starts_with(&["Edit", "Label"])
    }) {
        h.state().session().lock().selected_layers = targets.clone();
        if !h.state_mut().menu(&item.id) {
            continue;
        }
        settle(&mut h);
        if dialog_open(&h) {
            key(&mut h, Modifiers::NONE, Key::Escape);
        }
        ran.push(format!("{} ▸ {}", item.path.join(" ▸ "), item.title));
    }
    println!("ran {} items: {ran:?}", ran.len());
    no_failures(&h, "layer menu items");
    let before = bae_model::export_flattened_json(h.state().stage());
    assert_ne!(before, initial, "the menu items changed nothing");
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    for name in ["属性 往返.bae", "属性 往返.baez"] {
        let path = dir.join(name);
        h.state_mut().dialogs.save.push_back(Some(path.clone()));
        key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
        assert!(path.exists(), "{name} not written");
        let mut g = gui();
        g.state_mut().dialogs.open.push_back(Some(path));
        key(&mut g, Modifiers::COMMAND, Key::O);
        assert!(
            g.state().dialogs.errors.is_empty(),
            "{:?}",
            g.state().dialogs.errors
        );
        assert_eq!(
            bae_model::export_flattened_json(g.state().stage()),
            before,
            "{name}: reopened project differs"
        );
    }
}

// ---------------------------------------------------------------------------
// C: 50 quick repetitions of each new dialog and tool.

#[test]
fn new_dialogs_and_tools_fifty_times_in_a_row() {
    let mut h = gui();
    comp_with(&mut h, &[]);
    let start = std::time::Instant::now();
    let layers = |h: &Ui| h.state().layers().len();
    // Open and cancel / confirm each dialog 50 times.
    for i in 0..50 {
        key(&mut h, Modifiers::COMMAND, Key::K); // Composition Settings
        key(
            &mut h,
            Modifiers::NONE,
            if i % 2 == 0 { Key::Escape } else { Key::Enter },
        );
        key(&mut h, Modifiers::COMMAND, Key::Y); // Solid Settings → a solid
        key(&mut h, Modifiers::NONE, Key::Enter);
        assert!(!dialog_open(&h), "round {i}: a dialog stayed open");
    }
    assert_eq!(layers(&h), 50);
    for id in ["motion.layer.new.camera", "motion.layer.new.light"] {
        for _ in 0..50 {
            assert!(add_layer(&mut h, id));
            settle(&mut h);
            key(&mut h, Modifiers::NONE, Key::Escape);
        }
    }
    assert_eq!(layers(&h), 50, "Esc in Camera / Light Settings made layers");
    // Layer Settings and Pre-compose on a selected solid, cancelled.
    let solid = h.state().layers()[0].clone();
    for _ in 0..50 {
        h.state().session().lock().selected_layers = vec![solid.clone()];
        key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Y);
        key(&mut h, Modifiers::NONE, Key::Escape);
        h.state().session().lock().selected_layers = vec![solid.clone()];
        key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::C);
        key(&mut h, Modifiers::NONE, Key::Escape);
    }
    assert_eq!(layers(&h), 50);
    // 50 rectangle drags with nothing selected, then undo them all.
    let c = canvas(&h);
    key(&mut h, Modifiers::NONE, Key::Q);
    for i in 0..50 {
        deselect(&h);
        let o = egui::vec2((i % 10) as f32 * 0.05, (i / 10) as f32 * 0.1);
        drag(
            &mut h,
            c.min + c.size() * (egui::vec2(0.2, 0.2) + o),
            c.min + c.size() * (egui::vec2(0.25, 0.27) + o),
            Modifiers::NONE,
        );
    }
    assert_eq!(layers(&h), 100, "50 drags did not make 50 shape layers");
    for _ in 0..50 {
        key(&mut h, Modifiers::COMMAND, Key::Z);
    }
    assert_eq!(
        layers(&h),
        50,
        "50 undos did not remove the 50 shape layers"
    );
    no_failures(&h, "50 rounds");
    println!(
        "50 rounds of dialogs and tools: {:.1} s (debug build)",
        start.elapsed().as_secs_f64()
    );
}

// ---------------------------------------------------------------------------
// #204 retest: layers through Edit ▸ Copy / Cut / Paste (AE: Copy, Cut and
// Paste are global commands, `CSwitchboard` in docs/reference/ae/mac/shortcuts.json).

fn three_layers() -> Ui {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
    assert!(add_layer(&mut h, "shape:rect"));
    settle(&mut h);
    focus(&mut h, TIMELINE.0, TIMELINE.1);
    h
}

fn item_enabled(h: &Ui, id: &str) -> bool {
    h.state()
        .menu_items()
        .into_iter()
        .find(|i| i.invocation.id == id)
        .map(|i| i.enabled)
        .unwrap_or_else(|| panic!("no menu item {id}"))
}

fn select(h: &Ui, layers: &[PrimPath]) {
    let s = h.state().session();
    let mut s = s.lock();
    s.selected_layers = layers.to_vec();
    s.selected_keys.clear();
}

#[test]
fn layers_copy_cut_and_paste_like_ae() {
    let mut h = three_layers();
    let [rect, text, solid]: [PrimPath; 3] = h.state().layers().try_into().unwrap();
    let names = |h: &Ui| h.state().layer_names();
    let before = names(&h);
    assert!(
        !item_enabled(&h, "edit.paste"),
        "Paste enabled with an empty clipboard"
    );

    // Cmd+C on two layers, Cmd+V with the middle one selected.
    select(&h, &[rect.clone(), solid.clone()]);
    key(&mut h, Modifiers::COMMAND, Key::C);
    assert!(item_enabled(&h, "edit.paste"), "Paste greyed after Copy");
    select(&h, std::slice::from_ref(&text));
    key(&mut h, Modifiers::COMMAND, Key::V);
    let after = names(&h);
    assert_eq!(after.len(), 5, "{after:?}");
    // Above the topmost selected layer (the text), named like Duplicate, selected.
    let mut d = three_layers();
    let [drect, _, dsolid]: [PrimPath; 3] = d.state().layers().try_into().unwrap();
    select(&d, &[drect, dsolid]);
    key(&mut d, Modifiers::COMMAND, Key::D);
    let mut dup: Vec<String> = names(&d)
        .into_iter()
        .filter(|n| !before.contains(n))
        .collect();
    let mut pasted: Vec<String> = after
        .iter()
        .filter(|n| !before.contains(n))
        .cloned()
        .collect();
    dup.sort();
    pasted.sort();
    assert_eq!(
        pasted, dup,
        "pasted names differ from Duplicate's: {after:?}"
    );
    let text_at = after.iter().position(|n| *n == before[1]).unwrap();
    assert_eq!(
        text_at, 3,
        "the pasted layers are not just above the text layer: {after:?}"
    );
    let selected: Vec<String> = h
        .state()
        .selected_layers()
        .iter()
        .map(|l| h.state().name(l))
        .collect();
    let mut sel_sorted = selected.clone();
    sel_sorted.sort();
    assert_eq!(
        sel_sorted, pasted,
        "the pasted layers are not the selection"
    );
    let count = |h: &Ui| h.state().layers().len();
    one_undo_step(&mut h, "paste", count);
    no_failures(&h, "copy / paste");

    // The menu items do the same.
    let paste = h
        .state()
        .menu_items()
        .into_iter()
        .find(|i| i.invocation.id == "edit.paste")
        .unwrap();
    assert!(h.state_mut().menu(&paste.id));
    settle(&mut h);
    assert_eq!(count(&h), 7, "Edit ▸ Paste");

    // Cmd+X: the solid goes (one undo step), Cmd+V brings it back.
    let solid_name = h.state().name(&solid);
    select(&h, std::slice::from_ref(&solid));
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(count(&h), 6, "Cut did not remove the layer");
    one_undo_step(&mut h, "cut", |h: &Ui| {
        usize::MAX - h.state().layers().len()
    });
    key(&mut h, Modifiers::COMMAND, Key::V);
    assert_eq!(count(&h), 7, "Paste after Cut");
    let solids = |h: &Ui| {
        h.state()
            .layers()
            .iter()
            .filter(|l| {
                bae_motion::query::layer_kind(h.state().stage(), l).as_deref() == Some("solid")
            })
            .count()
    };
    assert_eq!(
        solids(&h),
        3,
        "the cut solid did not come back: {:?}",
        names(&h)
    );
    println!("cut “{solid_name}”, pasted back as {:?}", names(&h)[0]);

    // Into another comp.
    comp_with(&mut h, &[]);
    focus(&mut h, TIMELINE.0, TIMELINE.1);
    key(&mut h, Modifiers::COMMAND, Key::V);
    let other = h.state().layers();
    assert_eq!(other.len(), 1, "paste into the second comp");
    println!(
        "pasted into an empty comp as “{}”",
        h.state().name(&other[0])
    );
    assert_eq!(
        bae_motion::query::layer_kind(h.state().stage(), &other[0]).as_deref(),
        Some("solid")
    );
    no_failures(&h, "cut / paste across comps");

    // Saved and reopened, identical.
    let tmp = tempfile::tempdir().unwrap();
    let path = tricky_dir(tmp.path()).join("复制 粘贴.bae");
    let saved = bae_model::export_flattened_json(h.state().stage());
    h.state_mut().dialogs.save.push_back(Some(path.clone()));
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let mut g = gui();
    g.state_mut().dialogs.open.push_back(Some(path));
    key(&mut g, Modifiers::COMMAND, Key::O);
    assert_eq!(bae_model::export_flattened_json(g.state().stage()), saved);

    // Renaming a layer (Enter): Cmd+C / Cmd+V belong to the text field. The
    // shell hands the native menu's Cut / Copy / Paste to it
    // (`shell::menu::TEXT_EDIT_ACTIONS` while `text_edit_focused`); the keys
    // must not paste layers either.
    let layer = h.state().layers()[0].clone();
    select(&h, &[layer]);
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(h.ctx.text_edit_focused(), "Enter did not start renaming");
    let n = count(&h);
    key(&mut h, Modifiers::COMMAND, Key::C);
    key(&mut h, Modifiers::COMMAND, Key::V);
    assert_eq!(count(&h), n, "Cmd+V pasted layers while renaming");
    key(&mut h, Modifiers::NONE, Key::Escape);
}
