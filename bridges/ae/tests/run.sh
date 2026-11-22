#!/bin/sh
# Run the ExtendScript tests outside After Effects, with node if installed, else macOS jsc.
# Usage: bridges/ae/tests/run.sh [--update]
set -e
cd "$(dirname "$0")"
JSC=/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc
for f in harness.js anim-harness.js; do
    [ -f "$f" ] || continue
    echo "== $f"
    if command -v node >/dev/null 2>&1; then
        node node-run.js "$f" "$@"
    elif [ -x "$JSC" ]; then
        "$JSC" "$f" -- "$@"
    else
        echo "need node or macOS jsc to run $f" >&2
        exit 2
    fi
done
