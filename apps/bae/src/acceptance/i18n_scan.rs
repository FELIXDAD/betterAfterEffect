//! #152: with `BAE_LANG=zh`, after a walk through the main flow, the visible
//! text of the accessibility tree (what Narrator / VoiceOver read and UI
//! Automation sees) has no English words — apart from what the user typed
//! (text field contents), proper nouns, key legends and command ids.

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{AccessKitNode, NodeT as _, Queryable as _};

use super::*;

/// Proper nouns and key legends that stay in Latin letters in a Chinese UI.
pub(super) const ALLOWED: &[&str] = &[
    // Product, formats, color models, codecs.
    "BAE",
    "PNG",
    "TIFF",
    "EXR",
    "OpenEXR",
    "RGB",
    "RGBA",
    "HEVC",
    "ProRes",
    "HQ",
    // AE's Chinese output module keeps "RGB + Alpha".
    "Alpha",
    // AE's zh_CN keeps it: "3D 位置 Gizmo", "向前循环 3D Gizmo 模式" (KB/S/Tools).
    "Gizmo",
    // Frame rate unit: AE zh_CN keeps it ("社交媒体纵向 HD • 1080x1920 • 30 fps",
    // docs/reference/ae/preferences.json); the timeline shows "(30.00 fps)".
    "fps",
    // AE (keyboard layout name, "Roto 笔刷工具") and BAE's Agent workspace
    // (docs/glossary.md).
    "After",
    "Effects",
    "Roto",
    "Agent",
    // About: the license (SPDX id) and the platform the build is for.
    "Apache",
    "aarch64",
    "macos",
    "windows",
    "linux",
    // AE's official Chinese keeps this abbreviation: the Preferences page
    // "Memory & CPU" is "内存和 CPU" (docs/reference/ae/menus.json, 编辑 ▸ 首选项).
    "CPU",
    // Settings pages, dialogs and tool options that AE's official Chinese keeps in
    // Latin letters: docs/reference/ae/ui/preferences-dialog.json (labelZh: "与
    // After Effects 链接的 Dynamic Link…", "不确定的媒体 NTSC:", "将未标记的 Alpha
    // 解释为:", "44100 Hz", "MME", "XMP 元数据", "导入时将 XMP ID 写入文件",
    // "RAM 可用 [AE/Pr/AME/Au]:", "启用 Mercury Transmit", "启用 JavaScript 调试器",
    // MB / GB), composition-settings.json ("社交媒体横向 HD", "7.9 MB"), menus.json
    // ("RotoBezier").
    "Dynamic",
    "Link",
    "NTSC",
    "Alpha",
    "MB",
    "GB",
    "Hz",
    "MME",
    "XMP",
    "ID",
    "RAM",
    "AE",
    "Pr",
    "AME",
    "Au",
    "Mercury",
    "Transmit",
    "JavaScript",
    "HD",
    "RotoBezier",
    // GPU APIs (the adapter's own name is added at run time).
    "GPU",
    "Metal",
    "Vulkan",
    "Dx12",
    "DX12",
    "Gl",
    "GL",
    "WebGPU",
    // Keyboard Shortcuts: AE's zh_CN keeps the keyboard layout codes ("键盘布局 - en")
    // and the key cap legends of its keyboard ("⌥ Opt", "Num Lock").
    "en",
    "Opt",
    "Num",
    "Lock",
    // Key legends (shortcuts are shown as on the keyboard).
    "Ctrl",
    "Shift",
    "Alt",
    "Cmd",
    "Option",
    "Win",
    "PageUp",
    "PageDown",
    "Home",
    "End",
    "Delete",
    "Backspace",
    "Enter",
    "Return",
    "Tab",
    "Space",
    "Esc",
    "Escape",
    "Up",
    "Down",
    "Left",
    "Right",
    "Insert",
];

/// Visible texts with English words, as `(role, text)`.
fn english(h: &Ui, allowed: &[String]) -> Vec<String> {
    fn walk(n: AccessKitNode<'_>, out: &mut Vec<(Role, String)>) {
        // What the user typed is theirs.
        let typed = matches!(
            n.role(),
            Role::TextInput | Role::MultilineTextInput | Role::SearchInput
        );
        if let Some(l) = n.label() {
            out.push((n.role(), l));
        }
        if !typed {
            if let Some(v) = n.value() {
                out.push((n.role(), v));
            }
            for c in n.children() {
                walk(c, out);
            }
        }
    }
    let mut texts = Vec::new();
    walk(h.root().accesskit_node(), &mut texts);
    let mut found = Vec::new();
    for (role, text) in texts {
        let words: Vec<&str> = text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_')
            .filter(|w| !w.is_empty())
            // Command ids (`motion.toggle_key`) are identifiers.
            .filter(|w| !(w.contains('.') && w.chars().any(|c| c.is_ascii_lowercase())))
            .flat_map(|w| w.split(['.', '_']))
            .filter(|w| w.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 2)
            .filter(|w| !allowed.iter().any(|a| a == w))
            .collect();
        if !words.is_empty() {
            let line = format!("{role:?} “{text}”: {}", words.join(", "));
            if !found.contains(&line) {
                found.push(line);
            }
        }
    }
    found
}

/// Clicks every “展开” (Expand) twirl in the timeline, a few levels deep.
fn expand_all(h: &mut Ui) {
    for _ in 0..4 {
        let expand = bae_ui::tr("Expand");
        let twirls: Vec<Rect> = h
            .query_all_by_role(Role::DisclosureTriangle)
            .chain(h.query_all_by_role(Role::Button))
            .filter(|n| {
                n.accesskit_node()
                    .label()
                    .is_some_and(|l| l.ends_with(expand))
            })
            .map(|n| n.rect())
            .collect();
        if twirls.is_empty() {
            return;
        }
        for r in twirls {
            click_at(h, r.center());
        }
        settle(h);
    }
}

#[test]
fn chinese_ui_has_no_english_words() {
    if !in_child(
        "acceptance::i18n_scan::chinese_ui_has_no_english_words",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut g = Gui::new();
    let mut allowed: Vec<String> = ALLOWED.iter().map(|s| s.to_string()).collect();
    if let Some(gpu) = gpu() {
        // The adapter's name is a proper noun ("Apple M4", "NVIDIA GeForce …").
        allowed.extend(
            gpu.adapter_info()
                .name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .map(str::to_owned),
        );
        g.set_gpu_status(crate::ui::gpu_summary(&gpu));
    }
    let mut h = gui_with(g);
    let mut problems = Vec::new();
    let scan = |h: &mut Ui, step: &str, problems: &mut Vec<String>| {
        settle(h);
        for p in english(h, &allowed) {
            problems.push(format!("{step}: {p}"));
        }
    };
    scan(&mut h, "start", &mut problems);
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.text",
            "shape:rect",
        ],
    );
    expand_all(&mut h);
    scan(&mut h, "comp with layers, expanded", &mut problems);
    key(&mut h, Modifiers::COMMAND, Key::M);
    scan(&mut h, "render queue", &mut problems);
    for title in ["Effect Controls", "Project"] {
        let tab = h
            .query_all_by_label(bae_ui::tr(title))
            .map(|n| n.rect())
            .find(|r| (r.height() - bae_ui::theme::metrics::TAB_BAR_H).abs() < 1.0);
        if let Some(r) = tab {
            click_at(&mut h, r.center());
        }
        scan(&mut h, title, &mut problems);
    }
    // The project's default folders carry Chinese names (paths stay English).
    let stage = h.state().stage();
    for (path, zh) in [
        ("/comps", "合成"),
        ("/assets", "素材"),
        ("/assets/solids", "纯色"),
    ] {
        let name = bae_motion::query::display_name_of(stage, &PrimPath::new(path).unwrap());
        if name != zh {
            problems.push(format!("folder {path} is named “{name}”, not “{zh}”"));
        }
    }
    h.state_mut()
        .invoke(Invocation::new(ids::KEYBOARD_SHORTCUTS));
    scan(&mut h, "keyboard shortcuts", &mut problems);
    key(&mut h, Modifiers::NONE, Key::Escape);
    h.state_mut().invoke(Invocation::new(ids::ABOUT));
    scan(&mut h, "about", &mut problems);
    for ws in ["review", "agent"] {
        h.state_mut().invoke(Invocation::with_params(
            ids::SWITCH_WORKSPACE,
            serde_json::json!({ "id": ws }),
        ));
        scan(&mut h, ws, &mut problems);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
