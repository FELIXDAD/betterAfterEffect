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
    #[cfg(target_os = "macos")]
    if matches!(code, KeyCode::NumLock | KeyCode::NumpadClear) {
        return Some(egui::Key::BrowserBack);
    }
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
        KeyCode::NumpadDecimal | KeyCode::NumpadComma => "PadDecimal",
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

/// The modifiers held now: the latest change egui-winit queued since the last
/// frame (`pending`), else the last frame's (`last_frame`). A change can come
/// in with the press itself before any frame ran (Alt held, then numpad `*`,
/// right after startup on Windows, #537); the last frame's would miss it.
pub fn current_modifiers(pending: &[egui::Event], last_frame: egui::Modifiers) -> egui::Modifiers {
    pending
        .iter()
        .rev()
        .find_map(|e| match e {
            egui::Event::ModifiersChanged(m) => Some(*m),
            _ => None,
        })
        .unwrap_or(last_frame)
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

/// Windows lets go of Shift for a moment around a numpad key pressed with
/// NumLock on (a fake Shift up, the key as its navigation key, a fake Shift
/// down), so Shift+numpad 3 (or Shift+Page Down sent with the numpad's scan
/// code) arrives as a bare Page Down. AE still reads Shift+Page Down (10
/// frames, #468): a navigation key from the numpad right after Shift went up
/// keeps Shift.
pub fn restore_shift(
    events: &mut [egui::Event],
    from: usize,
    code: KeyCode,
    logical: &winit::keyboard::Key,
    shift_up: Option<std::time::Instant>,
) {
    use winit::keyboard::{Key, NamedKey};
    let numpad = matches!(
        code,
        KeyCode::Numpad0
            | KeyCode::Numpad1
            | KeyCode::Numpad2
            | KeyCode::Numpad3
            | KeyCode::Numpad4
            | KeyCode::Numpad5
            | KeyCode::Numpad6
            | KeyCode::Numpad7
            | KeyCode::Numpad8
            | KeyCode::Numpad9
            | KeyCode::NumpadDecimal
    );
    let navigation = matches!(
        logical,
        Key::Named(
            NamedKey::PageUp
                | NamedKey::PageDown
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Insert
                | NamedKey::Delete
        )
    );
    let just_up = shift_up.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(50));
    if !(cfg!(windows) && numpad && navigation && just_up) {
        return;
    }
    for e in events.iter_mut().skip(from) {
        if let egui::Event::Key { modifiers, .. } = e {
            modifiers.shift = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shift+Page Down from the numpad on Windows: Shift released a moment
    /// before the key still counts (#468); a Page Down long after does not.
    #[test]
    #[cfg(windows)]
    fn numpad_navigation_keeps_a_just_released_shift() {
        use winit::keyboard::{Key, NamedKey};
        let page_down = || egui::Event::Key {
            key: egui::Key::PageDown,
            physical_key: Some(egui::Key::Num3),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let shifted =
            |e: &egui::Event| matches!(e, egui::Event::Key { modifiers, .. } if modifiers.shift);
        let logical = Key::Named(NamedKey::PageDown);
        let mut events = vec![page_down()];
        let now = std::time::Instant::now();
        restore_shift(&mut events, 0, KeyCode::Numpad3, &logical, Some(now));
        assert!(shifted(&events[0]));
        let mut events = vec![page_down()];
        let long_ago = now.checked_sub(std::time::Duration::from_secs(1));
        restore_shift(&mut events, 0, KeyCode::Numpad3, &logical, long_ago);
        assert!(!shifted(&events[0]));
        // The dedicated Page Down key is left alone.
        let mut events = vec![page_down()];
        restore_shift(&mut events, 0, KeyCode::PageDown, &logical, Some(now));
        assert!(!shifted(&events[0]));
    }

    /// Alt pressed after the last frame still counts for numpad `*` (#537).
    #[test]
    fn queued_modifier_changes_count() {
        let pending = [
            egui::Event::ModifiersChanged(egui::Modifiers::ALT),
            egui::Event::Text("*".into()),
        ];
        assert!(current_modifiers(&pending, egui::Modifiers::NONE).alt);
        assert!(!current_modifiers(&[], egui::Modifiers::NONE).alt);
    }

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
