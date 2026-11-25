"""Push one consistent GitHub snapshot using the Mac's gh login."""
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request

REPO = "FELIXDAD/betterAfterEffects"
CONFIG = Path.home() / ".config/bae-board/env"
CACHE = Path.home() / "Library/Caches/bae-progress-sync/timelines.json"


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


def pull_records():
    query = """query($endCursor:String) {
      repository(owner:"FELIXDAD",name:"betterAfterEffects") {
        pullRequests(first:50,after:$endCursor) {
          pageInfo { hasNextPage endCursor }
          nodes { number state isDraft title body mergedAt updatedAt mergeCommit { oid }
            labels(first:100) { nodes { name } }
            commits(last:1) { nodes { commit { committedDate author { name user { login } } } } }
          }
        }
      }
    }"""
    raw = subprocess.check_output(["gh","api","graphql","--paginate","--slurp","-f","query="+query],
                                  timeout=180)
    for page in json.loads(raw):
        for pr in page["data"]["repository"]["pullRequests"]["nodes"]:
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


def collect():
    pulls = list(pull_records())
    issues = []
    cache = json.loads(CACHE.read_text()) if CACHE.exists() else {}
    for issue in pages("issues?state=all&per_page=100"):
        if "pull_request" in issue:
            continue
        parity = any(x["name"] == "ae-parity" for x in issue["labels"])
        row = {k:issue[k] for k in ("number","title","state","updated_at")}
        row.update(body=evidence(issue["body"]), parity=parity,
                   labels=[{"name":x["name"]} for x in issue["labels"]])
        if parity:
            key = str(issue["number"])
            if cache.get(key, {}).get("updated") != issue["updated_at"]:
                cache[key] = dict(updated=issue["updated_at"], events=timeline(issue["number"]))
            row["events"] = cache[key]["events"]
        issues.append(row)
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CACHE.with_suffix(f".{os.getpid()}.tmp")
    tmp.write_text(json.dumps(cache))
    tmp.replace(CACHE)
    return dict(type="github-sync", pulls=pulls, issues=issues)


def push(url, token, data):
    req = urllib.request.Request(url.rstrip("/") + "/api/report", data,
        {"Authorization":"Bearer " + token, "Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=90) as response:
        print(response.read().decode(), flush=True)


if __name__ == "__main__":
    env = dict(line.split("=",1) for line in CONFIG.read_text().splitlines() if "=" in line)
    data = json.dumps(collect(), ensure_ascii=False, separators=(",",":")).encode()
    push(env["BAE_BOARD_URL"], env["BAE_BOARD_TOKEN"], data)
    if os.environ.get("BAE_BOARD_LOCAL_MIRROR"):
        push(os.environ["BAE_BOARD_LOCAL_MIRROR"], env["BAE_BOARD_TOKEN"], data)
