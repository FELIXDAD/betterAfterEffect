#!/bin/bash
# Install the merge train and the progress board as launchd agents. See README.md.
#   install.sh [--dry-run] [--ref <git ref>] [--no-board]
#   --dry-run   merge train only logs what it would do (no merges, comments or messages)
#   --ref       ref to check the runner worktree out at first (before this tool is on dev:
#               origin/tools/merge-train); afterwards the runner follows runner_ref itself
#   --no-board  do not install the progress-board job
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY=0; REF=""; BOARD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY=1 ;;
    --ref) REF="$2"; shift ;;
    --no-board) BOARD=0 ;;
    -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

cfg() { sed -n "s/^$1 = \"\(.*\)\"/\1/p" "$HERE/config.toml" | head -1; }
RUNNER="$(cfg runner_dir)"; RUNNER="${RUNNER/#\~/$HOME}"
REF="${REF:-$(cfg runner_ref)}"
REPO="$(git -C "$HERE" rev-parse --path-format=absolute --git-common-dir)"
REPO="$(dirname "$REPO")"

git -C "$REPO" fetch -q origin
if [ ! -e "$RUNNER" ]; then
  echo "creating runner worktree $RUNNER at $REF"
  git -C "$REPO" worktree add --detach "$RUNNER" "$REF"
else
  echo "runner worktree exists: $RUNNER; checking out $REF"
  git -C "$RUNNER" checkout -q --detach "$REF"
fi
test -f "$RUNNER/tools/merge-train/merge_train.py" || {
  echo "$REF has no tools/merge-train/merge_train.py; pass --ref origin/tools/merge-train" >&2; exit 1; }

AGENTS="$HOME/Library/LaunchAgents"
mkdir -p "$AGENTS" "$HOME/Library/Logs"
EXTRA=""
[ "$DRY" = 1 ] && EXTRA=$'\n    <string>--dry-run</string>'

render() {  # template -> installed plist
  local src="$1" dst="$AGENTS/$(basename "$1")"
  python3 - "$src" "$dst" "$RUNNER" "$HOME" "$EXTRA" <<'PY'
import sys
src, dst, runner, home, extra = sys.argv[1:]
s = open(src).read().replace("__RUNNER__", runner).replace("__HOME__", home)
s = s.replace("__EXTRA_ARGS__", extra)
open(dst, "w").write(s)
PY
  plutil -lint "$dst" >/dev/null
  launchctl bootout "gui/$(id -u)" "$dst" 2>/dev/null || true
  launchctl bootstrap "gui/$(id -u)" "$dst"
  echo "loaded $(basename "$dst")"
}

echo "creating labels (merge-ready, merge-train-paused, blocked, reference-data) if missing"
/usr/bin/python3 "$RUNNER/tools/merge-train/merge_train.py" setup || true

render "$RUNNER/tools/merge-train/launchd/com.bae.merge-train.plist"
[ "$BOARD" = 1 ] && render "$RUNNER/tools/merge-train/launchd/com.bae.progress-board.plist"
echo "done. mode: $([ "$DRY" = 1 ] && echo dry-run || echo live). log: ~/Library/Logs/bae-merge-train.log"
