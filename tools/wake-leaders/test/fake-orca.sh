#!/bin/bash
# Stand-in for the orca CLI used by drill.sh. Behaviour comes from env vars:
#   FAKE_ACCOUNT_JSON  file returned by `account list`
#   FAKE_PROJECTS      dir with the fake transcripts
#   FAKE_MODE          alive (default) | dead | limited
#   FAKE_CALLS         file that records every call
echo "$*" >> "${FAKE_CALLS:?}"
case "$1 $2" in
  "account list") cat "$FAKE_ACCOUNT_JSON" ;;
  "agent show") echo '{"ok":true,"result":{"state":"done"}}' ;;
  "orchestration send")
    [ "${FAKE_MODE:-alive}" = dead ] && { echo "no live session" >&2; exit 1; }
    to=""; while [ $# -gt 0 ]; do [ "$1" = --to ] && to="$2"; shift; done
    sid="${to#orca_session_id:}"
    # find the fake transcript that belongs to this orca session id
    f=$(grep -l "\"orca_session\":\"$sid\"" "$FAKE_PROJECTS"/*.meta | head -1); f="${f%.meta}.jsonl"
    ts=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
    echo "{\"type\":\"user\",\"uuid\":\"u-$RANDOM\",\"timestamp\":\"$ts\",\"message\":{\"role\":\"user\",\"content\":\"[nudge]\"}}" >> "$f"
    if [ "${FAKE_MODE:-alive}" = limited ]; then
      echo "{\"type\":\"assistant\",\"uuid\":\"e-$RANDOM\",\"timestamp\":\"$ts\",\"isApiErrorMessage\":true,\"error\":\"rate_limit\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"You've hit your 5-hour limit · resets 4:10pm (America/Los_Angeles)\"}]}}" >> "$f"
    else
      echo "{\"type\":\"assistant\",\"uuid\":\"a-$RANDOM\",\"timestamp\":\"$ts\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"resuming\"}]}}" >> "$f"
    fi
    echo '{"ok":true,"result":{"delivered":true}}' ;;
  *) echo "{}" ;;
esac
