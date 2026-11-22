//! Module dialogs: modal windows drawn over the workbench, such as AE's
//! Composition Settings, Solid Settings or the Preferences window.
//!
//! A feature crate registers a [`DialogHost`] in `bae_app::Registry<dyn DialogHost>`
//! (id = module name). Every frame, after the dock, the workbench calls
//! [`DialogHost::ui`] with a [`PanelContext`]; requests made through it (commands,
//! opening panels) are applied after the frame like a panel's, so a dialog's OK is
//! one command and one undo step. While any host reports [`DialogHost::is_open`],
//! the workbench treats the frame as modal ([`crate::panel::modal_open`]): no
//! shortcuts and no panel keys reach the project behind the dialog.
//!
//! [`window`] draws the AE look: its own title bar with the title, draggable,
//! centered over the main window when it opens, the main window not dimmed but
//! blocked. [`buttons`] draws OK / Cancel in the platform's order (Windows: OK,
//! Cancel; macOS: Cancel, OK at the far right) and maps Enter / Esc to them.
//!
//! ```
//! use bae_app::Resources;
//! use bae_ui::dialog::{self, DialogHost, Outcome};
//! use bae_ui::PanelContext;
//! use std::sync::Mutex;
//!
//! #[derive(Default)]
//! struct Rename { open: Mutex<Option<String>> }
//!
//! impl DialogHost for Rename {
//!     fn is_open(&self, _: &Resources) -> bool {
//!         self.open.lock().unwrap().is_some()
//!     }
//!     fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>) {
//!         let mut open = self.open.lock().unwrap();
//!         let Some(name) = open.as_mut() else { return };
//!         let shown = dialog::window(ctx, egui::Id::new("rename"), "Rename", |ui| {
//!             ui.text_edit_singleline(name);
//!             dialog::buttons(ui, true)
//!         });
//!         match shown {
//!             Some(Outcome::Ok) => {
//!                 cx.invoke_with("attr.set", serde_json::json!({"name": name.clone()}));
//!                 *open = None;
//!             }
//!             Some(Outcome::Cancel) => *open = None,
//!             _ => {}
//!         }
//!     }
//! }
//! ```

use bae_app::Resources;
use egui::{Align, Color32, Id, Layout, Order, Sense, Ui, vec2};

use crate::i18n::tr;
use crate::panel::PanelContext;
use crate::theme::{self, color};

/// Dialogs of one module, drawn by the workbench over the dock.
pub trait DialogHost: Send + Sync + 'static {
    /// Whether one of this module's dialogs is open. While any host's is, the
    /// workbench routes no shortcuts or panel keys to the project.
    fn is_open(&self, resources: &Resources) -> bool;

    /// Draw the open dialogs. Called every frame after the dock (also when
    /// [`DialogHost::is_open`] is false, so a host may open a dialog itself).
    fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>);
}

/// What a dialog's buttons or keys did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    None,
    /// OK (the button or Enter).
    Ok,
    /// Cancel (the button, Esc or the title bar's close button).
    Cancel,
}

/// Background of dialog windows (AE's dialogs are a step lighter than panels).
pub const DIALOG_BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);

/// Draw a modal dialog window titled `title` (translate it first). The window
/// has its own title bar and can be dragged; it opens centered over the main
/// window and is not resizable. Clicks on the rest of the window are blocked
/// (the background is not dimmed, like AE). Returns `add_contents`' result;
/// `None` when the title bar's close button was clicked (treat it as Cancel;
/// [`closed_by_title_bar`] tells it apart from a frame where the window was
/// not shown).
pub fn window<R>(
    ctx: &egui::Context,
    id: Id,
    title: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    // Newly opened (not shown in the previous pass): center it again and put
    // it above the blocker. Later passes leave the order to egui, so popups
    // the dialog opens stay above it.
    let pass = ctx.cumulative_pass_nr();
    let last = ctx.data(|d| d.get_temp::<u64>(id.with("pass")));
    let opened = last.is_none_or(|p| p + 1 < pass);
    ctx.data_mut(|d| d.insert_temp(id.with("pass"), pass));
    block_background(ctx, id);
    let mut open = true;
    let frame = egui::Frame::window(&ctx.global_style())
        .fill(DIALOG_BG)
        .stroke(egui::Stroke::new(1.0, color::DIVIDER))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(16, 12));
    let mut w = egui::Window::new(egui::RichText::new(title).color(color::TEXT_STRONG))
        .id(id)
        .order(Order::Foreground)
        .frame(frame)
        .collapsible(false)
        .resizable(false)
        .movable(true)
        .open(&mut open)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center());
    if opened {
        w = w.current_pos(ctx.content_rect().center());
    }
    take_enter(ctx);
    let shown = w.show(ctx, |ui| {
        ui.style_mut().spacing.item_spacing = vec2(8.0, 6.0);
        add_contents(ui)
    });
    if opened {
        ctx.move_to_top(egui::LayerId::new(Order::Foreground, id));
    }
    ctx.data_mut(|d| d.insert_temp(id.with("closed"), !open));
    if !open {
        return None;
    }
    shown.and_then(|r| r.inner)
}

/// Whether the dialog `id` was closed with its title bar button this frame.
pub fn closed_by_title_bar(ctx: &egui::Context, id: Id) -> bool {
    ctx.data(|d| d.get_temp::<bool>(id.with("closed")))
        .unwrap_or(false)
}

/// An invisible layer over the whole window, below the dialog, that eats
/// clicks and drags meant for the panels behind it.
fn block_background(ctx: &egui::Context, id: Id) {
    let rect = ctx.content_rect();
    egui::Area::new(id.with("blocker"))
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            ui.allocate_rect(rect, Sense::click_and_drag());
        });
}

/// OK and Cancel at the bottom right in the platform's order, with Enter as OK
/// (when `ok_enabled`) and Esc as Cancel.
pub fn buttons(ui: &mut Ui, ok_enabled: bool) -> Outcome {
    buttons_labeled(ui, ok_enabled, tr("OK"), tr("Cancel"))
}

/// [`buttons`] with other labels (e.g. "Import" / "Cancel").
pub fn buttons_labeled(ui: &mut Ui, ok_enabled: bool, ok: &str, cancel: &str) -> Outcome {
    let mut out = Outcome::None;
    ui.add_space(4.0);
    // One row high: a right-to-left layout straight in a vertical one would
    // take all the height the window offers and leave a gap above the row.
    let row = vec2(ui.available_width(), 24.0);
    ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
        // Right to left: the first button drawn is the rightmost.
        let mac = cfg!(target_os = "macos");
        let order: [bool; 2] = if mac { [true, false] } else { [false, true] };
        for is_ok in order {
            if is_ok {
                let b = egui::Button::new(egui::RichText::new(ok).color(color::TEXT_STRONG))
                    .fill(color::ACCENT_DIM)
                    .min_size(vec2(72.0, 22.0));
                if ui.add_enabled(ok_enabled, b).clicked() {
                    out = Outcome::Ok;
                }
            } else {
                let b = egui::Button::new(egui::RichText::new(cancel).color(color::TEXT_STRONG))
                    .min_size(vec2(72.0, 22.0));
                if ui.add(b).clicked() {
                    out = Outcome::Cancel;
                }
            }
        }
    });
    if out == Outcome::None {
        out = keys(ui.ctx(), ok_enabled);
    }
    out
}

fn enter_id() -> Id {
    Id::new("bae_ui.dialog.enter")
}

/// AE: in a dialog, Enter (and keypad Enter) is always the default button,
/// OK; Space activates the focused control. egui clicks a focused button on
/// Enter too, so unless a text field has the focus (a single-line one commits
/// on Enter, a multi-line one makes a new line), this takes this frame's Enter
/// before the dialog's controls see it, and [`keys`] reports it as OK. Call
/// it before drawing the dialog ([`window`] does); returns whether it took
/// one.
pub fn take_enter(ctx: &egui::Context) -> bool {
    let typing = ctx
        .memory(|m| m.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
    if typing {
        return false;
    }
    let taken = ctx.input_mut(|i| {
        let before = i.events.len();
        i.events.retain(|e| {
            !matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.shift
            )
        });
        i.events.len() != before
    });
    if taken {
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(enter_id(), pass));
    }
    taken
}

/// An Enter [`take_enter`] took in this pass (once: the first dialog that
/// asks gets it).
fn taken_enter(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.remove_temp::<u64>(enter_id())) == Some(pass)
}

/// Enter (OK) and Esc (Cancel) pressed this frame. Enter in a multi-line text
/// field (one that keeps focus after Enter) does not count.
pub fn keys(ctx: &egui::Context, ok_enabled: bool) -> Outcome {
    let (enter, escape) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Enter) && !i.modifiers.shift,
            i.key_pressed(egui::Key::Escape),
        )
    });
    let enter = enter || (ok_enabled && taken_enter(ctx));
    // Consume the key so a dialog under this one (the Color Picker over Solid
    // Settings) does not see it too.
    if escape {
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        return Outcome::Cancel;
    }
    // A single-line field gives up focus on Enter; a multi-line one keeps it
    // (Enter is a new line there). Call this after drawing the fields.
    let typing_multiline = ctx
        .memory(|m| m.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
    if enter && ok_enabled && !typing_multiline {
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        return Outcome::Ok;
    }
    Outcome::None
}

/// A label column of fixed width followed by the row's controls, the layout
/// of AE's settings dialogs ("Width: [1920] px").
pub fn row<R>(ui: &mut Ui, label_width: f32, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(label_width, 18.0), Sense::hover());
        ui.painter().text(
            rect.right_center(),
            egui::Align2::RIGHT_CENTER,
            label,
            theme::body(),
            color::TEXT,
        );
        add(ui)
    })
    .inner
}

/// A titled group box (AE: "Advanced: Motion Blur" …).
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).color(color::TEXT_STRONG));
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, color::DIVIDER))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, add)
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One dialog with a swatch-like button, a dropdown, a number, a
    /// single-line and a multi-line text field.
    struct Form {
        clicked: bool,
        number: f64,
        line: String,
        text: String,
    }

    const CONTROLS: [&str; 5] = ["button", "combo", "number", "line", "text"];

    fn frame(
        ctx: &egui::Context,
        form: &mut Form,
        events: Vec<egui::Event>,
        focus: Option<&str>,
    ) -> Outcome {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut outcome = Outcome::None;
        let mut out = ctx.run_ui(raw, |ui| {
            let shown = window(ui.ctx(), Id::new("form"), "Form", |ui| {
                let mut responses = Vec::new();
                let b = ui.button("Swatch");
                form.clicked |= b.clicked();
                responses.push(b);
                let mut sel = 0;
                responses.push(
                    egui::ComboBox::from_id_salt("combo")
                        .selected_text("A")
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut sel, 1, "B");
                        })
                        .response,
                );
                responses.push(ui.add(egui::DragValue::new(&mut form.number)));
                responses.push(ui.text_edit_singleline(&mut form.line));
                responses.push(ui.text_edit_multiline(&mut form.text));
                if let Some(name) = focus {
                    let i = CONTROLS.iter().position(|c| *c == name).unwrap();
                    responses[i].request_focus();
                }
                buttons(ui, true)
            });
            outcome = shown.unwrap_or(Outcome::None);
        });
        out.textures_delta.clear();
        outcome
    }

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Issue #205: Enter is OK whatever control has the focus (AE), except in
    /// a multi-line text field; it never activates the focused control.
    #[test]
    fn enter_is_ok_on_every_focused_control_but_multiline_text() {
        for (control, ok) in [
            ("button", true),
            ("combo", true),
            ("number", true),
            ("line", true),
            ("text", false),
        ] {
            let ctx = crate::testing::ctx_with_fonts();
            let mut form = Form {
                clicked: false,
                number: 1.0,
                line: String::new(),
                text: String::new(),
            };
            frame(&ctx, &mut form, Vec::new(), Some(control));
            frame(&ctx, &mut form, Vec::new(), None);
            let got = frame(&ctx, &mut form, vec![key(egui::Key::Enter)], None);
            let want = if ok { Outcome::Ok } else { Outcome::None };
            assert_eq!(got, want, "Enter with {control} focused");
            assert!(!form.clicked, "Enter clicked the focused {control}");
        }
        // Space still activates the focused button.
        let ctx = crate::testing::ctx_with_fonts();
        let mut form = Form {
            clicked: false,
            number: 1.0,
            line: String::new(),
            text: String::new(),
        };
        frame(&ctx, &mut form, Vec::new(), Some("button"));
        frame(&ctx, &mut form, Vec::new(), None);
        let got = frame(&ctx, &mut form, vec![key(egui::Key::Space)], None);
        assert_eq!(got, Outcome::None);
        assert!(form.clicked, "Space did not press the focused button");
    }
}
