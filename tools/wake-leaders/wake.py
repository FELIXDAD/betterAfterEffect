#!/usr/bin/env python3
"""Wake Claude sessions that stopped on a usage limit once the window has reset.

Subcommands:
  check  [--json]            read-only; exit 0 if some watched session needs a wake-up
  wake   [--only NAME] [--force]   nudge every session that needs it, then verify
  verify NAME                check that NAME is moving again (after a manual nudge)
  status                     print per-session state (never changes anything)
  reset  [NAME]              clear the failure counter / halt flag by hand

Everything is read-only except `wake`, `verify`, `reset` which write the state
file and log under state_dir. Python stdlib only.
"""
import argparse, glob, json, os, re, subprocess, sys, time
from datetime import datetime, timedelta, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
TAIL_BYTES = 8 * 1024 * 1024  # enough to reach the last conversation record
LIMIT_RE = re.compile(r"hit your .*?limit|usage limit reached|limit reached", re.I)
MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
WDAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
RESET_RE = re.compile(
    r"resets\s+(?:(?P<wd>mon|tue|wed|thu|fri|sat|sun)[a-z]*\s+"
    r"|(?P<mon>jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?(?:,|\s+at)?\s+)?"
    r"(?P<h>\d{1,2})(?::(?P<m>\d{2}))?\s*(?P<ap>[ap]m)?\s*(?:\((?P<tz>[^)]+)\))?",
    re.I,
)


def now():
    return float(os.environ.get("WAKE_NOW") or time.time())


def load_config():
    path = os.environ.get("WAKE_CONFIG") or os.path.join(HERE, "config.json")
    with open(path) as f:
        cfg = json.load(f)
    for k in ("projects_dir", "state_dir"):
        cfg[k] = os.path.expanduser(os.environ.get("WAKE_" + k.upper()) or cfg[k])
    cfg["orca_bin"] = os.environ.get("WAKE_ORCA_BIN") or cfg.get("orca_bin", "orca")
    return cfg


# ---------------------------------------------------------------- state / log
def state_path(cfg):
    return os.path.join(cfg["state_dir"], "state.json")


def load_state(cfg):
    try:
        with open(state_path(cfg)) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_state(cfg, st):
    os.makedirs(cfg["state_dir"], exist_ok=True)
    tmp = state_path(cfg) + ".tmp"
    with open(tmp, "w") as f:
        json.dump(st, f, indent=1, sort_keys=True)
    os.replace(tmp, state_path(cfg))


def log(cfg, event, **kw):
    os.makedirs(cfg["state_dir"], exist_ok=True)
    p = os.path.join(cfg["state_dir"], "wake.log")
    try:
        if os.path.getsize(p) > 1_000_000:
            os.replace(p, p + ".1")
    except OSError:
        pass
    rec = {"ts": datetime.fromtimestamp(now(), timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), "event": event}
    rec.update(kw)
    line = json.dumps(rec, ensure_ascii=False)
    with open(p, "a") as f:
        f.write(line + "\n")
    return line


# ----------------------------------------------------------------- transcript
def transcript_path(cfg, target):
    hits = glob.glob(os.path.join(cfg["projects_dir"], target["claude_session"] + "*.jsonl"))
    return hits[0] if hits else None


def parse_ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()


def last_conversation_record(path):
    """Newest user/assistant record in the main chain, or None."""
    size = os.path.getsize(path)
    with open(path, "rb") as f:
        f.seek(max(0, size - TAIL_BYTES))
        data = f.read()
    lines = data.split(b"\n")
    if size > TAIL_BYTES:
        lines = lines[1:]  # first line is cut off
    for raw in reversed(lines):
        if b'"type":"assistant"' not in raw and b'"type":"user"' not in raw:
            continue
        try:
            d = json.loads(raw)
        except ValueError:
            continue
        if d.get("type") in ("assistant", "user") and not d.get("isSidechain"):
            return d
    return None


def record_text(d):
    c = (d.get("message") or {}).get("content")
    if isinstance(c, str):
        return c
    if isinstance(c, list):
        return " ".join(x.get("text", "") for x in c if isinstance(x, dict) and x.get("type") == "text")
    return ""


def is_limit_error(d):
    if d.get("type") != "assistant" or not d.get("isApiErrorMessage"):
        return False
    text = record_text(d)
    return d.get("error") == "rate_limit" and bool(LIMIT_RE.search(text)) or bool(
        LIMIT_RE.search(text) and "resets" in text
    )


def parse_reset(text, ts, fallback_s):
    """Epoch when the window named in the error text reopens. Returns (epoch, how)."""
    m = re.search(r"limit reached\|(\d{9,10})", text)
    if m:
        return float(m.group(1)), "epoch"
    m = RESET_RE.search(text)
    if m and (m.group("ap") or m.group("m") or m.group("tz")):
        try:
            from zoneinfo import ZoneInfo
            tz = ZoneInfo(m.group("tz")) if m.group("tz") else datetime.now().astimezone().tzinfo
            base = datetime.fromtimestamp(ts, tz)
            h = int(m.group("h")) % (12 if m.group("ap") else 24)
            if m.group("ap") and m.group("ap").lower() == "pm":
                h += 12
            mi = int(m.group("m") or 0)
            cand = base.replace(hour=h, minute=mi, second=0, microsecond=0)
            if m.group("mon"):
                cand = cand.replace(month=MONTHS.index(m.group("mon").lower()[:3]) + 1, day=int(m.group("day")))
                if cand <= base:
                    cand = cand.replace(year=cand.year + 1)
            elif m.group("wd"):
                delta = (WDAYS.index(m.group("wd").lower()[:3]) - cand.weekday()) % 7
                cand += timedelta(days=delta)
                if cand <= base:
                    cand += timedelta(days=7)
            elif cand <= base:
                cand += timedelta(days=1)
            return cand.timestamp(), "text"
        except Exception:
            pass
    return ts + fallback_s, "fallback"


def inspect(cfg, target):
    """Describe one watched session. Never raises."""
    info = {"name": target["name"], "stalled": False}
    path = transcript_path(cfg, target)
    if not path:
        info["why"] = "no transcript"
        return info
    info["path"] = path
    info["idle_s"] = round(now() - os.path.getmtime(path))
    try:
        rec = last_conversation_record(path)
    except OSError as e:
        info["why"] = "read error: %s" % e
        return info
    if rec is None:
        info["why"] = "no conversation records"
        return info
    info["last_uuid"] = rec.get("uuid")
    info["last_ts"] = rec.get("timestamp")
    if not is_limit_error(rec):
        info["why"] = "last record is not a limit error"
        return info
    text = record_text(rec)
    ts = parse_ts(rec["timestamp"])
    reset_at, how = parse_reset(text, ts, cfg["fallback_window_s"])
    info.update(stalled=True, error_text=text, error_ts=ts, reset_at=reset_at, reset_how=how)
    return info


# --------------------------------------------------------------------- orca
def orca_json(cfg, *args, timeout=20):
    p = subprocess.run([cfg["orca_bin"], *args, "--json"], capture_output=True, text=True, timeout=timeout)
    if p.returncode != 0:
        raise RuntimeError("orca %s failed: %s" % (" ".join(args), (p.stderr or p.stdout).strip()[:300]))
    return json.loads(p.stdout)["result"]


def quota_gate(cfg, t_now):
    """(ok, reason). Codex weekly < 100% (never touch extraUsage); Claude not known-exhausted."""
    try:
        rl = orca_json(cfg, "account", "list")["rateLimits"]
    except Exception as e:  # fail closed: no reading, no Codex run
        return False, "cannot read account usage: %s" % e
    cx = rl.get("codex") or {}
    wk = cx.get("weekly") or {}
    if cx.get("status") != "ok" or wk.get("usedPercent") is None:
        return False, "codex usage unavailable (status=%s)" % cx.get("status")
    if wk["usedPercent"] >= cfg["codex_weekly_max_percent"]:
        return False, "codex weekly at %s%% (resets %s); extraUsage is not touched" % (wk["usedPercent"], wk.get("resetDescription"))
    cl = rl.get("claude") or {}
    if cl.get("status") == "ok":  # when the API is rate limited we trust the error text only
        for key in ("session", "weekly"):
            w = cl.get(key) or {}
            if (w.get("usedPercent") or 0) >= 100 and (w.get("resetsAt") or 0) / 1000 > t_now:
                return False, "claude %s still at %s%% until %s" % (key, w["usedPercent"], w.get("resetDescription"))
    return True, "codex weekly %s%%" % wk["usedPercent"]


# ------------------------------------------------------------------ decisions
def evaluate(cfg, only=None, force=False, with_gate=True):
    """Return (list of targets to wake, list of per-target report dicts)."""
    st, t = load_state(cfg), now()
    reports, wanted = [], []
    for target in cfg["watch"]:
        if only and target["name"] != only:
            continue
        info = inspect(cfg, target)
        s = st.setdefault(target["name"], {})
        info["action"] = "none"
        if not info["stalled"]:
            if s.get("failures") or s.get("halted"):
                s.update(failures=0, halted=False)  # healthy again: whoever fixed it, clear the books
            reports.append(info)
            continue
        if not force:
            if s.get("halted"):
                info["action"] = "skip: halted after %d failed wakes (run wake.sh reset %s)" % (s.get("failures", 0), target["name"])
            elif t < info["reset_at"] + cfg["reset_grace_s"]:
                info["action"] = "skip: window not reset yet (%s)" % datetime.fromtimestamp(info["reset_at"]).astimezone().strftime("%m-%d %H:%M %Z")
            elif info["idle_s"] < cfg["idle_s"]:
                info["action"] = "skip: transcript changed %ds ago, maybe resuming itself" % info["idle_s"]
            elif t - s.get("last_wake_ts", 0) < cfg["min_wake_interval_s"]:
                info["action"] = "skip: woken %ds ago" % (t - s["last_wake_ts"])
        if info["action"] == "none":
            wanted.append(target)
            info["action"] = "wake"
        reports.append(info)
    if wanted and with_gate and not force:
        ok, why = quota_gate(cfg, t)
        for r in reports:
            if r["action"] == "wake":
                r["gate"] = why
                if not ok:
                    r["action"] = "skip: " + why
        if not ok:
            wanted = []
    save_state(cfg, st)
    return wanted, reports


def cmd_check(cfg, args):
    wanted, reports = evaluate(cfg)
    for r in reports:
        if r["stalled"] or args.verbose:
            log(cfg, "check", **{k: v for k, v in r.items() if k not in ("path",)})
    summary = {"wake": [w["name"] for w in wanted], "sessions": [
        {k: r.get(k) for k in ("name", "stalled", "action", "why", "reset_at", "reset_how") if r.get(k) is not None} for r in reports]}
    print(json.dumps(summary, ensure_ascii=False))
    return 0 if wanted else 1


def send_nudge(cfg, target, info):
    reset = datetime.fromtimestamp(info["reset_at"]).astimezone().strftime("%m-%d %H:%M %Z")
    body = ("[自动唤醒] 你因为用量上限（\"%s\"）停住了，窗口已在 %s 恢复。"
            "请继续刚才被中断的工作；先看一眼自己的最近进度和未回复的消息，再接着做。"
            "如果还是被限流，不用回复，等下一次唤醒。" % (info["error_text"], reset))
    p = subprocess.run(
        [cfg["orca_bin"], "orchestration", "send", "--to", "orca_session_id:" + target["orca_session"],
         "--subject", "自动唤醒：用量窗口已恢复，请继续", "--body", body, "--type", "status", "--json"],
        capture_output=True, text=True, timeout=30)
    return p.returncode == 0, (p.stdout + p.stderr).strip()[:300]


def orca_state(cfg, target):
    try:
        return orca_json(cfg, "agent", "show", "orca_session_id:" + target["orca_session"]).get("state")
    except Exception:
        return None


def wait_alive(cfg, target, stalled_uuid):
    """True once the transcript has a newer conversation record that is not a limit error."""
    path = transcript_path(cfg, target)
    deadline = time.time() + cfg["verify_timeout_s"]
    seen_user = False
    while True:
        rec = last_conversation_record(path) if path else None
        if rec and rec.get("uuid") != stalled_uuid:
            if is_limit_error(rec):
                return False, "limit error again"
            if rec.get("type") == "assistant":
                return True, "assistant record %s" % rec.get("timestamp")
            seen_user = True
        if time.time() > deadline:
            return False, "timeout (message %s)" % ("arrived, no reply" if seen_user else "not seen in transcript")
        time.sleep(float(os.environ.get("WAKE_POLL_S", 5)))


def lock(cfg):
    os.makedirs(cfg["state_dir"], exist_ok=True)
    d = os.path.join(cfg["state_dir"], "lock")
    try:
        if time.time() - os.path.getmtime(d) > 900:
            os.rmdir(d)
    except OSError:
        pass
    try:
        os.mkdir(d)
        return d
    except FileExistsError:
        return None


def record_result(cfg, name, ok, detail):
    st = load_state(cfg)
    s = st.setdefault(name, {})
    if ok:
        s.update(failures=0, halted=False)
    else:
        s["failures"] = s.get("failures", 0) + 1
        if s["failures"] >= cfg["max_failures"]:
            s["halted"] = True
    save_state(cfg, st)
    log(cfg, "wake_ok" if ok else "wake_failed", name=name, detail=detail,
        failures=s.get("failures", 0), halted=bool(s.get("halted")))


def cmd_wake(cfg, args):
    d = lock(cfg)
    if not d:
        print("another wake run is in progress")
        return 0
    try:
        wanted, reports = evaluate(cfg, only=args.only, force=args.force, with_gate=False)
        if not wanted:
            print(json.dumps([{k: r.get(k) for k in ("name", "action", "why")} for r in reports], ensure_ascii=False))
            return 0
        rc = 0
        for target in wanted:
            info = next(r for r in reports if r["name"] == target["name"])
            st = load_state(cfg)
            st.setdefault(target["name"], {})["last_wake_ts"] = now()
            save_state(cfg, st)
            ok, out = send_nudge(cfg, target, info)
            log(cfg, "sent", name=target["name"], ok=ok, out=out, orca_state=orca_state(cfg, target))
            if not ok:
                record_result(cfg, target["name"], False, "send failed: " + out)
                print("%s: SEND FAILED (%s)" % (target["name"], out))
                rc = 3
                continue
            alive, detail = wait_alive(cfg, target, info["last_uuid"])
            record_result(cfg, target["name"], alive, detail)
            print("%s: %s (%s)" % (target["name"], "AWAKE" if alive else "NOT AWAKE", detail))
            rc = rc or (0 if alive else 4)
        return rc
    finally:
        try:
            os.rmdir(d)
        except OSError:
            pass


def cmd_verify(cfg, args):
    target = next(w for w in cfg["watch"] if w["name"] == args.name)
    info = inspect(cfg, target)
    if not info["stalled"]:
        print("%s: not stalled (%s)" % (args.name, info.get("why")))
        return 0
    alive, detail = wait_alive(cfg, target, info["last_uuid"])
    record_result(cfg, args.name, alive, "verify: " + detail)
    print("%s: %s (%s)" % (args.name, "AWAKE" if alive else "NOT AWAKE", detail))
    return 0 if alive else 4


def cmd_status(cfg, args):
    st = load_state(cfg)
    for target in cfg["watch"]:
        info = inspect(cfg, target)
        info.pop("error_text", None)
        print(json.dumps({"state": st.get(target["name"], {}), **info}, ensure_ascii=False))
    return 0


def cmd_reset(cfg, args):
    st = load_state(cfg)
    for name in ([args.name] if args.name else [w["name"] for w in cfg["watch"]]):
        st[name] = {k: v for k, v in st.get(name, {}).items() if k == "last_wake_ts"}
    save_state(cfg, st)
    log(cfg, "reset", name=args.name or "all")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check"); c.add_argument("-v", "--verbose", action="store_true"); c.set_defaults(fn=cmd_check)
    w = sub.add_parser("wake"); w.add_argument("--only"); w.add_argument("--force", action="store_true"); w.set_defaults(fn=cmd_wake)
    v = sub.add_parser("verify"); v.add_argument("name"); v.set_defaults(fn=cmd_verify)
    sub.add_parser("status").set_defaults(fn=cmd_status)
    r = sub.add_parser("reset"); r.add_argument("name", nargs="?"); r.set_defaults(fn=cmd_reset)
    args = ap.parse_args()
    cfg = load_config()
    try:
        sys.exit(args.fn(cfg, args))
    except Exception as e:  # precheck must never wake on an unexpected error
        log(cfg, "error", cmd=args.cmd, error=repr(e))
        print("error: %r" % e, file=sys.stderr)
        sys.exit(2 if args.cmd == "check" else 1)


if __name__ == "__main__":
    main()
