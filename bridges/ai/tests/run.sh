#!/bin/sh
# Test bae-ai-export.jsx without Illustrator (mock DOM, ES3 builtins only), with node if
# installed, else macOS jsc. Usage: bridges/ai/tests/run.sh
# `cargo test -p bae-interop-ai --test export_script` runs the same harness (CI does).
# Pass/fail comes from the summary line: jsc's quit() always exits 0.
set -e
cd "$(dirname "$0")"
JSC=/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc
if command -v node >/dev/null 2>&1; then
    out=$(node node-run.js harness.js 2>&1) || true
elif [ -x "$JSC" ]; then
    out=$("$JSC" harness.js 2>&1) || true
else
    echo "need node or macOS jsc" >&2
    exit 2
fi
printf '%s\n' "$out"
printf '%s\n' "$out" | grep -q ' checks, 0 failed$'
