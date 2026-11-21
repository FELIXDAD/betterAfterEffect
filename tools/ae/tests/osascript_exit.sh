#!/usr/bin/env bash
# Regression test for aectl: when osascript fails before the script runs (Apple event
# not delivered, refused, osascript error), aectl must report it at once with exit
# code 5 and osascript's message — not wait for --timeout while AE sits idle.
# Needs After Effects installed; aectl starts it if necessary.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
AECTL="$HERE/../aectl"
fake=$(mktemp -t fake-osascript)
cat > "$fake" <<'FAKE'
#!/bin/sh
echo "execution error: Adobe After Effects 2026 got an error: simulated failure (-1708)" >&2
exit 1
FAKE
chmod +x "$fake"
"$AECTL" start >/dev/null 2>&1 || { echo "SKIP: After Effects could not be started"; rm -f "$fake"; exit 0; }
t0=$(date +%s)
out=$(BAE_AE_OSASCRIPT="$fake" "$AECTL" eval '"never runs"' --timeout 300)
rc=$?
dt=$(( $(date +%s) - t0 ))
echo "$out"
[ "$rc" -eq 5 ] || { echo "FAIL: expected exit code 5, got $rc"; exit 1; }
grep -q 'simulated failure' <<<"$out" || { echo "FAIL: osascript's error message is missing"; exit 1; }
[ "$dt" -lt 30 ] || { echo "FAIL: took ${dt}s, it should not wait for the 300 s timeout"; exit 1; }
# An Apple event timeout (-1712) means AE is not answering: it must be handled like a
# timeout (exit code 4, AE restarted), not reported as an early failure.
cat > "$fake" <<'FAKE'
#!/bin/sh
echo "87:162: execution error: Adobe After Effects 2026 got an error: AppleEvent timed out. (-1712)" >&2
exit 1
FAKE
chmod +x "$fake"
out=$(BAE_AE_OSASCRIPT="$fake" "$AECTL" eval '"never runs"' --timeout 300)
rc=$?
rm -f "$fake"
echo "$out"
[ "$rc" -eq 4 ] || { echo "FAIL: -1712 should give exit code 4 (timeout + restart), got $rc"; exit 1; }
grep -q 'restarted' <<<"$out" || { echo "FAIL: AE was not restarted after -1712"; exit 1; }
"$AECTL" eval '"ok"' --timeout 60 | grep -q '"ok":true' || { echo "FAIL: a normal eval no longer works"; exit 1; }
echo "PASS (failure reported in ${dt}s)"
