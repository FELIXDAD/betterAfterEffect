#!/bin/bash
# Install the merge train and the progress board as launchd agents. See README.md.
#   install.sh [--dry-run] [--ref <git ref>] [--no-board]
#   --dry-run   merge train only logs what it would do (no merges, comments or messages)
#   --ref       ref to check the runner clone out at first (default: runner_ref, origin/dev);
#               afterwards the runner follows runner_ref by itself
#   --no-board  do not install the progress-board job
#
# The runner is a separate full clone under ~/Library/Application Support: launchd jobs
# cannot read ~/Documents (macOS privacy protection), so neither the main checkout nor a
# worktree of it (whose objects stay in ~/Documents/.../.git) works.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY=0; REF=""; BOARD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY=1 ;;
    --ref) REF="$2"; shift ;;
    --no-board) BOARD=0 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

cfg() { sed -n "s/^$1 = \"\(.*\)\"/\1/p" "$HERE/config.toml" | head -1; }
RUNNER="$(cfg runner_dir)"; RUNNER="${RUNNER/#\~/$HOME}"
REF="${REF:-$(cfg runner_ref)}"
URL="$(git -C "$HERE" remote get-url origin)"
case "$RUNNER" in "$HOME/Documents"*|"$HOME/Desktop"*|"$HOME/Downloads"*)
  echo "runner_dir $RUNNER is in a folder launchd jobs cannot read; use ~/Library/..." >&2; exit 1 ;;
esac

if [ -f "$RUNNER/.git" ]; then
  echo "$RUNNER is a worktree (its objects live in the main checkout); remove it first:" >&2
  echo "  git worktree remove \"$RUNNER\"" >&2; exit 1
elif [ ! -d "$RUNNER/.git" ]; then
  echo "cloning $URL into $RUNNER"
  mkdir -p "$(dirname "$RUNNER")"
  git clone -q "$URL" "$RUNNER"
fi
git -C "$RUNNER" fetch -q --prune origin
git -C "$RUNNER" checkout -q --detach "$REF"
echo "runner clone $RUNNER at $(git -C "$RUNNER" rev-parse --short HEAD) ($REF)"
test -f "$RUNNER/tools/merge-train/merge_train.py" || {
  echo "$REF has no tools/merge-train/merge_train.py" >&2; exit 1; }

AGENTS="$HOME/Library/LaunchAgents"
mkdir -p "$AGENTS" "$HOME/Library/Logs"
EXTRA=""
[ "$DRY" = 1 ] && EXTRA=$'\n    <string>--dry-run</string>'

render() {  # template -> installed plist
  local src="$1" dst
  dst="$AGENTS/$(basename "$1")"
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
if [ "$BOARD" = 1 ]; then
  render "$RUNNER/tools/merge-train/launchd/com.bae.progress-board.plist"
else
  launchctl bootout "gui/$(id -u)/com.bae.progress-board" 2>/dev/null || true
  rm -f "$AGENTS/com.bae.progress-board.plist"
fi
echo "done. mode: $([ "$DRY" = 1 ] && echo dry-run || echo live). log: ~/Library/Logs/bae-merge-train.log"
echo "run once now: launchctl kickstart -k gui/$(id -u)/com.bae.merge-train"
