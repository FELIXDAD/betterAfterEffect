"""Push GitHub PRs and issues updated since the last sync, using the Mac's gh login."""
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request

REPO = "FELIXDAD/betterAfterEffects"
CONFIG = Path.home() / ".config/bae-board/env"
# Last sync time per board URL. Delete the file to force a full resync.
STATE = Path.home() / "Library/Caches/bae-progress-sync/watermarks.json"
BATCH = 200
PULLS = """query($endCursor:String) {
  repository(owner:"FELIXDAD",name:"betterAfterEffects") {
    pullRequests(first:50,after:$endCursor,orderBy:{field:UPDATED_AT,direction:DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes { number state isDraft title body mergedAt updatedAt mergeCommit { oid }
        labels(first:100) { nodes { name } }
        commits(last:1) { nodes { commit { committedDate author { name user { login } } } } }
      }
    }
  }
}"""


def pages(endpoint):
    raw = subprocess.check_output(["gh", "api", "--paginate", "--slurp",
                                   f"repos/{REPO}/{endpoint}"], timeout=180)
    return [row for page in json.loads(raw) for row in page]


def evidence(text):
    return "\n".join(line for line in (text or "").splitlines()
                     if re.search(r"(?i)fix|clos|resolv|board-item|修复|^模块|^module|^能力\s*ID", line))


def timeline(number):
    result = []
    for e in pages(f"issues/{number}/timeline?per_page=100"):
        kind = e["event"]
        row = {k:e[k] for k in ("event","created_at","commit_id","author_association") if k in e}
        if kind == "commented":
            row["body"] = evidence(e.get("body"))
            if not row["body"]:
                continue
        elif kind in ("connected", "disconnected"):
            src = (e.get("source") or {}).get("issue") or {}
            row["source"] = {"issue": {k:src[k] for k in ("number","repository_url") if k in src}}
            row["source"]["issue"]["pull_request"] = bool(src.get("pull_request"))
        elif kind not in ("closed","reopened"):
            continue
        result.append(row)
    return result


def pull_records(since):
    cursor = None
    while True:
        args = ["gh", "api", "graphql", "-f", "query=" + PULLS]
        if cursor:
            args += ["-f", "endCursor=" + cursor]
        page = json.loads(subprocess.check_output(args, timeout=180))["data"]["repository"]["pullRequests"]
        for pr in page["nodes"]:
            if since and pr["updatedAt"] < since:
                return
            opened = pr["state"] == "OPEN"
            labels = {x["name"].lower() for x in pr["labels"]["nodes"]}
            state = {"MERGED":"done", "CLOSED":"closed", "OPEN":"doing" if pr["isDraft"] else "ready"}[pr["state"]]
            commits = pr["commits"]["nodes"]
            commit = commits[-1]["commit"] if commits else {}
            author = commit.get("author") or {}
            yield dict(number=pr["number"], status=state, title=pr["title"], body=evidence(pr["body"]),
                       merged_at=pr["mergedAt"], updated_at=pr["updatedAt"],
                       merge_commit_sha=(pr["mergeCommit"] or {}).get("oid"),
                       queued=opened and "merge-ready" in labels,
                       committed_at=commit.get("committedDate"),
                       commit_author=(author.get("user") or {}).get("login") or author.get("name"))
        if not page["pageInfo"]["hasNextPage"]:
            return
        cursor = page["pageInfo"]["endCursor"]


def collect(since):
    pulls = list(pull_records(since))
    issues = []
    for issue in pages("issues?state=all&per_page=100" + (f"&since={since}" if since else "")):
        if "pull_request" in issue:
            continue
        parity = any(x["name"] == "ae-parity" for x in issue["labels"])
        row = {k:issue[k] for k in ("number","title","state","updated_at")}
        row.update(body=evidence(issue["body"]), parity=parity,
                   labels=[{"name":x["name"]} for x in issue["labels"]])
        if parity:
            row["events"] = timeline(issue["number"])
        issues.append(row)
    return pulls, issues


def batches(pulls, issues):
    # The server merges partial updates. Pulls go first so a fresh server holds every PR
    # before issues are reconciled, and each request stays well under its size limit.
    rows = [("pulls", row) for row in pulls] + [("issues", row) for row in issues]
    for start in range(0, len(rows), BATCH):
        part = dict(type="github-sync", pulls=[], issues=[])
        for kind, row in rows[start:start + BATCH]:
            part[kind].append(row)
        yield json.dumps(part, ensure_ascii=False, separators=(",",":")).encode()


def push(url, token, data):
    req = urllib.request.Request(url.rstrip("/") + "/api/report", data,
        {"Authorization":"Bearer " + token, "Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=90) as response:
        print(response.read().decode(), flush=True)


if __name__ == "__main__":
    env = dict(line.split("=",1) for line in CONFIG.read_text().splitlines() if "=" in line)
    targets = [env["BAE_BOARD_URL"]]
    if os.environ.get("BAE_BOARD_LOCAL_MIRROR"):
        targets.append(os.environ["BAE_BOARD_LOCAL_MIRROR"])
    marks = json.loads(STATE.read_text()) if STATE.exists() else {}
    # Overlap windows so clock skew and edits made while this run fetches are not missed.
    started = (datetime.now(timezone.utc) - timedelta(minutes=5)).strftime("%Y-%m-%dT%H:%M:%SZ")
    fetched = {}
    for url in targets:
        since = marks.get(url)
        if since not in fetched:
            fetched[since] = collect(since)
        for data in batches(*fetched[since]):
            push(url, env["BAE_BOARD_TOKEN"], data)
        marks[url] = started
        STATE.parent.mkdir(parents=True, exist_ok=True)
        tmp = STATE.with_suffix(f".{os.getpid()}.tmp")
        tmp.write_text(json.dumps(marks))
        tmp.replace(STATE)
