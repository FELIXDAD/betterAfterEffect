"""Local and systemd entry point for the betterAfterEffects progress board."""
import argparse
import base64
from contextlib import closing
from datetime import datetime, timezone
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import secrets
import sqlite3
import subprocess
import threading
import time
import urllib.request
import sys

HERE = Path(__file__).resolve().parent
CONF = Path(os.getenv("BAE_BOARD_CONFIG", Path.home() / ".config/bae-board"))
DB = Path(os.getenv("BAE_BOARD_DB", CONF / "board.sqlite3"))
CHANGE = threading.Condition()
VERSION = 0


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def private_file(path, content):
    with path.open("x", encoding="utf-8") as target:
        target.write(content)


def credentials():
    CONF.mkdir(mode=0o700, parents=True, exist_ok=True)
    secret = CONF / "server.env"
    if not secret.exists():
        private_file(secret, f"BAE_BOARD_PASSWORD={secrets.token_urlsafe(18)}\n"
                     f"BAE_BOARD_TOKEN={secrets.token_urlsafe(32)}\n")
    config = dict(line.split("=", 1) for line in secret.read_text().splitlines())
    password, token = config["BAE_BOARD_PASSWORD"], config["BAE_BOARD_TOKEN"]
    if not password or not token:
        raise ValueError("Empty credentials")
    if not (CONF / "env").exists():
        private_file(CONF / "env", f"BAE_BOARD_URL=http://127.0.0.1:{PORT}\nBAE_BOARD_TOKEN={token}\n")
    return "Basic " + base64.b64encode(("bae:" + password).encode()).decode(), "Bearer " + token


def effects():
    root = HERE.parents[1]
    files = sorted((root / "docs/reference/ae/effects").glob("*.json"))
    if not files:
        raise SystemExit("首次导入需要完整仓库；独立部署请复制 SQLite 备份。")
    code = root / "crates/bae-effects"
    sources = []
    for folder in ("src", "packages"):
        for path in (code / folder).rglob("*"):
            if path.is_file():
                sources.append(path.read_text(errors="replace"))
    if not sources:
        raise SystemExit("缺少 bae-effects 源码")
    sources = "\n".join(sources)
    tags = {name: tag for tag, names in json.loads((HERE / "tags.json").read_text()).items()
            for name in names.split("|")}
    for path in files:
        if path.stem.startswith("obsolete") or path.stem == "hidden-presets":
            continue
        data = json.loads(path.read_text())
        for fx in data["effects"]:
            match, name = fx["matchName"], fx["displayName"]
            tag = tags.get(name, "待评估")
            if path.stem in ("immersive-video", "3d-channel"):
                tag = "后放"
            if name.startswith("OCIO "):
                tag = "做"
            found = re.search(r'(?<![\w-])' + re.escape(match) + r'(?![\w-]| [\w])', sources)
            yield ("fx:" + match, "effect", data["category"], name,
                   "done" if found else "todo", tag, "[]")


def connect():
    db = sqlite3.connect(DB, timeout=10)
    db.row_factory = sqlite3.Row
    return db


def initialize():
    with closing(connect()) as db, db:
        db.execute("pragma journal_mode=WAL")
        db.execute("""create table if not exists items(
            id text primary key, kind text, module text, name text, status text, tag text,
            prs text, agent text default '', updated text, note text default '')""")
        db.execute("""create table if not exists activity(
            seq integer primary key, item text, agent text, status text, note text, updated text)""")
        db.execute("""create table if not exists pulls(
            number integer primary key, item text, title text, state text, updated text)""")
        db.execute("begin immediate")
        if not db.execute("select 1 from items limit 1").fetchone():
            seed_board(db)
        register_legacy_effects(db)
        db.execute("""create table if not exists parity_issues(
            number integer primary key, title text not null, state text not null, updated text not null)""")
        db.execute("""create table if not exists parity_resolution(
            item text primary key, merged text not null)""")
        db.execute("""insert into items(id,kind,module,name,status,agent,tag,prs,updated)
            values('ae-panels-workspace','milestone','面板 / 工作区',
            '所有面板工作区、补齐 AE 26 的所有面板、窗口菜单逐项可用','todo','K','','[]',?)
            on conflict(id) do nothing""", (now(),))
        if not db.execute("select 1 from parity_issues where number=400").fetchone():
            save_issue(db, 400, "P0 易用性交互", "open", "")


# The first four checks come from issue 400, the last three from its owner comment.
INTERACTIONS = (
    ("图层内容可选中及复制", "点名称选中行，小箭头展开。形状、效果、蒙版、文字动画器支持复制、粘贴和同层级副本。"),
    ("查看器优先操作已选图层", "拖动、手柄、键盘微移作用于已选图层；取消选择后再按点击位置拾取最上层。"),
    ("数值框全选、Tab 与方向键", "单击和 Tab / Shift+Tab 全选；上下键 ±1，Cmd/Ctrl ±0.1，Shift ±10；Enter 确认，Esc 取消。"),
    ("多选同时修改", "对应属性值及 3D、运动模糊、隐藏、锁定等开关同步修改，一步撤销。"),
    ("取消选择图层", "时间轴或查看器空白处、F2、Cmd+Shift+A 清空选择；随后形状工具可新建形状图层。"),
    ("双击编辑形状", "选取工具双击查看器形状，进入路径编辑或自由变换。"),
    ("系统核对交互细节", "按 AE 核查选择模型、工具与修饰键、新建规则、数值编辑和键盘操作；列出差异，逐批修复并提交 PR。"),
)


def parity_tasks(number, title):
    checks = INTERACTIONS if number == 400 else [(title, "")]
    for index, (name, detail) in enumerate(checks, 1):
        prefix = f"ae-{number}-{index}" if number == 400 else f"ae-{number}"
        yield dict(name=name, detail=detail, fix=prefix + "-fix", verify=prefix + "-verify")


def save_issue(db, number, title, state, updated):
    db.execute("""insert into parity_issues values(?,?,?,?)
        on conflict(number) do update set title=excluded.title,state=excluded.state,
        updated=excluded.updated where parity_issues.title!=excluded.title
        or parity_issues.state!=excluded.state or parity_issues.updated!=excluded.updated""",
               (number, title, state, updated))
    for task in parity_tasks(number, title):
        for role, kind, module in (("fix", "milestone" if number == 400 else "repair", "交互对齐"),
                                   ("verify", "acceptance", "交互验收")):
            db.execute("""insert into items(id,kind,module,name,status,tag,prs,updated)
                values(?,?,?,?,'todo','','[]',?) on conflict(id) do nothing""",
                       (task[role], kind, module, task["name"], now()))


MODULES_400 = ("时间轴", "查看器工具", "数值编辑", "时间轴", "选择与快捷键", "查看器工具", "交互核查")
MODULE_RULES = (
    ("面板 / 工作区", r"面板|工作区|窗口菜单|panel|workspace"),
    ("数值编辑", r"数值|数字框|numeric|number field"),
    ("时间轴", r"时间轴|时间线|关键帧|timeline|keyframe"),
    ("查看器工具", r"查看器|钢笔|手柄|viewer|viewport"),
    ("效果", r"效果|effect"),
    ("文字", r"文字|文本|text"),
    ("音频", r"音频|audio"),
    ("媒体与导出", r"导入|导出|媒体|export|import"),
    ("选择与快捷键", r"快捷键|键盘|多选|选择|shortcut|keyboard"),
)


def issue_module(issue):
    labels = [x["name"] for x in issue.get("labels", [])]
    body = issue.get("body") or ""
    explicit = re.search(r"^(?:模块|module)\s*[:：]\s*(.{1,60})$", body, re.I | re.M)
    if explicit:
        return explicit[1].strip()
    for text in [*labels, issue["title"], body]:
        for name, pattern in MODULE_RULES:
            if re.search(pattern, text, re.I):
                return name
    return "其他交互"


def github_pages(endpoint):
    for page in range(1, 10001):
        separator = "&" if "?" in endpoint else "?"
        url = ("https://api.github.com/repos/FELIXDAD/betterAfterEffects/" +
               f"{endpoint}{separator}per_page=100&page={page}")
        with urllib.request.urlopen(url, timeout=15) as response:
            batch = json.load(response)
        yield from batch
        if len(batch) < 100:
            return


# Only advance these caches after fetching complete pages. A restart rebuilds them.
PR_CACHE = {}
TIMELINES = {}


def github_evidence(issues):
    cutoff = max((p["updated_at"] for p in PR_CACHE.values()), default="")
    updates = {}
    for pr in github_pages("pulls?state=all&sort=updated&direction=desc"):
        if pr["updated_at"] < cutoff:
            break
        updates[pr["number"]] = {key: pr.get(key) for key in
            ("number", "title", "body", "updated_at", "merged_at", "merge_commit_sha")}
    PR_CACHE.update(updates)
    for issue in issues:
        n = issue["number"]
        if n not in TIMELINES or TIMELINES[n][0] != issue["updated_at"]:
            events = list(github_pages(f"issues/{n}/timeline"))
            TIMELINES[n] = issue["updated_at"], events
    return PR_CACHE.values(), {n: entry[1] for n, entry in TIMELINES.items()}


def reconcile(db, issue, pulls, events):
    n = issue["number"]
    save_issue(db, n, issue["title"], issue["state"], issue["updated_at"])
    reopened = max((e["created_at"] for e in events if e["event"] == "reopened"), default="")
    closes = [e for e in events if e["event"] == "closed" and e["created_at"] >= reopened]
    closing_sha = closes[-1].get("commit_id") if closes else None
    closing_ref = (rf"\b(?:fix(?:es|ed)?|clos(?:e|es|ed)|resolv(?:e|es|ed))\s*:?\s*"
                   rf"(?:#|FELIXDAD/betterAfterEffects#|https://github.com/FELIXDAD/betterAfterEffects/issues/){n}(?![\w/-])")
    linked = set()
    for event in events:
        source = (event.get("source") or {}).get("issue") or {}
        if event["event"] in ("connected", "disconnected") and source.get("pull_request"):
            if source.get("repository_url", "").endswith("/FELIXDAD/betterAfterEffects"):
                if event["event"] == "connected":
                    linked.add(source["number"])
                else:
                    linked.discard(source["number"])
    notes = [issue.get("body") or ""] + [e.get("body") or "" for e in events if e["event"] == "commented"
        and e.get("author_association") in ("OWNER", "MEMBER", "COLLABORATOR")]
    merged_pulls = [p for p in pulls if p.get("merged_at")]
    for index, t in enumerate(parity_tasks(n, issue["title"])):
        module = MODULES_400[index] if n == 400 else issue_module(issue)
        db.execute("""update items set name=?,module=? where id in (?,?)
            and (name!=? or module!=?)""", (t["name"], module, t["fix"], t["verify"], t["name"], module))
        row = db.execute("select status,prs from items where id=?", (t["fix"],)).fetchone()
        refs = set(json.loads(row["prs"]))
        link_prefix = re.escape(t["fix"]) + r"\s+" if n == 400 else ""
        pr_reference = (r"(?im)^" + link_prefix + r"(?:修复\s*PR|Fix-PR)\s*[:：]\s*"
                        r"(?:#|https://github.com/FELIXDAD/betterAfterEffects/pull/)([1-9]\d*)\b")
        explicit = {int(value) for text in notes for value in re.findall(pr_reference, text)}
        matches = []
        for pr in merged_pulls:
            msg = (pr.get("title") or "") + "\n" + (pr.get("body") or "")
            scoped = re.search(r"(?im)^(?:fix(?:es|ed)?|close[sd]?|resolve[sd]?|board-item|修复)"
                               r"\s*[:：]?\s*`?" + re.escape(t["fix"]) + r"(?![\w-])", msg)
            if n == 400:
                scoped = scoped or re.search(rf"(?i)(?:fix(?:es|ed)?|修复)\s*#400\s*"
                                             rf"(?:第\s*|[/.:：-]\s*){index+1}(?!\d)", msg)
            closure = n != 400 and (re.search(closing_ref, msg, re.I) or
                                      (closing_sha and closing_sha == pr.get("merge_commit_sha")))
            if pr["number"] in refs | explicit or (n != 400 and pr["number"] in linked) or scoped or closure:
                matches.append(pr)
        merged = sorted(p["number"] for p in matches)
        # An aggregate issue closed by a PR still needs per-child evidence.
        manual_close = issue["state"] == "closed" and (n != 400 or not closing_sha)
        resolved = manual_close or any(p["merged_at"] > reopened for p in matches)
        status = "done" if resolved else "todo"
        prs = sorted(refs.union(merged))
        if row["status"] != status or refs != set(prs):
            db.execute("update items set status=?,prs=?,updated=? where id=?",
                       (status, json.dumps(prs), now(), t["fix"]))
        db.execute("""insert into parity_resolution values(?,?) on conflict(item) do update
            set merged=excluded.merged where merged!=excluded.merged""", (t["fix"], json.dumps(merged)))


def sync_parity():
    issues = [i for i in github_pages("issues?state=all&labels=ae-parity") if "pull_request" not in i]
    pulls, timelines = github_evidence(issues)
    with closing(connect()) as db, db:
        for issue in issues:
            reconcile(db, issue, pulls, timelines[issue["number"]])
        changed = db.total_changes
    if changed:
        notify_clients()


def parity_loop():
    while True:
        try:
            sync_parity()
        except (OSError, ValueError, KeyError, sqlite3.Error) as error:
            print(f"GitHub issue sync failed: {error}", file=sys.stderr, flush=True)
        time.sleep(300)


def notify_clients():
    global VERSION
    with CHANGE:
        VERSION += 1
        CHANGE.notify_all()


def register_legacy_effects(db):
    stamp = now()
    rows = []
    for line in (HERE / "legacy-effects.tsv").read_text(encoding="utf-8").splitlines():
        match, name, status, tag, pr, note = line.split("\t")
        rows.append(("fx:" + match, name, status, tag, json.dumps([int(pr)] if pr else []),
                     note, stamp))
    db.executemany("""insert into items(id,kind,module,name,status,tag,prs,note,updated)
        values(?,'effect','旧版效果',?,?,?,?,?,?) on conflict(id) do nothing""", rows)


def seed_board(db):
    rows = list(effects())
    for line in (HERE / "milestones.tsv").read_text().splitlines():
        module, item, name, status, *pr = line.split("\t")
        prs = [int(n) for n in pr[0].split(",")] if pr else []
        rows.append((item, "milestone", module, name, status, "", json.dumps(prs)))
    db.executemany("insert into items(id,kind,module,name,status,tag,prs,updated) "
                   "values(?,?,?,?,?,?,?,?)", [(*row, now()) for row in rows])


def import_github():
    command = ["gh", "pr", "list", "--repo", "FELIXDAD/betterAfterEffects", "--state", "all",
               "--limit", "10000", "--json", "number,title,state,headRefName,updatedAt"]
    pulls = json.loads(subprocess.check_output(command, timeout=90))
    with closing(connect()) as db, db:
        db.execute("begin immediate")
        owners = {n: row["id"] for row in db.execute("select id,prs from items")
                  for n in json.loads(row["prs"])}
        for pr in pulls:
            if pr["state"] not in ("OPEN", "MERGED"):
                continue
            number = pr["number"]
            item = owners.get(number)
            branch = pr["headRefName"]
            if not item and branch.startswith(("effects/knb-", "effects/dcs-", "effects/generate-text-transition-")):
                item = "fx-a"
            if item and number not in owners:
                row = db.execute("select prs from items where id=?", (item,)).fetchone()
                db.execute("update items set prs=? where id=?", (json.dumps(json.loads(row[0])+[number]), item))
            db.execute("insert or replace into pulls values(?,?,?,?,?)",
                       (number, item, pr["title"], pr["state"], pr["updatedAt"]))


def snapshot():
    with closing(connect()) as db, db:
        db.execute("begin")
        items = [dict(row) for row in db.execute("select * from items order by rowid")]
        for item in items:
            item["prs"] = json.loads(item["prs"])
        agents = [dict(row) for row in db.execute(
            "select a.* from activity a join (select max(seq) seq from activity group by agent) b using(seq)")]
        feed = [dict(row) for row in db.execute("""
            select seq,item,agent,status,note,updated,null as pr from activity
            union all select 0,'','GitHub',state,title,updated,number from pulls where item is null
            order by updated desc, seq desc""")]
        issues = [dict(row) for row in db.execute("select * from parity_issues order by number")]
        merged = {row[0]: json.loads(row[1]) for row in db.execute("select item,merged from parity_resolution")}
        for issue in issues:
            issue["tasks"] = list(parity_tasks(issue["number"], issue["title"]))
            for task in issue["tasks"]:
                task["merged"] = merged.get(task["fix"], [])
        return dict(items=items, agents=agents, activity=feed, parity=issues)


def field(data, key, limit, required=False):
    value = data.get(key, "")
    if not isinstance(value, str) or len(value) > limit or (required and not value.strip()):
        raise ValueError("无效字段：" + key)
    return value.strip()


def report(data):
    if not isinstance(data, dict):
        raise ValueError("需要 JSON 对象")
    item = field(data, "item", 180, True)
    agent = field(data, "agent", 120, True)
    status = field(data, "status", 8, True)
    note = field(data, "note", 2000)
    name = field(data, "name", 160) or item
    module = field(data, "module", 80) or "新增能力"
    pr = str(data.get("pr", "")).lstrip("#")
    if status not in ("done", "doing", "todo") or (pr and not re.fullmatch("[1-9][0-9]{0,8}", pr)):
        raise ValueError("状态或 PR 编号无效")
    stamp = now()
    with closing(connect()) as db, db:
        db.execute("begin immediate")
        row = db.execute("select * from items where id=?", (item,)).fetchone()
        if row and re.fullmatch(r"ae-\d+(?:-\d+)?-fix", item):
            status = row["status"]
        if not row:
            if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,119}", item):
                raise ValueError("新增能力 ID 仅支持小写字母、数字、点、下划线和连字符")
            kind = "tool" if item.startswith("board-") else "milestone"
            db.execute("insert into items(id,name,module,kind,tag,prs) values(?,?,?,?,?,?)",
                       (item, name, module, kind, "", "[]"))
        prs = json.loads(row["prs"]) if row else []
        if pr and int(pr) not in prs:
            prs.append(int(pr))
        elif row and (row["status"], row["agent"], row["note"]) == (status, agent, note):
            return False
        db.execute("update items set status=?,agent=?,note=?,updated=?,prs=? where id=?",
                   (status, agent, note, stamp, json.dumps(prs), item))
        db.execute("insert into activity(item,agent,status,note,updated) values(?,?,?,?,?)",
                   (item, agent, status, note, stamp))
    return True


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(30)

    def headers_for(self, status, mime, length=None):
        self.send_response(status)
        self.send_header("Content-Type", mime)
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        if length is not None:
            self.send_header("Content-Length", str(length))
        self.end_headers()

    def authorized(self, expected):
        if hmac.compare_digest(self.headers.get("Authorization", "").encode(), expected.encode()):
            return True
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="betterAfterEffects", charset="UTF-8"'
                         if expected == BASIC else "Bearer")
        self.end_headers()
        return False

    def reply(self, data, status=200):
        raw = json.dumps(data, ensure_ascii=False).encode()
        self.headers_for(status, "application/json; charset=utf-8", len(raw))
        self.wfile.write(raw)

    def do_GET(self):
        if not self.authorized(BASIC):
            return
        if self.path == "/api/state":
            self.reply(snapshot())
        elif self.path == "/":
            body = (HERE / "index.html").read_bytes()
            self.headers_for(200, "text/html; charset=utf-8", len(body))
            self.wfile.write(body)
        elif self.path == "/api/events":
            self.headers_for(200, "text/event-stream")
            last = -1
            try:
                while True:
                    with CHANGE:
                        CHANGE.wait_for(lambda: VERSION != last, timeout=15)
                        version = VERSION
                    packet = ": heartbeat\n\n" if version == last else f"data: {version}\n\n"
                    self.wfile.write(packet.encode())
                    self.wfile.flush()
                    last = version
            except (ConnectionError, TimeoutError, OSError):
                pass
        else:
            self.send_error(404)

    def do_POST(self):
        if not self.authorized(BEARER):
            return
        if self.path != "/api/report":
            self.send_error(404)
            return
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 16384:
                raise ValueError("请求长度无效")
            changed = report(json.loads(self.rfile.read(size)))
        except (ValueError, RecursionError) as error:
            self.reply({"error": str(error)}, 400)
            return
        if changed:
            notify_clients()
        self.reply({"changed": changed})


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8787)
    parser.add_argument("--github", action="store_true", help="启动前导入开放和已合并的 PR")
    args = parser.parse_args()
    PORT = args.port
    os.umask(0o077)
    BASIC, BEARER = credentials()
    initialize()
    if args.github:
        import_github()
    threading.Thread(target=parity_loop, daemon=True).start()
    print(f"http://{args.host}:{PORT}", flush=True)
    ThreadingHTTPServer((args.host, PORT), Handler).serve_forever()
