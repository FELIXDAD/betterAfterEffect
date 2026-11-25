//! The visual keyboard of the Keyboard Shortcuts dialog: AE's key layout
//! (macOS: measured from AE 26.2.1, docs/reference/ae/mac/ui/keyboard-shortcuts.json
//! `visual_keyboard`; Windows: the PC layout with Esc, Insert and Num Lock),
//! coloured by what each key does with the active modifiers.

use egui::{Color32, Key};

use super::text::{self, MAC, Mods};
use crate::i18n::{Lang, lang};

/// Size of the keyboard in AE's points (the dialog scales it to fit).
pub const SIZE: egui::Vec2 = egui::vec2(1480.0, 385.0);

/// AE's key colours (sampled).
pub mod colors {
    use egui::Color32;
    pub const APPLICATION: Color32 = Color32::from_rgb(0x87, 0x65, 0xce);
    pub const PANEL: Color32 = Color32::from_rgb(0x16, 0xbe, 0x9e);
    pub const UNASSIGNED: Color32 = Color32::from_rgb(0x83, 0x83, 0x83);
    pub const MODIFIER: Color32 = Color32::from_rgb(0x63, 0x63, 0x63);
    pub const PRESSED: Color32 = Color32::from_rgb(0x00, 0x71, 0xca);
    pub const SELECTED: Color32 = Color32::from_rgb(0x4e, 0xa9, 0xf6);
}

/// A modifier key cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Shift,
    /// The macOS Control key.
    Ctrl,
    /// Option / Alt.
    Alt,
    /// ⌘ on macOS, Ctrl on Windows.
    Command,
}

impl Modifier {
    pub fn toggle(self, m: &mut Mods) {
        let flag = match self {
            Modifier::Shift => &mut m.shift,
            Modifier::Ctrl => &mut m.ctrl,
            Modifier::Alt => &mut m.alt,
            Modifier::Command => &mut m.command,
        };
        *flag = !*flag;
    }

    pub fn is_on(self, m: Mods) -> bool {
        match self {
            Modifier::Shift => m.shift,
            Modifier::Ctrl => m.ctrl,
            Modifier::Alt => m.alt,
            Modifier::Command => m.command,
        }
    }
}

/// What a key cap stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    /// A key shortcuts can use.
    Key(Key),
    /// A numpad key, by its chord name (`Pad1`): a key only when the keymap
    /// knows numpad keys (`Chord::parse`), else drawn but inert.
    Pad(&'static str),
    Modifier(Modifier),
    /// Drawn like AE (Caps Lock, Num Lock, Win, Menu) but not bindable.
    Inert,
}

/// One key of the drawing: AE's position (points from the keyboard's corner),
/// label and meaning.
#[derive(Clone, Copy, Debug)]
pub struct KeyCap {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub cap: Cap,
    /// The label when it is not the key's name (modifiers, numpad, Caps Lock).
    pub label: Option<(&'static str, &'static str)>,
}

impl KeyCap {
    /// The bindable key, if any.
    pub fn key(&self) -> Option<Key> {
        match self.cap {
            Cap::Key(k) => Some(k),
            Cap::Pad(name) => crate::keymap::Chord::parse(name).ok().map(|c| c.key),
            _ => None,
        }
    }

    /// The name printed at the bottom left, in the UI language.
    pub fn name(&self) -> String {
        if let Some((en, zh)) = self.label {
            return match lang() {
                Lang::Zh => zh.to_owned(),
                Lang::En => en.to_owned(),
            };
        }
        match self.cap {
            Cap::Key(k) => text::key_name(k),
            _ => String::new(),
        }
    }
}

/// Distance between key rows / columns, and a key's side (AE points).
const PITCH: f32 = 65.0;
const SIDE: f32 = 60.0;

fn cap(x: f32, y: f32, w: f32, cap: Cap) -> KeyCap {
    KeyCap {
        x,
        y,
        w,
        h: SIDE,
        cap,
        label: None,
    }
}

fn labeled(x: f32, y: f32, w: f32, h: f32, cap: Cap, en: &'static str, zh: &'static str) -> KeyCap {
    KeyCap {
        x,
        y,
        w,
        h,
        cap,
        label: Some((en, zh)),
    }
}

/// The keys of the platform's keyboard, in drawing order.
pub fn layout() -> Vec<KeyCap> {
    use Key::*;
    let mut v = Vec::with_capacity(110);
    // Function row.
    if MAC {
        let xs = [
            0., 80., 160., 240., 321., 401., 481., 562., 642., 722., 802., 883.,
        ];
        for (i, k) in [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12]
            .iter()
            .enumerate()
        {
            v.push(KeyCap {
                w: 76.0,
                ..cap(xs[i], 0.0, 76.0, Cap::Key(*k))
            });
        }
    } else {
        v.push(cap(0.0, 0.0, SIDE, Cap::Key(Escape)));
        let xs = [
            105., 170., 235., 300., 385., 450., 515., 580., 665., 730., 795., 860.,
        ];
        for (i, k) in [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12]
            .iter()
            .enumerate()
        {
            v.push(cap(xs[i], 0.0, SIDE, Cap::Key(*k)));
        }
    }
    // Number row.
    for (i, k) in [
        Backtick, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0, Minus, Equals,
    ]
    .iter()
    .enumerate()
    {
        v.push(cap(i as f32 * PITCH, PITCH, SIDE, Cap::Key(*k)));
    }
    v.push(cap(845.0, PITCH, 114.0, Cap::Key(Backspace)));
    // Tab row.
    v.push(cap(0.0, 2.0 * PITCH, 114.0, Cap::Key(Tab)));
    for (i, k) in [
        Q,
        W,
        E,
        R,
        T,
        Y,
        Key::U,
        I,
        O,
        P,
        OpenBracket,
        CloseBracket,
        Backslash,
    ]
    .iter()
    .enumerate()
    {
        v.push(cap(
            119.0 + i as f32 * PITCH,
            2.0 * PITCH,
            SIDE,
            Cap::Key(*k),
        ));
    }
    // Home row.
    v.push(labeled(
        0.0,
        3.0 * PITCH,
        120.0,
        SIDE,
        Cap::Inert,
        "Caps Lock",
        "大写锁定",
    ));
    for (i, k) in [A, S, D, F, G, H, J, Key::K, L, Semicolon, Quote]
        .iter()
        .enumerate()
    {
        v.push(cap(
            124.0 + i as f32 * PITCH,
            3.0 * PITCH,
            SIDE,
            Cap::Key(*k),
        ));
    }
    v.push(cap(839.0, 3.0 * PITCH, 120.0, Cap::Key(Enter)));
    // Shift row.
    let shift = Cap::Modifier(Modifier::Shift);
    let (shift_en, shift_zh) = if MAC {
        ("\u{21e7} Shift", "\u{21e7} Shift")
    } else {
        ("Shift", "Shift")
    };
    v.push(labeled(
        0.0,
        4.0 * PITCH,
        153.0,
        SIDE,
        shift,
        shift_en,
        shift_zh,
    ));
    for (i, k) in [Z, X, C, V, B, N, M, Comma, Period, Slash]
        .iter()
        .enumerate()
    {
        v.push(cap(
            157.0 + i as f32 * PITCH,
            4.0 * PITCH,
            SIDE,
            Cap::Key(*k),
        ));
    }
    v.push(labeled(
        807.0,
        4.0 * PITCH,
        152.0,
        SIDE,
        shift,
        shift_en,
        shift_zh,
    ));
    // Bottom row.
    let y = 5.0 * PITCH;
    if MAC {
        let ctrl = Cap::Modifier(Modifier::Ctrl);
        let opt = Cap::Modifier(Modifier::Alt);
        let cmd = Cap::Modifier(Modifier::Command);
        // AE: "⌃ Ctrl", "⌥ Opt", "⌘ Cmd" ($$$/dvaui/ksv/mac/*KeyName).
        let (c, o, m) = ("\u{2303} Ctrl", "\u{2325} Opt", "\u{2318} Cmd");
        v.push(labeled(0.0, y, 90.0, SIDE, ctrl, c, c));
        v.push(labeled(95.0, y, 60.0, SIDE, opt, o, o));
        v.push(labeled(160.0, y, 90.0, SIDE, cmd, m, m));
        v.push(cap(255.0, y, 450.0, Cap::Key(Space)));
        v.push(labeled(710.0, y, 90.0, SIDE, cmd, m, m));
        v.push(labeled(805.0, y, 60.0, SIDE, opt, o, o));
        v.push(labeled(870.0, y, 89.0, SIDE, ctrl, c, c));
    } else {
        let ctrl = Cap::Modifier(Modifier::Command);
        let alt = Cap::Modifier(Modifier::Alt);
        v.push(labeled(0.0, y, 90.0, SIDE, ctrl, "Ctrl", "Ctrl"));
        v.push(labeled(95.0, y, 60.0, SIDE, Cap::Inert, "Win", "Win"));
        v.push(labeled(160.0, y, 90.0, SIDE, alt, "Alt", "Alt"));
        v.push(cap(255.0, y, 450.0, Cap::Key(Space)));
        v.push(labeled(710.0, y, 90.0, SIDE, alt, "Alt", "Alt"));
        v.push(labeled(805.0, y, 60.0, SIDE, Cap::Inert, "Menu", "菜单"));
        v.push(labeled(870.0, y, 89.0, SIDE, ctrl, "Ctrl", "Ctrl"));
    }
    // Editing keys and arrows.
    if MAC {
        v.push(cap(1061.0, PITCH, SIDE, Cap::Key(Home)));
        v.push(cap(1126.0, PITCH, SIDE, Cap::Key(PageUp)));
        v.push(cap(996.0, 2.0 * PITCH, SIDE, Cap::Key(Delete)));
    } else {
        v.push(cap(996.0, PITCH, SIDE, Cap::Key(Insert)));
        v.push(cap(1061.0, PITCH, SIDE, Cap::Key(Home)));
        v.push(cap(1126.0, PITCH, SIDE, Cap::Key(PageUp)));
        v.push(cap(996.0, 2.0 * PITCH, SIDE, Cap::Key(Delete)));
    }
    v.push(cap(1061.0, 2.0 * PITCH, SIDE, Cap::Key(End)));
    v.push(cap(1126.0, 2.0 * PITCH, SIDE, Cap::Key(PageDown)));
    v.push(cap(1061.0, 4.0 * PITCH, SIDE, Cap::Key(ArrowUp)));
    v.push(cap(996.0, 5.0 * PITCH, SIDE, Cap::Key(ArrowLeft)));
    v.push(cap(1061.0, 5.0 * PITCH, SIDE, Cap::Key(ArrowDown)));
    v.push(cap(1126.0, 5.0 * PITCH, SIDE, Cap::Key(ArrowRight)));
    // Numpad (AE: "7 (Num)", zh "7（数字）").
    let pad =
        |x: f32, y: f32, w: f32, h: f32, name, en, zh| labeled(x, y, w, h, Cap::Pad(name), en, zh);
    let (x0, x1, x2, x3) = (1225.0, 1290.0, 1355.0, 1420.0);
    if MAC {
        v.push(labeled(
            x0,
            PITCH,
            SIDE,
            SIDE,
            Cap::Inert,
            "= (Num)",
            "=（数字）",
        ));
    } else {
        v.push(labeled(
            x0,
            PITCH,
            SIDE,
            SIDE,
            Cap::Inert,
            "Num Lock",
            "Num Lock",
        ));
    }
    v.push(pad(
        x1,
        PITCH,
        SIDE,
        SIDE,
        "PadSlash",
        "/ (Num)",
        "/（数字）",
    ));
    v.push(pad(
        x2,
        PITCH,
        SIDE,
        SIDE,
        "PadMultiply",
        "* (Num)",
        "*（数字）",
    ));
    v.push(pad(
        x3,
        PITCH,
        SIDE,
        SIDE,
        "PadMinus",
        "- (Num)",
        "-（数字）",
    ));
    v.push(pad(
        x0,
        2.0 * PITCH,
        SIDE,
        SIDE,
        "Pad7",
        "7 (Num)",
        "7（数字）",
    ));
    v.push(pad(
        x1,
        2.0 * PITCH,
        SIDE,
        SIDE,
        "Pad8",
        "8 (Num)",
        "8（数字）",
    ));
    v.push(pad(
        x2,
        2.0 * PITCH,
        SIDE,
        SIDE,
        "Pad9",
        "9 (Num)",
        "9（数字）",
    ));
    v.push(pad(
        x3,
        2.0 * PITCH,
        SIDE,
        125.0,
        "PadPlus",
        "+ (Num)",
        "+（数字）",
    ));
    v.push(pad(
        x0,
        3.0 * PITCH,
        SIDE,
        SIDE,
        "Pad4",
        "4 (Num)",
        "4（数字）",
    ));
    v.push(pad(
        x1,
        3.0 * PITCH,
        SIDE,
        SIDE,
        "Pad5",
        "5 (Num)",
        "5（数字）",
    ));
    v.push(pad(
        x2,
        3.0 * PITCH,
        SIDE,
        SIDE,
        "Pad6",
        "6 (Num)",
        "6（数字）",
    ));
    v.push(pad(
        x0,
        4.0 * PITCH,
        SIDE,
        SIDE,
        "Pad1",
        "1 (Num)",
        "1（数字）",
    ));
    v.push(pad(
        x1,
        4.0 * PITCH,
        SIDE,
        SIDE,
        "Pad2",
        "2 (Num)",
        "2（数字）",
    ));
    v.push(pad(
        x2,
        4.0 * PITCH,
        SIDE,
        SIDE,
        "Pad3",
        "3 (Num)",
        "3（数字）",
    ));
    v.push(labeled(
        x3,
        4.0 * PITCH,
        SIDE,
        125.0,
        Cap::Inert,
        "Enter (Num)",
        "输入（数字）",
    ));
    v.push(pad(
        x0,
        5.0 * PITCH,
        125.0,
        SIDE,
        "Pad0",
        "0 (Num)",
        "0（数字）",
    ));
    v.push(pad(
        x2,
        5.0 * PITCH,
        SIDE,
        SIDE,
        "PadDecimal",
        ". (Num)",
        ".（数字）",
    ));
    v
}

/// How a key looks with the active modifiers and scope.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyLook {
    /// The application command's name (Application scope).
    pub application: Option<String>,
    /// A panel command is bound to it; with a panel scope, that command's name.
    pub panel: Option<String>,
}

impl KeyLook {
    /// (upper-left triangle, lower-right triangle) colours.
    pub fn fills(&self) -> (Color32, Color32) {
        match (&self.application, &self.panel) {
            (Some(_), Some(_)) => (colors::APPLICATION, colors::PANEL),
            (Some(_), None) => (colors::APPLICATION, colors::APPLICATION),
            (None, Some(_)) => (colors::UNASSIGNED, colors::PANEL),
            (None, None) => (colors::UNASSIGNED, colors::UNASSIGNED),
        }
    }

    /// The name printed on the key: the application command, or (panel scope)
    /// the panel command. A key with only panel commands shows none in the
    /// Application scope, like AE.
    pub fn label(&self, panel_scope: bool) -> Option<&str> {
        if panel_scope {
            self.panel.as_deref()
        } else {
            self.application.as_deref()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_do_not_overlap_and_fit() {
        let caps = layout();
        for (i, a) in caps.iter().enumerate() {
            assert!(a.x >= 0.0 && a.y >= 0.0, "{a:?}");
            assert!(
                a.x + a.w <= SIZE.x + 0.5 && a.y + a.h <= SIZE.y + 0.5,
                "{a:?}"
            );
            let ra = egui::Rect::from_min_size(egui::pos2(a.x, a.y), egui::vec2(a.w, a.h));
            for b in &caps[i + 1..] {
                let rb = egui::Rect::from_min_size(egui::pos2(b.x, b.y), egui::vec2(b.w, b.h));
                assert!(
                    !ra.shrink(0.5).intersects(rb.shrink(0.5)),
                    "{a:?} overlaps {b:?}"
                );
            }
        }
        // Every letter and digit once.
        for k in [Key::A, Key::Z, Key::Num0, Key::F12, Key::Space, Key::Enter] {
            assert_eq!(
                caps.iter().filter(|c| c.cap == Cap::Key(k)).count(),
                1,
                "{k:?}"
            );
        }
        let mods = caps
            .iter()
            .filter(|c| matches!(c.cap, Cap::Modifier(_)))
            .count();
        assert_eq!(mods, if MAC { 8 } else { 6 });
    }

    #[test]
    fn looks_like_ae() {
        let both = KeyLook {
            application: Some("Clear (Delete)".into()),
            panel: Some("Backspace".into()),
        };
        assert_eq!(both.fills(), (colors::APPLICATION, colors::PANEL));
        assert_eq!(both.label(false), Some("Clear (Delete)"));
        let panel = KeyLook {
            application: None,
            panel: Some("Show Snapshot 1".into()),
        };
        assert_eq!(panel.fills(), (colors::UNASSIGNED, colors::PANEL));
        assert_eq!(
            panel.label(false),
            None,
            "AE writes no panel names in Application"
        );
        assert_eq!(panel.label(true), Some("Show Snapshot 1"));
    }
}
