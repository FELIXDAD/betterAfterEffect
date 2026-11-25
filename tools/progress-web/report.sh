#!/usr/bin/env bash
set -eu
if [[ $# -lt 2 || $# -gt 4 ]]; then
  echo '用法：report.sh <item-id> <doing|done|todo> [PR号] [备注]' >&2
  exit 2
fi
case "$2" in doing|done|todo) ;; *) exit 2 ;; esac
if [[ -n "${MSYSTEM:-}" ]] && command -v powershell.exe >/dev/null; then
  args=(-Item "$1" -Status "$2")
  if [[ -n "${3:-}" ]]; then args+=(-Pr "$3"); fi
  if [[ -n "${4:-}" ]]; then args+=(-Note "$4"); fi
  MSYS2_ARG_CONV_EXCL='*' exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w "$(dirname "$0")/report.ps1")" "${args[@]}"
fi
exec python3 - "$@" <<'PY'
import json, os, socket, subprocess, sys, urllib.request, urllib.error
from pathlib import Path
cfg = dict(line.split("=", 1) for line in (Path.home()/".config/bae-board/env").read_text().splitlines() if "=" in line)
branch = subprocess.run(["git","branch","--show-current"], capture_output=True, text=True).stdout.strip()
item, status, *extra = sys.argv[1:]
data = dict(agent=os.getenv("BAE_BOARD_AGENT") or cfg.get("BAE_BOARD_AGENT") or (socket.gethostname()+"/"+(branch or "worker")), item=item, status=status)
data.update(zip(("pr","note"),extra))
req = urllib.request.Request(cfg["BAE_BOARD_URL"].rstrip("/")+"/api/report", json.dumps(data).encode(),
    {"Authorization":"Bearer "+cfg["BAE_BOARD_TOKEN"], "Content-Type":"application/json"})
try:
    with urllib.request.urlopen(req, timeout=15) as response:
        print(response.read().decode())
except urllib.error.HTTPError as error:
    sys.exit("汇报失败 HTTP %s %s" % (error.code,error.read().decode()))
PY
