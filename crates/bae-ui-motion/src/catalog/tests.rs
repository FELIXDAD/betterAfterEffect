//! Checks the catalog against AE's reference data and keeps
//! `docs/ui/ae-commands.md` in sync.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use serde_json::Value;

use super::*;

const MENUS_JSON: &str = include_str!("../../../../docs/reference/ae/menus.json");
const SHORTCUTS_JSON: &str = include_str!("../../../../docs/reference/ae/shortcuts.json");
const MAC_SHORTCUTS_JSON: &str = include_str!("../../../../docs/reference/ae/mac/shortcuts.json");
const DOC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/ui/ae-commands.md");

/// Submenus AE fills when they open (`menus.json` has them empty): BAE may list
/// entries there.
const FILLED_ON_OPEN: &[&str] = &[
    "History",
    "Team Project",
    "Layer Styles",
    "Variable Font Axes",
];

fn params(text: &str) -> Value {
    if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(text).unwrap_or_else(|e| panic!("bad params {text}: {e}"))
    }
}

/// One step of the walk: `menus.json` entries against catalog nodes.
fn compare(ae: &[Value], ours: &[Node], path: &str, errors: &mut Vec<String>) {
    let mut a = ae.iter().peekable();
    let mut o = ours.iter().peekable();
    loop {
        match (a.peek(), o.peek()) {
            (None, None) => break,
            (Some(x), None) => {
                let x = (*x).clone();
                errors.push(format!(
                    "{path}: AE has {} more entries, from {x}",
                    a.count()
                ));
                break;
            }
            (None, Some(n)) => {
                errors.push(format!("{path}: extra entry {n:?}"));
                break;
            }
            (Some(x), Some(n)) => {
                let kind = x["type"].as_str().unwrap_or_default();
                match (kind, n) {
                    ("separator", Node::Separator) => {}
                    ("item", Node::Item(it)) => {
                        let id = x["commandId"].as_u64().unwrap_or(0) as u32;
                        if id != it.ae {
                            errors.push(format!(
                                "{path}: AE item {} (id {id}) vs catalog {} (id {})",
                                x["label"], it.title, it.ae
                            ));
                        }
                        if id == 0 && it.status != Status::Dynamic {
                            errors.push(format!("{path}: run-time list {} not Dynamic", it.title));
                        }
                    }
                    ("submenu", Node::Submenu(s)) => {
                        let children = x["items"].as_array().cloned().unwrap_or_default();
                        let sub_path = format!("{path} ▸ {}", s.title);
                        if children.is_empty() && FILLED_ON_OPEN.contains(&s.title) {
                            // AE builds these when they open.
                        } else {
                            compare(&children, s.children, &sub_path, errors);
                        }
                    }
                    ("submenu", Node::Effects) => {
                        // The effect categories: every remaining AE submenu.
                        let rest: Vec<&Value> = a.by_ref().collect();
                        if rest.len() != EFFECT_CATEGORIES.len()
                            || rest.iter().any(|v| v["type"] != "submenu")
                        {
                            errors.push(format!(
                                "{path}: {} AE effect categories vs {}",
                                rest.len(),
                                EFFECT_CATEGORIES.len()
                            ));
                        }
                        o.next();
                        continue;
                    }
                    _ => errors.push(format!("{path}: AE {kind} {} vs catalog {n:?}", x["label"])),
                }
                a.next();
                o.next();
            }
        }
    }
}

/// The menu tree is AE 26's (Windows menus.json): same top-level menus,
/// submenus, order, separators and AE command ids. Fails when an entry is added,
/// dropped or moved on either side.
#[test]
fn menu_tree_matches_ae() {
    let json: Value = serde_json::from_str(MENUS_JSON).unwrap();
    let tops = json["menus"].as_array().unwrap();
    assert_eq!(tops.len(), MENUS.len());
    let mut errors = Vec::new();
    for (ae, (title, nodes)) in tops.iter().zip(MENUS) {
        compare(ae["items"].as_array().unwrap(), nodes, title, &mut errors);
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    // Every AE command id appears once (MRU placeholders aside).
    let mut seen = HashSet::new();
    for (path, it) in menu_items() {
        if it.ae != 0 {
            assert!(seen.insert(it.ae), "{} listed twice ({path:?})", it.ae);
        }
    }
    assert_eq!(
        seen.len(),
        json["commandIdCount"].as_u64().unwrap() as usize - effect_count(&json)
    );
}

/// AE command ids of the effect items (the Effect menu lists them from the
/// effect registry instead).
fn effect_count(json: &Value) -> usize {
    fn count(v: &Value) -> usize {
        v["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|i| match i["type"].as_str() {
                        Some("item") => usize::from(i["commandId"].is_u64()),
                        Some("submenu") => count(i),
                        _ => 0,
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
    let effect = &json["menus"][4];
    effect["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["type"] == "submenu")
        .map(count)
        .sum()
}

/// Every command of AE's default shortcut file is in the key table, in AE's
/// order, with AE's shortcuts.
#[test]
fn every_ae_shortcut_command_is_mapped() {
    let json: Value = serde_json::from_str(SHORTCUTS_JSON).unwrap();
    let mut ae = Vec::new();
    for (ctx, cmds) in json["contexts"].as_object().unwrap() {
        for c in cmds.as_array().unwrap() {
            let keys: Vec<&str> = c["shortcuts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap())
                .collect();
            ae.push((
                ctx.clone(),
                c["command"].as_str().unwrap().to_owned(),
                keys.join("|"),
            ));
        }
    }
    let ours: Vec<(String, String, String)> = KEYS
        .iter()
        .filter(|k| k.context != "Menu" && k.flags & MAC_ONLY == 0)
        .map(|k| {
            (
                k.context.to_owned(),
                k.command.to_owned(),
                k.keys.to_owned(),
            )
        })
        .collect();
    assert_eq!(ours.len(), json["commandCount"].as_u64().unwrap() as usize);
    for (a, o) in ae.iter().zip(&ours) {
        assert_eq!(a, o);
    }
    for k in KEYS {
        params(k.params);
    }
}

/// On macOS every command has AE's macOS defaults (AE 26.2.1's own file): the
/// Windows chord with Ctrl → ⌘ and Alt → Option, or the `MAC_KEYS` exception.
#[test]
fn mac_chords_follow_the_mac_file() {
    let json: Value = serde_json::from_str(MAC_SHORTCUTS_JSON).unwrap();
    let mut errors = Vec::new();
    let mut count = 0;
    for (ctx, cmds) in json["contexts"].as_object().unwrap() {
        for c in cmds.as_array().unwrap() {
            let command = c["command"].as_str().unwrap();
            let keys: Vec<&str> = c["shortcuts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap())
                .collect();
            let mut want: Vec<String> = mac_chords(&keys.join("|"));
            let Some(row) = KEYS
                .iter()
                .find(|k| k.context == ctx && k.command == command)
            else {
                errors.push(format!("{ctx}/{command} missing"));
                continue;
            };
            let mut got = row.chords(true);
            want.sort();
            got.sort();
            if want != got {
                errors.push(format!("{ctx}/{command}: mac {want:?}, catalog {got:?}"));
            }
            count += 1;
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    assert_eq!(count, json["commandCount"].as_u64().unwrap() as usize);
    for (ctx, cmd, _) in MAC_KEYS {
        assert!(
            KEYS.iter().any(|k| k.context == *ctx && k.command == *cmd),
            "{ctx}/{cmd}"
        );
    }
}

/// AE's notation of a menu shortcut (`Ctrl+数字小键盘 0`, `Alt+\`) in the
/// shortcut file's spelling (`Ctrl+Pad0`, `Alt+Backslash`), modifiers sorted.
fn normalize(chord: &str) -> String {
    let chord = chord
        .replace("数字小键盘 ", "Pad")
        .replace("Numpad Enter", "Enter")
        .replace("Numpad /", "PadSlash")
        .replace("Numpad *", "PadMultiply");
    let (mods, key) = match chord.rsplit_once('+') {
        Some((m, k)) if !k.is_empty() => (m, k),
        _ => ("", chord.as_str()),
    };
    let key = match key {
        "\\" => "Backslash",
        "'" => "SingleQuote",
        "," => "Comma",
        "Home" => "HOME",
        "End" => "END",
        "PageUp" => "PageUP",
        "PageDown" => "PageDOWN",
        other => other,
    };
    let mut m: Vec<&str> = mods.split('+').filter(|s| !s.is_empty()).collect();
    m.sort_unstable();
    m.push(key);
    m.join("+")
}

/// The shortcut AE's menu shows for an item is bound, in the key table, to the
/// item's own action: the BAE menu then shows the same shortcut.
#[test]
fn menu_shortcuts_run_the_menu_action() {
    let json: Value = serde_json::from_str(MENUS_JSON).unwrap();
    let mut shown: BTreeMap<u32, String> = BTreeMap::new();
    fn collect(v: &Value, out: &mut BTreeMap<u32, String>) {
        for i in v["items"].as_array().into_iter().flatten() {
            if let (Some(id), Some(sc)) = (i["commandId"].as_u64(), i["shortcut"].as_str()) {
                out.insert(id as u32, sc.to_owned());
            }
            collect(i, out);
        }
    }
    for top in json["menus"].as_array().unwrap() {
        collect(top, &mut shown);
    }
    let mut errors = Vec::new();
    for (path, it) in menu_items() {
        let Some(sc) = shown.get(&it.ae) else {
            continue;
        };
        let want = normalize(sc);
        let found = KEYS.iter().any(|k| {
            k.action == it.action
                && params(k.params) == params(it.params)
                && k.keys.split('|').any(|c| normalize(c) == want)
        });
        if !found {
            errors.push(format!(
                "{} ▸ {} ({sc}) runs {} {}",
                path.join(" ▸ "),
                it.title,
                it.action,
                it.params
            ));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

/// Action ids follow the naming rule of `docs/ui/ae-commands.md`.
#[test]
fn action_ids_follow_the_naming_rule() {
    const AREAS: &[&str] = &[
        "file", "edit", "comp", "layer", "effect", "anim", "view", "window", "help", "tool",
        "timeline", "viewer", "project", "panel",
    ];
    const LEGACY: &[&str] = &[
        "app",
        "file",
        "edit",
        "window",
        "workspace",
        "help",
        "viewer",
        "timeline",
    ];
    let ok = |id: &str| {
        let parts: Vec<&str> = id.split('.').collect();
        let snake = parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
        snake
            && match parts.as_slice() {
                ["motion", area, _] => AREAS.contains(area),
                ["motion", _] => true,
                [ns, _] => LEGACY.contains(ns),
                _ => false,
            }
    };
    for (path, it) in menu_items() {
        assert!(ok(it.action), "{path:?} {}", it.action);
        params(it.params);
    }
    for k in KEYS {
        assert!(ok(k.action), "{}/{} {}", k.context, k.command, k.action);
    }
}

#[test]
fn ae_notation_becomes_bae_chords() {
    assert_eq!(chords("Ctrl+Alt+F", false), ["Cmd+Alt+F"]);
    assert_eq!(chords("Ctrl+Alt+F", true), ["Cmd+Alt+F"]);
    assert_eq!(chords("PadMultiply|macControl+8", false), ["PadMultiply"]);
    assert_eq!(
        chords("PadMultiply|macControl+8", true),
        ["PadMultiply", "Ctrl+8"]
    );
    assert_eq!(chords("Delete|FwdDel", false), ["Backspace", "Delete"]);
    assert_eq!(chords("Alt+Shift+Comma", false), ["Alt+Shift+Comma"]);
    assert_eq!(chords("Ctrl+SingleQuote", false), ["Cmd+Quote"]);
    assert_eq!(chords("", false), Vec::<String>::new());
    assert_eq!(
        display_chord("Cmd+Alt+Shift+Quote", false),
        "Ctrl+Alt+Shift+'"
    );
    assert_eq!(display_chord("Ctrl+Cmd+Alt+J", true), "Control+Cmd+Opt+J");
}

// ---------------------------------------------------------------- the document

fn status_mark(s: Status) -> &'static str {
    match s {
        Status::Done => "✅",
        Status::Partial => "◐",
        Status::Todo => "☐",
        Status::Later => "⏸",
        Status::Adobe => "Ⓐ",
        Status::Dynamic => "⋯",
    }
}

fn owner_name(o: Owner) -> &'static str {
    match o {
        Owner::A => "A",
        Owner::B => "B",
        Owner::C => "C",
        Owner::D => "D",
        Owner::E => "E",
        Owner::F => "F",
    }
}

/// Markdown-safe chord list for one platform.
fn shown_chords(keys: &[&Key], mac: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    for k in keys {
        if k.flags & NATIVE != 0 {
            continue;
        }
        for c in k.chords(mac) {
            let d = display_chord(&c, mac)
                .replace('`', "\\`")
                .replace('\\', "\\\\");
            let d = d.replace("\\\\`", "\\`");
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out.join(" / ")
}

fn code(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        format!("`{s}`")
    }
}

fn invocation_keys(action: &str, p: &str) -> Vec<&'static Key> {
    let p = params(p);
    KEYS.iter()
        .filter(|k| k.action == action && params(k.params) == p)
        .collect()
}

fn render_doc() -> String {
    let mut s = String::new();
    let items = menu_items();
    let _ = writeln!(s, "# AE 命令目录");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "> 由 `crates/bae-ui-motion/src/catalog/` 生成，不要手改。改了目录后运行 \
         `BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog` 重新生成；文档过期时这个测试会失败。"
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "这是 AE 一致性工作的总表：After Effects 26 主菜单的每一项（`docs/reference/ae/menus.json`，Windows）\
         和默认快捷键文件的每一条命令（`docs/reference/ae/shortcuts.json`）都对应一个 BAE 动作 id。\
         菜单栏、AE 键位表都从这份目录生成；测试逐项对照 AE 的参考数据，菜单少了、多了、顺序变了都会报错。\
         新加动作的 agent 按下面的规则起名，B（`ui/ae-menus`）在后续 PR 里把它补进目录。"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "## 动作 id 命名");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "- 已有的 id 不改：外壳的 `app.* file.* edit.* window.* workspace.* help.*`，动效工作区的 `motion.*` 和面板动作 `viewer.* timeline.*`，领域命令 `comp.* layer.* key.* attr.*`。"
    );
    let _ = writeln!(
        s,
        "- 新的界面动作用 `motion.<区域>.<名字>`：区域是 AE 顶层菜单（`file edit comp layer effect anim view window help`），或 `tool`（工具）、`timeline`、`viewer`、`project`、`panel`；名字是 AE 英文菜单文字转成 snake_case（去掉省略号）。例：`motion.comp.composition_settings`、`motion.tool.pen`、`motion.view.show_grid`、`motion.timeline.graph_editor`。"
    );
    let _ = writeln!(
        s,
        "- 同一个动作的不同变体用参数区分：`motion.layer.blending_mode {{\"mode\":\"multiply\"}}`、`motion.edit.label {{\"label\":1}}`、`window.panel {{\"id\":\"info\",\"toggle\":true}}`。"
    );
    let _ = writeln!(
        s,
        "- 设置（首选项）各页用 `app.settings`，参数是页面 id：`{{\"page\":\"startup_repair\"}}`。"
    );
    let _ = writeln!(
        s,
        "- 新增领域命令（会改工程数据的）放在 `bae-motion`，用 `<对象>.<动作>`，例如 `mask.create`、`shape.add_group`、`path.set_vertices`。"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "## 图例");
    let _ = writeln!(s);
    let _ = writeln!(s, "| 状态 | 含义 |");
    let _ = writeln!(s, "|---|---|");
    let _ = writeln!(s, "| ✅ | 已实现，和 AE 一致 |");
    let _ = writeln!(s, "| ◐ | 已实现，有差异（见 `docs/ui/motion-parity.md`） |");
    let _ = writeln!(s, "| ☐ | 本轮计划实现；实现前菜单里置灰 |");
    let _ = writeln!(s, "| ⏸ | 以后再做；菜单里置灰 |");
    let _ = writeln!(
        s,
        "| Ⓐ | Adobe 的服务或产品集成（Bridge、Media Encoder、Dynamic Link……）；菜单里置灰 |"
    );
    let _ = writeln!(
        s,
        "| ⋯ | 运行时填充的列表（最近的文件、脚本、扩展），不是固定菜单项 |"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "| 负责 | 工作 | 分支 |");
    let _ = writeln!(s, "|---|---|---|");
    let _ = writeln!(
        s,
        "| A | 工具栏和合成查看器里的工具、蒙版和路径编辑、查看器 | `ui/motion-tools` |"
    );
    let _ = writeln!(
        s,
        "| B | 主菜单、AE 快捷键、菜单背后的简单命令、本目录 | `ui/ae-menus` |"
    );
    let _ = writeln!(
        s,
        "| C | 时间轴、图表编辑器、关键帧对话框、标记、表达式 | `ui/motion-timeline` |"
    );
    let _ = writeln!(s, "| D | 设置（首选项） | `ui/ae-settings` |");
    let _ = writeln!(
        s,
        "| E | 新建 / 编辑类对话框、面板、项目面板、导入 | `ui/motion-dialogs` |"
    );
    let _ = writeln!(s, "| F | 渲染器功能 | `render/masks` |");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "快捷键列：Windows 是 AE 26.0 的默认值（`docs/reference/ae/shortcuts.json`）；macOS 是 AE 26.2.1 的默认值（`docs/reference/ae/mac/shortcuts.json`）：\
         一般是 Ctrl→Cmd、Alt→Opt，例外按 Mac 文件（`catalog::MAC_KEYS`）。`Control` 是 macOS 的 Control 键（AE 文件里的 `macControl`）。`PadX` 是小键盘键。"
    );
    let _ = writeln!(s);

    // Counts.
    let fixed: Vec<_> = items
        .iter()
        .filter(|(_, it)| it.status != Status::Dynamic)
        .collect();
    let by_status: Vec<(&str, usize)> = [
        Status::Done,
        Status::Partial,
        Status::Todo,
        Status::Later,
        Status::Adobe,
    ]
    .into_iter()
    .map(|st| {
        (
            status_mark(st),
            fixed.iter().filter(|(_, it)| it.status == st).count(),
        )
    })
    .collect();
    let keys: Vec<&Key> = KEYS
        .iter()
        .filter(|k| k.context != "Menu" && k.flags & MAC_ONLY == 0)
        .collect();
    let commands: HashSet<&str> = keys.iter().map(|k| k.command).collect();
    let bound = keys
        .iter()
        .filter(|k| !k.keys.is_empty() && k.flags & NATIVE == 0)
        .count();
    let working = keys
        .iter()
        .filter(|k| !k.keys.is_empty() && k.flags & NATIVE == 0 && k.status.works())
        .count();
    let _ = writeln!(s, "## 统计");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "- 菜单项：{} 个固定项（含 macOS 应用菜单），另有效果菜单按效果注册表列出各分类的效果。{}",
        fixed.len(),
        by_status
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join("，")
    );
    let _ = writeln!(
        s,
        "- 快捷键命令：AE 默认快捷键文件 {} 条（{} 个上下文，{} 个不同的命令），其中 {} 条有默认快捷键；动作已实现的 {} 条。",
        keys.len(),
        keys.iter().map(|k| k.context).collect::<HashSet<_>>().len(),
        commands.len(),
        bound,
        working
    );
    let _ = writeln!(s);

    // Menus.
    let _ = writeln!(s, "## 主菜单");
    let _ = writeln!(s);
    let mut menu_invocations: HashSet<(String, String)> = HashSet::new();
    let tops: Vec<(&str, &[Node])> = MENUS
        .iter()
        .copied()
        .chain([("BAE", MAC_APP_MENU)])
        .collect();
    for (top, nodes) in tops {
        if top == "BAE" {
            let _ = writeln!(s, "### macOS 应用菜单（BAE）");
        } else {
            let _ = writeln!(s, "### {top}");
        }
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
        let mut path = vec![top];
        walk(nodes, &mut path, &mut |p, it| {
            let mut title = it.title.to_owned();
            if it.platform == Platform::Windows {
                title.push_str("（仅 Windows）");
            }
            let full = if p.len() > 1 {
                format!("{} ▸ {title}", p[1..].join(" ▸ "))
            } else {
                title
            };
            let ks = invocation_keys(it.action, it.params);
            menu_invocations.insert((it.action.to_owned(), params(it.params).to_string()));
            let ae = if it.ae == 0 {
                String::new()
            } else {
                it.ae.to_string()
            };
            let _ = writeln!(
                s,
                "| {} | {} | `{}` | {} | {} | {} | {} | {} |",
                full.replace('|', "\\|"),
                ae,
                it.action,
                code(it.params),
                shown_chords(&ks, false),
                shown_chords(&ks, true),
                owner_name(it.owner),
                status_mark(it.status)
            );
        });
        if nodes.iter().any(|n| matches!(n, Node::Effects)) {
            let _ = writeln!(
                s,
                "| 效果分类（{}） ▸ 各效果 | | `motion.effect.apply` | `{{\"match_name\":\"…\"}}` | | | B | ☐ |",
                EFFECT_CATEGORIES.join("、")
            );
        }
        let _ = writeln!(s);
    }

    // Shortcut-only commands.
    let _ = writeln!(s, "## 只有快捷键的命令");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "AE 快捷键文件里不在主菜单上的命令（菜单上的命令见上表的快捷键列）。作用范围是 BAE 的面板种类，空 = 任何面板。"
    );
    let _ = writeln!(s);
    let mut by_ctx: BTreeMap<&str, Vec<&Key>> = BTreeMap::new();
    for k in KEYS {
        if menu_invocations.contains(&(k.action.to_owned(), params(k.params).to_string())) {
            continue;
        }
        by_ctx.entry(k.context).or_default().push(k);
    }
    for (ctx, list) in by_ctx {
        let _ = writeln!(s, "### `{ctx}`");
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
        for k in list {
            let mut note = String::new();
            if k.flags & NATIVE != 0 {
                note.push_str("（文本框自带）");
            }
            if k.flags & BOUND_ELSEWHERE != 0 {
                note.push_str("（由负责的 agent 绑定）");
            }
            if k.flags & SHADOWED != 0 {
                note.push_str("（同一快捷键 AE 先给了别的命令）");
            }
            if k.flags & MAC_ONLY != 0 {
                note.push_str("（只在 macOS 26.2 的快捷键文件里）");
            }
            let _ = writeln!(
                s,
                "| `{}`{} | `{}` | {} | {} | {} | {} | {} | {} |",
                k.command,
                note,
                k.action,
                code(k.params),
                shown_chords(&[k], false),
                shown_chords(&[k], true),
                k.scope().unwrap_or(""),
                owner_name(k.owner),
                status_mark(k.status)
            );
        }
        let _ = writeln!(s);
    }
    s
}

/// `docs/ui/ae-commands.md` is generated from the catalog
/// (`BAE_UPDATE_DOCS=1` rewrites it).
#[test]
fn catalog_doc_is_up_to_date() {
    let doc = render_doc();
    if std::env::var_os("BAE_UPDATE_DOCS").is_some() {
        std::fs::write(DOC_PATH, &doc).unwrap();
        return;
    }
    let current = std::fs::read_to_string(DOC_PATH).unwrap_or_default();
    assert!(
        current == doc,
        "docs/ui/ae-commands.md is stale: run BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog"
    );
}
