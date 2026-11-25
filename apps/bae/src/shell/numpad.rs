//! Numpad keys for AE's shortcuts (#261). egui has no numpad keys: egui-winit
//! reports numpad digits as the main digits, numpad `*` not at all. The shell
//! replaces the key event of a numpad press with the function key
//! `bae_ui::keymap::NUMPAD` gives it (`PadMultiply` = F21 …); the text the press
//! types stays, so text fields still get it.

use winit::keyboard::KeyCode;

/// The key standing for a numpad press, if AE's shortcuts use it. Only presses
/// that type a character: with NumLock off (Windows) numpad 4 / 6 / 8 / 2 / 7 /
/// 1 / 9 / 3 / . are arrows, Home, End, Page Up / Down and Delete
/// and stay those keys.
pub fn key(code: KeyCode, logical: &winit::keyboard::Key) -> Option<egui::Key> {
    // AE binds both Num 0 and Num Insert to the same preview transport.
    let pad_insert = code == KeyCode::Numpad0
        && matches!(
            logical,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Insert)
        );
    if !pad_insert && !matches!(logical, winit::keyboard::Key::Character(_)) {
        return None;
    }
    let name = match code {
        KeyCode::NumpadMultiply => "PadMultiply",
        KeyCode::NumpadDivide => "PadSlash",
        KeyCode::NumpadAdd => "PadPlus",
        KeyCode::NumpadSubtract => "PadMinus",
        KeyCode::NumpadDecimal => "PadDecimal",
        KeyCode::Numpad0 => "Pad0",
        KeyCode::Numpad1 => "Pad1",
        KeyCode::Numpad2 => "Pad2",
        KeyCode::Numpad3 => "Pad3",
        KeyCode::Numpad4 => "Pad4",
        KeyCode::Numpad5 => "Pad5",
        KeyCode::Numpad6 => "Pad6",
        KeyCode::Numpad7 => "Pad7",
        KeyCode::Numpad8 => "Pad8",
        KeyCode::Numpad9 => "Pad9",
        _ => return None,
    };
    bae_ui::keymap::NUMPAD
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, k, _)| *k)
}

/// Replaces the key events egui-winit added for one numpad press (`events[from..]`)
/// with one event of `pad`, with their modifiers (or `modifiers` when there was
/// none: numpad `*`).
pub fn rewrite(
    events: &mut Vec<egui::Event>,
    from: usize,
    pad: egui::Key,
    pressed: bool,
    repeat: bool,
    mut modifiers: egui::Modifiers,
) {
    let mut i = from;
    while i < events.len() {
        if let egui::Event::Key { modifiers: m, .. } = events[i] {
            modifiers = m;
            events.remove(i);
        } else {
            i += 1;
        }
    }
    events.push(egui::Event::Key {
        key: pad,
        physical_key: Some(pad),
        pressed,
        repeat,
        modifiers,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Numpad 1 is its own key (not the main 1) and still types "1".
    #[test]
    fn numpad_presses_become_their_own_keys() {
        let ch = |c: &str| winit::keyboard::Key::Character(c.into());
        let pad1 = key(KeyCode::Numpad1, &ch("1")).unwrap();
        assert_eq!(bae_ui::Chord::parse("Pad1").unwrap().key, pad1);
        let mut events = vec![
            egui::Event::Key {
                key: egui::Key::Num1,
                physical_key: Some(egui::Key::Num1),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            },
            egui::Event::Text("1".into()),
        ];
        rewrite(&mut events, 0, pad1, true, false, egui::Modifiers::NONE);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], egui::Event::Text(ref t) if t == "1"));
        assert!(matches!(
            events[1],
            egui::Event::Key { key, modifiers, .. } if key == pad1 && modifiers.alt
        ));
        assert!(key(KeyCode::Digit1, &ch("1")).is_none());
        let insert = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Insert);
        assert_eq!(
            key(KeyCode::Numpad0, &insert),
            key(KeyCode::Numpad0, &ch("0"))
        );
        assert!(key(KeyCode::Digit0, &ch("0")).is_none());
        assert!(key(KeyCode::NumpadMultiply, &ch("*")).is_some());
        // NumLock off: numpad 4 is the left arrow, not Pad4.
        let left = winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowLeft);
        assert!(key(KeyCode::Numpad4, &left).is_none());
    }
}
