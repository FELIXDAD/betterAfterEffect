#!/bin/bash
# Rehearsal with fake transcripts and a fake orca CLI. Touches no real session.
# Usage: test/drill.sh        (prints PASS/FAIL per step, exit 0 if all pass)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"; TOOL="$(dirname "$HERE")"
W="$(mktemp -d /private/tmp/wake-drill.XXXXXX)"
mkdir -p "$W/projects" "$W/state" "$W/bin"
cp "$HERE/fake-orca.sh" "$W/bin/orca"; chmod +x "$W/bin/orca"
export FAKE_PROJECTS="$W/projects" FAKE_CALLS="$W/calls.txt" FAKE_ACCOUNT_JSON="$W/account.json"
export WAKE_PROJECTS_DIR="$W/projects" WAKE_STATE_DIR="$W/state" WAKE_ORCA_BIN="$W/bin/orca" WAKE_POLL_S=0.2
: > "$FAKE_CALLS"
python3 -I - "$W" "$TOOL/config.json" <<'PY'
import json,sys
w,cfgp=sys.argv[1:]
cfg=json.load(open(cfgp)); cfg["verify_timeout_s"]=3; cfg["idle_s"]=0
json.dump(cfg,open(w+"/config.json","w"))
PY
export WAKE_CONFIG="$W/config.json"
T0=1795600000   # fixed "now" so reset times are deterministic; see WAKE_NOW
iso() { python3 -I -c "import sys,datetime;print(datetime.datetime.fromtimestamp($1,datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.000Z'))"; }
fixture() { # name claude_session orca_session mode(stalled|healthy) error_ts
  local f="$W/projects/$2.jsonl"
  echo "{\"orca_session\":\"$3\"}" > "$W/projects/$2.meta"
  { echo "{\"type\":\"user\",\"uuid\":\"u0\",\"timestamp\":\"$(iso $((T0-9000)))\",\"message\":{\"role\":\"user\",\"content\":\"work\"}}"
    echo "{\"type\":\"assistant\",\"uuid\":\"a0\",\"timestamp\":\"$(iso $((T0-8000)))\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}"
    if [ "$4" = stalled ]; then
      echo "{\"type\":\"assistant\",\"uuid\":\"err-$2\",\"timestamp\":\"$5\",\"isApiErrorMessage\":true,\"error\":\"rate_limit\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"You've hit your 5-hour limit · resets 4:10pm (America/Los_Angeles)\"}]}}"
      echo '{"type":"system","uuid":"s1","timestamp":"'$5'"}'      # trailing metadata must be ignored
      echo '{"type":"attachment","uuid":"s2","timestamp":"'$5'"}'
    fi; } > "$f"
}
account() { # codex_percent
  cat > "$FAKE_ACCOUNT_JSON" <<J
{"ok":true,"result":{"rateLimits":{"claude":{"status":"error","error":"rate limited"},
"codex":{"status":"ok","weekly":{"usedPercent":$1,"resetDescription":"Tue 1:40 AM"},"extraUsage":{"enabled":true,"balance":1342}}}}}
J
}
fails=0
expect() { # description expected_exit actual_exit
  if [ "$2" = "$3" ]; then echo "PASS  $1 (exit $3)"; else echo "FAIL  $1: wanted exit $2, got $3"; fails=$((fails+1)); fi
}
run() { WAKE_NOW=$1 "$TOOL/precheck.sh" >"$W/out.txt" 2>&1; echo $?; }

# Error at 2026-11-24 ~07:00Z-equivalent: pick a stamp, window reopens 16:10 PST = 00:10Z next day.
ERR_TS=2026-11-24T20:00:00.000Z          # 12:00 PST on 11-24 -> resets 16:10 PST = 2026-11-25T00:10Z
RESET=$(python3 -I -c "import datetime;print(int(datetime.datetime(2026,11,25,0,10,tzinfo=datetime.timezone.utc).timestamp()))")
fixture ca3a6511 8e562540-abf8-4e46-8c36-a0bc08e760e0 ca3a6511-c6a8-4cd9-aca7-379637d84b6f stalled $ERR_TS
fixture 09fe9cc0 2f5efdb3-f481-4946-a013-5dc9a146956f 09fe9cc0-76f2-4ae5-934c-b33f04bc0c0b healthy x
account 40

echo "--- precheck decisions"
expect "stalled but window not reset yet -> no wake"   1 "$(run $((RESET-600)))"
expect "stalled, window reset 1 min ago (grace) -> no wake" 1 "$(run $((RESET+60)))"
expect "stalled, window reset 5 min ago -> WAKE"       0 "$(run $((RESET+300)))"; cat "$W/out.txt"
account 100
expect "codex weekly 100% -> no wake (extraUsage untouched)" 1 "$(run $((RESET+300)))"; cat "$W/out.txt"
account 40
mv "$W/state/state.json" "$W/state.bak"; echo '{"ca3a6511":{"halted":true,"failures":2}}' > "$W/state/state.json"
expect "halted after 2 failures -> no wake"            1 "$(run $((RESET+300)))"
rm "$W/state/state.json"
echo '{"ca3a6511":{"last_wake_ts":'$((RESET+200))'}}' > "$W/state/state.json"
expect "woken 100 s ago -> no wake (rate limit)"       1 "$(run $((RESET+300)))"
rm "$W/state/state.json"
fixture 09fe9cc0 2f5efdb3-f481-4946-a013-5dc9a146956f 09fe9cc0-76f2-4ae5-934c-b33f04bc0c0b healthy x
expect "garbage on the line / bad config path -> no wake (fail closed)" 1 "$(WAKE_CONFIG=/nonexistent run $((RESET+300)))"

echo "--- wake action: session comes back"
WAKE_NOW=$((RESET+300)) FAKE_MODE=alive "$TOOL/wake.sh" wake; expect "wake.sh wake, session resumes" 0 $?
grep -c "orchestration send --to orca_session_id:ca3a6511-c6a8-4cd9-aca7-379637d84b6f" "$FAKE_CALLS" | sed 's/^/      nudges sent to ca3a6511: /'
grep -q "09fe9cc0" "$FAKE_CALLS" && { echo "FAIL  healthy session was nudged"; fails=$((fails+1)); } || echo "PASS  healthy 09fe9cc0 not touched"
expect "after waking: precheck says nothing to do"     1 "$(run $((RESET+400)))"

echo "--- wake action: two failures stop the automation"
fixture ca3a6511 8e562540-abf8-4e46-8c36-a0bc08e760e0 ca3a6511-c6a8-4cd9-aca7-379637d84b6f stalled $ERR_TS
rm -f "$W/state/state.json"
WAKE_NOW=$((RESET+300)) FAKE_MODE=limited "$TOOL/wake.sh" wake; expect "nudge but limited again -> exit 4" 4 $?
expect "1 failure, 5 min later still inside 29 min -> no wake" 1 "$(run $((RESET+600)))"
fixture ca3a6511 8e562540-abf8-4e46-8c36-a0bc08e760e0 ca3a6511-c6a8-4cd9-aca7-379637d84b6f stalled $ERR_TS
WAKE_NOW=$((RESET+2200)) FAKE_MODE=dead "$TOOL/wake.sh" wake; expect "second failure (send fails) -> exit 3" 3 $?
expect "halted -> precheck no longer wakes"            1 "$(run $((RESET+4500)))"
python3 -I -c "import json;print('      state:',json.load(open('$W/state/state.json')))"
"$TOOL/wake.sh" reset ca3a6511
expect "after 'wake.sh reset' it may wake again"       0 "$(run $((RESET+4500)))"

echo "--- log"; cut -c1-200 "$W/state/wake.log" | tail -8
echo; [ $fails = 0 ] && echo "DRILL PASS" || echo "DRILL FAIL ($fails)"
[ $fails = 0 ] && rm -rf "$W" || echo "(workdir kept: $W)"
exit $fails
