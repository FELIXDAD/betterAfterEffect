#!/bin/bash
# Register the Orca automation. NOT run by the PR; ca3a6511 decides when.
# Usage: tools/wake-leaders/install.sh [--dry-run]   (WAKE_REPO=<dev worktree path> overrides the target)
set -eu
# The automation must point at the dev worktree (this branch's worktree goes away after the merge).
REPO="${WAKE_REPO:-/Users/a4/Documents/Orca/betterAfterEffect}"
SCRIPTS="$REPO/tools/wake-leaders"
[ -x "$SCRIPTS/precheck.sh" ] || { echo "missing $SCRIPTS/precheck.sh: merge this tool into dev first" >&2; exit 1; }
CMD=(orca automations create
  --name "wake-leaders"
  --trigger "7,37 * * * *"
  --precheck "bash $SCRIPTS/precheck.sh"
  --precheck-timeout 30
  --provider codex --model gpt-6.2-sol --effort low
  --workspace "path:$REPO"
  --fresh-session
  --prompt "$(cat "$SCRIPTS/automation-prompt.txt")"
  --json)
if [ "${1:-}" = "--dry-run" ]; then printf '%q ' "${CMD[@]}"; echo; exit 0; fi
"${CMD[@]}"
