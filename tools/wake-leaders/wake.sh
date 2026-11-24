#!/bin/bash
# Wake every stalled, recovered leader session. See README.md.
export PATH="/Applications/Orca.app/Contents/Resources/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
exec /usr/bin/python3 -I "$(cd "$(dirname "$0")" && pwd)/wake.py" "$@" 
