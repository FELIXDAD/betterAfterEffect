#!/bin/bash
set -euo pipefail
runner="$HOME/Library/Application Support/bae-progress-sync"
plist="$HOME/Library/LaunchAgents/com.bae.progress-sync.plist"
mkdir -p "$runner" "$(dirname "$plist")" "$HOME/Library/Logs"
install -m 700 "$(dirname "$0")/sync.py" "$runner/sync.py"
/usr/bin/python3 - "$runner" "$plist" "$(dirname "$(command -v gh)")" <<'PY'
import plistlib,sys
from pathlib import Path
runner, plist, gh_bin = sys.argv[1:]
log = str(Path.home() / "Library/Logs/bae-progress-sync.log")
job = dict(Label="com.bae.progress-sync", WorkingDirectory=runner, StartInterval=300,
           RunAtLoad=True, ProgramArguments=["/usr/bin/python3",runner+"/sync.py"],
           EnvironmentVariables=dict(PATH=gh_bin+":/usr/bin:/bin", BAE_BOARD_LOCAL_MIRROR="http://127.0.0.1:8787"),
           StandardOutPath=log, StandardErrorPath=log)
with open(plist,"wb") as f:
    plistlib.dump(job,f)
PY
launchctl bootout "gui/$(id -u)/com.bae.progress-sync" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$plist"
