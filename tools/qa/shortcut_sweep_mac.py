#!/usr/bin/env python3
"""AE shortcut sweep on a real Mac: presses every AE default shortcut BAE has
implemented as a real key event and checks which action BAE ran.

Rows come from docs/ui/ae-shortcuts.json (generated from the AE command catalog,
crates/bae-ui-motion/src/catalog). BAE runs with `BAE_HOME` in a temporary
folder (the user's settings and projects are not touched) and
`BAE_ACTION_LOG=<file>`, which makes it append every action it dispatches as a
JSON line; after each key the script reads the new lines.

Keys are real keyboard events posted to the BAE process (`tools/qa/postkey.swift`,
CGEvent postToPid: background input, no foreground takeover). Driving the UI
holds /private/tmp/bae-locks/gui.lock. Chords macOS keeps for itself (Help menu
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
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LOCK = "/private/tmp/bae-locks/gui.lock"

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

# BAE key names (egui) -> macOS virtual key codes (kVK_*).
LETTERS = {
    "A": 0x00, "S": 0x01, "D": 0x02, "F": 0x03, "H": 0x04, "G": 0x05, "Z": 0x06, "X": 0x07,
    "C": 0x08, "V": 0x09, "B": 0x0B, "Q": 0x0C, "W": 0x0D, "E": 0x0E, "R": 0x0F, "Y": 0x10,
    "T": 0x11, "O": 0x1F, "U": 0x20, "I": 0x22, "P": 0x23, "L": 0x25, "J": 0x26, "K": 0x28,
    "N": 0x2D, "M": 0x2E,
}
CODES = {
    "Num1": 0x12, "Num2": 0x13, "Num3": 0x14, "Num4": 0x15, "Num6": 0x16, "Num5": 0x17,
    "Num9": 0x19, "Num7": 0x1A, "Num8": 0x1C, "Num0": 0x1D,
    "Equals": 0x18, "Minus": 0x1B, "CloseBracket": 0x1E, "OpenBracket": 0x21, "Quote": 0x27,
    "Semicolon": 0x29, "Backslash": 0x2A, "Comma": 0x2B, "Slash": 0x2C, "Period": 0x2F,
    "Backtick": 0x32, "Enter": 0x24, "Tab": 0x30, "Space": 0x31, "Backspace": 0x33,
    "Escape": 0x35, "Delete": 0x75, "Home": 0x73, "End": 0x77, "PageUp": 0x74, "PageDown": 0x79,
    "ArrowLeft": 0x7B, "ArrowRight": 0x7C, "ArrowDown": 0x7D, "ArrowUp": 0x7E,
    "F1": 0x7A, "F2": 0x78, "F3": 0x63, "F4": 0x76, "F5": 0x60, "F6": 0x61, "F7": 0x62,
    "F8": 0x64, "F9": 0x65, "F10": 0x6D, "F11": 0x67, "F12": 0x6F,
}
CODES.update(LETTERS)


def key_args(chord):
    """BAE chord (`Cmd+Alt+Shift+Y`, `Ctrl` = the Control key) -> postkey args."""
    parts = chord.split("+")
    code = CODES[parts[-1]]
    mods = [{"Cmd": "cmd", "Ctrl": "ctrl", "Alt": "alt", "Shift": "shift"}[m] for m in parts[:-1]]
    return [str(code), *mods]


def build_postkey():
    out = os.path.join(tempfile.mkdtemp(prefix="bae-postkey-"), "postkey")
    src = os.path.join(ROOT, "tools/qa/postkey.swift")
    subprocess.run(["swiftc", "-O", src, "-o", out], check=True)
    return out


def orca(*args):
    return subprocess.run(
        ["orca", "computer", *args, "--no-screenshot"],
        capture_output=True,
        text=True,
        timeout=60,
    )


class Bae:
    postkey = None

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

    def alive(self):
        return self.proc.poll() is None

    def new_actions(self):
        with open(self.log) as f:
            lines = f.read().splitlines()
        out = [json.loads(l) for l in lines[self.read :] if l.strip()]
        self.read = len(lines)
        return out

    def key(self, chord):
        return subprocess.run(
            [Bae.postkey, str(self.proc.pid), *key_args(chord)], capture_output=True, text=True
        )

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
    args = ap.parse_args()

    os.makedirs(os.path.dirname(LOCK), exist_ok=True)
    try:
        fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        sys.exit(f"{LOCK} is held: {open(LOCK).read().strip()}")
    os.write(fd, f"shortcut_sweep_mac pid {os.getpid()}\n".encode())
    os.close(fd)

    all_rows = rows(args.table)
    working = [r for r in all_rows if r["works"]]
    passed, failed, skipped, not_covered = [], [], [], []
    bae = None
    try:
        Bae.postkey = build_postkey()
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
        os.remove(LOCK)

    unimplemented = [r for r in all_rows if not r["works"] and r["mac"]]
    by_owner = {}
    for r in unimplemented:
        by_owner[r["owner"]] = by_owner.get(r["owner"], 0) + 1
    date = datetime.date.today().isoformat()
    lines = [
        f"# AE 快捷键真机扫描（macOS，{date}）",
        "",
        f"工具：`tools/qa/shortcut_sweep_mac.py`（真实键盘事件：`tools/qa/postkey.swift`，CGEvent 发给 BAE 进程；BAE 的动作日志 `BAE_ACTION_LOG`）；数据：`docs/ui/ae-shortcuts.json`。",
        "",
        f"- AE 默认快捷键（BAE 能绑定的 Mac 键）：{len(all_rows)} 条命令",
        f"- 已实现：{len(working)} 条；真机按键测了 {len(passed) + len(failed)} 个键，通过 {len(passed)}，失败 {len(failed)}",
        f"- 跳过（会退出、关闭工程、打开文件对话框 / 浏览器，或 macOS 自己占用的键）：{len(skipped)}",
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
