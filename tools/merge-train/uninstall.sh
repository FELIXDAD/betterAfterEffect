#!/bin/bash
# Stop and remove the merge-train and progress-board launchd agents. See README.md.
# The runner worktree, state (~/Library/Application Support/bae-merge-train) and log stay.
set -uo pipefail
for label in com.bae.merge-train com.bae.progress-board; do
  plist="$HOME/Library/LaunchAgents/$label.plist"
  launchctl bootout "gui/$(id -u)/$label" 2>/dev/null && echo "stopped $label"
  [ -f "$plist" ] && rm "$plist" && echo "removed $plist"
done
exit 0
