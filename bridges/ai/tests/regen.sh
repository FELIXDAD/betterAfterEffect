#!/bin/sh
# Regenerate the Illustrator test data of workstream 16 in real Illustrator (macOS):
# build the fixture documents (make-fixtures.jsx), save them as .ai and export each one
# with bae-ai-export.jsx. Output: crates/bae-interop-ai/tests/data/illustrator/.
#   bridges/ai/tests/regen.sh [fixture-name]
#
# Illustrator only ever sees a staging folder under /private/tmp: the repository usually
# lives in ~/Documents (or ~/Desktop), which macOS guards with a privacy prompt that
# Illustrator has no grant for, and a prompt would stall the unattended run.
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd -P)
out="$root/crates/bae-interop-ai/tests/data/illustrator"
aictl="$root/tools/ai/aictl"
stage=$(mktemp -d /private/tmp/bae-ai-regen.XXXXXX)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/out" "$out"
cp "$root/bridges/ai/bae-ai-export.jsx" "$root/bridges/ai/tests/make-fixtures.jsx" \
   "$root/bridges/ai/tests/export-open.jsx" "$stage/"
only=""
[ $# -ge 1 ] && only="var BAE_FIXTURE_ONLY = \"$1\";"
"$aictl" eval "var BAE_FIXTURE_DIR = \"$stage/out\"; $only \$.evalFile(\"$stage/make-fixtures.jsx\");" --timeout 300
"$aictl" eval "var BAE_EXPORT_DIR = \"$stage/out\"; var BAE_AI_EXPORT_SCRIPT = \"$stage/bae-ai-export.jsx\"; \$.evalFile(\"$stage/export-open.jsx\");" --timeout 300
cp "$stage/out/"*.ai "$stage/out/"*.bae-ai.json "$out/"
ls -l "$out"
