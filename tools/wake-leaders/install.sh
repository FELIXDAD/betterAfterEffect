#!/bin/bash
# Install wake-leaders. NOT run by the PR; ca3a6511 decides when.
# Usage: tools/wake-leaders/install.sh [--launchd] [--dry-run]
#   (default)  register the Orca automation (Codex runs wake.sh wake when precheck passes)
#   --launchd  instead install the model-free launchd job (com.bae.wake-leaders)
#   --dry-run  print what would be done
# Both first copy this tool to ~/Library/Application Support/bae-wake-leaders/tools/wake-leaders
# (WAKE_HOME overrides the base dir): background jobs cannot read ~/Documents (macOS privacy
# protection), so the scripts must not run from the checkout. Rerun after updating the tool.
set -eu
SRC="$(cd "$(dirname "$0")" && pwd)"
WAKE_HOME="${WAKE_HOME:-$HOME/Library/Application Support/bae-wake-leaders}"
SCRIPTS="$WAKE_HOME/tools/wake-leaders"
# The Codex session of the automation works in the dev worktree.
REPO="${WAKE_REPO:-/Users/a4/Documents/Orca/betterAfterEffect}"
MODE=automation; DRY=0
for a in "$@"; do
  case "$a" in
    --launchd) MODE=launchd ;;
    --dry-run) DRY=1 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
run() { if [ "$DRY" = 1 ]; then printf '%q ' "$@"; echo; else "$@"; fi; }

run mkdir -p "$SCRIPTS"
run rsync -a --delete --exclude .DS_Store "$SRC/" "$SCRIPTS/"

if [ "$MODE" = launchd ]; then
  PLIST="$HOME/Library/LaunchAgents/com.bae.wake-leaders.plist"
  if [ "$DRY" = 1 ]; then echo "render $SRC/com.bae.wake-leaders.plist.example -> $PLIST"; else
    mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
    python3 - "$SRC/com.bae.wake-leaders.plist.example" "$PLIST" "$SCRIPTS" "$HOME" <<'PY'
import sys
src, dst, scripts, home = sys.argv[1:]
s = open(src).read().replace("__SCRIPTS__", scripts).replace("__HOME__", home)
open(dst, "w").write(s)
PY
    plutil -lint "$PLIST" >/dev/null
    launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
    launchctl bootstrap "gui/$(id -u)" "$PLIST"
    echo "loaded $PLIST (scripts in $SCRIPTS)"
  fi
  exit 0
fi

CMD=(orca automations create
  --name "wake-leaders"
  --trigger "7,37 * * * *"
  --precheck "bash '$SCRIPTS/precheck.sh'"
  --precheck-timeout 30
  --provider codex --model gpt-6.2-sol --effort low
  --workspace "path:$REPO"
  --fresh-session
  --prompt "$(sed -E "s#tools/wake-leaders/([A-Za-z.]+)#\"$SCRIPTS/\1\"#g" "$SRC/automation-prompt.txt")"
  --json)
run "${CMD[@]}"
