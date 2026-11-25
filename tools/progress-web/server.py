"""Local and systemd entry point for the betterAfterEffects progress board."""
import argparse
import base64
from contextlib import closing
from datetime import datetime, timedelta, timezone
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

HERE = Path(__file__).resolve().parent
BOARD = json.loads((HERE / "board.json").read_text(encoding="utf-8"))
ISSUE_GROUPS = {int(number): group for number, group in BOARD["issues"].items()}
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
    config = dict(line.split("=", 1) for line in secret.read_text().splitlines() if line.strip())
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
        db.execute("create table if not exists github_prs(number integer primary key,status text,updated text)")
        db.execute("""create table if not exists pr_activity(number integer primary key,
            queued integer not null, committed_at text, author text)""")
        db.execute("""create table if not exists doing_ping(item text primary key,
            agent text not null, updated text not null)""")
        db.execute("""create table if not exists github_links(
            item text, number integer, primary key(item,number))""")
        db.execute("begin immediate")
        if not db.execute("select 1 from items limit 1").fetchone():
            db.executemany("insert into items(id,kind,module,name,status,tag,prs,updated) "
                           "values(?,?,?,?,?,?,?,?)", [(*row, now()) for row in effects()])
        register_milestones(db)
        register_legacy_effects(db)
        db.execute("""create table if not exists parity_issues(
            number integer primary key, title text not null, state text not null, updated text not null)""")
        db.execute("""create table if not exists parity_resolution(
            item text primary key, merged text not null)""")
        for number, group in ISSUE_GROUPS.items():
            row = db.execute("select * from parity_issues where number=?", (number,)).fetchone()
            if row:
                save_issue(db, number, row["title"], row["state"], row["updated"])
            else:
                save_issue(db, number, group["title"], "open", "")


def parity_tasks(number, title):
    group = ISSUE_GROUPS.get(number)
    checks = group["tasks"] if group else [dict(name=title, detail="")]
    for check in checks:
        prefix = f"ae-{number}"
        if group:
            prefix += f"-{check['slot']}"
        yield dict(check, fix=prefix + "-fix", verify=prefix + "-verify")


def save_issue(db, number, title, state, updated):
    db.execute("""insert into parity_issues values(?,?,?,?)
        on conflict(number) do update set title=excluded.title,state=excluded.state,
        updated=excluded.updated where parity_issues.title!=excluded.title
        or parity_issues.state!=excluded.state or parity_issues.updated!=excluded.updated""",
               (number, title, state, updated))
    for task in parity_tasks(number, title):
        for role, kind, module in (("fix", "milestone" if number in ISSUE_GROUPS else "repair", "交互对齐"),
                                   ("verify", "acceptance", "交互验收")):
            db.execute("""insert into items(id,kind,module,name,status,tag,prs,updated)
                values(?,?,?,?,'todo','','[]',?) on conflict(id) do nothing""",
                       (task[role], kind, task.get("module", module), task["name"], now()))


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
    for t in parity_tasks(n, issue["title"]):
        module = t.get("module") or issue_module(issue)
        db.execute("""update items set name=?,module=? where id in (?,?)
            and (name!=? or module!=?)""", (t["name"], module, t["fix"], t["verify"], t["name"], module))
        row = db.execute("select status,prs from items where id=?", (t["fix"],)).fetchone()
        refs = set(json.loads(row["prs"]))
        link_prefix = re.escape(t["fix"]) + r"\s+" if n in ISSUE_GROUPS else ""
        pr_reference = (r"(?im)^" + link_prefix + r"(?:修复\s*PR|Fix-PR)\s*[:：]\s*"
                        r"(?:#|https://github.com/FELIXDAD/betterAfterEffects/pull/)([1-9]\d*)\b")
        explicit = {int(value) for text in notes for value in re.findall(pr_reference, text)}
        matches = []
        for pr in merged_pulls:
            msg = (pr.get("title") or "") + "\n" + (pr.get("body") or "")
            scoped = re.search(r"(?im)^(?:fix(?:es|ed)?|close[sd]?|resolve[sd]?|board-item|修复)"
                               r"\s*[:：]?\s*`?" + re.escape(t["fix"]) + r"(?![\w-])", msg)
            if n in ISSUE_GROUPS and t["fix"] in issue_links(pr, {n: issue}, {}):
                scoped = True
            if n in ISSUE_GROUPS:
                scoped = scoped or re.search(rf"(?i)(?:fix(?:es|ed)?|修复)\s*#{n}\s*"
                                             rf"(?:第\s*|[/.:：-]\s*){t['slot']}(?!\d)", msg)
            closure = n not in ISSUE_GROUPS and (re.search(closing_ref, msg, re.I) or
                                      (closing_sha and closing_sha == pr.get("merge_commit_sha")))
            if pr["number"] in refs | explicit or (n not in ISSUE_GROUPS and pr["number"] in linked) or scoped or closure:
                matches.append(pr)
        merged = sorted(p["number"] for p in matches)
        # An aggregate issue closed by a PR still needs per-child evidence.
        manual_close = issue["state"] == "closed" and (n not in ISSUE_GROUPS or not closing_sha)
        resolved = manual_close or any(p["merged_at"] > reopened for p in matches)
        status = "done" if resolved else "todo"
        prs = sorted(refs.union(merged))
        if row["status"] != status or refs != set(prs):
            db.execute("update items set status=?,prs=?,updated=? where id=?",
                       (status, json.dumps(prs), now(), t["fix"]))
        db.execute("""insert into parity_resolution values(?,?) on conflict(item) do update
            set merged=excluded.merged where merged!=excluded.merged""", (t["fix"], json.dumps(merged)))


def milestone_status(prs, states, recent=False):
    if prs and all(states.get(n) == "done" for n in prs):
        return "done"
    if recent or any(states.get(n) == "doing" for n in prs):
        return "doing"
    if any(states.get(n) == "ready" for n in prs):
        return "ready"
    return "todo"


CLOSES = re.compile(r"\b(?:fix(?:es|ed)?|clos(?:e|es|ed)|resolv(?:e|es|ed))\s*:?\s*"
                    r"(?:#|FELIXDAD/betterAfterEffects#|https://github.com/FELIXDAD/betterAfterEffects/issues/)(\d+)\b", re.I)


def issue_links(pr, issues, milestones):
    text = pr.get("body") or ""
    for match in CLOSES.finditer(text):
        number = int(match[1])
        issue = issues.get(number)
        if not issue:
            continue
        if number in ISSUE_GROUPS:
            # Aggregate references need an explicit child, e.g. Fixes #400（第 6 项）.
            tail = text[match.end():].split("\n", 1)[0]
            scope = re.match(r"\s*(?:[（(]([^）)]*)|(?:第|[/.:：-])\s*(\d+))", tail)
            if scope:
                slots = re.findall(r"第\s*(\d+)\s*项", scope[1] or "") or [scope[2]]
                for t in parity_tasks(number, issue["title"]):
                    if str(t["slot"]) in slots:
                        yield t["fix"]
            continue
        module = issue_module(issue)
        if issue["parity"]:
            yield f"ae-{number}-fix"
        explicit = re.search(r"(?im)^(?:能力\s*ID|board-item)\s*[:：]\s*([\w.-]+)", issue.get("body",""))
        if explicit and explicit[1] in milestones:
            yield explicit[1]
        else:
            targets = [item for item, area in milestones.items() if area == module]
            target = BOARD.get("module_items", {}).get(module)
            if target in milestones:
                yield target
            elif len(targets) == 1:
                yield targets[0]
    for item in re.findall(r"(?im)^(?:fixes|board-item|修复)\s*[:：]?\s*`?(ae-\d+-\d+-fix)\b", text):
        if item in milestones:
            yield item


def apply_github(data):
    pulls = data["pulls"]
    issues = {i["number"]: i for i in data["issues"]}
    with closing(connect()) as db, db:
        db.execute("begin immediate")
        for pr in pulls:
            if type(pr["number"]) is not int or pr["status"] not in ("closed","done","ready","doing"):
                raise ValueError("无效的 PR 状态")
            db.execute("""insert into github_prs values(?,?,?) on conflict(number) do update
                set status=excluded.status,updated=excluded.updated
                where status!=excluded.status or updated!=excluded.updated""",
                       (pr["number"], pr["status"], pr["updated_at"]))
            if "queued" in pr:
                db.execute("""insert into pr_activity values(?,?,?,?) on conflict(number) do update
                    set queued=excluded.queued,committed_at=excluded.committed_at,author=excluded.author
                    where queued!=excluded.queued or committed_at is not excluded.committed_at
                    or author is not excluded.author""",
                           (pr["number"], bool(pr["queued"]), pr.get("committed_at"), pr.get("commit_author")))
        for issue in issues.values():
            if issue["parity"]:
                reconcile(db, issue, pulls, issue["events"])
        milestones = dict(db.execute("select id,module from items where kind='milestone'"))
        links = {(item, p["number"]) for p in pulls for item in issue_links(p, issues, milestones)}
        previous = {tuple(r) for r in db.execute("select item,number from github_links")}
        db.executemany("insert into github_links values(?,?)", links - previous)
        db.executemany("delete from github_links where item=? and number=?", previous - links)
        return bool(db.total_changes)


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


def register_milestones(db):
    rows = []
    for line in (HERE / "milestones.tsv").read_text(encoding="utf-8").splitlines():
        module, item, name, status, *extra = line.split("\t")
        pr, agent = (extra + ["", ""])[:2]
        prs = [int(n) for n in pr.split(",")] if pr else []
        rows.append((item, module, name, status, json.dumps(prs), agent, now()))
    db.executemany("""insert into items(id,kind,module,name,status,tag,prs,agent,updated)
        values(?,'milestone',?,?,?,'',?,?,?) on conflict(id) do nothing""", rows)


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
        pr_data = list(db.execute("select * from github_prs"))
        states = {r["number"]: r["status"] for r in pr_data}
        updated = {r["number"]: r["updated"] for r in pr_data}
        links = {}
        for link in db.execute("select * from github_links"):
            links.setdefault(link["item"], set()).add(link["number"])
        clock = datetime.now(timezone.utc)
        cutoff = (clock - timedelta(hours=2)).timestamp()
        commits = {}
        queue = 0
        for r in db.execute("select * from pr_activity"):
            queue += int(r["queued"] and states.get(r["number"]) in ("ready","doing"))
            if r["committed_at"]:
                epoch = datetime.fromisoformat(r["committed_at"].replace("Z","+00:00")).timestamp()
                commits[r["number"]] = (epoch, r["committed_at"], r["author"], "commit")
        reports = {}
        for r in db.execute("""select a.item,a.updated,a.agent from activity a join
                (select max(seq) seq from activity group by item) latest using(seq)
                union all select item,updated,agent from doing_ping"""):
            epoch = datetime.fromisoformat(r["updated"]).timestamp()
            if r["item"] not in reports or epoch >= reports[r["item"]][0]:
                reports[r["item"]] = (epoch, r["updated"], r["agent"], "report")
        doing = dict(db.execute("""select item,max(updated) from (
            select item,updated from activity where status='doing'
            union all select item,updated from doing_ping) group by item"""))
        for item in items:
            item["prs"] = sorted(set(json.loads(item["prs"])) | links.get(item["id"], set()))
            if item["kind"] == "milestone":
                item["repair_status"] = item["status"]
                history = [commits[n] for n in item["prs"] if n in commits]
                recent = any(cutoff < entry[0] <= clock.timestamp() for entry in history)
                if item["id"] in doing:
                    recent |= cutoff < datetime.fromisoformat(doing[item["id"]]).timestamp() <= clock.timestamp()
                if item["id"] in reports:
                    history.append(reports[item["id"]])
                latest = max(history, key=lambda e:e[0]) if history else None
                item["last_activity"] = dict(time=latest[1], agent=latest[2], source=latest[3]) if latest else None
                item["pr_counts"] = {key: sum(states.get(n)==key for n in item["prs"])
                                     for key in ("done","ready","doing")}
                item["status"] = milestone_status(item["prs"], states, recent)
                item["updated"] = max([item["updated"]] + [updated[n] for n in item["prs"] if n in updated])
        agents = [dict(row) for row in db.execute(
            "select a.* from activity a join (select max(seq) seq from activity group by agent) b using(seq)")]
        feed = [dict(row) for row in db.execute("""
            select seq,item,agent,status,note,updated,null as pr from activity
            union all select 0,'','GitHub',state,title,updated,number from pulls where item is null
            order by updated desc, seq desc, pr desc limit 200""")]
        issues = [dict(row) for row in db.execute("select * from parity_issues order by number")]
        merged = {row[0]: json.loads(row[1]) for row in db.execute("select item,merged from parity_resolution")}
        for issue in issues:
            issue["tasks"] = list(parity_tasks(issue["number"], issue["title"]))
            for task in issue["tasks"]:
                task["merged"] = merged.get(task["fix"], [])
        return dict(milestone=BOARD["milestone"], merge_queue=queue, items=items, agents=agents, activity=feed, parity=issues)


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
        reported_status = status
        ping_changed = False
        if status == "doing" and (not row or row["kind"] == "milestone"):
            ping_changed = bool(db.execute("""insert into doing_ping values(?,?,?) on conflict(item)
                do update set agent=excluded.agent,updated=excluded.updated
                where agent!=excluded.agent or updated!=excluded.updated""",
                (item, agent, stamp)).rowcount)
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
            return ping_changed
        db.execute("update items set status=?,agent=?,note=?,updated=?,prs=? where id=?",
                   (status, agent, note, stamp, json.dumps(prs), item))
        db.execute("insert into activity(item,agent,status,note,updated) values(?,?,?,?,?)",
                   (item, agent, reported_status, note, stamp))
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
            if not 0 < size <= 2 * 1024 * 1024:
                raise ValueError("请求长度无效")
            data = json.loads(self.rfile.read(size))
            if isinstance(data, dict) and data.get("type") == "github-sync":
                changed = apply_github(data)
            elif size <= 16384:
                changed = report(data)
            else:
                raise ValueError("汇报过长")
        except (ValueError, KeyError, TypeError, RecursionError) as error:
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
    print(f"http://{args.host}:{PORT}", flush=True)
    ThreadingHTTPServer((args.host, PORT), Handler).serve_forever()
