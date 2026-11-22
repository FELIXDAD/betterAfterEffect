#!/usr/bin/env python3
"""Extract macOS After Effects default keyboard shortcuts and diff against
Windows docs/reference/ae/shortcuts.json.

Produces:
  - docs/reference/ae/mac/shortcuts.json
  - docs/reference/ae/mac/shortcuts.md
"""

from __future__ import annotations

import argparse
import datetime
import json
import pathlib
import platform
import plistlib
import subprocess
import sys
from typing import Any

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[2]
sys.path.insert(0, str(SCRIPT_DIR))

import parse_ae_configs as pac  # noqa: E402


DEFAULT_AE_APP = pathlib.Path(
    "/Applications/Adobe After Effects 2026/Adobe After Effects 2026.app"
)
DEFAULT_AEKS_FILE = (
    pathlib.Path.home()
    / "Library"
    / "Preferences"
    / "Adobe"
    / "After Effects"
    / "26.2"
    / "aeks"
    / "After Effects Default.txt"
)
DEFAULT_PREFS_DIR = (
    pathlib.Path.home() / "Library" / "Preferences" / "Adobe" / "After Effects" / "26.2"
)
DEFAULT_WIN_SHORTCUTS = REPO_ROOT / "docs" / "reference" / "ae" / "shortcuts.json"
DEFAULT_OUT_DIR = REPO_ROOT / "docs" / "reference" / "ae" / "mac"


def detect_macos_platform() -> str:
    try:
        prod = subprocess.check_output(["sw_vers", "-productName"], text=True).strip()
        ver = subprocess.check_output(["sw_vers", "-productVersion"], text=True).strip()
        build = subprocess.check_output(["sw_vers", "-buildVersion"], text=True).strip()
        arch = platform.machine()
        return f"{prod} {ver} ({build}, {arch})"
    except Exception:
        return f"macOS ({platform.machine()})"


def detect_ae_version(app_path: pathlib.Path) -> dict[str, Any]:
    plist_path = app_path / "Contents" / "Info.plist"
    if plist_path.exists():
        with plist_path.open("rb") as fh:
            info = plistlib.load(fh)
        short_ver = str(info.get("CFBundleShortVersionString", "unknown"))
        bundle_ver = str(info.get("CFBundleVersion", short_ver))
        build_num: int | None = None
        parts = bundle_ver.split(".")
        if parts and parts[-1].isdigit():
            build_num = int(parts[-1])
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


def detect_aeks_header(aeks_file: pathlib.Path) -> dict[str, str]:
    header: dict[str, str] = {}
    for sec_name, items in pac.parse_ae_ini_file(aeks_file):
        if sec_name == "** header **":
            for k, v in items:
                val, _ = pac.decode_ae_pref_token(v)
                header[k] = val if val is not None else v.strip('"')
            break
    return header


def read_std_system_shortcuts_pref(prefs_dir: pathlib.Path) -> bool | None:
    pref_file = prefs_dir / "Adobe After Effects 26.2 Prefs-indep-general.txt"
    if not pref_file.exists():
        return None
    for sec_name, items in pac.parse_ae_ini_file(pref_file):
        if sec_name == "Main Pref Section v2":
            for k, v in items:
                if k == "Pref_USE_STD_SYSTEM_SHORTCUTS_v2":
                    _, raw_bytes = pac.decode_ae_pref_token(v)
                    if len(raw_bytes) == 1:
                        return raw_bytes[0] != 0
    return None


def map_win_to_mac_simple(binding: str) -> str:
    tokens = binding.split("+")
    mapped = [
        "Cmd" if t == "Ctrl" else ("Option" if t == "Alt" else t) for t in tokens
    ]
    return "+".join(mapped)


def classify_exception(
    ctx: str, cmd: str, win_bindings: list[str], mac_bindings: list[str]
) -> tuple[str, str]:
    if ctx in ("MacSysShortcutsStd", "MacSysShortcutsAlt"):
        return (
            "mac_system_menu_shortcuts",
            "macOS application/window menu shortcuts controlled by Pref_USE_STD_SYSTEM_SHORTCUTS_v2 (unbound on Windows).",
        )
    if cmd.endswith("NewKF") and not win_bindings and mac_bindings:
        return (
            "mac_option_direct_keyframe",
            "macOS has no Alt+letter menu mnemonics, so Option+<key> directly adds a keyframe (Windows leaves unbound and requires Alt+Shift+<key>).",
        )
    if ctx in ("CCompCompCmd", "CPanoProjLayerPano") and cmd.startswith("NudgeOpacity"):
        return (
            "ctrl_to_mac_control",
            "Uses macControl (⌃) instead of Cmd (⌘) with Option+PadPlus/PadMinus to avoid macOS/viewer zoom conflicts.",
        )
    if ctx == "CCompCompCmd" and cmd in ("TwirlExplode", "TwirlPreserveSoloExplode"):
        return (
            "ctrl_to_mac_control",
            "Uses macControl (⌃) instead of Cmd (⌘) because Cmd+` is reserved by macOS for switching windows of the current application.",
        )
    if cmd in ("ScanForChangedFootage", "TextResetTracking"):
        return (
            "logout_conflict_adds_mac_control",
            "Adds macControl (⌃) because Cmd+Shift+Q / Cmd+Option+Shift+Q are reserved by macOS for Log Out.",
        )
    return ("other", "Differs from simple Ctrl->Cmd / Alt->Option substitution.")


def classify_mac_only_command(ctx: str, cmd: str) -> tuple[str, str]:
    if ctx == "CSwitchboard" and cmd == "Clear":
        return (
            "platform_or_keyboard",
            "Bound to PadClear (Apple Extended Keyboard Clear key); CSwitchboardModal.Clear has PadClear on both platforms.",
        )
    return (
        "version_26_2_vs_26_0",
        "Present in Mac AE 26.2.1 default aeks; absent in Windows AE 26.0x67 snapshot (likely added in AE 26.1/26.2 across platforms).",
    )


def build_diff(
    mac_data: dict[str, Any],
    win_data: dict[str, Any],
    std_sys_pref_default: bool | None,
) -> dict[str, Any]:
    mac_ctx = {
        c: {e["command"]: e["shortcuts"] for e in entries}
        for c, entries in mac_data["contexts"].items()
    }
    win_ctx = {
        c: {e["command"]: e["shortcuts"] for e in entries}
        for c, entries in win_data["contexts"].items()
    }

    exact_simple_count = 0
    non_trivial_exceptions: list[dict[str, Any]] = []
    mac_only_commands: list[dict[str, Any]] = []
    win_only_commands: list[dict[str, Any]] = []
    mac_control_bindings: list[dict[str, Any]] = []

    for ctx in sorted(set(mac_ctx) | set(win_ctx)):
        m_cmds = mac_ctx.get(ctx, {})
        w_cmds = win_ctx.get(ctx, {})
        for cmd in sorted(set(m_cmds) | set(w_cmds)):
            if cmd not in w_cmds:
                cat, note = classify_mac_only_command(ctx, cmd)
                mac_only_commands.append(
                    {
                        "context": ctx,
                        "command": cmd,
                        "macShortcuts": m_cmds[cmd],
                        "category": cat,
                        "note": note,
                    }
                )
                continue
            if cmd not in m_cmds:
                win_only_commands.append(
                    {
                        "context": ctx,
                        "command": cmd,
                        "winShortcuts": w_cmds[cmd],
                    }
                )
                continue

            wb = w_cmds[cmd]
            mb = m_cmds[cmd]
            expected_mb = [map_win_to_mac_simple(x) for x in wb]
            is_exception = mb != expected_mb
            if not is_exception:
                exact_simple_count += 1
            else:
                cat, reason = classify_exception(ctx, cmd, wb, mb)
                non_trivial_exceptions.append(
                    {
                        "context": ctx,
                        "command": cmd,
                        "winShortcuts": wb,
                        "expectedSimpleMac": expected_mb,
                        "macShortcuts": mb,
                        "category": cat,
                        "reason": reason,
                    }
                )

            if any("macControl" in s for s in mb):
                mac_control_bindings.append(
                    {
                        "context": ctx,
                        "command": cmd,
                        "winShortcuts": wb,
                        "macShortcuts": mb,
                        "alsoInNonTrivialExceptions": is_exception,
                    }
                )

    return {
        "comparedAgainst": win_data.get("source", {}),
        "summary": {
            "macContextCount": mac_data["contextCount"],
            "winContextCount": win_data["contextCount"],
            "macCommandCount": mac_data["commandCount"],
            "winCommandCount": win_data["commandCount"],
            "exactSimpleCtrlCmdAltOptionMatches": exact_simple_count,
            "nonTrivialExceptionCount": len(non_trivial_exceptions),
            "macOnlyCommandCount": len(mac_only_commands),
            "winOnlyCommandCount": len(win_only_commands),
            "macControlBindingCount": len(mac_control_bindings),
            "macControlSharedWithWinAeksCount": sum(
                1
                for item in mac_control_bindings
                if not item["alsoInNonTrivialExceptions"]
            ),
        },
        "systemShortcutPreference": {
            "prefSection": "Main Pref Section v2",
            "prefKey": "Pref_USE_STD_SYSTEM_SHORTCUTS_v2",
            "defaultValue": std_sys_pref_default,
            "whenTrueContext": "MacSysShortcutsStd",
            "whenFalseContext": "MacSysShortcutsAlt",
            "conflictsWhenTrue": [
                {
                    "shortcut": "Cmd+H",
                    "macSystemAction": "MacSysShortcutsStd.MacSysMenuHideMe",
                    "aeCommand": "CSwitchboard.ReplaceFootage",
                    "altSystemShortcut": "Cmd+macControl+H",
                },
                {
                    "shortcut": "Cmd+Option+H",
                    "macSystemAction": "MacSysShortcutsStd.MacSysMenuHideOthers",
                    "aeCommand": "CSwitchboard.ToggleKeyframeHoldInterp",
                    "altSystemShortcut": "Cmd+Option+macControl+H",
                },
                {
                    "shortcut": "Cmd+M",
                    "macSystemAction": "MacSysShortcutsStd.MacSysMenuMinimize",
                    "aeCommand": "CSwitchboard.AddCompToRenderQueue",
                    "aeAlternateBinding": "Cmd+Shift+/",
                    "altSystemShortcut": "Cmd+macControl+M",
                },
            ],
        },
        "nonTrivialExceptions": non_trivial_exceptions,
        "macOnlyCommands": mac_only_commands,
        "winOnlyCommands": win_only_commands,
        "macControlBindings": mac_control_bindings,
    }


def fmt_bindings(bindings: list[str]) -> str:
    if not bindings:
        return "*(无)*"
    return " / ".join(f"`` {b} ``" if "`" in b else f"`{b}`" for b in bindings)


def render_markdown(payload: dict[str, Any]) -> str:
    src = payload["source"]
    diff = payload["windowsDiff"]
    win_src = diff["comparedAgainst"]
    summary = diff["summary"]
    sys_pref = diff["systemShortcutPreference"]

    lines: list[str] = [
        "# After Effects macOS 默认键盘快捷键与 Windows 差异对照",
        "",
        f"> 来源：{src['app']} {src['version']}（build `{src['bundleVersion']}`）| 平台：{src['platform']} | 提取日期：{src['extractedAt']}",
        f"> 对照基准：{win_src.get('app', 'Adobe After Effects')} {win_src.get('version', '26.0x67')} | 平台：{win_src.get('platform', 'Windows')}",
        "",
        "完整 32 个上下文、674 条命令的解析结果见 [`shortcuts.json`](shortcuts.json)（与 Windows 版 [`../shortcuts.json`](../shortcuts.json) 保持相同顶层结构，并附加 `windowsDiff` 机器可读差异表）。",
        "",
        "## 1. 总体统计",
        "",
        "| 指标 | 数值 | 说明 |",
        "|---|---:|---|",
        f"| 上下文数（Mac / Win） | {summary['macContextCount']} / {summary['winContextCount']} | 32 个上下文名称完全一致 |",
        f"| 命令总数（Mac / Win） | {summary['macCommandCount']} / {summary['winCommandCount']} | Mac 26.2.1 比 Win 26.0x67 多 {summary['macOnlyCommandCount']} 条命令，少 {summary['winOnlyCommandCount']} 条 |",
        f"| 简单换算完全一致（`Ctrl→Cmd`、`Alt→Option`） | {summary['exactSimpleCtrlCmdAltOptionMatches']} | 占双方共有 669 条命令的 {summary['exactSimpleCtrlCmdAltOptionMatches'] / 669 * 100:.1f}% |",
        f"| **非简单换算例外** | **{summary['nonTrivialExceptionCount']}** | 见第 2 节逐条清单 |",
        f"| 仅在 Mac `aeks` 出现的命令 | {summary['macOnlyCommandCount']} | 见第 4 节（含 26.2 vs 26.0 版本差异说明） |",
        f"| 含 `macControl`（`⌃`）的有效 Mac 绑定 | {summary['macControlBindingCount']} | 其中 {summary['macControlSharedWithWinAeksCount']} 条在 Windows `aeks` 中也有同名字符串但 Windows 无 `macControl` 键，仅在 Mac 生效（见第 3 节） |",
        "",
        "## 2. 非简单 `Ctrl→Cmd` / `Alt→Option` 换算的全部例外（26 条）",
        "",
        "以下 26 条命令不能通过将 Windows 绑定中的 `Ctrl` 替换为 `Cmd`、`Alt` 替换为 `Option` 得到，开发快捷键系统时必须按平台单独处理。",
        "",
    ]

    category_order = [
        (
            "mac_option_direct_keyframe",
            "### 2.1 Mac 独占 `Option+<属性键>` 直接添加关键帧（8 条）",
            "Windows 因 `Alt+字母` 会触发菜单栏助记符，这 8 条命令在 Windows 默认留空（需用 `Alt+Shift+<键>` 的 `*AddStateNewKF`）；macOS 无 `Alt` 菜单助记符，直接绑定为 `Option+<键>`（同时保留 `Option+Shift+<键>`）。",
        ),
        (
            "ctrl_to_mac_control",
            "### 2.2 `Ctrl` 映射为物理 `macControl`（`⌃`）而非 `Cmd`（`⌘`）（10 条）",
            "为避开 macOS 系统快捷键（`` Cmd+` `` 切换同应用窗口）以及视图缩放冲突，以下命令在 Mac 上把 Windows 的 `Ctrl` 改映射为 `macControl`（Control 键）。注意：`CTopic.TwirlExplode` 在 `aeks` 中仍保留 `` Cmd+` ``，而时间轴上下文 `CCompCompCmd` 覆盖为 `` macControl+` ``。",
        ),
        (
            "logout_conflict_adds_mac_control",
            "### 2.3 避开 macOS 系统注销快捷键而追加 `macControl`（`⌃`）（2 条）",
            "macOS 将 `Cmd+Shift+Q` 和 `Cmd+Option+Shift+Q` 保留为退出登录，AE 在 Mac 上追加了 `macControl` 修饰键。",
        ),
        (
            "mac_system_menu_shortcuts",
            "### 2.4 macOS 系统菜单快捷键（6 条，受偏好开关控制）",
            "Windows 上这 6 条全部为空；macOS 上由 `Main Pref Section v2` 中的 `Pref_USE_STD_SYSTEM_SHORTCUTS_v2`（界面项「使用系统快捷键」/ *Use System Shortcut Keys*）控制启用哪一组。",
        ),
    ]

    by_cat: dict[str, list[dict[str, Any]]] = {}
    for exc in diff["nonTrivialExceptions"]:
        by_cat.setdefault(exc["category"], []).append(exc)

    for cat_key, heading, intro in category_order:
        items = by_cat.get(cat_key, [])
        lines.append(heading)
        lines.append("")
        lines.append(intro)
        lines.append("")
        lines.append("| 上下文 | 命令标识 | Windows 默认 | 简单换算预期 | macOS 实际默认 |")
        lines.append("|---|---|---|---|---|")
        for it in items:
            lines.append(
                f"| `{it['context']}` | `{it['command']}` | {fmt_bindings(it['winShortcuts'])} | {fmt_bindings(it['expectedSimpleMac'])} | {fmt_bindings(it['macShortcuts'])} |"
            )
        lines.append("")

    # System shortcut conflict callout
    default_str = (
        "`true`（启用 `MacSysShortcutsStd`）"
        if sys_pref["defaultValue"] is True
        else ("`false`（启用 `MacSysShortcutsAlt`）" if sys_pref["defaultValue"] is False else "未知")
    )
    lines.extend(
        [
            "#### 「使用系统快捷键」开关与 AE 快捷键冲突说明",
            "",
            f"- 偏好键：`[{sys_pref['prefSection']}]` / `{sys_pref['prefKey']}`，本机 AE 26.2.1 默认值为 **{default_str}**（仅存在于 macOS 偏好文件，Windows 偏好文件无此键）。",
            "- 开启时（默认），以下 3 个 macOS 系统菜单快捷键会覆盖同键位的 AE 命令；关闭后系统菜单快捷键改为追加 `Control`（`MacSysShortcutsAlt`），释放原键位给 AE：",
            "",
            "| 快捷键 | 系统菜单命令（开启时生效） | 被覆盖的 AE 命令 | AE 备用绑定 | 关闭开关后的系统快捷键 |",
            "|---|---|---|---|---|",
        ]
    )
    for c in sys_pref["conflictsWhenTrue"]:
        alt_ae = f"`{c['aeAlternateBinding']}`" if "aeAlternateBinding" in c else "*(无)*"
        lines.append(
            f"| `{c['shortcut']}` | `{c['macSystemAction']}` | `{c['aeCommand']}` | {alt_ae} | `{c['altSystemShortcut']}` |"
        )
    lines.append("")

    # Section 3: macControl bindings active on macOS (including ones also present in Win aeks)
    shared_mc = [
        item
        for item in diff["macControlBindings"]
        if not item["alsoInNonTrivialExceptions"]
    ]
    lines.extend(
        [
            f"## 3. 仅在 macOS 实际生效的 `macControl`（`⌃`）绑定（{len(shared_mc)} 条）",
            "",
            f"以下 {len(shared_mc)} 条命令在 Windows `aeks` 文件中也保留了含 `macControl` 的跨平台字符串，但 Windows 没有第四修饰键（Control 已用作 `Ctrl`），因此这些 `macControl+...` 绑定**仅在 macOS 上可用**。它们主要是为无数字小键盘的 MacBook 提供的替代键位，以及带 `Cmd+Ctrl` 的递归品质/分辨率命令：",
            "",
            "| 上下文 | 命令标识 | macOS 绑定（含 `macControl`） | 用途分类 |",
            "|---|---|---|---|",
        ]
    )

    def mc_purpose(cmd: str) -> str:
        if cmd.endswith("Recurse"):
            return "四修饰键递归设置嵌套合成品质/分辨率"
        if cmd == "WriteStringFile":
            return "内部调试命令"
        if cmd == "PurgeAllMemory":
            return "Mac 键盘 Clear 键配合 Control 清理内存"
        return "无数字小键盘（MacBook）替代快捷键"

    for item in shared_mc:
        lines.append(
            f"| `{item['context']}` | `{item['command']}` | {fmt_bindings(item['macShortcuts'])} | {mc_purpose(item['command'])} |"
        )
    lines.append("")

    # Section 4: Mac-only commands (26.2.1 vs 26.0x67)
    lines.extend(
        [
            f"## 4. 仅在 Mac 26.2.1 出现的命令（{len(diff['macOnlyCommands'])} 条）",
            "",
            "> 注意：Windows 参考数据提取自 AE `26.0x67`，本机 Mac 为 AE `26.2.1`，以下差异包含小版本新增项。",
            "",
            "| 上下文 | 命令标识 | macOS 绑定 | 差异归因 |",
            "|---|---|---|---|",
        ]
    )
    for item in diff["macOnlyCommands"]:
        zh_note = (
            "绑定为 `PadClear`（Apple 扩展键盘数字区 Clear 键；`CSwitchboardModal.Clear` 在双平台均有 `PadClear`）"
            if item["category"] == "platform_or_keyboard"
            else "AE 26.2.1 默认 `aeks` 新增项，Windows `26.0x67` 快照时尚未收录（推测为 26.1/26.2 跨平台新增）"
        )
        lines.append(
            f"| `{item['context']}` | `{item['command']}` | {fmt_bindings(item['macShortcuts'])} | {zh_note} |"
        )
    lines.append("")

    # Section 5: Key naming & implementation notes for BAE
    lines.extend(
        [
            "## 5. 给 BAE 快捷键实现的 macOS 键位约定说明",
            "",
            "1. **修饰键标识映射**：AE `aeks` 文件使用 4 个修饰键词元：`Cmd`（macOS `⌘`，对应 Windows `Ctrl`）、`Option`（macOS `⌥`，对应 Windows `Alt`）、`Shift`（`⇧`）、`macControl`（macOS 物理 Control `⌃`，**不是** `Cmd`）。",
            "2. **删除键区分**：AE 的 `Delete` 对应主键盘退格键（macOS 标为 Delete `⌫`，Windows 标为 Backspace），`FwdDel` 对应向前删除键（macOS 扩展键盘 `⌦` 或 MacBook `Fn+Delete`，Windows 标为 Delete）。可从 `TextLayerUI.TextDeleteBackward = [\"Delete\"]` 与 `TextLayerUI.TextDeleteForward = [\"FwdDel\"]` 交叉印证。",
            "3. **回车键区分**：`Return` 为主键盘回车（`↩`，用于重命名 `COutline.Rename`、`Cmd+Return` 提交文字），`Enter` 为数字小键盘 Enter（`⌤`，MacBook 上为 `Fn+Return`，用于打开项目项 `POutlinePano.OpenItemWindow`、提交文字 `TextLayerUI.TextCommit`）。",
            "4. **无小键盘导航键（MacBook）**：AE 大量时间轴/定位快捷键使用 `HOME`、`END`、`PageUP`、`PageDOWN`（如 `Cmd+HOME` 视点居中、`Cmd+Option+HOME` 居中锚点、`PageUP`/`PageDOWN` 逐帧步进），在 MacBook 上对应 `Fn+←/→/↑/↓`；此外 `macControl+0` / `macControl+.` / `macControl+8` 等为小键盘 `Pad0`/`PadDecimal`/`PadMultiply` 的官方 Mac 替代键。",
            "5. **设置对话框快捷键**：在 `aeks` 默认配置中，`CSwitchboard.PrefsGeneral` 与 `CSwitchboard.PrefsStartupAndRepairV2` 均绑定为 `Cmd+Option+;`（与 Windows 的 `Ctrl+Alt+;` 对应），**而非** macOS 常规应用的 `Cmd+,`（实际菜单栏快捷键显示将在菜单调研中进一步核对）。",
            "6. **文件换行符**：macOS 版 `After Effects Default.txt` 使用经典 CR（`\\r`）换行，解析时需开启通用换行处理。",
            "",
        ]
    )

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Extract macOS AE default shortcuts and diff with Windows."
    )
    parser.add_argument("--aeks-file", type=pathlib.Path, default=DEFAULT_AEKS_FILE)
    parser.add_argument("--prefs-dir", type=pathlib.Path, default=DEFAULT_PREFS_DIR)
    parser.add_argument("--ae-app", type=pathlib.Path, default=DEFAULT_AE_APP)
    parser.add_argument(
        "--win-shortcuts", type=pathlib.Path, default=DEFAULT_WIN_SHORTCUTS
    )
    parser.add_argument("--out-dir", type=pathlib.Path, default=DEFAULT_OUT_DIR)
    args = parser.parse_args()

    if not args.aeks_file.exists():
        raise FileNotFoundError(f"Mac shortcuts file not found: {args.aeks_file}")
    if not args.win_shortcuts.exists():
        raise FileNotFoundError(f"Windows shortcuts file not found: {args.win_shortcuts}")

    mac_raw = pac.parse_shortcuts(args.aeks_file)
    win_data = json.loads(args.win_shortcuts.read_text(encoding="utf-8"))
    ae_ver = detect_ae_version(args.ae_app)
    aeks_header = detect_aeks_header(args.aeks_file)
    std_sys_pref = read_std_system_shortcuts_pref(args.prefs_dir)

    diff = build_diff(mac_raw, win_data, std_sys_pref)

    payload: dict[str, Any] = {
        "source": {
            "app": ae_ver["app"],
            "version": ae_ver["version"],
            "bundleVersion": ae_ver["bundleVersion"],
            "buildNumber": ae_ver["buildNumber"],
            "platform": detect_macos_platform(),
            "extractedAt": datetime.date.today().isoformat(),
            "aeksSource": "~/Library/Preferences/Adobe/After Effects/26.2/aeks/After Effects Default.txt",
            "aeksHeader": aeks_header,
        },
        "contextCount": mac_raw["contextCount"],
        "commandCount": mac_raw["commandCount"],
        "contexts": mac_raw["contexts"],
        "windowsDiff": diff,
    }

    args.out_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.out_dir / "shortcuts.json"
    md_path = args.out_dir / "shortcuts.md"

    json_path.write_text(
        json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    md_path.write_text(render_markdown(payload), encoding="utf-8")

    s = diff["summary"]
    print(
        f"Wrote {json_path} ({json_path.stat().st_size} bytes) and {md_path} ({md_path.stat().st_size} bytes)"
    )
    print(
        f"Mac: {s['macContextCount']} contexts, {s['macCommandCount']} commands | "
        f"Simple matches: {s['exactSimpleCtrlCmdAltOptionMatches']} | "
        f"Non-trivial exceptions: {s['nonTrivialExceptionCount']} | "
        f"Mac-only commands: {s['macOnlyCommandCount']} | "
        f"macControl bindings: {s['macControlBindingCount']}"
    )


if __name__ == "__main__":
    main()
