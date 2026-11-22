//! Keyboard shortcuts: chords bound to command ids, one keymap per workspace.
//!
//! A [`Keymap`] (registered in `bae_app::Registry<Keymap>`, e.g. `"ae"`, `"ai"`) is a
//! list of [`Binding`]s. Each workspace names its keymap, so switching workspaces
//! switches shortcuts. Users customize a keymap with [`KeymapOverrides`] (added
//! and removed bindings, saved in their settings); [`Keymap::conflicts`] reports
//! chords bound twice in the same scope.
//!
//! Chords are written like AE's documentation: `"Cmd+Shift+Z"`, `"Alt+["`, `"U"`.
//! `Cmd` is the platform command key (⌘ on macOS, Ctrl on Windows); `Ctrl` is the
//! macOS Control key (on Windows it means the same as `Cmd`).

use std::fmt;

use egui::{Key, Modifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A key plus modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    /// ⌘ on macOS, Ctrl elsewhere.
    pub command: bool,
    /// The macOS Control key. Elsewhere it is folded into `command` when parsing.
    pub mac_ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Chord {
    pub const fn key(key: Key) -> Self {
        Chord {
            key,
            command: false,
            mac_ctrl: false,
            shift: false,
            alt: false,
        }
    }

    pub const fn cmd(mut self) -> Self {
        self.command = true;
        self
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Whether the modifiers of a key event match this chord exactly.
    pub fn matches_modifiers(&self, m: Modifiers) -> bool {
        // egui sets `command` from ⌘ on macOS and from Ctrl elsewhere; on macOS
        // `ctrl` is the Control key.
        let mac_ctrl = cfg!(target_os = "macos") && m.ctrl;
        m.command == self.command
            && mac_ctrl == self.mac_ctrl
            && m.shift == self.shift
            && m.alt == self.alt
    }

    /// Whether a key event (logical key, physical key, modifiers) triggers this chord.
    /// The physical key is checked too, so `Shift+[` matches even though the
    /// logical key is `{`, and shortcuts keep working with non-Latin layouts.
    pub fn matches(&self, key: Key, physical: Option<Key>, modifiers: Modifiers) -> bool {
        (key == self.key || physical == Some(self.key)) && self.matches_modifiers(modifiers)
    }

    /// Parse `"Cmd+Shift+Z"`. Modifier names are case-insensitive; `Cmd`, `Command`,
    /// `Meta` and `⌘` are the command key, `Opt`/`Option`/`Alt` the alt key.
    pub fn parse(s: &str) -> Result<Chord, ChordError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ChordError(s.to_owned()));
        }
        // Split on '+' but allow the key itself to be '+' ("Cmd++", "+").
        let (mods, key) = match s.strip_suffix("++") {
            Some(rest) => (rest, "+"),
            None if s == "+" => ("", "+"),
            None => match s.rfind('+') {
                Some(i) => (&s[..i], &s[i + 1..]),
                None => ("", s),
            },
        };
        let key = Key::from_name(key).ok_or_else(|| ChordError(s.to_owned()))?;
        let mut chord = Chord::key(key);
        for m in mods.split('+').filter(|m| !m.is_empty()) {
            match m.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "meta" | "⌘" | "super" => chord.command = true,
                "ctrl" | "control" | "⌃" => {
                    if cfg!(target_os = "macos") {
                        chord.mac_ctrl = true;
                    } else {
                        chord.command = true;
                    }
                }
                "shift" | "⇧" => chord.shift = true,
                "alt" | "opt" | "option" | "⌥" => chord.alt = true,
                _ => return Err(ChordError(s.to_owned())),
            }
        }
        Ok(chord)
    }

    /// Display text for menus and the shortcut editor: `⌃⌥⇧⌘Z` on macOS,
    /// `Ctrl+Alt+Shift+Z` elsewhere.
    pub fn display(&self) -> String {
        let key = key_label(self.key);
        if cfg!(target_os = "macos") {
            let mut s = String::new();
            if self.mac_ctrl {
                s.push('⌃');
            }
            if self.alt {
                s.push('⌥');
            }
            if self.shift {
                s.push('⇧');
            }
            if self.command {
                s.push('⌘');
            }
            s + key
        } else {
            let mut parts = Vec::new();
            if self.command {
                parts.push("Ctrl");
            }
            if self.alt {
                parts.push("Alt");
            }
            if self.shift {
                parts.push("Shift");
            }
            parts.push(key);
            parts.join("+")
        }
    }

    /// Whether the chord types text when no modifier (other than Shift) is held.
    /// Such shortcuts must not fire while a text field has keyboard focus.
    pub fn is_typing_key(&self) -> bool {
        !self.command && !self.mac_ctrl && !self.alt && !is_navigation_key(self.key)
    }
}

fn is_navigation_key(key: Key) -> bool {
    use Key::*;
    matches!(
        key,
        Escape
            | F1
            | F2
            | F3
            | F4
            | F5
            | F6
            | F7
            | F8
            | F9
            | F10
            | F11
            | F12
            | PageUp
            | PageDown
            | Home
            | End
    )
}

/// Key name for display. Arrow keys are `←→↑↓`, which the system UI fonts have (egui's
/// `⏴⏵⏶⏷` are only in egui's bundled fonts, which BAE does not ship).
fn key_label(key: Key) -> &'static str {
    match key {
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::Escape => "Esc",
        Key::Comma => ",",
        Key::Period => ".",
        Key::Minus => "-",
        Key::Plus => "+",
        Key::Equals => "=",
        Key::Semicolon => ";",
        Key::Backslash => "\\",
        Key::Slash => "/",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Backtick => "`",
        Key::Quote => "'",
        other => other.symbol_or_name(),
    }
}

impl fmt::Display for Chord {
    /// Canonical, platform-independent form accepted by [`Chord::parse`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mac_ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.command {
            f.write_str("Cmd+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        f.write_str(self.key.name())
    }
}

impl Serialize for Chord {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Chord::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// A chord string that could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid shortcut {0:?}")]
pub struct ChordError(pub String);

/// One shortcut: a chord bound to a command id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub chord: Chord,
    /// Command id (see [`crate::action`]).
    pub action: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
    /// Only active while a panel of this kind is focused; `None` = everywhere.
    /// A panel-scoped binding wins over a global one with the same chord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

impl Binding {
    /// `Binding::new("Cmd+Z", "edit.undo")`. Panics on an invalid chord, so use it
    /// for built-in tables (covered by tests), not for user input.
    pub fn new(chord: &str, action: &str) -> Self {
        Binding {
            chord: Chord::parse(chord).unwrap_or_else(|e| panic!("{e}")),
            action: action.to_owned(),
            params: serde_json::Value::Null,
            when: None,
        }
    }

    pub fn params(mut self, params: serde_json::Value) -> Self {
        self.params = params;
        self
    }

    pub fn when(mut self, panel_kind: &str) -> Self {
        self.when = Some(panel_kind.to_owned());
        self
    }

    /// The invocation this binding triggers.
    pub fn invocation(&self) -> crate::action::Invocation {
        crate::action::Invocation::with_params(self.action.clone(), self.params.clone())
    }

    fn same_slot(&self, chord: &Chord, when: Option<&str>) -> bool {
        self.chord == *chord && self.when.as_deref() == when
    }
}

/// A named set of bindings (registered in `Registry<Keymap>`, id e.g. `"ae"`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Keymap {
    /// English title ("After Effects"), displayed through [`crate::i18n::tr`].
    pub title: String,
    pub bindings: Vec<Binding>,
}

/// Two or more bindings on the same chord in the same scope.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub chord: Chord,
    pub when: Option<String>,
    /// Indices into [`Keymap::bindings`].
    pub bindings: Vec<usize>,
}

impl Keymap {
    pub fn new(title: impl Into<String>, bindings: Vec<Binding>) -> Self {
        Keymap {
            title: title.into(),
            bindings,
        }
    }

    /// The chord bound to exactly this invocation (action and parameters), e.g. the
    /// shortcut a menu item shows. Global bindings first.
    pub fn chord_for_invocation(&self, inv: &crate::action::Invocation) -> Option<&Chord> {
        let same = |b: &&Binding| {
            b.action == inv.id
                && (b.params == inv.params
                    || (b.params.is_null() && inv.params == serde_json::json!({}))
                    || (inv.params.is_null() && b.params == serde_json::json!({})))
        };
        self.bindings
            .iter()
            .filter(same)
            .find(|b| b.when.is_none())
            .or_else(|| self.bindings.iter().find(same))
            .map(|b| &b.chord)
    }

    /// The binding for a chord, preferring one scoped to the focused panel kind.
    pub fn lookup(&self, chord: &Chord, focused_kind: Option<&str>) -> Option<&Binding> {
        let scoped = focused_kind.and_then(|kind| {
            self.bindings
                .iter()
                .find(|b| b.chord == *chord && b.when.as_deref() == Some(kind))
        });
        scoped.or_else(|| {
            self.bindings
                .iter()
                .find(|b| b.chord == *chord && b.when.is_none())
        })
    }

    /// The binding a key event triggers (see [`Chord::matches`]).
    pub fn match_event(
        &self,
        key: Key,
        physical: Option<Key>,
        modifiers: Modifiers,
        focused_kind: Option<&str>,
    ) -> Option<&Binding> {
        let candidates = self
            .bindings
            .iter()
            .filter(|b| b.chord.matches(key, physical, modifiers));
        let mut global = None;
        for b in candidates {
            match b.when.as_deref() {
                Some(kind) if Some(kind) == focused_kind => return Some(b),
                None if global.is_none() => global = Some(b),
                _ => {}
            }
        }
        global
    }

    /// Chords bound to `action` (for menu hints), global bindings first.
    pub fn chords_for(&self, action: &str) -> impl Iterator<Item = &Chord> + '_ {
        let action = action.to_owned();
        let mut list: Vec<&Binding> = self
            .bindings
            .iter()
            .filter(|b| b.action == action && b.params.is_null())
            .collect();
        list.sort_by_key(|b| b.when.is_some());
        list.into_iter().map(|b| &b.chord)
    }

    /// Chords bound more than once in the same scope.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut out: Vec<Conflict> = Vec::new();
        for (i, b) in self.bindings.iter().enumerate() {
            if let Some(c) = out
                .iter_mut()
                .find(|c| b.same_slot(&c.chord, c.when.as_deref()))
            {
                c.bindings.push(i);
                continue;
            }
            if self.bindings[i + 1..]
                .iter()
                .any(|o| o.same_slot(&b.chord, b.when.as_deref()))
            {
                out.push(Conflict {
                    chord: b.chord,
                    when: b.when.clone(),
                    bindings: vec![i],
                });
            }
        }
        out
    }

    /// What binding `chord` in scope `when` would collide with, if anything
    /// (for the shortcut editor, before the user confirms).
    pub fn conflict_for(&self, chord: &Chord, when: Option<&str>) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.same_slot(chord, when))
    }

    /// This keymap with the user's changes applied: removals first, then additions
    /// (an added binding replaces whatever had the same chord and scope).
    pub fn with_overrides(&self, overrides: &KeymapOverrides) -> Keymap {
        let mut out = self.clone();
        out.bindings.retain(|b| {
            !overrides
                .remove
                .iter()
                .any(|r| b.same_slot(&r.chord, r.when.as_deref()))
        });
        for add in &overrides.add {
            out.bindings
                .retain(|b| !b.same_slot(&add.chord, add.when.as_deref()));
            out.bindings.push(add.clone());
        }
        out
    }
}

/// Bindings a module adds to an existing keymap (`Registry<KeymapBindings>`, any
/// unique id such as `"motion.timeline"`). The workbench merges every extension
/// targeting the active keymap after the keymap's own bindings and before the
/// user's overrides, so workspace crates add panel shortcuts (`when:
/// Some("timeline")`) without replacing the shell's keymap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapBindings {
    /// Id of the keymap to extend (`"ae"`).
    pub keymap: String,
    pub bindings: Vec<Binding>,
    /// Workspace of the contributing module: while it is active, these bindings
    /// win over other modules' bindings on the same chord (otherwise the first
    /// registered extension wins). `None` = no preference.
    pub workspace: Option<String>,
}

impl KeymapBindings {
    pub fn new(keymap: impl Into<String>, bindings: Vec<Binding>) -> Self {
        KeymapBindings {
            keymap: keymap.into(),
            bindings,
            workspace: None,
        }
    }

    /// Prefer these bindings while `workspace` is active.
    pub fn for_workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }
}

/// A binding slot (chord + scope) removed by the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unbind {
    pub chord: Chord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

/// The user's customization of one keymap, stored in their settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeymapOverrides {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<Binding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<Unbind>,
}

impl KeymapOverrides {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    /// Bind `chord` (in scope `when`) to `binding`'s action, replacing any binding
    /// in that slot.
    pub fn bind(&mut self, binding: Binding) {
        self.add
            .retain(|b| !b.same_slot(&binding.chord, binding.when.as_deref()));
        self.remove
            .retain(|r| !(r.chord == binding.chord && r.when == binding.when));
        self.add.push(binding);
    }

    /// Remove whatever is bound to `chord` in scope `when`.
    pub fn unbind(&mut self, chord: Chord, when: Option<String>) {
        self.add.retain(|b| !b.same_slot(&chord, when.as_deref()));
        let unbind = Unbind { chord, when };
        if !self.remove.contains(&unbind) {
            self.remove.push(unbind);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_roundtrip() {
        for s in [
            "U",
            "Cmd+Z",
            "Cmd+Shift+Z",
            "Alt+OpenBracket",
            "Shift+F10",
            "Cmd+Plus",
            "Space",
            "Backtick",
        ] {
            let c = Chord::parse(s).unwrap();
            assert_eq!(Chord::parse(&c.to_string()).unwrap(), c, "{s}");
        }
        assert_eq!(
            Chord::parse("cmd+shift+z").unwrap(),
            Chord::key(Key::Z).cmd().shift()
        );
        assert_eq!(
            Chord::parse("Alt+[").unwrap(),
            Chord::key(Key::OpenBracket).alt()
        );
        assert_eq!(Chord::parse("Cmd++").unwrap(), Chord::key(Key::Plus).cmd());
        assert_eq!(Chord::parse("+").unwrap(), Chord::key(Key::Plus));
        assert!(Chord::parse("Hyper+Z").is_err());
        assert!(Chord::parse("Cmd+").is_err());
        assert!(Chord::parse("").is_err());
    }

    #[test]
    fn chord_serde_is_a_string() {
        let b = Binding::new("Cmd+Shift+Z", "edit.redo");
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, r#"{"chord":"Cmd+Shift+Z","action":"edit.redo"}"#);
        assert_eq!(serde_json::from_str::<Binding>(&json).unwrap(), b);
    }

    #[test]
    fn event_matching_uses_physical_key_and_exact_modifiers() {
        let c = Chord::parse("Shift+[").unwrap();
        let shift = Modifiers::SHIFT;
        assert!(c.matches(Key::OpenCurlyBracket, Some(Key::OpenBracket), shift));
        assert!(!c.matches(Key::OpenBracket, Some(Key::OpenBracket), Modifiers::NONE));
        let undo = Chord::parse("Cmd+Z").unwrap();
        assert!(undo.matches(Key::Z, None, Modifiers::COMMAND));
        assert!(!undo.matches(Key::Z, None, Modifiers::COMMAND | Modifiers::SHIFT));
        assert!(Chord::parse("U").unwrap().is_typing_key());
        assert!(!undo.is_typing_key());
        assert!(!Chord::parse("F9").unwrap().is_typing_key());
    }

    #[test]
    fn panel_scoped_bindings_win() {
        let km = Keymap::new(
            "test",
            vec![
                Binding::new("Equals", "view.zoom_in"),
                Binding::new("Equals", "timeline.zoom_in").when("timeline"),
            ],
        );
        let eq = Chord::key(Key::Equals);
        assert_eq!(km.lookup(&eq, None).unwrap().action, "view.zoom_in");
        assert_eq!(
            km.lookup(&eq, Some("project")).unwrap().action,
            "view.zoom_in"
        );
        assert_eq!(
            km.lookup(&eq, Some("timeline")).unwrap().action,
            "timeline.zoom_in"
        );
        let hit = km.match_event(Key::Equals, None, Modifiers::NONE, Some("timeline"));
        assert_eq!(hit.unwrap().action, "timeline.zoom_in");
        assert!(
            km.conflicts().is_empty(),
            "different scopes do not conflict"
        );
    }

    #[test]
    fn conflicts_and_overrides() {
        let km = Keymap::new(
            "test",
            vec![
                Binding::new("Cmd+Z", "edit.undo"),
                Binding::new("Cmd+K", "comp.settings"),
                Binding::new("Cmd+Z", "other.thing"),
            ],
        );
        let conflicts = km.conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].bindings, vec![0, 2]);
        assert_eq!(
            km.conflict_for(&Chord::parse("Cmd+K").unwrap(), None)
                .unwrap()
                .action,
            "comp.settings"
        );

        let mut ov = KeymapOverrides::default();
        ov.unbind(Chord::parse("Cmd+Z").unwrap(), None);
        ov.bind(Binding::new("Cmd+Z", "edit.undo"));
        ov.bind(Binding::new("Cmd+K", "comp.new"));
        let custom = km.with_overrides(&ov);
        assert!(custom.conflicts().is_empty());
        let k = Chord::parse("Cmd+K").unwrap();
        assert_eq!(custom.lookup(&k, None).unwrap().action, "comp.new");
        let json = serde_json::to_string(&ov).unwrap();
        assert_eq!(serde_json::from_str::<KeymapOverrides>(&json).unwrap(), ov);
    }
}
