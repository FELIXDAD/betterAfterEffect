//! Texts of the Keyboard Shortcuts dialog: shortcuts written the way AE's dialog
//! writes them (`⇧+⌘+1` on macOS, `Ctrl+Shift+1` on Windows), AE's key names
//! and the dialog's Chinese strings (AE's zh_CN dictionary,
//! docs/reference/ae/mac/ui/keyboard-shortcuts.json).

use egui::Key;

use crate::i18n::{Lang, lang};
use crate::keymap::Chord;

/// Whether shortcuts are shown with the macOS modifier glyphs.
pub const MAC: bool = cfg!(target_os = "macos");

/// A modifier combination, in the order AE's Key table lists them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    /// ⌘ on macOS, Ctrl elsewhere (`Chord::command`).
    pub command: bool,
    pub alt: bool,
    pub shift: bool,
    /// The macOS Control key (`Chord::mac_ctrl`).
    pub ctrl: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        command: false,
        alt: false,
        shift: false,
        ctrl: false,
    };

    /// The chord of `key` with these modifiers.
    pub fn chord(self, key: Key) -> Chord {
        Chord {
            key,
            command: self.command,
            mac_ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
        }
    }

    pub fn of(c: &Chord) -> Mods {
        Mods {
            command: c.command,
            alt: c.alt,
            shift: c.shift,
            ctrl: c.mac_ctrl,
        }
    }

    /// The rows of AE's Key table: None, ⌘, ⌥, ⇧, ⌥+⌘, ⇧+⌘, ⌥+⇧, ⌥+⇧+⌘, then
    /// the same with ⌃ (macOS). Windows has no separate Control key: 8 rows.
    pub fn table() -> Vec<Mods> {
        let base = [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ];
        let ctrls: &[bool] = if MAC { &[false, true] } else { &[false] };
        ctrls
            .iter()
            .flat_map(|&ctrl| {
                base.iter().map(move |&(command, alt, shift)| Mods {
                    command,
                    alt,
                    shift,
                    ctrl,
                })
            })
            .collect()
    }

    /// `⌃+⌥+⇧+⌘` (macOS) or `Ctrl+Alt+Shift`; [`none_text`] for no modifier.
    pub fn text(self) -> String {
        if self == Mods::NONE {
            return none_text().into();
        }
        let mut s = self.prefix();
        s.pop();
        s
    }

    /// The modifiers as a chord prefix: `⌃+⌥+⇧+⌘+` / `Ctrl+Alt+Shift+`.
    pub fn prefix(self) -> String {
        let mut s = String::new();
        if MAC {
            for (on, m) in [
                (self.ctrl, "\u{2303}+"),
                (self.alt, "\u{2325}+"),
                (self.shift, "\u{21e7}+"),
                (self.command, "\u{2318}+"),
            ] {
                if on {
                    s.push_str(m);
                }
            }
        } else {
            for (on, m) in [
                (self.command || self.ctrl, "Ctrl+"),
                (self.alt, "Alt+"),
                (self.shift, "Shift+"),
            ] {
                if on {
                    s.push_str(m);
                }
            }
        }
        s
    }
}

/// "None" in the Key table (AE: 无).
pub fn none_text() -> &'static str {
    match lang() {
        Lang::Zh => "无",
        Lang::En => "None",
    }
}

/// A shortcut as AE's Keyboard Shortcuts dialog writes it: `⇧+⌘+1`, `⌥+Up`,
/// `Return` (macOS), `Ctrl+Shift+1` (Windows).
pub fn chord_text(c: &Chord) -> String {
    Mods::of(c).prefix() + &key_name(c.key)
}

/// Several shortcuts as one cell of the tree: `⌘+N, ⌘+K`.
pub fn chords_text<'a>(chords: impl IntoIterator<Item = &'a Chord>) -> String {
    chords
        .into_iter()
        .map(chord_text)
        .collect::<Vec<_>>()
        .join(", ")
}

/// AE's name of a key in the dialog (tree, Key table, key caps), in the UI
/// language (AE's zh_CN dictionary: `$$$/dvaui/ksv/*Key`).
pub fn key_name(key: Key) -> String {
    if let Some((_, _, label)) = crate::keymap::NUMPAD.iter().find(|(_, k, _)| *k == key) {
        return crate::i18n::tr(label).to_owned();
    }
    let zh = lang() == Lang::Zh;
    let named = |en: &'static str, zh_name: &'static str| if zh { zh_name } else { en };
    let s: &str = match key {
        Key::Enter => named("Return", "返回"),
        Key::Escape => named("Escape", "Esc"),
        Key::Tab => "Tab",
        Key::Space => named("Space", "空格"),
        // macOS calls Backspace "Delete" and Delete "Forward Delete".
        Key::Backspace if MAC => named("Delete", "删除"),
        Key::Backspace => "Backspace",
        Key::Delete if MAC => named("Forward Delete", "向前删除"),
        Key::Delete => named("Delete", "删除"),
        Key::Insert => "Insert",
        Key::Home => named("Home", "开头"),
        Key::End => named("End", "结束"),
        Key::PageUp => named("Page Up", "页上移"),
        Key::PageDown => named("Page Down", "页下移"),
        Key::ArrowUp => named("Up", "向上"),
        Key::ArrowDown => named("Down", "向下"),
        Key::ArrowLeft => named("Left", "向左"),
        Key::ArrowRight => named("Right", "向右"),
        Key::Comma => ",",
        Key::Period => ".",
        Key::Minus => "-",
        Key::Plus => "+",
        Key::Equals => "=",
        Key::Semicolon => ";",
        Key::Colon => ":",
        Key::Backslash => "\\",
        Key::Slash => "/",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Backtick => "`",
        Key::Quote => "'",
        Key::Questionmark => "?",
        Key::Exclamationmark => "!",
        Key::Pipe => "|",
        Key::OpenCurlyBracket => "{",
        Key::CloseCurlyBracket => "}",
        other => other.symbol_or_name(),
    };
    s.to_owned()
}

/// Chinese for the dialog's strings (AE's zh_CN dictionary keys in
/// docs/reference/ae/mac/ui/keyboard-shortcuts.json).
pub const ZH: &[(&str, &str)] = &[
    ("Preset:", "预设:"),
    ("Save As...", "另存为..."),
    ("Delete", "删除"),
    ("Copy To Clipboard", "复制到剪贴板"),
    ("Keyboard Layout:", "键盘布局 -"),
    ("Commands:", "命令:"),
    ("Application", "应用程序"),
    ("Panels", "面板"),
    ("Command", "命令"),
    ("Shortcut", "快捷键"),
    ("Modifiers", "修饰符"),
    ("Key: ", "键: "),
    (" (multiple commands assigned)", "（已分配多个命令）"),
    ("Undo", "撤消"),
    ("Clear", "清除"),
    (
        "Create new or edit existing shortcut by clicking in the \"Shortcut\" column",
        "通过单击“快捷键”列创建新快捷键或编辑现有快捷键",
    ),
    (
        "The shortcut: {chord} is already in use by another Application command: {command}. That command will no longer have a shortcut.",
        "快捷键“{chord}”已被另一个应用程序命令“{command}”使用。该命令将不再有快捷键。",
    ),
    (
        "The shortcut: {chord} is already in use by another command of this panel: {command}. That command will no longer have a shortcut.",
        "快捷键“{chord}”已被此面板“{command}”的另一个命令使用。该命令将不再有快捷键。",
    ),
    (
        "The shortcut: {chord} is already in use by the Application command: {command}. That command will be overridden by this panel command when the panel has focus.",
        "快捷键“{chord}”已被应用程序命令“{command}”使用，切换到该面板时，该命令将被此面板命令覆盖。",
    ),
    // The legend under the keyboard ($$$/VisualShortcutsDialog/legend/*).
    (
        "Application shortcuts are active regardless of panel focus",
        "无论面板焦点如何，应用程序快捷键都处于活动状态",
    ),
    (
        "Panel shortcuts override application shortcuts when the panel has focus",
        "面板具有焦点时，面板快捷键会覆盖应用程序快捷键",
    ),
    ("Modifier key pressed", "按下辅助按键"),
    ("Default.txt", "默认值.txt"),
    ("Untitled Custom Preset.txt", "无标题自定义预设.txt"),
    ("Create Preset", "创建预设"),
    ("Preset Name:", "预设名称 -"),
    ("Save", "保存"),
    ("Copied to clipboard", "复制到剪贴板"),
    (
        "The key set name you entered is not valid. Please choose another name.",
        "您输入的键组名称无效。请选择另一个名称。",
    ),
    (
        "A custom shortcut set named “{name}” already exists. Do you want to overwrite it?",
        "已存在名为“{name}”的自定义快捷键集。是否要将其覆盖？",
    ),
    (
        "Are you sure you want to delete the keyboard layout set “{name}”?",
        "是否确定要删除键盘布局集“{name}”？",
    ),
    ("Custom.txt", "自定义.txt"),
    // The scopes of the Commands menu and the Panels groups (AE: $$$/AE/KB/*).
    ("Character and Paragraph Panels", "字符和段落面板"),
    ("Composition Panel", "合成面板"),
    ("Effect Controls Panel", "效果控件面板"),
    ("Flowchart Panel", "流程图面板"),
    ("Layer Panel", "图层面板"),
    ("Paint Panel", "绘画面板"),
    ("Project Panel", "项目面板"),
    ("Render Queue Panel", "渲染队列面板"),
    ("Timeline Panel", "时间轴面板"),
    ("Tracker Panel", "跟踪器面板"),
    ("Viewer Panel", "查看器面板"),
    ("Dialog Boxes", "对话框"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_are_written_like_ae() {
        let c = Chord::parse("Cmd+Shift+1").unwrap();
        let k = Chord::parse("Alt+Up").unwrap();
        if MAC {
            assert_eq!(chord_text(&c), "\u{21e7}+\u{2318}+1");
            let all = Chord::parse("Ctrl+Cmd+Alt+Shift+J").unwrap();
            assert_eq!(chord_text(&all), "\u{2303}+\u{2325}+\u{21e7}+\u{2318}+J");
            assert_eq!(chords_text([&c, &k]), "\u{21e7}+\u{2318}+1, \u{2325}+Up");
            assert_eq!(key_name(Key::Backspace), "Delete");
            assert_eq!(key_name(Key::Delete), "Forward Delete");
        } else {
            assert_eq!(chord_text(&c), "Ctrl+Shift+1");
            assert_eq!(chord_text(&k), "Alt+Up");
            assert_eq!(key_name(Key::Delete), "Delete");
        }
        assert_eq!(chord_text(&Chord::parse("Enter").unwrap()), "Return");
        assert_eq!(chord_text(&Chord::parse("Quote").unwrap()), "'");
    }

    #[test]
    fn the_key_table_lists_every_combination_once() {
        let rows = Mods::table();
        assert_eq!(rows.len(), if MAC { 16 } else { 8 });
        assert_eq!(rows[0], Mods::NONE);
        for (i, a) in rows.iter().enumerate() {
            assert!(!rows[i + 1..].contains(a));
        }
        if MAC {
            // AE: None, ⌘, ⌥, ⇧, ⌥+⌘, ⇧+⌘, ⌥+⇧, ⌥+⇧+⌘, ⌃ …
            let texts: Vec<String> = rows[1..9].iter().map(|m| m.text()).collect();
            assert_eq!(texts, ["⌘", "⌥", "⇧", "⌥+⌘", "⇧+⌘", "⌥+⇧", "⌥+⇧+⌘", "⌃"]);
        }
    }
}
