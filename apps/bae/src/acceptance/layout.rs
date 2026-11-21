//! #133 / #134: the default workspace and the Keyboard Shortcuts window at
//! common screens and display scales. Sizes are window client areas in
//! logical points (physical pixels / scale): every widget of a panel must lie
//! inside the panel, and no panel may need more room than it gets.

use bae_ui::dock::{PanelArea, panel_areas};
use bae_ui_motion::render_queue::ItemStatus;
use egui::accesskit::Role;
use egui::{Key, Modifiers, vec2};
use egui_kittest::kittest::{AccessKitNode, NodeT as _, Queryable as _};

use super::*;

/// (screen, window client area in logical points).
const WINDOWS: [(&str, [f32; 2]); 6] = [
    // BAE's first-launch window (1440×900) on 3840×2160 @150 %: the
    // 2160×1320 px client area of #133.
    ("default window, 3840×2160 @150%", [1440.0, 880.0]),
    ("1920×1080 @100%, maximized", [1920.0, 1009.0]),
    ("2560×1440 @125%, maximized", [2048.0, 1113.0]),
    ("3840×2160 @150%, maximized", [2560.0, 1369.0]),
    ("MacBook Air 13\" Retina, maximized", [1470.0, 919.0]),
    ("MacBook Pro 16\" Retina, maximized", [1728.0, 1084.0]),
];

/// Containers: their bounds are the scroll content, not something to click.
const CONTAINERS: [Role; 5] = [
    Role::GenericContainer,
    Role::Pane,
    Role::ScrollView,
    Role::Window,
    Role::Unknown,
];

fn find_pane<'t>(node: AccessKitNode<'t>, panel: &str) -> Option<AccessKitNode<'t>> {
    let id = format!("panel:{panel}");
    if node.role() == Role::Pane && node.author_id() == Some(id.as_str()) {
        return Some(node);
    }
    node.children().find_map(|c| find_pane(c, panel))
}

fn widgets<'t>(node: AccessKitNode<'t>, out: &mut Vec<AccessKitNode<'t>>) {
    for c in node.children() {
        if !CONTAINERS.contains(&c.role()) && c.bounding_box().is_some() {
            out.push(c);
        }
        // A text field's text scrolls inside it (clipped by the field).
        if !matches!(c.role(), Role::TextInput | Role::MultilineTextInput) {
            widgets(c, out);
        }
    }
}

/// A node's bounds in logical points (accesskit has them in physical pixels:
/// `ppp` times larger on a Retina display).
fn rect_of(node: &AccessKitNode<'_>, ppp: f32) -> Rect {
    let b = node.bounding_box().expect("bounds");
    Rect::from_min_max(
        egui::pos2(b.x0 as f32 / ppp, b.y0 as f32 / ppp),
        egui::pos2(b.x1 as f32 / ppp, b.y1 as f32 / ppp),
    )
}

/// Panels that need more room than they get, and widgets outside their panel.
fn layout_problems(h: &Ui) -> Vec<String> {
    let areas: Vec<PanelArea> = panel_areas(&h.ctx);
    assert!(!areas.is_empty(), "no panels drawn");
    let root = h.root();
    let mut out = Vec::new();
    for a in &areas {
        let panel = a.panel.as_str();
        if a.overflows() {
            out.push(format!(
                "{panel}: needs {:.0}×{:.0}, has {:.0}×{:.0}",
                a.content.x,
                a.content.y,
                a.rect.width(),
                a.rect.height()
            ));
        }
        let Some(pane) = find_pane(root.accesskit_node(), panel) else {
            out.push(format!("{panel}: no accessibility pane"));
            continue;
        };
        let mut nodes = Vec::new();
        widgets(pane, &mut nodes);
        let inside = a.rect.expand(1.0);
        for n in nodes {
            let r = rect_of(&n, h.ctx.pixels_per_point());
            if r.width() <= 0.0 || r.height() <= 0.0 || inside.contains_rect(r) {
                continue;
            }
            out.push(format!(
                "{panel}: {:?} {:?} at {:.0},{:.0}–{:.0},{:.0} outside {:.0},{:.0}–{:.0},{:.0}",
                n.role(),
                n.label().unwrap_or_default(),
                r.left(),
                r.top(),
                r.right(),
                r.bottom(),
                a.rect.left(),
                a.rect.top(),
                a.rect.right(),
                a.rect.bottom(),
            ));
        }
    }
    out
}

/// The tab of a docked panel (its hit area spans the tab bar's height),
/// when it is shown in the tab bar rather than only in the » menu.
fn tab(h: &Ui, title: &str) -> Option<Rect> {
    h.query_all_by_label(bae_ui::tr(title))
        .map(|n| n.rect())
        .find(|r| (r.height() - bae_ui::theme::metrics::TAB_BAR_H).abs() < 1.0 && r.width() >= 30.0)
}

fn click_tab(h: &mut Ui, title: &str) {
    let r = tab(h, title).unwrap_or_else(|| panic!("no {title} tab"));
    click_at(h, r.center());
    settle(h);
}

/// The default workspace with a comp, a solid and a Render Queue item in
/// every state, at one window size: everything fits.
fn check_default_workspace(screen: &str, size: [f32; 2]) -> Vec<String> {
    check_default_workspace_at(screen, size, 1.0)
}

fn check_default_workspace_at(screen: &str, size: [f32; 2], ppp: f32) -> Vec<String> {
    let mut h = gui_scaled(Gui::new(), vec2(size[0], size[1]), ppp);
    let mut problems = Vec::new();
    let check = |h: &mut Ui, what: &str, problems: &mut Vec<String>| {
        settle(h);
        for p in layout_problems(h) {
            problems.push(format!("{screen} {size:?}, {what}: {p}"));
        }
    };
    check(&mut h, "empty project", &mut problems);
    comp_with(&mut h, &["motion.layer.new.solid"]);
    check(&mut h, "comp with a solid", &mut problems);
    key(&mut h, Modifiers::COMMAND, Key::M);
    assert_eq!(h.state().render_queue().lock().items().len(), 1);
    check(&mut h, "render queue: queued", &mut problems);
    // Project, Effect Controls and Render Queue share the left column: all
    // three tabs stay in the tab bar (shortened if need be), none only in ».
    for title in ["Project", "Effect Controls", "Render Queue"] {
        if tab(&h, title).is_none() {
            problems.push(format!("{screen} {size:?}: the {title} tab is hidden"));
        }
    }
    let tmp = std::env::temp_dir().join("bae-layout-check_[####].png");
    let id = h.state().render_queue().lock().items()[0].id;
    let states = [
        (
            "rendering",
            ItemStatus::Rendering {
                done: 12,
                total: 30,
            },
        ),
        (
            "done",
            ItemStatus::Done {
                files: 30,
                first: tmp.clone(),
                seconds: 1.4,
            },
        ),
        (
            "stopped",
            ItemStatus::Stopped {
                done: 12,
                total: 30,
                first: Some(tmp.clone()),
            },
        ),
        ("failed", ItemStatus::Failed("The disk is full.".into())),
    ];
    for (what, status) in states {
        h.state().render_queue().lock().item_mut(id).unwrap().status = status;
        check(&mut h, &format!("render queue: {what}"), &mut problems);
    }
    click_tab(&mut h, "Effect Controls");
    check(&mut h, "effect controls", &mut problems);
    click_tab(&mut h, "Project");
    check(&mut h, "project", &mut problems);
    // The other built-in workspaces, with the same project.
    for ws in ["review", "agent"] {
        h.state_mut().invoke(Invocation::with_params(
            ids::SWITCH_WORKSPACE,
            serde_json::json!({ "id": ws }),
        ));
        check(&mut h, &format!("{ws} workspace"), &mut problems);
        assert_eq!(h.state().workspace.active_workspace(), ws);
    }
    problems
}

#[test]
fn default_workspace_fits_common_screens() {
    let problems: Vec<String> = WINDOWS
        .iter()
        .flat_map(|(screen, size)| check_default_workspace(screen, *size))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn default_workspace_fits_common_screens_in_chinese() {
    if !in_child(
        "acceptance::layout::default_workspace_fits_common_screens_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let problems: Vec<String> = WINDOWS
        .iter()
        .flat_map(|(screen, size)| check_default_workspace(screen, *size))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The Keyboard Shortcuts window: command names on one line, the column as
/// wide as they need, the window resizable and closed by Escape (#134).
#[test]
fn keyboard_shortcuts_window_columns() {
    let mut h = gui_sized(Gui::new(), vec2(1440.0, 880.0));
    h.state_mut()
        .invoke(Invocation::new(ids::KEYBOARD_SHORTCUTS));
    settle(&mut h);
    let window = |h: &Ui| {
        h.query_all_by_role(Role::Window)
            .find(|n| n.accesskit_node().label().as_deref() == Some("Keyboard Shortcuts"))
            .map(|n| n.rect())
    };
    let before = window(&h).expect("Keyboard Shortcuts window");
    let line = |h: &Ui, text: &str| {
        h.query_all_by_label(text)
            .map(|n| n.rect())
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("no row {text}"))
    };
    // Long names, from the #134 screenshots (since #140 the keyframe toggle is
    // listed per property: Alt+Shift+A is the longest).
    let names = ["Anchor Point: Add or Remove Keyframe", "Accept Proposal"];
    for name in names {
        let r = line(&h, name);
        assert!(r.height() < 20.0, "{name} wraps: {r:?}");
    }
    // The shortcut column starts right of the longest name and uses the rest.
    let plus: Vec<Rect> = h
        .query_all_by_label("+")
        .map(|n| n.rect())
        .filter(|r| before.contains_rect(*r))
        .collect();
    assert!(!plus.is_empty(), "no + buttons");
    let name_right = line(&h, names[0]).right();
    let first_plus = plus.iter().map(|r| r.left()).fold(f32::MAX, f32::min);
    assert!(
        first_plus > name_right && first_plus < before.center().x + before.width() * 0.25,
        "shortcut column at {first_plus}, names end at {name_right}, window {before:?}"
    );
    // Resizable: drag the bottom-right corner.
    let corner = before.right_bottom() - vec2(3.0, 3.0);
    h.input_mut().events.extend([
        Event::PointerMoved(corner),
        Event::PointerButton {
            pos: corner,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]);
    frame(&mut h);
    for i in 1..=10 {
        let p = corner + vec2(20.0 * i as f32, 10.0 * i as f32);
        send(&mut h, Event::PointerMoved(p));
    }
    send(
        &mut h,
        Event::PointerButton {
            pos: corner + vec2(200.0, 100.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    );
    settle(&mut h);
    let after = window(&h).expect("window after resize");
    assert!(
        after.width() > before.width() + 150.0 && after.height() > before.height() + 50.0,
        "not resizable: {before:?} → {after:?}"
    );
    // Escape closes it.
    key(&mut h, Modifiers::NONE, Key::Escape);
    settle(&mut h);
    assert!(window(&h).is_none(), "Escape did not close the window");
}

// ---- Retest of #133 / #134 on macOS (dev af27424): Retina scales, narrow
// windows, the shortcuts window in Chinese ----

/// Mac windows at 2× (Retina) and 1.5×: the default BAE window, MacBook Air
/// 13" and Pro 14" / 16" maximized.
const MAC_WINDOWS: [(&str, [f32; 2]); 4] = [
    ("BAE default window", [1440.0, 900.0]),
    ("MacBook Air 13\" Retina, maximized", [1470.0, 919.0]),
    ("MacBook Pro 14\" Retina, maximized", [1512.0, 945.0]),
    ("MacBook Pro 16\" Retina, maximized", [1728.0, 1084.0]),
];

fn mac_problems() -> Vec<String> {
    let mut out = Vec::new();
    for ppp in [2.0, 1.5] {
        for (screen, size) in MAC_WINDOWS {
            out.extend(check_default_workspace_at(
                &format!("{screen} @{ppp}x"),
                size,
                ppp,
            ));
        }
    }
    out
}

#[test]
fn default_workspace_fits_mac_retina_screens() {
    let problems = mac_problems();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn default_workspace_fits_mac_retina_screens_in_chinese() {
    if !in_child(
        "acceptance::layout::default_workspace_fits_mac_retina_screens_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let problems = mac_problems();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Windows smaller than any default (a user drags the window small): panels
/// that do not fit must scroll, not clip; panels that fit keep every widget
/// inside; every frame still shows its active tab.
#[test]
fn narrow_windows_scroll_instead_of_clipping() {
    let mut report = Vec::new();
    for size in [[1280.0, 720.0], [1024.0, 640.0], [900.0, 600.0]] {
        let mut h = gui_scaled(Gui::new(), vec2(size[0], size[1]), 2.0);
        comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
        key(&mut h, Modifiers::COMMAND, Key::M);
        settle(&mut h);
        let areas = panel_areas(&h.ctx);
        // More than rounding (a 152-point column holding 153.4 points of
        // content shows everything).
        let overflowing: Vec<String> = areas
            .iter()
            .filter(|a| a.content.x > a.rect.width() + 2.0 || a.content.y > a.rect.height() + 2.0)
            .map(|a| a.panel.as_str().to_owned())
            .collect();
        // Widgets reaching more than 2 points out of a panel that does not
        // scroll: that is clipping (a box touching the edge is not).
        let ppp = h.ctx.pixels_per_point();
        let mut clipped: Vec<String> = Vec::new();
        for a in &areas {
            let panel = a.panel.as_str();
            if overflowing.iter().any(|o| o == panel) {
                continue;
            }
            let Some(pane) = find_pane(h.root().accesskit_node(), panel) else {
                continue;
            };
            let mut nodes = Vec::new();
            widgets(pane, &mut nodes);
            for n in nodes {
                let r = rect_of(&n, ppp);
                if r.width() > 0.0 && r.height() > 0.0 && !a.rect.expand(2.0).contains_rect(r) {
                    clipped.push(format!(
                        "{panel}: {:?} {:?} {r:?} outside {:?}",
                        n.role(),
                        n.label().unwrap_or_default(),
                        a.rect
                    ));
                }
            }
        }
        // Every overflowing panel offers scrolling (a scroll view in its pane).
        let root = h.root();
        let not_scrollable: Vec<String> = overflowing
            .iter()
            .filter(|panel| {
                find_pane(root.accesskit_node(), panel).is_some_and(|pane| {
                    let mut stack = vec![pane];
                    let mut found = false;
                    while let Some(n) = stack.pop() {
                        if n.role() == Role::ScrollView {
                            found = true;
                            break;
                        }
                        stack.extend(n.children());
                    }
                    !found
                })
            })
            .cloned()
            .collect();
        // The active tab of the main frames is shown.
        let hidden_tabs: Vec<&str> = ["Timeline", "Project"]
            .into_iter()
            .filter(|t| {
                h.query_all_by_label_contains(bae_ui::tr(t))
                    .next()
                    .is_none()
            })
            .collect();
        report.push(format!(
            "{size:?}@2x: overflowing (scroll) {overflowing:?}, clipped {clipped:?}, not scrollable {not_scrollable:?}, hidden tabs {hidden_tabs:?}"
        ));
        assert!(clipped.is_empty(), "{size:?}: {clipped:#?}");
        assert!(
            not_scrollable.is_empty(),
            "{size:?}: overflowing panels without scrolling {not_scrollable:?}"
        );
        assert!(hidden_tabs.is_empty(), "{size:?}: {hidden_tabs:?}");
    }
    println!("{}", report.join("\n"));
}

#[test]
fn keyboard_shortcuts_window_in_chinese_in_a_small_retina_window() {
    if !in_child(
        "acceptance::layout::keyboard_shortcuts_window_in_chinese_in_a_small_retina_window",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let size = vec2(1024.0, 640.0);
    let mut h = gui_scaled(Gui::new(), size, 2.0);
    // Opened with its shortcut (Cmd+Alt+'), like a user.
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    let title = bae_ui::tr("Keyboard Shortcuts");
    let window = |h: &Ui| {
        h.query_all_by_role(Role::Window)
            .find(|n| n.accesskit_node().label().as_deref() == Some(title))
            .map(|n| n.rect())
    };
    let w = window(&h).expect("the shortcut opened no Keyboard Shortcuts window");
    let screen = Rect::from_min_size(egui::Pos2::ZERO, size);
    assert!(
        screen.expand(1.0).contains_rect(w),
        "window {w:?} outside the screen {screen:?}"
    );
    // Long Chinese command names stay on one line.
    let mut checked = 0;
    for en in [
        "Add or Remove Keyframe at Current Time",
        "Accept Proposal",
        "Go to Previous Keyframe",
    ] {
        let zh = bae_ui::tr(en);
        if let Some(r) = h
            .query_all_by_label(zh)
            .map(|n| n.rect())
            .find(|r| w.contains_rect(*r))
        {
            assert!(r.height() < 20.0, "{zh} wraps: {r:?}");
            checked += 1;
        }
    }
    assert!(checked >= 1, "no command rows found in the window");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(window(&h).is_none(), "Escape did not close it");
}
