#!/usr/bin/env python3
"""AE shortcut sweep on a real Mac: presses every AE default shortcut BAE has
implemented as a real key event and checks which action BAE ran.

Rows come from docs/ui/ae-shortcuts.json (generated from the AE command catalog,
crates/bae-ui-motion/src/catalog). BAE runs with `BAE_HOME` in a temporary
folder (the user's settings and projects are not touched) and
`BAE_ACTION_LOG=<file>`, which makes it append every action it dispatches as a
JSON line; after each key the script reads the new lines.

Keys are sent with `orca computer hotkey` / `press-key` (real key events,
background input to the BAE process, no foreground takeover; plain CGEvent
postToPid does not reach an inactive winit window). Driving the UI holds
the machine's GUI directory lock (`--lock`, mkdir: one holder at a time, at
most 20 minutes). orca cannot send F keys yet: those rows are
listed as not pressed (the CI sweep covers them). Chords macOS keeps for itself (Help menu
search, Mission Control, screenshots, …) are not pressed. After a key that fails,
BAE is restarted so the next key starts clean.

Usage (from the repository root):

    cargo build --release -p bae
    tools/qa/shortcut_sweep_mac.py --bae target/release/bae \
        --report docs/test-reports/m4/$(date +%F)-shortcut-sweep.md

The CI sweep (apps/bae acceptance::shortcut_sweep) covers every scope with
synthetic events; this tool checks what a real keyboard does on macOS
(application-wide shortcuts, through the native menu where it takes the key).
"""

import argparse
import datetime
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Actions the sweep does not press: they quit, close the project or panels,
# open file / save dialogs or a web browser, which would stop the run or need
# a person.
SKIP = {
    "app.quit": "quits",
    "window.close_panel": "closes the focused panel",
    "file.new_project": "replaces the test project",
    "file.close_project": "closes the test project",
    "file.open_project": "opens a file dialog",
    "file.save": "opens a save dialog (untitled project)",
    "file.save_as": "opens a save dialog",
    "help.docs": "opens a web browser",
    "motion.file.import_file": "opens a file dialog",
    "motion.file.import_multiple_files": "opens a file dialog",
}

# Chords macOS keeps for itself: pressing them would open the Help menu
# search, switch Spaces, take screenshots, hide the Dock or lock the screen.
SYSTEM = {
    "Cmd+Shift+Slash": "macOS Help menu search",
    "Cmd+Space": "Spotlight",
    "Cmd+Tab": "app switcher",
    "Cmd+Backtick": "window cycling",
    "Ctrl+ArrowLeft": "Mission Control",
    "Ctrl+ArrowRight": "Mission Control",
    "Ctrl+ArrowUp": "Mission Control",
    "Ctrl+ArrowDown": "Mission Control",
    "Cmd+Shift+Num3": "screenshot",
    "Cmd+Shift+Num4": "screenshot",
    "Cmd+Shift+Num5": "screenshot",
    "Cmd+Alt+D": "Dock",
    "Cmd+Ctrl+Q": "lock screen",
    "Cmd+Alt+Escape": "Force Quit",
}

# BAE key names (egui) -> names `orca computer` understands.
KEYS = {
    "Equals": "=",
    "Minus": "-",
    "OpenBracket": "[",
    "CloseBracket": "]",
    "Period": ".",
    "Comma": ",",
    "Slash": "/",
    "Backslash": "\\",
    "Semicolon": ";",
    "Quote": "'",
    "Backtick": "`",
    "ArrowLeft": "Left",
    "ArrowRight": "Right",
    "ArrowUp": "Up",
    "ArrowDown": "Down",
    "Backspace": "Backspace",
    "Delete": "ForwardDelete",
    "Enter": "Return",
    "Space": "Space",
}


def orca_key(chord):
    """BAE chord (`Cmd+Alt+Shift+Y`, `Ctrl` = the Control key) -> (combo, plain)."""
    parts = chord.split("+")
    mods = [{"Cmd": "Cmd", "Ctrl": "Ctrl", "Alt": "Option", "Shift": "Shift"}[m] for m in parts[:-1]]
    key = KEYS.get(parts[-1], parts[-1])
    if key.startswith("Num") and key[3:].isdigit():
        key = key[3:]
    return "+".join(mods + [key]), not mods


def is_f_key(chord):
    key = chord.split("+")[-1]
    return key.startswith("F") and key[1:].isdigit()


def orca(*args):
    return subprocess.run(
        ["orca", "computer", *args, "--no-screenshot"],
        capture_output=True,
        text=True,
        timeout=60,
    )


class Bae:
    def __init__(self, binary):
        self.home = tempfile.mkdtemp(prefix="bae-sweep-")
        self.log = os.path.join(self.home, "actions.jsonl")
        open(self.log, "w").close()
        env = dict(os.environ, BAE_HOME=self.home, BAE_ACTION_LOG=self.log, BAE_LANG="en")
        self.proc = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.app = f"pid:{self.proc.pid}"
        self.read = 0
        # Wait for the window.
        for _ in range(60):
            time.sleep(0.5)
            r = orca("list-windows", "--app", self.app)
            if r.returncode == 0 and "window" in (r.stdout + r.stderr).lower():
                break
        time.sleep(1.0)
        # Keys reach the window only once it is key: click into it (the
        # Composition viewer, empty without a comp).
        orca("click", "--app", self.app, "--x", "700", "--y", "300")
        time.sleep(0.5)

    def alive(self):
        return self.proc.poll() is None

    def new_actions(self):
        with open(self.log) as f:
            lines = f.read().splitlines()
        out = [json.loads(l) for l in lines[self.read :] if l.strip()]
        self.read = len(lines)
        return out

    def key(self, chord):
        combo, plain = orca_key(chord)
        if plain:
            return orca("press-key", "--app", self.app, "--key", combo)
        return orca("hotkey", "--app", self.app, "--key", combo)

    def setup(self):
        """A comp with a solid, selected: Cmd+N (Composition Settings, Return),
        Cmd+Y (Solid Settings, Return)."""
        for chord in ("Cmd+N", "Cmd+Y"):
            self.key(chord)
            time.sleep(1.0)
            self.key("Enter")
            time.sleep(1.0)
        self.new_actions()

    def quit(self):
        if self.alive():
            self.proc.terminate()
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()


def rows(path):
    with open(path) as f:
        data = json.load(f)
    out = []
    for r in data["rows"]:
        if r["native"] or r["shadowed"] or not r["own_panel"] or not r["mac"]:
            continue
        out.append(r)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bae", default=os.path.join(ROOT, "target/release/bae"))
    ap.add_argument("--table", default=os.path.join(ROOT, "docs/ui/ae-shortcuts.json"))
    ap.add_argument("--report", default=None)
    ap.add_argument("--settle", type=float, default=0.6, help="seconds to wait after each key")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument(
        "--lock",
        default=os.environ.get("BAE_GUI_LOCK"),
        required=os.environ.get("BAE_GUI_LOCK") is None,
        help="the machine's GUI directory lock (mkdir), or BAE_GUI_LOCK",
    )
    args = ap.parse_args()

    # Wait for the GUI lock (another tool may be driving the screen).
    os.makedirs(os.path.dirname(args.lock), exist_ok=True)
    while True:
        try:
            os.mkdir(args.lock)
            break
        except FileExistsError:
            time.sleep(5)
    with open(os.path.join(args.lock, "owner"), "w") as f:
        f.write(f"shortcut_sweep_mac pid {os.getpid()}\n")

    all_rows = rows(args.table)
    working = [r for r in all_rows if r["works"]]
    passed, failed, skipped, not_covered = [], [], [], []
    bae = None
    try:
        bae = Bae(args.bae)
        bae.setup()
        todo = working[: args.limit] if args.limit else working
        for r in todo:
            what = f"`{r['context']}/{r['command']}` → `{r['action']}` {json.dumps(r['params']) if r['params'] else ''}".strip()
            if r["action"] in SKIP:
                skipped.append(f"{what}: {SKIP[r['action']]}")
                continue
            if r["scope"] is not None:
                not_covered.append(f"{what} ({r['scope']} panel; the CI sweep covers it)")
                continue
            for chord in r["mac"]:
                if chord in SYSTEM:
                    skipped.append(f"{what} `{chord}`: macOS keeps it ({SYSTEM[chord]})")
                    continue
                if is_f_key(chord):
                    skipped.append(f"{what} `{chord}`: orca computer cannot send F keys (CI sweep covers it)")
                    continue
                res = bae.key(chord)
                time.sleep(args.settle)
                ran = bae.new_actions()
                hits = [a for a in ran if a["id"] == r["action"] and (r["params"] is None or a.get("params") == r["params"])]
                line = f"{what} `{chord}`"
                if res.returncode != 0:
                    failed.append(f"{line}: key not sent ({(res.stderr or res.stdout).strip()[:120]})")
                elif len(hits) == 1:
                    passed.append(line)
                else:
                    got = ", ".join(a["id"] for a in ran) or "nothing"
                    failed.append(f"{line}: ran {len(hits)} times (dispatched: {got})")
                # Close what the key opened (dialogs, windows).
                bae.key("Escape")
                time.sleep(0.2)
                bae.key("Escape")
                time.sleep(0.2)
                bae.new_actions()
                if not bae.alive():
                    failed.append(f"{line}: BAE exited")
                if not bae.alive() or len(hits) != 1:
                    # Start the next key from a clean app.
                    bae.quit()
                    bae = Bae(args.bae)
                    bae.setup()
    finally:
        if bae:
            bae.quit()
        shutil.rmtree(args.lock, ignore_errors=True)

    unimplemented = [r for r in all_rows if not r["works"] and r["mac"]]
    by_owner = {}
    for r in unimplemented:
        by_owner[r["owner"]] = by_owner.get(r["owner"], 0) + 1
    date = datetime.date.today().isoformat()
    lines = [
        f"# AE 快捷键真机扫描（macOS，{date}）",
        "",
        f"工具：`tools/qa/shortcut_sweep_mac.py`（真实按键：`orca computer hotkey` / `press-key`，后台发给 BAE 进程；BAE 的动作日志 `BAE_ACTION_LOG`）；数据：`docs/ui/ae-shortcuts.json`。",
        "",
        f"- AE 默认快捷键（BAE 能绑定的 Mac 键）：{len(all_rows)} 条命令",
        f"- 已实现：{len(working)} 条；真机按键测了 {len(passed) + len(failed)} 个键，通过 {len(passed)}，失败 {len(failed)}",
        f"- 跳过（会退出、关闭工程、打开文件对话框 / 浏览器，macOS 自己占用的键，或 orca 发不了的 F 键）：{len(skipped)}",
        f"- 面板作用域（真机不点面板，CI 扫描覆盖）：{len(not_covered)}",
        f"- 和 AE 不一致（AE 有默认键、BAE 还没实现）：{len(unimplemented)} 条，按负责人："
        + "，".join(f"{k} {v}" for k, v in sorted(by_owner.items())),
        "",
    ]
    for title, items in (("失败", failed), ("跳过", skipped), ("面板作用域", not_covered), ("通过", passed)):
        lines.append(f"## {title}（{len(items)}）")
        lines.append("")
        lines.extend(f"- {i}" for i in items)
        lines.append("")
    lines.append("## 和 AE 不一致：AE 有默认快捷键、BAE 还没实现的命令")
    lines.append("")
    for r in unimplemented:
        lines.append(f"- `{r['context']}/{r['command']}` {' / '.join(r['mac'])} → `{r['action']}`（{r['owner']}，{r['status']}）")
    report = "\n".join(lines) + "\n"
    if args.report:
        os.makedirs(os.path.dirname(args.report), exist_ok=True)
        with open(args.report, "w") as f:
            f.write(report)
    print(report[:3000])
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
