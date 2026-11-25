#!/usr/bin/env python3
"""BAE merge train and progress board.

Runs under launchd on the coordinator Mac, outside any AI session. Leaders decide WHAT
merges by adding the `merge-ready` label; this script only executes, one PR at a time in
label order. See README.md (Chinese) for install, pause/resume and logs.

Subcommands:
  run      merge-train daemon (or one pass with --once)
  board    rewrite the pinned progress-board issue once
  setup    create the labels the train uses
  pause    pause the queue (pause file + label on the board issue)
  resume   resume the queue
  status   print the queue and its current decisions

Only the Python 3.9 standard library is used (macOS system python3).
"""

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
HANDOFF_START = "<!-- handoff:start -->"
HANDOFF_END = "<!-- handoff:end -->"
BOARD_MARKER = "<!-- bae-progress-board -->"

# --------------------------------------------------------------------------- TOML subset


class TomlError(ValueError):
    pass


def _toml_value(s, i):
    """Parse one TOML value at s[i:]; return (value, next_index)."""
    while i < len(s) and s[i] in " \t\r\n":
        i += 1
    if i >= len(s):
        raise TomlError("missing value")
    c = s[i]
    if c == '"':
        out, i = [], i + 1
        esc = {"n": "\n", "t": "\t", '"': '"', "\\": "\\", "r": "\r"}
        while i < len(s) and s[i] != '"':
            if s[i] == "\\":
                i += 1
                if s[i] == "u":
                    out.append(chr(int(s[i + 1:i + 5], 16)))
                    i += 5
                    continue
                out.append(esc.get(s[i], s[i]))
            else:
                out.append(s[i])
            i += 1
        if i >= len(s):
            raise TomlError("unterminated string")
        return "".join(out), i + 1
    if c == "'":
        j = s.index("'", i + 1)
        return s[i + 1:j], j + 1
    if c == "[":
        arr, i = [], i + 1
        while True:
            while i < len(s) and (s[i] in " \t\r\n," or s[i] == "#"):
                if s[i] == "#":
                    while i < len(s) and s[i] != "\n":
                        i += 1
                else:
                    i += 1
            if i >= len(s):
                raise TomlError("unterminated array")
            if s[i] == "]":
                return arr, i + 1
            v, i = _toml_value(s, i)
            arr.append(v)
    m = re.compile(r"true|false|[-+]?\d+(\.\d+)?").match(s, i)
    if not m:
        raise TomlError("bad value near %r" % s[i:i + 20])
    t = m.group(0)
    if t in ("true", "false"):
        return t == "true", m.end()
    return (float(t) if "." in t else int(t)), m.end()


def _toml_key(k):
    k = k.strip()
    if len(k) >= 2 and k[0] == k[-1] and k[0] in "\"'":
        return k[1:-1]
    return k


def toml_loads(text):
    """Minimal TOML: tables, arrays of tables, strings, ints, floats, bools, arrays."""
    root = {}
    cur = root
    i, n = 0, len(text)
    while i < n:
        j = text.find("\n", i)
        j = n if j < 0 else j
        line = text[i:j].strip()
        if not line or line.startswith("#"):
            i = j + 1
            continue
        if line.startswith("[["):
            path = [_toml_key(p) for p in line[2:line.index("]]")].split(".")]
            tgt = root
            for p in path[:-1]:
                tgt = tgt.setdefault(p, {})
            cur = {}
            tgt.setdefault(path[-1], []).append(cur)
            i = j + 1
            continue
        if line.startswith("["):
            path = [_toml_key(p) for p in line[1:line.index("]")].split(".")]
            cur = root
            for p in path:
                cur = cur.setdefault(p, {})
            i = j + 1
            continue
        eq = text.index("=", i)
        key = _toml_key(text[i:eq])
        val, k = _toml_value(text, eq + 1)
        cur[key] = val
        rest = text.find("\n", k)
        rest = n if rest < 0 else rest
        tail = text[k:rest].strip()
        if tail and not tail.startswith("#"):
            raise TomlError("trailing text after %s: %r" % (key, tail))
        i = rest + 1
    return root


def load_toml(path):
    with open(path, encoding="utf-8") as f:
        return toml_loads(f.read())


# --------------------------------------------------------------------------- utilities

ARGS = None
CFG = {}
LEADERS = {}


def log(msg, tag="train"):
    stamp = dt.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print("%s [%s] %s" % (stamp, tag, msg), flush=True)


def expand(p):
    return os.path.abspath(os.path.expanduser(p))


def now_utc():
    return dt.datetime.now(dt.timezone.utc)


def parse_ts(s):
    if not s:
        return None
    return dt.datetime.strptime(s.replace("Z", "+0000"), "%Y-%m-%dT%H:%M:%S%z")


def local_hm(ts):
    if ts is None:
        return "—"
    return ts.astimezone().strftime("%m-%d %H:%M")


def sh(cmd, cwd=None, check=True, env=None, input_text=None):
    """Run a command; return stdout (str). Raise RuntimeError on failure when check."""
    p = subprocess.run(cmd, cwd=cwd, env=env, input=input_text, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, universal_newlines=True)
    if check and p.returncode != 0:
        raise RuntimeError("%s failed (%d): %s" % (" ".join(cmd[:4]), p.returncode,
                                                    (p.stderr or p.stdout).strip()[:500]))
    return p.stdout if check else (p.returncode, p.stdout, p.stderr)


def gh_json(args):
    return json.loads(sh(["gh"] + args) or "null")


def gql(query, **variables):
    cmd = ["gh", "api", "graphql", "-f", "query=" + query]
    for k, v in variables.items():
        cmd += ["-F" if isinstance(v, int) else "-f", "%s=%s" % (k, v)]
    out = json.loads(sh(cmd))
    if out.get("errors"):
        raise RuntimeError("graphql: %s" % out["errors"])
    return out["data"]


def repo_owner_name():
    owner, name = CFG["repo"].split("/")
    return owner, name


def state_dir():
    d = expand(CFG["state_dir"])
    os.makedirs(d, exist_ok=True)
    return d


def load_state():
    try:
        with open(os.path.join(state_dir(), "state.json"), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_state(st):
    path = os.path.join(state_dir(), "state.json")
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(st, f, ensure_ascii=False, indent=1, sort_keys=True)
    os.replace(tmp, path)


def pause_file():
    return os.path.join(state_dir(), "PAUSED")


def auto_pause_file():
    """Exists while the queue is paused by a red dev commit (holds that commit). A manual
    pause (`pause`, or the label put on by a person) has no such marker."""
    return os.path.join(state_dir(), "AUTO_PAUSED")


def auto_paused_sha():
    try:
        with open(auto_pause_file(), encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


def clear_auto_pause():
    if os.path.exists(auto_pause_file()):
        os.remove(auto_pause_file())


def red_ack_get():
    """dev commit whose red CI was already handled (paused once, or resumed past)."""
    try:
        with open(os.path.join(state_dir(), "dev-red-ack"), encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


def red_ack_set(sha):
    with open(os.path.join(state_dir(), "dev-red-ack"), "w", encoding="utf-8") as f:
        f.write(sha + "\n")


def mutate(desc):
    """True when a side effect may run; in dry-run, log what would happen instead."""
    if ARGS.dry_run:
        log("DRY-RUN would: " + desc)
        return False
    return True


def repo_git_dir():
    """A git checkout to run fetch/diff in: the runner clone, else this checkout."""
    d = expand(CFG["runner_dir"])
    return d if os.path.isdir(os.path.join(d, ".git")) or os.path.isfile(
        os.path.join(d, ".git")) else os.path.dirname(os.path.dirname(HERE))


def git(*args, cwd=None, check=True):
    return sh(["git"] + list(args), cwd=cwd or repo_git_dir(), check=check)


# --------------------------------------------------------------------------- orca


def orca_worktrees():
    """All BAE worktrees on this host and the paired hosts: list of dicts with
    host, name, branch, comment, status, agents (newest last)."""
    hosts = [("local", [])] + [(e, ["--environment", e]) for e in CFG.get("orca_environments", [])]
    out = []
    for host, extra in hosts:
        try:
            data = json.loads(sh(["orca", "worktree", "ps", "--json"] + extra))
        except (RuntimeError, ValueError, OSError) as e:
            log("orca worktree ps on %s failed: %s" % (host, e), "orca")
            continue
        for w in data.get("result", {}).get("worktrees", []):
            if w.get("repo") != CFG.get("orca_repo") or w.get("isArchived"):
                continue
            agents = sorted(w.get("agents") or [], key=lambda a: a.get("updatedAt") or 0)
            out.append({
                "host": host,
                "name": w.get("displayName") or os.path.basename(w.get("path", "")),
                "branch": (w.get("branch") or "").replace("refs/heads/", ""),
                "comment": w.get("comment") or "",
                "status": w.get("workspaceStatus") or "",
                "last_activity": w.get("lastActivityAt") or 0,
                "agents": agents,
            })
    return out


def find_owner(branch, worktrees):
    """(worktree, agent) owning a branch: the worktree on that branch whose newest agent
    was updated last."""
    best = None
    for w in worktrees:
        if w["branch"] != branch or not w["agents"]:
            continue
        a = w["agents"][-1]
        if best is None or (a.get("updatedAt") or 0) > (best[1].get("updatedAt") or 0):
            best = (w, a)
    return best or (None, None)


def leader_for(pr, owner_wt):
    host = owner_wt["host"] if owner_wt else None
    wname = owner_wt["name"] if owner_wt else ""
    labels = set(pr.get("labels", []))
    branch = pr["headRefName"]
    for rule in LEADERS.get("rules", []):
        if "host" in rule and rule["host"] != host:
            continue
        if "branch_prefix" in rule and not any(branch.startswith(p) for p in rule["branch_prefix"]):
            continue
        if "worktree_prefix" in rule and not any(wname.startswith(p) for p in rule["worktree_prefix"]):
            continue
        if "label" in rule and not labels.intersection(rule["label"]):
            continue
        return rule["leader"]
    return LEADERS.get("default")


def send(leader_or_addr, subject, body, msg_type="status", priority=None, on=None):
    """Send an Orca message. `leader_or_addr` is a leaders.toml key or an address."""
    info = LEADERS.get("leaders", {}).get(leader_or_addr)
    addr = info["address"] if info else leader_or_addr
    on = on or (info or {}).get("on")
    cmd = ["orca", "orchestration", "send", "--from", CFG.get("orca_from", "merge-train"),
           "--to", addr, "--type", msg_type, "--subject", subject, "--body", body, "--json"]
    if on:
        cmd += ["--on", on]
    if priority:
        cmd += ["--priority", priority]
    if not mutate("send %s%s: %s" % (addr, " on " + on if on else "", subject)):
        return
    rc, out, err = sh(cmd, check=False)
    if rc != 0:
        log("orca send to %s failed: %s" % (addr, (err or out).strip()[:300]), "orca")


def owner_address(agent, wt):
    if not agent:
        return None, None
    addr = agent.get("address") or "orca_session_id:" + agent["orcaSessionId"]
    return addr, (wt["host"] if wt and wt["host"] != "local" else None)


# --------------------------------------------------------------------------- GitHub data

PR_FIELDS = """
  number title url isDraft state baseRefName headRefName headRefOid mergeable
  isCrossRepository
  author { login }
  labels(first: 30) { nodes { name } }
  timelineItems(itemTypes: [LABELED_EVENT], last: 50) {
    nodes { ... on LabeledEvent { createdAt label { name } } }
  }
"""


def _norm_pr(n):
    n["labels"] = [x["name"] for x in n["labels"]["nodes"]]
    q = CFG["labels"]["queue"]
    times = [e["createdAt"] for e in n.pop("timelineItems")["nodes"]
             if e and e.get("label", {}).get("name") == q]
    n["queued_at"] = max(times) if times else None
    return n


def fetch_queue():
    owner, name = repo_owner_name()
    data = gql("""query($o:String!,$n:String!,$l:String!){repository(owner:$o,name:$n){
      pullRequests(states:OPEN, labels:[$l], first:50){ nodes { %s } } } }""" % PR_FIELDS,
               o=owner, n=name, l=CFG["labels"]["queue"])
    prs = [_norm_pr(n) for n in data["repository"]["pullRequests"]["nodes"]]
    if ARGS and getattr(ARGS, "simulate", None):
        have = {p["number"] for p in prs}
        for num in ARGS.simulate:
            if num not in have:
                p = fetch_pr(num)
                p["queued_at"] = p["queued_at"] or now_utc().strftime("%Y-%m-%dT%H:%M:%SZ")
                p["simulated"] = True
                prs.append(p)
    prs.sort(key=lambda p: (p["queued_at"] or "9999", p["number"]))
    return prs


def fetch_pr(num):
    owner, name = repo_owner_name()
    data = gql("""query($o:String!,$n:String!,$p:Int!){repository(owner:$o,name:$n){
      pullRequest(number:$p){ %s } } }""" % PR_FIELDS, o=owner, n=name, p=num)
    return _norm_pr(data["repository"]["pullRequest"])


def check_runs(sha):
    """Newest check run per name on a commit: {name: run}."""
    runs = []
    page = 1
    while True:
        d = gh_json(["api", "repos/%s/commits/%s/check-runs?per_page=100&page=%d"
                     % (CFG["repo"], sha, page)])
        runs += d["check_runs"]
        if len(runs) >= d["total_count"] or not d["check_runs"]:
            break
        page += 1
    latest = {}
    for r in runs:
        if r["name"] not in latest or r["id"] > latest[r["name"]]["id"]:
            latest[r["name"]] = r
    return latest


def required_checks():
    c = CFG["checks"]
    full = list(c.get("full", [])) if c.get("pr_full", True) else []
    return list(c.get("fast", [])) + full


def evaluate_checks(sha):
    """('green'|'pending'|'failed', details list of (name, state, url))."""
    latest = check_runs(sha)
    details, failed, pending = [], False, False
    for name in required_checks():
        r = latest.get(name)
        if r is None:
            details.append((name, "missing", ""))
            pending = True
        elif r["status"] != "completed":
            details.append((name, r["status"], r["html_url"]))
            pending = True
        elif r["conclusion"] == "success":
            details.append((name, "success", r["html_url"]))
        elif r["conclusion"] == "skipped":
            # The full tier is skipped until the label lands; a labelled run follows.
            details.append((name, "skipped", r["html_url"]))
            pending = True
        elif r["conclusion"] == "cancelled":
            # Not a test failure (concurrency, runner loss, outside cancel): process_head
            # reruns the job; a second cancellation on the same head fails the PR.
            details.append((name, "cancelled", r["html_url"]))
            pending = True
        else:
            details.append((name, r["conclusion"], r["html_url"]))
            failed = True
    if CFG["checks"].get("fail_on_other_failures", True):
        req = set(required_checks())
        for name, r in latest.items():
            if name not in req and r["status"] == "completed" and \
                    r["conclusion"] in ("failure", "timed_out", "startup_failure"):
                details.append((name, r["conclusion"], r["html_url"]))
                failed = True
    return ("failed" if failed else "pending" if pending else "green"), details


# --------------------------------------------------------------------------- merge logic


def overlap(pr_sha):
    """Files the PR changed that dev also changed after the PR's merge-base."""
    dev = "origin/" + CFG["base"]
    mb = git("merge-base", dev, pr_sha).strip()
    prf = set(filter(None, git("diff", "--name-only", mb, pr_sha).splitlines()))
    devf = set(filter(None, git("diff", "--name-only", mb, dev).splitlines()))

    def keep(path, ignore):
        return not any(path == i or (i.endswith("/") and path.startswith(i)) for i in ignore)

    shared = sorted({f for f in prf if keep(f, CFG["merge"]["overlap_ignore_pr"])}
                    & {f for f in devf if keep(f, CFG["merge"]["overlap_ignore_dev"])})
    if shared or not devf:
        return shared  # dev changed nothing since the merge-base: nothing to relate
    if not CFG["merge"].get("relate_crates", True):
        return []  # file-level only; dev's post-merge CI catches cross-file breaks
    rel = crate_relation(prf, devf)
    return ["crate " + rel] if rel else []


_GRAPH = {}


def crate_graph():
    """Workspace crates of the dev checkout, via tools/ci/affected.py: (members dir→name,
    deps name→set, member glob roots, exact members, string-literal refs). Cached per
    dev commit; the runner clone is checked out at origin/dev."""
    root = repo_git_dir()
    key = git("rev-parse", "HEAD", cwd=root).strip()
    if key in _GRAPH:
        return _GRAPH[key]
    sys.path.insert(0, os.path.join(root, "tools", "ci"))
    try:
        import affected  # noqa: PLC0415 — lives in the checked-out repo, not next to us
    finally:
        sys.path.pop(0)
    from pathlib import Path  # noqa: PLC0415
    r = Path(root)
    members, deps = affected.workspace(r)
    glob_roots, exact = affected.member_globs((r / "Cargo.toml").read_text(encoding="utf-8"))
    tops = {d.split("/")[0] for d in members}
    outside = sorted(p.name for p in r.iterdir()
                     if not p.name.startswith(".") and p.name not in tops and p.name != "target")
    if "tools" not in outside and (r / "tools").is_dir():
        outside.append("tools")
    refs = affected.source_references(r, members, outside)
    _GRAPH.clear()
    _GRAPH[key] = (affected, members, deps, glob_roots, exact, refs)
    return _GRAPH[key]


ALL_CRATES = "*"


def crates_of(files, graph):
    """Workspace crates a set of changed files touches (directly). Files that only
    affect tooling (.github/, Cargo.lock, docs/tools nobody's code reads) touch none;
    root manifests, toolchain, vendor/ or xtask/ touch every crate (ALL_CRATES)."""
    affected, members, _deps, glob_roots, exact, refs = graph
    files = [f for f in files if not f.startswith(".github/") and f != "Cargo.lock"]
    mode, direct, _ = affected.classify(files, members, glob_roots, exact, refs)
    return {ALL_CRATES} if mode == "full" else set(direct)


def crate_relation(files_a, files_b):
    """A short description if the crates the two file sets touch are the same or depend
    on each other (either direction, transitively), else ''. Unknown → treated as related."""
    try:
        graph = crate_graph()
        a, b = crates_of(files_a, graph), crates_of(files_b, graph)
    except Exception as e:  # noqa: BLE001 — be safe: an unknown relation means update first
        return "关系算不出来（%s）" % str(e)[:120]
    if not a or not b:
        return ""
    if ALL_CRATES in a or ALL_CRATES in b:
        return "改到工作区根配置（影响所有 crate）"
    affected, deps = graph[0], graph[2]
    same = sorted(a & b)
    if same:
        return "同一个 crate：" + ", ".join(same[:3])
    down_a, down_b = affected.downstream(a, deps), affected.downstream(b, deps)
    pairs = ["%s 依赖 %s" % (x, y) for x in sorted(a) for y in sorted(b) if x in down_b and x != y]
    pairs += ["%s 依赖 %s" % (y, x) for x in sorted(a) for y in sorted(b) if y in down_a and x != y]
    return ("有依赖：" + ", ".join(pairs[:3])) if pairs else ""


def fetch_pr_head(num):
    git("fetch", "-q", "origin", "+refs/heads/%s:refs/remotes/origin/%s" % (CFG["base"], CFG["base"]),
        "+refs/pull/%d/head:refs/merge-train/pr-%d" % (num, num))


def squash_wanted(pr):
    return CFG["labels"]["reference_data"] in pr["labels"] or any(
        pr["headRefName"].startswith(p) for p in CFG["merge"]["squash_branch_prefixes"])


def comment(num, body):
    if mutate("comment on #%d: %s" % (num, body.splitlines()[0])):
        sh(["gh", "pr", "comment", str(num), "--repo", CFG["repo"], "--body", body], check=False)


def remove_label(num, label):
    if mutate("remove label %s from #%d" % (label, num)):
        sh(["gh", "pr", "edit", str(num), "--repo", CFG["repo"], "--remove-label", label], check=False)


def fail_pr(pr, reason, details=(), worktrees=None):
    """Take a PR out of the queue: drop the label, comment, notify owner and leader."""
    num = pr["number"]
    lines = ["merge-train：#%d 没有合并，已去掉 `%s`。" % (num, CFG["labels"]["queue"]),
             "", "原因：" + reason]
    for name, state, url in details:
        if state != "success":
            lines.append("- %s：%s %s" % (name, state, url))
    lines += ["", "修好并推送后，请领导重新打 `%s`。" % CFG["labels"]["queue"]]
    body = "\n".join(lines)
    log("FAIL #%d: %s" % (num, reason))
    remove_label(num, CFG["labels"]["queue"])
    comment(num, body)
    worktrees = worktrees if worktrees is not None else orca_worktrees()
    wt, agent = find_owner(pr["headRefName"], worktrees)
    leader = leader_for(pr, wt)
    subject = "merge-train: #%d 没有合并（%s）" % (num, reason.split("：")[0][:40])
    msg = body + "\n" + pr["url"]
    addr, on = owner_address(agent, wt)
    if addr:
        send(addr, subject, msg, priority="high", on=on)
    # With an owner found, the owner fixes it; the leader hears only about orphaned PRs.
    if leader and (not addr or CFG.get("notify", {}).get("bounce_leader", True)):
        send(leader, subject, msg + ("\n负责 agent：%s" % (addr or "（Orca 里没找到这个分支）")))


def _is_worktree_of(path, repo):
    common = lambda d: os.path.realpath(os.path.join(d, sh(  # noqa: E731
        ["git", "rev-parse", "--git-common-dir"], cwd=d, check=False)[1].strip() or "?"))
    return common(path) == common(repo)


def rerun_cancelled(pr, ps, names):
    """Rerun required jobs whose newest run on this head was cancelled. Returns a failure
    reason once a check has been cancelled twice on the same head, else None."""
    runs = check_runs(pr["headRefOid"])
    seen = ps.setdefault("cancelled_runs", {})  # check name -> cancelled check-run ids
    grace = CFG.get("cancel_grace_minutes", 2)
    for name in names:
        r = runs.get(name)
        if not r or r["conclusion"] != "cancelled":
            continue
        ids = seen.setdefault(name, [])
        if r["id"] in ids or r["id"] in seen.get("_infra", []):
            continue  # rerun already requested; waiting for the new run to show up
        done = parse_ts(r.get("completed_at"))
        if done and (now_utc() - done).total_seconds() < grace * 60:
            continue  # a replacing run (e.g. after update-branch) may still appear
        infra = runner_not_acquired(r["id"])
        if not infra and len(ids) >= CFG.get("max_cancellations", 2) - 1:
            ids.append(r["id"])
            return "%s 在同一个 head %s 上被取消了 %d 次" % (name, pr["headRefOid"][:7], len(ids))
        # A check run's id is its Actions job id.
        if mutate("rerun cancelled job %s (%s) on #%d" % (r["id"], name, pr["number"])):
            rc, out, err = sh(["gh", "api", "-X", "POST", "repos/%s/actions/jobs/%d/rerun"
                               % (CFG["repo"], r["id"])], check=False)
            if rc != 0:
                # Usually: the workflow run still has other jobs running. Retry next pass.
                if ps.get("rerun_error") != r["id"]:
                    log("#%d rerun of %s (job %d) not possible yet: %s"
                        % (pr["number"], name, r["id"], (err or out).strip()[:200]))
                    ps["rerun_error"] = r["id"]
                continue
        if infra:
            # GitHub had no runner (capacity); not counted, bounded by ci_timeout_minutes.
            seen.setdefault("_infra", []).append(r["id"])
            log("#%d %s got no runner on %s (GitHub capacity); reran job %d, not counted"
                % (pr["number"], name, pr["headRefOid"][:7], r["id"]))
            continue
        ids.append(r["id"])
        log("#%d %s was cancelled on %s; reran job %d (%d/%d)"
            % (pr["number"], name, pr["headRefOid"][:7], r["id"], len(ids),
               CFG.get("max_cancellations", 2)))
    return None


def runner_not_acquired(check_run_id):
    """True if GitHub cancelled the job because no hosted runner picked it up."""
    try:
        notes = gh_json(["api", "repos/%s/check-runs/%d/annotations" % (CFG["repo"], check_run_id)])
    except (RuntimeError, ValueError):
        return False
    return any("not acquired by Runner" in (a.get("message") or "") for a in notes or [])


def scratch_at(commit):
    """Reset the scratch worktree to `commit`; return a git runner bound to it."""
    work = expand(CFG["work_dir"])
    if os.path.exists(work) and not _is_worktree_of(work, repo_git_dir()):
        log("scratch dir %s is not a worktree of %s; recreating it" % (work, repo_git_dir()))
        shutil.rmtree(work)
    if not os.path.exists(work):
        os.makedirs(os.path.dirname(work), exist_ok=True)
        git("worktree", "prune")
        git("worktree", "add", "--detach", work, "origin/" + CFG["base"])
    w = lambda *a, **k: git(*a, cwd=work, **k)  # noqa: E731
    w("merge", "--abort", check=False)
    w("reset", "-q", "--hard")
    w("clean", "-qfd")
    w("checkout", "-q", "--detach", commit)
    return w


def fast_ci_active(head):
    """The fast workflow can be waiting for a runner before its required job exists."""
    endpoint = "repos/%s/actions/workflows/ci-fast.yml/runs?head_sha=%s&per_page=100" % (
        CFG["repo"], head)
    pages = gh_json(["api", "--paginate", "--slurp", endpoint])
    return any(run["status"] != "completed" for page in pages for run in page["workflow_runs"])


def nudge_ci(pr, ps):
    """Required checks never started on this head (e.g. it predates a CI change, or
    the run was never triggered): make GitHub run CI on a new head. Behind dev →
    update-branch; already contains dev → push an empty commit. Once per head."""
    num, branch, head = pr["number"], pr["headRefName"], pr["headRefOid"]
    ps["nudged_head"] = head
    fetch_pr_head(num)
    behind = sh(["git", "merge-base", "--is-ancestor", "origin/" + CFG["base"], head],
                cwd=repo_git_dir(), check=False)[0] != 0
    if behind:
        if mutate("gh pr update-branch %d (required checks missing on %s)" % (num, head[:7])):
            rc, out, err = sh(["gh", "pr", "update-branch", str(num), "--repo", CFG["repo"]],
                              check=False)
            if rc != 0:
                log("#%d nudge: update-branch failed: %s" % (num, (err or out).strip()[:200]))
                return
            ps.update(updated_head=head, updated_at=now_utc().isoformat())
        log("#%d nudge: required checks missing on %s; ran update-branch" % (num, head[:7]))
        return
    if pr.get("isCrossRepository"):
        log("#%d nudge: checks missing but the PR is from a fork; waiting for the timeout" % num)
        return
    if not mutate("push an empty commit to %s to retrigger CI (checks missing on %s)"
                  % (branch, head[:7])):
        return
    w = scratch_at(head)
    w("commit", "-q", "--allow-empty", "-m",
      "merge-train: retrigger CI (required checks missing on %s)" % head[:7])
    rc, out, err = w("push", "origin", "HEAD:refs/heads/" + branch, check=False)
    if rc != 0:
        log("#%d nudge: push failed: %s" % (num, err.strip()[:200]))
        return
    log("#%d nudge: required checks missing on %s; pushed an empty commit" % (num, head[:7]))


def try_conflict_repair(pr):
    """Merge dev into the PR head in the scratch worktree. If the only conflict is
    Cargo.lock, take dev's lock, let cargo regenerate it, push. Returns (ok, message)."""
    num, branch, head = pr["number"], pr["headRefName"], pr["headRefOid"]
    if pr.get("isCrossRepository"):
        return False, "PR 来自 fork，脚本不能推送"
    fetch_pr_head(num)
    w = scratch_at(head)
    rc, out, err = w("merge", "--no-ff", "--no-commit", "origin/" + CFG["base"], check=False)
    conflicts = set(filter(None, w("diff", "--name-only", "--diff-filter=U").splitlines()))
    if rc != 0 and not conflicts:
        w("merge", "--abort", check=False)
        return False, "本地合并 dev 失败：%s" % (err or out).strip()[:200]
    if rc == 0 and w("rev-parse", "-q", "--verify", "MERGE_HEAD", check=False)[0] != 0:
        return True, "PR 已经包含最新的 dev，无需推送"
    if conflicts - {"Cargo.lock"}:
        w("merge", "--abort", check=False)
        return False, "和 dev 有冲突：" + ", ".join(sorted(conflicts)[:10])
    if "Cargo.lock" in conflicts:
        w("checkout", "origin/" + CFG["base"], "--", "Cargo.lock")
        env = dict(os.environ, CARGO_TARGET_DIR=expand(CFG["cargo_target_dir"]))
        rc, out, err = sh(["cargo", "metadata", "--format-version", "1"], cwd=expand(CFG["work_dir"]),
                          env=env, check=False)
        if rc != 0:
            w("merge", "--abort", check=False)
            return False, "cargo metadata 重新生成 Cargo.lock 失败：%s" % err.strip()[-300:]
        w("add", "Cargo.lock")
        what = "merge %s into %s, regenerate Cargo.lock" % (CFG["base"], branch)
    else:
        what = "merge %s into %s" % (CFG["base"], branch)
    if not mutate("commit + push to %s: %s" % (branch, what)):
        w("merge", "--abort", check=False)
        return True, "(dry-run) " + what
    w("commit", "-q", "-m", "merge-train: " + what)
    rc, out, err = w("push", "origin", "HEAD:refs/heads/" + branch, check=False)
    if rc != 0:
        return False, "推送失败：%s" % err.strip()[:200]
    return True, what


def decide(pr, st):
    """Look at one PR and return (decision, detail, data). Pure reads only."""
    if pr["state"] != "OPEN":
        return "gone", "PR 已经不是 open", {}
    if pr["isDraft"] and not (ARGS.dry_run and ARGS.ignore_draft):
        return "reject", "PR 还是 Draft", {}
    if pr["baseRefName"] != CFG["base"]:
        return "reject", "PR 的目标分支是 %s，不是 %s" % (pr["baseRefName"], CFG["base"]), {}
    if pr["mergeable"] == "CONFLICTING":
        return "conflict", "GitHub 显示有冲突", {}
    status, details = evaluate_checks(pr["headRefOid"])
    data = {"checks": details}
    if status == "failed":
        return "ci-failed", "CI 没过", data
    if status == "pending":
        return "wait-ci", "等 CI：" + ", ".join("%s=%s" % (n, s) for n, s, _ in details
                                                if s != "success"), data
    fetch_pr_head(pr["number"])
    over = overlap(pr["headRefOid"])
    data["overlap"] = over
    if over:
        return "update", "和更新的 dev 改了同样的文件或有依赖的 crate：" + ", ".join(over[:5]), data
    if pr["mergeable"] != "MERGEABLE":
        return "wait-mergeable", "GitHub mergeable=%s" % pr["mergeable"], data
    return "merge", "CI 全绿，无重叠，MERGEABLE", data


def pr_state(st, num):
    return st.setdefault("prs", {}).setdefault(str(num), {})


def process_head(pr, st):
    """Act on the PR at the front of the queue. Returns True if it should be re-examined
    soon (something changed), False to wait for the next poll."""
    num = pr["number"]
    ps = pr_state(st, num)
    if ps.get("head") != pr["headRefOid"]:
        ps.update(head=pr["headRefOid"], since=now_utc().isoformat())
        for k in ("unknown_since", "cancelled_runs", "stuck_since"):
            ps.pop(k, None)
    decision, detail, data = decide(pr, st)
    changed = (decision, detail) != (ps.get("decision"), ps.get("detail"))
    ps.update(decision=decision, detail=detail, checked=now_utc().isoformat())
    if changed:
        log("#%d %s → %s: %s" % (num, pr["headRefOid"][:7], decision, detail))
    elif ARGS.dry_run:
        return False  # same verdict as last pass; already reported
    age_min = (now_utc() - dt.datetime.fromisoformat(ps["since"])).total_seconds() / 60

    if decision == "gone":
        return True
    if decision == "reject":
        fail_pr(pr, detail)
        return True
    if decision == "ci-failed":
        fail_pr(pr, detail, data.get("checks", ()))
        return True
    if decision == "wait-ci":
        if age_min > CFG["ci_timeout_minutes"]:
            fail_pr(pr, "CI 超过 %d 分钟没有结果（%s）" % (CFG["ci_timeout_minutes"], detail),
                    data.get("checks", ()))
            return True
        cancelled = [n for n, x, _ in data.get("checks", ()) if x == "cancelled"]
        if cancelled:
            why = rerun_cancelled(pr, ps, cancelled)
            if why:
                fail_pr(pr, why, data.get("checks", ()))
                return True
        # The required fast job appears only after its prerequisite, changes, finishes.
        states = [st_ for _, st_, _ in data.get("checks", ()) if st_ != "success"]
        if states and all(x in ("missing", "skipped") for x in states):
            ps.setdefault("stuck_since", now_utc().isoformat())
            stuck = (now_utc() - dt.datetime.fromisoformat(ps["stuck_since"])).total_seconds() / 60
            if stuck >= CFG.get("missing_nudge_minutes", 10) and ps.get("nudged_head") != pr["headRefOid"]:
                if fast_ci_active(pr["headRefOid"]):
                    ps.pop("stuck_since")
                else:
                    nudge_ci(pr, ps)
        else:
            ps.pop("stuck_since", None)
        return False
    if decision == "wait-mergeable":
        ps.setdefault("unknown_since", now_utc().isoformat())
        waited = (now_utc() - dt.datetime.fromisoformat(ps["unknown_since"])).total_seconds() / 60
        if waited > CFG["unknown_timeout_minutes"]:
            fail_pr(pr, "GitHub 的 mergeable 一直是 %s" % pr["mergeable"])
            return True
        return False
    ps.pop("unknown_since", None)
    if decision == "update":
        if ps.get("updated_head") == pr["headRefOid"] and not ARGS.dry_run:
            since = ps.get("updated_at")
            if since and (now_utc() - dt.datetime.fromisoformat(since)).total_seconds() < 300:
                return False  # GitHub may still report the pre-update head; give it time
            # update-branch did not move the head (e.g. GitHub refused); try locally.
            decision = "conflict"
        else:
            if mutate("gh pr update-branch %d (%s)" % (num, detail)):
                rc, out, err = sh(["gh", "pr", "update-branch", str(num), "--repo", CFG["repo"]],
                                  check=False)
                if rc != 0:
                    log("#%d update-branch failed: %s" % (num, (err or out).strip()[:200]))
                    decision = "conflict"
                else:
                    ps.update(updated_head=pr["headRefOid"], updated_at=now_utc().isoformat())
                    log("#%d update-branch done; waiting for CI on the new head" % num)
            else:
                ps["updated_head"] = pr["headRefOid"]
            if decision == "update":
                return False
    if decision == "conflict":
        ok, msg = try_conflict_repair(pr)
        if ok:
            log("#%d conflict repair: %s; waiting for CI" % (num, msg))
            ps.update(updated_head=pr["headRefOid"], updated_at=now_utc().isoformat())
            return False
        fail_pr(pr, msg)
        return True
    if decision == "merge":
        return do_merge(pr, st)
    return False


def do_merge(pr, st):
    num = pr["number"]
    method = "--squash" if squash_wanted(pr) else "--merge"
    if not mutate("gh pr merge %d %s --match-head-commit %s  (%s)"
                  % (num, method, pr["headRefOid"][:7], pr["title"])):
        return False
    rc, out, err = sh(["gh", "pr", "merge", str(num), "--repo", CFG["repo"], method,
                       "--match-head-commit", pr["headRefOid"]], check=False)
    if rc != 0:
        ps = pr_state(st, num)
        ps["merge_failures"] = ps.get("merge_failures", 0) + 1
        msg = (err or out).strip()[:300]
        log("#%d merge failed (%d): %s" % (num, ps["merge_failures"], msg))
        if ps["merge_failures"] >= 3:
            fail_pr(pr, "gh pr merge 连续失败 3 次：" + msg)
            return True
        return False  # re-evaluated next pass (head moved, mergeable changed, ...)
    git("fetch", "-q", "origin", CFG["base"])
    new = git("rev-parse", "--short", "origin/" + CFG["base"]).strip()
    log("MERGED #%d %s → %s %s" % (num, pr["title"], CFG["base"], new))
    st.setdefault("merged", []).append({"pr": num, "title": pr["title"], "dev": new,
                                        "at": now_utc().isoformat()})
    st["merged"] = st["merged"][-20:]
    st.get("prs", {}).pop(str(num), None)
    wt, _agent = find_owner(pr["headRefName"], orca_worktrees())
    leader = leader_for(pr, wt)
    # The board already lists merges; a message per merge only wakes the leader.
    if leader and CFG.get("notify", {}).get("merged", True):
        line = "已合并 #%d %s → %s %s" % (num, pr["title"], CFG["base"], new)
        send(leader, line[:120], line + "\n" + pr["url"])
    return True


# --------------------------------------------------------------------------- pause


def board_issue_number(st):
    num = st.get("board_issue")
    if num:
        return num
    title = CFG["board"]["title"]
    found = gh_json(["issue", "list", "--repo", CFG["repo"], "--state", "open", "--search",
                     '"%s" in:title' % title, "--json", "number,title"])
    for it in found or []:
        if it["title"] == title:
            st["board_issue"] = it["number"]
            return it["number"]
    return None


def paused_reason(st):
    """Reason string if the queue is paused (pause file or label on the board issue)."""
    if os.path.exists(pause_file()):
        with open(pause_file(), encoding="utf-8") as f:
            return f.read().strip() or "暂停文件存在"
    num = board_issue_number(st)
    if num:
        labels = gh_json(["issue", "view", str(num), "--repo", CFG["repo"], "--json", "labels"])
        if any(l["name"] == CFG["labels"]["paused"] for l in labels["labels"]):
            return "看板 issue #%d 上有 %s 标签" % (num, CFG["labels"]["paused"])
    return None


def set_paused(st, reason):
    with open(pause_file(), "w", encoding="utf-8") as f:
        f.write(reason + "\n")
    num = board_issue_number(st)
    if num:
        sh(["gh", "issue", "edit", str(num), "--repo", CFG["repo"], "--add-label",
            CFG["labels"]["paused"]], check=False)


def rerun_dev_cancelled(st, sha, latest):
    """dev runs are never cancelled by concurrency, so a cancelled required job there
    (usually: no runner acquired) leaves dev with no CI signal at all. Rerun it, as often
    as needed, until it has a real result."""
    done_ids = st.setdefault("dev_reruns", [])
    for name in CFG["checks"]["full"]:
        r = latest.get(name)
        if not r or r["status"] != "completed" or r["conclusion"] != "cancelled" or r["id"] in done_ids:
            continue
        ended = parse_ts(r.get("completed_at"))
        if ended and (now_utc() - ended).total_seconds() < CFG.get("cancel_grace_minutes", 2) * 60:
            continue
        if not mutate("rerun cancelled dev job %d (%s) on %s" % (r["id"], name, sha[:7])):
            continue
        rc, out, err = sh(["gh", "api", "-X", "POST", "repos/%s/actions/jobs/%d/rerun"
                           % (CFG["repo"], r["id"])], check=False)
        if rc != 0:
            if st.get("dev_rerun_error") != r["id"]:
                log("dev %s: rerun of %s (job %d) not possible yet: %s"
                    % (sha[:7], name, r["id"], (err or out).strip()[:200]))
                st["dev_rerun_error"] = r["id"]
            continue
        done_ids.append(r["id"])
        st["dev_reruns"] = done_ids[-50:]
        log("dev %s: %s was cancelled; reran job %d" % (sha[:7], name, r["id"]))


def auto_resume_if_green(st, sha, latest):
    """Lift a pause the train put on itself for a red dev commit once the newest dev
    commit's full checks are all green. Manual pauses are left alone."""
    red = auto_paused_sha()
    if not red:
        return False
    full = CFG["checks"]["full"]
    if not all(n in latest and latest[n]["status"] == "completed"
               and latest[n]["conclusion"] == "success" for n in full):
        return False
    line = "merge-train 自动恢复：dev %s 的全量 CI 全绿（暂停原因是 dev %s 红了）" % (sha[:7], red[:7])
    log(line)
    if not mutate("auto-resume the queue and notify %s" % LEADERS.get("pause_notify")):
        return True
    if os.path.exists(pause_file()):
        os.remove(pause_file())
    num = board_issue_number(st)
    if num:
        sh(["gh", "issue", "edit", str(num), "--repo", CFG["repo"], "--remove-label",
            CFG["labels"]["paused"]], check=False)
    clear_auto_pause()
    st.pop("paused", None)
    for leader in LEADERS.get("pause_notify", []):
        send(leader, line[:120], line)
    return True


def check_dev_red(st):
    """Pause the queue when the newest dev commit's required full checks failed; lift
    that pause by itself once the newest dev commit is green again."""
    sha = git("rev-parse", "origin/" + CFG["base"]).strip()
    latest = check_runs(sha)
    rerun_dev_cancelled(st, sha, latest)
    auto_resume_if_green(st, sha, latest)
    if red_ack_get() == sha:
        return False
    bad = [(n, latest[n]["conclusion"], latest[n]["html_url"]) for n in CFG["checks"]["full"]
           if n in latest and latest[n]["status"] == "completed"
           and latest[n]["conclusion"] in ("failure", "timed_out", "startup_failure")]
    if not bad:
        return False
    reason = "dev %s 的 CI 红了：%s" % (sha[:7], ", ".join("%s=%s" % (n, c) for n, c, _ in bad))
    log("PAUSE: " + reason)
    if mutate("pause the queue and notify %s" % LEADERS.get("pause_notify")):
        red_ack_set(sha)  # pause once per red commit
        set_paused(st, reason + "（dev 最新提交全量变绿后自动恢复；也可手动 merge_train.py resume）")
        with open(auto_pause_file(), "w", encoding="utf-8") as f:
            f.write(sha + "\n")
        body = reason + "\n" + "\n".join("- %s %s" % (n, u) for n, _, u in bad) + \
            "\n合并队列已暂停。dev 最新提交的全量 CI 全绿后会自动恢复并通知；也可以手动运行 `merge_train.py resume`。"
        for leader in LEADERS.get("pause_notify", []):
            send(leader, "merge-train 暂停：" + reason[:80], body, msg_type="escalation",
                 priority="high")
    return True


# --------------------------------------------------------------------------- runner


def self_digest():
    h = hashlib.sha256()
    for f in ("merge_train.py", "config.toml", "leaders.toml"):
        try:
            with open(os.path.join(HERE, f), "rb") as fh:
                h.update(fh.read())
        except OSError:
            pass
    return h.hexdigest()


def refresh_runner():
    """Fetch, and move the runner worktree to runner_ref when this script runs from it.
    Returns True if our own files changed (caller exits; launchd restarts us)."""
    runner = expand(CFG["runner_dir"])
    git("fetch", "-q", "--prune", "origin")
    if not HERE.startswith(runner + os.sep):
        return False
    ref = CFG["runner_ref"]
    if sh(["git", "cat-file", "-e", ref + ":tools/merge-train/merge_train.py"], cwd=runner,
          check=False)[0] != 0:
        log("runner_ref %s has no merge_train.py; staying on the current checkout" % ref)
        return False
    if sh(["git", "merge-base", "--is-ancestor", "HEAD", ref], cwd=runner, check=False)[0] != 0:
        # Checked out at a commit runner_ref does not contain yet (install.sh --ref <branch>):
        # stay there until it is merged, instead of jumping back to an older version.
        return False
    if git("status", "--porcelain", cwd=runner).strip():
        log("runner clone has local changes; not updating it")
        return False
    before = self_digest()
    git("checkout", "-q", "--detach", ref, cwd=runner)
    return self_digest() != before


def one_pass(st):
    q = fetch_queue()
    st["queue"] = [{"pr": p["number"], "title": p["title"], "branch": p["headRefName"],
                    "queued_at": p["queued_at"]} for p in q]
    st["last_pass"] = now_utc().isoformat()
    check_dev_red(st)
    reason = paused_reason(st)
    st["paused"] = reason
    if reason:
        if st.get("logged_pause") != reason:
            log("queue paused: " + reason)
            st["logged_pause"] = reason
        return False
    st.pop("logged_pause", None)
    live = {str(p["number"]) for p in q}
    for k in list(st.get("prs", {})):
        if k not in live:
            st["prs"].pop(k)
    for ref in git("for-each-ref", "--format=%(refname)", "refs/merge-train/").split():
        if ref.rsplit("-", 1)[-1] not in live:
            git("update-ref", "-d", ref, check=False)
    if not q:
        return False
    if st.get("head_pr") != q[0]["number"]:
        st["head_pr"], st["head_since"] = q[0]["number"], now_utc().isoformat()
    if ARGS.dry_run:
        # Nothing is mutated, so the head would stay stuck: show a verdict for every PR.
        again = False
        for p in q:
            again = process_head(p, st) or again
        return False
    if process_head(q[0], st):
        return True
    return skip_ahead(q, st)


def changed_files(sha):
    """Files a PR head changed since its merge-base with dev."""
    mb = git("merge-base", "origin/" + CFG["base"], sha).strip()
    return set(filter(None, git("diff", "--name-only", mb, sha).splitlines()))


def head_conflict(head_files, files):
    """Why a later PR must not skip ahead of the head: shared files (Cargo.lock and
    docs/ excluded) or crates that are the same or depend on each other. '' if none."""
    ignore = CFG["merge"]["overlap_ignore_dev"]
    keep = lambda f: not any(f == i or (i.endswith("/") and f.startswith(i)) for i in ignore)  # noqa: E731
    shared = sorted(f for f in head_files & files if keep(f))
    if shared:
        return ", ".join(shared[:5])
    if not CFG["merge"].get("relate_crates", True):
        return ""
    rel = crate_relation(head_files, files)
    return ("crate " + rel) if rel else ""


def skip_ahead(q, st):
    """When the queue head has been waiting too long, merge the first later PR (in
    label order) that is ready to merge right now and touches none of the head's files.
    A PR that overlaps the head would push it into another update-branch and full CI
    run (starving it), so it keeps waiting behind the head. Later PRs are only merged,
    never updated, repaired or failed here; the head keeps its place."""
    waited = (now_utc() - dt.datetime.fromisoformat(st["head_since"])).total_seconds() / 60
    if len(q) < 2 or waited < CFG.get("skip_ahead_after_minutes", 30):
        return False
    head_files = None
    for p in q[1:]:
        try:
            decision, detail, _ = decide(p, st)
        except Exception as e:  # noqa: BLE001 — one bad PR must not stop the look-ahead
            decision, detail = "error", str(e)[:200]
        ps = pr_state(st, p["number"])
        if (decision, detail) != (ps.get("decision"), ps.get("detail")):
            log("#%d (behind #%d) %s → %s: %s" % (p["number"], q[0]["number"],
                                                  p["headRefOid"][:7], decision, detail))
        if decision == "merge":
            try:
                if head_files is None:
                    fetch_pr_head(q[0]["number"])
                    head_files = changed_files(q[0]["headRefOid"])
                why = head_conflict(head_files, changed_files(p["headRefOid"]))
            except Exception as e:  # noqa: BLE001 — unknown overlap: do not jump the queue
                why = "?（%s）" % str(e)[:80]
            if why:
                decision, detail = "wait-head", "已就绪，但和队首 #%d 改了同样的文件或有依赖的 crate，不插队：%s" % (
                    q[0]["number"], why)
                if detail != ps.get("detail"):
                    log("#%d %s" % (p["number"], detail))
        ps.update(decision=decision, detail=detail, checked=now_utc().isoformat())
        if decision == "merge":
            log("#%d has waited %d min at the front; merging ready #%d ahead of it"
                % (q[0]["number"], waited, p["number"]))
            return do_merge(p, st)
    return False


def cmd_run():
    log("start (%s) pid=%d, script %s" % ("dry-run" if ARGS.dry_run else "live", os.getpid(), HERE))
    st = load_state()
    st["mode"] = "dry-run" if ARGS.dry_run else "live"
    last_err = None
    while True:
        try:
            if refresh_runner():
                log("runner updated to a new version of the merge train; restarting")
                save_state(st)
                return 0
            mode = st["mode"]
            st = load_state()  # pause/resume/board may have written it meanwhile
            st["mode"] = mode
            again = one_pass(st)
            save_state(st)
            last_err = None
        except Exception as e:  # noqa: BLE001 — keep the daemon alive; log and retry
            err = "%s: %s" % (type(e).__name__, e)
            if err != last_err:
                log("pass failed: " + err)
            last_err = err
            again = False
        if ARGS.once:
            return 0
        time.sleep(5 if again else CFG["poll_seconds"])


# --------------------------------------------------------------------------- board


def gh_open_prs():
    owner, name = repo_owner_name()
    data = gql("""query($o:String!,$n:String!){repository(owner:$o,name:$n){
      pullRequests(states:OPEN, first:100, orderBy:{field:UPDATED_AT,direction:DESC}){ nodes {
        number title url isDraft headRefName baseRefName
        labels(first:30){nodes{name}}
        comments(last:20){nodes{body createdAt}}
        commits(last:1){nodes{commit{oid committedDate statusCheckRollup{
          contexts(first:50){nodes{... on CheckRun{name status conclusion}}}}}}}
      } } } }""", o=owner, n=name)
    return data["repository"]["pullRequests"]["nodes"]


def branch_dates(branches):
    """{branch: committedDate} for the given branches, via one aliased GraphQL query."""
    owner, name = repo_owner_name()
    branches = sorted(set(b for b in branches if b))
    out = {}
    for i in range(0, len(branches), 60):
        chunk = branches[i:i + 60]
        parts = ["b%d: ref(qualifiedName:%s){target{... on Commit{committedDate}}}"
                 % (j, json.dumps("refs/heads/" + b)) for j, b in enumerate(chunk)]
        data = gql("query($o:String!,$n:String!){repository(owner:$o,name:$n){%s}}"
                   % " ".join(parts), o=owner, n=name)["repository"]
        for j, b in enumerate(chunk):
            ref = data.get("b%d" % j)
            if ref and ref.get("target"):
                out[b] = parse_ts(ref["target"].get("committedDate"))
    return out


def ci_summary(pr):
    nodes = pr["commits"]["nodes"]
    if not nodes or not nodes[0]["commit"].get("statusCheckRollup"):
        return "无检查"
    latest = {}
    for c in nodes[0]["commit"]["statusCheckRollup"]["contexts"]["nodes"]:
        if c and c.get("name"):
            latest[c["name"]] = c  # GraphQL returns oldest first; keep the newest
    icon = {"SUCCESS": "✅", "FAILURE": "❌", "CANCELLED": "⊘", "SKIPPED": "⏭",
            "TIMED_OUT": "❌", "NEUTRAL": "◻"}
    parts = []
    for n, c in sorted(latest.items()):
        if "${{" in n:
            continue
        s = icon.get(c.get("conclusion") or "", "⏳")
        parts.append("%s %s" % (s, n))
    return "<br>".join(parts) or "无检查"


def blocked_reason(pr):
    if CFG["labels"]["blocked"] not in [l["name"] for l in pr["labels"]["nodes"]]:
        return ""
    for c in reversed(pr["comments"]["nodes"]):
        m = re.match(r"\s*(blocked|卡住|受阻)\s*[:：]\s*(.+)", c["body"], re.I | re.S)
        if m:
            return "⛔ " + cell(m.group(2), 80)
    return "⛔（`blocked`，原因见 PR）"


def cell(s, limit=60):
    s = re.sub(r"\s+", " ", s or "").strip().replace("|", "\\|")
    return s if len(s) <= limit else s[:limit - 1] + "…"


STATE_ORDER = {"working": 0, "blocked": 1, "waiting": 2, "done": 3}


def build_board(st, old_body):
    worktrees = orca_worktrees()
    prs = gh_open_prs()
    by_branch = {}
    for p in prs:
        by_branch.setdefault(p["headRefName"], []).append(p)
    dates = branch_dates([w["branch"] for w in worktrees])
    dev_sha = git("rev-parse", "--short", "origin/" + CFG["base"]).strip()
    dev_ci = check_runs(git("rev-parse", "origin/" + CFG["base"]).strip())
    full = CFG["checks"]["full"]
    concl = [dev_ci[n]["conclusion"] if n in dev_ci and dev_ci[n]["status"] == "completed"
             else "pending" for n in full]
    dev_icon = "✅" if all(c == "success" for c in concl) else \
        "❌" if any(c in ("failure", "timed_out") for c in concl) else "⏳"

    L = [BOARD_MARKER, "# %s" % CFG["board"]["title"], "",
         "更新于 %s（每 10 分钟自动改写，数据来自 GitHub 和 orca）· %s `%s` CI %s"
         % (dt.datetime.now().astimezone().strftime("%Y-%m-%d %H:%M %Z"), CFG["base"], dev_sha, dev_icon),
         "", "只改下面“交接快照”一节；其他部分会被脚本覆盖。", ""]

    # merge queue
    L += ["## 合并队列", ""]
    last = st.get("last_pass")
    hb = dt.datetime.fromisoformat(last) if last else None
    stale = hb is None or (now_utc() - hb).total_seconds() > 600
    if st.get("paused"):
        L.append("状态：⏸ **暂停** — %s。恢复：`python3 tools/merge-train/merge_train.py resume`" % st["paused"])
    elif stale:
        L.append("状态：⚠️ 合并脚本没有在跑（最后一轮 %s）" % local_hm(hb))
    else:
        L.append("状态：▶️ 运行中%s（最后一轮 %s）"
                 % ("，**干跑**（只记录不合并）" if st.get("mode") == "dry-run" else "", local_hm(hb)))
    L.append("")
    queue = st.get("queue", [])
    if queue:
        L += ["| 顺序 | PR | 分支 | 打标签 | 当前判断 |", "|---|---|---|---|---|"]
        for i, q in enumerate(queue, 1):
            ps = st.get("prs", {}).get(str(q["pr"]), {})
            L.append("| %d | #%d %s | `%s` | %s | %s |" % (
                i, q["pr"], cell(q["title"], 50), q["branch"], local_hm(parse_ts(q["queued_at"])),
                cell("%s：%s" % (ps.get("decision", "未处理"), ps.get("detail", "")), 90)))
    else:
        L.append("队列为空（没有打 `%s` 的 PR）。" % CFG["labels"]["queue"])
    merged = st.get("merged", [])[-5:]
    if merged:
        L += ["", "最近合并：" + "；".join("#%d → `%s`（%s）" % (m["pr"], m["dev"],
              local_hm(dt.datetime.fromisoformat(m["at"]))) for m in reversed(merged))]

    # agents
    def row(w):
        a = w["agents"][-1]
        ps_ = by_branch.get(w["branch"], [])
        pr_cell = "<br>".join("#%d %s" % (p["number"], "Draft" if p["isDraft"] else "Ready") for p in ps_) or "—"
        ci = "<br>".join(ci_summary(p) for p in ps_) or "—"
        blk = " ".join(filter(None, (blocked_reason(p) for p in ps_)))
        host = "" if w["host"] == "local" else "（%s）" % w["host"]
        return "| %s%s | `%s` %s | `%s` | %s | %s | %s | %s | %s |" % (
            w["name"], host, a["orcaSessionId"][:8], a.get("state", ""), w["branch"] or "—",
            pr_cell, ci, local_hm(dates.get(w["branch"])), blk or "", cell(w["comment"], 60))

    head = ["| 工作流 | 会话 | 分支 | PR | CI | 最近提交 | 卡住 | 备注 |",
            "|---|---|---|---|---|---|---|---|"]
    with_agents = [w for w in worktrees if w["agents"]]
    active = [w for w in with_agents if w["agents"][-1].get("state") != "done"
              or w["branch"] in by_branch]
    idle = [w for w in with_agents if w not in active]
    key = lambda w: (STATE_ORDER.get(w["agents"][-1].get("state"), 9), -(w["last_activity"] or 0))  # noqa: E731
    L += ["", "## Agent", ""]
    if active:
        L += head + [row(w) for w in sorted(active, key=key)]
    else:
        L.append("没有在干活或开着 PR 的 agent。")
    if idle:
        L += ["", "<details><summary>空闲的工作树（%d）</summary>" % len(idle), ""] + head + \
             [row(w) for w in sorted(idle, key=key)] + ["", "</details>"]

    known = {w["branch"] for w in worktrees}
    orphans = [p for p in prs if p["headRefName"] not in known]
    if orphans:
        L += ["", "## 没有对应工作树的 PR", "", "| PR | 分支 | 状态 | CI | 卡住 |", "|---|---|---|---|---|"]
        for p in orphans:
            L.append("| #%d %s | `%s` | %s | %s | %s |" % (
                p["number"], cell(p["title"], 50), p["headRefName"], "Draft" if p["isDraft"] else "Ready",
                ci_summary(p), blocked_reason(p)))

    # handoff snapshot, preserved verbatim
    snap = "\n（领导每一轮在这里写：谁在做什么、待决定的事。脚本改写看板时原样保留这一节。）\n"
    if old_body and HANDOFF_START in old_body and HANDOFF_END in old_body:
        snap = old_body.split(HANDOFF_START, 1)[1].split(HANDOFF_END, 1)[0]
    L += ["", "## 交接快照", "", HANDOFF_START + snap + HANDOFF_END, ""]
    return "\n".join(L)


def cmd_board():
    st = load_state()
    try:
        git("fetch", "-q", "origin", CFG["base"])
    except RuntimeError as e:
        log("fetch failed: %s" % e, "board")
    num = board_issue_number(st)
    old = ""
    if num:
        old = gh_json(["issue", "view", str(num), "--repo", CFG["repo"], "--json", "body"])["body"]
    body = build_board(st, old)
    if ARGS.dry_run:
        print(body)
        return 0
    if not num:
        url = sh(["gh", "issue", "create", "--repo", CFG["repo"], "--title", CFG["board"]["title"],
                  "--body", body]).strip()
        num = int(url.rstrip("/").split("/")[-1])
        sh(["gh", "issue", "pin", str(num), "--repo", CFG["repo"]], check=False)
        log("created and pinned board issue #%d" % num, "board")
    else:
        # Re-read right before writing so a handoff edit made meanwhile is not lost.
        fresh = gh_json(["issue", "view", str(num), "--repo", CFG["repo"], "--json", "body"])["body"]
        if fresh != old:
            body = build_board(st, fresh)
        sh(["gh", "issue", "edit", str(num), "--repo", CFG["repo"], "--body-file", "-"], input_text=body)
    if st.get("board_issue") != num:
        st2 = load_state()
        st2["board_issue"] = num
        save_state(st2)
    log("board #%d updated" % num, "board")
    return 0


# --------------------------------------------------------------------------- small commands


def cmd_setup():
    labels = [(CFG["labels"]["queue"], "0e8a16", "领导审完：交给合并脚本按顺序合并"),
              (CFG["labels"]["paused"], "b60205", "合并队列暂停（打在看板 issue 上）"),
              (CFG["labels"]["blocked"], "d93f0b", "被卡住；在评论里写 blocked: 原因"),
              (CFG["labels"]["reference_data"], "c5def5", "参考数据 PR，squash 合并")]
    for name, color, desc in labels:
        if mutate("create label %s" % name):
            rc, out, err = sh(["gh", "label", "create", name, "--repo", CFG["repo"], "--color", color,
                               "--description", desc], check=False)
            log("label %s: %s" % (name, "created" if rc == 0 else (err.strip() or out.strip())[:80]))
    return 0


def cmd_pause():
    st = load_state()
    clear_auto_pause()  # a person paused it: only a person resumes it
    reason = "手动暂停 %s%s" % (dt.datetime.now().strftime("%m-%d %H:%M"),
                              "：" + ARGS.reason if ARGS.reason else "")
    set_paused(st, reason)
    save_state(st)
    log("paused: " + reason)
    return 0


def cmd_resume():
    st = load_state()
    clear_auto_pause()
    if os.path.exists(pause_file()):
        os.remove(pause_file())
    num = board_issue_number(st)
    if num:
        sh(["gh", "issue", "edit", str(num), "--repo", CFG["repo"], "--remove-label",
            CFG["labels"]["paused"]], check=False)
    try:
        git("fetch", "-q", "origin", CFG["base"])
        red_ack_set(git("rev-parse", "origin/" + CFG["base"]).strip())
    except RuntimeError:
        pass
    st.pop("paused", None)
    save_state(st)
    log("resumed (a red CI on the current dev commit will not pause again)")
    return 0


def cmd_status():
    st = load_state()
    print("mode:", st.get("mode"), "| last pass:", st.get("last_pass"), "| paused:", paused_reason(st))
    for q in fetch_queue():
        ps = st.get("prs", {}).get(str(q["number"]), {})
        print("#%d %-40s queued %s  %s: %s" % (q["number"], q["headRefName"], q["queued_at"],
                                               ps.get("decision", "-"), ps.get("detail", "")))
    return 0


def main(argv=None):
    global ARGS, CFG, LEADERS
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["run", "board", "setup", "pause", "resume", "status"])
    ap.add_argument("--dry-run", action="store_true", help="print what would be done; change nothing")
    ap.add_argument("--once", action="store_true", help="run: one pass, then exit")
    ap.add_argument("--simulate", type=int, nargs="*", default=[],
                    help="dry-run only: treat these PR numbers as if labelled merge-ready")
    ap.add_argument("--ignore-draft", action="store_true",
                    help="dry-run only: evaluate Draft PRs as if they were Ready")
    ap.add_argument("--reason", default="", help="pause: reason shown on the board")
    ap.add_argument("--config", default=os.path.join(HERE, "config.toml"))
    ap.add_argument("--leaders", default=os.path.join(HERE, "leaders.toml"))
    ARGS = ap.parse_args(argv)
    if (ARGS.simulate or ARGS.ignore_draft) and not ARGS.dry_run:
        ap.error("--simulate/--ignore-draft need --dry-run")
    CFG = load_toml(ARGS.config)
    LEADERS = load_toml(ARGS.leaders)
    return {"run": cmd_run, "board": cmd_board, "setup": cmd_setup, "pause": cmd_pause,
            "resume": cmd_resume, "status": cmd_status}[ARGS.command]()


if __name__ == "__main__":
    sys.exit(main())
