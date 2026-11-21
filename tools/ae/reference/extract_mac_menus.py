#!/usr/bin/env python3
"""Extract macOS After Effects menu bar structure, ExtendScript internal
command IDs, and zh_CN cross-reference labels, then diff against Windows
docs/reference/ae/menus.json.

Produces:
  - docs/reference/ae/mac/menus.json
  - docs/reference/ae/mac/menus.md
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import pathlib
import platform
import plistlib
import re
import subprocess
import tempfile
from typing import Any

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[2]

DEFAULT_AE_APP = pathlib.Path(
    "/Applications/Adobe After Effects 2026/Adobe After Effects 2026.app"
)
DEFAULT_WIN_MENUS = REPO_ROOT / "docs" / "reference" / "ae" / "menus.json"
DEFAULT_OUT_DIR = REPO_ROOT / "docs" / "reference" / "ae" / "mac"
AECTL_PATH = REPO_ROOT / "tools" / "ae" / "aectl"
SWIFT_DUMPER = SCRIPT_DIR / "dump_mac_menus.swift"

MRU_SUBMENU_TITLES = {
    "Open Recent",
    "Import Recent Footage",
    "Recent Script Files",
    "Recent Animation Presets",
    "打开最近的文件",
    "导入最近的素材",
    "最近的脚本文件",
    "最近的动画预设",
}

GLYPH_MAP = {
    2: "Tab",
    4: "Numpad Enter",
    10: "FwdDel",
    11: "Return",
    27: "Esc",
    102: "Home",
    104: "PageUp",
    105: "End",
    107: "PageDown",
    111: "F1",
    112: "F2",
    113: "F3",
    114: "F4",
    115: "F5",
    116: "F6",
    117: "F7",
    118: "F8",
    119: "F9",
    120: "F10",
    121: "F11",
    122: "F12",
}


def detect_macos_platform() -> str:
    try:
        prod = subprocess.check_output(["sw_vers", "-productName"], text=True).strip()
        ver = subprocess.check_output(["sw_vers", "-productVersion"], text=True).strip()
        build = subprocess.check_output(["sw_vers", "-buildVersion"], text=True).strip()
        return f"{prod} {ver} ({build}, {platform.machine()})"
    except Exception:
        return f"macOS ({platform.machine()})"


def detect_ae_version(app_path: pathlib.Path) -> dict[str, Any]:
    plist_path = app_path / "Contents" / "Info.plist"
    if plist_path.exists():
        with plist_path.open("rb") as fh:
            info = plistlib.load(fh)
        short_ver = str(info.get("CFBundleShortVersionString", "unknown"))
        bundle_ver = str(info.get("CFBundleVersion", short_ver))
        build_num = int(bundle_ver.split(".")[-1]) if bundle_ver.split(".")[-1].isdigit() else None
        return {
            "app": "Adobe After Effects",
            "version": short_ver,
            "bundleVersion": bundle_ver,
            "buildNumber": build_num,
        }
    return {
        "app": "Adobe After Effects",
        "version": "unknown",
        "bundleVersion": "unknown",
        "buildNumber": None,
    }


def decode_ax_shortcut(item: dict[str, Any]) -> str | None:
    ch = item.get("cmdChar")
    gl = item.get("cmdGlyph")
    mods = item.get("cmdModifiers")
    if not ch and not gl:
        return None
    parts: list[str] = []
    if mods is not None:
        if mods & 16:
            parts.append("Fn")
        if mods & 4:
            parts.append("Ctrl")
        if mods & 2:
            parts.append("Option")
        if mods & 1:
            parts.append("Shift")
        if not (mods & 8):
            parts.append("Cmd")
    key = GLYPH_MAP.get(gl, ch) if gl else ch
    if not key:
        return None
    parts.append(str(key))
    return "+".join(parts)


def dump_ax_menubar() -> list[dict[str, Any]]:
    out = subprocess.check_output(["swift", str(SWIFT_DUMPER)], text=True)
    return json.loads(out)


def probe_ae_command_ids(raw_menus: list[dict[str, Any]]) -> tuple[str, dict[str, int]]:
    """Use tools/ae/aectl to query app.isoLanguage and app.findMenuCommandId()."""
    lang_out = subprocess.check_output(
        [str(AECTL_PATH), "eval", "app.isoLanguage"], text=True
    )
    lang_json = json.loads(lang_out)
    iso_lang = lang_json.get("value") or "unknown"

    titles: list[str] = []
    seen: set[str] = set()

    def collect(items: list[dict[str, Any]]) -> None:
        for it in items:
            t = it.get("title", "")
            if it["type"] == "submenu":
                if t in MRU_SUBMENU_TITLES or t in ("Services", "服务"):
                    continue
                collect(it.get("items", []))
            elif it["type"] == "item":
                if t and t not in seen and not t.startswith("<redacted:"):
                    seen.add(t)
                    titles.append(t)

    collect(raw_menus)

    pid = os.getpid()
    jsx_path = pathlib.Path(f"/private/tmp/bae_probe_menus_{pid}.jsx")
    tsv_path = pathlib.Path(f"/private/tmp/bae_probe_menus_{pid}.tsv")
    try:
        jsx_lines = [
            "var titles = " + json.dumps(titles, ensure_ascii=False) + ";",
            f"var out = new File({json.dumps(str(tsv_path))});",
            "out.encoding = 'UTF-8';",
            "out.lineFeed = 'Unix';",
            "out.open('w');",
            "for (var i = 0; i < titles.length; i++) {",
            "    var t = titles[i];",
            "    var id = 0;",
            "    try { id = app.findMenuCommandId(t); } catch (e) { id = -1; }",
            "    out.writeln(t.replace(/[\\t\\r\\n]/g, ' ') + '\\t' + id);",
            "}",
            "out.close();",
            "'ok:' + titles.length;",
        ]
        jsx_path.write_text("\n".join(jsx_lines), encoding="utf-8")
        subprocess.check_output([str(AECTL_PATH), "run", str(jsx_path)], text=True)

        ids: dict[str, int] = {}
        for line in tsv_path.read_text(encoding="utf-8").splitlines():
            if "\t" in line:
                k, v = line.split("\t", 1)
                try:
                    ids[k] = int(v)
                except ValueError:
                    pass
        return iso_lang, ids
    finally:
        for p in (jsx_path, tsv_path):
            if p.exists():
                p.unlink()


def load_zh_dict_highlights(app_path: pathlib.Path) -> dict[str, str]:
    """Read official zh_CN labels for Mac-specific menu strings from AE bundle."""
    dat_path = (
        app_path / "Contents" / "Dictionaries" / "zh_CN" / "after_effects_zh_CN.dat"
    )
    want_keys = {
        "$$$/AE/MenuID/0001/AboutAfterEffects_256",
        "$$$/AE/MenuID/MacAppMenu_Services",
        "$$$/AE/MenuID/MacAppMenu_Hide_AE",
        "$$$/AE/MenuID/MacAppMenu_Hide_Others",
        "$$$/AE/MenuID/MacAppMenu_Show_All",
        "$$$/AE/MenuID/MacAppMenu_Quit",
        "$$$/AE/MenuID/Various/RevealInFileSystem",
        "$$$/AE/MenuID/0003/QuickApply_10809",
        "$$$/AE/Dialog/PrefsDialog",
    }
    found: dict[str, str] = {}
    if not dat_path.exists():
        return found
    for line in dat_path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r'^\s*"(\$\$\$/[^=]+)=(.*)"\s*$', line)
        if not m:
            continue
        k, v = m.group(1), m.group(2)
        if k in want_keys:
            # Take macOS branch before {{*MSWindows*}} if forked
            mac_val = v.split("{{*MSWindows*}}")[0]
            # Strip Windows-style (&X) mnemonic if present
            mac_val = re.sub(r"\(&[A-Za-z0-9]\)", "", mac_val)
            found[k] = mac_val
    return found


def sanitize_menu_tree(
    raw_items: list[dict[str, Any]], cmd_ids: dict[str, int]
) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for it in raw_items:
        itype = it["type"]
        if itype == "separator":
            out.append({"type": "separator"})
            continue

        title = it.get("title", "")
        if itype == "submenu":
            if title in MRU_SUBMENU_TITLES:
                out.append(
                    {
                        "type": "submenu",
                        "label": title,
                        "items": [
                            {
                                "type": "item",
                                "label": "<redacted:MRU list>",
                                "aeCommandId": None,
                            }
                        ],
                    }
                )
            elif title in ("Services", "服务"):
                out.append(
                    {
                        "type": "submenu",
                        "label": title,
                        "items": [
                            {
                                "type": "item",
                                "label": "<redacted:macOS Services>",
                                "aeCommandId": None,
                            }
                        ],
                    }
                )
            else:
                out.append(
                    {
                        "type": "submenu",
                        "label": title,
                        "items": sanitize_menu_tree(it.get("items", []), cmd_ids),
                    }
                )
        else:
            clean_label = title
            if clean_label.startswith("Sign Out"):
                clean_label = "Sign Out... (<redacted:account>)"
            elif clean_label.startswith("注销"):
                clean_label = "注销... (<redacted:account>)"
            elif clean_label.startswith("/Users/") or clean_label.startswith("/private/tmp/"):
                clean_label = "<redacted:path>"

            entry: dict[str, Any] = {
                "type": "item",
                "label": clean_label,
            }
            sc = decode_ax_shortcut(it)
            if sc:
                entry["shortcut"] = sc
            cid = cmd_ids.get(title)
            entry["aeCommandId"] = cid if cid is not None else None
            out.append(entry)
    return out


def count_tree(menus: list[dict[str, Any]]) -> tuple[int, int, int]:
    top_count = len(menus)
    sub_count = 0
    item_count = 0

    def walk(items: list[dict[str, Any]], depth: int) -> None:
        nonlocal sub_count, item_count
        for it in items:
            if it["type"] == "submenu":
                if depth > 0:
                    sub_count += 1
                walk(it.get("items", []), depth + 1)
            elif it["type"] == "item":
                item_count += 1

    walk(menus, 0)
    return top_count, sub_count, item_count


def extract_settings_pages(menus: list[dict[str, Any]]) -> list[dict[str, Any]]:
    pages: list[dict[str, Any]] = []
    group_idx = 1
    for top in menus:
        if top.get("label") == "After Effects":
            for it in top.get("items", []):
                if it["type"] == "submenu" and (
                    it["label"].startswith("Settings") or it["label"].startswith("Preferences")
                ):
                    for sub in it.get("items", []):
                        if sub["type"] == "separator":
                            group_idx += 1
                        elif sub["type"] == "item":
                            pages.append(
                                {
                                    "group": group_idx,
                                    "labelEn": sub["label"],
                                    "shortcut": sub.get("shortcut"),
                                    "aeCommandId": sub.get("aeCommandId"),
                                }
                            )
    return pages


def build_windows_diff(
    menus: list[dict[str, Any]],
    win_data: dict[str, Any],
    zh_dict: dict[str, str],
) -> dict[str, Any]:
    # Extract Windows Preferences zh_CN page labels for 1-to-1 pairing
    win_pref_labels: list[str] = []
    for top in win_data.get("menus", []):
        if "编辑" in top.get("label", ""):
            for it in top.get("items", []):
                if it.get("type") == "submenu" and "首选项" in it.get("label", ""):
                    for p in it.get("items", []):
                        if p.get("type") == "item":
                            lbl = re.sub(r"\([A-Za-z]\)", "", p["label"])
                            win_pref_labels.append(lbl)

    settings_pages = extract_settings_pages(menus)
    for idx, page in enumerate(settings_pages):
        if idx < len(win_pref_labels):
            page["labelZh"] = win_pref_labels[idx]

    return {
        "comparedAgainst": win_data.get("source", {}),
        "topLevelMenus": {
            "mac": [m["label"] for m in menus],
            "win": [m["label"] for m in win_data.get("menus", [])],
            "notes": [
                "macOS adds the After Effects application menu as the first app menu (10 app menus vs 9 on Windows; excluding the system Apple menu).",
                "macOS menu titles omit Windows Alt+letter mnemonic suffixes such as (F), (E), (C), (L), (T), (A), (V), (H).",
            ],
        },
        "appMenuItems": [
            {
                "labelEn": "About After Effects...",
                "labelZh": zh_dict.get("$$$/AE/MenuID/0001/AboutAfterEffects_256", "关于 After Effects..."),
                "shortcut": None,
                "aeCommandId": 0,
                "windowsLocation": "帮助(H) > 关于 After Effects...",
            },
            {
                "labelEn": "Settings…",
                "labelZh": "设置…（NIB 原文为 Preferences，macOS 13+ 系统自动改名为 Settings…/设置…；对话框窗口标题仍为“首选项”）",
                "shortcut": None,
                "aeCommandId": None,
                "windowsLocation": "编辑(E) > 首选项(F)",
            },
            {
                "labelEn": "Services",
                "labelZh": zh_dict.get("$$$/AE/MenuID/MacAppMenu_Services", "服务"),
                "shortcut": None,
                "aeCommandId": None,
                "windowsLocation": None,
            },
            {
                "labelEn": "Hide After Effects",
                "labelZh": zh_dict.get("$$$/AE/MenuID/MacAppMenu_Hide_AE", "隐藏 After Effects"),
                "shortcut": "Cmd+H",
                "altShortcutWhenSysPrefOff": "Ctrl+Cmd+H",
                "aeCommandId": 0,
                "windowsLocation": None,
            },
            {
                "labelEn": "Hide Others",
                "labelZh": zh_dict.get("$$$/AE/MenuID/MacAppMenu_Hide_Others", "隐藏其他"),
                "shortcut": "Option+Cmd+H",
                "altShortcutWhenSysPrefOff": "Ctrl+Option+Cmd+H",
                "aeCommandId": 0,
                "windowsLocation": None,
            },
            {
                "labelEn": "Show All",
                "labelZh": zh_dict.get("$$$/AE/MenuID/MacAppMenu_Show_All", "全部显示"),
                "shortcut": None,
                "aeCommandId": 0,
                "windowsLocation": None,
            },
            {
                "labelEn": "Quit After Effects",
                "labelZh": zh_dict.get("$$$/AE/MenuID/MacAppMenu_Quit", "退出 After Effects"),
                "shortcut": "Cmd+Q",
                "aeCommandId": 0,
                "windowsLocation": "文件(F) > 退出(X)",
            },
        ],
        "settingsPages": settings_pages,
        "movedOrRemovedFromStandardMenus": [
            {
                "windowsMenu": "文件(F) > 退出(X)",
                "macStatus": "moved_to_app_menu",
                "macLocation": "After Effects > Quit After Effects",
            },
            {
                "windowsMenu": "编辑(E) > 首选项(F)",
                "macStatus": "moved_and_renamed",
                "macLocation": "After Effects > Settings…",
            },
            {
                "windowsMenu": "帮助(H) > 关于 After Effects...",
                "macStatus": "moved_to_app_menu",
                "macLocation": "After Effects > About After Effects...",
            },
        ],
        "platformRenamedItems": [
            {
                "macPath": "File > Reveal in Finder",
                "macLabelZh": zh_dict.get("$$$/AE/MenuID/Various/RevealInFileSystem", "在 Finder 中显示"),
                "winPath": "文件(F) > 在资源管理器中显示",
                "aeCommandId": 2562,
            },
            {
                "macPath": "Layer > Reveal in Finder",
                "macLabelZh": zh_dict.get("$$$/AE/MenuID/Various/RevealInFileSystem", "在 Finder 中显示"),
                "winPath": "图层(L) > 在资源管理器中显示",
                "aeCommandId": 2562,
            },
            {
                "macPath": "Layer > Reveal > Reveal in Finder",
                "macLabelZh": zh_dict.get("$$$/AE/MenuID/Various/RevealInFileSystem", "在 Finder 中显示"),
                "winPath": "图层(L) > 显示 > 在资源管理器中显示",
                "aeCommandId": 2562,
            },
        ],
        "macSystemInjectedItems": [
            {
                "macPath": "Edit > AutoFill > (Contact… / Passwords… / Credit Card…)",
                "shortcut": None,
                "aeCommandId": 0,
                "origin": "macOS AppKit automatic Edit menu injection",
            },
            {
                "macPath": "Edit > Start Dictation…",
                "shortcut": "Fn+D",
                "aeCommandId": -196648,
                "origin": "macOS AppKit automatic Edit menu injection",
            },
            {
                "macPath": "View > Enter Full Screen",
                "shortcut": "Fn+F",
                "aeCommandId": -589862,
                "origin": "macOS AppKit automatic View menu full-screen toggle",
            },
        ],
        "macMenuShortcutDisplayDifferences": [
            {
                "macPath": "File > Replace Footage > File...",
                "macDisplayedShortcut": "Ctrl+Cmd+H",
                "winDisplayedShortcut": "Ctrl+H",
                "reason": "Dynamically remapped to Ctrl+Cmd+H when Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true because Cmd+H is assigned to Hide After Effects.",
            },
            {
                "macPath": "Composition > Add to Render Queue",
                "macDisplayedShortcut": "Ctrl+Cmd+M",
                "winDisplayedShortcut": "Ctrl+M",
                "reason": "Dynamically remapped to Ctrl+Cmd+M when Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true because Cmd+M is assigned to macOS Minimize.",
            },
            {
                "macPath": "Animation > Toggle Hold Keyframe",
                "macDisplayedShortcut": "Ctrl+Option+Cmd+H",
                "winDisplayedShortcut": "Ctrl+Alt+H",
                "reason": "Dynamically remapped to Ctrl+Option+Cmd+H when Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true because Option+Cmd+H is assigned to Hide Others.",
            },
            {
                "macPath": "Edit > Purge > All Memory",
                "macDisplayedShortcut": "Ctrl+FwdDel",
                "winDisplayedShortcut": "Ctrl+Alt+Numpad /",
                "reason": "Displays macOS Control+Clear/FwdDel (macControl+PadClear) primary hint instead of Numpad /.",
            },
            {
                "macPath": "Composition > Save Current Preview...",
                "macDisplayedShortcut": "Ctrl+Shift+C",
                "winDisplayedShortcut": "Ctrl+数字小键盘 0",
                "reason": "Displays MacBook non-numpad binding (macControl+Shift+C) instead of Numpad 0.",
            },
            {
                "macPath": "Layer > Markers > Add Marker",
                "macDisplayedShortcut": "Ctrl+8",
                "winDisplayedShortcut": "Numpad *",
                "reason": "Displays MacBook non-numpad binding (macControl+8) instead of Numpad *.",
            },
            {
                "macPath": "View > Use Display Color Management",
                "macDisplayedShortcut": "Ctrl+Option+/",
                "winDisplayedShortcut": "Shift+Numpad /",
                "reason": "Displays MacBook non-numpad binding (macControl+Option+/) instead of Shift+Numpad /.",
            },
            {
                "macPath": "After Effects > Settings… > Startup & Repair...",
                "macDisplayedShortcut": "Option+Cmd+;",
                "winDisplayedShortcut": "Ctrl+Alt+;",
                "reason": "Note: AE keeps Option+Cmd+; rather than standard macOS Cmd+, for Settings.",
            },
        ],
        "versionOrInstallDeltas26_2_vs_26_0": [
            {
                "macPath": "Edit > Quick Apply...",
                "labelZh": zh_dict.get("$$$/AE/MenuID/0003/QuickApply_10809", "快速应用..."),
                "shortcut": "Cmd+Return",
                "aeCommandId": 10809,
                "note": "Added in AE 26.2 (absent in Windows 26.0x67 snapshot).",
            },
            {
                "macPath": "Edit > Paste Reversed Keyframes",
                "shortcut": "Shift+Cmd+V",
                "aeCommandId": 54,
                "note": "Menu item existed in 26.0 without shortcut; default Shift+Cmd+V added in 26.2.",
            },
            {
                "macPath": "File > New > New Team Project... / File > Open Team Project... / Edit > Team Project (6 items)",
                "aeCommandId": 10428,
                "note": "Populated when Creative Cloud Team Projects is active on the machine.",
            },
            {
                "macPath": "Help > Adobe Crash Reporter... / Manage My Account... / Sign Out... / Updates...",
                "aeCommandId": 4009,
                "note": "Present in live Mac 26.2.1 Help menu; omitted/absent in Windows 26.0x67 snapshot.",
            },
        ],
        "windowMenuNote": "AE's Window menu on macOS is dedicated to Workspaces, Extensions, Panels, Viewers, and ScriptUI panels and does NOT contain standard Cocoa Minimize / Zoom / Bring All to Front items, even though MacSysMenuMinimize (Cmd+M / Ctrl+Cmd+M) is still intercepted as a system shortcut.",
    }


def render_markdown(payload: dict[str, Any]) -> str:
    src = payload["source"]
    diff = payload["windowsDiff"]
    win_src = diff["comparedAgainst"]

    lines: list[str] = [
        "# After Effects macOS 菜单栏结构与 Windows 差异对照",
        "",
        f"> 来源：{src['app']} {src['version']}（build `{src['bundleVersion']}`，实机运行语言 `{src['language']}`）| 平台：{src['platform']} | 提取日期：{src['extractedAt']}",
        f"> 对照基准：{win_src.get('app', 'Adobe After Effects')} {win_src.get('version', '26.0x67')} (`{win_src.get('language', 'zh_CN')}`) | 平台：{win_src.get('platform', 'Windows')}",
        "",
        "完整脱敏后的 macOS 菜单树见 [`menus.json`](menus.json)（共 **{top}** 个顶层应用菜单、**{sub}** 个子菜单、**{items}** 个叶子菜单项）。".format(
            top=payload["topLevelCount"],
            sub=payload["submenuCount"],
            items=payload["itemCount"],
        ),
        "",
        "## 1. 提取与验证方法",
        "",
        "1. **实机 `AXMenuBar` 全量读取（已成功）**：通过 `tools/ae/reference/dump_mac_menus.swift` 调用 macOS `AXUIElement` 只读遍历运行中 AE 26.2.1 的主菜单栏（含所有子菜单、系统注入项及实时生效的快捷键修饰符与字形）。",
        "2. **ExtendScript `app.findMenuCommandId()` 逐项探测**：通过 `tools/ae/aectl` 对所有菜单项标题查询 AE 内部命令 ID（`aeCommandId`）。注意：Windows 版 `../menus.json` 的 `commandId` 来自 Win32 `GetMenuItemID`（`HMENU` 句柄编码，如 `131074`），而 macOS `menus.json` 的 `aeCommandId` 是 AE 跨平台内部命令 ID（如 `New Project = 2`、`New Composition... = 2000`、`Reveal in Finder = 2562`）；由 Cocoa 原生处理的菜单项（`About`、`Hide`、`Quit`、`AutoFill`）在 `findMenuCommandId()` 中返回 `0`。",
        "3. **应用包资源核对**：读取 `Contents/Resources/After Effects.nib` 与 `Contents/Dictionaries/zh_CN/after_effects_zh_CN.dat`，确认官方简体中文译名与 `{{*MSWindows*}}` 平台分叉文案。",
        "",
        "## 2. 顶层菜单栏对比",
        "",
        "- **macOS（10 个应用菜单，不含左上角苹果图标菜单）**："
        + " · ".join(f"`{m}`" for m in diff["topLevelMenus"]["mac"]),
        "- **Windows（9 个菜单）**："
        + " · ".join(f"`{m}`" for m in diff["topLevelMenus"]["win"]),
        "- **关键差异**：macOS 首位新增 `After Effects` 应用菜单，且所有菜单与子菜单标题均**不带** Windows 的 `Alt` 助记符后缀（如 `(F)`、`(E)`、`(C)`）。",
        "",
        "## 3. macOS 独占 `After Effects` 应用菜单",
        "",
        "| 英文标签 | 官方中文标签（对照） | 默认快捷键 | 关闭「使用系统快捷键」后 | `aeCommandId` | Windows 对应位置 |",
        "|---|---|---|---|---:|---|",
    ]

    for it in diff["appMenuItems"]:
        sc = f"`{it['shortcut']}`" if it.get("shortcut") else "*(无)*"
        alt_sc = (
            f"`{it['altShortcutWhenSysPrefOff']}`"
            if it.get("altShortcutWhenSysPrefOff")
            else "—"
        )
        cid = str(it["aeCommandId"]) if it.get("aeCommandId") is not None else "*(子菜单)*"
        wloc = f"`{it['windowsLocation']}`" if it.get("windowsLocation") else "*(macOS 独有)*"
        lines.append(
            f"| `{it['labelEn']}` | {it['labelZh']} | {sc} | {alt_sc} | {cid} | {wloc} |"
        )

    lines.extend(
        [
            "",
            "## 4. 首选项 / 设置（Settings…）在 Mac 上的位置、命名与 16 个页面",
            "",
            "- **菜单位置**：位于 **`After Effects ▸ Settings…`**（中文 macOS 13+ 下为 **`After Effects ▸ 设置…`**），而非 Windows 的 `编辑(E) ▸ 首选项(F)`。",
            "- **命名细节**：`After Effects.nib` 内部原始标题为 `Preferences`，在 macOS 13（Ventura）及更高版本（本机为 macOS 26.7.3）上系统 AppKit 自动将其显示为 **`Settings…` / `设置…`**；但打开后的对话框窗口标题在 AE 词典（`$$$/AE/Dialog/PrefsDialog`）中仍为「首选项」/ *Preferences*。",
            "- **快捷键**：仍沿用 AE 传统的 **`Option+Cmd+;`**（绑定在第一页 `Startup & Repair...` 上），**不是** macOS 常见应用的 `Cmd+,`。",
            "- **页面清单（共 16 页，按分隔线分 5 组，顺序与 Windows 完全一致）**：",
            "",
            "| 分组 | 英文页面名 | 中文页面名（对照） | 快捷键 | `aeCommandId` |",
            "|---:|---|---|---|---:|",
        ]
    )

    for p in diff["settingsPages"]:
        sc = f"`{p['shortcut']}`" if p.get("shortcut") else "*(无)*"
        zh = p.get("labelZh", "")
        cid = p.get("aeCommandId", "")
        lines.append(f"| {p['group']} | `{p['labelEn']}` | {zh} | {sc} | {cid} |")

    lines.extend(
        [
            "",
            "## 5. 相对 Windows `menus.json` 的全部结构与文案差异",
            "",
            "### 5.1 移入 `After Effects` 应用菜单（原位置移除）",
            "",
            "| Windows 位置 | macOS 处理方式 | macOS 实际位置 |",
            "|---|---|---|",
        ]
    )
    for m in diff["movedOrRemovedFromStandardMenus"]:
        status_zh = "移至应用菜单" if m["macStatus"] == "moved_to_app_menu" else "移至应用菜单并改名"
        lines.append(f"| `{m['windowsMenu']}` | {status_zh} | `{m['macLocation']}` |")

    lines.extend(
        [
            "",
            "### 5.2 平台文案差异（Finder vs 资源管理器）",
            "",
            "AE 简体中文词典（`$$$/AE/MenuID/Various/RevealInFileSystem`）使用 `{{*MSWindows*}}` 分叉，macOS 简体中文界面实际显示为 **「在 Finder 中显示」**（保留英文 `Finder`，未译为“访达”）：",
            "",
            "| macOS 菜单路径 | macOS 中文标签 | Windows 菜单路径 | `aeCommandId` |",
            "|---|---|---|---:|",
        ]
    )
    for r in diff["platformRenamedItems"]:
        lines.append(
            f"| `{r['macPath']}` | {r['macLabelZh']} | `{r['winPath']}` | {r['aeCommandId']} |"
        )

    lines.extend(
        [
            "",
            "### 5.3 macOS 系统自动注入的菜单项 & `Window` 菜单说明",
            "",
            "| macOS 菜单路径 | 快捷键 | `aeCommandId` | 来源说明 |",
            "|---|---|---:|---|",
        ]
    )
    inj_origin_zh = {
        "Edit > AutoFill > (Contact… / Passwords… / Credit Card…)": "macOS AppKit 自动向 Edit 菜单注入的自动填充子菜单",
        "Edit > Start Dictation…": "macOS AppKit 自动向 Edit 菜单注入的听写项",
        "View > Enter Full Screen": "macOS AppKit 自动向 View 菜单注入的系统全屏切换",
    }
    for inj in diff["macSystemInjectedItems"]:
        sc = f"`{inj['shortcut']}`" if inj.get("shortcut") else "*(无)*"
        lines.append(
            f"| `{inj['macPath']}` | {sc} | {inj['aeCommandId']} | {inj_origin_zh.get(inj['macPath'], inj['origin'])} |"
        )

    lines.extend(
        [
            "",
            "- **关于 `Window`（窗口）菜单**：与普通 macOS 文档应用不同，AE 的 `Window` 菜单完全用于工作区、扩展、面板、查看器和 ScriptUI 面板，**不包含**标准 Cocoa 的 `Minimize`（最小化）、`Zoom`（缩放）或 `Bring All to Front`（前置全部窗口）菜单项；但 `Cmd+M`（最小化）在「使用系统快捷键」开启时仍会被系统/AE 拦截，因此 `Add to Render Queue` 被动态改绑为 `Ctrl+Cmd+M`。",
            "",
            "### 5.4 菜单栏实时快捷键提示差异（重点）",
            "",
            "即使 `aeks` 配置文件写的是 `Cmd+H` / `Cmd+M` / `Option+Cmd+H`，当 macOS 独占偏好 `Pref_USE_STD_SYSTEM_SHORTCUTS_v2` 为开启（默认）时，AE 会在菜单栏中**动态将冲突命令加上 `Control`（`⌃`）**；另外若干带小键盘的命令在 Mac 菜单栏优先展示无小键盘替代键：",
            "",
            "| 菜单路径 | macOS 实机菜单显示 | Windows 菜单显示 | 原因 |",
            "|---|---|---|---|",
        ]
    )
    sc_reason_zh = {
        "File > Replace Footage > File...": "`Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true` 时 `Cmd+H` 让位给「隐藏 After Effects」，动态改绑为 `Ctrl+Cmd+H`",
        "Composition > Add to Render Queue": "`Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true` 时 `Cmd+M` 让位给 macOS 最小化，动态改绑为 `Ctrl+Cmd+M`",
        "Animation > Toggle Hold Keyframe": "`Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true` 时 `Option+Cmd+H` 让位给「隐藏其他」，动态改绑为 `Ctrl+Option+Cmd+H`",
        "Edit > Purge > All Memory": "菜单优先显示 `macControl+PadClear`（`⌃⌦`），而非小键盘 `/`",
        "Composition > Save Current Preview...": "菜单优先显示 MacBook 无小键盘绑定 `macControl+Shift+C`，而非小键盘 `0`",
        "Layer > Markers > Add Marker": "菜单优先显示 MacBook 无小键盘绑定 `macControl+8`，而非小键盘 `*`",
        "View > Use Display Color Management": "菜单优先显示 MacBook 无小键盘绑定 `macControl+Option+/`，而非 `Shift+Numpad /`",
        "After Effects > Settings… > Startup & Repair...": "AE 保留传统 `Option+Cmd+;`，**不是** macOS 常规的 `Cmd+,`",
    }
    for sc_diff in diff["macMenuShortcutDisplayDifferences"]:
        lines.append(
            f"| `{sc_diff['macPath']}` | `{sc_diff['macDisplayedShortcut']}` | `{sc_diff['winDisplayedShortcut']}` | {sc_reason_zh.get(sc_diff['macPath'], sc_diff['reason'])} |"
        )

    lines.extend(
        [
            "",
            "### 5.5 版本与安装环境差异（AE 26.2.1 vs Win 26.0x67，非平台差异）",
            "",
            "| 菜单路径 | 快捷键 | `aeCommandId` | 说明 |",
            "|---|---|---:|---|",
        ]
    )
    vd_note_zh = {
        "Edit > Quick Apply...": "AE 26.2 新增命令（中文「快速应用...」），Windows 26.0x67 快照尚未包含",
        "Edit > Paste Reversed Keyframes": "26.0 已有此菜单项但无快捷键，26.2 新增默认快捷键 `Shift+Cmd+V`",
        "File > New > New Team Project... / File > Open Team Project... / Edit > Team Project (6 items)": "Creative Cloud 团队项目启用时显示",
        "Help > Adobe Crash Reporter... / Manage My Account... / Sign Out... / Updates...": "实机 Mac 26.2.1 帮助菜单底部包含账号与更新项；Windows 26.0x67 快照未包含",
    }
    for vd in diff["versionOrInstallDeltas26_2_vs_26_0"]:
        sc = f"`{vd['shortcut']}`" if vd.get("shortcut") else "*(无)*"
        lines.append(
            f"| `{vd['macPath']}` | {sc} | {vd.get('aeCommandId', '')} | {vd_note_zh.get(vd['macPath'], vd['note'])} |"
        )

    lines.extend(
        [
            "",
            "> 备注：未打开菜单弹窗时，Cocoa 延迟填充的 `Edit ▸ Label` 16 个颜色槽在 `AXMenuBar` 中显示为占位 `Unused`（对应 `aeCommandId=2351`），展开后按偏好填充 16 种标签色（与 Windows `menus.json` 相同）。内置 22 个 `Effect` 分类子菜单与 Windows 数量完全一致（本机 Mac 仅含内置 `Mocha AE`，未装第三方插件）。",
            "",
        ]
    )

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Extract macOS AE menus via AXMenuBar + aectl findMenuCommandId."
    )
    parser.add_argument("--ae-app", type=pathlib.Path, default=DEFAULT_AE_APP)
    parser.add_argument("--win-menus", type=pathlib.Path, default=DEFAULT_WIN_MENUS)
    parser.add_argument("--out-dir", type=pathlib.Path, default=DEFAULT_OUT_DIR)
    parser.add_argument(
        "--raw-ax-json",
        type=pathlib.Path,
        default=None,
        help="Optional pre-dumped AXMenuBar JSON file (skips invoking swift dumper).",
    )
    args = parser.parse_args()

    raw_menus = (
        json.loads(args.raw_ax_json.read_text(encoding="utf-8"))
        if args.raw_ax_json
        else dump_ax_menubar()
    )
    iso_lang, cmd_ids = probe_ae_command_ids(raw_menus)
    sanitized_menus = sanitize_menu_tree(raw_menus, cmd_ids)
    top_count, sub_count, item_count = count_tree(sanitized_menus)

    win_data = json.loads(args.win_menus.read_text(encoding="utf-8"))
    zh_dict = load_zh_dict_highlights(args.ae_app)
    ae_ver = detect_ae_version(args.ae_app)
    diff = build_windows_diff(sanitized_menus, win_data, zh_dict)

    payload: dict[str, Any] = {
        "source": {
            "app": ae_ver["app"],
            "version": ae_ver["version"],
            "bundleVersion": ae_ver["bundleVersion"],
            "buildNumber": ae_ver["buildNumber"],
            "language": iso_lang,
            "platform": detect_macos_platform(),
            "extractedAt": datetime.date.today().isoformat(),
        },
        "extractionMethods": [
            "macos_AXUIElement_AXMenuBar",
            "extendscript_app_findMenuCommandId",
            "bundle_nib_and_zh_CN_dictionary",
        ],
        "sanitizationNote": "MRU recent-file/footage/script/preset lists, host macOS Services entries, and signed-in account strings have been redacted.",
        "topLevelCount": top_count,
        "submenuCount": sub_count,
        "itemCount": item_count,
        "menus": sanitized_menus,
        "windowsDiff": diff,
    }

    args.out_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.out_dir / "menus.json"
    md_path = args.out_dir / "menus.md"

    json_path.write_text(
        json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    md_path.write_text(render_markdown(payload), encoding="utf-8")

    print(
        f"Wrote {json_path} ({json_path.stat().st_size} bytes) and {md_path} ({md_path.stat().st_size} bytes)"
    )
    print(f"Top-level: {top_count}, Submenus: {sub_count}, Items: {item_count}, Language: {iso_lang}")


if __name__ == "__main__":
    main()
