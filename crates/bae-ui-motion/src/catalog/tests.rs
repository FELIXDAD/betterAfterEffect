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
    // The same chords; the catalog may put the one AE's menu shows first.
    let sorted = |keys: &str| {
        let mut v: Vec<&str> = keys.split('|').collect();
        v.sort_unstable();
        v.join("|")
    };
    for (a, o) in ae.iter().zip(&ours) {
        assert_eq!((&a.0, &a.1, sorted(&a.2)), (&o.0, &o.1, sorted(&o.2)));
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
    // In the keymap: the command runs and BAE can bind one of its chords.
    let active = |mac: bool| {
        KEYS.iter()
            .filter(|k| {
                k.status.works()
                    && k.flags & (NATIVE | BOUND_ELSEWHERE | SHADOWED) == 0
                    && in_own_panel(k)
                    && k.chords(mac)
                        .iter()
                        .any(|c| bae_ui::Chord::parse(c).is_ok())
            })
            .count()
    };
    let _ = writeln!(
        s,
        "- 快捷键命令：AE 默认快捷键文件 {} 条（{} 个上下文，{} 个不同的命令），其中 {} 条有默认快捷键；动作已实现的 {} 条。",
        keys.len(),
        keys.iter().map(|k| k.context).collect::<HashSet<_>>().len(),
        commands.len(),
        bound,
        working
    );
    let _ = writeln!(
        s,
        "- 已生效（AE 键位表里绑定了 AE 的默认快捷键，按下就运行）：Windows {} 条，macOS {} 条。其余的命令由负责的 agent 实现后自动生效：启动时注册了处理器的 AE 命令会绑定上 AE 的默认快捷键，不用改目录（`actions::late_bindings`）。",
        active(false),
        active(true)
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
    // Windows checkouts may convert the doc to CRLF.
    let current = std::fs::read_to_string(DOC_PATH)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        current == doc,
        "docs/ui/ae-commands.md is stale: run BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog"
    );
}

const MAC_MENUS_JSON: &str = include_str!("../../../../docs/reference/ae/mac/menus.json");

/// Drop separators that no longer separate anything: leading, trailing and
/// doubled ones within each level (lines are `Top > … > Label`).
fn fold_separators(lines: Vec<String>) -> Vec<String> {
    let parent = |l: &str| {
        l.rsplit_once(" > ")
            .map(|(p, _)| p.to_owned())
            .unwrap_or_default()
    };
    let is_sep = |l: &str| l.ends_with(" > ---");
    // Last kept entry of each level: separator or not.
    let mut last: BTreeMap<String, bool> = BTreeMap::new();
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        let p = parent(&l);
        if is_sep(&l) {
            match last.get(&p) {
                Some(false) => {}
                _ => continue,
            }
        }
        last.insert(p, is_sep(&l));
        out.push(l);
    }
    // Trailing separators of each level.
    let mut keep = vec![true; out.len()];
    for i in 0..out.len() {
        if !is_sep(&out[i]) {
            continue;
        }
        let p = parent(&out[i]);
        let next_own = out[i + 1..]
            .iter()
            .find(|l| parent(l) == p || !l.starts_with(&format!("{p} > ")));
        if next_own.is_none_or(|l| parent(l) != p) {
            keep[i] = false;
        }
    }
    out.into_iter()
        .zip(keep)
        .filter_map(|(l, k)| k.then_some(l))
        .collect()
}

/// The catalog's menu bar as macOS shows it, in English: the application menu
/// first, no Windows-only entries, sorted runs in English order, no run-time
/// list placeholders.
fn catalog_mac_lines() -> Vec<String> {
    fn walk(prefix: &str, nodes: &[Node], out: &mut Vec<String>) {
        let mut level: Vec<String> = Vec::new();
        let mut run: Option<usize> = None;
        let sort_run = |level: &mut Vec<String>, run: &mut Option<usize>| {
            if let Some(start) = run.take() {
                level[start..].sort();
            }
        };
        for n in nodes {
            match n {
                Node::Item(it) if it.platform == Platform::All && it.status != Status::Dynamic => {
                    if it.sorted {
                        run.get_or_insert(level.len());
                    } else {
                        sort_run(&mut level, &mut run);
                    }
                    level.push(format!("{prefix} > {}", mac_title(it.title)));
                }
                Node::Separator => {
                    sort_run(&mut level, &mut run);
                    level.push(format!("{prefix} > ---"));
                }
                Node::Submenu(sub) if sub.platform == Platform::All => {
                    sort_run(&mut level, &mut run);
                    level.push(format!("{prefix} > {} ▸", sub.title));
                    walk(
                        &format!("{prefix} > {}", sub.title),
                        sub.children,
                        &mut level,
                    );
                }
                Node::Effects => {
                    sort_run(&mut level, &mut run);
                    for c in EFFECT_CATEGORIES {
                        level.push(format!("{prefix} > {c} ▸"));
                    }
                }
                _ => {}
            }
        }
        sort_run(&mut level, &mut run);
        out.extend(level);
    }
    let mut out = Vec::new();
    for (top, nodes) in [("BAE", MAC_APP_MENU)]
        .into_iter()
        .chain(MENUS.iter().copied())
    {
        out.push(format!("{top} ▸"));
        walk(top, nodes, &mut out);
    }
    fold_separators(out)
}

/// Submenus whose entries AE fills at run time (recent files, installed
/// scripts and extensions, effects, Services): only the submenu is compared.
const MAC_RUNTIME_SUBMENUS: &[&str] = &[
    "BAE > Services",
    "File > Open Recent",
    "File > Import Recent Footage",
    "File > Scripts > Recent Script Files",
    "Animation > Recent Animation Presets",
    "Window > Extensions",
    "Edit > History",
    "Edit > Team Project",
    "Layer > Layer Styles",
    "Animation > Animate Text > Variable Font Axes",
];

/// Entries of AE 26.2.1 on macOS that are not menu commands of AE 26.0
/// (`docs/reference/ae/mac/menus.md` 5.3 and 5.5): AppKit's injected items,
/// AE 26.2's Quick Apply, Team Project and account items.
const MAC_NOT_IN_CATALOG: &[&str] = &[
    "Edit > AutoFill",
    "Edit > Start Dictation…",
    "View > Enter Full Screen",
    "Edit > Quick Apply...",
    "File > New > New Team Project...",
    "File > Open Team Project...",
    "Help > Adobe Crash Reporter...",
    "Help > Manage My Account...",
    "Help > Sign Out...",
    "Help > Updates...",
];

/// AE's macOS menu bar (`docs/reference/ae/mac/menus.json`, AE 26.2.1, English)
/// as `Top > … > Label` lines, with "After Effects" read as "BAE" and the
/// run-time entries left out.
fn ae_mac_lines() -> Vec<String> {
    fn label(v: &Value) -> String {
        v["label"]
            .as_str()
            .unwrap_or_default()
            .replace("After Effects", "BAE")
            .replace("Settings…", "Settings")
    }
    fn walk(prefix: &str, items: &[Value], out: &mut Vec<String>) {
        for v in items {
            let line = format!("{prefix} > {}", label(v));
            if MAC_NOT_IN_CATALOG.iter().any(|x| line.starts_with(x)) {
                continue;
            }
            match v["type"].as_str() {
                Some("separator") => out.push(format!("{prefix} > ---")),
                Some("submenu") => {
                    let path = format!("{prefix} > {}", label(v));
                    out.push(format!("{path} ▸"));
                    let runtime = MAC_RUNTIME_SUBMENUS.contains(&path.as_str())
                        || EFFECT_CATEGORIES
                            .iter()
                            .any(|c| path == format!("Effect > {c}"));
                    if !runtime {
                        walk(
                            &path,
                            v["items"].as_array().map(Vec::as_slice).unwrap_or(&[]),
                            out,
                        );
                    }
                }
                _ => out.push(line),
            }
        }
    }
    let json: Value = serde_json::from_str(MAC_MENUS_JSON).unwrap();
    let mut out = Vec::new();
    for top in json["menus"].as_array().unwrap() {
        let title = label(top);
        out.push(format!("{title} ▸"));
        walk(&title, top["items"].as_array().unwrap(), &mut out);
    }
    fold_separators(out.into_iter().filter_map(|l| mac_normalize(&l)).collect())
}

/// What differs between AE's live macOS menu and its menu commands, read the
/// way the catalog lists them (`None` = not a fixed menu command):
/// - installed scripts (`File ▸ Scripts ▸ *.jsx`);
/// - titles AE fills in when the menu opens: "Can't Undo" / "Can't Redo" (BAE's
///   shell does the same), the 16 label colours ("Unused" until opened),
///   `^1...` (Layer ▸ Transform ▸ Anchor Point...), and the active workspace's
///   name in Window ▸ Workspace ▸ Reset … and Assign Shortcut to … Workspace;
/// - viewer items name what the viewer shows ("Composition: (none)");
/// - Services is the system's submenu (BAE uses the native one).
fn mac_normalize(line: &str) -> Option<String> {
    if line.ends_with(".jsx") {
        return None;
    }
    let line = line
        .replace("Can’t Undo", "Undo")
        .replace("Can’t Redo", "Redo")
        .replace("Transform > ^1...", "Transform > Anchor Point...")
        .replace("BAE > Services ▸", "BAE > Services");
    // “Name” inserted by AE: drop the quoted parts.
    let mut line = line;
    while let (Some(a), Some(b)) = (line.find('“'), line.find('”')) {
        if b < a {
            break;
        }
        line = format!("{}{}", &line[..a], &line[b + '”'.len_utf8()..]);
    }
    let line = line
        .replace("Reset  to Saved Layout", "Reset to Saved Layout")
        .replace("Assign Shortcut to  Workspace", "Assign Shortcut to Workspace")
        .replace(" (Replace )", "")
        // Viewer items name the viewer's item: "Composition: Comp 1".
        .replace(": (none)", "");
    Some(line)
}

/// Lines both sides list differently by design: the label colours (AE fills
/// them when the menu opens) and Track in Boris FX Mocha (AE 26.2 moved it next
/// to the other tracking commands; the catalog follows AE 26.0, `menus.json`).
fn mac_compare_skip(line: &str) -> bool {
    const COLORS: &[&str] = &[
        "Unused",
        "Red",
        "Yellow",
        "Aqua",
        "Pink",
        "Lavender",
        "Peach",
        "Sea Foam",
        "Blue",
        "Green",
        "Purple",
        "Orange",
        "Brown",
        "Fuchsia",
        "Cyan",
        "Sandstone",
        "Dark Green",
    ];
    line == "Animation > Track in Boris FX Mocha"
        || line
            .strip_prefix("Edit > Label > ")
            .is_some_and(|c| COLORS.contains(&c))
}

/// The catalog matches AE's macOS menu bar too: the application menu (About,
/// Settings ▸ the 16 pages, Services, Hide, Quit), no Exit / Preferences /
/// About in File, Edit and Help, Finder instead of Explorer.
#[test]
fn mac_menu_bar_matches_ae_mac() {
    let ours: Vec<String> = catalog_mac_lines()
        .into_iter()
        .filter(|l| !mac_compare_skip(l))
        .collect();
    let ae: Vec<String> = ae_mac_lines()
        .into_iter()
        .filter(|l| !mac_compare_skip(l))
        .collect();
    if let Some(dir) = std::env::var_os("BAE_DUMP_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::write(dir.join("ours.txt"), ours.join("\n")).unwrap();
        std::fs::write(dir.join("ae.txt"), ae.join("\n")).unwrap();
    }
    let first = ours.iter().zip(&ae).position(|(a, b)| a != b);
    assert!(
        ours == ae,
        "first difference at line {first:?}: ours {:?} vs AE {:?}",
        first.and_then(|i| ours.get(i)),
        first.and_then(|i| ae.get(i))
    );
}

/// AE sorts some entries by their localized name (Export, Keyframe Assistant,
/// workspaces, the Window panel list). The catalog lists them in the order of
/// AE's Chinese menu (`menus.json`), so the Chinese titles must sort the same
/// way by code point: a wrong translation of one of them shows up here.
#[test]
fn sorted_entries_follow_ae_chinese_order() {
    let zh: BTreeMap<&str, &str> = MENU_ZH
        .iter()
        .copied()
        .chain(crate::i18n::ZH.iter().copied())
        .collect();
    let zh_of = |t: &str| {
        zh.get(t)
            .copied()
            .or_else(|| bae_ui::i18n::translation(t))
            .unwrap_or_else(|| panic!("no Chinese for {t}"))
    };
    fn runs(nodes: &'static [Node], out: &mut Vec<Vec<&'static str>>) {
        let mut run: Vec<&'static str> = Vec::new();
        for n in nodes {
            match n {
                Node::Item(it) if it.sorted => run.push(it.title),
                Node::Submenu(s) => {
                    out.push(std::mem::take(&mut run));
                    runs(s.children, out);
                }
                _ => out.push(std::mem::take(&mut run)),
            }
        }
        out.push(run);
    }
    let mut all = Vec::new();
    for (_, nodes) in MENUS {
        runs(nodes, &mut all);
    }
    let mut errors = Vec::new();
    for run in all.iter().filter(|r| r.len() > 1) {
        for pair in run.windows(2) {
            let (a, b) = (zh_of(pair[0]), zh_of(pair[1]));
            if a >= b {
                errors.push(format!("{} ({a}) before {} ({b})", pair[0], pair[1]));
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

/// The menu shows the chord AE's menu shows: of the AE chords of a menu item's
/// command, the first one BAE can bind is that one (Windows `menus.json`).
#[test]
fn menu_items_show_the_chord_ae_shows() {
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
        let first = KEYS
            .iter()
            .filter(|k| k.action == it.action && params(k.params) == params(it.params))
            .flat_map(|k| k.keys.split('|'))
            .find(|c| {
                let bae = chords(c, false);
                bae.first().is_some_and(|b| bae_ui::Chord::parse(b).is_ok())
            });
        if let Some(first) = first
            && normalize(first) != want
            && chords(sc, false)
                .iter()
                .all(|b| bae_ui::Chord::parse(b).is_ok())
        {
            errors.push(format!(
                "{} ▸ {}: AE shows {sc}, BAE {first}",
                path.join(" ▸ "),
                it.title
            ));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
