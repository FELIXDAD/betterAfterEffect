#!/bin/bash
# Read-only. Exit 0 = some watched leader session needs a wake-up, anything else = no.
export PATH="/Applications/Orca.app/Contents/Resources/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
exec /usr/bin/python3 -I "$(cd "$(dirname "$0")" && pwd)/wake.py" check "$@"
